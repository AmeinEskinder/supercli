# Document events — Frappe-style `doc_events` for supercli

**Status:** design only. No implementation in this document.
**Implementation:** new crate `crates/supercli-events`, after the rename lands,
as a subagent track in parallel with the OpenMuse ideas.
**Goal:** users script supercli through document lifecycle hooks, like
Frappe's `hooks.py` `doc_events` — without a parallel event system.

---

## 1. What exists today (and what this reuses)

There is no doc-event framework in the tree. This design reuses three
existing primitives instead of inventing a fourth:

| Primitive | Location | Role in this design |
|---|---|---|
| Write-ahead review + fsync + hash-chained JSONL audit | `crates/supercli-core/src/action_reviews.rs` (`record_review`, `Actor`, `ChainError::NonCanonical`) | The durability anchor: every hook run is one more hash-chained entry in this log, with a new `Actor::Hook { name }` variant |
| Outcome classifier | `AttemptOutcome::{Executed{success}, Ambiguous{reason}, NeverRan{reason}}` in `action_reviews.rs` | Maps to ToolCall events: `on_execute` / `on_fail` / `on_outcome_unknown` |
| `SessionEvent` | `crates/supercli-serve/src/session_events.rs` | The Host→UI wire. Doc events extend it with one additive variant; existing match arms are untouched except for the new arm |
| `state_bus::announce` | `crates/supercli-core/src/state_bus.rs` | Cross-frontend "shared state changed" ping. `after_*`/`on_*` doc events that mutate shared state reuse `announce(Change::…)` so frontends refresh — no second bus |

Two similarly-named things are **not** involved, to avoid confusion:

- `HookState` (`supercli-serve/src/activity.rs`) is the per-session activity
  indicator (`Idle`/`Busy`/`Attention`) for the UI. It is not a hook-scripting
  system and this design does not touch it.
- `state_bus` is a fire-and-forget ping bus ("re-read shared state"), not a
  pub/sub channel. Doc hooks are a **synchronous interception layer** on the
  document lifecycle, not broadcast messages. The only reuse is the
  post-change `announce` so UIs refresh.

---

## 2. Entities ("doctypes")

| Entity | What it is | Backing store (today) |
|---|---|---|
| `Session` | A conversation/session | session store (`sessions.rs`, session JSONL) |
| `Turn` | One agent turn (prompt → response) | turn records in the session |
| `ToolCall` | One proposed tool action | write-ahead review entry (`action-reviews.jsonl`) |
| `Approval` | An allow/ask/deny decision on a ToolCall | `ApprovalHub` (`supercli-serve/src/approvals.rs`) |
| `Grant` | A persisted permission grant | `grant_writer.rs` / `grants.json` |
| `Schedule` / `Job` | A scheduled task definition / one firing | `scheduled.rs` |
| `Connector` | A connector registration | `supercli-connector` registry |
| `Device` | An emulator/device handle | `supercli-device` |
| `FileWrite` | A file delivery (artifact/install payload) | delivery path (exact emission point fixed at implementation) |
| `Idea` | (later) an idea-surface entry | TBD |

## 3. Event catalog

Frappe names are used wherever the meaning matches. All events carry the
full doc as JSON.

### 3.1 Lifecycle (all entities unless noted)

| Event | Fires | May |
|---|---|---|
| `before_insert` | Before a new doc is persisted | patch the proposed doc, or reject |
| `autoname` | During name assignment (Session, Schedule/Job only) | return a name via patch |
| `after_insert` | After the insert is durable | observe only |
| `before_validate` | Before schema validation | patch, or reject |
| `validate` | During validation (after `before_validate`) | reject with reason |
| `before_save` | Before an update is persisted | patch, or reject |
| `on_update` | After an update is durable | observe only |
| `on_change` | After an update, **only if a field actually changed** (diff non-empty) | observe only |
| `on_trash` | Before a delete | reject (veto the delete) |
| `after_delete` | After a delete is durable | observe only |
| `on_archive` / `on_restore` | Session only, around archive/restore | observe only (`on_archive` may also reject, like `on_trash`) |

