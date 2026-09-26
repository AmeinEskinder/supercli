//! User notifications via `UNUserNotificationCenter`.
//!
//! Rust replacement for `DesktopNotifier.swift` (macOS). The Swift original
//! posts Notification Center banners for session triggers (needs input,
//! opted-in completion, informational alerts) and routes banner taps back to
//! session selection via callbacks.
//!
//! The public surface here is platform-agnostic: [`Notifier`] posts a
//! [`Notification`] and reports taps through [`NotificationTapHandler`].
//! On Apple targets this is backed by `UNUserNotificationCenter`; elsewhere
//! it is a stub.

use super::PlatformError;
use std::fmt;

/// A notification to post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// Stable identifier; re-posting the same id replaces the banner.
    pub id: String,
    /// Banner title.
    pub title: String,
    /// Banner body text.
    pub body: String,
    /// Opaque payload returned on tap (e.g. a session id).
    pub user_info: Vec<(String, String)>,
    /// Play the default sound.
    pub sound: bool,
}

impl Notification {
    /// Build a session-attention notification (mirrors `DesktopNotifier`'s
    /// "needs input" / "done" banners).
    pub fn session_attention(
        session_id: &str,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            id: format!("supercli.session.{session_id}"),
            title: title.into(),
            body: body.into(),
            user_info: vec![("sessionID".to_owned(), session_id.to_owned())],
            sound: true,
        }
    }
}

/// What the user did with a delivered notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationTap {
    /// Default tap on the banner body.
    Default { notification_id: String },
    /// Tap on an action button.
    Action {
        notification_id: String,
        action_id: String,
    },
    /// Banner dismissed without interaction.
    Dismissed { notification_id: String },
}

impl fmt::Display for NotificationTap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default { notification_id } => {
                write!(f, "tapped notification {notification_id}")
            }
            Self::Action {
                notification_id,
                action_id,
            } => write!(
                f,
                "tapped action {action_id} on notification {notification_id}"
            ),
            Self::Dismissed { notification_id } => {
                write!(f, "dismissed notification {notification_id}")
            }
        }
    }
}

/// Receives notification interaction callbacks. Implementors must be `Send`;
/// taps may arrive on a platform callback thread.
pub trait NotificationTapHandler: Send + Sync {
    fn on_tap(&self, tap: NotificationTap);
}

/// Authorization status for notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationAuthorization {
    NotDetermined,
    Denied,
    Authorized,
    /// e.g. provisional / ephemeral authorization.
    Partial,
}

/// Platform notifier. Construct with [`Notifier::new`].
pub struct Notifier {
    inner: Inner,
}

enum Inner {
    #[cfg(target_vendor = "apple")]
    Native(native::NativeNotifier),
    #[cfg(not(target_vendor = "apple"))]
    Stub,
}

impl Notifier {
    /// Create the platform notifier.
    pub fn new() -> Self {
        Self {
            inner: {
                #[cfg(target_vendor = "apple")]
                {
                    Inner::Native(native::NativeNotifier::new())
                }
                #[cfg(not(target_vendor = "apple"))]
                {
                    Inner::Stub
                }
            },
        }
    }

    /// Request banner/sound authorization. Safe to call repeatedly; the
    /// system prompt appears at most once.
    pub fn request_authorization(&self) -> Result<NotificationAuthorization, PlatformError> {
        match &self.inner {
            #[cfg(target_vendor = "apple")]
            Inner::Native(n) => n.request_authorization(),
            #[cfg(not(target_vendor = "apple"))]
            Inner::Stub => Err(PlatformError::UnsupportedPlatform("notifications")),
        }
    }

    /// Post (or replace) a notification.
    pub fn post(&self, notification: &Notification) -> Result<(), PlatformError> {
        match &self.inner {
            #[cfg(target_vendor = "apple")]
            Inner::Native(n) => n.post(notification),
            #[cfg(not(target_vendor = "apple"))]
            Inner::Stub => {
                let _ = notification;
                Err(PlatformError::UnsupportedPlatform("notifications"))
            }
        }
    }

    /// Remove a delivered/posted notification by id.
    pub fn remove(&self, notification_id: &str) -> Result<(), PlatformError> {
        match &self.inner {
            #[cfg(target_vendor = "apple")]
            Inner::Native(n) => n.remove(notification_id),
            #[cfg(not(target_vendor = "apple"))]
            Inner::Stub => {
                let _ = notification_id;
                Err(PlatformError::UnsupportedPlatform("notifications"))
            }
        }
    }

