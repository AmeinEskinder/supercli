//! Desktop OS notifications for the Dioxus desktop launcher.
//!
//! Swift source: `clients/native/.../DesktopNotifier.swift` — macOS
//! Notification Center banners for the triggers that drive phone push:
//! needs-input, opted-in completion, and informational alerts. The two
//! channels are independent: the Mac notifies when the desktop isn't already
//! showing the session, phone push fires when no phone is viewing it.
//!
//! The Dioxus desktop launcher is a webview, so the OS banner goes through
//! the **Web Notification API** (`window.__supercliNotify`, installed by
//! [`NOTIFIER_JS`]) instead of `UNUserNotificationCenter` — no native shell
//! code needed. The ported policy:
//!
//! - [`NotificationKind`] keeps Swift's kind strings (`needs_input`,
//!   `done`, `alert`) so the Host contract is unchanged.
//! - [`NotifierState::should_post`] collapses repeats for one tag exactly
//!   like Swift's `threadIdentifier` + request identifier: the Web
//!   Notification `tag` option natively replaces an earlier banner with the
//!   same tag, and the policy additionally refuses to re-post an identical
//!   tag back-to-back.
//! - A tap on the banner reports `notify-click:{tag}` into Rust via
//!   `dioxus.send`; the launcher maps the tag back to a session id and
//!   selects it — Swift's `onSelectSession`.
//! - Swift's `requestAuthorizationIfNeeded` (ask once, early) becomes the
//!   lazy `Notification.requestPermission()` inside `__supercliNotify`: the
//!   first post prompts, a denial silently no-ops later posts.
//!
//! Trigger wiring (the Dioxus equivalent of the HookServer
//! `/deliverNotification` path, which doesn't exist in the webview
//! clients): the desktop launcher diffs each bootstrap's
//! `pending_approvals` against the previous snapshot and posts
//! `needs_input` for approvals on sessions other than the selected one —
//! the "Mac notifies when the desktop isn't already showing the session"
//! rule. Workspace-pool attention banners have no Dioxus equivalent (no
//! workspace pool) and are not ported.

use crate::i18n::t;

/// Kind strings shared with the Host/phone contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    NeedsInput,
    Done,
    Alert,
}

impl NotificationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationKind::NeedsInput => "needs_input",
            NotificationKind::Done => "done",
            NotificationKind::Alert => "alert",
        }
    }
}

/// One banner to post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopNotification {
    pub title: String,
    pub body: String,
    /// Deduplication key. The launcher builds it so a tap can be mapped
    /// back, e.g. `approval:{approval_id}`.
    pub tag: String,
    pub kind: NotificationKind,
}

impl DesktopNotification {
    /// Approval waiting on a session the desktop isn't showing — the
    /// needs-input trigger.
    pub fn approval_needed(approval_id: &str, session_title: &str, detail: Option<&str>) -> Self {
        Self {
            title: session_title.to_string(),
            body: detail
                .map(|d| format!("Needs your input — {d}"))
                .unwrap_or_else(|| { t("notifier.needs_your_input") }.to_string()),
            tag: format!("approval:{approval_id}"),
            kind: NotificationKind::NeedsInput,
        }
    }
}

/// Launcher-side dedup state. Each tag notifies at most once: repeats of
/// one approval across bootstraps collapse (Swift's `threadIdentifier` +
/// request-identifier collapse), and tags already pending at connect time
/// are seeded via [`NotifierState::mark_seen`] so connecting doesn't burst
/// a notification per pre-existing approval — only approvals that arrive
/// while the desktop is running notify.
#[derive(Debug, Default, Clone)]
pub struct NotifierState {
    seen_tags: std::collections::HashSet<String>,
    tag_routes: std::collections::HashMap<String, (String, String)>,
}

impl NotifierState {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when this notification should actually post: the first time its
    /// tag is seen.
    pub fn should_post(&mut self, notif: &DesktopNotification) -> bool {
        self.seen_tags.insert(notif.tag.clone())
    }

    /// Seed a tag as already handled without posting (connect-time).
    pub fn mark_seen(&mut self, tag: String) {
        self.seen_tags.insert(tag);
    }

    /// Remember where a banner tag routes to for tap handling.
    pub fn route_tag(&mut self, tag: String, host_id: String, session_id: String) {
        self.tag_routes.insert(tag, (host_id, session_id));
    }

    /// Resolve a `notify-click:{tag}` message to `(host_id, session_id)`.
    pub fn route_for_click_message(&self, msg: &str) -> Option<(&str, &str)> {
        let tag = msg.strip_prefix("notify-click:")?;
        self.tag_routes
            .get(tag)
            .map(|(h, s)| (h.as_str(), s.as_str()))
    }
}

