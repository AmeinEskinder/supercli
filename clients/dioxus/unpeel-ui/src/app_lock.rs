//! Optional biometric app lock, ported from iOS `AppLock.swift`.
//!
//! When armed, the app covers itself with [`AppLockOverlay`] on every
//! backgrounding and on cold launch, and clears it only after a successful
//! authentication. This is a UI shield for shoulder-surfing / handed-phone
//! scenarios — the pairing token already lives in the keychain; nothing here
//! re-encrypts data at rest.
//!
//! The actual biometric prompt is a **native-shell capability**
//! (LocalAuthentication on iOS): Rust cannot present it from the webview.
//! Everything around it — the full state machine, the overlay, the launcher
//! wiring — is ported and tested here. The shell plugs in through
//! [`BiometricBackend`]:
//!
//! - Plain webview / dev builds use [`ShellBiometricBackend`] **without**
//!   the `shell-biometric` cargo feature: `capability()` reports
//!   unavailable, so the Security toggle stays disabled exactly like Swift's
//!   `.disabled(!capability.available)`.
//! - The iOS shell build enables the `shell-biometric` feature and exports
//!   two C symbols (see `SHELL_CONTRACT`); Rust calls them through
//!   [`ShellBiometricBackend`].
//!
//! # Shell contract (exact Mac-side completion instructions)
//!
//! In the iOS shell target, add a Swift file with:
//!
//! ```swift
//! import LocalAuthentication
//!
//! // 0 = unavailable, 1 = passcode-only, 2 = touchID, 3 = faceID, 4 = opticID.
//! @_cdecl("unpeel_shell_biometric_capability")
//! public func unpeel_shell_biometric_capability() -> Int32 {
//!     let context = LAContext()
//!     var error: NSError?
//!     guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &error) else { return 0 }
//!     switch context.biometryType {
//!     case .touchID: return 2
//!     case .faceID: return 3
//!     case .opticID: return 4
//!     default: return 1
//!     }
//! }
//!
//! // 0 = success, 1 = user cancel, 2 = system/app cancel, 3 = failure.
//! // Called on a Rust background thread — never the main thread — so
//! // blocking on a semaphore here is safe.
//! @_cdecl("unpeel_shell_biometric_authenticate")
//! public func unpeel_shell_biometric_authenticate(_ reason: UnsafePointer<CChar>) -> Int32 {
//!     let reasonString = String(cString: reason)
//!     let context = LAContext()
//!     let sema = DispatchSemaphore(value: 0)
//!     var result: Int32 = 3
//!     context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reasonString) { success, error in
//!         defer { sema.signal() }
//!         guard !success else { result = 0; return }
//!         guard let e = error as? LAError else { return }
//!         switch e.code {
//!         case .userCancel: result = 1
//!         case .systemCancel, .appCancel: result = 2
//!         default: result = 3
//!         }
//!     }
//!     sema.wait()
//!     return result
//! }
//! ```
//!
//! Then build the mobile launcher with `--features shell-biometric`.
//! Without the feature the calls are compiled out and the backend reports
//! unavailable — no link errors in either configuration.

use std::fmt;
use std::sync::Arc;

use crate::i18n::t;
use dioxus::prelude::*;

/// Which biometric (if any) the device offers. Mirrors `LABiometryType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BiometryType {
    #[default]
    None,
    TouchId,
    FaceId,
    OpticId,
}

/// Whether the device can authenticate at all (biometry enrolled OR a
/// passcode set), and which biometry it offers. `biometry == None` with
/// `available == true` means passcode-only — mirroring Swift's
/// `AppLockManager.capability()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppLockCapability {
    pub available: bool,
    pub biometry: BiometryType,
}

impl AppLockCapability {
    pub const UNAVAILABLE: Self = Self {
        available: false,
        biometry: BiometryType::None,
    };
}

