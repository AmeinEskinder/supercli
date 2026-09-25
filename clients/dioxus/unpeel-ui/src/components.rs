//! Presentational components shared by the desktop and mobile clients.
//!
//! Convention: components are pure renderers over `unpeel_client` DTOs.
//! All Host I/O happens in the app shell (`unpeel-desktop` /
//! `unpeel-mobile`), which passes snapshots down and receives intents up
//! via `EventHandler`s. This keeps the components testable and the I/O
//! policy in one place.

use std::collections::HashMap;

use dioxus::prelude::*;
use unpeel_client::dto::{
    ActivityState, PendingApproval, SessionSummary, TranscriptEntry, TranscriptRole,
};

use super::presence::{ViewerAvatars, ViewerInfo};
use super::i18n::t;

use unpeel_client::TransportKind;

/// Top bar: Host identity + connection state + transport + refresh.
///
/// The transport label mirrors the native clients: only **Direct** or
/// **Via Link** is ever shown — never relay internals.
#[component]
pub fn ConnectionBar(
    host_name: Option<String>,
    connected: bool,
    transport: TransportKind,
    on_refresh: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "connection-bar",
            span { class: if connected { "dot ok" } else { "dot bad" } }
            span { class: "host-name",
                {host_name.unwrap_or_else(|| "No host".to_string())}
            }
            span { class: "transport",
                {transport.to_string()}
            }
            button {
                class: "refresh",
                onclick: move |_| on_refresh.call(()),
                "Refresh"
            }
        }
    }
}

/// Sessions to render: archived sessions are hidden unless the archive
/// toggle is on. Pure so the rule is unit-testable without a renderer.
fn visible_sessions(sessions: &[SessionSummary], show_archived: bool) -> Vec<&SessionSummary> {
    sessions
        .iter()
        .filter(|s| !s.archived || show_archived)
        .collect()
}

