# Unpeel Dioxus — native iOS shell completion

The Rust side (`unpeel-ui` + the mobile launcher) is complete and verified;
what remains can only be done on a Mac with Xcode, because it needs the iOS
SDK, code signing, and a real device/Simulator. This directory holds
everything the Mac side needs: drop-in sources, capability declarations, and
the exact wiring.

**Status: nothing in this directory has been compiled.** The Swift was
adapted from the working Swift client (`clients/ios/SupercliIOS/Sources/`)
and follows its verified patterns (auth chains, engine guards, fallback
order), but it must be built in Xcode before it ships.

## Files

| File | What it is |
|---|---|
| `UnpeelPushBridge.swift` | APNs token acquisition + notification-tap handoff |
| `UnpeelSpeechBridge.swift` | `SFSpeechRecognizer` / `SpeechAnalyzer` dictation driver |
| `UnpeelReflectBridge.swift` | FoundationModels reflection pass (verbatim copy of the client's reflector) |
| `Unpeel.entitlements` | `aps-environment` (Push Notifications capability) |
| `InfoPlistAdditions.plist` | Merge into the shell target's Info.plist |

## Step-by-step (Xcode)

1. **Create the shell target** (if not done): iOS App, Swift, SwiftUI or
   UIKit lifecycle — either works, the bridges attach to a `WKWebView`.
   Minimum deployment: iOS 17 (legacy recognizer); iOS 26 unlocks
   `SpeechAnalyzer` + FoundationModels.

2. **Add the Swift files** to the target (step 1's target membership
   checkbox). The Dioxus mobile launcher renders into a `WKWebView` —
   point the bridges at that web view.

3. **Capabilities** (target → Signing & Capabilities → + Capability):
   - **Push Notifications** — writes `aps-environment` into the
     entitlements file. Use the `development` value for
     debug/TestFlight builds so it matches the Rust side's
     `PUSH_ENVIRONMENT=sandbox`; Xcode flips it to `production` for
     App Store builds, and the Rust side reads `PUSH_ENVIRONMENT`
     (default `production` in release) to match.
   - **Background Modes** → check **Remote notifications** (lets iOS wake
     the app for pushes).

4. **Info.plist** (target → Info → Custom iOS Target Properties): merge
   every key from `InfoPlistAdditions.plist`:
   - `UIBackgroundModes` / `remote-notification`
   - `NSSpeechRecognitionUsageDescription`
   - `NSMicrophoneUsageDescription`

5. **Wire the app delegate / scene owner** (exact code is in the header
   comments of each Swift file):
   ```swift
   let pushBridge = UnpeelPushBridge()
   let speechBridge = UnpeelSpeechBridge()
   let reflectBridge = UnpeelReflectBridge()

   // after the Dioxus WKWebView exists:
   pushBridge.attach(to: webView)
   speechBridge.attach(to: webView)
   reflectBridge.attach(to: webView)
   speechBridge.reflectBridge = reflectBridge

   // every launch:
   pushBridge.registerForPush()
   UNUserNotificationCenter.current().delegate = pushBridge
   ```
   plus the three `UIApplicationDelegate` callbacks forwarding to
   `pushBridge.didRegister(deviceToken:)` /
   `pushBridge.didFail(error:)` (see `UnpeelPushBridge.swift` header).

6. **Host push payload**: the notification-tap contract needs the session
   id under the `sessionId` key in the push payload — this is the Swift
   client's existing contract, no Host change needed if it already sends it.

## Verification checklist (on the Mac)

- [ ] Shell builds with no errors on the iOS 26 SDK (and still builds with
      the deployment target at iOS 17 — the `#available` / `canImport`
      guards must hold).
- [ ] `xcodebuild -showBuildSettings` shows `aps-environment` =
      `development` for debug, `production` for release.
- [ ] On a real device: allow notifications → the Dioxus pairing screen's
      notifications row flips to registered; the Host receives the token
      upload at `/mobile/push-token`.
- [ ] Deny notifications → the row shows the failure with a retry button
      (never a crash).
- [ ] Tapping a push notification opens the named session in the Dioxus app.
- [ ] Dictation: tap mic → system speech/mic prompts appear once → speak →
      live transcript streams → stop → transcript pastes (or the refined
      version when Apple Intelligence is available).
- [ ] Airplane mode mid-dictation on iOS 17: recognizer dies → the pill
      shows "interrupted" and offers the kept transcript (paste-kept path).
- [ ] Cancel (pill X) mid-reflection: nothing commits, no late answer
      appears.

## What the Rust side already guarantees

- Token hex is validated before upload; only changed tokens re-upload
  (`push_ingest_hex_token`).
- A token arriving before the launcher's pump is stashed at
  `window.__unpeelPushToken` and picked up by the one-time probe.
- Reflection answers are nonce-scoped: a stale/late/double answer can
  never commit over a newer dictation (`complete_refining`).
- Any reflection failure, timeout (4s model cap, 6s launcher backstop),
  or wrong-shaped output commits the verbatim transcript.
- The Web Speech backend is never installed when the native probe hits,
  so the mic is never double-driven.

## Alternative: the Rust staticlib bridge (`../unpeel-ios-bridge/`)

The Swift files above are one way to drive the native frameworks. The
other way is the Rust crate `unpeel-ios-bridge`, which drives
`SFSpeechRecognizer`/`AVAudioEngine` and `UserNotifications` **directly**
through `objc2` and reflects every native callback as a JSON event over a
single `extern "C"` callback. Pick one driver per framework — never both
(the mic and the notification center must each have exactly one owner).

Status: the crate **compiles** — `cargo build -p unpeel-ios-bridge
--target aarch64-apple-ios` succeeds (the real Apple-framework code, not
stubs; the stubs are only for non-Apple hosts). Linking it into the shell
and the runtime behavior are still Xcode-verified-only.

Shell wiring (Xcode):

1. Build the staticlib: from `clients/dioxus`,
   `cargo build --release -p unpeel-ios-bridge --target aarch64-apple-ios`
   (and `--target aarch64-apple-ios-sim` for the Simulator).
2. Add `libunpeel_ios_bridge.a` to the shell target (Link Binary With
   Libraries), plus a bridging header declaring the C surface:
   ```c
   // UnpeelBridge.h
   typedef void (*unpeel_event_cb)(const char *json);
   void unpeel_ios_bridge_set_event_callback(unpeel_event_cb cb);
   void unpeel_speech_request_authorization(void);
   void unpeel_speech_start(void);
   void unpeel_speech_stop(void);
   void unpeel_notifications_request_authorization(void);
   void unpeel_notifications_register_remote(void);   // main thread only
   void unpeel_notifications_install_response_delegate(void);
   void unpeel_apns_ingest_token(const char *hex);    // from app delegate
   void unpeel_apns_ingest_error(const char *message); // from app delegate
   ```
3. At startup (before the web view loads), install the event callback and
   forward each JSON document into the web view — e.g. via
   `webView.evaluateJavaScript("window.__unpeelBridgeEvent('\(json)')")`
   — so the launcher's pump can parse the same `dictation:*` / push event
   shapes it already handles. The pointer is valid only for the call;
   copy it synchronously.
4. Drive dictation from the existing JS probe: `start` on mic tap, `stop`
   on the stop button. Events arrive as `{"kind":"partial"|"final",
   "backend":"speech_recognizer","text":"…"}` — the launcher's existing
   `ntext:` pump shape, as JSON.
5. APNs: the shell's app delegate **still owns token acquisition**
   (iOS delivers it there). Forward it verbatim:
   ```swift
   func application(_ app: UIApplication,
                    didRegisterForRemoteNotificationsWithDeviceToken token: Data) {
       token.map { String(format: "%02x", $0) }.joined()
            .withCString { unpeel_apns_ingest_token($0) }
   }
   func application(_ app: UIApplication,
                    didFailToRegisterForRemoteNotificationsWithError e: Error) {
       e.localizedDescription.withCString { unpeel_apns_ingest_error($0) }
   }
   ```
   The Rust side validates (even-length hex), normalizes, and emits
   `{"kind":"apns_token","token":"…"}`.
6. Notification taps: `unpeel_notifications_install_response_delegate()`
   once at startup; taps arrive as
   `{"kind":"notification_opened","identifier":"…","user_info":{…}}` —
   route `user_info.sessionId` to the session like `did_open_notification`.

Deliberately NOT in the Rust bridge: `SpeechAnalyzer` (iOS 26) — no
`objc2` binding exists yet (`objc2-speech` 0.3.2 is recognizer-only), so
the analyzer path stays Swift-side until the binding ships. The JSON
events don't change when the backend does.
