# Device support — Android & iOS in supercli

**Status:** implemented on `track-b-device` (CI green pending final gate).
**Reference:** [baguette](https://github.com/tddworks/baguette) (Apache-2.0) — an agent drives it while its browser UI streams a headless iPhone: 60 fps H.264 over WebSocket, real taps/swipes/pinch/Home/Lock, an a11y tree, live `os_log`. That agent-drives, human-watches loop is the whole point.
**Constraint:** LITE. Shell out to installed platform tools. No bundled emulators, no vendored servers, no heavy deps. Behind a `device` cargo feature; zero cost when unused.

**Key correction (2026-09-26):** `simctl` has **no input injection** — no tap, swipe, type, pinch. A simctl-only iOS backend cannot meet the spec. iOS input and streaming therefore go through baguette; simctl is kept only for lifecycle (boot/shutdown) and screenshots. The Android side is brought to the same quality bar with scrcpy-server.

---

## 1. What is implemented today

| Piece | Location | State |
|---|---|---|
| `supercli-device` crate (trait, errors, backends) | `crates/supercli-device/` | implemented, `device` feature |
| Native scrcpy backend (no `scrcpy` CLI needed) | `src/scrcpy_native.rs` | implemented, CI-tested |
| Wire format 0x01–0x04, device points | `src/wire_format.rs` | implemented |
| `adb` fallback backend | `src/adb.rs` | implemented |
| `simctl` lifecycle backend | `src/simctl.rs` | implemented (lifecycle + screenshots only) |
| baguette iOS backend | `src/baguette.rs` | in progress |
| `supercli device setup android\|ios` | `crates/supercli-cli/src/device_cli.rs` | implemented |
| Web Devices panel + `/farm` wall | `crates/supercli-serve/src/devices.rs`, `static/devices.html`, `static/farm.html` | implemented |
| Android CI (headless emulator) | `.github/workflows/device-android.yml` | correctness gate (see §7) |
| Mac bench script | `scripts/device-bench.sh` | implemented, untested on hardware |
| Full `supercli device` verb table (§2) | — | planned |

## 2. CLI surface (target)

New top-level verb group: `supercli device <subcommand>`. All subcommands take `--device <id>` (adb serial / baguette session id / simctl UDID); when omitted, the single running device is used, and multiple running devices is an error asking for `--device`.

Implemented today: `supercli device setup android [--yes] [--dry-run]` (checks adb/emulator, downloads the pinned scrcpy-server jar, verifies SHA-256) and `supercli device setup ios` (checks macOS + Apple Silicon + baguette on PATH). Exit codes: 0 success · 1 tool failure · 2 missing tool / bad usage / not-macOS.

The rest of the table is the target, implemented backend-first:

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
| `stream` | scrcpy-server H.264 piped to stdout (see §3); fallback `adb exec-out screenrecord --output-format=h264 -` | baguette `serve` WebSocket (H.264); fallback `simctl io recordVideo` segments |

**Platform gating:** every iOS subcommand first checks `cfg!(target_os = "macos")`. On other hosts it exits 2 with exactly: `iOS Simulator requires a macOS host`. No fake success, no partial output. baguette-backed commands additionally check for Apple Silicon (`std::env::consts::ARCH == "aarch64"`) and baguette on PATH; missing → exit 2 with `baguette not found on PATH (see https://github.com/tddworks/baguette; requires Apple Silicon, macOS 15+, Xcode 26)`. Android subcommands check for `adb` on PATH (exit 2, `adb not found on PATH (install Android SDK platform-tools)`).

**Timeouts:** all tool invocations go through one `run_tool()` helper with a 120 s default timeout, capturing stdout/stderr; on timeout the child is killed and exit 124 is reported with the tool name.

---

## 3. Native scrcpy backend (`scrcpy_native.rs`)

No `scrcpy` CLI is required. The backend downloads the pinned server jar and drives it directly:

- **Pinned server:** scrcpy-server **v2.7** (`SCRCPY_SERVER_VERSION`), SHA-256 verified after download, cached under `~/.supercli`, pushed to `/data/local/tmp/scrcpy-server.jar` via `adb push` (only after `sys.boot_completed=1` **and** `pm path android` — the package manager must be ready).
- **Server launch:** `CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 2.7 tunnel_forward=true audio=false control=true cleanup=false video_codec=h264 max_fps=60`. The first `app_process` argument must be the **exact** server version string (`2.7`, not `2.7.0`); anything else and the server dies immediately.
- **v2.x forward-tunnel handshake** (`handshake_video_control`) — this ordering is load-bearing. With `tunnel_forward=true` the server accepts **all** sockets first (video, then control) and only then writes anything, so a client that waits for the video header before opening the control socket deadlocks:
  1. Connect socket #1 (video).
  2. Connect socket #2 (control) **immediately** (no audio socket: `audio=false`).
  3. Read the 1-byte dummy on the video socket. EOF/reset here means the server isn't listening yet: close both sockets and retry the connect every 100 ms for up to 10 s.
  4. Read the 76-byte header: 64-byte NUL-padded device name, then u32 codec id (`h264`), u32 width, u32 height, big-endian.
  5. Packets: 8-byte pts/flags header (config = bit 63, keyframe = bit 62), u32 length, payload.
- **Control channel** (`ControlChannel`): touch injection in **device points** (see §4), keycodes, text injection, back/screen-on, clipboard, display power, rotate, notification panel expand/collapse.
- `send_device_meta` and `send_dummy_byte` stay at their defaults. The default encoder is used (the forced `video_encoder=c2.android.avc.encoder` was dropped).
- The handshake is pinned by unit tests against a scripted fake server that reproduces the exact server ordering (accepts both sockets before writing anything), so the deadlock cannot regress.

## 4. Wire format (`wire_format.rs`) — device points, baguette convention

Touch input uses `DevicePoint`: coordinates in the **device's point coordinate system**, not normalized 0.0–1.0 and not raw pixels. Callers pipe the 0x01 description's `width_points`/`height_points` straight through; `DevicePoint::from_android_pixels` / `to_android_pixels` convert via density DPI at the edges.

Frame types 0x01–0x04: `0x01` stream description (screen size in device points), `0x02` H.264 packet, `0x03` JPEG seed frame, `0x04` baguette-passthrough frame (`wire_from_baguette`). The iOS baguette path reuses the same framing so the web panel and the Host see one stream shape.

## 5. Web UI: Devices panel + `/farm`

`crates/supercli-serve/src/devices.rs` (local-only routes):

- `GET /farm` — HTML multi-device wall (`static/farm.html`): a grid of live tiles, one per running device, at reduced fps per tile.
- Devices panel (`static/devices.html`): single focused device — live screen filling a device frame (notch/punch-hole styling), toolbar (home, back, rotate), click → tap, drag → swipe, keyboard → type. All coordinates are device points end-to-end (JS multiplies by the 0x01 description's `width_points`/`height_points`; the Rust `/touch` API validates finite, non-negative points).
- Streaming: scrcpy H.264 → WebCodecs `VideoDecoder` → `<canvas>` (Android); baguette WebSocket → same path (iOS). Fallback: `adb exec-out screenrecord`, then `screencap -p` polling at 2 fps — the panel labels the stream source honestly.

## 6. Agent tools (MCP surface on the Host) — planned

New tools on the unified MCP server (`supercli-host __mcp__`), all taking `device_id`. The loop: agent calls `device.describe-ui` → picks an element → acts via `tap`/`type`/`swipe` → human watches the stream. **Agents act on elements, not pixels** — `describe-ui` is the primary perception tool; raw-coordinate input is the fallback.

| Tool | Approval | Notes |
|---|---|---|
| `device.describe-ui` | none (read-only) | **NEW.** a11y tree: baguette JSON on iOS, `uiautomator dump` XML on Android. Element ids, labels, bounds, actions. |
| `device.screenshot` | none (read-only) | returns PNG bytes (base64) + width/height |
| `device.tap` | none | x, y in device points (prefer element bounds from describe-ui) |
| `device.type` | none | text; Android `input text` escaping documented |
| `device.swipe` | none | x1,y1,x2,y2, duration_ms; pinch via baguette on iOS |
| `device.logs` | none (read-only) | last N lines (logcat / os_log); `--clear` not exposed to agents |
| `device.install` | **Ask (approval flow)** | apk/app path must be inside the workspace |
| `device.uninstall` | **Ask** | package/bundle id |
| `device.erase` | **Ask** | `adb wipe-data` / `simctl erase`; double-confirm in prompt text |

Read-only/input/perception tools are agent-usable without prompting (same policy class as terminal input). Anything that writes persistent device state goes through the existing `ApprovalHub` — no new approval plumbing.

---

## 7. CI: Android correctness gate

`.github/workflows/device-android.yml` runs the native-scrcpy e2e on a real headless emulator (`ReactiveCircus/android-emulator-runner@v2`, API 34 `google_apis` x86_64, `pixel_7` profile = 1080x2400).

The CI runner has no GPU: the emulator renders with SwiftShader and scrcpy software-encodes 1080x2400 on 2–4 vCPUs. That hardware cannot do 60 fps, and the encoder dominates latency — so CI is a **correctness gate**, not a performance gate:

- handshake completes (dummy byte, control connected, valid 76-byte header);
- ≥ 100 H.264 packets in the window;
- pinch OK, HOME key OK;
- tap-to-frame misses ≤ 10% (a trial with no frame within 1 s is a miss, counted, not fatal);
- control-channel tap-to-frame p50 **<** `adb shell input` tap-to-frame p50.

fps and latency are **recorded in `metrics.json` as numbers, not gates**. The same job also runs a `max_size=720` pass and records both; the emulator's own render rate during the fling window (`dumpsys gfxinfo` / SurfaceFlinger) is captured so the renderer vs encoder bottleneck is visible.

Measured (run #12, 1080x2400): control-channel tap-to-frame p50 382 ms / p95 1035 ms vs adb input p50 676 ms / p95 1156 ms — the control channel is ~1.8× faster. fps while the home screen is static is ~2–3 (scrcpy only emits on screen change); fps is measured while animating (Settings fling-scroll loop via the control channel).

Artifacts on every run (`if: always()`): `metrics.json`, screenshot, 10 s MKV, `e2e.log`, scrcpy-filtered logcat, server stderr. The workflow publishes key stages to `GITHUB_STEP_SUMMARY` (boot_completed, jar pushed, server alive, first packet, packet count) so the run page is readable without log access.

## 8. Local bench: `scripts/device-bench.sh`

60 fps gets proven on real hardware, not CI. `scripts/device-bench.sh` runs on an Apple Silicon Mac (HVF + GPU):

- prereq checks (`emulator`, `adb`, `cargo`, `ANDROID_SDK_ROOT`; one-time SDK install steps in the header);
- creates/reuses AVD `supercli-bench` (API 34 `google_apis/arm64-v8a`, `pixel_7`);
- headless boot (`-no-window -no-audio -no-boot-anim -gpu host`), waits for `sys.boot_completed=1` **and** `pm path android`;
- runs the same e2e as CI (`SUPERCLI_ANDROID_E2E=1 cargo test -p supercli-device --features device --test android_e2e`), so `metrics.json` is byte-for-byte the same schema CI uploads;
- saves `screenshot.png`, a 10 s screen recording, and `e2e.log`; kills the emulator on exit unless `KEEP_EMULATOR=1`.

Env overrides: `AVD_NAME`, `API_LEVEL`, `DEVICE_PROFILE`, `ANDROID_SERIAL`, `OUT_DIR`, `WIPE_DATA`, `KEEP_EMULATOR`.

## 9. iOS

`simctl.rs` covers lifecycle + screenshots only (boot/shutdown/install/launch/screenshot). Input, streaming, a11y, and live logs go through the baguette passthrough (`baguette.rs`, in progress): baguette's `serve` WebSocket framed as 0x04 wire frames, proxied to the same Devices panel and `/farm` infrastructure as Android.

## 10. LITE architecture notes

```
crates/supercli-device/
  Cargo.toml          # [features] device = [] ; no mandatory deps
  src/lib.rs          # DeviceId, Platform, DeviceInfo, DeviceBackend trait, DeviceError
  src/adb.rs          # Android via adb/emulator (fallback tier)
  src/scrcpy.rs       # Android via installed scrcpy CLI (alternate tier)
  src/scrcpy_native.rs# Android via pushed scrcpy-server v2.7 (primary tier)
  src/simctl.rs       # iOS lifecycle + screenshots ONLY
  src/baguette.rs     # iOS via installed baguette (stream/input/a11y/logs)
  src/wire_format.rs  # 0x01–0x04 framing, DevicePoint
  src/setup.rs        # `device setup` checks + jar download/verify
  src/fake.rs         # #[cfg(test)] ONLY — test double, never compiled into any binary
```

**Binary-size delta (to be measured at implementation):** build `supercli` release with and without `--features device`, report both sizes. Expected delta: < 50 KB (pure `std::process` wrappers, no new mandatory crates). The number goes in this doc after measurement — do not ship without it.

**`fake.rs` is a test double, full stop.** `#[cfg(test)]` only — it is never a runtime path and can never report success to a user. If a fake backend is ever needed outside tests, it must be a separate explicit `--dry-run` flag, not this module.

## 11. gpuidart widgets needed (for Amein)

Already added to `docs/gpuidart-requirements.md` as P0-6/P0-7:

**P0-6. GPU texture / video surface.** Displays a decoded video frame (WebCodecs on web; native decoder on desktop) with pointer and key events — this is the live device screen.

**P0-7. Device-frame container.** Rounded-rect frame with notch/status-bar styling so the panel reads as a device. Multi-device grid is a layout of several P0-7 frames.

**Note for P0-7/future:** the a11y tree from `describe-ui` could later be rendered as an element-outline overlay; not a new widget, just data the app draws.
