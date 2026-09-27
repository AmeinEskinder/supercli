//! APNs push registration state machine.
//!
//! Port of the UI-agnostic `PushRegistrationState` enum from `PushManager.swift`
//! (`clients/legacy/ios/SupercliIOS`). The `PushManager` itself is UIKit-bound
//! (`UNUserNotificationCenter`, `UIApplication` delegates) and stays out; the
//! state machine — diagnostics, sidebar warnings, retry policy, and the
//! device-token hex encoding — is pure and portable.

/// APNs registration state. Never carries the token itself in diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushRegistrationState {
    NotRequested,
    RequestingPermission,
    PermissionDenied,
    Registering,
    Registered { environment: String },
    Failed(String),
}

impl PushRegistrationState {
    /// User-visible diagnostics for the device half of push delivery.
    ///
    /// Port of `PushRegistrationState.diagnosticLabel`.
    pub fn diagnostic_label(&self) -> String {
        match self {
            PushRegistrationState::NotRequested => "Not requested".to_string(),
            PushRegistrationState::RequestingPermission => {
                "Waiting for notification permission…".to_string()
            }
            PushRegistrationState::PermissionDenied => {
                "Notifications are denied in iOS Settings".to_string()
            }
            PushRegistrationState::Registering => "Waiting for an APNs device token…".to_string(),
            PushRegistrationState::Registered { environment } => {
                if environment == "production" {
                    "Ready (production)".to_string()
                } else {
                    "Ready (sandbox)".to_string()
                }
            }
            PushRegistrationState::Failed(message) => {
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
    ///
    /// Port of `PushRegistrationState.sidebarWarning`.
    pub fn sidebar_warning(&self) -> Option<&'static str> {
        match self {
            PushRegistrationState::PermissionDenied => Some("Notifications are off"),
            PushRegistrationState::Failed(_) => Some("Notifications aren't working"),
            PushRegistrationState::NotRequested
            | PushRegistrationState::RequestingPermission
            | PushRegistrationState::Registering
            | PushRegistrationState::Registered { .. } => None,
        }
    }

    /// Port of `PushRegistrationState.canRetry`.
    pub fn can_retry(&self) -> bool {
        matches!(
            self,
            PushRegistrationState::NotRequested
                | PushRegistrationState::PermissionDenied
                | PushRegistrationState::Failed(_)
        )
    }
}

/// Hex-encode an APNs device token, mirroring `PushManager.didRegister`.
pub fn hex_encode_device_token(token: &[u8]) -> String {
    token.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port of testRegistrationDiagnosticsNamePermissionTokenAndEnvironmentStates.
    #[test]
    fn registration_diagnostics_name_permission_and_environment_states() {
        assert_eq!(
            PushRegistrationState::PermissionDenied.diagnostic_label(),
            "Notifications are denied in iOS Settings"
        );
        assert_eq!(
            PushRegistrationState::Registering.diagnostic_label(),
            "Waiting for an APNs device token…"
        );
        assert_eq!(
            PushRegistrationState::Registered {
                environment: "production".to_string()
            }
            .diagnostic_label(),
            "Ready (production)"
        );
        assert_eq!(
            PushRegistrationState::Registered {
                environment: "sandbox".to_string()
            }
            .diagnostic_label(),
            "Ready (sandbox)"
        );
        assert!(PushRegistrationState::PermissionDenied.permission_was_denied());
        assert!(PushRegistrationState::Failed("offline".to_string()).can_retry());
        assert!(!PushRegistrationState::Registering.can_retry());
    }

    #[test]
    fn sidebar_warning_only_for_broken_delivery() {
        assert_eq!(
            PushRegistrationState::PermissionDenied.sidebar_warning(),
            Some("Notifications are off")
        );
        assert_eq!(
            PushRegistrationState::Failed("x".to_string()).sidebar_warning(),
            Some("Notifications aren't working")
        );
        assert_eq!(PushRegistrationState::Registering.sidebar_warning(), None);
        assert_eq!(
            PushRegistrationState::Registered {
                environment: "production".to_string()
            }
            .sidebar_warning(),
            None
        );
        assert_eq!(PushRegistrationState::NotRequested.sidebar_warning(), None);
    }

    #[test]
    fn device_token_hex_encoding() {
        assert_eq!(hex_encode_device_token(&[0x0a, 0xff, 0x00]), "0aff00");
        assert_eq!(hex_encode_device_token(&[]), "");
    }
}
