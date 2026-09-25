# Device testing with MobAI

How the Dioxus mobile launchers (`clients/dioxus/unpeel-mobile`) get
exercised on real devices and simulators. MobAI is the device-control
layer; it does **not** replace Xcode, signing, or the Android SDK — those
still produce the app artifact MobAI installs.

## What MobAI is

- CLI: `@mobai-app/cli` (npm, free). Thin client over the MobAI desktop
  app's local HTTP API (`http://127.0.0.1:8686`).
- CI harness: `mobai-ci` (`MobAI-App/mobai-ci@v1` GitHub Action, or
  `curl -fsSL https://mobai.run/ci/install.sh | sh` anywhere).
- Desktop app: the GUI bridge that talks to attached devices. Requires a
  display + GTK/WebKit on Linux; on macOS it drives the iOS simulator.
- Test scripts: `.mob` files (see `tests/device/`) or Maestro YAML.
  `mobai-ci validate <dir>` checks syntax; `mobai-ci test <dir>`
  runs them and emits JUnit plus a screenshot + UI tree per failed step.

## Lanes

| Lane | Runner | Cost | What runs it |
| --- | --- | --- | --- |
| iOS simulator | `macos-15` or newer | free | `apple.yml` → `mobai-device-tests` job |
| Android emulator | `ubuntu-latest` | free | `mobai-ci` `boot-emu` (enables KVM) |
| USB device | self-hosted macOS/Linux | free | operator's machine, device on USB |
| Cloud device farm (BrowserStack/Sauce/AWS) | any | **Pro** | `MOBAI_API_KEY` secret |
| BYOD (own device over a tunnel) | any | **Pro** | `MOBAI_API_KEY` secret + tunnel |

Remote/BYOD/cloud runs (any run against a device that isn't on the
runner) need a Pro plan and a MobAI account key. The free lanes cover
everything in CI.

## The smoke flows (`tests/device/`)

Five `.mob` flows, in dependency order:

1. `pair.mob` — fresh install → "Pair with an Unpeel Host" → type the
   pairing code (param `pair_code`) → tap "Pair" → session list appears.
2. `open-session.mob` — tap a session row (`session_title` param) →
   session detail with "‹ Sessions" back affordance.
3. `terminal-type.mob` — focus the live terminal, type a marker command,
   assert the Host's PTY echo comes back.
4. `gallery-annotate.mob` — open the session gallery ("Gallery" →
   "Browser Gallery"), take a "Screenshot" artifact, open the first
   entry, "Draw" a stroke, "Done" → the editor flattens the annotation
   at native resolution and uploads a new entry.
5. `dictation-toggle.mob` — "Dictate" → "Listening" → "Stop dictation".
   Control-plane only: real transcription needs a mic and the iOS shell
   speech bridge (see `docs/APPLE_SETUP.md`).

Run them locally (macOS, simulator booted):

```sh
mobai-ci validate ./tests/device
mobai-ci test ./tests/device --output device-reports
```

Parameters are passed per flow, e.g.:

```sh
mobai-ci test ./tests/device/pair.mob --param pair_code="UNPEEL:1:…" --output device-reports
```

## What the flows assert — and what they don't

- The flows drive the **real UI copy** ("Pair with an Unpeel Host",
  "UNPEEL:1:host:port:…", "Browser Gallery", "Draw", "Dictate"). If the
  UI copy changes, the flows must change with it.
- The gallery and dictation flows need a **live paired Host**: screenshots
  and dictation go through the Host protocol. CI runs them against a
  local Host started on the runner (see `apple.yml`).
- The `.mob` scripts are tuned for the Dioxus mobile launcher's current
  copy; they are smoke coverage, not a full UI suite.

## Local Linux evaluation (2026-09-22)

Recorded on this VM for the record:

- `@mobai-app/cli` 2.7.2 (npm) installs and runs; `mobai version` OK.
- `mobai-ci` 0.6.0 (linux amd64) installs; `validate ./tests/device`
  passes 5/5.
- The desktop app (v3.1.0 .deb, GTK GUI) **cannot run here**: it needs
  `libwebkit2gtk-4.1-0` / `libsoup-3.0` / `libjavascriptcoregtk-4.1-0`,
  which are not installed and cannot be fetched (apt is unreliable
  behind the proxy). So there is no local bridge at `127.0.0.1:8686`
  and `mobai devices list` fails with connection refused.
- No attached devices; no KVM (`/dev/kvm` absent, no vmx/svm flags) so
  no local Android emulator either.
- Cloud/BYOD lanes intentionally untested: no account, no API key.

### Android target investigation (2026-09-22)

Probed whether the Dioxus mobile client can target Android from this VM:

- Rust targets `aarch64-linux-android` + `x86_64-linux-android` installed.
- Android SDK provisioned manually under `~/workspace/muse-harness/tmp/android-sdk`
  (proxy blocks `sdkmanager`, so components were curled directly):
  platform-tools 37.0.1, build-tools 35.0.0, android-35 platform, NDK r27d.
- `dx bundle --platform android --package unpeel-mobile` **compiles the full
  Rust workspace for Android**: 504/505 crates built, producing a valid
  `libmain.so` (ELF x86-64 PIE, `/system/bin/linker64`).
- The investigation caught a real manifest bug: `unpeel-mobile` and
  `unpeel-desktop` called `dioxus::launch(...)` but only enabled the
  `mobile`/`desktop` features — `launch` is a separate feature in Dioxus 0.7
  (unpeel-web already had it). Fixed by adding `"launch"` to both crates'
  feature lists.
- APK assembly is **environmentally blocked**: the Gradle wrapper's
  distribution download fails (proxy), and even with Gradle 9.1.0 pre-seeded
  the Gradle daemon cannot complete its client handshake in this sandbox
  (daemon starts, binds, then expires; client reads garbage on the control
  socket). This is a sandbox networking quirk, not a project defect.
- Running is blocked regardless: no KVM, so no emulator; no attached device.

Bottom line: the Rust code is Android-viable today; producing and running an
APK needs a real machine (or CI with working Gradle + KVM).

Bottom line: on Linux this VM is a flow-authoring and flow-validation
machine only. Real device/simulator execution happens on macOS CI
(`apple.yml`) or an operator workstation.
