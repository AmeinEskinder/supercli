# Apple setup for the Dioxus mobile launchers

This covers everything needed to get `clients/dioxus/unpeel-mobile` onto a
real iPhone, from a Rust checkout to TestFlight. The Rust side is done and
verified; every step below needs a Mac with Xcode (the Linux VM cannot do
any of it).

## 1. Rust Apple targets (done)

- `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-apple-darwin`
  installed via rustup.
- `unpeel-client`'s full Apple-target check needs the Apple SDK (it pulls
  `ring` through rustls/tungstenite/ureq), so those checks run on macOS
  only — `apple.yml`'s `rust-apple-checks` job.
- `unpeel-ios-bridge` is ring-free and builds on Linux too:
  `cargo build -p unpeel-ios-bridge --target aarch64-apple-ios` produces
  `libunpeel_ios_bridge.a`.

## 2. Link the objc2 bridge staticlib (Mac, Xcode)

`clients/dioxus/unpeel-ios-bridge/` exposes nine `extern "C"` functions
(speech auth/start/stop, notification auth/register/delegate install,
APNs token/error ingestion, callback install). Link
`libunpeel_ios_bridge.a` into the shell app target, add the header
declarations from `clients/dioxus/native-shell/README.md`, and install
the callbacks before the webview pump starts. One owner per framework:
never run the Rust and Swift speech/notification drivers together.

## 3. Complete the native iOS shell (Mac, Xcode)

`clients/dioxus/native-shell/README.md` is the exact recipe — status:
**nothing there has been compiled on Xcode**. Drop-in pieces:

- `UnpeelPushBridge.swift` — APNs token acquisition + tap handoff
- `UnpeelSpeechBridge.swift` — `SFSpeechRecognizer` / `SpeechAnalyzer`
- `UnpeelReflectBridge.swift` — FoundationModels reflection
- `Unpeel.entitlements` — Push Notifications capability
- `InfoPlistAdditions.plist` — background modes, mic/speech usage strings

Minimum deployment iOS 17; iOS 26 unlocks `SpeechAnalyzer` + FoundationModels.

## 4. Signing & TestFlight (operator)

`clients/dioxus/fastlane/Fastfile` has the lanes. All secrets are
environment variables — nothing is committed:

| Variable | What it is |
| --- | --- |
| `UNPEEL_APPLE_ID` | Apple ID for the Developer account |
| `UNPEEL_TEAM_ID` | 10-char Developer Team ID |
| `UNPEEL_APP_IDENTIFIER` | Bundle ID, e.g. `com.unpeel.controller` |
| `APP_STORE_CONNECT_API_KEY_ID` | App Store Connect API key ID |
| `APP_STORE_CONNECT_API_ISSUER_ID` | Issuer ID |
| `APP_STORE_CONNECT_API_KEY_BASE64` | Base64 of the .p8 key |
| `UNPEEL_PROVISIONING_PROFILE_NAME` | Optional; defaults to `match AppStore <bundle id>` |

Lanes:

- `fastlane ios simulator` — unsigned simulator `.app` for MobAI tests.
- `fastlane ios testflight` — release staticlib → `dx bundle` → Xcode
  archive → TestFlight (internal testers; no external distribution).

The existing upstream Swift iOS app uses `UNPEEL_DEVELOPMENT_TEAM` from
`clients/ios/UnpeelIOS/project.yml`; keep the two apps' bundle IDs and
provisioning separate.

## 5. Device testing

Once a simulator `.app` exists, `apple.yml`'s `mobai-device-tests` job
boots a simulator, installs the app, and runs `tests/device/` through
`mobai-ci`. See `docs/DEVICE_TESTING.md` for the lanes, the flows, and
what's Pro-only.

## Operator input checklist

- [ ] Apple Developer Program membership (paid, for TestFlight/device)
- [ ] Team ID + bundle ID reserved in App Store Connect
- [ ] App Store Connect API key (Key ID, Issuer ID, .p8)
- [ ] APNs key if push is enabled (the Rust side uploads the token the
      shell acquires; `PUSH_ENVIRONMENT` must match the entitlement:
      `sandbox` for dev/TestFlight, `production` for App Store)
- [ ] MobAI account + API key + Pro plan — only for cloud device farms
      or BYOD; the free simulator/emulator lanes need none of this
- [ ] A Mac with Xcode for every step in sections 2–4