### 3.2 Approval

| Event | Fires | May |
|---|---|---|
| `before_submit` | When an answer (approve/deny) is proposed, before it is applied | **reject the proposed answer** — the approval stays pending; the hook can never substitute its own answer |
| `on_submit` | After an approval is durably recorded | observe only |
| `on_cancel` | After a denial is durably recorded | observe only |
| `on_expire` | After an approval times out | observe only |

### 3.3 ToolCall

| Event | Fires | Maps to |
|---|---|---|
| `before_execute` | After the write-ahead review is fsynced, before any tool bytes are sent | patch the call (tighten only), or reject → recorded as `NeverRan { reason: "hook_rejected" }`, nothing is sent |
| `on_execute` | After `AttemptOutcome::Executed { success: true }` is durably recorded | observe only |
| `on_fail` | After `AttemptOutcome::Executed { success: false }` is durably recorded | observe only |
| `on_outcome_unknown` | After `AttemptOutcome::Ambiguous` or `NeverRan` is durably recorded (carries which) | observe only |

### 3.4 `scheduler_events`

Kept exactly as they are today: the `all` / `hourly` / `daily` / `cron`
firing triggers are the schedule **engine**, orthogonal to doc events. The
`Schedule`/`Job` doctypes get the lifecycle events above for their
*definitions*; firing behavior does not change.

---

## 4. Semantics (safety-critical)

### 4.1 `before_*` and `validate` — synchronous, ordered, time-boxed

- Run **synchronously** in the mutating call path, in deterministic order:
  ascending `priority` (default 100); ties broken by global
  (`~/.supercli/hooks.toml`) before project (`.supercli/hooks.toml`),
  then declaration order.
- **Time-boxed**: default 2000 ms per handler, configurable per handler via
  `timeout_ms`. The dispatcher enforces the budget; there is no cooperative
  cancellation contract for handlers.
- A handler returns `{decision, patch, message}` (see §6). It may patch the
  proposed doc or reject it with a reason.
- **Fail closed, uniformly**: a handler crash, timeout, invalid output, or
  config error is treated as `reject` with reason
  `hook_crash` / `hook_timeout` / `hook_bad_output`. This applies to every
  entity — hooks are load-bearing once registered, which is why
  `supercli hooks test` (§7) exists as a dry-run validator.
- On `ToolCall`/`Approval` this is the critical path: a failed hook means
  the tool is not executed / the answer is not applied, and the rejection is
  audit-logged.

### 4.2 Tighten-only

Decisions form a strictness lattice: `allow < escalate < reject`.
(`escalate` is meaningful for policy-gated entities: Allow → Ask.)

- The effective decision is the **maximum** strictness across all handlers
  and the caller's proposed decision. Combining is monotonic.
- A hook can: **reject**, **escalate** Allow → Ask, or **add context**
  (patch additive fields).
- A hook can **never**: turn Ask or Deny into Allow, lower strictness in any
  way, or answer an approval. `before_submit` on an Approval may only block
  the proposed answer (approval stays pending); it cannot supply one.
- Patches are field-scoped per entity (fixed at implementation; e.g. a
  ToolCall patch may add context fields but may not change the tool name or
  widen its arguments). Out-of-scope patch keys are dropped and logged.

### 4.3 `after_*` and `on_*` — observers

- Fire **only after** the write-ahead record and the audit fsync for the
  underlying mutation. They observe a frozen copy of the doc.
- **Cannot change the outcome.** A patch returned by an observer is ignored;
  the dispatcher logs a contract violation in `hooks trace`.
- **At-least-once delivery** with idempotency key = event id
  (`<entity>:<doc_id>:<event>:<seq>`). Deliveries are retried with backoff;
  after the retry budget they go to the dead-letter file. Handlers must be
  idempotent on the event id (shell handlers receive it in the JSON;
  webhook handlers receive it as an `X-Supercli-Event-Id` header).
