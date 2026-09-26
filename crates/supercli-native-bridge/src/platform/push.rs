//! iOS push notification registration.
//!
//! Rust replacement for `PushManager.swift` (iOS). The Swift original
//! registers for remote notifications with `UIApplication`, receives the
//! APNs device token, and forwards it to the Host for phone push delivery.
//!
//! This module is iOS-only: on macOS it compiles to a stub (push over APNs
//! is an iOS concern; macOS uses [`crate::platform::notifications`]), and on
//! non-Apple targets it is an unsupported-platform stub.

use super::PlatformError;

/// APNs environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushEnvironment {
    Development,
    Production,
}

/// A registered APNs device token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceToken {
    /// Hex-encoded token bytes.
    pub hex: String,
    pub environment: PushEnvironment,
}

/// Events from the push registration lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushEvent {
    Registered(DeviceToken),
    RegistrationFailed(String),
    /// A remote notification arrived while the app was running.
    ReceivedRemote {
        payload: Vec<(String, String)>,
    },
}

/// Receives push lifecycle events. Must be `Send`; events may arrive on a
/// platform callback thread.
pub trait PushEventHandler: Send + Sync {
    fn on_push_event(&self, event: PushEvent);
}

/// iOS push manager. Construct with [`PushManager::new`].
pub struct PushManager {
    inner: Inner,
}

enum Inner {
    #[cfg(target_os = "ios")]
    Native(native::NativePushManager),
    #[cfg(not(target_os = "ios"))]
    Stub,
}

impl PushManager {
    pub fn new() -> Self {
        Self {
            inner: {
                #[cfg(target_os = "ios")]
                {
                    Inner::Native(native::NativePushManager::new())
                }
                #[cfg(not(target_os = "ios"))]
                {
                    Inner::Stub
                }
            },
        }
    }

    /// Register for remote notifications. The APNs token (or failure)
    /// arrives via the [`PushEventHandler`].
    pub fn register(&self) -> Result<(), PlatformError> {
        match &self.inner {
            #[cfg(target_os = "ios")]
            Inner::Native(m) => m.register(),
            #[cfg(not(target_os = "ios"))]
            Inner::Stub => Err(PlatformError::UnsupportedPlatform("push registration")),
        }
    }

    /// Install the event handler. Replaces any previous handler.
    pub fn set_handler(&self, handler: Box<dyn PushEventHandler>) {
        match &self.inner {
            #[cfg(target_os = "ios")]
            Inner::Native(m) => m.set_handler(handler),
            #[cfg(not(target_os = "ios"))]
            Inner::Stub => {
                let _ = handler;
            }
        }
    }
}

impl Default for PushManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "ios")]
mod native {
    //! objc2 bindings for iOS push registration (`UIApplication`).
    //!
    //! `PushManager.swift` implements `UIApplicationDelegate` callbacks
    //! `didRegisterForRemoteNotificationsWithDeviceToken` /
    //! `didFailToRegisterForRemoteNotificationsWithError`. A full Rust
    //! `UIApplicationDelegate` is out of scope for this slice — the app
    //! delegate owns those callbacks and forwards them here via
    //! [`NativePushManager::did_receive_token`] /
    //! [`NativePushManager::did_fail`]. [`NativePushManager::register`]
    //! triggers the system prompt through objc2.

    use super::{DeviceToken, PushEnvironment, PushEvent, PushEventHandler};
    use crate::platform::PlatformError;
    use objc2::rc::Retained;
    use objc2_foundation::NSData;
    use objc2_ui_kit::UIApplication;
    use std::sync::Mutex;

    pub(super) struct NativePushManager {
        handler: Mutex<Option<Box<dyn PushEventHandler>>>,
    }

    impl NativePushManager {
        pub(super) fn new() -> Self {
            Self {
                handler: Mutex::new(None),
            }
        }

        pub(super) fn register(&self) -> Result<(), PlatformError> {
            // Mirrors PushManager.swift: ask UIKit to start APNs registration.
            // Must be called on the main thread.
            let app = UIApplication::sharedApplication();
            app.registerForRemoteNotifications();
            Ok(())
        }

        pub(super) fn set_handler(&self, handler: Box<dyn PushEventHandler>) {
            *self.handler.lock().unwrap() = Some(handler);
        }

        /// Forward from the app delegate's
        /// `didRegisterForRemoteNotificationsWithDeviceToken`.
        pub fn did_receive_token(&self, token: &NSData, environment: PushEnvironment) {
            let hex: String = token.bytes().iter().map(|b| format!("{b:02x}")).collect();
            self.emit(PushEvent::Registered(DeviceToken { hex, environment }));
        }

        /// Forward from the app delegate's
        /// `didFailToRegisterForRemoteNotificationsWithError`.
        pub fn did_fail(&self, message: String) {
            self.emit(PushEvent::RegistrationFailed(message));
        }

        fn emit(&self, event: PushEvent) {
            if let Some(handler) = self.handler.lock().unwrap().as_ref() {
                handler.on_push_event(event);
            }
        }

        #[allow(dead_code)]
        fn _keep_nsdata_link(&self, token: Retained<NSData>) {
            let _ = token.length();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_token_fields() {
        let token = DeviceToken {
            hex: "ab12".into(),
            environment: PushEnvironment::Development,
        };
        assert_eq!(token.hex, "ab12");
    }

    /// Non-iOS targets report UnsupportedPlatform.
    #[cfg(not(target_os = "ios"))]
    #[test]
    fn stub_reports_unsupported() {
        let manager = PushManager::new();
        assert_eq!(
            manager.register(),
            Err(PlatformError::UnsupportedPlatform("push registration"))
        );
        struct Noop;
        impl PushEventHandler for Noop {
            fn on_push_event(&self, _event: PushEvent) {}
        }
        manager.set_handler(Box::new(Noop));
    }
}
