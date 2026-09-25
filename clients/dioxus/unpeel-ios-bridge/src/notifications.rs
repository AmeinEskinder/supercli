//! UserNotifications authorization + APNs token reflection.
//!
//! Split of responsibilities (matches the established push architecture):
//! - The **Swift shell owns the UIApplicationDelegate** (like Swift's
//!   `PushAppDelegate`): it receives
//!   `didRegisterForRemoteNotificationsWithDeviceToken` /
//!   `didFailToRegisterForRemoteNotificationsWithError` and forwards them
//!   into [`ingest_apns_token`] / [`ingest_apns_error`]. Rust validates,
//!   normalizes, and reflects them as JSON events.
//! - **Rust owns the UserNotifications half directly** through `objc2`:
//!   [`request_authorization`] and the notification-response delegate
//!   installed by [`install_response_delegate`]. A tap on a notification
//!   arrives as `{"kind":"notification_opened",…}` carrying the request
//!   identifier and the `userInfo` payload the Host attached (the launcher
//!   routes it to the session, mirroring `did_open_notification`).
//!
//! `register_for_remote_notifications()` asks UIKit to start the APNs
//! handshake; the resulting token still flows back through the shell.

#[cfg(target_vendor = "apple")]
pub(crate) mod apple {
    use crate::{emit, json_escape};
    use block2::{DynBlock, StackBlock};
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{define_class, extern_methods, msg_send, AnyThread, MainThreadMarker};
    use objc2_foundation::{NSArray, NSDictionary, NSError, NSString};
    use objc2_ui_kit::UIApplication;
    use objc2_user_notifications::{
        UNNotificationResponse, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
    };
    use std::ffi::c_char;
    use std::sync::Mutex;

    /// UNAuthorizationOptions bitmask: alert | badge | sound.
    const AUTH_OPTIONS: usize = 0b111;

    fn nsstring_to_string(s: &NSString) -> String {
        s.to_string()
    }

    /// Best-effort NSDictionary → JSON object. NSString values go through
    /// verbatim; anything else uses its `description` via a direct message
    /// send. Non-string keys are skipped (userInfo keys are strings in
    /// practice).
    fn user_info_json(info: &NSDictionary) -> String {
        let mut out = String::from("{");
        let mut first = true;
        // SAFETY: allKeys/objectAtIndex:/objectForKey: follow their
        // documented contracts, and every NSObject responds to description.
        unsafe {
            let keys: Retained<NSArray<objc2::runtime::AnyObject>> = info.allKeys();
            for i in 0..keys.count() {
                let key_obj = keys.objectAtIndex(i);
                let Some(key) = key_obj.downcast_ref::<NSString>() else {
                    continue;
                };
                let value = info.objectForKey(&key_obj);
                if !first {
                    out.push(',');
                }
                first = false;
                out.push('"');
                json_escape(&mut out, &nsstring_to_string(key));
                out.push_str("\":\"");
                let rendered = value
                    .as_ref()
                    .and_then(|v| v.downcast_ref::<NSString>())
                    .map(nsstring_to_string)
                    .or_else(|| {
                        value.as_ref().map(|v| {
                            let desc: Retained<NSString> = msg_send![v, description];
                            desc.to_string()
                        })
                    })
                    .unwrap_or_default();
                json_escape(&mut out, &rendered);
                out.push('"');
            }
        }
        out.push('}');
        out
    }

    /// Ask the user for notification authorization (alert + badge + sound).
    pub fn request_authorization() {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let block = StackBlock::new(|granted: Bool, err: *mut NSError| {
            let mut j = String::from("{\"kind\":\"notification_authorization\",\"granted\":");
            j.push_str(if granted.as_bool() { "true" } else { "false" });
            if !err.is_null() {
                let desc = unsafe { (*err).localizedDescription().to_string() };
                j.push_str(",\"error\":\"");
                json_escape(&mut j, &desc);
                j.push('"');
            }
            j.push('}');
            emit(&j);
        });
        // The framework copies the block before returning.
        center.requestAuthorizationWithOptions_completionHandler(
            objc2_user_notifications::UNAuthorizationOptions(AUTH_OPTIONS),
            &block,
        );
    }

    /// Start the APNs handshake. The token/error comes back through the
    /// shell's app delegate → [`super::ingest_apns_token`].
    /// Must be called on the main thread.
    pub fn register_for_remote_notifications() {
        let Some(mtm) = MainThreadMarker::new() else {
            emit("{\"kind\":\"error\",\"message\":\"register_for_remote_notifications must run on the main thread\"}");
            return;
        };
        // sharedApplication is main-thread-only; checked above.
        UIApplication::sharedApplication(mtm).registerForRemoteNotifications();
        emit("{\"kind\":\"apns_registration_requested\"}");
    }

