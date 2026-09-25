//! Session activity rows and the session-context model. Ported from
//! `clients/native/UnpeelNative/Sources/UnpeelNative/SessionActivity.swift`
//! and `SessionRowContext.swift`.
//!
//! The row is "everything the Host streams about a session's current state,
//! minus the transcript": identity, agent state, approval, timers, and the
//! derived presentation fields (row sublabel, sort key).

use crate::i18n::t;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent state as the Host reports it for a session. Mirrors the Swift
/// `SessionActivity` agent-state cases (verbatim copy).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionAgentState {
    #[default]
    Idle,
    Running,
    Waiting,
    /// The approval banner's full text, when one is on screen.
    Approval {
        banner_text: String,
    },
    /// Full title text the agent is generating (tool name for tool work,
    /// summary text for summaries, e.g. "Using a web browser").
    Generating {
        title: String,
        started_at: u64,
    },
}

impl SessionAgentState {
    pub fn is_approval(&self) -> bool {
        matches!(self, SessionAgentState::Approval { .. })
    }

    pub fn is_generating(&self) -> bool {
        matches!(self, SessionAgentState::Generating { .. })
    }
}

/// Everything the Host streams about a session's current state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionActivity {
    pub agent_state: SessionAgentState,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub started_at: Option<u64>,
    #[serde(default)]
    pub elapsed_secs: Option<u64>,
    /// Live line count from the transcript; None when unknown.
    #[serde(default)]
    pub transcript_lines: Option<u64>,
    /// Extra presentation fields the Host attaches (kept opaque).
    #[serde(default, flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// A session row in the sidebar: identity + activity + filing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRowContext {
    pub session_id: String,
    pub display_name: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub effective_project_id: Option<String>,
    pub activity: SessionActivity,
    /// Notifications the launcher has not yet surfaced for this session.
    #[serde(default)]
    pub pending_notifications: Vec<String>,
}

impl SessionRowContext {
    /// The row sublabel: approval text, generation title, or the provider.
    pub fn sublabel(&self) -> String {
        match &self.activity.agent_state {
            SessionAgentState::Approval { banner_text } => banner_text.clone(),
            SessionAgentState::Generating { title, .. } => title.clone(),
            _ => self
                .activity
                .provider_id
                .clone()
                .unwrap_or_else(|| "idle".to_string()),
        }
    }

    /// Sort key: approvals first, then generating, then by recency.
    pub fn sort_key(&self) -> (u8, u64) {
        let priority = match &self.activity.agent_state {
            SessionAgentState::Approval { .. } => 0,
            SessionAgentState::Generating { .. } => 1,
            SessionAgentState::Running | SessionAgentState::Waiting => 2,
            SessionAgentState::Idle => 3,
        };
        let recency = self.activity.started_at.unwrap_or(0);
        (priority, u64::MAX - recency)
    }

    pub fn has_unread_approval(&self) -> bool {
        self.activity.agent_state.is_approval() && !self.pending_notifications.is_empty()
    }
}

/// Dioxus component: the activity row used in the sidebar and the
/// notification center.
pub mod component {
    use super::*;
    use dioxus::prelude::*;

    #[component]
    pub fn SessionActivityRow(
        context: SessionRowContext,
        selected: bool,
        on_select: EventHandler<String>,
    ) -> Element {
        rsx! {
            button {
                class: if selected { "session-row selected" } else { "session-row" },
                onclick: {
                    let id = context.session_id.clone();
                    move |_| on_select.call(id.clone())
                },
                div { class: "session-row-name", "{context.display_name}" }
                div { class: "session-row-sublabel", "{context.sublabel()}" }
                if context.activity.agent_state.is_approval() {
                    span { class: "session-row-approval-badge", {t("activity.needs_approval")} }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approval_ctx() -> SessionRowContext {
        SessionRowContext {
            session_id: "s1".into(),
            display_name: "Session".into(),
            activity: SessionActivity {
                agent_state: SessionAgentState::Approval {
                    banner_text: "Approve edit?".into(),
                },
                ..Default::default()
            },
            pending_notifications: vec!["n1".into()],
            ..Default::default()
        }
    }

    #[test]
    fn sublabel_prefers_approval_then_generating() {
        assert_eq!(approval_ctx().sublabel(), "Approve edit?");
        let gen = SessionRowContext {
            session_id: "s2".into(),
            display_name: "S".into(),
            activity: SessionActivity {
                agent_state: SessionAgentState::Generating {
                    title: { t("activity.using_a_web_browser") }.into(),
                    started_at: 1,
                },
                provider_id: Some("claude".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(gen.sublabel(), "Using a web browser");
        let idle = SessionRowContext {
            session_id: "s3".into(),
            display_name: "S".into(),
            ..Default::default()
        };
        assert_eq!(idle.sublabel(), "idle");
    }

    #[test]
    fn sort_key_orders_approvals_first() {
        let approval = approval_ctx();
        let mut running = approval.clone();
        running.activity.agent_state = SessionAgentState::Running;
        running.activity.started_at = Some(9_999_999);
        running.pending_notifications.clear();
        let mut idle = approval.clone();
        idle.activity.agent_state = SessionAgentState::Idle;
        let mut v = [idle.clone(), running.clone(), approval.clone()];
        v.sort_by_key(|c| c.sort_key());
        assert_eq!(v[0].session_id, "s1"); // approval first
        assert_eq!(v[1].activity.agent_state, SessionAgentState::Running);
        assert_eq!(v[2].activity.agent_state, SessionAgentState::Idle);
    }

    #[test]
    fn unread_approval_flag() {
        assert!(approval_ctx().has_unread_approval());
        let mut c = approval_ctx();
        c.pending_notifications.clear();
        assert!(!c.has_unread_approval());
    }

    #[test]
    fn state_predicates() {
        assert!(SessionAgentState::Approval {
            banner_text: "x".into()
        }
        .is_approval());
        assert!(SessionAgentState::Generating {
            title: "x".into(),
            started_at: 0
        }
        .is_generating());
        assert!(!SessionAgentState::Idle.is_approval());
    }
}