/// Installs `window.__supercliNotify(title, body, tag)`.
///
/// - Requests Notification permission lazily on first post; denial (or a
///   browser without the API) silently no-ops — mirroring Swift's
///   "a denied grant just no-ops later posts".
/// - `tag` collapses repeats natively (Swift's `threadIdentifier`).
/// - Clicking the banner sends `notify-click:{tag}` through `dioxus.send`;
///   the launcher's eval pump (which must stay open) routes it via
///   [`NotifierState::session_for_click_message`].
pub const NOTIFIER_JS: &str = r#"(function() {
  if (window.__supercliNotifyInstalled) return true;
  window.__supercliNotifyInstalled = true;
  window.__supercliNotify = function(title, body, tag) {
    if (!({t("notifier.notification")} in window)) return;
    function post() {
      try {
        var n = new Notification(title, { body: body, tag: tag });
        n.onclick = function() { dioxus.send("notify-click:" + tag); };
      } catch (e) {}
    }
    if (Notification.permission === "granted") {
      post();
    } else if (Notification.permission !== "denied") {
      try {
        Notification.requestPermission().then(function(p) {
          if (p === "granted") post();
        });
      } catch (e) {}
    }
  };
  return true;
})()"#;

/// JS to post one notification. Arguments are passed through a single JSON
/// object substituted exactly once (the share-payload hardening pattern),
/// so hostile titles/bodies are embedded verbatim.
pub fn notifier_post_js(notif: &DesktopNotification) -> String {
    let args = serde_json::json!({
        "title": notif.title,
        "body": notif.body,
        "tag": notif.tag,
    });
    // `</` escaping: the JSON is inlined in a <script>-eval'd string.
    let json = args.to_string().replace("</", "<\\/");
    format!("(function(a){{ window.__supercliNotify(a.title, a.body, a.tag); }})({json})")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approval(tag_suffix: &str) -> DesktopNotification {
        DesktopNotification {
            title: "sess".to_string(),
            body: "Needs your input".to_string(),
            tag: format!("approval:{tag_suffix}"),
            kind: NotificationKind::NeedsInput,
        }
    }

    #[test]
    fn repeats_collapse() {
        let mut st = NotifierState::new();
        let a = approval("1");
        assert!(st.should_post(&a));
        assert!(!st.should_post(&a)); // repeat of the same tag: dropped
        let b = approval("2");
        assert!(st.should_post(&b));
        assert!(!st.should_post(&a)); // each tag notifies at most once
    }

    #[test]
    fn connect_time_seeding_suppresses_burst() {
        let mut st = NotifierState::new();
        st.mark_seen("approval:old".to_string());
        let old = approval("old");
        assert!(!st.should_post(&old));
        let fresh = approval("new");
        assert!(st.should_post(&fresh));
    }

    #[test]
    fn click_message_routes_to_host_and_session() {
        let mut st = NotifierState::new();
        st.route_tag(
            "approval:abc".to_string(),
            "host-9".to_string(),
            "sess-1".to_string(),
        );
        assert_eq!(
            st.route_for_click_message("notify-click:approval:abc"),
            Some(("host-9", "sess-1"))
        );
        assert_eq!(
            st.route_for_click_message("notify-click:approval:zzz"),
            None
        );
        assert_eq!(st.route_for_click_message("app-lock:hidden"), None);
    }

    #[test]
    fn kind_strings_match_host_contract() {
        assert_eq!(NotificationKind::NeedsInput.as_str(), "needs_input");
        assert_eq!(NotificationKind::Done.as_str(), "done");
        assert_eq!(NotificationKind::Alert.as_str(), "alert");
    }

    #[test]
    fn approval_needed_shapes() {
        let n = DesktopNotification::approval_needed("a1", "my session", Some("run tests?"));
        assert_eq!(n.tag, "approval:a1");
        assert_eq!(n.body, "Needs your input — run tests?");
        assert_eq!(n.kind, NotificationKind::NeedsInput);
        let n2 = DesktopNotification::approval_needed("a1", "my session", None);
        assert_eq!(n2.body, "Needs your input");
    }

    #[test]
    fn post_js_embeds_hostile_text_verbatim() {
        let n = DesktopNotification {
            title: "</script><script>alert(1)</script>".to_string(),
            body: "x".to_string(),
            tag: "approval:1".to_string(),
            kind: NotificationKind::Alert,
        };
        let js = notifier_post_js(&n);
        assert!(
            !js.contains("</script>"),
            "must escape </ to stay in the eval string"
        );
        assert!(js.contains("window.__supercliNotify(a.title, a.body, a.tag)"));
    }
}