- Pending observer deliveries live in `~/.supercli/hook-outbox.jsonl`
  (durable). On Host boot the dispatcher replays unacked entries — this is
  what makes at-least-on-ce crossed a Host restart work, and the idempotency
  key is what makes the replay safe (dedup).

### 4.4 Audit

- **Every hook run** — sync or observer, allow or reject — appends exactly
  one entry to the same hash-chained JSONL audit log as the action reviews
  (`action-reviews.jsonl`), reusing its append + fsync + canonical-form
  machinery.
- The entry carries actor `hook:<name>` — this requires a new
  `Actor::Hook { name }` variant in `action_reviews.rs` with
  `Display` = `hook:<name>` and an exact-inverse `parse`, because the
  chain verifier re-serializes parsed actors when re-hashing
  (the current fallback maps unknown strings to `human:unidentified`,
  which would corrupt the chain).
- The entry hashes the **decision and the patch** (patch canonicalized as
  JSON), plus timing (`started_ms`, `elapsed_ms`), priority, and the
  outcome (`ok` / `timeout` / `crash` / `rejected`).

### 4.5 Recursion guard

- The dispatcher carries a depth counter in the hook context, starting at 0.
- A handler that itself mutates a doc (e.g. a shell handler invoking the
  `supercli` CLI) re-enters the dispatcher at depth + 1.
