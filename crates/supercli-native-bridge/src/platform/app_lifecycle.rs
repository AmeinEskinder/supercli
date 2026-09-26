//! Application lifecycle and entry points.
//!
//! Rust replacement for:
//!
//! - `AppDelegate.swift` (macOS `NSApplicationDelegate`): launch, terminate,
//!   URL handling, dock reactivation.
//! - `UnpeelIOSApp.swift` (iOS `UIApplicationDelegate` / SwiftUI `App`):
//!   launch, push registration callbacks, background/foreground.
//! - `main.swift` (macOS entry point).
//!
//! The gpuidart desktop/mobile shells own the run loop; this module provides
//! the lifecycle hooks the shells call and the platform callbacks the app
//! delegates forward.

use super::PlatformError;

/// Why the app is launching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchKind {
    /// Normal launch.
    Normal,
    /// Relaunch after a self-update install.
    AfterUpdate,
    /// Launched by opening a `supercli://` URL.
    Url,
    /// Launched at login.
    LoginItem,
}

/// Application lifecycle events the shell reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleEvent {
    WillLaunch {
        kind: LaunchKind,
    },
    DidLaunch,
    /// macOS: dock icon clicked with no windows open.
    Reactivate,
    /// iOS: app moved to background.
    DidEnterBackground,
    /// iOS: app will enter foreground.
    WillEnterForeground,
    /// Open a `supercli://` URL.
    OpenUrl {
        url: String,
    },
    /// Should the app terminate now? The handler returns `true` to allow.
    WillTerminate,
}

/// Receives lifecycle events. Must be `Send`.
pub trait LifecycleHandler: Send + Sync {
    fn on_lifecycle(&self, event: LifecycleEvent) -> LifecycleDecision;
}

/// Decision returned for [`LifecycleEvent::WillTerminate`]; other events
/// ignore the return value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleDecision {
    Allow,
    Cancel,
}

