//! Push notification registration, ported from the Swift `PushManager`.
//!
//! The platform half (permission prompt, APNs/FCM token delivery) lives in
//! the launcher: on iOS the system delivers an APNs device token, on the
//! Dioxus webview the launcher bridges the platform push token through JS.
//! This module owns the platform-independent half: the registration state
//! machine, token dedup, the "re-hand the cached token after (re)pairing"
//! rule, and the tapped-notification → open-session routing. Components talk
//! to the Host through [`crate::client::HostClient::register_push_token`].

/// Registration state, mirroring Swift's `PushRegistrationState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushRegistrationState {
    NotRequested,
    RequestingPermission,
    PermissionDenied,
    Registering,
    Registered { environment: String },
    Failed { message: String },
}

impl PushRegistrationState {
    /// User-visible diagnostics for the device half of push delivery. Never
    /// includes the token itself.
    pub fn diagnostic_label(&self) -> String {
        match self {
            PushRegistrationState::NotRequested => "Not requested".to_string(),
            PushRegistrationState::RequestingPermission => {
                "Waiting for notification permission…".to_string()
            }
            PushRegistrationState::PermissionDenied => {
                "Notifications are denied in system Settings".to_string()
            }
            PushRegistrationState::Registering => "Waiting for a push device token…".to_string(),
            PushRegistrationState::Registered { environment } => {
                if environment == "production" {
                    "Ready (production)".to_string()
                } else {
                    "Ready (sandbox)".to_string()
                }
            }
            PushRegistrationState::Failed { message } => {
                format!("Registration failed: {message}")
            }
        }
    }

    pub fn permission_was_denied(&self) -> bool {
        matches!(self, PushRegistrationState::PermissionDenied)
    }

    /// Broken-delivery states worth surfacing outside the settings sheet
    /// (the sidebar warning). Transient startup states stay quiet so a
    /// healthy launch never flashes a warning while registration settles.
    pub fn sidebar_warning(&self) -> Option<&'static str> {
        match self {
            PushRegistrationState::PermissionDenied => Some("Notifications are off"),
            PushRegistrationState::Failed { .. } => Some("Notifications aren't working"),
            _ => None,
        }
    }

    pub fn can_retry(&self) -> bool {
        matches!(
            self,
            PushRegistrationState::NotRequested
                | PushRegistrationState::PermissionDenied
                | PushRegistrationState::Failed { .. }
        )
    }
}

