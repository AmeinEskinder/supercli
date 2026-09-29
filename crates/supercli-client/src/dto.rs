//! Serde data-transfer objects for the Host `/mobile` API.
//!
//! Ports the `Remote*` structs in `RemoteControlProtocol.swift`. Every field
//! is optional or defaulted: the DTOs must decode bootstrap snapshots from
//! both older and newer Hosts without failing, and unknown JSON fields are
//! ignored. Additive protocol evolution must never break the client.

use serde::{Deserialize, Serialize};

use base64::Engine as _;

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
///
/// Mirrors `RemoteSessionCapabilities` in `RemoteControlProtocol.swift`,
/// including its wire-compat rules: the retired keys `fork` and
/// `appendSystemContext` are accepted (ignored) on decode so payloads from
/// older Hosts still parse, and are always encoded as `false` because
/// older Controllers require them when decoding a summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SessionCapabilities {
    /// Legacy terminal-replacing Resume operation.
    pub restart: bool,
    /// Legacy protocol-minor-5 field. Decoded so newer Controllers remain
    /// wire-compatible with older Hosts, but presentation must never use
    /// it: an active managed runtime is no longer a user-facing restart
    /// target.
    pub restart_agent: Option<bool>,
    /// The Host can safely resume the stable managed agent after it has
    /// exited back to the shell, without replacing the Session or PTY.
    /// `None` = older Host that predates the field; `Some(false)` = a
    /// current Host where the operation is unavailable for this session.
    pub resume_agent: Option<bool>,
    /// Whether the archive verb is offered (evidence-based).
    pub archive: bool,
    /// Whether notify-when-done is offered for this session.
    pub notify_when_done: bool,
}

impl Serialize for SessionCapabilities {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("SessionCapabilities", 7)?;
        s.serialize_field("restart", &self.restart)?;
        if let Some(v) = self.restart_agent {
            s.serialize_field("restartAgent", &v)?;
        }
        if let Some(v) = self.resume_agent {
            s.serialize_field("resumeAgent", &v)?;
        }
        // Older Controllers require these legacy keys when decoding a
        // summary (Swift's `encode(to:)` emits them as `false`).
        s.serialize_field("fork", &false)?;
        s.serialize_field("appendSystemContext", &false)?;
        s.serialize_field("notifyWhenDone", &self.notify_when_done)?;
        s.serialize_field("archive", &self.archive)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for SessionCapabilities {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            restart: bool,
            #[serde(rename = "restartAgent", default)]
            restart_agent: Option<bool>,
            #[serde(rename = "resumeAgent", default)]
            resume_agent: Option<bool>,
            // Decode-compatible tombstones. Retired actions remain accepted
            // from older Hosts but are not represented in the current
            // capability model.
            #[serde(default)]
            fork: Option<bool>,
            #[serde(rename = "appendSystemContext", default)]
            append_system_context: Option<bool>,
            #[serde(rename = "notifyWhenDone")]
            notify_when_done: bool,
            #[serde(default)]
            archive: bool,
        }
        let raw = Raw::deserialize(deserializer)?;
        let _ = (raw.fork, raw.append_system_context);
        Ok(SessionCapabilities {
            restart: raw.restart,
            restart_agent: raw.restart_agent,
            resume_agent: raw.resume_agent,
            archive: raw.archive,
            notify_when_done: raw.notify_when_done,
        })
    }
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
    /// The CLI's brand/spinner tint (0xRRGGBB), resolved on the Host.
    /// Nil = older Host or no per-tool brand color.
    #[serde(rename = "spinnerColorHex", default)]
    pub spinner_color_hex: Option<i32>,
    /// Latest persisted App alert body when it is the Session's newest activity.
    #[serde(rename = "latestAlertBody", default)]
    pub latest_alert_body: Option<String>,
    #[serde(rename = "latestAlertAtUnixMs", default)]
    pub latest_alert_at_unix_ms: Option<i64>,
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
    // The Host's sidebar and bootstrap send `isGroup` (sessions.rs);
    // Swift's RemoteControlProtocol also uses `isGroup`. Canonical wire
    // spelling is `isGroup`. No `isFolder` alias: no frozen legacy client
    // uses camelCase `isFolder` in this protocol (legacy macOS native
    // reads snake_case `is_folder` from a different store format).
    #[serde(rename = "isGroup", default)]
    pub is_group: Option<bool>,
    #[serde(rename = "worktreeBranch", default)]
    pub worktree_branch: Option<String>,
    /// Plain organizational child folder id. Optional for wire
    /// compatibility: older Hosts omit it.
    #[serde(rename = "folderID", default)]
    pub folder_id: Option<String>,
    /// Sidebar folder tint id (`sky`, `blue`, …), resolved by the Host.
    /// Optional so older Hosts and Controllers remain wire-compatible.
    #[serde(rename = "colorID", default)]
    pub color_id: Option<String>,
    /// Plain group pinned above the parent's ordinary mixed rows. Optional
    /// for protocol-minor compatibility; absent means unpinned.
    #[serde(default)]
    pub pinned: Option<bool>,
    /// Current git branch of the checkout (HEAD), for the session subtitle.
    #[serde(rename = "gitBranch", default)]
    pub git_branch: Option<String>,
    #[serde(rename = "mcpBlocked", default)]
    pub mcp_blocked: bool,
    /// How many archived sessions this project's archive library holds.
    /// Absent on older Hosts => hide the archive entry.
    #[serde(rename = "archivedSessionCount", default)]
    pub archived_session_count: Option<i64>,
    /// Whether this project's sessions are date-sorted (newest first)
    /// instead of the manual drag order. Absent = default custom order.
    #[serde(rename = "dateSorted", default)]
    pub date_sorted: Option<bool>,
    /// Mixed regular-section ranks from the Host's `session-order.json` —
    /// session ids interleaved with child group/worktree ids. Present only
    /// when that list actually contains a child folder; older Hosts omit it
    /// and Controllers keep folders above sessions.
    #[serde(rename = "sessionOrder", default)]
    pub session_order: Option<Vec<String>>,
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
            && self.is_group == Some(true)
    }

    /// Mirror of Swift's `RemoteProjectSummary.replacingSessionOrder`: a
    /// copy of this project with the session order replaced.
    pub fn replacing_session_order(&self, session_order: Option<Vec<String>>) -> Self {
        let mut copy = self.clone();
        copy.session_order = session_order;
        copy
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
///
/// Mirrors `RemotePendingApproval` in `RemoteControlProtocol.swift`.
/// `session_id` (the Rust-resolved presentation target) is additive on top
/// of the Swift wire shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingApproval {
    /// Stable prompt id, the target of `ApprovalAnswerRequest`.
    pub id: String,
    /// `"write" | "browser" | "computer"` today; free-form for new kinds.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub body: String,
    /// The session asking for access.
    #[serde(rename = "callerSessionID", default)]
    pub caller_session_id: String,
    /// Write approvals only: the session being written into.
    #[serde(
        rename = "targetSessionID",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub target_session_id: Option<String>,
    #[serde(rename = "requestedAtUnixMs", default)]
    pub requested_at_unix_ms: i64,
    /// Resolved presentation target (see [`PendingApproval::presentation_session_id`]).
    #[serde(rename = "sessionID", default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

impl PendingApproval {
    /// Mirror of Swift's `RemotePendingApproval.presentationSessionID`:
    /// the session that should show the in-pane prompt and the attention
    /// badge. Write grants present on the destination so the user sees
    /// where input would land; other kinds have no destination and present
    /// on the caller. A missing/unknown destination falls back to the
    /// caller.
    pub fn presentation_session_id(&self, known_ids: &std::collections::HashSet<String>) -> String {
        if let Some(target) = &self.target_session_id {
            if known_ids.contains(target) {
                return target.clone();
            }
        }
        self.caller_session_id.clone()
    }
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
    /// Project folders from the Host's bootstrap.
    #[serde(default)]
    pub folders: Vec<serde_json::Value>,
    #[serde(default)]
    pub presets: Vec<PresetSummary>,
    #[serde(default)]
    pub sessions: Vec<SessionSummary>,
    /// The workspace's current behavior knobs (additive, minor 10).
    #[serde(rename = "workspaceSettings", default)]
    pub workspace_settings: Option<serde_json::Value>,
    /// Complete official App catalog, including missing Apps (minor 15).
    #[serde(rename = "availableApps", default)]
    pub available_apps: Option<Vec<serde_json::Value>>,
    /// Live installed App subset (minor 15).
    #[serde(rename = "installedApps", default)]
    pub installed_apps: Option<Vec<serde_json::Value>>,
    /// Typed resource selector -> App/editor/system (minor 15).
    #[serde(default)]
    pub openers: Option<std::collections::HashMap<String, String>>,
    /// Semantic App/pane envelope (minor 15).
    #[serde(rename = "appPresentations", default)]
    pub app_presentations: Option<serde_json::Value>,
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
    /// Whether the Git worktrees feature is enabled (additive).
    #[serde(rename = "experimentalWorktreesEnabled", default)]
    pub experimental_worktrees_enabled: Option<bool>,
    /// The Host workspace's chrome tint hue in degrees (presentation only).
    #[serde(rename = "hostTintHue", default)]
    pub host_tint_hue: Option<f64>,
    /// Stable hardware family of the Host ("macbook" | "linux" | ...).
    #[serde(rename = "hostDeviceKind", default)]
    pub host_device_kind: Option<String>,
    /// Host device model string.
    #[serde(rename = "hostDeviceModel", default)]
    pub host_device_model: Option<String>,
}

/// A device paired with the Host.
///
/// Mirrors `RemotePairedDeviceSummary` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDeviceSummary {
    pub id: String,
    pub name: String,
    pub platform: String,
    #[serde(rename = "appVersion", default)]
    pub app_version: Option<String>,
    #[serde(rename = "pairedAtUnixMs")]
    pub paired_at_unix_ms: i64,
    #[serde(rename = "lastSeenAtUnixMs", default)]
    pub last_seen_at_unix_ms: Option<i64>,
    /// Whether this device may reach the Host over the Supercli Link relay.
    /// Nil means allowed (pre-flag records) — the flag only ever narrows.
    #[serde(rename = "relayAllowed", default)]
    pub relay_allowed: Option<bool>,
}

/// A workspace on the connected Host.
///
/// Mirrors `RemoteWorkspaceSummary` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    /// Stable workspace id — the registry UUID, or a stable key for the
    /// default / current instance.
    pub id: String,
    pub name: String,
    /// That workspace's App color hue in degrees (nil = neutral default).
    #[serde(rename = "tintHue", default)]
    pub tint_hue: Option<f64>,
    /// True for the workspace THIS connected app instance is.
    #[serde(rename = "isCurrent")]
    pub is_current: bool,
    /// Whether that workspace's app instance is currently running.
    #[serde(rename = "isRunning")]
    pub is_running: bool,
    /// What the entry is on the connected Host: "local", "ssh", or "paired".
    /// Additive and optional: older Hosts omit it, nil decodes as "local".
    #[serde(default)]
    pub kind: Option<String>,
}

