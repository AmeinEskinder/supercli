// UnpeelPushBridge.swift
//
// Drop-in APNs half of the Dioxus push contract. STATUS: unverified on
// Xcode — written from the Swift client's PushManager behavior, never
// compiled (no Mac on this VM). Verify before shipping.
//
// What this replaces
// -------------------
// In the Swift client, PushManager owned APNs registration, the token hex,
// upload, and the notification-tap → session handoff. In the Dioxus build
// the Rust side owns ALL of that (unpeel-ui PushManager + the mobile
// launcher's upload pump); the native shell only ferries the two OS events
// it alone can see — the device token and the notification tap — into the
// webview through the `window.__unpeelPush` entry point the launcher
// installs (see unpeel-ui PUSH_BRIDGE_JS).
//
// Install
// -------
// 1. Add this file to the native shell target.
// 2. Capabilities: Signing & Capabilities → + Push Notifications (writes
//    aps-environment into Unpeel.entitlements; use `development` for
//    TestFlight/debug so it matches PUSH_ENVIRONMENT=sandbox).
// 3. Info.plist: merge InfoPlistAdditions.plist (UIBackgroundModes
//    remote-notification).
// 4. AppDelegate (or SceneDelegate owner):
//
//        let pushBridge = UnpeelPushBridge()
//
//        func application(_ application: UIApplication,
//                         didFinishLaunchingWithOptions launchOptions: ...) -> Bool {
//            pushBridge.attach(to: webView)          // the Dioxus WKWebView
//            pushBridge.registerForPush()
//            UNUserNotificationCenter.current().delegate = pushBridge
//            return true
//        }
//
//        func application(_ application: UIApplication,
//                         didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
//            pushBridge.didRegister(deviceToken: deviceToken)
//        }
//
//        func application(_ application: UIApplication,
//                         didFailToRegisterForRemoteNotificationsWithError error: Error) {
//            pushBridge.didFail(error: error)
//        }
//
// 5. The Host's push payload must carry the session id under the
//    `sessionId` key (the Swift PushManager contract).

import UIKit
import UserNotifications
import WebKit

/// Ferries APNs events into the Dioxus webview. All state (token hex,
/// upload, retry, tap → session) lives in Rust; this class only formats
/// and forwards.
final class UnpeelPushBridge: NSObject, UNUserNotificationCenterDelegate {
    private weak var webView: WKWebView?

    func attach(to webView: WKWebView) {
        self.webView = webView
    }

    /// Request authorization and register. Call on every launch — iOS does
    /// not cache the token, and registerForRemoteNotifications must be
    /// issued from the main thread.
    func registerForPush() {
        UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound, .badge]) { _, _ in }
        DispatchQueue.main.async {
            UIApplication.shared.registerForRemoteNotifications()
        }
    }

    /// AppDelegate → here. Hands the hex token to the launcher's
    /// `push:token:` pump (which validates, dedups, and uploads it).
    func didRegister(deviceToken: Data) {
        let hex = deviceToken.map { String(format: "%02x", $0) }.joined()
        // Hex is [0-9a-f]; plain interpolation is safe.
        send("window.__unpeelPush('token:' + '\(hex)')")
    }

    /// AppDelegate → here. Simulator / denied-permission / no-entitlement
    /// paths land here; the launcher shows the failure in its
    /// notifications row and offers a retry — never fatal.
    func didFail(error: Error) {
        let message = (error as NSError).localizedDescription
        guard let json = Self.jsonLiteral(message) else { return }
        send("window.__unpeelPush('error:' + \(json))")
    }

    // MARK: - UNUserNotificationCenterDelegate

    /// Tap → open the session named in the payload's `sessionId`, exactly
    /// like the Swift client's PushManager.
    nonisolated func userNotificationCenter(
        _: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        let sessionID =
            response.notification.request.content.userInfo["sessionId"] as? String
        completionHandler()
        if let sessionID, let json = Self.jsonLiteral(sessionID) {
            // Delivered on the main actor; the launcher's pump routes
            // `push:open:` into PushManager.did_open_notification.
            Task { @MainActor [weak self] in
                self?.send("window.__unpeelPush('open:' + \(json))")
            }
        }
    }

    // MARK: - Private

    private func send(_ js: String) {
        DispatchQueue.main.async { [weak self] in
            self?.webView?.evaluateJavaScript(js, completionHandler: nil)
        }
    }

    /// A Swift string as a JS string literal (JSON string encoding).
    /// Static so the nonisolated delegate callback can use it.
    private static func jsonLiteral(_ s: String) -> String? {
        guard let data = try? JSONSerialization.data(withJSONObject: [s]),
            var j = String(data: data, encoding: .utf8),
            j.count >= 2
        else { return nil }
        j.removeFirst()
        j.removeLast()
        return j
    }
}