/// Hex-encode raw token bytes (APNs device tokens arrive as bytes).
pub fn hex_token(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Platform-independent push registration manager.
///
/// The launcher drives it: `begin_request()` when the user enables
/// notifications (the launcher shows the OS permission prompt),
/// `resolve_permission(granted)` with the prompt outcome,
/// `did_register(token_bytes)` when the platform delivers a token, and
/// `did_fail_to_register(message)` on platform errors. `on_token_change`
/// fires with `(token_hex, environment)` whenever a new token arrives —
/// the connection layer uploads it to the paired Host via
/// `HostClient::register_push_token`. `on_open_session` fires with the
/// session id when a tapped notification names one.
pub struct PushManager {
    token_hex: Option<String>,
    state: PushRegistrationState,
    /// "sandbox" for debug/dev builds, "production" for release — the Host
    /// forwards this so the relay hits the matching push host.
    pub environment: String,
    pub on_token_change: Option<Box<dyn Fn(String, String) + Send + Sync>>,
    pub on_open_session: Option<Box<dyn Fn(String) + Send + Sync>>,
}

// Callbacks are wired once on the live instance (the launcher sets them at
// startup); a clone is an inert snapshot — token, environment, and state
// carry over, callbacks do not.
impl Clone for PushManager {
    fn clone(&self) -> Self {
        Self {
            token_hex: self.token_hex.clone(),
            state: self.state.clone(),
            environment: self.environment.clone(),
            on_token_change: None,
            on_open_session: None,
        }
    }
}

impl PushManager {
    pub fn new(environment: impl Into<String>) -> Self {
        Self {
            token_hex: None,
            state: PushRegistrationState::NotRequested,
            environment: environment.into(),
            on_token_change: None,
            on_open_session: None,
        }
    }

    pub fn state(&self) -> &PushRegistrationState {
        &self.state
    }

    pub fn token_hex(&self) -> Option<&str> {
        self.token_hex.as_deref()
    }

    /// Start the permission flow. Safe to call repeatedly; a no-op once a
    /// token is registered.
    pub fn begin_request(&mut self) {
        if matches!(self.state, PushRegistrationState::Registered { .. }) {
            return;
        }
        self.state = PushRegistrationState::RequestingPermission;
    }

    /// The OS permission prompt resolved. Granted → wait for the platform
    /// token; denied → surface the denied state (retryable from Settings).
    pub fn resolve_permission(&mut self, granted: bool) {
        if !matches!(self.state, PushRegistrationState::RequestingPermission) {
            return;
        }
        self.state = if granted {
            PushRegistrationState::Registering
        } else {
            PushRegistrationState::PermissionDenied
        };
    }

    /// The platform delivered a device token. Hex-encode, cache, and hand it
    /// off — unless it is unchanged, in which case stay quiet.
    pub fn did_register(&mut self, token_bytes: &[u8]) {
        let hex = hex_token(token_bytes);
        self.state = PushRegistrationState::Registered {
            environment: self.environment.clone(),
        };
        if Some(hex.as_str()) == self.token_hex.as_deref() {
            return;
        }
        self.token_hex = Some(hex.clone());
        if let Some(cb) = &self.on_token_change {
            cb(hex, self.environment.clone());
        }
    }

    /// Hex-string variant of [`PushManager::did_register`] for tokens that
    /// arrive already hex-encoded (env vars, config). Returns false and
    /// leaves state untouched when the string isn't valid hex.
    pub fn did_register_hex(&mut self, hex: &str) -> bool {
        let hex = hex.trim();
        if !hex.is_empty()
            && hex.len().is_multiple_of(2)
            && hex.bytes().all(|b| b.is_ascii_hexdigit())
        {
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0))
                .collect();
            self.did_register(&bytes);
            true
        } else {
            false
        }
    }

    pub fn did_fail_to_register(&mut self, message: impl Into<String>) {
        self.state = PushRegistrationState::Failed {
            message: message.into(),
        };
    }

    /// Re-hand the cached token (e.g. after (re)pairing so the new Host gets
    /// it). No-op until a token exists.
    pub fn upload_cached_token(&self) {
        let (Some(hex), Some(cb)) = (self.token_hex.as_deref(), &self.on_token_change) else {
            return;
        };
        cb(hex.to_string(), self.environment.clone());
    }

    /// A tapped notification named a session — the root view selects it.
    pub fn did_open_notification(&self, session_id: &str) {
        if let Some(cb) = &self.on_open_session {
            cb(session_id.to_string());
        }
    }
}

/// Native-shell push bridge (the iOS-shell half of APNs token acquisition).
///
/// The native shell cannot hand the APNs device token to Rust directly — the
/// token arrives in the app delegate, outside the webview. The contract:
///
/// - The shell installs nothing itself; the launcher evaluates
///   [`PUSH_BRIDGE_JS`] once at startup, which defines
///   `window.__unpeelPush(msg)`.
/// - On `didRegisterForRemoteNotificationsWithDeviceToken` the shell calls
///   `webView.evaluateJavaScript("window.__unpeelPush('token:' + hex)")`.
/// - On `didFailToRegisterForRemoteNotificationsWithError` it calls
///   `window.__unpeelPush('error:' + message)`.
/// - On a notification tap it calls `window.__unpeelPush('open:' +
///   sessionId)` (the payload's `sessionId`).
/// - The bridge stashes the latest token at `window.__unpeelPushToken` so a
///   token that arrives before the launcher's message pump is installed is
///   still picked up by the one-time [`PUSH_TOKEN_PROBE_JS`] read.
///
/// Messages the launcher pump handles are parsed by
/// [`parse_push_bridge_message`].
pub const PUSH_BRIDGE_JS: &str = r#"(function() {
  if (window.__unpeelPush) return;
  window.__unpeelPushToken = window.__unpeelPushToken || "";
  window.__unpeelPush = function(msg) {
    if (msg.indexOf("token:") === 0) window.__unpeelPushToken = msg.slice(6);
    dioxus.send("push:" + msg);
  };
})()"#;