    /// Shell → Rust: the APNs device token from the app delegate, as hex.
    /// Validates strictly (even length, hex only), lowercases, and reflects
    /// it as `{"kind":"apns_token","token":"…"}`.
    pub fn ingest_apns_token(hex_ptr: *const c_char) {
        let Some(hex) = cstr_to_string(hex_ptr) else {
            emit("{\"kind\":\"error\",\"message\":\"apns token pointer was null\"}");
            return;
        };
        let clean: String = hex.trim().to_lowercase();
        let valid = !clean.is_empty()
            && clean.len() % 2 == 0
            && clean.bytes().all(|b| b.is_ascii_hexdigit());
        if !valid {
            emit("{\"kind\":\"error\",\"message\":\"apns token was not even-length hex\"}");
            return;
        }
        let mut j = String::from("{\"kind\":\"apns_token\",\"token\":\"");
        j.push_str(&clean);
        j.push_str("\"}");
        emit(&j);
    }

    /// Shell → Rust: APNs registration failed in the app delegate.
    pub fn ingest_apns_error(msg_ptr: *const c_char) {
        let msg = cstr_to_string(msg_ptr).unwrap_or_else(|| "unknown".to_string());
        let mut j = String::from("{\"kind\":\"apns_error\",\"message\":\"");
        json_escape(&mut j, &msg);
        j.push_str("\"}");
        emit(&j);
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "UnpeelNotificationDelegate"]
        struct NotificationDelegate;

        // NSObjectProtocol has default bodies (forwarded via msg_send to the
        // NSObject superclass); the empty impl satisfies the supertrait bound
        // on UNUserNotificationCenterDelegate.
        unsafe impl NSObjectProtocol for NotificationDelegate {}

        unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
            #[allow(non_snake_case)]
            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            unsafe fn userNotificationCenter_didReceiveNotificationResponse_withCompletionHandler(
                &self,
                _center: &UNUserNotificationCenter,
                response: &UNNotificationResponse,
                completion_handler: &DynBlock<dyn Fn()>,
            ) {
                // Accessors follow their documented contracts.
                let request = response.notification().request();
                let identifier = request.identifier().to_string();
                let user_info = user_info_json(&request.content().userInfo());
                let mut j = String::from("{\"kind\":\"notification_opened\",\"identifier\":\"");
                json_escape(&mut j, &identifier);
                j.push_str("\",\"user_info\":");
                j.push_str(&user_info);
                j.push('}');
                emit(&j);
                completion_handler.call(());
            }
        }
    );

    impl NotificationDelegate {
        extern_methods!(
            #[unsafe(method(init))]
            #[unsafe(method_family = init)]
            unsafe fn init(this: Allocated<Self>) -> Retained<Self>;
        );
    }

    /// Process-lifetime delegate pointer. The notification center does not
    /// retain its delegate, so Rust leaks it; a raw pointer is the honest
    /// way to keep it alive.
    #[allow(dead_code)] // stored, never read back: the leak is the point
    struct DelegatePtr(*const NotificationDelegate);
    unsafe impl Send for DelegatePtr {}
    unsafe impl Sync for DelegatePtr {}

    static DELEGATE_PTR: Mutex<Option<DelegatePtr>> = Mutex::new(None);

    /// Install the response delegate on the current notification center.
    pub fn install_response_delegate() {
        let delegate: Retained<NotificationDelegate> =
            unsafe { NotificationDelegate::init(NotificationDelegate::alloc()) };
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let proto: Retained<ProtocolObject<dyn UNUserNotificationCenterDelegate>> =
            ProtocolObject::from_retained(delegate.clone());
        center.setDelegate(Some(&proto));
        // SAFETY: leaked for the process lifetime; the pointer stays valid.
        let ptr: *const NotificationDelegate = Retained::as_ptr(&delegate);
        std::mem::forget(delegate);
        *DELEGATE_PTR.lock().unwrap() = Some(DelegatePtr(ptr));
        emit("{\"kind\":\"notification_delegate_installed\"}");
    }

    pub fn cstr_to_string(ptr: *const c_char) -> Option<String> {
        if ptr.is_null() {
            return None;
        }
        // SAFETY: contract says NUL-terminated UTF-8 from the shell.
        unsafe {
            std::ffi::CStr::from_ptr(ptr)
                .to_str()
                .ok()
                .map(|s| s.to_string())
        }
    }
}

#[cfg(not(target_vendor = "apple"))]
pub(crate) mod apple {
    use crate::emit;
    use std::ffi::c_char;
    /// Inert stubs: identical signatures, no framework calls.
    pub fn request_authorization() {
        emit("{\"kind\":\"error\",\"message\":\"notifications unavailable: not an Apple target\"}");
    }
    pub fn register_for_remote_notifications() {
        emit("{\"kind\":\"error\",\"message\":\"notifications unavailable: not an Apple target\"}");
    }
    pub fn ingest_apns_token(_hex: *const c_char) {
        emit("{\"kind\":\"error\",\"message\":\"notifications unavailable: not an Apple target\"}");
    }
    pub fn ingest_apns_error(_msg: *const c_char) {
        emit("{\"kind\":\"error\",\"message\":\"notifications unavailable: not an Apple target\"}");
    }
    pub fn install_response_delegate() {
        emit("{\"kind\":\"error\",\"message\":\"notifications unavailable: not an Apple target\"}");
    }
}

pub use apple::{
    ingest_apns_error, ingest_apns_token, install_response_delegate,
    register_for_remote_notifications, request_authorization,
};