impl WorkspaceSummary {
    /// The effective kind, defaulting to "local" for older Hosts.
    pub fn effective_kind(&self) -> &str {
        self.kind.as_deref().unwrap_or("local")
    }
}

/// Progress of a resumable artifact upload.
///
/// Mirrors `RemoteArtifactUploadProgress` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactUploadProgress {
    #[serde(rename = "uploadID")]
    pub upload_id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "mimeType", default)]
    pub mime_type: Option<String>,
    #[serde(rename = "totalBytes")]
    pub total_bytes: i64,
    #[serde(rename = "receivedBytes")]
    pub received_bytes: i64,
    #[serde(rename = "chunkSize")]
    pub chunk_size: i64,
    #[serde(rename = "nextOffset")]
    pub next_offset: i64,
    pub complete: bool,
    #[serde(rename = "artifactID", default)]
    pub artifact_id: Option<String>,
    #[serde(rename = "updatedAtUnixMs")]
    pub updated_at_unix_ms: i64,
}

/// Request to create a new session.
///
/// Mirrors `RemoteCreateSessionRequest` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(rename = "projectID")]
    pub project_id: String,
    #[serde(rename = "presetID", default)]
    pub preset_id: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(rename = "worktreePath", default)]
    pub worktree_path: Option<String>,
    #[serde(rename = "worktreeBranch", default)]
    pub worktree_branch: Option<String>,
    #[serde(rename = "initialText", default)]
    pub initial_text: Option<String>,
    #[serde(rename = "initialTextSubmitMode", default)]
    pub initial_text_submit_mode: TextSubmitMode,
}

/// How text is submitted to a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextSubmitMode {
    #[default]
    PasteAndSubmit,
    PasteOnly,
    TypeAndSubmit,
}

/// Response to a session creation request.
///
/// Mirrors `RemoteCreateSessionResponse` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateSessionResponse {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "capturedAtUnixMs", default)]
    pub captured_at_unix_ms: Option<i64>,
    /// Present on newer Hosts so the client can render the starting
    /// session immediately instead of waiting for the next bootstrap poll.
    #[serde(default)]
    pub session: Option<SessionSummary>,
}

/// Text input to a session.
///
/// Mirrors `RemoteSessionTextInput` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTextInput {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub text: String,
    #[serde(rename = "submitMode")]
    pub submit_mode: TextSubmitMode,
}

/// A terminal write request.
///
/// Mirrors `RemoteTerminalWriteRequest` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalWriteRequest {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    /// Base64-encoded terminal input data.
    pub data: String,
    /// Optional idempotency key for one logical input send.
    /// Wire key is `wid`, matching the Host (controller_api.rs,
    /// remote_server.rs) and Swift's `writeID` CodingKey.
    #[serde(rename = "wid", default)]
    pub idempotency_key: Option<String>,
}

/// A terminal resize request.
///
/// Mirrors `RemoteTerminalResizeRequest` in `RemoteControlProtocol.swift`.
/// Wire field is `columns`, matching the Host (`controller_api.rs`
/// `resize_session` reads `columns`) and Swift.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalResizeRequest {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub columns: i64,
    pub rows: i64,
}

/// A terminal color.
///
/// Mirrors `RemoteTerminalColor` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalColor {
    pub kind: TerminalColorKind,
    #[serde(default)]
    pub index: Option<u8>,
    #[serde(default)]
    pub red: Option<u8>,
    #[serde(default)]
    pub green: Option<u8>,
    #[serde(default)]
    pub blue: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalColorKind {
    DefaultForeground,
    DefaultBackground,
    Ansi,
    Rgb,
}

impl TerminalColor {
    pub fn default_foreground() -> Self {
        Self {
            kind: TerminalColorKind::DefaultForeground,
            index: None,
            red: None,
            green: None,
            blue: None,
        }
    }

    pub fn default_background() -> Self {
        Self {
            kind: TerminalColorKind::DefaultBackground,
            index: None,
            red: None,
            green: None,
            blue: None,
        }
    }

    pub fn ansi(index: u8) -> Self {
        Self {
            kind: TerminalColorKind::Ansi,
            index: Some(index),
            red: None,
            green: None,
            blue: None,
        }
    }

    pub fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            kind: TerminalColorKind::Rgb,
            index: None,
            red: Some(red),
            green: Some(green),
            blue: Some(blue),
        }
    }
}

/// Terminal text style.
///
/// Mirrors `RemoteTerminalStyle` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TerminalStyle {
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub inverse: bool,
    #[serde(default)]
    pub dim: bool,
    #[serde(default)]
    pub strikethrough: bool,
}

/// A single terminal cell.
///
/// Mirrors `RemoteTerminalCell` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalCell {
    pub text: String,
    #[serde(default)]
    pub foreground: Option<TerminalColor>,
    #[serde(default)]
    pub background: Option<TerminalColor>,
    pub style: TerminalStyle,
}

/// A terminal cursor.
///
/// Mirrors `RemoteTerminalCursor` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalCursor {
    pub row: i64,
    pub column: i64,
    pub shape: CursorShape,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CursorShape {
    Block,
    Beam,
    Underline,
    Hidden,
}

/// A full viewport frame.
///
/// Mirrors `RemoteViewportFrame` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewportFrame {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub sequence: u64,
    pub rows: i64,
    pub columns: i64,
    pub cells: Vec<TerminalCell>,
    #[serde(default)]
    pub cursor: Option<TerminalCursor>,
    #[serde(rename = "alternateScreen", default)]
    pub alternate_screen: bool,
    #[serde(rename = "capturedAtUnixMs")]
    pub captured_at_unix_ms: i64,
}

/// A viewport subscription request.
///
/// Mirrors `RemoteViewportSubscription` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewportSubscription {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub rows: i64,
    pub columns: i64,
}

/// A run of cells with the same style (for viewport patches).
///
/// Mirrors `RemoteTerminalCellRun` in `RemoteControlProtocol.swift`.
/// Wire field is `column`, matching Swift. (The Host does not currently
/// emit viewport patches; Swift is the reference implementation.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalCellRun {
    pub row: i64,
    pub column: i64,
    pub cells: Vec<TerminalCell>,
}

/// A viewport patch (incremental update).
///
/// Mirrors `RemoteViewportPatch` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewportPatch {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub sequence: u64,
    #[serde(rename = "baseSequence")]
    pub base_sequence: u64,
    #[serde(rename = "changedRuns", default)]
    pub changed_runs: Vec<TerminalCellRun>,
    #[serde(default)]
    pub cursor: Option<TerminalCursor>,
    #[serde(rename = "capturedAtUnixMs")]
    pub captured_at_unix_ms: i64,
}

/// Kind of a stream event.
///
/// Mirrors `RemoteStreamEventKind` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamEventKind {
    BootstrapSnapshot,
    SessionsChanged,
    ProjectsChanged,
    TranscriptSnapshot,
    TranscriptChunk,
    ViewportFrame,
    ViewportPatch,
    InputAccepted,
    InputRejected,
    DeviceRevoked,
    Heartbeat,
    Error,
}

/// A generic stream event envelope.
///
/// Mirrors `RemoteStreamEvent` in `RemoteControlProtocol.swift`.
/// Note: This is distinct from `SessionEventWire` in `events.rs`, which is
/// the newer typed session-event stream (Phase 6 R4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamEvent {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: i64,
    pub id: String,
    #[serde(rename = "requestID", default)]
    pub request_id: Option<String>,
    pub kind: StreamEventKind,
    #[serde(rename = "sessionID", default)]
    pub session_id: Option<String>,
    /// Binary payload (base64 in JSON). The concrete type depends on `kind`:
    /// e.g. `ViewportFrame` for `ViewportFrame`, `TranscriptSnapshot` for
    /// `TranscriptSnapshot`.
    #[serde(default, with = "serde_bytes_option")]
    pub payload: Option<Vec<u8>>,
    #[serde(rename = "createdAtUnixMs")]
    pub created_at_unix_ms: i64,
}

/// Answer a pending MCP approval prompt from a controller.
///
/// Mirrors `RemoteApprovalAnswerRequest` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalAnswerRequest {
    pub id: String,
    pub approved: bool,
}

/// Tell the Host the client opened/observed a session, clearing unread.
///
/// Mirrors `RemoteMarkReadRequest` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkReadRequest {
    #[serde(rename = "sessionID")]
    pub session_id: String,
}

/// A session action (stop, restart, remove, etc.).
///
/// Mirrors `RemoteSessionAction` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionAction {
    /// Kill the hosted PTY but keep the session row/history restartable.
    Stop,
    /// Re-run the original command with the desktop resume behavior.
    Restart,
    /// Legacy protocol-minor-5 action. Kept only so newer Hosts can decode
    /// requests from older Controllers.
    #[serde(rename = "restart_agent")]
    RestartAgent,
    /// Resume an ended managed agent inside its still-live terminal.
    #[serde(rename = "resume_agent")]
    ResumeAgent,
    /// Remove the session row and delete its on-disk artifacts.
    Remove,
}

/// Request to perform a session action.
///
/// Mirrors `RemoteSessionActionRequest` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionActionRequest {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub action: SessionAction,
}

/// Patch for session organization (title, pin, archive, project).
///
/// Mirrors `RemoteSessionOrganizationPatch` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionOrganizationPatch {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub archived: Option<bool>,
    #[serde(rename = "notifyWhenDone", default)]
    pub notify_when_done: Option<bool>,
    #[serde(rename = "projectID", default)]
    pub project_id: Option<String>,
}

/// Request a screenshot of a session.
///
/// Mirrors `RemoteScreenshotRequest` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenshotRequest {
    #[serde(rename = "sessionID")]
    pub session_id: String,
}

/// Response to a screenshot request.
///
/// Mirrors `RemoteScreenshotRequestResponse` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenshotRequestResponse {
    pub accepted: bool,
    #[serde(rename = "requestedAtUnixMs")]
    pub requested_at_unix_ms: i64,
}

/// A plugin update.
///
/// Mirrors `RemotePluginUpdate` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginUpdate {
    pub id: String,
    pub state: String,
    #[serde(rename = "installedVersion", default)]
    pub installed_version: Option<String>,
    #[serde(rename = "latestVersion", default)]
    pub latest_version: Option<String>,
    #[serde(rename = "updateAvailable")]
    pub update_available: bool,
}