/// One-time read of the stashed token for the pre-pump race. Evaluates to
/// the hex string, or `""` when no token has arrived yet.
pub const PUSH_TOKEN_PROBE_JS: &str = r#"window.__unpeelPushToken || """#;

/// A decoded `push:` bridge message from the native shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushBridgeEvent {
    /// `push:token:{hex}` — APNs delivered a device token.
    Token(String),
    /// `push:error:{message}` — registration failed (simulator, denied…).
    Error(String),
    /// `push:open:{sessionId}` — the user tapped a notification; id is the
    /// payload's `sessionId`, mirroring the Swift PushManager tap path.
    Open(String),
}

/// Parse one `push:` message body (the `push:` prefix already stripped).
/// Returns `None` for malformed input, which the pump ignores.
pub fn parse_push_bridge_message(body: &str) -> Option<PushBridgeEvent> {
    if let Some(hex) = body.strip_prefix("token:") {
        let hex = hex.trim();
        if hex.is_empty() {
            return None;
        }
        Some(PushBridgeEvent::Token(hex.to_string()))
    } else if let Some(id) = body.strip_prefix("open:") {
        let id = id.trim();
        if id.is_empty() {
            return None;
        }
        Some(PushBridgeEvent::Open(id.to_string()))
    } else {
        body.strip_prefix("error:")
            .map(|msg| PushBridgeEvent::Error(msg.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn diagnostic_labels_never_contain_the_token() {
        let states = [
            PushRegistrationState::NotRequested,
            PushRegistrationState::RequestingPermission,
            PushRegistrationState::PermissionDenied,
            PushRegistrationState::Registering,
            PushRegistrationState::Registered {
                environment: "production".to_string(),
            },
            PushRegistrationState::Failed {
                message: "boom".to_string(),
            },
        ];
        for state in states {
            let label = state.diagnostic_label();
            assert!(!label.contains("deadbeef"), "{state:?} label leaks token?");
            assert!(!label.is_empty());
        }
    }

    #[test]
    fn sidebar_warning_only_for_broken_delivery() {
        assert!(PushRegistrationState::NotRequested
            .sidebar_warning()
            .is_none());
        assert!(PushRegistrationState::RequestingPermission
            .sidebar_warning()
            .is_none());
        assert!(PushRegistrationState::Registering
            .sidebar_warning()
            .is_none());
        assert!(PushRegistrationState::Registered {
            environment: "sandbox".to_string()
        }
        .sidebar_warning()
        .is_none());
        assert_eq!(
            PushRegistrationState::PermissionDenied.sidebar_warning(),
            Some("Notifications are off")
        );
        assert_eq!(
            PushRegistrationState::Failed {
                message: "x".to_string()
            }
            .sidebar_warning(),
            Some("Notifications aren't working")
        );
    }

    #[test]
    fn can_retry_states() {
        assert!(PushRegistrationState::NotRequested.can_retry());
        assert!(PushRegistrationState::PermissionDenied.can_retry());
        assert!(PushRegistrationState::Failed {
            message: "x".to_string()
        }
        .can_retry());
        assert!(!PushRegistrationState::RequestingPermission.can_retry());
        assert!(!PushRegistrationState::Registering.can_retry());
        assert!(!PushRegistrationState::Registered {
            environment: "sandbox".to_string()
        }
        .can_retry());
    }

    #[test]
    fn register_hex_validates() {
        let mut manager = PushManager::new("sandbox");
        assert!(manager.did_register_hex("deadBEEF"));
        assert_eq!(manager.token_hex(), Some("deadbeef"));
        assert!(!manager.did_register_hex("xyz"));
        assert!(!manager.did_register_hex("abc"));
        assert!(!manager.did_register_hex(""));
        // Rejected input leaves the cached token alone.
        assert_eq!(manager.token_hex(), Some("deadbeef"));
    }

    #[test]
    fn token_dedup_and_rehand() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        let mut manager = PushManager::new("sandbox");
        manager.on_token_change = Some(Box::new(move |hex, env| {
            seen2.lock().unwrap().push((hex, env));
        }));

        manager.begin_request();
        assert_eq!(
            *manager.state(),
            PushRegistrationState::RequestingPermission
        );
        manager.resolve_permission(true);
        assert_eq!(*manager.state(), PushRegistrationState::Registering);

        manager.did_register(&[0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(manager.token_hex(), Some("deadbeef"));
        assert_eq!(seen.lock().unwrap().len(), 1);

        // Same token again: state updates, callback stays quiet.
        manager.did_register(&[0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(seen.lock().unwrap().len(), 1);

        // Re-hand after (re)pairing.
        manager.upload_cached_token();
        assert_eq!(seen.lock().unwrap().len(), 2);
        assert_eq!(seen.lock().unwrap()[0].1, "sandbox");
    }

    #[test]
    fn denied_then_failed_flows() {
        let mut manager = PushManager::new("production");
        manager.begin_request();
        manager.resolve_permission(false);
        assert!(manager.state().permission_was_denied());
        assert!(manager.state().can_retry());

        manager.begin_request();
        manager.resolve_permission(true);
        manager.did_fail_to_register("APNs unreachable");
        assert_eq!(
            manager.state().diagnostic_label(),
            "Registration failed: APNs unreachable"
        );
    }

    #[test]
    fn open_notification_routes_to_session() {
        let opened = Arc::new(Mutex::new(Vec::new()));
        let opened2 = opened.clone();
        let mut manager = PushManager::new("sandbox");
        manager.on_open_session = Some(Box::new(move |id| {
            opened2.lock().unwrap().push(id);
        }));
        manager.did_open_notification("sess-42");
        assert_eq!(*opened.lock().unwrap(), vec!["sess-42".to_string()]);
    }

    #[test]
    fn begin_request_is_quiet_once_registered() {
        let mut manager = PushManager::new("sandbox");
        manager.did_register(&[1, 2, 3]);
        manager.begin_request();
        assert!(matches!(
            manager.state(),
            PushRegistrationState::Registered { .. }
        ));
    }

    #[test]
    fn push_bridge_parses_token_and_error() {
        assert_eq!(
            parse_push_bridge_message("token:deadBEEF"),
            Some(PushBridgeEvent::Token("deadBEEF".to_string()))
        );
        // The shell may pad; the hex itself is validated by did_register_hex.
        assert_eq!(
            parse_push_bridge_message("token:  abc123  "),
            Some(PushBridgeEvent::Token("abc123".to_string()))
        );
        assert_eq!(
            parse_push_bridge_message("error:remote notifications not supported"),
            Some(PushBridgeEvent::Error(
                "remote notifications not supported".to_string()
            ))
        );
        assert_eq!(parse_push_bridge_message("token:"), None);
        assert_eq!(parse_push_bridge_message("token:   "), None);
        assert_eq!(
            parse_push_bridge_message("open:sess-42"),
            Some(PushBridgeEvent::Open("sess-42".to_string()))
        );
        assert_eq!(parse_push_bridge_message("open:"), None);
        assert_eq!(parse_push_bridge_message("bogus"), None);
        assert_eq!(parse_push_bridge_message(""), None);
    }

    #[test]
    fn push_bridge_installer_defines_entry_point_and_stash() {
        // Static contract checks: the installer must be idempotent, define
        // window.__unpeelPush, stash the token for the pre-pump race, and
        // forward through dioxus.send with the push: prefix.
        assert!(PUSH_BRIDGE_JS.contains("window.__unpeelPushToken"));
        assert!(PUSH_BRIDGE_JS.contains("window.__unpeelPush = function(msg)"));
        assert!(PUSH_BRIDGE_JS.contains("dioxus.send(\"push:\" + msg)"));
        assert!(PUSH_TOKEN_PROBE_JS.contains("__unpeelPushToken"));
    }
}
