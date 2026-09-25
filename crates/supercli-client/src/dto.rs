//! Serde data-transfer objects for the Host `/mobile` API.
//!
//! Ports the `Remote*` structs in `RemoteControlProtocol.swift`. Every field
//! is optional or defaulted: the DTOs must decode bootstrap snapshots from
//! both older and newer Hosts without failing, and unknown JSON fields are
//! ignored. Additive protocol evolution must never break the client.

use serde::{Deserialize, Serialize};

/// What the agent in a session is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityState {
    Starting,
    Working,
    Blocked,
    Done,
    Idle,
    /// Also catches unrecognized future states.
    #[default]
    #[serde(other)]
    Unknown,
}

/// Lifecycle status of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Running,
    #[default]
    #[serde(other)]
    Other,
}

/// Where `activity` came from while running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivitySource {
    /// Exact: the runtime's Supercli integration (hook latch).
    Hooks,
    /// The Host's screen fallback — lower confidence, never a completion
    /// notification.
    Screen,
    #[serde(other)]
    Other,
}

/// Per-session capability flags the Host advertises on each session
/// summary. The iOS organize sheet gates every verb on these — missing
/// capability data fails closed so a Controller never promises a resume
/// the Host cannot do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionCapabilities {
    /// Legacy terminal-replacing Resume operation.
    #[serde(default)]
    pub restart: bool,
    /// Shell-only Resume Agent after the managed runtime exited.
    #[serde(rename = "resumeAgent", default)]
    pub resume_agent: bool,
    /// Whether the archive verb is offered (evidence-based).
    #[serde(default)]
    pub archive: bool,
    /// Whether notify-when-done is offered for this session.
    #[serde(rename = "notifyWhenDone", default)]
    pub notify_when_done: bool,
}

/// A session row, as published by bootstrap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    #[serde(rename = "projectID", default)]
    pub project_id: String,
    #[serde(rename = "activeRuntimeID", default)]
    pub active_runtime_id: Option<String>,
    #[serde(rename = "runtimeLaunchPending", default)]
    pub runtime_launch_pending: bool,
    #[serde(rename = "providerID", default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub command: String,
    #[serde(rename = "createdAtUnixMs", default)]
    pub created_at_unix_ms: i64,
    #[serde(rename = "updatedAtUnixMs", default)]
    pub updated_at_unix_ms: Option<i64>,
    #[serde(default)]
    pub status: SessionStatus,
    #[serde(default)]
    pub activity: ActivityState,
    #[serde(rename = "activitySource", default)]
    pub activity_source: Option<ActivitySource>,
    #[serde(default)]
    pub unread: bool,
    #[serde(default)]
    pub pinned: bool,
    #[serde(rename = "worktreePath", default)]
    pub worktree_path: Option<String>,
    #[serde(rename = "worktreeBranch", default)]
    pub worktree_branch: Option<String>,
    #[serde(rename = "parentSessionID", default)]
    pub parent_session_id: Option<String>,
    #[serde(rename = "lastOutputPreview", default)]
    pub last_output_preview: Option<String>,
    #[serde(rename = "notifyWhenDone", default)]
    pub notify_when_done: bool,
    #[serde(rename = "terminalBackgroundHex", default)]
    pub terminal_background_hex: Option<i32>,
    #[serde(default)]
    pub archived: bool,
    /// Per-session capability flags advertised by the Host; gates the
    /// organize-sheet verbs (restart/resume-agent/archive/notify).
    #[serde(default)]
    pub capabilities: SessionCapabilities,
}

/// A project/group from the Host's bootstrap, as the iOS `Project` model
/// carries it. Filing destinations for the session "Move to" verb are
/// derived from these with [`move_destinations`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(rename = "parentProjectID", default)]
    pub parent_project_id: Option<String>,
    #[serde(rename = "sortOrder", default)]
    pub sort_order: Option<i64>,
    // The Host's bootstrap names plain child groups `isGroup`; the iOS
    // model reads `isFolder`. Accept both wire spellings.
    #[serde(rename = "isFolder", default, alias = "isGroup")]
    pub is_folder: Option<bool>,
    #[serde(rename = "worktreeBranch", default)]
    pub worktree_branch: Option<String>,
}

impl ProjectSummary {
    /// Mirror of Swift's `Project.isWorktree`.
    pub fn is_worktree(&self) -> bool {
        self.worktree_branch.is_some() && self.parent_project_id.is_some()
    }

    /// Mirror of Swift's `Project.acceptsSessionDrop`: only plain
    /// organizational child groups accept a session filing.
    pub fn accepts_session_drop(&self) -> bool {
        self.parent_project_id.is_some()
            && self.worktree_branch.is_none()
            && self.is_folder == Some(true)
    }
}