/// Plugin updates status.
///
/// Mirrors `RemotePluginUpdates` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginUpdates {
    pub checking: bool,
    #[serde(default)]
    pub items: Vec<PluginUpdate>,
}

/// Request to restart a session.
///
/// Mirrors `RemoteRestartSessionRequest` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestartSessionRequest {
    #[serde(rename = "sessionID")]
    pub session_id: String,
}

/// Push token registration.
///
/// Mirrors `RemotePushTokenRegistration` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushTokenRegistration {
    pub token: String,
    pub platform: String,
    #[serde(rename = "appVersion", default)]
    pub app_version: Option<String>,
}

/// Serde helper for `Option<Vec<u8>>` as base64.
mod serde_bytes_option {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(bytes) => s.serialize_some(&base64_encode(bytes)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let opt: Option<String> = Option::deserialize(d)?;
        opt.map(|s| base64_decode(&s).map_err(serde::de::Error::custom))
            .transpose()
    }

    fn base64_encode(bytes: &[u8]) -> String {
        // Simple base64 implementation to avoid extra deps.
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let mut buf = [0u8; 3];
            buf[..chunk.len()].copy_from_slice(chunk);
            let n = (buf[0] as u32) << 16 | (buf[1] as u32) << 8 | buf[2] as u32;
            out.push(CHARS[((n >> 18) & 63) as usize] as char);
            out.push(CHARS[((n >> 12) & 63) as usize] as char);
            out.push(if chunk.len() > 1 {
                CHARS[((n >> 6) & 63) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                CHARS[(n & 63) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
        // Simple base64 decoder.
        let mut out = Vec::new();
        let mut buf = 0u32;
        let mut bits = 0;
        for c in s.chars() {
            if c == '=' {
                break;
            }
            let v = match c {
                'A'..='Z' => c as u32 - 'A' as u32,
                'a'..='z' => c as u32 - 'a' as u32 + 26,
                '0'..='9' => c as u32 - '0' as u32 + 52,
                '+' => 62,
                '/' => 63,
                _ => return Err(format!("invalid base64 char: {c}")),
            };
            buf = (buf << 6) | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buf >> bits) as u8);
                buf &= (1 << bits) - 1;
            }
        }
        Ok(out)
    }
}

/// One plugin's activation change. Mirrors Swift `RemotePluginActivationPatch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginActivationPatch {
    pub id: String,
    pub active: bool,
}

/// Patch for the Host's workspace settings. Mirrors Swift
/// `RemoteWorkspaceSettingsPatch`. All fields are optional; only the set
/// ones are applied.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSettingsPatch {
    #[serde(
        rename = "pluginOrder",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub plugin_order: Option<Vec<String>>,
    #[serde(
        rename = "pluginActivation",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub plugin_activation: Option<PluginActivationPatch>,
    #[serde(
        rename = "transcriptSettings",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub transcript_settings: Option<TranscriptSettingsUpdate>,
    #[serde(
        rename = "appearanceSettings",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub appearance_settings: Option<AppearanceSettingsUpdate>,
    #[serde(
        rename = "notificationSettings",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub notification_settings: Option<NotificationSettingsUpdate>,
    #[serde(
        rename = "experimentalSettings",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub experimental_settings: Option<ExperimentalSettingsUpdate>,
    #[serde(
        rename = "autoStopArchiveMinutes",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub auto_stop_archive_minutes: Option<i64>,
    #[serde(
        rename = "sidebarStoppedLimit",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub sidebar_stopped_limit: Option<i64>,
    #[serde(
        rename = "browserDefaultAccess",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub browser_default_access: Option<String>,
    #[serde(
        rename = "mcpNonchildWriteAccess",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub mcp_nonchild_write_access: Option<String>,
    #[serde(
        rename = "computerAccess",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub computer_access: Option<String>,
    #[serde(
        rename = "mcpWorktreeAccess",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub mcp_worktree_access: Option<bool>,
    #[serde(
        rename = "mcpAutoAddBrowserScreenshots",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub mcp_auto_add_browser_screenshots: Option<bool>,
}

impl WorkspaceSettingsPatch {
    /// Mirror of Swift's `RemoteWorkspaceSettingsPatch.isEmpty`: true when
    /// no setting is being changed.
    pub fn is_empty(&self) -> bool {
        self.plugin_order.is_none()
            && self.plugin_activation.is_none()
            && self.transcript_settings.is_none()
            && self.appearance_settings.is_none()
            && self.notification_settings.is_none()
            && self.experimental_settings.is_none()
            && self.auto_stop_archive_minutes.is_none()
            && self.sidebar_stopped_limit.is_none()
            && self.browser_default_access.is_none()
            && self.mcp_nonchild_write_access.is_none()
            && self.computer_access.is_none()
            && self.mcp_worktree_access.is_none()
            && self.mcp_auto_add_browser_screenshots.is_none()
    }
}

/// One-project organization patch. Mirrors Swift
/// `RemoteProjectOrganizationPatch`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectOrganizationPatch {
    #[serde(rename = "projectID")]
    pub project_id: String,
    #[serde(rename = "folderID", default, skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<String>,
    #[serde(rename = "sortOrder", default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i64>,
    #[serde(
        rename = "displayName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub display_name: Option<String>,
    #[serde(rename = "colorID", default, skip_serializing_if = "Option::is_none")]
    pub color_id: Option<String>,
    #[serde(
        rename = "dateSorted",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub date_sorted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
}

/// One-preset edit patch. Mirrors Swift `RemotePresetPatch`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PresetPatch {
    #[serde(rename = "presetID", default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(
        rename = "quickLaunch",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub quick_launch: Option<bool>,
    #[serde(rename = "sortOrder", default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<bool>,
}

/// The Host's receipt for a created session. Mirrors Swift
/// `NativeRemoteCreatedSession`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatedSession {
    #[serde(rename = "requestID", default)]
    pub request_id: u64,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "capturedAtUnixMs", default)]
    pub captured_at_unix_ms: Option<i64>,
    #[serde(default)]
    pub session: Option<SessionSummary>,
}

// MARK: - RemoteControlProtocol batch 2 (porter T)
//
// Remaining `Remote*` types from `RemoteControlProtocol.swift` not covered by
// the first DTO pass. Wire rules: fields Swift declares non-optional are
// REQUIRED (no `#[serde(default)]`) so wire drift surfaces as a decode error
// instead of a silent zero value; Swift `Optional` fields are `Option<T>`
// with `#[serde(default)]`. Unknown JSON fields are ignored.

/// A plain organizational folder in the project tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectFolderSummary {
    pub id: String,
    pub name: String,
    #[serde(rename = "parentFolderID", default)]
    pub parent_folder_id: Option<String>,
    #[serde(rename = "colorID", default)]
    pub color_id: Option<String>,
    #[serde(rename = "sortOrder", default)]
    pub sort_order: Option<i32>,
}

/// GET /mobile/archive?project_id= — one project's archived sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchivedSessionsResponse {
    #[serde(rename = "projectID")]
    pub project_id: String,
    pub sessions: Vec<SessionSummary>,
}

/// The kind of a transcript content block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TranscriptBlockKind {
    Text,
    Reasoning,
    ToolCall,
    ToolResult,
    Permission,
    Info,
    FileChange,
    Diff,
    PlanUpdate,
    Usage,
    Attachment,
}

/// One offset-addressed slice of a live transcript stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptStreamChunk {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "providerID", default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    pub resolved: bool,
    pub offset: u64,
    #[serde(rename = "nextOffset")]
    pub next_offset: u64,
    pub partial: String,
    pub truncated: bool,
    pub entries: Vec<TranscriptEntry>,
    #[serde(rename = "fallbackReason", default)]
    pub fallback_reason: Option<String>,
    #[serde(rename = "updatedAtUnixMs")]
    pub updated_at_unix_ms: i64,
}

/// One page of transcript history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptHistoryPage {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "providerID", default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    pub resolved: bool,
    #[serde(rename = "startOffset")]
    pub start_offset: u64,
    #[serde(rename = "endOffset")]
    pub end_offset: u64,
    pub truncated: bool,
    pub entries: Vec<TranscriptEntry>,
    #[serde(rename = "fallbackReason", default)]
    pub fallback_reason: Option<String>,
    #[serde(rename = "updatedAtUnixMs")]
    pub updated_at_unix_ms: i64,
}

/// A group of panes shown together.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneGroupSummary {
    pub id: String,
    #[serde(rename = "representativeSessionID")]
    pub representative_session_id: String,
    #[serde(rename = "sessionIDs")]
    pub session_ids: Vec<String>,
}

/// One official App advertised by the Host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub tint: Option<String>,
    #[serde(rename = "iconSvg", default)]
    pub icon_svg: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(rename = "installedVersion", default)]
    pub installed_version: Option<String>,
    #[serde(rename = "updateAvailable")]
    pub update_available: bool,
    #[serde(rename = "installCommand", default)]
    pub install_command: Option<String>,
    pub command: String,
    #[serde(rename = "mediaTypes")]
    pub media_types: Vec<String>,
    #[serde(rename = "fileExtensions")]
    pub file_extensions: std::collections::HashMap<String, String>,
    #[serde(rename = "resourceKinds")]
    pub resource_kinds: Vec<String>,
    #[serde(rename = "defaultFor")]
    pub default_for: Vec<String>,
    pub installed: bool,
}

impl AppSummary {
    /// Whether this App handles a `file:<media-type>` or
    /// `resource:<kind>` selector. Mirrors Swift
    /// `RemoteAppSummary.handles(selector:)`.
    pub fn handles(&self, selector: &str) -> bool {
        if let Some(media_type) = selector.strip_prefix("file:") {
            if media_type.is_empty() {
                return false;
            }
            return self
                .media_types
                .iter()
                .any(|m| m.eq_ignore_ascii_case(media_type));
        }
        if let Some(kind) = selector.strip_prefix("resource:") {
            if kind.is_empty() {
                return false;
            }
            return self
                .resource_kinds
                .iter()
                .any(|k| k.eq_ignore_ascii_case(kind));
        }
        false
    }

    /// The media type for a file path, from the first App whose
    /// `fileExtensions` map contains the path's extension. Mirrors Swift
    /// `RemoteAppSummary.mediaType(forPath:in:)`.
    pub fn media_type_for_path(path: &str, apps: &[AppSummary]) -> Option<String> {
        let ext = std::path::Path::new(path)
            .extension()?
            .to_str()?
            .to_lowercase();
        if ext.is_empty() {
            return None;
        }
        apps.iter()
            .find_map(|a| a.file_extensions.get(&ext).cloned())
    }
}