/// Sidebar list of sessions with activity badges and unread dots.
///
/// Archived sessions are hidden by default — they belong to the Host's
/// archive, not the working set — with a toggle to reveal them. Both
/// launchers share this behavior through this component.
///
/// Archive/restore ride the Host's session-organization patch
/// (`POST /mobile/session-organization`), not the session-action verbs —
/// mirroring the native clients. The handlers are optional: a launcher
/// that doesn't pass them simply offers no archive UI. `on_organize`
/// opens the session organize sheet (rename/pin/notify/verbs).
#[component]
pub fn SessionList(
    sessions: Vec<SessionSummary>,
    selected_id: Option<String>,
    on_select: EventHandler<String>,
    on_archive: Option<EventHandler<String>>,
    on_restore: Option<EventHandler<String>>,
    on_organize: Option<EventHandler<String>>,
    /// Session id → current viewers for the avatar chips. `None` (the
    /// mobile default) renders no chips.
    viewers: Option<HashMap<String, Vec<ViewerInfo>>>,
) -> Element {
    let mut show_archived = use_signal(|| false);
    let archived_count = sessions.iter().filter(|s| s.archived).count();
    let show = *show_archived.read();
    let visible = visible_sessions(&sessions, show);
    rsx! {
        div { class: "session-list", "data-testid": "session-list",
            for s in visible {
                {
                    let id = s.id.clone();
                    let selected = selected_id.as_deref() == Some(id.as_str());
                    let badge = match s.activity {
                        ActivityState::Working => "working",
                        ActivityState::Blocked => "blocked",
                        ActivityState::Done => "done",
                        ActivityState::Starting => "starting",
                        _ => "idle",
                    };
                    let unread = s.unread;
                    let title = s.title.clone();
                    let archived = s.archived;
                    let archive_id = id.clone();
                    let restore_id = id.clone();
                    let test_id = format!("session-row-{}", id);
                    rsx! {
                        div {
                            key: "{id}",
                            class: if selected { "session-row selected" } else { "session-row" },
                            "data-testid": "{test_id}",
                            onclick: move |_| on_select.call(id.clone()),
                            span { class: "unread-dot", hidden: !unread }
                            span { class: "session-title", "data-testid": "session-title", "{title}" }
                            {
                                let chips = viewers
                                    .as_ref()
                                    .and_then(|m| m.get(&id))
                                    .filter(|v| !v.is_empty())
                                    .cloned()
                                    .unwrap_or_default();
                                rsx! {
                                    if !chips.is_empty() {
                                        ViewerAvatars { viewers: chips }
                                    }
                                }
                            }
                            span { class: "activity-badge {badge}", "{badge}" }
                            if archived {
                                if let Some(on_restore) = on_restore {
                                    button {
                                        class: "session-restore",
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            on_restore.call(restore_id.clone());
                                        },
                                        "Restore"
                                    }
                                }
                            } else if let Some(on_archive) = on_archive {
                                button {
                                    class: "session-archive",
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        on_archive.call(archive_id.clone());
                                    },
                                    "Archive"
                                }
                            }
                            if let Some(on_organize) = on_organize {
                                {
                                    let oid = id.clone();
                                    rsx! {
                                        button {
                                            class: "session-organize",
                                            onclick: move |e| {
                                                e.stop_propagation();
                                                on_organize.call(oid.clone());
                                            },
                                            "⋯"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if archived_count > 0 {
                button {
                    class: "archived-toggle",
                    onclick: move |_| show_archived.toggle(),
                    if show { "Hide archived" } else { "Show archived ({archived_count})" }
                }
            }
        }
    }
}

/// Chat-style rendering of a session transcript: user right, agent left,
/// like the Codex desktop app.
#[component]
pub fn ChatView(entries: Vec<TranscriptEntry>) -> Element {
    rsx! {
        div { class: "chat-view",
            for e in entries {
                {
                    let mine = matches!(e.role, TranscriptRole::User);
                    let text = e
                        .blocks
                        .iter()
                        .filter_map(|b| b.text.clone())
                        .collect::<Vec<_>>()
                        .join("\n");
                    rsx! {
                        div {
                            key: "{e.id}",
                            class: if mine { "bubble mine" } else { "bubble theirs" },
                            pre { "{text}" }
                        }
                    }
                }
            }
        }
    }
}

/// Non-semantic transcript pane: renders the Host's raw transcript
/// Markdown as a transcript, explicitly NOT as chat bubbles. Raw PTY
/// output must never be dressed up as agent chat turns.
#[component]
pub fn TranscriptView(markdown: String) -> Element {
    rsx! {
        div { class: "transcript-view",
            pre { "{markdown}" }
        }
    }
}

/// Inline approval card: the single consent surface for MCP approvals.
/// Approve / Deny map straight onto `POST /mobile/approvals/answer`.
///
/// Accessibility: rendered as an `alertdialog` with labelledby always
/// pointing at the title, and describedby pointing at the detail only when
/// a detail exists (a dangling describedby id is worse than none).
#[component]
pub fn ApprovalCard(approval: PendingApproval, on_answer: EventHandler<bool>) -> Element {
    let title_id = format!("approval-title-{}", approval.id);
    let detail_id = format!("approval-detail-{}", approval.id);
    let title = approval
        .title
        .clone()
        .unwrap_or_else(|| "Approval requested".to_string());
    // Only reference the detail node when one is rendered.
    let describedby: Option<String> = approval.detail.as_ref().map(|_| detail_id.clone());
    rsx! {
        div {
            class: "approval-card",
            role: "alertdialog",
            aria_modal: "false",
            aria_labelledby: "{title_id}",
            aria_describedby: describedby,
            div { class: "approval-title", id: "{title_id}", "{title}" }
            if let Some(detail) = approval.detail.clone() {
                div { class: "approval-detail", id: "{detail_id}", "{detail}" }
            }
            div { class: "approval-actions",
                button {
                    class: "approve",
                    autofocus: true,
                    aria_label: "Approve: {title}",
                    onclick: move |_| on_answer.call(true),
                    "{t(\"approval.approve\")}"
                }
                button {
                    class: "deny",
                    aria_label: "Deny: {title}",
                    onclick: move |_| on_answer.call(false),
                    "{t(\"approval.deny\")}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unpeel_client::dto::{ActivityState, SessionCapabilities, SessionStatus};

    fn session(id: &str, archived: bool) -> SessionSummary {
        SessionSummary {
            id: id.to_string(),
            project_id: String::new(),
            active_runtime_id: None,
            runtime_launch_pending: false,
            provider_id: None,
            title: id.to_string(),
            command: String::new(),
            created_at_unix_ms: 0,
            updated_at_unix_ms: None,
            status: SessionStatus::default(),
            activity: ActivityState::Idle,
            activity_source: None,
            unread: false,
            pinned: false,
            worktree_path: None,
            worktree_branch: None,
            parent_session_id: None,
            last_output_preview: None,
            notify_when_done: false,
            terminal_background_hex: None,
            archived,
            capabilities: SessionCapabilities::default(),
        }
    }

    #[test]
    fn archived_sessions_hidden_by_default() {
        let sessions = vec![session("a", false), session("b", true)];
        let visible = visible_sessions(&sessions, false);
        assert_eq!(
            visible.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec!["a"]
        );
    }

    #[test]
    fn archived_sessions_shown_with_toggle() {
        let sessions = vec![session("a", false), session("b", true)];
        let visible = visible_sessions(&sessions, true);
        assert_eq!(visible.len(), 2);
    }

    #[test]
    fn no_archived_sessions_means_no_filtering() {
        let sessions = vec![session("a", false)];
        assert_eq!(visible_sessions(&sessions, false).len(), 1);
    }
}
