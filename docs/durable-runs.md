# Durable Runs — design doc

**Status:** design only, no implementation. Amein's priority OpenMuse-track
item: a subagent or task run that crashes is picked up after restart and
continues where it stopped, instead of vanishing.

## 1. Problem

Today `crates/supercli-core/src/schedule_leases.rs` provides leases +
fencing for *scheduled triggers* only. An agent run (a turn loop, a
subagent, a long task plan) that crashes — kill -9, OOM, host reboot —
leaves nothing resumable behind:

- A takeover re-fires from scratch (duplicate side effects), or
- The run goes to `NeedsReview` with no record of which steps already
  completed.

Durable Runs give every run a crash-safe journal so a new worker can
**resume at the first incomplete step** with zero duplicated side effects.

## 2. Storage: SQLite (WAL) — `runs.db`

One database per home: `<SUPERCLI_HOME>/runs.db`, opened in WAL mode
(`journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=5000`).
Three tables; all writes go through one `RunsDb` handle.

### 2.1 `runs`

| column | type | notes |
|---|---|---|
| `id` | TEXT PK | `run_<ulid>` |
| `parent_run` | TEXT NULL | set for subagent child runs; NULL for roots |
| `state` | TEXT | one of §3, default `QUEUED` |
| `plan` | TEXT | JSON: ordered step descriptors + budgets (see §6) |
| `budgets` | TEXT | JSON: `{max_steps, max_tool_calls, max_ms, max_cost_cents}` |
| `lease_owner` | TEXT | worker id holding the run lease (see §4) |
| `lease_generation` | INTEGER | fencing token, monotonic per run |
| `lease_expires_at` | INTEGER | ms, database-clock (same `julianday` expr as schedule_leases) |
| `created_at_ms` | INTEGER | |
| `updated_at_ms` | INTEGER | bumped on every state/step write |

### 2.2 `run_steps` — append-only step journal

| column | type | notes |
|---|---|---|
| `run_id` | TEXT | FK → runs.id |
| `step_no` | INTEGER | 0-based, unique per run |
| `kind` | TEXT | `model` \| `tool` \| `subagent` |
| `input_hash` | TEXT | SHA-256 of the canonical step input (idempotency key) |
| `output` | TEXT NULL | JSON result once complete |
| `outcome` | TEXT NULL | NULL until decided: `executed_ok` \| `executed_failed` \| `ambiguous` \| `never_ran` (mirrors `AttemptOutcome`, §5) |
| `attempt` | INTEGER | retry counter for this step_no |
| `review_id` | TEXT NULL | links to the action-review entry for tool steps |
| `ts_ms` | INTEGER | |

**Append-only:** rows are INSERTed, never UPDATEd. A retry is a new row
with the same `step_no` and `attempt+1`. The *effective* state of a step
is its latest row. This preserves the full forensic trail and makes
"did this side effect already happen?" answerable without replaying.

### 2.3 `run_events` — UI replay log

| column | type | notes |
|---|---|---|
| `run_id` | TEXT | |
| `seq` | INTEGER | per-run sequence |
| `event` | TEXT | JSON: `{type, ts_ms, payload}` — mirrors the live `SessionEvent` stream |
| `ts_ms` | INTEGER | |

UIs replay `run_events` in order to reconstruct a crashed run's visible
history without re-running anything. This is the durable counterpart of
the in-memory event bus.

## 3. Run states

```
QUEUED → AWAITING_MODEL → EXECUTING_TOOLS → AWAITING_MODEL → …
                         ↘ AWAITING_APPROVAL ↗ (approval requested)
         PAUSED (any active state, via pause)
         DONE | FAILED | NEEDS_REVIEW (terminal-ish; NEEDS_REVIEW is
         resumable by human decision via retry)
```

- `QUEUED`: created, not yet picked up by a worker.
- `AWAITING_MODEL`: waiting for the next model turn.
- `EXECUTING_TOOLS`: tool calls in flight for the current step.
- `AWAITING_APPROVAL`: blocked on a human approval (not a crash — the
  approval survives restart via the existing approvals store).
- `PAUSED`: explicit human/operator pause; a worker must not claim it.
- `DONE` / `FAILED`: terminal. `FAILED` keeps the journal for audit.
- `NEEDS_REVIEW`: an external write had an uncertain outcome (§5).
  Resumable only by explicit human `retry` (never auto-replayed).

State transitions are single-row `UPDATE ... WHERE state = <expected>`
inside `BEGIN IMMEDIATE`; a lost race returns 0 rows and the worker
treats it as "someone else owns this run now".