/// Host-owned semantic App companions and their caller-relative reveal
/// intents. Nested coding keys match the app-state envelope (`app_id`,
/// `companion_session_id`, …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppPresentationsFile {
    pub version: i32,
    pub instances: Vec<AppPresentationInstance>,
    pub presentations: Vec<AppPresentation>,
}

/// One App companion instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppPresentationInstance {
    pub id: String,
    #[serde(rename = "app_id")]
    pub app_id: String,
    #[serde(rename = "companion_session_id")]
    pub companion_session_id: String,
}

/// One caller-relative reveal intent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppPresentation {
    pub id: String,
    #[serde(rename = "caller_session_id")]
    pub caller_session_id: String,
    #[serde(rename = "instance_id")]
    pub instance_id: String,
    pub target: String,
    #[serde(rename = "reveal_revision")]
    pub reveal_revision: u64,
}

/// What the entry is on the connected Host: "local", "ssh", or "paired".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostEnvironment {
    pub kind: String,
    pub id: String,
}

impl HostEnvironment {
    /// A short host-row label, e.g. "Box · bx_1a2b…". Mirrors Swift
    /// `RemoteHostEnvironment.rowLabel`.
    pub fn row_label(&self) -> String {
        let short_id: String = if self.id.chars().count() > 10 {
            format!("{}…", self.id.chars().take(9).collect::<String>())
        } else {
            self.id.clone()
        };
        if self.kind == "box" {
            format!("Box · {short_id}")
        } else {
            format!("{} · {short_id}", self.kind)
        }
    }
}

/// A controller asking a connected Host to serve a different local
/// workspace over the same connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSelectRequest {
    #[serde(rename = "workspaceId")]
    pub workspace_id: String,
}

/// Acknowledgement of a workspace switch, echoing the selected workspace.
/// `workspace` is required: Swift's `RemoteWorkspaceSelectResponse` declares
/// it non-optional with a synthesized decoder, so a missing/null value is
/// a wire error, not an empty selection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSelectResponse {
    pub workspace: WorkspaceSummary,
}

/// Controller → Host: resize a desktop session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesktopResizeRequest {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(default)]
    pub columns: Option<i32>,
    #[serde(default)]
    pub rows: Option<i32>,
    #[serde(default)]
    pub clear: Option<bool>,
}

/// Named keys a controller can send to a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyName {
    Enter,
    Escape,
    Tab,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ControlC,
    ControlD,
    ControlZ,
}

/// Controller → Host: send named keys to a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionKeyInput {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub keys: Vec<KeyName>,
}

/// Terminal dimensions and view state for one session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalMetrics {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub columns: i32,
    pub rows: i32,
    #[serde(rename = "capturedAtUnixMs")]
    pub captured_at_unix_ms: i64,
    /// Whether the desktop app is actively viewing this session.
    /// Nil on older Macs.
    #[serde(rename = "desktopViewing", default)]
    pub desktop_viewing: Option<bool>,
}

/// One offset-addressed read of a session's terminal tail.
///
/// Mirrors `RemoteTerminalOutputChunk` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalOutputChunk {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub offset: u64,
    #[serde(rename = "nextOffset")]
    pub next_offset: u64,
    #[serde(rename = "dataBase64")]
    pub data_base64: String,
    #[serde(default)]
    pub truncated: bool,
    #[serde(rename = "capturedAtUnixMs")]
    pub captured_at_unix_ms: i64,
    /// DEC-mode restore preamble (base64) for a fresh tail read: the
    /// sequences that established mouse tracking, alt screen, bracketed
    /// paste, … usually precede the retained tail, so a client that resets
    /// its VT before feeding this chunk feeds these bytes first. Not
    /// journal bytes — never part of `offset`/`nextOffset`. Absent on
    /// older Hosts and whenever there is nothing to restore.
    #[serde(
        rename = "modePreambleBase64",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub mode_preamble_base64: Option<String>,
}

impl TerminalOutputChunk {
    /// Mirror of Swift's `RemoteTerminalOutputChunk.modePreamble`: the
    /// decoded preamble bytes, or `None` when absent, not valid base64, or
    /// empty (Swift's `Data(base64Encoded:)` returns nil for all three).
    pub fn mode_preamble(&self) -> Option<Vec<u8>> {
        let encoded = self.mode_preamble_base64.as_deref()?;
        let stripped: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
        if stripped.is_empty() {
            return None;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&stripped)
            .ok()?;
        if bytes.is_empty() {
            None
        } else {
            Some(bytes)
        }
    }
}

/// One file the browser MCP produced for a session — a screenshot or a
/// download. Metadata only; bytes are fetched via [`BrowserArtifactChunk`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BrowserArtifact {
    /// `"screenshots"` or `"downloads"` — also the on-disk subdirectory.
    pub kind: String,
    pub name: String,
    pub size: u64,
    #[serde(rename = "modifiedAtUnixMs")]
    pub modified_at_unix_ms: i64,
}

/// The gallery listing for one session, newest-first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserArtifactList {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub artifacts: Vec<BrowserArtifact>,
    #[serde(rename = "capturedAtUnixMs")]
    pub captured_at_unix_ms: i64,
}

/// One offset-addressed slice of an artifact's bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserArtifactChunk {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub kind: String,
    pub name: String,
    #[serde(rename = "contentType")]
    pub content_type: String,
    pub offset: u64,
    #[serde(rename = "nextOffset")]
    pub next_offset: u64,
    #[serde(rename = "totalSize")]
    pub total_size: u64,
    #[serde(rename = "dataBase64")]
    pub data_base64: String,
    #[serde(rename = "capturedAtUnixMs")]
    pub captured_at_unix_ms: i64,
}

/// Patch for the Host-advertised transcript rendering values.
/// Nil fields are left unchanged.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TranscriptSettingsUpdate {
    #[serde(rename = "includeUser", default)]
    pub include_user: Option<bool>,
    #[serde(rename = "includeAssistant", default)]
    pub include_assistant: Option<bool>,
    #[serde(rename = "includeReasoning", default)]
    pub include_reasoning: Option<bool>,
    #[serde(rename = "includeTools", default)]
    pub include_tools: Option<bool>,
    #[serde(rename = "includeFileChanges", default)]
    pub include_file_changes: Option<bool>,
    #[serde(rename = "includePlanUpdates", default)]
    pub include_plan_updates: Option<bool>,
    #[serde(rename = "includeSessionInfo", default)]
    pub include_session_info: Option<bool>,
    #[serde(rename = "maxEntries", default)]
    pub max_entries: Option<i32>,
}

/// The Host-advertised transcript rendering values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptSettings {
    #[serde(rename = "includeUser")]
    pub include_user: bool,
    #[serde(rename = "includeAssistant")]
    pub include_assistant: bool,
    #[serde(rename = "includeReasoning")]
    pub include_reasoning: bool,
    #[serde(rename = "includeTools")]
    pub include_tools: bool,
    #[serde(rename = "includeFileChanges")]
    pub include_file_changes: bool,
    #[serde(rename = "includePlanUpdates")]
    pub include_plan_updates: bool,
    #[serde(rename = "includeSessionInfo")]
    pub include_session_info: bool,
    #[serde(rename = "maxEntries")]
    pub max_entries: i32,
}

/// Patch for appearance values owned by the Host workspace.
/// Nil fields are left unchanged.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AppearanceSettingsUpdate {
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(rename = "appTint", default)]
    pub app_tint: Option<String>,
    #[serde(rename = "backgroundOpacity", default)]
    pub background_opacity: Option<f64>,
    #[serde(rename = "surfaceOpacity", default)]
    pub surface_opacity: Option<f64>,
    #[serde(rename = "backgroundTone", default)]
    pub background_tone: Option<f64>,
    #[serde(rename = "surfaceTone", default)]
    pub surface_tone: Option<f64>,
    #[serde(rename = "sessionTitleMode", default)]
    pub session_title_mode: Option<String>,
}

/// Appearance values owned by the Host workspace but rendered by
/// whichever Controller is currently scoped to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppearanceSettings {
    pub theme: String,
    #[serde(rename = "appTint")]
    pub app_tint: String,
    #[serde(rename = "backgroundOpacity")]
    pub background_opacity: f64,
    #[serde(rename = "surfaceOpacity")]
    pub surface_opacity: f64,
    #[serde(rename = "backgroundTone")]
    pub background_tone: f64,
    #[serde(rename = "surfaceTone")]
    pub surface_tone: f64,
    #[serde(rename = "sessionTitleMode")]
    pub session_title_mode: String,
}

/// Patch for Host-owned attention behavior.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NotificationSettingsUpdate {
    #[serde(rename = "menuAttentionDetection", default)]
    pub menu_attention_detection: Option<bool>,
}

/// Host-owned attention behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationSettings {
    #[serde(rename = "menuAttentionDetection")]
    pub menu_attention_detection: bool,
}

/// Patch for Host feature toggles (Settings ▸ Features).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ExperimentalSettingsUpdate {
    #[serde(default)]
    pub worktrees: Option<bool>,
    #[serde(rename = "sessionsMcp", default)]
    pub sessions_mcp: Option<bool>,
    #[serde(rename = "browserMcp", default)]
    pub browser_mcp: Option<bool>,
    #[serde(rename = "computerUse", default)]
    pub computer_use: Option<bool>,
    #[serde(default)]
    pub workspaces: Option<bool>,
}

/// Host feature toggles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExperimentalSettings {
    pub worktrees: bool,
    #[serde(rename = "sessionsMcp")]
    pub sessions_mcp: bool,
    #[serde(rename = "browserMcp")]
    pub browser_mcp: bool,
    #[serde(rename = "computerUse")]
    pub computer_use: bool,
    /// Nil is an older Host and must not be guessed from hardware kind.
    #[serde(rename = "computerUseAvailable", default)]
    pub computer_use_available: Option<bool>,
    #[serde(rename = "computerUseReady", default)]
    pub computer_use_ready: Option<bool>,
    #[serde(rename = "computerUseUnavailableReason", default)]
    pub computer_use_unavailable_reason: Option<String>,
    pub workspaces: bool,
}

/// The selected Host's agent inventory; no Controller-side PATH guesses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub command: String,
    pub installed: bool,
    #[serde(rename = "installCommand", default)]
    pub install_command: Option<String>,
    #[serde(rename = "websiteURL", default)]
    pub website_url: Option<String>,
    #[serde(rename = "integrationInstallable", default)]
    pub integration_installable: Option<bool>,
    #[serde(rename = "integrationInstalled", default)]
    pub integration_installed: Option<bool>,
    #[serde(rename = "integrationSummary", default)]
    pub integration_summary: Option<String>,
    #[serde(rename = "integrationManualCommand", default)]
    pub integration_manual_command: Option<String>,
}