    /// Install the tap handler. Replaces any previous handler.
    pub fn set_tap_handler(&self, handler: Box<dyn NotificationTapHandler>) {
        match &self.inner {
            #[cfg(target_vendor = "apple")]
            Inner::Native(n) => n.set_tap_handler(handler),
            #[cfg(not(target_vendor = "apple"))]
            Inner::Stub => {
                let _ = handler;
            }
        }
    }
}

impl Default for Notifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_vendor = "apple")]
mod native {
    //! objc2 bindings for `UNUserNotificationCenter`.
    //!
    //! `DesktopNotifier.swift` sets itself as the `UNUserNotificationCenter`
    //! delegate to route taps. Doing the same from Rust requires a delegate
    //! class; the tap handler indirection keeps the public API free of
    //! Objective-C types.

    use super::{Notification, NotificationAuthorization, NotificationTap, NotificationTapHandler};
    use crate::platform::PlatformError;
    use block2::Block;
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{define_class, msg_send};
    use objc2_foundation::{NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent,
        UNNotificationRequest, UNNotificationSound, UNUserNotificationCenter,
    };
    use std::sync::Mutex;

    fn ns(s: &str) -> Retained<NSString> {
        NSString::from_str(s)
    }

    fn platform_err(context: &str, err: Option<&NSError>) -> PlatformError {
        let detail = err
            .map(|e| e.localizedDescription().to_string())
            .unwrap_or_else(|| "unknown error".to_owned());
        PlatformError::Platform(format!("{context}: {detail}"))
    }

    fn map_status(status: UNAuthorizationStatus) -> NotificationAuthorization {
        // UNAuthorizationStatus is an NSInteger enum; compare raw values to
        // stay robust across SDK versions.
        match status.0 {
            0 => NotificationAuthorization::NotDetermined, // NotDetermined
            1 => NotificationAuthorization::Denied,        // Denied
            2 => NotificationAuthorization::Authorized,    // Authorized
            _ => NotificationAuthorization::Partial,       // Provisional / Ephemeral / CarPlay
        }
    }

    struct DelegateIvars {
        handler: Mutex<Option<Box<dyn NotificationTapHandler>>>,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[ivars = DelegateIvars]
        struct TapDelegate;

        /// SAFETY: TapDelegate implements the delegate methods below.
        unsafe impl objc2_user_notifications::UNUserNotificationCenterDelegate for TapDelegate {}

        impl TapDelegate {
            /// UNUserNotificationCenterDelegate callback. The completion
            /// handler is a `void (^)(void)` block; invoke it via block2.
            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            fn did_receive_response(
                &self,
                _center: &UNUserNotificationCenter,
                response: &ProtocolObject<dyn objc2_user_notifications::UNNotificationResponse>,
                completion: &Block<dyn Fn()>,
            ) {
                let id = response
                    .notification()
                    .request()
                    .identifier()
                    .to_string();
                let tap = NotificationTap::Default {
                    notification_id: id,
                };
                if let Some(handler) = self.ivars().handler.lock().unwrap().as_ref() {
                    handler.on_tap(tap);
                }
                completion.call();
            }
        }
    );

    impl TapDelegate {
        fn new() -> Retained<Self> {
            let this = Self::alloc().set_ivars(DelegateIvars {
                handler: Mutex::new(None),
            });
            unsafe { msg_send![super(this), init] }
        }
    }

    pub(super) struct NativeNotifier {
        delegate: Retained<TapDelegate>,
    }

    impl NativeNotifier {
        pub(super) fn new() -> Self {
            Self {
                delegate: TapDelegate::new(),
            }
        }

        fn center() -> Retained<UNUserNotificationCenter> {
            UNUserNotificationCenter::currentNotificationCenter()
        }

        pub(super) fn request_authorization(
            &self,
        ) -> Result<NotificationAuthorization, PlatformError> {
            // Synchronous wrapper around the async authorization API using a
            // channel, matching DesktopNotifier's "request once, early in
            // launch" behavior.
            use std::sync::mpsc::channel;
            let (tx, rx) = channel::<Result<bool, PlatformError>>();
            let options = UNAuthorizationOptions::Alert
                | UNAuthorizationOptions::Sound
                | UNAuthorizationOptions::Badge;
            let block = Block::new(move |granted: bool, error: *mut NSError| {
                let result = if granted {
                    Ok(true)
                } else if error.is_null() {
                    Ok(false)
                } else {
                    Err(platform_err(
                        "request notification authorization",
                        Some(unsafe { &*error }),
                    ))
                };
                let _ = tx.send(result);
            });
            unsafe {
                Self::center().requestAuthorizationWithOptions_completionHandler(options, &block);
            }
            let granted = rx.recv().map_err(|_| {
                PlatformError::Platform("notification authorization reply lost".to_owned())
            })??;
            if granted {
                Ok(NotificationAuthorization::Authorized)
            } else {
                // Distinguish "denied" from "not determined" via settings.
                self.current_status()
            }
        }