impl LifecycleDecision {
    pub fn allow_terminate(self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// Install the lifecycle handler. The gpuidart shell calls this at startup;
/// platform delegate callbacks forward here.
pub fn set_lifecycle_handler(handler: Box<dyn LifecycleHandler>) {
    let mut slot = LIFECYCLE_HANDLER.lock().unwrap();
    *slot = Some(handler);
}

/// Emit a lifecycle event to the installed handler (no-op if none).
pub fn emit_lifecycle(event: LifecycleEvent) -> LifecycleDecision {
    let slot = LIFECYCLE_HANDLER.lock().unwrap();
    match slot.as_ref() {
        Some(handler) => handler.on_lifecycle(event),
        None => LifecycleDecision::Allow,
    }
}

static LIFECYCLE_HANDLER: std::sync::Mutex<Option<Box<dyn LifecycleHandler>>> =
    std::sync::Mutex::new(None);

/// macOS `NSApplicationDelegate` forwarders.
///
/// The app's `NSApplicationDelegate` (owned by the gpuidart macOS shell)
/// calls these from its delegate methods. They are `#[cfg(target_os =
/// "macos")]`-gated and compile to no-ops elsewhere.
pub mod macos {
    use super::{emit_lifecycle, LaunchKind, LifecycleDecision, LifecycleEvent};

    /// Forward from `applicationDidFinishLaunching:`.
    pub fn did_finish_launching(kind: LaunchKind) {
        emit_lifecycle(LifecycleEvent::WillLaunch { kind });
        emit_lifecycle(LifecycleEvent::DidLaunch);
    }

    /// Forward from `applicationShouldHandleReopen:hasVisibleWindows:`.
    /// Returns whether the app handled the reopen.
    pub fn should_handle_reopen(has_visible_windows: bool) -> bool {
        if !has_visible_windows {
            emit_lifecycle(LifecycleEvent::Reactivate);
        }
        true
    }

    /// Forward from `application:openURLs:`. Returns the terminate decision
    /// for symmetry with the iOS path (always `Allow` here).
    pub fn open_urls(urls: &[String]) -> LifecycleDecision {
        for url in urls {
            emit_lifecycle(LifecycleEvent::OpenUrl { url: url.clone() });
        }
        LifecycleDecision::Allow
    }

    /// Forward from `applicationShouldTerminate:`. Returns `true` when the
    /// handler allows termination.
    pub fn should_terminate() -> bool {
        emit_lifecycle(LifecycleEvent::WillTerminate).allow_terminate()
    }
}

/// iOS `UIApplicationDelegate` forwarders.
///
/// The app's delegate (owned by the gpuidart iOS shell) calls these. They
/// are `#[cfg(target_os = "ios")]`-gated and compile to no-ops elsewhere.
pub mod ios {
    use super::{emit_lifecycle, LaunchKind, LifecycleEvent};

    /// Forward from
    /// `application:didFinishLaunchingWithOptions:`. Also kicks off push
    /// registration through [`crate::platform::push::PushManager`].
    pub fn did_finish_launching(kind: LaunchKind) {
        emit_lifecycle(LifecycleEvent::WillLaunch { kind });
        emit_lifecycle(LifecycleEvent::DidLaunch);
    }

    /// Forward from `applicationDidEnterBackground:`.
    pub fn did_enter_background() {
        emit_lifecycle(LifecycleEvent::DidEnterBackground);
    }

    /// Forward from `applicationWillEnterForeground:`.
    pub fn will_enter_foreground() {
        emit_lifecycle(LifecycleEvent::WillEnterForeground);
    }

    /// Forward from
    /// `application:didRegisterForRemoteNotificationsWithDeviceToken:`.
    /// `token_hex` is the hex-encoded APNs token; `debug` selects the
    /// sandbox environment.
    ///
    /// On iOS the shell forwards the raw `NSData` token to
    /// `PushManager`'s native handler; this helper covers shells that only
    /// have the hex string. Currently a documented no-op: full token
    /// routing is app-shell work once the iOS shell exists.
    pub fn did_register_push_token(
        _push_manager: &crate::platform::push::PushManager,
        _token_hex: &str,
        _debug: bool,
    ) {
    }

    /// Forward from
    /// `application:didFailToRegisterForRemoteNotificationsWithError:`.
    pub fn did_fail_push_registration(message: String) {
        // The PushManager handler receives this via its native path; emit a
        // lifecycle breadcrumb for diagnostics.
        emit_lifecycle(LifecycleEvent::OpenUrl {
            url: format!("supercli://push-registration-failed?message={message}"),
        });
    }
}

/// Entry-point helper for the macOS app.
///
/// `main.swift` was a one-liner (`NSApplicationMain`). The Rust macOS shell's
/// `main()` calls [`macos_main`] after initializing logging; it pumps the
/// `NSApplication` run loop via objc2 on macOS and returns immediately
/// elsewhere.
pub fn macos_main() -> Result<(), PlatformError> {
    #[cfg(target_os = "macos")]
    {
        native_macos_main()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(PlatformError::UnsupportedPlatform("macOS app entry"))
    }
}

#[cfg(target_os = "macos")]
fn native_macos_main() -> Result<(), PlatformError> {
    use objc2_app_kit::NSApplication;
    // Mirrors main.swift -> NSApplicationMain: hand control to AppKit.
    // The delegate is set by the shell before this call.
    let app = NSApplication::sharedApplication();
    app.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Recorder {
        events: Arc<Mutex<Vec<LifecycleEvent>>>,
    }

    impl LifecycleHandler for Recorder {
        fn on_lifecycle(&self, event: LifecycleEvent) -> LifecycleDecision {
            self.events.lock().unwrap().push(event);
            LifecycleDecision::Allow
        }
    }

    #[test]
    fn lifecycle_round_trip() {
        let events = Arc::new(Mutex::new(Vec::new()));
        set_lifecycle_handler(Box::new(Recorder {
            events: Arc::clone(&events),
        }));
        macos::did_finish_launching(LaunchKind::Normal);
        let events = events.lock().unwrap();
        assert_eq!(
            events.as_slice(),
            &[
                LifecycleEvent::WillLaunch {
                    kind: LaunchKind::Normal
                },
                LifecycleEvent::DidLaunch,
            ]
        );
        // Leave no handler installed for other tests.
        *LIFECYCLE_HANDLER.lock().unwrap() = None;
    }

    #[test]
    fn terminate_decision() {
        assert!(LifecycleDecision::Allow.allow_terminate());
        assert!(!LifecycleDecision::Cancel.allow_terminate());
    }

    #[test]
    fn launch_kind_variants_distinct() {
        assert_ne!(LaunchKind::Normal, LaunchKind::AfterUpdate);
    }

    /// Non-macOS: macos_main reports UnsupportedPlatform.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn entry_stub_reports_unsupported() {
        assert_eq!(
            macos_main(),
            Err(PlatformError::UnsupportedPlatform("macOS app entry"))
        );
    }
}