/// Outcome of one authentication attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// No authenticator is plugged in (plain webview build).
    Unavailable,
    /// The user dismissed the prompt — silent, like Swift: the overlay's
    /// unlock button re-triggers on demand.
    UserCancel,
    /// The system took the prompt away — silent, like Swift.
    SystemCancel,
    /// Genuine failure; surfaced on the overlay via `last_error`.
    Failed(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::Unavailable => write!(f, "{}", {
                t("app_lock.biometric_unlock_is_not_available_in_thi")
            }),
            AuthError::UserCancel => {
                write!(f, "{}", { t("app_lock.authentication_was_cancelled") })
            }
            AuthError::SystemCancel => {
                write!(f, "{}", { t("app_lock.authentication_was_interrupted") })
            }
            AuthError::Failed(msg) => write!(f, "{msg}"),
        }
    }
}

/// User-facing name of the unlock method on this device. Mirrors Swift's
/// `AppLockManager.methodLabel()`.
pub fn method_label(capability: &AppLockCapability) -> String {
    match capability.biometry {
        BiometryType::FaceId => t("app_lock.face_id"),
        BiometryType::TouchId => t("app_lock.touch_id"),
        BiometryType::OpticId => t("app_lock.optic_id"),
        BiometryType::None => {
            if capability.available {
                {
                    t("app_lock.passcode")
                }
            } else {
                {
                    t("app_lock.face_id")
                }
            }
        }
    }
}

/// Bridge to the native shell's authenticator. Implemented by
/// [`ShellBiometricBackend`] (LocalAuthentication via the C contract
/// above); tests use a scripted fake.
pub trait BiometricBackend: Send + Sync {
    /// Whether authentication is possible and which biometry is offered.
    fn capability(&self) -> AppLockCapability;
    /// Present the system prompt with `reason`. Blocks until the user
    /// answers; cancellations are silent variants, genuine failures carry a
    /// message. Called on a background thread, never the UI thread.
    fn authenticate(&self, reason: &str) -> Result<(), AuthError>;
}

/// Explicitly unavailable backend, for tests and plain dev builds that want
/// the disabled-toggle behavior without the shell feature.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoBiometricBackend;

impl BiometricBackend for NoBiometricBackend {
    fn capability(&self) -> AppLockCapability {
        AppLockCapability::UNAVAILABLE
    }

    fn authenticate(&self, _reason: &str) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
}

#[cfg(feature = "shell-biometric")]
mod shell_ffi {
    use std::os::raw::{c_char, c_int};

    extern "C" {
        pub fn unpeel_shell_biometric_capability() -> c_int;
        pub fn unpeel_shell_biometric_authenticate(reason: *const c_char) -> c_int;
    }
}

/// Backend for shell builds: with the `shell-biometric` feature the two C
/// symbols are imported from the native shell (see the module docs);
/// without the feature it reports unavailable, so the Security toggle stays
/// disabled exactly like Swift's `.disabled(!capability.available)`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShellBiometricBackend;

impl BiometricBackend for ShellBiometricBackend {
    fn capability(&self) -> AppLockCapability {
        #[cfg(feature = "shell-biometric")]
        {
            // SAFETY: the shell contract guarantees a pure, thread-safe
            // query returning 0–4 (module docs).
            let code = unsafe { shell_ffi::unpeel_shell_biometric_capability() };
            return match code {
                1 => AppLockCapability {
                    available: true,
                    biometry: BiometryType::None,
                },
                2 => AppLockCapability {
                    available: true,
                    biometry: BiometryType::TouchId,
                },
                3 => AppLockCapability {
                    available: true,
                    biometry: BiometryType::FaceId,
                },
                4 => AppLockCapability {
                    available: true,
                    biometry: BiometryType::OpticId,
                },
                _ => AppLockCapability::UNAVAILABLE,
            };
        }
        #[cfg(not(feature = "shell-biometric"))]
        {
            AppLockCapability::UNAVAILABLE
        }
    }

    fn authenticate(&self, reason: &str) -> Result<(), AuthError> {
        #[cfg(feature = "shell-biometric")]
        {
            use std::ffi::CString;
            use std::os::raw::c_int;
            let c_reason = CString::new(reason).unwrap_or_default();
            // SAFETY: the shell contract guarantees a thread-safe,
            // semaphore-blocked call returning 0–3 (module docs).
            let code: c_int =
                unsafe { shell_ffi::unpeel_shell_biometric_authenticate(c_reason.as_ptr()) };
            return match code {
                0 => Ok(()),
                1 => Err(AuthError::UserCancel),
                2 => Err(AuthError::SystemCancel),
                _ => Err(AuthError::Failed(
                    { t("app_lock.authentication_failed") }.to_string(),
                )),
            };
        }
        #[cfg(not(feature = "shell-biometric"))]
        {
            let _ = reason;
            Err(AuthError::Unavailable)
        }
    }
}

