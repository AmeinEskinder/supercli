import SwiftUI

@main
struct SupercliIOSApp: App {
    // Bridges the remote-notification callbacks (which SwiftUI's App can't
    // receive) into PushManager.
    @UIApplicationDelegateAdaptor(PushAppDelegate.self) private var pushDelegate

    var body: some Scene {
        WindowGroup {
            SupercliIOSRootView()
        }
    }
}