- **Max depth 3** (Frappe's `flags` equivalent). A mutation at depth ≥ 3
  proceeds with defaults and **no hooks fire**; the skip is audit-logged
  with reason `recursion_guard`.
- This bounds both accidental cycles (a `Session.on_update` handler that
  updates the session) and adversarial ones.

---

## 5. Registration — `hooks.toml`

Declarative. Global file at `~/.supercli/hooks.toml`, per-project at
`.supercli/hooks.toml` (project = cwd at invocation). Both are loaded and
merged; global handlers run before project handlers at equal priority.

```toml
# Map entity -> event -> list of handlers.
[doc_events.Session]
on_archive = [
  { name = "notify", command = ["/usr/local/bin/on-archive.sh"],
    priority = 100, timeout_ms = 2000 },
]
on_change = [
  { name = "audit-trail", webhook = "http://127.0.0.1:8787/hook",
    priority = 50 },
]

[doc_events.ToolCall]
before_execute = [
  { name = "policy", command = ["policy-check"], priority = 10,
    timeout_ms = 2000 },
]

[doc_events.Approval]
before_submit = [
  { name = "four-eyes", webhook = "http://127.0.0.1:8787/four-eyes",
    priority = 10 },
]
```

- Handler fields: `name` (required; becomes the audit actor `hook:<name>`),
  `priority` (default 100), `timeout_ms` (default 2000), and exactly one of
  `command` (argv array) or `webhook` (URL).
- `command` entries must be absolute paths or found on `PATH` at load time;
  missing → handler disabled with a config error (fail closed at
  registration, not at 2 a.m.).
- `webhook` URLs must resolve to **localhost only** (`127.0.0.1`, `::1`,
  `localhost`). Anything else is a config error and the handler is disabled.
  No remote webhooks in Phase 1 — this is a deliberate exfiltration guard.

---

## 6. Handler contract

Identical JSON contract for every handler kind, so Phase 1 shell handlers
port unchanged to Phase 2 scripts.

**Input** (stdin for `command`; POST body for `webhook`):

```json
{
  "event_id": "ToolCall:tc-9f2:on_execute:41",
  "entity": "ToolCall",
  "event": "before_execute",
  "doc": { "id": "tc-9f2", "tool": "write_file", "args": { "path": "src/main.rs" } },
  "context": { "actor": "human:dev-1", "depth": 0, "dry_run": false }
}
```

**Output** (stdout for `command`; response body for `webhook`):

```json
{ "decision": "allow", "patch": { "note": "reviewed by policy" }, "message": "" }
```

- `decision`: `"allow"` | `"escalate"` | `"reject"`. Unknown values are
  treated as `reject` with reason `hook_bad_output`.
- `patch`: object merged into the proposed doc (sync events only;
  field-scoped per §4.2; ignored for observers).
- `message`: human-readable reason, surfaced on rejection and in `trace`.

### Phase 1 handlers

1. **Shell command.** JSON doc on stdin; JSON `{decision, patch, message}`
   on stdout; exit code ignored (the JSON decides). Stderr is captured into
   the trace log. Time-boxed per `timeout_ms`; on expiry the child is killed
   and the run is recorded as `timeout` → reject.
2. **Webhook to localhost only.** POST with `Content-Type: application/json`
   and `X-Supercli-Event-Id`. Same JSON contract. Same timeout semantics.

### Phase 2 handlers (embedded scripting, like Frappe Server Scripts)

- **Recommendation: Rhai** — pure Rust (no C toolchain), tiny binary
  footprint, sandboxed by default. WASM (wasmtime) is the alternative;
  either sits behind a `events-script` cargo feature flag so the default
  build pays nothing.
- The script receives the same `doc` map and returns a map with
  `decision`/`patch`/`message` — the JSON contract is identical, so a
  Phase 1 shell handler's logic ports by transliteration.
- Scripts run in-process with no filesystem/network access by default;
  capabilities are allowlisted per handler in `hooks.toml` if ever needed.

---

## 7. CLI

- `supercli hooks list [--json]` — merged global + project handlers in
  effective execution order (priority, then file precedence), with
  `timeout_ms` and enabled/disabled state. Disabled handlers show their
  config error.
- `supercli hooks test <Entity> <event> --doc sample.json [--handler name]` —
  dry run. Loads the doc from file, runs the matching `before_*`/`validate`
  handlers synchronously against a fake context (`dry_run: true`, depth 0),
  prints the effective decision, the merged patch, per-handler timings, and
  the audit entry that *would* be written. **Never touches real docs,
  never executes tools, never answers approvals.**
- `supercli hooks trace [--limit 50] [--json]` — recent hook runs read from
  the audit log: event id, entity/event, handler, decision, elapsed ms,
  outcome, and any contract violations (e.g. observer patch ignored).

---

## 8. Emission points (where hooks fire in existing code)

| Entity.event | Emission point |
|---|---|
| `Session.*` | session lifecycle ops (`sessions.rs`: create/archive/restore/delete) |
| `Turn.before_insert` / `after_insert` | turn start, alongside `SessionEvent::TurnStarted` |
| `ToolCall.before_execute` | **after** `record_review` fsyncs, before any tool bytes are sent |
| `ToolCall.on_execute` / `on_fail` / `on_outcome_unknown` | after the `OutcomeEntry` is durably recorded |
| `Approval.before_submit` | `ApprovalHub::answer`, before the answer is applied |
| `Approval.on_submit` / `on_cancel` / `on_expire` | after the approval outcome entry is fsynced |
| `Grant.*` | grant persist path (`grant_writer.rs`) |
| `Schedule`/`Job.*` | schedule save path (`scheduled.rs`) |
| `Connector.*` | connector register/unregister |
| `Device.*` | device ops; composes with the approval flow (`install`/`erase` are already approval-gated — both fire) |
| `FileWrite.*` | delivery path (exact point fixed at implementation) |

Doc events that mutate shared state additionally call the existing
`state_bus::announce(Change::…)` after the observer phase, so frontends
refresh through the channel they already listen on.

**Wire:** one additive `SessionEvent` variant —
`DocEvent { session_id, seq, at_ms, entity, event, doc_id, doc: Value }` —
carries doc events to UIs. Existing variants and match arms are untouched
apart from the new arm.

---

## 9. Observer delivery design (at-least-once)

1. When an `after_*`/`on_*` event fires, the dispatcher appends the event to
   `~/.supercli/hook-outbox.jsonl` (fsync) **before** attempting delivery.
2. Delivery attempts run on a background worker in the Host: for each
   registered observer handler, invoke with the event id; on success, mark
   acked; on failure, schedule retry with backoff (1s, 5s, 30s, 5min).
3. After the retry budget (default 24h, configurable), the event moves to
   `~/.supercli/hook-dead-letter.jsonl` with the failure history, and an
   operator-visible warning is emitted.
4. On Host boot, unacked outbox entries are replayed. Handlers dedup on the
   event id — a replayed delivery is a no-op for a correct handler, which
   is what makes "at-least-once across a Host restart" safe.
5. One-shot CLI processes flush the outbox worker before exit (same pattern
   as `state_bus::flush`).

---

## 10. Tests to specify

Implemented in `crates/supercli-events` (unit) plus Host integration tests:

1. **Ordering** — three handlers with priorities 10/50/100 record
   invocation order in a test double; assert 10 → 50 → 100.
2. **Patch applied** — `before_insert` handler returns a patch; assert the
   persisted doc contains the patched fields.
3. **Reject blocks execution** — `before_execute` returns `reject`; assert no
   tool bytes were sent and the outcome entry is
   `NeverRan { reason: "hook_rejected" }`.
4. **Timeout fails closed** — handler sleeps past `timeout_ms`; assert the
   decision is `reject` with reason `hook_timeout` and the tool did not run.
5. **Observers can't mutate** — `on_execute` handler returns a patch;
   assert the recorded outcome is unchanged and a contract violation is
   logged in trace.
6. **Exactly one audit entry per run** — one hook run, then count audit-log
   entries with that event id; assert exactly 1, with actor `hook:<name>`
   and the decision+patch hash present.
7. **Recursion cap** — handler that writes a doc on every `on_update`,
   5 levels deep; assert hooks fired for depths 0–2 only, depth ≥ 3
   proceeded without hooks, and each skip is audit-logged
   (`recursion_guard`).
8. **At-least-once with dedup across Host restart** — observer handler with
   an idempotent counter; kill the Host mid-delivery; restart; assert the
   effect was applied exactly once (handler invoked ≥ 1, deduped by event
   id) and the outbox is empty.

---

## 11. Implementation plan (`crates/supercli-events`)

After the rename lands; subagent track, parallel with the OpenMuse ideas.

```
crates/supercli-events/
  Cargo.toml            # no mandatory deps; `events-script` feature for Rhai (Phase 2)
  src/lib.rs            # DocEvent, Entity, EventName, Decision types
  src/registry.rs       # hooks.toml load + merge (global + project), validation
  src/dispatcher.rs     # sync before_*/validate path + observer path, depth guard
  src/handlers.rs       # command executor + localhost webhook client (Phase 1)
  src/outbox.rs         # hook-outbox.jsonl, retries, dead-letter, boot replay
  src/audit.rs          # Actor::Hook integration with action_reviews.rs
  src/script.rs         # #[cfg(feature = "events-script")] Rhai host (Phase 2)
  src/cli.rs            # `hooks list` / `test` / `trace` wiring
```

`Actor::Hook { name }` is added to `supercli-core/src/action_reviews.rs`
with exact-inverse `Display`/`parse` (`hook:<name>`), keeping the
hash-chain canonical-form property (`ChainError::NonCanonical`).

## 12. Non-goals

- No new event bus, no pub/sub broker, no remote webhooks (localhost only).
- No Phase 2 scripting in Phase 1; Rhai/WASM behind a feature flag later.
- `Idea` entity ships later; the catalog reserves its events.
- `scheduler_events` firing semantics are untouched.
- `HookState` (activity indicator) is untouched.

## 13. Open questions (for implementation, not this doc)

1. Exact per-entity patchable-field allowlists (§4.2) — enumerated during
   implementation against each emission point.
2. Whether `on_change` diffing is field-level or whole-doc hash comparison
   (recommendation: field-level, so trace can name the changed fields).
3. `FileWrite` emission point — the delivery path needs a precise hook
   site once the artifact/install flows are mapped.
4. Outbox retry budget defaults (24h proposed) and whether dead-letter
   should page the operator or just warn.