/// Host → Controller: the workspace's current Host-owned settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSettings {
    #[serde(rename = "pluginOrder", default)]
    pub plugin_order: Option<Vec<String>>,
    #[serde(rename = "pluginActivation", default)]
    pub plugin_activation: Option<std::collections::HashMap<String, bool>>,
    #[serde(rename = "availableAgents", default)]
    pub available_agents: Option<Vec<AgentSummary>>,
    #[serde(rename = "mcpShimPath", default)]
    pub mcp_shim_path: Option<String>,
    #[serde(rename = "transcriptSettings", default)]
    pub transcript_settings: Option<TranscriptSettings>,
    #[serde(rename = "appearanceSettings", default)]
    pub appearance_settings: Option<AppearanceSettings>,
    #[serde(rename = "notificationSettings", default)]
    pub notification_settings: Option<NotificationSettings>,
    #[serde(rename = "experimentalSettings", default)]
    pub experimental_settings: Option<ExperimentalSettings>,
    #[serde(rename = "autoStopArchiveMinutes")]
    pub auto_stop_archive_minutes: i64,
    #[serde(rename = "sidebarStoppedLimit")]
    pub sidebar_stopped_limit: i64,
    #[serde(rename = "browserDefaultAccess")]
    pub browser_default_access: String,
    #[serde(rename = "mcpNonchildWriteAccess")]
    pub mcp_nonchild_write_access: String,
    #[serde(rename = "computerAccess")]
    pub computer_access: String,
    #[serde(rename = "mcpWorktreeAccess")]
    pub mcp_worktree_access: bool,
    #[serde(rename = "mcpAutoAddBrowserScreenshots")]
    pub mcp_auto_add_browser_screenshots: bool,
}

