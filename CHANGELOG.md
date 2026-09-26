# Changelog — supercli

All notable changes to supercli are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0] — 2026-09-26

First supercli release: the renamed, device-first agent harness.
(Previously developed as Unpeel; see `docs/rename-allowlist.md` for the
rename record.)

### Devices

- Native Android device backend over the scrcpy v2 protocol: correct
  v2 forward-tunnel handshake (video + control sockets accepted before
  any read), 64-byte device-name/codec/resolution header parse, H.264
  packet streaming.
- Device-point coordinate system end to end (0x01–0x04 wire framing):
  tap, swipe, pinch, and HOME all address device points, never
  normalized floats.
- `supercli device setup android` / `supercli device setup ios`:
  guided setup with honest gate messages (host OS, required tools,
  server reachability).
- iOS baguette passthrough: WebSocket video stream (`0x01` geometry
  frame, byte-identical relay) and NDJSON input pipe
  (tap/swipe/touch/button/key/text), same Devices panel and `/farm`
  path as Android.
- `/farm` HTTP endpoint and web Devices panel: live device grid with
  per-tile streams, tap/type/keys, device-point input.
- Device agent tools: `describe-ui` (uiautomator XML and baguette a11y
  JSON → structured UI trees), live `logcat`/`os_log` streaming over
  WebSocket, and MCP exposure (`device_tap`, `device_swipe`,
  `device_type`, `device_describe_ui`, `device_screenshot`,
  `device_logs`).
- Dangerous device operations (install/uninstall/erase) sit behind a
  fail-closed `ApprovalGate`: every allow AND deny is appended to a
  JSONL audit log; denial makes zero backend calls.
- `scripts/device-bench.sh`: reproducible 60 s animated-stream
  benchmark (metrics.json schema shared with CI) for Apple Silicon
  hosts with GPU acceleration.

### Events

- New `supercli-events` crate: durable, hash-chained hook event log
  with `hooks.toml` routing.
- `supercli hooks` CLI: `list`, `test <name>`, `trace [id]` (audit-log
  lineage, relative timing, approve/deny decisions, entry hashes).
- ToolCall `before_execute` hooked after write-ahead review fsync and
  before tool bytes are sent; hook escalations re-enter the approval
  flow as Ask (never silent rejection).
- Doctypes: Session (`autoname`, `on_update`/`on_change`,
  `on_trash` with reject veto, `after_delete`), Turn
  (`on_update`/`on_change` on finish), Run (scheduled runs with
  field-level `doc_diff`), Device (`emit_device_update` for backend
  state transitions), Approval, ToolCall.

### Durable runs

- Write-ahead run journal: `begin_step`/`complete_step` fsync before
  effects; per-operation-kind reconciliation (Model, Read, File write,
  Idempotent HTTP, Opaque write).
- Fail closed: the scheduled runner refuses to run without a journal
  unless `--no-durable` / `allow_unjournaled()` is given explicitly.
- Crash recovery proven: daemon SIGKILL mid-run resumes the same run
  ID with zero duplicate completed effects.
- Scheduled triggers (`run-once`, daemon) wired to durable runs.

### gpuidart app

- Native desktop/mobile client (Dart/Flutter): session list, approval
  cards, composer, takeover surface.
- Headless approval proof: real production Host connection handler
  (TLS, ApprovalHub, bearer auth); Dart headless process approves a
  real blocking request, Host asserts `approved == true` and
  `answered_by == paired-device`.
- Keyboard map: Ctrl+Enter approve, Ctrl+Shift+Enter deny, arrows
  navigate sessions, Ctrl+L composer focus.

### Memory

- `supercli memory` CLI: `set`, `get`, `promote`, `forget`, `list`;
  session and long-term scopes with atomic, lock-protected saves.
- Operator profile: advisory-only hints. `suggest_auto_allow` is pure
  advice — it can never grant anything by itself; only an explicit
  user action through the audited grant path creates a grant.
- Human approval decisions feed profile counters.

### Browser takeover

- CDP client refuses any endpoint that is not loopback (127.0.0.0/8,
  ::1, localhost) before any socket is opened; fails closed on
  unresolvable names.
- Live human takeover: pause the agent's browser actions, forward the
  human's clicks and keys via CDP `Input.dispatchMouseEvent` /
  `Input.dispatchKeyEvent`, then hand control back. Every transition
  (`takeover_begin`, `takeover_pause`, `takeover_resume`) is an
  fsync'd audit entry; input from the side not holding control is
  refused.

### Security

- Secret scanning (gitleaks) and the rename guard run in CI and in
  `scripts/fresh-clone-verify.sh`; both must be green.
- Every commit is authored and committed as
  `Amein Eskinder <62555273+AmeinEskinder@users.noreply.github.com>`;
  the identity check enforces this on all reachable commits.
