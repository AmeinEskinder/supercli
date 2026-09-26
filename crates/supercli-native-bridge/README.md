# supercli-native-bridge

Two things live here during the Swift rewrite:

1. **Legacy C ABI** — panic-contained C ABI over `supercli-core` for the
   Swift Mac app (`clients/legacy`). Unchanged; the frozen Swift app links
   this static library.

2. **`src/platform/` — new Rust-native platform glue** (objc2 bindings
   replacing the 9 Swift "platform glue" files from
   `docs/swift-to-rust-parity.md`):
   - `notifications` — `UNUserNotificationCenter` (replaces `DesktopNotifier.swift`)
   - `push` — iOS APNs registration (replaces `PushManager.swift`)
   - `speech` — `SFSpeechRecognizer` (replaces `DictationReflection.swift`,
     `VoiceDictationController.swift`)
   - `menu_bar` — `NSStatusItem` (replaces `MenuBarController.swift`)
   - `file_picker` — `NSOpenPanel`/`NSSavePanel`
   - `updater` — Sparkle-replacement self-update interface
   - `app_lifecycle` — app entry points (replaces `AppDelegate.swift`,
     `UnpeelIOSApp.swift`, `main.swift`)
   - `license` — license key storage, **reusing** `supercli-connector`'s
     keychain (replaces `LicenseKeychain.swift`; no duplicated bindings)

   Platform gating: Apple targets get real objc2 bindings; other targets
   (Linux CI) compile stub implementations returning
   `PlatformError::UnsupportedPlatform`, so the workspace keeps building
   everywhere. macOS-only APIs are `#[cfg(target_os = "macos")]`-gated,
   iOS-only APIs `#[cfg(target_os = "ios")]`-gated.

Keep this crate a thin translation layer: logic belongs in `supercli-core`, and
anything another client needs must live there (one core, many
clients).