/// The app-lock state machine, ported from Swift's `AppLockManager`.
///
/// `authenticate` calls block on the backend, so `enable()` / `unlock()`
/// must run on a background thread — exactly like Swift's `Task { await … }`
/// off the main actor. `auth_in_flight` still guards re-entrancy so the
/// overlay button and a foreground auto-prompt can never stack two prompts.
#[derive(Clone)]
pub struct AppLockManager {
    backend: Arc<dyn BiometricBackend>,
    /// Whether the lock is armed. Persisted by the launcher; change via
    /// `enable()` / `disable()`.
    is_enabled: bool,
    /// True while the lock overlay must cover the app.
    is_locked: bool,
    /// True while a system auth prompt is up.
    auth_in_flight: bool,
    /// Last non-cancel auth failure, surfaced on the overlay.
    last_error: Option<String>,
    /// One automatic prompt per foreground: the system sheet itself bounces
    /// the app through hidden → visible, so re-prompting on every visible
    /// would loop after a user cancel.
    auto_prompted_this_foreground: bool,
}

impl AppLockManager {
    /// Cold launch starts covered when armed — mirroring Swift's `init`.
    pub fn new(backend: Arc<dyn BiometricBackend>, enabled: bool) -> Self {
        Self {
            backend,
            is_enabled: enabled,
            is_locked: enabled,
            auth_in_flight: false,
            last_error: None,
            auto_prompted_this_foreground: false,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    pub fn is_locked(&self) -> bool {
        self.is_locked
    }

    pub fn auth_in_flight(&self) -> bool {
        self.auth_in_flight
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn capability(&self) -> AppLockCapability {
        self.backend.capability()
    }

    /// Arm the lock. Authenticates first so the toggle confirms the method
    /// actually works before the next backgrounding depends on it.
    /// Returns `true` only when the lock is now armed.
    pub fn enable(&mut self) -> bool {
        if self.is_enabled {
            return true;
        }
        let reason = format!(
            "Confirm {} to lock Unpeel when you leave the app",
            method_label(&self.capability())
        );
        if !self.authenticate(&reason) {
            return false;
        }
        self.is_enabled = true;
        self.is_locked = false;
        true
    }

    /// Disarm. No auth gate: reaching this toggle already required an unlock.
    pub fn disable(&mut self) {
        self.is_enabled = false;
        self.is_locked = false;
    }

    /// The app went to the background: cover the app and re-arm the one
    /// automatic foreground prompt.
    pub fn lock_if_enabled(&mut self) {
        if !self.is_enabled {
            return;
        }
        self.is_locked = true;
        self.auto_prompted_this_foreground = false;
    }

    /// The app became visible: returns `true` exactly once per foreground
    /// while locked, telling the caller to fire the single automatic unlock
    /// prompt (on a background thread).
    pub fn begin_foreground_unlock(&mut self) -> bool {
        if !self.is_locked || self.auto_prompted_this_foreground {
            return false;
        }
        self.auto_prompted_this_foreground = true;
        true
    }

    /// Clear the lock after a successful auth (overlay button + auto-prompt).
    /// Returns `true` when the app is now unlocked.
    pub fn unlock(&mut self) -> bool {
        if !self.is_locked {
            return true;
        }
        if self.authenticate(&{ t("app_lock.unlock_unpeel") }) {
            self.is_locked = false;
            true
        } else {
            false
        }
    }

    /// One guarded authentication round. Cancellations are silent (the
    /// overlay's unlock button re-triggers on demand); genuine failures land
    /// in `last_error`.
    fn authenticate(&mut self, reason: &str) -> bool {
        if self.auth_in_flight {
            return false;
        }
        self.auth_in_flight = true;
        self.last_error = None;
        let result = self.backend.authenticate(reason);
        self.auth_in_flight = false;
        match result {
            Ok(()) => true,
            Err(AuthError::UserCancel | AuthError::SystemCancel | AuthError::Unavailable) => false,
            Err(failed) => {
                self.last_error = Some(failed.to_string());
                false
            }
        }
    }
}

/// Full-screen cover shown while the app is locked. Opaque on purpose: it is
/// also what the app-switcher snapshot captures, so session titles and
/// terminal content never appear there while the lock is armed.
#[component]
pub fn AppLockOverlay(
    method_label: String,
    last_error: Option<String>,
    auth_in_flight: bool,
    on_unlock: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "app-lock-overlay",
            div { class: "app-lock-card",
                svg {
                    class: "app-lock-icon",
                    view_box: "0 0 24 24",
                    width: "68",
                    height: "68",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.6",
                    rect {
                        x: "4",
                        y: "10",
                        width: "16",
                        height: "10",
                        rx: "2.5",
                    }
                    path { d: "M8 10V7a4 4 0 0 1 8 0v3" }
                    circle { cx: "12", cy: "15", r: "1.4", fill: "currentColor", stroke: "none" }
                }
                div { class: "app-lock-title", {t("app_lock.unpeel_is_locked")} }
                if let Some(error) = last_error {
                    div { class: "app-lock-error", "{error}" }
                }
                button {
                    class: "app-lock-unlock",
                    disabled: auth_in_flight,
                    onclick: move |_| on_unlock.call(()),
                    "Unlock with {method_label}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Scripted backend: each `authenticate` pops the next outcome; records
    /// the reasons it was asked for.
    struct FakeBackend {
        outcomes: Mutex<Vec<Result<(), AuthError>>>,
        reasons: Mutex<Vec<String>>,
        capability: AppLockCapability,
    }

    impl FakeBackend {
        fn succeeding() -> Arc<Self> {
            Arc::new(Self {
                outcomes: Mutex::new(vec![Ok(())]),
                reasons: Mutex::new(vec![]),
                capability: AppLockCapability {
                    available: true,
                    biometry: BiometryType::FaceId,
                },
            })
        }

        fn failing(outcomes: Vec<Result<(), AuthError>>) -> Arc<Self> {
            Arc::new(Self {
                outcomes: Mutex::new(outcomes),
                reasons: Mutex::new(vec![]),
                capability: AppLockCapability {
                    available: true,
                    biometry: BiometryType::FaceId,
                },
            })
        }
    }

    impl BiometricBackend for FakeBackend {
        fn capability(&self) -> AppLockCapability {
            self.capability
        }

        fn authenticate(&self, reason: &str) -> Result<(), AuthError> {
            self.reasons.lock().unwrap().push(reason.to_string());
            self.outcomes
                .lock()
                .unwrap()
                .pop()
                .unwrap_or(Err(AuthError::Failed("out of scripted outcomes".into())))
        }
    }

    #[test]
    fn cold_launch_starts_covered_when_armed() {
        let lock = AppLockManager::new(FakeBackend::succeeding(), true);
        assert!(lock.is_enabled());
        assert!(lock.is_locked());
    }

    #[test]
    fn cold_launch_starts_open_when_disarmed() {
        let lock = AppLockManager::new(FakeBackend::succeeding(), false);
        assert!(!lock.is_locked());
    }

    #[test]
    fn enable_authenticates_first() {
        let backend = FakeBackend::succeeding();
        let mut lock = AppLockManager::new(backend.clone(), false);
        assert!(lock.enable());
        assert!(lock.is_enabled());
        assert!(!lock.is_locked());
        let reasons = backend.reasons.lock().unwrap();
        assert_eq!(reasons.len(), 1);
        assert!(
            reasons[0].contains("Face ID"),
            "confirm reason names the method"
        );
    }

    #[test]
    fn enable_fails_when_auth_fails() {
        let mut lock = AppLockManager::new(
            FakeBackend::failing(vec![Err(AuthError::Failed("nope".into()))]),
            false,
        );
        assert!(!lock.enable());
        assert!(!lock.is_enabled());
        assert!(!lock.is_locked());
        assert_eq!(lock.last_error(), Some("nope"));
    }

    #[test]
    fn enable_is_idempotent_when_already_armed() {
        let backend = FakeBackend::succeeding();
        let mut lock = AppLockManager::new(backend.clone(), true);
        // Already enabled: no second prompt.
        assert!(lock.enable());
        assert!(backend.reasons.lock().unwrap().is_empty());
    }

    #[test]
    fn disable_needs_no_auth() {
        let backend = FakeBackend::succeeding();
        let mut lock = AppLockManager::new(backend.clone(), true);
        lock.disable();
        assert!(!lock.is_enabled());
        assert!(!lock.is_locked());
        assert!(backend.reasons.lock().unwrap().is_empty());
    }

    #[test]
    fn background_covers_and_foreground_prompts_once() {
        let mut lock = AppLockManager::new(FakeBackend::succeeding(), true);
        // Fresh launch is already covered; foreground fires one prompt.
        assert!(lock.begin_foreground_unlock());
        assert!(!lock.begin_foreground_unlock(), "no second auto-prompt");
        // Unlock, background, foreground: the cycle repeats exactly once.
        assert!(lock.unlock());
        assert!(!lock.is_locked());
        lock.lock_if_enabled();
        assert!(lock.is_locked());
        assert!(lock.begin_foreground_unlock());
        assert!(!lock.begin_foreground_unlock());
    }

    #[test]
    fn lock_if_enabled_is_noop_when_disarmed() {
        let mut lock = AppLockManager::new(FakeBackend::succeeding(), false);
        lock.lock_if_enabled();
        assert!(!lock.is_locked());
        assert!(!lock.begin_foreground_unlock());
    }

    #[test]
    fn user_cancel_is_silent_stays_locked() {
        let mut lock =
            AppLockManager::new(FakeBackend::failing(vec![Err(AuthError::UserCancel)]), true);
        assert!(!lock.unlock());
        assert!(lock.is_locked());
        assert_eq!(lock.last_error(), None, "cancels never surface an error");
    }

    #[test]
    fn failed_auth_surfaces_last_error() {
        let mut lock = AppLockManager::new(
            FakeBackend::failing(vec![Err(AuthError::Failed("biometry lockout".into()))]),
            true,
        );
        assert!(!lock.unlock());
        assert!(lock.is_locked());
        assert_eq!(lock.last_error(), Some("biometry lockout"));
        // A later successful attempt clears the error.
        let mut lock2 = AppLockManager::new(FakeBackend::succeeding(), true);
        assert!(lock2.unlock());
        assert_eq!(lock2.last_error(), None);
    }

    #[test]
    fn unlock_is_noop_when_already_unlocked() {
        let backend = FakeBackend::succeeding();
        let mut lock = AppLockManager::new(backend.clone(), false);
        assert!(lock.unlock());
        assert!(backend.reasons.lock().unwrap().is_empty());
    }

    #[test]
    fn method_labels_match_swift() {
        let face = AppLockCapability {
            available: true,
            biometry: BiometryType::FaceId,
        };
        let touch = AppLockCapability {
            available: true,
            biometry: BiometryType::TouchId,
        };
        let optic = AppLockCapability {
            available: true,
            biometry: BiometryType::OpticId,
        };
        let passcode = AppLockCapability {
            available: true,
            biometry: BiometryType::None,
        };
        assert_eq!(method_label(&face), "Face ID");
        assert_eq!(method_label(&touch), "Touch ID");
        assert_eq!(method_label(&optic), "Optic ID");
        assert_eq!(method_label(&passcode), "Passcode");
        assert_eq!(method_label(&AppLockCapability::UNAVAILABLE), "Face ID");
    }

    #[cfg(not(feature = "shell-biometric"))]
    #[test]
    fn shell_backend_reports_unavailable_without_feature() {
        let backend = ShellBiometricBackend;
        assert_eq!(backend.capability(), AppLockCapability::UNAVAILABLE);
        assert_eq!(
            backend.authenticate("x"),
            Err(AuthError::Unavailable),
            "plain webview/dev builds never prompt"
        );
        let backend = NoBiometricBackend;
        assert_eq!(backend.capability(), AppLockCapability::UNAVAILABLE);
    }
}
