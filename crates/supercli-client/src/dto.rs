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
    #[serde(rename = "idempotencyKey", default)]
    pub idempotency_key: Option<String>,
}

/// A terminal resize request.
///
/// Mirrors `RemoteTerminalResizeRequest` in `RemoteControlProtocol.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalResizeRequest {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub cols: i64,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalCellRun {
    pub row: i64,
    #[serde(rename = "startColumn")]
    pub start_column: i64,
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
            "idempotencyKey": "key-1",
        }))
        .unwrap();
        assert_eq!(w.session_id, "sess-1");
        assert_eq!(w.data, "aGVsbG8=");
        assert_eq!(w.idempotency_key, Some("key-1".to_string()));

        let r: TerminalResizeRequest = serde_json::from_value(serde_json::json!({
            "sessionID": "sess-1",
            "cols": 80,
            "rows": 24,
        }))
        .unwrap();
        assert_eq!(r.cols, 80);
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
                    "startColumn": 0,
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
}