/// Controller → Host: replace one project's hand-ordered sidebar session
/// ranks (capability `session.order.set`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionOrderRequest {
    #[serde(rename = "projectID")]
    pub project_id: String,
    #[serde(rename = "orderedSessionIDs")]
    pub ordered_session_ids: Vec<String>,
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
                is_group: None,
                worktree_branch: None,
                folder_id: None,
                color_id: None,
                pinned: None,
                git_branch: None,
                mcp_blocked: false,
                archived_session_count: None,
                date_sorted: None,
                session_order: None,
            },
            ProjectSummary {
                id: "group-a".into(),
                name: "Group A".into(),
                path: "/home/a".into(),
                parent_project_id: Some("home".into()),
                sort_order: Some(1),
                is_group: Some(true),
                worktree_branch: None,
                folder_id: None,
                color_id: None,
                pinned: None,
                git_branch: None,
                mcp_blocked: false,
                archived_session_count: None,
                date_sorted: None,
                session_order: None,
            },
            ProjectSummary {
                id: "group-b".into(),
                name: "Group B".into(),
                path: "/home/b".into(),
                parent_project_id: Some("home".into()),
                sort_order: Some(0),
                is_group: Some(true),
                worktree_branch: None,
                folder_id: None,
                color_id: None,
                pinned: None,
                git_branch: None,
                mcp_blocked: false,
                archived_session_count: None,
                date_sorted: None,
                session_order: None,
            },
            // Not a plain group (no isFolder): not a filing destination.
            ProjectSummary {
                id: "proj".into(),
                name: "Proj".into(),
                path: "/proj".into(),
                parent_project_id: Some("home".into()),
                sort_order: Some(2),
                is_group: None,
                worktree_branch: None,
                folder_id: None,
                color_id: None,
                pinned: None,
                git_branch: None,
                mcp_blocked: false,
                archived_session_count: None,
                date_sorted: None,
                session_order: None,
            },
            // Worktree child: never a filing destination.
            ProjectSummary {
                id: "wt".into(),
                name: "Worktree".into(),
                path: "/wt".into(),
                parent_project_id: Some("home".into()),
                sort_order: Some(3),
                is_group: Some(true),
                worktree_branch: Some("feature".into()),
                folder_id: None,
                color_id: None,
                pinned: None,
                git_branch: None,
                mcp_blocked: false,
                archived_session_count: None,
                date_sorted: None,
                session_order: None,
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

    // Tests for RemoteControlProtocol.swift DTO ports.
    // Each mirrors a Swift XCTest in RemoteControlProtocolTests.swift.

    #[test]
    fn paired_device_summary_round_trips_last_seen() {
        // Mirrors Swift testPairedDeviceSummaryRoundTripsLastSeen.
        let d: PairedDeviceSummary = serde_json::from_value(serde_json::json!({
            "id": "dev-1",
            "name": "iPhone",
            "platform": "ios",
            "appVersion": "1.2.3",
            "pairedAtUnixMs": 1700000000000i64,
            "lastSeenAtUnixMs": 1700000001000i64,
            "relayAllowed": false,
        }))
        .unwrap();
        assert_eq!(d.id, "dev-1");
        assert_eq!(d.last_seen_at_unix_ms, Some(1700000001000));
        assert_eq!(d.relay_allowed, Some(false));

        // Round-trip.
        let json = serde_json::to_value(&d).unwrap();
        let d2: PairedDeviceSummary = serde_json::from_value(json).unwrap();
        assert_eq!(d, d2);

        // Optional fields decode as None when absent (pre-flag records).
        let d3: PairedDeviceSummary = serde_json::from_value(serde_json::json!({
            "id": "dev-2",
            "name": "Mac",
            "platform": "macos",
            "pairedAtUnixMs": 1700000000000i64,
        }))
        .unwrap();
        assert_eq!(d3.last_seen_at_unix_ms, None);
        assert_eq!(d3.relay_allowed, None);
    }

    #[test]
    fn workspace_summary_decodes_kind_default() {
        // Mirrors Swift testBootstrapRoundTripsHostWorkspaceList.
        let w: WorkspaceSummary = serde_json::from_value(serde_json::json!({
            "id": "ws-1",
            "name": "Main",
            "tintHue": 210.5,
            "isCurrent": true,
            "isRunning": true,
            "kind": "local",
        }))
        .unwrap();
        assert_eq!(w.effective_kind(), "local");

        // Older Hosts omit kind; nil decodes as "local".
        let w2: WorkspaceSummary = serde_json::from_value(serde_json::json!({
            "id": "ws-2",
            "name": "Secondary",
            "isCurrent": false,
            "isRunning": false,
        }))
        .unwrap();
        assert_eq!(w2.effective_kind(), "local");
        assert_eq!(w2.tint_hue, None);
    }

    #[test]
    fn artifact_upload_progress_round_trips() {
        // Mirrors Swift testResumableArtifactUploadProgressRoundTrips.
        let p: ArtifactUploadProgress = serde_json::from_value(serde_json::json!({
            "uploadID": "up-1",
            "sessionID": "sess-1",
            "fileName": "screenshot.png",
            "mimeType": "image/png",
            "totalBytes": 102400,
            "receivedBytes": 51200,
            "chunkSize": 16384,
            "nextOffset": 51200,
            "complete": false,
            "updatedAtUnixMs": 1700000000000i64,
        }))
        .unwrap();
        assert_eq!(p.upload_id, "up-1");
        assert!(!p.complete);
        assert_eq!(p.artifact_id, None);

        let json = serde_json::to_value(&p).unwrap();
        let p2: ArtifactUploadProgress = serde_json::from_value(json).unwrap();
        assert_eq!(p, p2);
    }

    #[test]
    fn create_session_request_round_trips_initial_prompt() {
        // Mirrors Swift testCreateSessionRequestRoundTripsInitialPrompt.
        let r: CreateSessionRequest = serde_json::from_value(serde_json::json!({
            "projectID": "proj-1",
            "presetID": "preset-1",
            "initialText": "Hello, world!",
            "initialTextSubmitMode": "pasteAndSubmit",
        }))
        .unwrap();
        assert_eq!(r.project_id, "proj-1");
        assert_eq!(r.initial_text, Some("Hello, world!".to_string()));
        assert_eq!(r.initial_text_submit_mode, TextSubmitMode::PasteAndSubmit);

        let json = serde_json::to_value(&r).unwrap();
        let r2: CreateSessionRequest = serde_json::from_value(json).unwrap();
        assert_eq!(r, r2);
    }

    #[test]
    fn terminal_write_resize_round_trip() {
        // Mirrors Swift testTerminalWriteResizeAndCreateResponseRoundTrip.
        let w: TerminalWriteRequest = serde_json::from_value(serde_json::json!({
            "sessionID": "sess-1",
            "data": "aGVsbG8=",
            "wid": "key-1",
        }))
        .unwrap();
        assert_eq!(w.session_id, "sess-1");
        assert_eq!(w.data, "aGVsbG8=");
        assert_eq!(w.idempotency_key, Some("key-1".to_string()));

        let r: TerminalResizeRequest = serde_json::from_value(serde_json::json!({
            "sessionID": "sess-1",
            "columns": 80,
            "rows": 24,
        }))
        .unwrap();
        assert_eq!(r.columns, 80);
        assert_eq!(r.rows, 24);
    }

    #[test]
    fn viewport_frame_round_trips_styled_cells() {
        // Mirrors Swift testViewportFrameRoundTripsStyledCells.
        let f: ViewportFrame = serde_json::from_value(serde_json::json!({
            "sessionID": "sess-1",
            "sequence": 42,
            "rows": 24,
            "columns": 80,
            "cells": [
                {
                    "text": "h",
                    "foreground": {"kind": "ansi", "index": 1},
                    "style": {"bold": true}
                },
                {
                    "text": "i",
                    "style": {}
                }
            ],
            "cursor": {"row": 0, "column": 2, "shape": "block", "visible": true},
            "alternateScreen": false,
            "capturedAtUnixMs": 1700000000000i64,
        }))
        .unwrap();
        assert_eq!(f.session_id, "sess-1");
        assert_eq!(f.sequence, 42);
        assert_eq!(f.cells.len(), 2);
        assert_eq!(f.cells[0].text, "h");
        assert!(f.cells[0].style.bold);
        assert_eq!(f.cells[0].foreground, Some(TerminalColor::ansi(1)));
        assert!(f.cursor.is_some());

        let json = serde_json::to_value(&f).unwrap();
        let f2: ViewportFrame = serde_json::from_value(json).unwrap();
        assert_eq!(f, f2);
    }

    #[test]
    fn viewport_patch_round_trips_changed_runs() {
        // Mirrors Swift testViewportPatchRoundTripsChangedRuns.
        let p: ViewportPatch = serde_json::from_value(serde_json::json!({
            "sessionID": "sess-1",
            "sequence": 43,
            "baseSequence": 42,
            "changedRuns": [
                {
                    "row": 0,
                    "column": 0,
                    "cells": [{"text": "x", "style": {}}]
                }
            ],
            "capturedAtUnixMs": 1700000000000i64,
        }))
        .unwrap();
        assert_eq!(p.base_sequence, 42);
        assert_eq!(p.changed_runs.len(), 1);
        assert_eq!(p.changed_runs[0].cells[0].text, "x");
    }

    #[test]
    fn stream_event_round_trip() {
        // Mirrors Swift testStreamEventCanCarryEncodedViewportFrame.
        let e: StreamEvent = serde_json::from_value(serde_json::json!({
            "protocolVersion": 1,
            "id": "evt-1",
            "kind": "viewportFrame",
            "sessionID": "sess-1",
            "createdAtUnixMs": 1700000000000i64,
        }))
        .unwrap();
        assert_eq!(e.kind, StreamEventKind::ViewportFrame);
        assert_eq!(e.session_id, Some("sess-1".to_string()));
        assert_eq!(e.payload, None);
    }

    #[test]
    fn session_action_request_round_trip() {
        // Mirrors Swift testSessionActionRequestRoundTrips.
        let r: SessionActionRequest = serde_json::from_value(serde_json::json!({
            "sessionID": "sess-1",
            "action": "stop",
        }))
        .unwrap();
        assert_eq!(r.action, SessionAction::Stop);

        // Legacy restart_agent decodes.
        let r2: SessionActionRequest = serde_json::from_value(serde_json::json!({
            "sessionID": "sess-1",
            "action": "restart_agent",
        }))
        .unwrap();
        assert_eq!(r2.action, SessionAction::RestartAgent);
    }

    #[test]
    fn session_organization_patch_round_trips_partial_fields() {
        // Mirrors Swift testSessionOrganizationPatchRoundTripsPartialFields.
        let p: SessionOrganizationPatch = serde_json::from_value(serde_json::json!({
            "sessionID": "sess-1",
            "title": "New Title",
            "pinned": true,
        }))
        .unwrap();
        assert_eq!(p.title, Some("New Title".to_string()));
        assert_eq!(p.pinned, Some(true));
        assert_eq!(p.archived, None);
        assert_eq!(p.project_id, None);
    }

    #[test]
    fn screenshot_request_round_trip() {
        // Mirrors Swift testScreenshotRequestAndAcknowledgementRoundTrip.
        let r: ScreenshotRequest = serde_json::from_value(serde_json::json!({
            "sessionID": "sess-1",
        }))
        .unwrap();
        assert_eq!(r.session_id, "sess-1");

        let resp: ScreenshotRequestResponse = serde_json::from_value(serde_json::json!({
            "accepted": true,
            "requestedAtUnixMs": 1700000000000i64,
        }))
        .unwrap();
        assert!(resp.accepted);
    }

    #[test]
    fn plugin_updates_round_trip() {
        let u: PluginUpdates = serde_json::from_value(serde_json::json!({
            "checking": false,
            "items": [
                {
                    "id": "plugin-1",
                    "state": "active",
                    "installedVersion": "1.0.0",
                    "latestVersion": "1.1.0",
                    "updateAvailable": true,
                }
            ],
        }))
        .unwrap();
        assert!(!u.checking);
        assert_eq!(u.items.len(), 1);
        assert!(u.items[0].update_available);
    }

    #[test]
    fn terminal_color_constructors() {
        // Mirrors Swift RemoteTerminalColor static constructors.
        assert_eq!(
            TerminalColor::default_foreground().kind,
            TerminalColorKind::DefaultForeground
        );
        assert_eq!(
            TerminalColor::ansi(1),
            TerminalColor {
                kind: TerminalColorKind::Ansi,
                index: Some(1),
                red: None,
                green: None,
                blue: None,
            }
        );
        let rgb = TerminalColor::rgb(255, 0, 0);
        assert_eq!(rgb.red, Some(255));
        assert_eq!(rgb.green, Some(0));
    }

    // MARK: - RemoteControlProtocol batch 2 (porter T)

    #[test]
    fn project_folder_summary_decodes() {
        let f: ProjectFolderSummary = serde_json::from_value(serde_json::json!({
            "id": "f1",
            "name": "Work",
            "parentFolderID": null,
            "colorID": "sky",
            "sortOrder": 2,
        }))
        .unwrap();
        assert_eq!(f.id, "f1");
        assert_eq!(f.color_id.as_deref(), Some("sky"));
        assert_eq!(f.sort_order, Some(2));
        assert_eq!(f.parent_folder_id, None);
    }

    #[test]
    fn archived_sessions_response_decodes() {
        let r: ArchivedSessionsResponse = serde_json::from_value(serde_json::json!({
            "projectID": "p1",
            "sessions": [{"id": "s1", "status": "running", "activity": "idle"}],
        }))
        .unwrap();
        assert_eq!(r.project_id, "p1");
        assert_eq!(r.sessions.len(), 1);
        assert_eq!(r.sessions[0].id, "s1");
    }

    #[test]
    fn transcript_block_kind_round_trips() {
        let k: TranscriptBlockKind = serde_json::from_value(serde_json::json!("toolCall")).unwrap();
        assert_eq!(k, TranscriptBlockKind::ToolCall);
        let k2: TranscriptBlockKind =
            serde_json::from_value(serde_json::json!("fileChange")).unwrap();
        assert_eq!(k2, TranscriptBlockKind::FileChange);
        assert_eq!(
            serde_json::to_value(TranscriptBlockKind::PlanUpdate).unwrap(),
            serde_json::json!("planUpdate")
        );
    }

    #[test]
    fn transcript_stream_chunk_decodes() {
        let c: TranscriptStreamChunk = serde_json::from_value(serde_json::json!({
            "sessionID": "s1",
            "resolved": true,
            "offset": 10,
            "nextOffset": 20,
            "partial": "hel",
            "truncated": false,
            "entries": [],
            "updatedAtUnixMs": 123,
        }))
        .unwrap();
        assert_eq!(c.session_id, "s1");
        assert_eq!(c.offset, 10);
        assert_eq!(c.next_offset, 20);
        assert_eq!(c.partial, "hel");
        assert!(c.entries.is_empty());
    }

    #[test]
    fn transcript_history_page_decodes() {
        let p: TranscriptHistoryPage = serde_json::from_value(serde_json::json!({
            "sessionID": "s1",
            "resolved": false,
            "startOffset": 0,
            "endOffset": 50,
            "truncated": true,
            "entries": [],
            "updatedAtUnixMs": 7,
        }))
        .unwrap();
        assert_eq!(p.start_offset, 0);
        assert_eq!(p.end_offset, 50);
        assert!(p.truncated);
    }

    #[test]
    fn pane_group_summary_decodes() {
        let g: PaneGroupSummary = serde_json::from_value(serde_json::json!({
            "id": "g1",
            "representativeSessionID": "s1",
            "sessionIDs": ["s1", "s2"],
        }))
        .unwrap();
        assert_eq!(g.representative_session_id, "s1");
        assert_eq!(g.session_ids, vec!["s1", "s2"]);
    }

    #[test]
    fn app_summary_handles_selectors() {
        let a: AppSummary = serde_json::from_value(serde_json::json!({
            "id": "a1",
            "name": "TestApp",
            "description": "test app",
            "command": "open",
            "updateAvailable": false,
            "mediaTypes": ["image/png"],
            "resourceKinds": ["folder"],
            "fileExtensions": {"png": "image/png"},
            "defaultFor": [],
            "installed": true,
        }))
        .unwrap();
        assert!(a.handles("file:image/png"));
        assert!(a.handles("file:IMAGE/PNG"));
        assert!(!a.handles("file:text/plain"));
        assert!(a.handles("resource:folder"));
        assert!(!a.handles("resource:file"));
        assert!(!a.handles("file:"));
        assert!(!a.handles("bogus"));
        // Required fields decode as provided (no silent defaults).
        assert_eq!(a.description, "test app");
        assert!(!a.update_available);
        assert!(a.installed);
    }

    #[test]
    fn app_summary_media_type_for_path() {
        let apps = vec![AppSummary {
            id: "a1".into(),
            name: "Img".into(),
            description: String::new(),
            tint: None,
            icon_svg: None,
            version: None,
            installed_version: None,
            update_available: false,
            install_command: None,
            command: "open".into(),
            media_types: vec![],
            file_extensions: [("png".to_string(), "image/png".to_string())]
                .into_iter()
                .collect(),
            resource_kinds: vec![],
            default_for: vec![],
            installed: false,
        }];
        assert_eq!(
            AppSummary::media_type_for_path("/tmp/photo.PNG", &apps).as_deref(),
            Some("image/png")
        );
        assert_eq!(AppSummary::media_type_for_path("/tmp/noext", &apps), None);
        assert_eq!(AppSummary::media_type_for_path("/tmp/f.txt", &apps), None);
    }

    #[test]
    fn app_presentations_file_decodes_snake_keys() {
        let f: AppPresentationsFile = serde_json::from_value(serde_json::json!({
            "version": 1,
            "instances": [{"id": "i1", "app_id": "a1", "companion_session_id": "s9"}],
            "presentations": [{
                "id": "p1",
                "caller_session_id": "s1",
                "instance_id": "i1",
                "target": "main",
                "reveal_revision": 3,
            }],
        }))
        .unwrap();
        assert_eq!(f.version, 1);
        assert_eq!(f.instances[0].app_id, "a1");
        assert_eq!(f.presentations[0].reveal_revision, 3);
    }

    #[test]
    fn app_presentations_file_rejects_missing_required() {
        // Swift declares version/instances/presentations non-optional, so a
        // Host that omits them is a wire error (Swift's own custom-decoder
        // leniency is a client-side fallback, not the wire contract).
        assert!(serde_json::from_value::<AppPresentationsFile>(serde_json::json!({})).is_err());
        assert!(serde_json::from_value::<AppPresentationsFile>(
            serde_json::json!({"version": 1, "instances": []})
        )
        .is_err());
    }

    #[test]
    fn host_environment_row_label() {
        let b = HostEnvironment {
            kind: "box".into(),
            id: "bx_1a2b3c4d5e6f".into(),
        };
        assert_eq!(b.row_label(), "Box · bx_1a2b3c…");
        let short = HostEnvironment {
            kind: "box".into(),
            id: "bx_1".into(),
        };
        assert_eq!(short.row_label(), "Box · bx_1");
        let other = HostEnvironment {
            kind: "nas".into(),
            id: "abc".into(),
        };
        assert_eq!(other.row_label(), "nas · abc");
    }

    #[test]
    fn workspace_summary_decodes() {
        let w: WorkspaceSummary = serde_json::from_value(serde_json::json!({
            "id": "w1",
            "name": "Main",
            "tintHue": 210.5,
            "isCurrent": true,
            "isRunning": true,
            "kind": "local",
        }))
        .unwrap();
        assert_eq!(w.tint_hue, Some(210.5));
        assert!(w.is_current);
        assert_eq!(w.kind.as_deref(), Some("local"));
    }

    #[test]
    fn workspace_select_round_trips() {
        let req = WorkspaceSelectRequest {
            workspace_id: "w2".into(),
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["workspaceId"], "w2");
        // `workspace` is required (Swift declares it non-optional): a full
        // payload round-trips, and a missing workspace is a decode error.
        let resp: WorkspaceSelectResponse = serde_json::from_value(serde_json::json!({
            "workspace": {
                "id": "w2",
                "name": "Work",
                "displayName": "Work",
                "kind": "local",
                "isCurrent": true,
                "isRunning": true,
            }
        }))
        .unwrap();
        assert_eq!(resp.workspace.id, "w2");
        assert!(resp.workspace.is_current);
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["workspace"]["id"], "w2");
        assert!(serde_json::from_value::<WorkspaceSelectResponse>(serde_json::json!({})).is_err());
        assert!(serde_json::from_value::<WorkspaceSelectResponse>(
            serde_json::json!({"workspace": null})
        )
        .is_err());
    }

    #[test]
    fn desktop_resize_request_decodes() {
        let r: DesktopResizeRequest = serde_json::from_value(serde_json::json!({
            "sessionID": "s1",
            "columns": 120,
            "rows": 40,
            "clear": true,
        }))
        .unwrap();
        assert_eq!(r.columns, Some(120));
        assert_eq!(r.clear, Some(true));
    }

    #[test]
    fn key_name_round_trips_camel() {
        let k: KeyName = serde_json::from_value(serde_json::json!("arrowUp")).unwrap();
        assert_eq!(k, KeyName::ArrowUp);
        let c: KeyName = serde_json::from_value(serde_json::json!("controlC")).unwrap();
        assert_eq!(c, KeyName::ControlC);
        let input: SessionKeyInput = serde_json::from_value(serde_json::json!({
            "sessionID": "s1",
            "keys": ["enter", "controlC"],
        }))
        .unwrap();
        assert_eq!(input.keys, vec![KeyName::Enter, KeyName::ControlC]);
    }

    #[test]
    fn terminal_metrics_decodes() {
        let m: TerminalMetrics = serde_json::from_value(serde_json::json!({
            "sessionID": "s1",
            "columns": 80,
            "rows": 24,
            "capturedAtUnixMs": 99,
            "desktopViewing": true,
        }))
        .unwrap();
        assert_eq!(m.columns, 80);
        assert_eq!(m.desktop_viewing, Some(true));
    }

    #[test]
    fn browser_artifact_decodes() {
        let a: BrowserArtifact = serde_json::from_value(serde_json::json!({
            "kind": "screenshots",
            "name": "shot.png",
            "size": 12345,
            "modifiedAtUnixMs": 5,
        }))
        .unwrap();
        assert_eq!(a.size, 12345);
        let l: BrowserArtifactList = serde_json::from_value(serde_json::json!({
            "sessionID": "s1",
            "artifacts": [],
            "capturedAtUnixMs": 5,
        }))
        .unwrap();
        assert!(l.artifacts.is_empty());
        let c: BrowserArtifactChunk = serde_json::from_value(serde_json::json!({
            "sessionID": "s1",
            "kind": "screenshots",
            "name": "shot.png",
            "contentType": "image/png",
            "offset": 0,
            "nextOffset": 1024,
            "totalSize": 2048,
            "dataBase64": "aGk=",
            "capturedAtUnixMs": 5,
        }))
        .unwrap();
        assert_eq!(c.next_offset, 1024);
        assert_eq!(c.total_size, 2048);
    }

    #[test]
    fn settings_updates_are_all_optional() {
        let t: TranscriptSettingsUpdate =
            serde_json::from_value(serde_json::json!({"maxEntries": 50})).unwrap();
        assert_eq!(t.max_entries, Some(50));
        assert_eq!(t.include_user, None);
        let a: AppearanceSettingsUpdate =
            serde_json::from_value(serde_json::json!({"theme": "midnight"})).unwrap();
        assert_eq!(a.theme.as_deref(), Some("midnight"));
        assert_eq!(a.app_tint, None);
        let n: NotificationSettingsUpdate = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(n.menu_attention_detection, None);
        let e: ExperimentalSettingsUpdate =
            serde_json::from_value(serde_json::json!({"worktrees": true})).unwrap();
        assert_eq!(e.worktrees, Some(true));
        assert_eq!(e.computer_use, None);
    }

    #[test]
    fn experimental_settings_decodes_adapter_state() {
        let e: ExperimentalSettings = serde_json::from_value(serde_json::json!({
            "worktrees": true,
            "sessionsMcp": false,
            "browserMcp": false,
            "computerUse": true,
            "computerUseAvailable": true,
            "computerUseReady": false,
            "computerUseUnavailableReason": "no driver",
            "workspaces": true,
        }))
        .unwrap();
        assert_eq!(e.computer_use_available, Some(true));
        assert_eq!(
            e.computer_use_unavailable_reason.as_deref(),
            Some("no driver")
        );
    }

    #[test]
    fn agent_summary_decodes() {
        let a: AgentSummary = serde_json::from_value(serde_json::json!({
            "id": "claude",
            "name": "Claude",
            "command": "claude",
            "installed": true,
            "integrationInstallable": true,
        }))
        .unwrap();
        assert!(a.installed);
        assert_eq!(a.integration_installable, Some(true));
        assert_eq!(a.website_url, None);
    }

    #[test]
    fn workspace_settings_decodes() {
        let w: WorkspaceSettings = serde_json::from_value(serde_json::json!({
            "autoStopArchiveMinutes": 30,
            "sidebarStoppedLimit": 10,
            "browserDefaultAccess": "ask",
            "mcpNonchildWriteAccess": "deny",
            "computerAccess": "ask",
            "mcpWorktreeAccess": true,
            "mcpAutoAddBrowserScreenshots": false,
            "transcriptSettings": {"includeUser": true, "includeAssistant": true,
                "includeReasoning": false, "includeTools": true,
                "includeFileChanges": true, "includePlanUpdates": true,
                "includeSessionInfo": false, "maxEntries": 100},
        }))
        .unwrap();
        assert_eq!(w.auto_stop_archive_minutes, 30);
        assert!(w.mcp_worktree_access);
        let ts = w.transcript_settings.unwrap();
        assert_eq!(ts.max_entries, 100);
        assert!(ts.include_user);
        assert!(w.plugin_order.is_none());
        assert!(w.available_agents.is_none());
    }

    #[test]
    fn session_order_request_decodes() {
        let r: SessionOrderRequest = serde_json::from_value(serde_json::json!({
            "projectID": "p1",
            "orderedSessionIDs": ["s2", "s1"],
        }))
        .unwrap();
        assert_eq!(r.project_id, "p1");
        assert_eq!(r.ordered_session_ids, vec!["s2", "s1"]);
    }

    // MARK: - Protocol fixtures: remaining batch-2 DTOs
    //
    // Every batch-2 DTO without a dedicated test above gets a wire fixture
    // here: a representative JSON payload decodes, the value round-trips,
    // and a payload with one required field removed fails to decode.
    // Required Swift fields are required on the wire (no `#[serde(default)]`),
    // so missing fields must surface as decode errors, not silent zeros.

    /// Decode `T` from the fixture `v`, assert the value round-trips
    /// unchanged, then assert decoding fails with `field` removed.
    fn assert_fixture_required<T>(v: serde_json::Value, field: &str)
    where
        T: for<'de> serde::Deserialize<'de> + serde::Serialize + PartialEq + std::fmt::Debug,
    {
        let t: T = serde_json::from_value(v.clone()).expect("fixture should decode");
        let rt: T = serde_json::from_value(serde_json::to_value(&t).unwrap())
            .expect("round-trip should decode");
        assert_eq!(t, rt, "round-trip should preserve the value");
        let mut obj = v.as_object().cloned().expect("fixture must be an object");
        obj.remove(field);
        assert!(
            serde_json::from_value::<T>(serde_json::Value::Object(obj)).is_err(),
            "missing required field `{field}` should fail to decode"
        );
    }

    #[test]
    fn app_presentation_instance_fixture() {
        assert_fixture_required::<AppPresentationInstance>(
            serde_json::json!({
                "id": "i1",
                "app_id": "a1",
                "companion_session_id": "s9",
            }),
            "app_id",
        );
    }

    #[test]
    fn app_presentation_fixture() {
        assert_fixture_required::<AppPresentation>(
            serde_json::json!({
                "id": "p1",
                "caller_session_id": "s1",
                "instance_id": "i1",
                "target": "main",
                "reveal_revision": 3,
            }),
            "reveal_revision",
        );
    }

    #[test]
    fn session_key_input_fixture() {
        let k: SessionKeyInput = serde_json::from_value(serde_json::json!({
            "sessionID": "s1",
            "keys": ["enter", "arrowUp", "controlC"],
        }))
        .unwrap();
        assert_eq!(k.session_id, "s1");
        assert_eq!(
            k.keys,
            vec![KeyName::Enter, KeyName::ArrowUp, KeyName::ControlC]
        );
        assert_fixture_required::<SessionKeyInput>(
            serde_json::json!({
                "sessionID": "s1",
                "keys": ["enter"],
            }),
            "keys",
        );
    }

    #[test]
    fn browser_artifact_list_fixture() {
        assert_fixture_required::<BrowserArtifactList>(
            serde_json::json!({
                "sessionID": "s1",
                "artifacts": [{
                    "kind": "screenshots",
                    "name": "shot-1.png",
                    "size": 1234,
                    "modifiedAtUnixMs": 1700000000000i64,
                }],
                "capturedAtUnixMs": 1700000000001i64,
            }),
            "capturedAtUnixMs",
        );
    }

    #[test]
    fn browser_artifact_chunk_fixture() {
        assert_fixture_required::<BrowserArtifactChunk>(
            serde_json::json!({
                "sessionID": "s1",
                "kind": "screenshots",
                "name": "shot-1.png",
                "contentType": "image/png",
                "offset": 0,
                "nextOffset": 4096,
                "totalSize": 8192,
                "dataBase64": "aGVsbG8=",
                "capturedAtUnixMs": 1700000000000i64,
            }),
            "dataBase64",
        );
    }

    #[test]
    fn transcript_settings_fixture() {
        assert_fixture_required::<TranscriptSettings>(
            serde_json::json!({
                "includeUser": true,
                "includeAssistant": true,
                "includeReasoning": false,
                "includeTools": true,
                "includeFileChanges": true,
                "includePlanUpdates": true,
                "includeSessionInfo": false,
                "maxEntries": 100,
            }),
            "maxEntries",
        );
    }

    #[test]
    fn transcript_settings_update_stays_all_optional() {
        // Patch DTO: every field is Option, so an empty object decodes and
        // only the provided fields are set.
        let u: TranscriptSettingsUpdate = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(u, TranscriptSettingsUpdate::default());
        let u: TranscriptSettingsUpdate = serde_json::from_value(serde_json::json!({
            "includeUser": false,
            "maxEntries": 50,
        }))
        .unwrap();
        assert_eq!(u.include_user, Some(false));
        assert_eq!(u.max_entries, Some(50));
        assert_eq!(u.include_tools, None);
    }

    #[test]
    fn appearance_settings_fixture() {
        assert_fixture_required::<AppearanceSettings>(
            serde_json::json!({
                "theme": "midnight",
                "appTint": "blue",
                "backgroundOpacity": 0.9,
                "surfaceOpacity": 0.95,
                "backgroundTone": 0.1,
                "surfaceTone": 0.2,
                "sessionTitleMode": "auto",
            }),
            "theme",
        );
    }

    #[test]
    fn appearance_settings_update_stays_all_optional() {
        let u: AppearanceSettingsUpdate =
            serde_json::from_value(serde_json::json!({"theme": "midnight"})).unwrap();
        assert_eq!(u.theme.as_deref(), Some("midnight"));
        assert_eq!(u.app_tint, None);
    }

    #[test]
    fn notification_settings_fixture() {
        assert_fixture_required::<NotificationSettings>(
            serde_json::json!({"menuAttentionDetection": true}),
            "menuAttentionDetection",
        );
    }

    #[test]
    fn notification_settings_update_stays_all_optional() {
        let u: NotificationSettingsUpdate = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(u.menu_attention_detection, None);
    }

    #[test]
    fn experimental_settings_update_stays_all_optional() {
        let u: ExperimentalSettingsUpdate = serde_json::from_value(serde_json::json!({
            "computerUse": true,
        }))
        .unwrap();
        assert_eq!(u.computer_use, Some(true));
        assert_eq!(u.worktrees, None);
    }

    // --- RemoteControlProtocol batch 3 (porter U) ---

    #[test]
    fn session_capabilities_decodes_restart_agent_and_tombstones() {
        // Swift: RemoteSessionCapabilities decodes legacy `restartAgent` and
        // accepts the retired `fork` / `appendSystemContext` keys.
        let c: SessionCapabilities = serde_json::from_value(serde_json::json!({
            "restart": true,
            "restartAgent": false,
            "resumeAgent": true,
            "fork": true,
            "appendSystemContext": true,
            "notifyWhenDone": true,
            "archive": false,
        }))
        .unwrap();
        assert!(c.restart);
        assert_eq!(c.restart_agent, Some(false));
        assert_eq!(c.resume_agent, Some(true));
        assert!(c.notify_when_done);
        assert!(!c.archive);

        // Encode: tombstones always emitted as `false` for older
        // Controllers; absent optionals omitted.
        let v = serde_json::to_value(c).unwrap();
        assert_eq!(v["fork"], false);
        assert_eq!(v["appendSystemContext"], false);
        assert_eq!(v["restartAgent"], false);
        assert_eq!(v["resumeAgent"], true);
    }

    #[test]
    fn session_capabilities_distinguishes_old_host_from_unavailable() {
        // Absent `resumeAgent` = older Host (None); explicit false = current
        // Host where the operation is unavailable for this session.
        let old: SessionCapabilities = serde_json::from_value(serde_json::json!({
            "restart": false,
            "notifyWhenDone": false,
        }))
        .unwrap();
        assert_eq!(old.resume_agent, None);
        assert_eq!(old.restart_agent, None);

        let current: SessionCapabilities = serde_json::from_value(serde_json::json!({
            "restart": false,
            "resumeAgent": false,
            "notifyWhenDone": false,
        }))
        .unwrap();
        assert_eq!(current.resume_agent, Some(false));
    }

    #[test]
    fn project_summary_decodes_full_wire_shape() {
        // Swift: RemoteProjectSummary full shape incl. sessionOrder.
        let p: ProjectSummary = serde_json::from_value(serde_json::json!({
            "id": "g1",
            "name": "G",
            "path": "/g",
            "folderID": "f0",
            "colorID": "sky",
            "pinned": true,
            "gitBranch": "main",
            "mcpBlocked": true,
            "archivedSessionCount": 3,
            "dateSorted": true,
            "sessionOrder": ["s1", "s2"],
        }))
        .unwrap();
        assert_eq!(p.folder_id.as_deref(), Some("f0"));
        assert_eq!(p.color_id.as_deref(), Some("sky"));
        assert_eq!(p.pinned, Some(true));
        assert_eq!(p.git_branch.as_deref(), Some("main"));
        assert!(p.mcp_blocked);
        assert_eq!(p.archived_session_count, Some(3));
        assert_eq!(p.date_sorted, Some(true));
        assert_eq!(
            p.session_order.as_deref(),
            Some(&["s1".to_string(), "s2".to_string()][..])
        );

        // Older Hosts omit the additive fields.
        let minimal: ProjectSummary =
            serde_json::from_value(serde_json::json!({"id": "g1"})).unwrap();
        assert_eq!(minimal.session_order, None);
        assert!(!minimal.mcp_blocked);
    }

    #[test]
    fn project_summary_replacing_session_order() {
        let p: ProjectSummary = serde_json::from_value(serde_json::json!({
            "id": "g1",
            "sessionOrder": ["s1"],
        }))
        .unwrap();
        let q = p.replacing_session_order(Some(vec!["s2".to_string()]));
        assert_eq!(q.session_order.as_deref(), Some(&["s2".to_string()][..]));
        // The original is unchanged (Swift returns a new value).
        assert_eq!(q.id, "g1");
        assert_eq!(p.session_order.as_deref(), Some(&["s1".to_string()][..]));
        let cleared = q.replacing_session_order(None);
        assert_eq!(cleared.session_order, None);
    }

    #[test]
    fn pending_approval_presents_write_on_known_target_otherwise_caller() {
        // Swift: RemotePendingApproval.presentationSessionID.
        let a = PendingApproval {
            id: "a1".into(),
            kind: "write".into(),
            title: None,
            detail: None,
            body: "".into(),
            caller_session_id: "caller".into(),
            target_session_id: Some("target".into()),
            requested_at_unix_ms: 0,
            session_id: None,
        };
        let known: std::collections::HashSet<String> = ["caller".to_string(), "target".to_string()]
            .into_iter()
            .collect();
        assert_eq!(a.presentation_session_id(&known), "target");

        // Unknown destination falls back to the caller.
        let unknown: std::collections::HashSet<String> =
            ["caller".to_string()].into_iter().collect();
        assert_eq!(a.presentation_session_id(&unknown), "caller");

        // Non-write kinds have no destination: present on the caller.
        let browser = PendingApproval {
            target_session_id: None,
            ..a.clone()
        };
        assert_eq!(browser.presentation_session_id(&known), "caller");
    }

    #[test]
    fn pending_approval_decodes_swift_wire_shape() {
        let a: PendingApproval = serde_json::from_value(serde_json::json!({
            "id": "a1",
            "kind": "write",
            "title": "Allow write?",
            "body": "wants to write",
            "callerSessionID": "caller",
            "targetSessionID": "target",
            "requestedAtUnixMs": 1789996800000i64,
        }))
        .unwrap();
        assert_eq!(a.kind, "write");
        assert_eq!(a.caller_session_id, "caller");
        assert_eq!(a.target_session_id.as_deref(), Some("target"));
        assert_eq!(a.requested_at_unix_ms, 1789996800000i64);
    }

    #[test]
    fn terminal_output_chunk_round_trips_offsets() {
        // Swift: RemoteTerminalOutputChunk wire shape.
        let c: TerminalOutputChunk = serde_json::from_value(serde_json::json!({
            "sessionID": "s1",
            "offset": 100,
            "nextOffset": 132,
            "dataBase64": "aGVsbG8=",
            "truncated": true,
            "capturedAtUnixMs": 1789996800000i64,
            "modePreambleBase64": "G1s=",
        }))
        .unwrap();
        assert_eq!(c.offset, 100);
        assert_eq!(c.next_offset, 132);
        assert!(c.truncated);
        assert_eq!(c.mode_preamble(), Some(vec![0x1b, 0x5b]));

        let back = serde_json::to_value(&c).unwrap();
        let again: TerminalOutputChunk = serde_json::from_value(back).unwrap();
        assert_eq!(again, c);
    }

    #[test]
    fn terminal_output_chunk_mode_preamble_edge_cases() {
        let base = || TerminalOutputChunk {
            session_id: "s1".into(),
            offset: 0,
            next_offset: 0,
            data_base64: "".into(),
            truncated: false,
            captured_at_unix_ms: 0,
            mode_preamble_base64: None,
        };
        // Absent -> None.
        assert_eq!(base().mode_preamble(), None);
        // Not valid base64 -> None (Swift returns nil).
        let mut bad = base();
        bad.mode_preamble_base64 = Some("!!!".into());
        assert_eq!(bad.mode_preamble(), None);
        // Empty -> None (Swift returns nil for empty Data).
        let mut empty = base();
        empty.mode_preamble_base64 = Some("".into());
        assert_eq!(empty.mode_preamble(), None);
        // Older Hosts omit the key entirely.
        let decoded: TerminalOutputChunk = serde_json::from_value(serde_json::json!({
            "sessionID": "s1",
            "offset": 0,
            "nextOffset": 0,
            "dataBase64": "",
            "capturedAtUnixMs": 0,
        }))
        .unwrap();
        assert_eq!(decoded.mode_preamble_base64, None);
        assert_eq!(decoded.mode_preamble(), None);
    }

    #[test]
    fn workspace_settings_patch_is_empty() {
        // Swift: RemoteWorkspaceSettingsPatch.isEmpty.
        let empty = WorkspaceSettingsPatch::default();
        assert!(empty.is_empty());

        let p = WorkspaceSettingsPatch {
            sidebar_stopped_limit: Some(5),
            ..Default::default()
        };
        assert!(!p.is_empty());

        // The newly added settings groups participate too.
        let q = WorkspaceSettingsPatch {
            transcript_settings: Some(TranscriptSettingsUpdate {
                include_user: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(!q.is_empty());
    }
}
