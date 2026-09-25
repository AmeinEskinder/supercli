# Track B HTTP surface security review (Phase 8 S1)

Date: 2026-09-23. Reviewer: Sophia (Muse). Scope: every HTTP surface Track B
added or changed.

## Surfaces in scope

| Surface | Listener | Auth |
|---|---|---|
| `GET /mobile/events` | mobile (LAN TLS / relay) | Bearer (paired device / owner transport); 401 pre-route |
| `POST /mobile/turn-cancel` | mobile | Bearer; 401 pre-route |
| `POST /mobile/session-organization` | mobile | Bearer; 401 pre-route |
| `POST /mobile/approvals/answer` | mobile (+ relay live route) | Bearer on mobile; relay inherits tunnel auth |
| `POST /mcp/approve-connector` (+ `/mcp/approve-write`, `/mcp/approve-browser`, `/mcp/approve-computer`, `/mcp/approve-app-open`) | hook listener, **127.0.0.1 only** | `x-unpeel-auth` shared token |
| Scheduler / daemon | — | No HTTP listener. Schedules are local files driven by `unpeel schedule` CLI + in-process tick (`unpeel-core::scheduled`). No remote input reaches the scheduler except through the already-reviewed mobile routes. |

## Findings

### F1 — FIXED: connector approval grants were keyed by tool alone, not (connector, tool)

**Severity: high.** `already_granted("connector", session, tool)` /
`persist_grant("connector", session, tool)` ignored the connector name.
Tool names are bare MCP names and two connectors can expose the same one
(`SessionConnectors::find_tool` resolves the first connector, sorted, that
advertises the name). Approving `search` for connector `github` therefore
auto-approved `search` for connector `evil` with no further prompt — the
persisted grant was broader than the approval the user gave.

**Fix** (`crates/unpeel-serve/src/approvals.rs`, `hook_listener.rs`):
new `persist_connector_grant(caller, connector, tool)` /
`connector_grant_exists(caller, connector, tool)` storing
`{"connector": c, "tool": t}` objects under `mcp_connector_approvals`.
The generic single-target grant arms can no longer express a connector
grant and fail closed. Legacy bare-string entries never match (fail
closed → the user is re-prompted once, then the namespaced grant is
persisted).

**Negative tests:** `connector_grant_does_not_leak_across_connectors`,
`connector_grant_ignores_legacy_bare_string_entries`,
`approve_connector_honors_namespaced_grant`.

### F2 — FIXED (hardened): `/mcp/*` body fields accepted path metacharacters

**Severity: medium.** `handle_mcp` extracted `session_id` /
`caller_session_id` / `target_session_id` with trim + non-empty only.
A crafted id reached `session_display_name` → `load_manifest` (a
read probe: `<sessions>/<id>/manifest.json`) and was stored verbatim as
a grant key. The hook listener is loopback + token-gated, so exploitability
was limited to local token holders, but the input class is the same one
the mobile routes already reject.

**Fix** (`hook_listener.rs`): the shared `field()` extractor now rejects
`/`, `\`, and `..` in every field; a rejected id yields 400 before any
lookup or grant write.

**Negative tests:** `approve_connector_rejects_traversal_session_id`
(400 over a real loopback TCP pair with a real `x-unpeel-auth` token),
`approve_connector_rejects_missing_auth` (401).

### Verified, no change: session-id path traversal on the mobile routes

Every session id that reaches a filesystem path is validated:

- POST bodies funnel through `mobile::body_session_id`, which enforces
  `safe_session_id` (rejects empty, `/`, `\`, `..`) — this covers
  `/mobile/turn-cancel`, `/mobile/session-organization`, and
  `/mobile/resize-desktop`.
- GET query ids are filtered with `safe_session_id` at each handler
  (`/mobile/events`, `/mobile/output`) and at the presence-touch call site.
- Core-routed verbs (`route_with_effects`) validate via
  `controller_api::{body_session_id, query_session_id}` →
  `valid_session_id`; `controller_host` enforces `safe_session_id` plus a
  length cap.
- Hook-router path ids (`/hook/{id}`, `/notify/{id}`, …) reject `/` and
  `..` at the router.

**Negative tests (new, pinning the funnels):**
`session_id_funnel_rejects_path_traversal`,
`turn_cancel_rejects_traversal_session_id`,
`events_rejects_traversal_session_id`, `output_rejects_traversal_session_id`.

### Verified, no change: authentication

- Every `/mobile/*` route except `/mobile/pair` requires a bearer token
  (`principal_for_bearer`: SHA-256 hash compared against `devices.json`);
  failures are 401 before routing. `/mobile/pair` is the sealed
  application-layer pairing exchange by design.
- Plaintext bearers are refused with 426 and the credential redacted
  (`Bearer <redacted>`); the token is never reflected.
- `/mcp/*` requires the `x-unpeel-auth` token; the hook listener binds
  127.0.0.1 only.

### Verified, no change: request size / connection limits

- 4 MiB body cap on both listeners (413/400 past it), 64 connections
  total / 16 per peer on the mobile listener, pre-auth deadline for slow
  unauthenticated connections (tested:
  `slow_unauthenticated_connections_are_capped_and_cut_at_the_deadline`).

### Verified, no change: replay / duplicate handling

- Approval answers are UUIDv4 ids, single-use: answering twice (or after
  timeout) is 409 `approval no longer pending`. Guessing is infeasible.
- `POST /mobile/turn-cancel` is idempotent: a second call finds no
  in-flight attempts and returns `cancelled: true, idle: true` without
  re-marking anything.
- Remembered connector grants are by design (see F1 for the corrected
  keying); write/browser/computer/app-open grants are unchanged.
- Session creation has no request-id dedup; retries can create a second
  session. Documented effect-unknown semantics (controllers must refresh,
  not blindly retry) — not a vulnerability, no change.

### Verified, no change: error leakage

- 500 bodies on the reviewed routes carry the review/attempt id plus the
  underlying IO error text (paths under the private UNPEEL_HOME, never
  tokens or credentials). Bearer tokens are hashed before comparison and
  never echoed. The 426 plaintext gate redacts the credential.

### Trust-model note (accepted, no code change): single-owner authorization

Any authenticated paired device can read events, cancel turns, answer
approvals, and reorganize **any** session on the Host. `owner_principal_id`
is attribution (who created / who answered), not an access boundary —
nothing in the tree enforces it, and the product has no multi-tenant
model: all paired devices are the owner's devices. The residual risk is a
stolen/unlocked paired device, which is inherent to the bearer-token
pairing design; pairing itself is a physical-proximity one-shot sealed
exchange. If scoped (Link/Room) principals are ever added, the exhaustive
match in `principal_can_create_session` forces an explicit authorization
decision at compile time instead of silently inheriting owner powers.

### Accepted risk (noted, no change): `mcp_auth::verify_auth` timing

The token comparison is not constant-time. The token is 256-bit,
loopback-only, and rotated per home; a timing oracle over the loopback
TCP stack is not a practical attack. Documented in code; left as is.

## Test summary (S1)

New: 3 approvals tests, 4 mobile tests, 3 hook_listener tests — all pass.
Full `unpeel-serve` lib suite: 172 passed, 3 failed — the 3 are the known
environmental real-UDP punch timeouts in `direct_path.rs` (untouched by
this change; same class as the Phase 7 closeout). `cargo clippy
--all-targets -- -D warnings` clean; `cargo fmt --check` clean.
