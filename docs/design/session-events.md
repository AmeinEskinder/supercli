# Session Events — typed Host → client event stream

Phase 6 R4. Status: design proposal + first implementation slice (turn +
approval events). The full event vocabulary below is the target; only the
first slice is implemented.

## Problem

Clients today derive session state by polling and by parsing terminal
output: is a turn running, was a tool call requested, was it approved,
did the lease fence change. Parsing output is fragile and races the
check-then-act window (P5-3). The Host already knows all of this
structurally — hook ingestion (`hook_listener.rs`), the activity engine
(`activity.rs`), the approval queue (`approvals.rs`), and the Phase 5
durable records (`unpeel-core/src/action_reviews.rs`). The event stream
exposes that knowledge as typed facts.

## Design principles

- **Host-first, additive.** The Host defines the event vocabulary and the
  wire shape. Clients never invent events. New event kinds are additive;
  unknown kinds are ignored by older clients.
- **Capability-advertised.** The stream is gated behind the
  `session.events.v1` capability; the protocol minor version bumps when the
  capability ships. Clients probe the capability before polling `/events`.
- **Monotonic per-session sequence.** Every event carries `session_id` and
  a `seq` that is strictly increasing within that session. Clients keep a
  per-session cursor (`after_seq`) and resume without loss or duplication.
- **Mapped onto Phase 5 records, not output.** Approval and tool events
  carry the same identifiers as the durable action-review records
  (`review_id`, hash chain), so a client can correlate an event with the
  tamper-evident record instead of scraping the transcript.
- **Bounded.** The Host keeps a ring buffer per session (default 512
  events). Cursors older than the buffer get the oldest available event
  plus a `resync: true` hint so the client re-bootstraps.

## Event vocabulary

### Turn lifecycle

```json
{ "kind": "turn.started", "session_id": "s1", "seq": 41, "at_ms": 1710000000000,
  "turn_id": "t-9f3", "trigger": "user" }
{ "kind": "turn.finished", "session_id": "s1", "seq": 42, "at_ms": 1710000005123,
  "turn_id": "t-9f3", "outcome": "completed" }
```

`trigger` is one of `user`, `scheduled:<id>`, `hook:<name>`,
`policy:allow`. `outcome` is one of `completed`, `stopped`,
`awaiting_approval`, `failed:<code>`. The `turn_id` correlates start and
finish; `actor` semantics follow the Phase 5 requirement that every path
records an explicit actor.

### Tool calls

```json
{ "kind": "tool.requested", "session_id": "s1", "seq": 43,
  "review_id": "r-77", "tool": "bash.exec", "summary": "cargo test" }
{ "kind": "tool.approved", "session_id": "s1", "seq": 44,
  "review_id": "r-77", "answered_by": "device:phone-1" }
{ "kind": "tool.denied", "session_id": "s1", "seq": 44,
  "review_id": "r-77", "answered_by": "device:phone-1" }
{ "kind": "tool.executed", "session_id": "s1", "seq": 45,
  "review_id": "r-77", "exit": "ok" }
{ "kind": "tool.ambiguous", "session_id": "s1", "seq": 46,
  "review_id": "r-77", "reason": "uncertain-write" }
{ "kind": "tool.never_ran", "session_id": "s1", "seq": 47,
  "review_id": "r-78", "reason": "stale lease before tool call" }
```

`review_id` is the Phase 5 action-review record id; the client can fetch
the tamper-evident record for the full detail. `tool.ambiguous` fires when
the Host cannot determine whether an external write took effect — the
no-retry rule (P5-1) applies and the event says so explicitly.
`tool.never_ran` fires when the attempt provably never ran (stale-lease
fence refused before any tool-call bytes were sent, or a definite
transport failure that provably never reached the far side). It carries
no `needs_review`: there is nothing uncertain, and a later worker may
safely re-fire the schedule.

### Review escalation

```json
{ "kind": "needs_review", "session_id": "s1", "seq": 47,
  "review_id": "r-78", "reason": "takeover-escalation" }
```

Fired when the four-gate fence escalates to `NeedsReview` (P5-3), or any
other path that needs a human decision.

### Lease / fence changes

```json
{ "kind": "lease.changed", "session_id": "s1", "seq": 48,
  "generation": 7, "holder": "device:phone-1" }
```

Carries the monotonic lease generation (P5-3). Clients use the generation
— not wall-clock time — to decide whether they hold the fence.

## Wire

`GET /mobile/events?session_id=<id>&after_seq=<n>&limit=<n>` (existing
auth). Response:

```json
{ "events": [ ... ], "next_seq": 49, "resync": false }
```

`limit` defaults to 128, clamped to 1024. `after_seq` defaults to 0
(everything buffered). `next_seq` is the cursor for the next poll.

The client SDK exposes:

- `HostClient::events(session_id, after_seq, limit)` — one poll.
- `ClientEventCursor` — per-session cursor map with `next(session_id)`,
  `advance(session_id, next_seq)`, `reset(session_id)`.

The Dioxus composer polls (2s cadence while a session is focused) and maps:

- `turn.started` → Send becomes Stop (running).
- `turn.finished` → Stop becomes Send; drain one queued follow-up.
- `tool.requested` / `needs_review` → surface the approval card.

This replaces the provisional R3 wiring that inferred running state from
`ActivityState` polling and sent raw Ctrl-C: with typed events the
composer knows exactly when a turn starts and ends.

## Phase 5 mapping

| Event | Phase 5 record |
|---|---|
| `tool.requested` | action-review record written write-ahead before execution |
| `tool.approved` / `tool.denied` | approval answer appended to the review |
| `tool.executed` | execution outcome appended; hash chain extended |
| `tool.ambiguous` | uncertain-write marker; never auto-retried |
| `tool.never_ran` | provable pre-send refusal; safe to re-fire, no escalation |
| `needs_review` | fence escalation to `NeedsReview` |
| `lease.changed` | monotonic generation bump, DB-side time |

## First slice (implemented)

- `SessionEvent` enum + `EventBus` (per-session ring buffer, monotonic
  seq) in `unpeel-serve`.
- `GET /mobile/events` handler in `mobile.rs`, wired to the shared bus.
- Approval events (`tool.requested`, `tool.approved`, `tool.denied`)
  emitted from `ApprovalHub::request` / `ApprovalHub::answer`.
- Turn events (`turn.started`, `turn.finished`) emitted from the activity
  engine's turn transitions.
- `session.events.v1` capability + `PROTOCOL_MINOR` 21 → 22, mirrored in
  the Swift `RemoteControlProtocol` and `protocol/host-capabilities-v1.json`.
- Client DTOs in `unpeel-client/src/events.rs`, `HostClient::events()`,
  `ClientEventCursor`, `has_session_events()`.
- Dioxus composer consumes turn events to drive Send/Stop.
- Host tests 4/4, client tests 6/6.

## Later slices (not implemented)

- `tool.executed` / `tool.ambiguous` emission from the tool runner.
- `lease.changed` emission from the fence manager.
- Push delivery (currently poll; the 2s cadence is the interim).
- Swift client consumption.