        fn current_status(&self) -> Result<NotificationAuthorization, PlatformError> {
            use objc2_user_notifications::UNNotificationSettings;
            use std::sync::mpsc::channel;
            let (tx, rx) = channel::<UNAuthorizationStatus>();
            let block = Block::new(move |settings: *mut UNNotificationSettings| {
                let status = unsafe { (*settings).authorizationStatus() };
                let _ = tx.send(status);
            });
            unsafe {
                Self::center().getNotificationSettingsWithCompletionHandler(&block);
            }
            let status = rx.recv().map_err(|_| {
                PlatformError::Platform("notification settings reply lost".to_owned())
            })?;
            Ok(map_status(status))
        }

        pub(super) fn post(&self, notification: &Notification) -> Result<(), PlatformError> {
            let content = UNMutableNotificationContent::new();
            content.setTitle(&ns(&notification.title));
            content.setBody(&ns(&notification.body));
            if notification.sound {
                content.setSound(Some(&UNNotificationSound::defaultSound()));
            }
            if !notification.user_info.is_empty() {
                let dict = objc2_foundation::NSMutableDictionary::new();
                for (k, v) in &notification.user_info {
                    dict.setObject_forKey(&ns(v), &ns(k));
                }
                content.setUserInfo(&dict);
            }
            let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
                &ns(&notification.id),
                &content,
                None,
            )
            .ok_or_else(|| {
                PlatformError::Platform("could not build notification request".to_owned())
            })?;
            use std::sync::mpsc::channel;
            let (tx, rx) = channel::<Result<(), PlatformError>>();
            let block = Block::new(move |error: *mut NSError| {
                let result = if error.is_null() {
                    Ok(())
                } else {
                    Err(platform_err("post notification", Some(unsafe { &*error })))
                };
                let _ = tx.send(result);
            });
            unsafe {
                Self::center().addNotificationRequest_withCompletionHandler(&request, &block);
            }
            rx.recv()
                .map_err(|_| PlatformError::Platform("post notification reply lost".to_owned()))?
        }

        pub(super) fn remove(&self, notification_id: &str) -> Result<(), PlatformError> {
            let ids = objc2_foundation::NSArray::from_slice(&[ns(notification_id)]);
            Self::center().removeDeliveredNotificationsWithIdentifiers(&ids);
            Self::center().removePendingNotificationRequestsWithIdentifiers(&ids);
            Ok(())
        }

        pub(super) fn set_tap_handler(&self, handler: Box<dyn NotificationTapHandler>) {
            *self.delegate.ivars().handler.lock().unwrap() = Some(handler);
            let delegate_obj: &ProtocolObject<
                dyn objc2_user_notifications::UNUserNotificationCenterDelegate,
            > = ProtocolObject::from_ref(self.delegate.as_ref());
            Self::center().setDelegate(Some(delegate_obj));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_attention_notification_shape() {
        let n = Notification::session_attention("abc", "Needs input", "Agent is waiting");
        assert_eq!(n.id, "supercli.session.abc");
        assert_eq!(n.title, "Needs input");
        assert!(n.sound);
        assert_eq!(
            n.user_info,
            vec![("sessionID".to_owned(), "abc".to_owned())]
        );
    }

    #[test]
    fn tap_display() {
        let tap = NotificationTap::Default {
            notification_id: "x".into(),
        };
        assert_eq!(format!("{tap}"), "tapped notification x");
    }

    /// On non-Apple targets every operation reports UnsupportedPlatform.
    #[cfg(not(target_vendor = "apple"))]
    #[test]
    fn stub_reports_unsupported() {
        let notifier = Notifier::new();
        let n = Notification::session_attention("s", "t", "b");
        assert_eq!(
            notifier.post(&n),
            Err(PlatformError::UnsupportedPlatform("notifications"))
        );
        assert_eq!(
            notifier.remove("x"),
            Err(PlatformError::UnsupportedPlatform("notifications"))
        );
        assert_eq!(
            notifier.request_authorization(),
            Err(PlatformError::UnsupportedPlatform("notifications"))
        );
        // Setting a handler must not panic on stubs.
        struct Noop;
        impl NotificationTapHandler for Noop {
            fn on_tap(&self, _tap: NotificationTap) {}
        }
        notifier.set_tap_handler(Box::new(Noop));
    }
}
