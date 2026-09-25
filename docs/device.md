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
