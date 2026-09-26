# Device support design — Android & iOS in supercli

**Status:** design only (no implementation yet)
**Reference:** [baguette](https://github.com/tddworks/baguette) (Apache-2.0) — an agent drives it while its browser UI streams a headless iPhone: 60 fps H.264 over WebSocket, real taps/swipes/pinch/Home/Lock, an a11y tree, live `os_log`. That agent-drives, human-watches loop is the whole point.
**Constraint:** LITE. Shell out to installed platform tools. No bundled emulators, no vendored servers, no heavy deps. Behind a `device` cargo feature; zero cost when unused.

**Key correction (2026-09-26):** `simctl` has **no input injection** — no tap, swipe, type, pinch. A simctl-only iOS backend cannot meet the spec. iOS input and streaming therefore go through baguette; simctl is kept only for lifecycle (boot/shutdown) and screenshots. The Android side is brought to the same quality bar with scrcpy-server.

---

## 1. CLI surface

New top-level verb group: `supercli device <subcommand>`. All subcommands take `--device <id>` (adb serial / baguette session id / simctl UDID); when omitted, the single running device is used, and multiple running devices is an error asking for `--device`.

| Subcommand | Android | iOS |
|---|---|---|
| `list` | `adb devices -l` + `emulator -list-avds` (merge: running vs available AVDs) | baguette session list, else `xcrun simctl list devices available --json` |
| `boot <avd\|udid>` | `emulator -avd <name> -no-window -no-audio` (daemonize; wait for `adb wait-for-device` + `sys.boot_completed=1`, 120 s timeout) | `xcrun simctl boot <udid>` (simctl: lifecycle only) |
| `stop` | `adb -s <id> emu kill` | `xcrun simctl shutdown <udid>` (simctl: lifecycle only) |
| `install <apk\|app>` | `adb -s <id> install -r <apk>` | `xcrun simctl install <udid> <app>` |
| `launch <pkg\|bundle-id>` | `adb -s <id> shell monkey -p <pkg> 1` | `xcrun simctl launch <udid> <bundle-id>` |
| `screenshot [-o file]` | `adb -s <id> exec-out screencap -p` (PNG to stdout) | baguette CLI screenshot, else `xcrun simctl io <udid> screenshot <file>` |
| `logs [--clear]` | `adb -s <id> logcat -d` (or `-c` for clear) | baguette `serve` WebSocket `os_log` stream (live); `simctl spawn <udid> log show` as fallback |
| `tap <x> <y>` | scrcpy input injection; fallback `adb -s <id> shell input tap <x> <y>` | **baguette** (real tap). simctl cannot inject input — never report success from simctl. |
| `type <text>` | scrcpy input injection; fallback `adb -s <id> shell input text <escaped>` | **baguette** (real key input). |
| `swipe <x1> <y1> <x2> <y2> [ms]` | scrcpy input injection; fallback `adb -s <id> shell input swipe …` | **baguette** (real swipe; pinch also available via baguette only) |
| `describe-ui` | `adb -s <id> shell uiautomator dump` → XML a11y tree | baguette a11y-tree JSON |
| `stream` | scrcpy-server H.264 (60 fps) piped to stdout (see §3); fallback `adb exec-out screenrecord --output-format=h264 -` | baguette `serve` WebSocket (60 fps H.264); fallback `simctl io recordVideo` segments |

**Platform gating:** every iOS subcommand first checks `cfg!(target_os = "macos")`. On other hosts it exits 2 with exactly: `iOS Simulator requires a macOS host`. No fake success, no partial output. baguette-backed commands additionally check for Apple Silicon (`std::env::consts::ARCH == "aarch64"`) and baguette on PATH; missing → exit 2 with `baguette not found on PATH (see https://github.com/tddworks/baguette; requires Apple Silicon, macOS 15+, Xcode 26)`. Android subcommands check for `adb` on PATH (exit 2, `adb not found on PATH (install Android SDK platform-tools)`); scrcpy-backed paths check for `scrcpy` on PATH and fall back to adb automatically.

**Timeouts:** all tool invocations go through one `run_tool()` helper with a 120 s default timeout, capturing stdout/stderr; on timeout the child is killed and exit 124 is reported with the tool name.

---

## 2. LITE architecture

New crate `crates/supercli-device`, added to the workspace `members` list from day one (named `supercli-device`, no rename conflict).

```
crates/supercli-device/
  Cargo.toml          # [features] device = [] ; no mandatory deps
  src/lib.rs          # Backend trait + error types (always compiled)
  src/adb.rs          # #[cfg(feature = "device")] Android via adb/emulator (fallback tier)
  src/scrcpy.rs       # #[cfg(feature = "device")] Android via installed scrcpy-server (primary tier)
  src/simctl.rs       # #[cfg(feature = "device")] iOS lifecycle + screenshots ONLY
  src/baguette.rs     # #[cfg(feature = "device")] iOS via installed baguette (stream/input/a11y/logs)
  src/fake.rs         # #[cfg(test)] ONLY — test double, never compiled into any binary
```

- **Core (`lib.rs`, no feature gate):** `DeviceId`, `DeviceInfo`, `DeviceBackend` trait (`list/boot/stop/install/launch/screenshot/logs/input/stream/describe_ui`), `DeviceError` (ToolMissing, NotMacOSHost, Timeout, ToolFailed { tool, code, stderr }, Unsupported). Compiles to ~nothing without the feature.
- **Backends (`adb.rs`, `scrcpy.rs`, `simctl.rs`, `baguette.rs`):** `#[cfg(feature = "device")]`. Only dependency is `std::process::Command` (+ std WebSocket client for baguette's `serve` socket — `tungstenite` is allowed as an optional dependency only if hand-rolling proves unworkable; default to std). No vendored servers: baguette and scrcpy must be installed by the user; the crate shells out to them.
  - **iOS:** `BraidBackend` wraps baguette (CLI JSON for control, `serve` WebSocket for H.264 + a11y + os_log + input). `SimctlBackend` handles boot/shutdown/install/launch/screenshot only. `tap`/`type`/`swipe`/`describe_ui` on simctl return `Unsupported` honestly.
  - **Android:** `ScrcpyBackend` is tried first when `scrcpy` is on PATH (H.264 stream + input injection + `uiautomator dump` a11y + logcat). `AdbBackend` is the fallback (screenrecord stream, `input` injection, uiautomator, logcat).
- **`fake.rs` is a test double, full stop.** `#[cfg(test)]` only — it is never a runtime path and can never report success to a user. If a fake backend is ever needed outside tests, it must be a separate explicit `--dry-run` flag, not this module.
- **CLI wiring:** `supercli-cli` gets `device` as an *optional* dependency: `supercli-device = { path = "../supercli-device", optional = true }`, and a `device` feature on the CLI that enables it. `supercli device …` without the feature prints: `rebuild with --features device` (exit 2).
- **Zero cost when unused:** default build has no `device` feature → `supercli-device` compiles to the trait + error types only (a few KB). All four backends are `#[cfg]`'d out entirely.

**Binary-size delta (to be measured at implementation):** build `supercli` release with and without `--features device`, report both sizes. Expected delta: < 50 KB (pure `std::process` wrappers, no new mandatory crates). The number goes in this doc after measurement — do not ship without it.

---

## 3. Web UI Devices panel

Location: new tab in the Dioxus web UI next to the conversation (mirrors the baguette reference: live device preview beside the chat — the agent drives, the human watches).

- **Chrome:** minimal — device frame (rounded rect, notch/Dynamic Island for iOS, punch-hole for Android), live screen filling the frame, one small toolbar (home, back, rotate).
- **Multi-device grid:** when more than one device is running, the panel shows them side-by-side in a grid; clicking a device focuses it (input routes to the focused device, `device_id` is explicit everywhere).
- **Input:** click on screen → `tap x y` (frame pixels scaled to device pixels via the last a11y/screenshot dimensions). Keyboard focus on screen → `type`. Drag → `swipe`; two-finger/pinch gesture → baguette pinch (iOS only). Toolbar → home/back/rotate.
- **iOS streaming (primary):** baguette `serve` WebSocket → 60 fps H.264 → WebCodecs `VideoDecoder` → `<canvas>`. Fallback: `simctl io recordVideo` 10 s segments played sequentially, or `simctl io screenshot` polling at 2 fps. Honest labeling: the panel shows "baguette" or "simctl fallback" as the stream source.
- **Android streaming (primary):** scrcpy-server H.264 (60 fps) → WebCodecs. Fallback: `adb exec-out screenrecord` (reconnect every 170 s under the 180 s cap), then `screencap -p` polling at 2 fps.
- **a11y overlay (optional):** `describe-ui` results can be overlaid as element outlines on the stream (debug aid; agents consume the raw tree).
- **Latency:** measure input-to-frame as (a) time from `tap` command spawn to next decoded frame presented, sampled over 50 taps per backend, reported as p50/p99 in the implementation report. **Do not claim "low latency"** — publish the numbers.

---

## 4. Agent tools (MCP surface on the Host)

New tools on the unified MCP server (`supercli-host __mcp__`), all taking `device_id`. The loop: agent calls `device.describe-ui` → picks an element → acts via `tap`/`type`/`swipe` → human watches the stream. **Agents act on elements, not pixels** — `describe-ui` is the primary perception tool; raw-coordinate input is the fallback.

| Tool | Approval | Notes |
|---|---|---|
| `device.describe-ui` | none (read-only) | **NEW.** a11y tree: baguette JSON on iOS, `uiautomator dump` XML on Android. Element ids, labels, bounds, actions. |
| `device.screenshot` | none (read-only) | returns PNG bytes (base64) + width/height |
| `device.tap` | none | x, y in device pixels (prefer element bounds from describe-ui) |
| `device.type` | none | text; Android `input text` escaping documented |
| `device.swipe` | none | x1,y1,x2,y2, duration_ms; pinch via baguette on iOS |
| `device.logs` | none (read-only) | last N lines (logcat / os_log); `--clear` not exposed to agents |
| `device.install` | **Ask (approval flow)** | apk/app path must be inside the workspace |
| `device.uninstall` | **Ask** | package/bundle id |
| `device.erase` | **Ask** | `adb wipe-data` / `simctl erase`; double-confirm in prompt text |

Read-only/input/perception tools are agent-usable without prompting (same policy class as terminal input). Anything that writes persistent device state goes through the existing `ApprovalHub` — no new approval plumbing.

---

## 5. Tests

- **Unit (`#[cfg(test)]` in `supercli-device/src/fake.rs`):** a `FakeBackend` implementing `DeviceBackend` with scripted responses. Tests: list parsing (adb + simctl JSON + baguette JSON fixtures), `describe-ui` fixture parsing (uiautomator XML + baguette a11y JSON), boot timeout path, `NotMacOSHost` gating (assert the exact error string), tool-missing error, tap coordinate passthrough, `Unsupported` on simctl input. Runs on every PR via the existing `linux-cli.yml` workspace tests. No emulator needed.
- **Android e2e (CI):** new job in `.github/workflows/android.yml` using `ReactiveCircus/android-emulator-runner@v2` on `ubuntu-latest` with `api-level: 34, arch: x86_64, target: google_apis`. Steps: boot AVD, `supercli device list` shows `emulator-5554`, `describe-ui` returns non-empty XML, `screenshot` produces a PNG, `tap`/`type`/`swipe` exit 0, `logs` non-empty. If scrcpy is installable on the runner, run the scrcpy tier; otherwise adb tier — the report states which.
- **iOS e2e (CI):** new job in `.github/workflows/apple.yml` on an Apple Silicon macOS runner with Xcode 26 and baguette installed: boot a simulator via simctl, drive it through the baguette tier (`describe-ui`, `tap`, `screenshot`, live `os_log`), `shutdown`. If the runner lacks Xcode 26 / baguette, the job reports `SKIP (missing Xcode 26 or baguette)` honestly — never fake-passes.
- **Report rule:** the implementation report must state exactly what ran where (e.g. "fake-backend 31/31 on ubuntu-22.04; android adb-tier e2e on GHA ubuntu-latest KVM; ios e2e SKIP — no macOS runner with Xcode 26; no local runs — this VM has no /dev/kvm and no Xcode").

---

## 6. gpuidart widgets needed (for Amein)

Already added to `docs/gpuidart-requirements.md` as P0-6/P0-7:

**P0-6. GPU texture / video surface.** Displays a decoded video frame (WebCodecs on web; native decoder on desktop) with pointer and key events — this is the live device screen.

**P0-7. Device-frame container.** Rounded-rect frame with notch/status-bar styling so the panel reads as a device. Multi-device grid is a layout of several P0-7 frames.

**Note for P0-7/future:** the a11y tree from `describe-ui` could later be rendered as an element-outline overlay; not a new widget, just data the app draws.

---

## 7. Environment facts (this VM, 2026-09-26)

- `/dev/kvm`: **does not exist** — no KVM, Android emulator cannot run here.
- `adb`, `emulator`, `scrcpy`, `xcrun`, `baguette`: **not installed**.
- Host arch: `x86_64` Linux. iOS work must be macOS-only by design (§1 gating); baguette additionally needs Apple Silicon.
- Consequence: all emulator e2e runs in CI (GHA ubuntu KVM runners, Apple Silicon macOS runner with Xcode 26 + baguette). Local verification here is limited to the fake-backend unit suite and CLI arg parsing.

---

## 8. Implementation order (for the 2–3 h push)

1. `crates/supercli-device`: lib.rs (trait + errors, incl. `describe_ui`) → adb.rs → scrcpy.rs → simctl.rs (lifecycle only) → baguette.rs → fake.rs + unit tests.
2. CLI wiring: `supercli device` subcommands (incl. `describe-ui`) in `supercli-cli` behind `device` feature; measure binary-size delta.
3. MCP tools in `supercli-host` (`device.*` incl. `device.describe-ui`; approval on install/uninstall/erase).
4. CI jobs: android e2e (emulator-runner, scrcpy tier if installable), iOS job (Apple Silicon macOS + Xcode 26 + baguette; honest SKIP otherwise).
5. Web UI Devices panel (Dioxus web): device frame + stream + input + toolbar + multi-device grid; latency numbers.
6. P0-6/P0-7 already in `docs/gpuidart-requirements.md` — no action.

---

## 9. Design delta — Amein review 2026-09-26 (headless + native scrcpy client + unified wire format)

**Context:** Amein reviewed `crates/supercli-device` at `e0b131c`. Good: the emulator already boots `-no-window -no-audio`. Problems: (a) the 'scrcpy tier' is only a label — tap/swipe/type still go through `adb shell input` (200–500 ms per call, no multi-touch or pinch); (b) `stream()` runs `scrcpy --no-window --record -`, which scrcpy does not support to stdout — there is no real 60 fps stream, and the fallback `screenrecord` has a 3-minute cap; (c) there is no web Devices panel or WebSocket route at all yet, no `/farm`, no live logs, no webcam.

Android must be HEADLESS and lite exactly like baguette is for iOS: no emulator window, no Android Studio, no scrcpy desktop app.

### 9.1 Headless emulator (the analog of baguette's headless iPhone)

Boot: `emulator -avd <name> -no-window -no-audio -no-boot-anim -gpu host -grpc <port> -camera-back webcam0`. Fall back to `-gpu swiftshader_indirect` when host GPU is unavailable. Quick-boot snapshots for fast start (`-snapshot default_boot` / `snapshot save`). Host webcam → emulated camera is built into the emulator (`-camera-back webcam0`) — this is the analog of baguette's webcam feature, no extra plumbing.

### 9.2 Native scrcpy client in Rust (no scrcpy app)

New module `scrcpy_native.rs` (`#[cfg(feature = "device")]`) speaks the scrcpy-server protocol directly:

1. `adb push` the **pinned** scrcpy-server jar (Apache-2.0, SHA-256 verified at download; pin recorded in `docs/device.md` at implementation), `adb forward tcp:<port>`, then start the server via `app_process` with: h264 video, `max_fps 60`, audio off, control on.
2. **Video socket** carries H.264 packets → relayed to the browser over WebSocket → WebCodecs decode. No stdout pipe, no 3-minute cap.
3. **Control socket** carries: multi-touch with pointer ids (so pinch works), HOME / BACK / POWER (Lock), text, clipboard, rotation.
4. `adb shell input` is dropped to **last-resort fallback** (when the native client cannot start), and its use is logged as a fallback event.

The scrcpy-server jar is Apache-2.0; on implementation its copyright/version notice goes into `THIRD_PARTY_NOTICES.txt`.

**Pinned server (recorded at implementation, 2026-09-26):** scrcpy-server **v2.7**
(`crates/supercli-device/src/scrcpy_native.rs`).
- Release asset: `https://github.com/Genymobile/scrcpy/releases/download/v2.7/scrcpy-server-v2.7`
- SHA-256: `a23c5659f36c260f105c022d27bcb3eafffa26070e7baa9eda66d01377a1adba`
  (computed by downloading the asset from the official Genymobile/scrcpy
  release; independently re-verified with Python hashlib; 71,200 bytes).
- Notice: `THIRD_PARTY_NOTICES.txt` → "MANUAL NOTICE 1 (scrcpy-server)".
- Wire details implemented: video header = 64-byte device name + codec id
  (`b"h264"`) + width + height (u32 BE each); packets = 12-byte header
  (u64-BE pts in µs + u32-BE size) + Annex-B H.264 payload; keyframes
  detected by NAL-unit scan for IDR slices (v2.x carries no keyframe flag —
  the flag bits in the PTS high bits are a scrcpy-3.x protocol change).
- Control messages (big-endian, leading type byte): keycode (14 B),
  text (5+len B), touch (32 B: action + u64 pointer id + x/y + u16 w/h +
  u16 fixed-point pressure + action button + buttons), clipboard set
  (14+len B), rotate (1 B). All byte-exact encoders are unit-tested.
- Unified wire format (§9.3) is implemented in the same module:
  `type (1 B) | length (u32 BE) | payload` with 0x01 description (JSON),
  0x02 keyframe, 0x03 delta, 0x04 JPEG seed.

### 9.2b Native baguette client in Rust (serve WebSocket + input pipe)

New module `baguette_native.rs` (`#[cfg(feature = "device")]`, std-only —
the hand-rolled WebSocket handshake kept `tungstenite` out, per the §2
LITE default-to-std rule) speaks baguette's long-lived protocols directly, with
no per-command CLI round-trips. baguette itself is **not vendored**; it must
be installed by the user (`brew install baguette`).

**Verified against upstream on 2026-09-26** (baguette README "Quick start"
+ "Wire protocol — `baguette input"`):

- `baguette serve` binds **127.0.0.1:8421** (web UI at `/simulators`).
  `BaguetteNative::connect` probes the port and spawns `baguette serve`
  only when nothing listens (15 s startup wait); `Drop` kills the serve
  child **only if this session spawned it**.
- Video: `WS /devices/<udid>/stream?format=avcc`. Every WebSocket message
  is one unified wire frame (§9.3): the first is the `0x01` description
  (parsed for device name + point/pixel geometry), then `0x02`/`0x03`/`0x04`
  frames, validated with `wire_from_baguette` and relayed **byte-identical**
  — supercli proxies baguette's iOS stream directly, exactly as §9.3
  intends.
- WebSocket: RFC 6455 upgrade with hand-rolled SHA-1/Base64
  `Sec-WebSocket-Accept` verification (pinned against the RFC 6455 §1.3
  test vector), masked client frames, ping→pong, close→`Closed`, 64 MiB
  message cap, masked server frames rejected as protocol violations, and
  handshake-overflow buffering (one TCP segment can carry the `101` head
  *and* the first frames).
- Input: a persistent `baguette input --udid <udid>` child. Gestures are
  newline-delimited JSON on stdin → one `{"ok":true}` /
  `{"ok":false,"error":…}` ack per line on stdout (5 s ack deadline via a
  dedicated reader thread; an exited child surfaces an error, never a
  hang). Encoders, all in **device points** (rounded to integer points,
  matching the documented examples):
  - `tap`: `{"type":"tap","x":219,"y":478,"width":438,"height":954,"duration":0.05}`
  - `swipe`: `startX/startY/endX/endY` + `width/height` + `duration`
  - `touch1-down/move/up` (+ optional `edge` for system gestures),
    `touch2-down/move/up` (the pinch path)
  - `button`: `home`, `lock`, `power`, `volume-up/down`, `action`,
    `app-switcher`, `swipe-to-home`, … (+ optional `duration`)
  - `key` (W3C `code`), `text`
- Session shape mirrors `scrcpy_native.rs`: `BaguetteNative::connect`
  gates (macOS → Apple Silicon → `baguette` on PATH, honest
  `NotMacOSHost`/`ToolMissing` errors), `split()` hands out disjoint
  `(&WsClient, &mut InputChannel)` borrows so one thread pumps video while
  another sends input, `description_frame()` returns the raw `0x01` frame
  so the Devices panel / `/farm` bootstrap iOS sessions exactly like
  Android ones — one decoder path, one input path, platform selected only
  by the device id.
- Tests: scripted fake `baguette serve` (upgrade path assertion, accept-key
  verification, ping→masked-pong, `0x01`→`0x02`→close frame flow,
  byte-identical passthrough) and stub `baguette input` children
  (ack round-trip, rejection surfacing, exited-child error). All run on
  Linux; the real-device path needs a macOS host with baguette installed.

### 9.3 One wire format for both platforms

Adopt baguette's framing verbatim: `0x01` description (stream metadata), `0x02` keyframe, `0x03` delta, `0x04` JPEG seed (recovery on packet loss) — plus baguette's device-point coordinate convention for input. supercli then **proxies baguette's iOS stream directly** and emits the **same** format for Android from the native scrcpy client. The web Devices panel and the gpuidart P0-6 surface stay platform-agnostic: one decoder path, one input path, platform selected only by the device id.

### 9.4 Parity with the post (baguette reference)

Full target surface, both platforms: 60 fps H.264 stream; real taps, swipes, **pinch**, Home, Lock; a11y tree (`uiautomator dump` on Android, baguette `describe-ui` on iOS); **live logs streamed over WebSocket** (`logcat` on Android, `os_log` on iOS); webcam-to-camera; and a multi-device **/farm wall** where each tile gets reduced fps/bitrate (e.g. 15 fps, 2 Mbps per tile) so N devices stream without saturating the link.

### 9.5 One-command setup (analog of `brew install baguette`)

- `supercli device setup android` — uses `sdkmanager` to install `emulator` + `platform-tools` + one system image (`arm64` on Apple Silicon, `x86_64` elsewhere), then creates the AVD (`avdmanager create avd`). It **downloads**, so it goes through the approval flow (Ask) like `device.install`.
- `supercli device setup ios` — checks for baguette on PATH; if missing, prints exactly `brew install baguette` and exits 2. No download, no approval needed.

### 9.6 Web: Devices panel + /farm

`supercli-serve` gains: a **Devices panel** (single focused device: frame + 60 fps stream + toolbar + input) and a **/farm route** (multi-device wall, reduced fps/bitrate tiles, click-to-focus). Both speak the unified wire format from §9.3 over WebSocket. The gpuidart surface lands in `clients/supercli-app/` as **P0-6/P0-7** (already specified in `docs/gpuidart-requirements.md`); web (Dioxus) and gpuidart share the same frame format, so input/decode code stays platform-agnostic.

### 9.7 Proof (CI, not this VM)

- **Real headless emulator in CI:** ubuntu runner **with KVM** (state `/dev/kvm` presence in the report; this VM has none — see §7). No emulator window, ever.
- **Metrics:** fps measured over 60 s on the H.264 stream; **tap-to-frame latency p50/p95** through the native control channel vs `adb shell input` (same 50-tap protocol as §3, reported side by side — prove the 200–500 ms claim against reality).
- **Pinch test:** scripted pinch on a map or photo app via control socket (multi-pointer), asserted from the video/a11y state, not from adb.
- **/farm:** 3 headless devices streaming simultaneously at reduced bitrate; report per-tile fps.
- **Honesty rule (extends §5):** every implementation report states exactly what ran in CI vs this VM, including whether `/dev/kvm` existed wherever the run happened.