/// "Move to" filing destinations for a session, mirroring Swift's
/// `SessionMoveRules.destinations`: the session's home project (nearest
/// non-plain-group ancestor) plus its plain child groups, in sidebar
/// order, minus the session's current location. Pure and portable — no
/// Host I/O.
pub fn move_destinations(
    session_project_id: &str,
    effective_project_id: &str,
    projects: &[ProjectSummary],
) -> Vec<ProjectSummary> {
    fn home_project_id(session_project_id: &str, projects: &[ProjectSummary]) -> String {
        let by_id: std::collections::HashMap<&str, &ProjectSummary> =
            projects.iter().map(|p| (p.id.as_str(), p)).collect();
        let mut id = session_project_id;
        let mut hops = 0;
        while let Some(project) = by_id.get(id) {
            if project.is_worktree() {
                break;
            }
            match project.parent_project_id.as_deref() {
                Some(parent) if hops < 16 => {
                    id = parent;
                    hops += 1;
                }
                _ => break,
            }
        }
        id.to_string()
    }

    let home_id = home_project_id(session_project_id, projects);
    let mut groups: Vec<&ProjectSummary> = projects
        .iter()
        .filter(|p| {
            p.parent_project_id.as_deref() == Some(home_id.as_str()) && p.accepts_session_drop()
        })
        .collect();
    groups.sort_by_key(|p| p.sort_order.unwrap_or(0));
    let mut destinations: Vec<ProjectSummary> = Vec::new();
    if let Some(home) = projects.iter().find(|p| p.id == home_id) {
        if home.id != effective_project_id {
            destinations.push(home.clone());
        }
    }
    for group in groups {
        if group.id != effective_project_id {
            destinations.push(group.clone());
        }
    }
    destinations
}

/// Role of a transcript entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptRole {
    User,
    Agent,
    System,
    Tool,
    #[default]
    #[serde(other)]
    Other,
}

/// One content block inside a transcript entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptBlock {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub text: Option<String>,
}

/// One entry of a session transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub id: String,
    #[serde(default)]
    pub role: TranscriptRole,
    #[serde(default)]
    pub blocks: Vec<TranscriptBlock>,
}

/// A snapshot (or page) of a session transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptSnapshot {
    #[serde(default)]
    pub entries: Vec<TranscriptEntry>,
    #[serde(rename = "hasMore", default)]
    pub has_more: bool,
}

/// An MCP approval prompt waiting for the user's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: String,
    #[serde(rename = "sessionID", default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

/// A launch preset row, mirroring Swift's `RemotePresetSummary` wire shape
/// (label/command/enabled/quickLaunch/isDefault; Mac-native cliID/pluginID/
/// tintColorHex are absent on non-Mac Hosts).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetSummary {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub command: String,
    #[serde(rename = "pluginID", default)]
    pub plugin_id: Option<String>,
    #[serde(rename = "projectID", default)]
    pub project_id: Option<String>,
    #[serde(rename = "cliID", default)]
    pub cli_id: Option<String>,
    /// Missing on older Hosts means enabled — a preset row must never be
    /// hidden just because the Host predates the field.
    #[serde(default = "preset_enabled_default")]
    pub enabled: bool,
    #[serde(rename = "quickLaunch", default)]
    pub quick_launch: bool,
    #[serde(rename = "isDefault", default)]
    pub is_default: bool,
    #[serde(rename = "tintColorHex", default)]
    pub tint_color_hex: Option<i64>,
}

fn preset_enabled_default() -> bool {
    true
}