## 4. Leases: reuse `schedule_leases`, do not duplicate

`schedule_leases.rs` already implements claim/renew/fence with
generation-monotonic fencing tokens and database-clock expiry. Durable
Runs reuses it by treating each run id as a *schedule id* in the same
`schedule_leases` table (tenant `"default"`):

- **Claim:** worker calls `ScheduleLeases::claim(run_id)` before touching
  a run. The returned `ClaimInfo { generation, ... }` becomes the run's
  `lease_owner` / `lease_generation` / `lease_expires_at`.
- **Renew:** the worker renews on a heartbeat (every `ttl/3`); a run
  whose lease lapsed is claimable by another worker (takeover).
- **Fence:** before every side-effecting step — each tool call, each
  journal append that records an outcome — the worker checks
  `LeaseFence::is_current()`. A stale fence aborts the step with a
  distinct error; the step is journaled as `never_ran` (provably no side
  effect, §5) and the worker drops the run.
- **Release:** clean completion (`DONE`/`FAILED`/`PAUSED`/`NEEDS_REVIEW`
  decided) releases the lease (tombstone kept, generation monotonic).

No new lease protocol. `RunsDb` holds a `ScheduleLeases` handle; the
runs table mirrors the lease columns for debuggability (`runs list`
shows owner/generation/expiry without joining).

## 5. Resume semantics

On worker start (and on explicit `resume`), for each non-terminal run
the worker does not own:

1. **Claim** the run lease (takeover if lapsed). Loser skips.
2. **Replay, never re-execute:** read `run_steps` ordered by
   `(step_no, attempt)`; the latest row per `step_no` with a non-NULL
   `outcome` is *complete*. Its `output` is fed back into the agent
   loop as if it had just happened — no tool is re-called, no model
   turn re-issued.
3. **Continue at the first incomplete step** (no row, or latest row has
   NULL outcome, or latest outcome is `never_ran`):
   - `never_ran` → **safe to execute**: the step provably never ran
     (stale fence before send, or definite transport failure before the
     far side). Just run it.
   - `ambiguous` → the step *may* have executed. **Never replay it.**
     Transition the run to `NEEDS_REVIEW` with the step's `review_id`
     and `input_hash`. A human inspects and either marks the step
     `executed_ok`/`executed_failed` (with evidence) or issues a
     compensating step; then `retry` continues.
   - `executed_ok` / `executed_failed` → these are complete; the loop
     would not have stopped here unless the crash landed exactly on the
     boundary. Replay the output and move on.
4. **Subagents:** child runs (same DB, `parent_run` set) are claimed and
   resumed the same way. The parent **re-attaches** to existing child
   runs instead of spawning new ones: on resume it lists children by
   `parent_run`, claims each, and continues their journals. A child in
   `DONE` is replayed; a child mid-step resumes per (3).

The uncertain-outcome rule reuses the existing
`AttemptOutcome::{Executed, Ambiguous, NeverRan}` classifier from
`action_reviews.rs` verbatim — step `outcome` values are that enum's
`kind_str()` strings (`executed`, `ambiguous`, `never_ran`) plus the
`success` bit for executed. No parallel classification logic.

## 6. Task plans: pause / resume / cancel / retry

The `runs.plan` JSON is an ordered list of step descriptors:

```json
{
  "steps": [
    {"kind": "model", "prompt_ref": "…"},
    {"kind": "tool", "tool": "write_file", "args_hash": "…"},
    {"kind": "subagent", "task": "…", "child_run_id": "run_…"}
  ]
}
```

Operations (all go through lease claim first):

- **pause:** `UPDATE runs SET state='PAUSED'` + release lease. A paused
  run is never auto-claimed.
- **resume:** claim lease, set state to the pre-pause state (stored in
  plan JSON as `paused_from`), continue per §5.
- **cancel:** claim, set `CANCELLED`-equivalent (`FAILED` with
  `cancelled=true` in plan), release lease. In-flight tool calls are
  fenced out on their next check.
- **retry:** from `FAILED` or `NEEDS_REVIEW` (after human marks the
  ambiguous step resolved): claim, append a new attempt row for the
  failed step, continue per §5.

Budgets (`max_steps`, `max_tool_calls`, `max_ms`, `max_cost_cents`) are
enforced on resume too — a resumed run cannot exceed its original
budget; exceeding sets `FAILED` with `budget_exceeded`.

## 7. CLI

