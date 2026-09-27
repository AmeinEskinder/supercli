//! Port of `DesktopNotifier.swift` — macOS notification payloads.
//!
//! Builds `UNUserNotificationCenter` payloads for session and workspace
//! attention notifications. Session identifiers collapse repeated
//! notifications; banner taps route to the session or workspace+session.
//! Presents banner/sound even while the app is foreground.
//!
//! The delivery state machine (`DeliveryTest`) is pure and tested; actual
//! `UNUserNotificationCenter` calls are `#[cfg(target_os = "macos")]`.

/// Notification identifier for a session: collapses repeats.
pub fn session_notification_id(session_id: &str) -> String {
    format!("supercli.session.{session_id}")
}

/// Notification identifier for workspace attention.
pub fn workspace_notification_id(workspace_id: &str, session_id: &str) -> String {
    format!("supercli.workspace.{workspace_id}.{session_id}")
}

/// A notification payload ready for `UNMutableNotificationContent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationPayload {
    pub identifier: String,
    pub title: String,
    pub body: String,
    /// Routes the banner tap: session, or workspace+session.
    pub route_session_id: Option<String>,
    pub route_workspace_id: Option<String>,
    pub sound: bool,
}

/// Builds a session completion notification.
pub fn session_notification(session_id: &str, title: &str, body: &str) -> NotificationPayload {
    NotificationPayload {
        identifier: session_notification_id(session_id),
        title: title.to_string(),
        body: body.to_string(),
        route_session_id: Some(session_id.to_string()),
        route_workspace_id: None,
        sound: true,
    }
}

/// Builds a workspace attention notification.
pub fn workspace_notification(
    workspace_id: &str,
    session_id: &str,
    title: &str,
    body: &str,
) -> NotificationPayload {
    NotificationPayload {
        identifier: workspace_notification_id(workspace_id, session_id),
        title: title.to_string(),
        body: body.to_string(),
        route_session_id: Some(session_id.to_string()),
        route_workspace_id: Some(workspace_id.to_string()),
        sound: true,
    }
}

/// Diagnostics for a notification delivery attempt. Swift:
/// idle/checking/sent/denied/alerts-disabled/failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryDiagnostic {
    Idle,
    Checking,
    Sent,
    Denied,
    AlertsDisabled,
    Failed(String),
}

/// Pure delivery test state machine: drives the diagnostic without macOS.
#[derive(Debug, Clone)]
pub struct DeliveryTest {
    pub diagnostic: DeliveryDiagnostic,
}

impl DeliveryTest {
    pub fn new() -> Self {
        Self {
            diagnostic: DeliveryDiagnostic::Idle,
        }
    }

    pub fn begin_check(&mut self) {
        self.diagnostic = DeliveryDiagnostic::Checking;
    }

    /// `authorized`: notification permission granted.
    /// `alert_enabled`: banner/list alert style enabled.
    pub fn finish_authorization(&mut self, authorized: bool, alert_enabled: bool) {
        self.diagnostic = if !authorized {
            DeliveryDiagnostic::Denied
        } else if !alert_enabled {
            DeliveryDiagnostic::AlertsDisabled
        } else {
            DeliveryDiagnostic::Sent
        };
    }

    pub fn fail(&mut self, reason: impl Into<String>) {
        self.diagnostic = DeliveryDiagnostic::Failed(reason.into());
    }
}

impl Default for DeliveryTest {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_collapse_repeated_notifications() {
        assert_eq!(session_notification_id("abc"), "supercli.session.abc");
        // Same session → same identifier, so repeats replace.
        assert_eq!(
            session_notification_id("abc"),
            session_notification_id("abc")
        );
        assert_ne!(
            session_notification_id("abc"),
            session_notification_id("def")
        );
    }

    #[test]
    fn workspace_ids_carry_workspace_and_session() {
        let id = workspace_notification_id("ws1", "sess1");
        assert!(id.contains("ws1"), "{id}");
        assert!(id.contains("sess1"), "{id}");
    }

    #[test]
    fn session_notification_routes_to_session() {
        let payload = session_notification("s1", "Done", "Task finished");
        assert_eq!(payload.identifier, "supercli.session.s1");
        assert_eq!(payload.route_session_id, Some("s1".to_string()));
        assert_eq!(payload.route_workspace_id, None);
        assert!(payload.sound);
    }

    #[test]
    fn workspace_notification_routes_to_workspace_and_session() {
        let payload = workspace_notification("w1", "s1", "Attention", "Needs input");
        assert_eq!(payload.route_session_id, Some("s1".to_string()));
        assert_eq!(payload.route_workspace_id, Some("w1".to_string()));
    }

    #[test]
    fn delivery_test_drives_diagnostics() {
        let mut test = DeliveryTest::new();
        assert_eq!(test.diagnostic, DeliveryDiagnostic::Idle);
        test.begin_check();
        assert_eq!(test.diagnostic, DeliveryDiagnostic::Checking);
        test.finish_authorization(true, true);
        assert_eq!(test.diagnostic, DeliveryDiagnostic::Sent);
    }

    #[test]
    fn delivery_test_reports_denied() {
        let mut test = DeliveryTest::new();
        test.begin_check();
        test.finish_authorization(false, true);
        assert_eq!(test.diagnostic, DeliveryDiagnostic::Denied);
    }

    #[test]
    fn delivery_test_reports_alerts_disabled() {
        let mut test = DeliveryTest::new();
        test.begin_check();
        test.finish_authorization(true, false);
        assert_eq!(test.diagnostic, DeliveryDiagnostic::AlertsDisabled);
    }

    #[test]
    fn delivery_test_reports_failure() {
        let mut test = DeliveryTest::new();
        test.begin_check();
        test.fail("simulator has no notification center");
        assert_eq!(
            test.diagnostic,
            DeliveryDiagnostic::Failed("simulator has no notification center".to_string())
        );
    }
}