/// The Host's bootstrap snapshot: everything a Controller needs to render.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BootstrapSnapshot {
    #[serde(rename = "protocolVersion", default)]
    pub protocol_version: i64,
    #[serde(rename = "hostProtocol", default)]
    pub host_protocol: Option<crate::protocol::HostProtocolDescriptor>,
    #[serde(rename = "macID", default)]
    pub host_id: Option<String>,
    #[serde(rename = "macName", default)]
    pub host_name: Option<String>,
    #[serde(default)]
    pub presets: Vec<PresetSummary>,
    #[serde(default)]
    pub sessions: Vec<SessionSummary>,
    /// Projects/groups from the Host's bootstrap; the "Move to" filing
    /// destinations are derived from these.
    #[serde(default)]
    pub projects: Vec<ProjectSummary>,
    #[serde(rename = "pendingApprovals", default)]
    pub pending_approvals: Vec<PendingApproval>,
    #[serde(rename = "capturedAtUnixMs", default)]
    pub captured_at_unix_ms: i64,
    #[serde(rename = "remoteServerPort", default)]
    pub remote_server_port: Option<i64>,
    #[serde(rename = "remoteServerCertificateFingerprint", default)]
    pub remote_server_certificate_fingerprint: Option<String>,
    #[serde(rename = "proEntitled", default)]
    pub pro_entitled: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_summary_decodes_minimal() {
        let s: SessionSummary = serde_json::from_value(serde_json::json!({
            "id": "abc",
            "status": "running",
            "activity": "working",
        }))
        .unwrap();
        assert_eq!(s.id, "abc");
        assert_eq!(s.activity, ActivityState::Working);
        assert!(!s.unread);
    }

    #[test]
    fn activity_state_future_proof() {
        let s: SessionSummary = serde_json::from_value(serde_json::json!({
            "id": "abc",
            "activity": "pondering",
        }))
        .unwrap();
        assert_eq!(s.activity, ActivityState::Unknown);
    }

    #[test]
    fn preset_decodes_host_wire_shape() {
        let p: PresetSummary = serde_json::from_value(serde_json::json!({
            "id": "p1",
            "label": "Claude",
            "command": "claude",
            "projectID": "proj-1",
            "enabled": true,
            "quickLaunch": false,
            "isDefault": false,
        }))
        .unwrap();
        assert_eq!(p.label, "Claude");
        assert_eq!(p.project_id.as_deref(), Some("proj-1"));
        assert!(p.enabled);
        assert_eq!(p.cli_id, None);
        assert_eq!(p.tint_color_hex, None);
    }

    #[test]
    fn preset_missing_enabled_means_enabled() {
        let p: PresetSummary = serde_json::from_value(serde_json::json!({
            "id": "p2",
            "label": "Old",
            "command": "old",
        }))
        .unwrap();
        assert!(
            p.enabled,
            "older Hosts predate the field; never hide the row"
        );
    }

    #[test]
    fn preset_decodes_mac_native_fields() {
        let p: PresetSummary = serde_json::from_value(serde_json::json!({
            "id": "p3",
            "label": "claude",
            "command": "claude",
            "cliID": "claude-code",
            "enabled": true,
            "tintColorHex": 0xD97757,
        }))
        .unwrap();
        assert_eq!(p.cli_id.as_deref(), Some("claude-code"));
        assert_eq!(p.tint_color_hex, Some(0xD97757));
    }

    #[test]
    fn bootstrap_decodes_minimal() {
        let b: BootstrapSnapshot = serde_json::from_value(serde_json::json!({
            "protocolVersion": 1,
            "sessions": [],
        }))
        .unwrap();
        assert!(b.sessions.is_empty());
        assert!(b.host_protocol.is_none());
    }

    fn test_projects() -> Vec<ProjectSummary> {
        vec![
            ProjectSummary {
                id: "home".into(),
                name: "Home".into(),
                path: "/home".into(),
                parent_project_id: None,
                sort_order: Some(0),
                is_folder: None,
                worktree_branch: None,
            },
            ProjectSummary {
                id: "group-a".into(),
                name: "Group A".into(),
                path: "/home/a".into(),
                parent_project_id: Some("home".into()),
                sort_order: Some(1),
                is_folder: Some(true),
                worktree_branch: None,
            },
            ProjectSummary {
                id: "group-b".into(),
                name: "Group B".into(),
                path: "/home/b".into(),
                parent_project_id: Some("home".into()),
                sort_order: Some(0),
                is_folder: Some(true),
                worktree_branch: None,
            },
            // Not a plain group (no isFolder): not a filing destination.
            ProjectSummary {
                id: "proj".into(),
                name: "Proj".into(),
                path: "/proj".into(),
                parent_project_id: Some("home".into()),
                sort_order: Some(2),
                is_folder: None,
                worktree_branch: None,
            },
            // Worktree child: never a filing destination.
            ProjectSummary {
                id: "wt".into(),
                name: "Worktree".into(),
                path: "/wt".into(),
                parent_project_id: Some("home".into()),
                sort_order: Some(3),
                is_folder: Some(true),
                worktree_branch: Some("feature".into()),
            },
        ]
    }

    #[test]
    fn move_destinations_home_plus_groups_in_sidebar_order() {
        let projects = test_projects();
        // Session lives in group-a; destinations are home + group-b (not
        // the current location, not non-groups, not worktrees), groups in
        // sortOrder.
        let dests = move_destinations("group-a", "group-a", &projects);
        let ids: Vec<&str> = dests.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["home", "group-b"]);
    }

    #[test]
    fn move_destinations_from_home_lists_groups() {
        let projects = test_projects();
        // Groups in sortOrder (group-b has sort_order 0, group-a has 1).
        let dests = move_destinations("home", "home", &projects);
        let ids: Vec<&str> = dests.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["group-b", "group-a"]);
    }

    #[test]
    fn project_summary_wire_shapes() {
        // The Host's bootstrap names plain groups `isGroup`.
        let p: ProjectSummary = serde_json::from_value(serde_json::json!({
            "id": "g",
            "name": "G",
            "path": "/g",
            "parentProjectID": "home",
            "sortOrder": 0,
            "isGroup": true,
        }))
        .unwrap();
        assert!(p.accepts_session_drop());
        assert!(!p.is_worktree());
    }
}