```
supercli runs list [--state <s>] [--json]
supercli runs show <id> [--json]        # state, plan, step journal tail, lease info
supercli runs resume <id>               # claim + continue per §5
supercli runs pause <id>
supercli runs cancel <id>
supercli runs retry <id>                # from FAILED / NEEDS_REVIEW
```

`runs list` shows id, parent, state, step progress (`3/7`), lease
owner/expiry. `--json` for scripting.

## 8. doc_events integration (after `supercli-events` lands)

`Run` becomes a doctype in the events crate (like Session/Approval):

- `before_insert` on run creation — hooks may patch the plan or reject.
  Reject fails closed (run stays `QUEUED`, never starts).
- `on_update` / `on_change` on every state transition and step-outcome
  journal append — observers only, after the journal fsync (§events.md
  §3: observers fire post-write, at-least-once).
- `on_trash` / `after_delete` on run journal pruning (retention policy).

Integration point: `RunsDb` calls into `supercli_events::Dispatcher`
at the same points `Session` will — the events crate already exposes
`DocType`/`DocEvent`; adding `DocType::Run` and `DocEvent::{BeforeInsert,
OnUpdate, OnChange, OnTrash, AfterDelete}` is additive. The
tighten-only lattice applies: a hook can reject a run's creation or
escalate it to require approval, never auto-approve.

## 9. Safety invariants (non-negotiable)

1. **Never re-execute a completed side effect.** Completed = latest
   journal row has a non-NULL outcome. Replay output, don't re-call.
2. **Ambiguous is never replayed.** Uncertain external writes go to
   `NEEDS_REVIEW` and wait for a human. (Same rule as Phase 5:
   ambiguous attempts are never auto-retried.)
3. **Fence before every side effect.** A worker that lost its lease
   journals `never_ran` and stops; it must not emit tool calls.
4. **Lease claim is single-flight.** Two workers, one run: exactly one
   wins the `BEGIN IMMEDIATE` claim; the loser skips.
5. **Journal appends are fsync'd before the outcome is acted on.**
   A crash between "tool returned" and "journal fsync" leaves the step
   incomplete → on resume it is `ambiguous` → `NEEDS_REVIEW`, never
   silently assumed complete.

## 10. Tests to specify

1. **kill -9 at every step boundary, 50 chaos iterations:** a scripted
   run with N deterministic steps (model → tool → tool → subagent →
   tool); the harness SIGKILLs the worker at a random step boundary
   (between journal fsync and next step start) on each iteration, then
   restarts and resumes. Assert: the run reaches `DONE`; the audit log
   shows **zero duplicated side effects** (each tool's `input_hash`
   appears with exactly one `executed` outcome).
2. **Resume lands on the right step:** after kill at boundary k, the
   first re-executed action is step k+1 (verified via run_events seq).
3. **Parent + 3 subagents killed mid-run:** all four journals resume;
   every child completes **exactly once**; parent re-attaches (no new
   child run ids — assert `parent_run` children count stays 3).
4. **Uncertain write → NEEDS_REVIEW:** inject an ambiguous outcome on a
   tool step (transport dropped post-send), kill, resume. Assert: run
   is `NEEDS_REVIEW`, the step was NOT replayed, and `retry` after
   human resolution continues from the next step.
5. **Lease fencing under takeover:** worker A stalls past lease expiry
   mid-step; worker B claims (generation+1) and completes the step;
   A wakes and its fence check fails → A journals nothing, exits.
   Assert single `executed` outcome in the journal.
6. **Budget enforcement across resume:** run with `max_tool_calls=2`
   killed after 2 calls; resume attempts a 3rd → `FAILED`
   `budget_exceeded`, no 3rd call journaled.

## 11. Implementation sketch (for the build track)

- New module `crates/supercli-core/src/durable_runs.rs` (~1200 lines):
  `RunsDb` (rusqlite, WAL), `RunState`, `StepKind`, journal append,
  claim/resume loop, child re-attach, budget checks.
- Reuse: `schedule_leases::{ScheduleLeases, LeaseFence}`,
  `action_reviews::{AttemptOutcome, record_review}`,
  `browser_engine::sha256_hex` for `input_hash`.
- CLI: `crates/supercli-cli/src/runs_cli.rs` wired into `cli.rs`.
- Feature-gate: none needed (rusqlite already a core dep via
  schedule_leases); keep the module behind no flag so resume works in
  every build.
- Migration: fresh `runs.db`; no migration of old state (nothing
  durable exists to migrate — that's the point).
