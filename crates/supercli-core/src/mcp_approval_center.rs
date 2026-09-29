//! Unified ask-mode MCP approval queue.
//!
//! Ports `MCPApprovalCenter.swift` from the native app: the FIFO queue of
//! `PendingMcpApproval`s behind the ask-mode bridge routes
//! (`/mcp/approve-write|browser|computer|app-open`) with stable ids,
//! coalescing of identical requests, and first-answer-wins semantics, plus
//! the pure `McpApprovalAttention` overlay rule for the session tree.
//!
//! The `SupercliStore` extension parts that drive AppKit (`NSApp.activate`),
//! the remote runtime, and toasts are surface-specific and stay with the
//! store port; what lives here is the queue state machine, the presentation
//! rule, and the prompt copy.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

/// One pending ask-mode approval request, unified across the kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMcpApproval {
    pub id: String,
    pub kind: McpApprovalKind,
    /// Inter-session `send_text`/`send_keys`/`report` (caller → target pair).
    pub caller_session_id: String,
    /// Write approvals only: the session being written into.
    pub target_session_id: Option<String>,
    /// App-open approvals only: the installed App being launched.
    pub target_app_id: Option<String>,
    pub target_app_name: Option<String>,
    pub requested_at: SystemTime,
}

/// The ask-mode approval kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpApprovalKind {
    /// Inter-session `send_text`/`send_keys`/`report` (caller → target pair).
    Write,
    /// First browser action of a session under Ask.
    Browser,
    /// First computer action of a session under Ask.
    Computer,
    /// First launch of one installed App by this session.
    AppOpen,
}

impl McpApprovalKind {
    /// The wire raw value (`PendingMcpApproval.Kind` rawValue).
    pub fn raw_value(self) -> &'static str {
        match self {
            McpApprovalKind::Write => "write",
            McpApprovalKind::Browser => "browser",
            McpApprovalKind::Computer => "computer",
            McpApprovalKind::AppOpen => "app-open",
        }
    }

    pub fn from_raw(raw: &str) -> Option<Self> {
        match raw {
            "write" => Some(McpApprovalKind::Write),
            "browser" => Some(McpApprovalKind::Browser),
            "computer" => Some(McpApprovalKind::Computer),
            "app-open" => Some(McpApprovalKind::AppOpen),
            _ => None,
        }
    }
}

impl PendingMcpApproval {
    /// Session that should show the in-pane prompt and the attention badge.
    /// Write grants present on the destination so the user sees where input
    /// would land; other kinds have no destination and present on the caller.
    /// A missing/unknown destination falls back to the caller.
    pub fn presentation_session_id<'a>(&'a self, known_ids: &HashSet<String>) -> &'a str {
        if let Some(target) = &self.target_session_id {
            if known_ids.contains(target) {
                return target.as_str();
            }
        }
        self.caller_session_id.as_str()
    }
}

/// A Host-presented approval row, as reconciled into the queue.
#[derive(Debug, Clone)]
pub struct PresentedApproval {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub caller_session_id: String,
    pub target_session_id: Option<String>,
    pub requested_at_unix_ms: u64,
}

/// Where an answered approval must be routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerRoute {
    /// The selected scope's worker owns this prompt: answer through its
    /// runtime and let the next bootstrap confirm the row is gone.
    ScopedRemote,
    /// The local Host owns this prompt: answer through the local backend.
    HostOwned,
}

/// FIFO queue of pending approvals with stable ids. One queue, two sources:
/// the own-home adapter (`scoped == false`) and the selected scope's
/// bootstrap (`scoped == true`). Each source only removes the rows it owns,
/// so a Local prompt survives a scoped refresh and vice versa.
#[derive(Debug, Default)]
pub struct McpApprovalQueue {
    pending: Vec<PendingMcpApproval>,
    host_owned_ids: HashSet<String>,
    scoped_owned_ids: HashSet<String>,
    messages: HashMap<String, (String, String)>,
    answers_in_flight: HashSet<String>,
}

impl McpApprovalQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconcile the Host's approval queue into the pending list. Returns the
    /// newly added approvals so the caller can notify and reveal them. The
    /// callback is a snapshot, not an effect: an approval answered by a phone
    /// simply disappears here on the next generation.
    pub fn reconcile(
        &mut self,
        presented: &[PresentedApproval],
        scoped: bool,
    ) -> Vec<PendingMcpApproval> {
        let incoming: HashSet<&str> = presented.iter().map(|item| item.id.as_str()).collect();

        let mut owned = if scoped {
            std::mem::take(&mut self.scoped_owned_ids)
        } else {
            std::mem::take(&mut self.host_owned_ids)
        };
        let other_owned: HashSet<&str> = if scoped {
            self.host_owned_ids.iter().map(String::as_str).collect()
        } else {
            self.scoped_owned_ids.iter().map(String::as_str).collect()
        };

        self.pending.retain(|approval| {
            !owned.contains(&approval.id) || incoming.contains(approval.id.as_str())
        });
        owned.retain(|id| incoming.contains(id.as_str()));

        let mut retained: HashSet<&str> = incoming.clone();
        retained.extend(other_owned);
        self.messages.retain(|id, _| retained.contains(id.as_str()));
        self.answers_in_flight
            .retain(|id| retained.contains(id.as_str()));

        let existing: HashSet<String> = self
            .pending
            .iter()
            .map(|approval| approval.id.clone())
            .collect();
        let mut added = Vec::new();
        for item in presented {
            self.messages
                .insert(item.id.clone(), (item.title.clone(), item.body.clone()));
            let kind = match McpApprovalKind::from_raw(&item.kind) {
                Some(kind) => kind,
                None => {
                    owned.insert(item.id.clone());
                    continue;
                }
            };
            if existing.contains(item.id.as_str()) {
                owned.insert(item.id.clone());
                continue;
            }
            let approval = PendingMcpApproval {
                id: item.id.clone(),
                kind,
                caller_session_id: item.caller_session_id.clone(),
                target_session_id: item.target_session_id.clone(),
                target_app_id: None,
                target_app_name: None,
                requested_at: UNIX_EPOCH
                    + std::time::Duration::from_millis(item.requested_at_unix_ms),
            };
            owned.insert(item.id.clone());
            self.pending.push(approval.clone());
            added.push(approval);
        }

        if scoped {
            self.scoped_owned_ids = owned;
        } else {
            self.host_owned_ids = owned;
        }
        added
    }

    /// Answer a pending approval by id. Returns the backend route, or None
    /// when the id is no longer pending (already answered elsewhere) — remote
    /// callers surface that as "handled on another device" instead of an
    /// error. A stale unowned row is removed locally and reports
    /// [`AnswerRoute`] as None with the row gone.
    pub fn answer(&mut self, id: &str) -> Option<AnswerRoute> {
        let index = self.pending.iter().position(|approval| approval.id == id)?;
        if self.scoped_owned_ids.contains(id) {
            // First answer wins; repeats are no-ops while one is in flight.
            self.answers_in_flight.insert(id.to_string());
            return Some(AnswerRoute::ScopedRemote);
        }
        if self.host_owned_ids.contains(id) {
            self.answers_in_flight.insert(id.to_string());
            return Some(AnswerRoute::HostOwned);
        }
        // Every pending approval is Host-owned since the Swift Host
        // retirement; an id that is not is a stale row the next
        // reconciliation removes.
        self.pending.remove(index);
        None
    }

    /// Mark a backend-routed answer as settled so a later bootstrap can
    /// confirm the row is gone.
    pub fn settle_answer(&mut self, id: &str) {
        self.answers_in_flight.remove(id);
        self.pending.retain(|approval| approval.id != id);
        self.scoped_owned_ids.remove(id);
        self.host_owned_ids.remove(id);
        self.messages.remove(id);
    }

    /// Session ids that should show the attention badge.
    pub fn attention_session_ids(&self, known_ids: &HashSet<String>) -> HashSet<String> {
        self.pending
            .iter()
            .map(|approval| approval.presentation_session_id(known_ids).to_string())
            .collect()
    }

    pub fn pending_for_session(
        &self,
        session_id: &str,
        known_ids: &HashSet<String>,
    ) -> Option<&PendingMcpApproval> {
        self.pending
            .iter()
            .find(|approval| approval.presentation_session_id(known_ids) == session_id)
    }

    pub fn pending_count_for_session(
        &self,
        session_id: &str,
        known_ids: &HashSet<String>,
    ) -> usize {
        self.pending
            .iter()
            .filter(|approval| approval.presentation_session_id(known_ids) == session_id)
            .count()
    }

    /// Prompt copy shared by the in-session overlay and remote controllers.
    /// Resolved at render time so titles follow session renames.
    pub fn approval_message(
        &self,
        approval: &PendingMcpApproval,
        display_name: &dyn Fn(&str) -> String,
    ) -> (String, String) {
        if let Some(presented) = self.messages.get(&approval.id) {
            return presented.clone();
        }
        match approval.kind {
            McpApprovalKind::Write => {
                let target = approval
                    .target_session_id
                    .as_deref()
                    .map(display_name)
                    .unwrap_or_else(|| "another session".to_string());
                (
                    format!(
                        "Allow “{}” to type into “{target}”?",
                        display_name(&approval.caller_session_id)
                    ),
                    "An agent session is asking to send input to another session. \
                     Allowing remembers this pair until either session is removed — \
                     manage approvals in Settings ▸ Sessions use."
                        .to_string(),
                )
            }
            McpApprovalKind::Browser => (
                format!(
                    "Allow “{}” to use a browser?",
                    display_name(&approval.caller_session_id)
                ),
                "The agent gets its own isolated browser window — separate profile, no \
                 access to your logins or tabs. Allowing remembers this session until \
                 it is removed — manage approvals in Settings ▸ Browser."
                    .to_string(),
            ),
            McpApprovalKind::Computer => (
                format!(
                    "Allow “{}” to control this Mac?",
                    display_name(&approval.caller_session_id)
                ),
                "The agent will be able to read app windows and click and type into them \
                 in the background — your real apps, including anything sensitive they \
                 show. It won't move your cursor or steal focus. Allowing remembers \
                 this session until it is removed — manage approvals in \
                 Settings ▸ Computer."
                    .to_string(),
            ),
            McpApprovalKind::AppOpen => {
                let app_name = approval
                    .target_app_name
                    .clone()
                    .or_else(|| approval.target_app_id.clone())
                    .unwrap_or_else(|| "an App".to_string());
                (
                    format!(
                        "Allow “{}” to open {app_name}?",
                        display_name(&approval.caller_session_id)
                    ),
                    "The App runs in a hosted companion process and can appear as a panel \
                     beside this agent. Allowing remembers this App for this session."
                        .to_string(),
                )
            }
        }
    }
}

/// Lifecycle status of a native session row. Initial port of the
/// `SessionStatus` cases needed by the attention rule; unified with the
/// `Models.swift` port when it lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeSessionStatus {
    Starting,
    Busy,
    Idle,
    Attention,
    Exited,
}

/// A session row in the native sidebar tree. Minimal initial port of
/// `SessionEntry` carrying the fields the attention rule reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSessionEntry {
    pub id: String,
    pub project_id: String,
    pub label: String,
    pub command: String,
    pub created_at: i64,
    pub status: NativeSessionStatus,
}

impl NativeSessionEntry {
    pub fn is_live(&self) -> bool {
        self.status != NativeSessionStatus::Exited
    }
}

/// Minimal initial port of `Project` carrying the fields tests construct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeProject {
    pub id: String,
    pub name: String,
    pub path: String,
    pub parent_project_id: Option<String>,
    pub sort_order: Option<i64>,
    pub is_folder: Option<bool>,
    pub worktree_branch: Option<String>,
    pub workspaces_enabled: Option<bool>,
    pub mcp_blocked: Option<bool>,
}

/// Minimal initial port of `ProjectNode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeProjectNode {
    pub project: NativeProject,
    pub sessions: Vec<NativeSessionEntry>,
    pub worktrees: Vec<NativeProjectNode>,
}

/// Overlay pending MCP approvals onto the displayed session tree as
/// attention. Pure so the sidebar, activity menu, and tests share one rule.
pub fn apply_attention_to_session(
    session: &NativeSessionEntry,
    pending_session_ids: &HashSet<String>,
) -> NativeSessionEntry {
    if !session.is_live() || !pending_session_ids.contains(&session.id) {
        return session.clone();
    }
    let mut next = session.clone();
    next.status = NativeSessionStatus::Attention;
    next
}

pub fn apply_attention_to_nodes(
    nodes: &[NativeProjectNode],
    pending_session_ids: &HashSet<String>,
) -> Vec<NativeProjectNode> {
    if pending_session_ids.is_empty() {
        return nodes.to_vec();
    }
    nodes
        .iter()
        .map(|node| NativeProjectNode {
            project: node.project.clone(),
            sessions: node
                .sessions
                .iter()
                .map(|session| apply_attention_to_session(session, pending_session_ids))
                .collect(),
            worktrees: apply_attention_to_nodes(&node.worktrees, pending_session_ids),
        })
        .collect()
}

pub fn apply_attention_to_map(
    sessions: &HashMap<String, NativeSessionEntry>,
    pending_session_ids: &HashSet<String>,
) -> HashMap<String, NativeSessionEntry> {
    if pending_session_ids.is_empty() {
        return sessions.clone();
    }
    sessions
        .iter()
        .map(|(id, session)| {
            (
                id.clone(),
                apply_attention_to_session(session, pending_session_ids),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approval(kind: McpApprovalKind, caller: &str, target: Option<&str>) -> PendingMcpApproval {
        PendingMcpApproval {
            id: "approval".to_string(),
            kind,
            caller_session_id: caller.to_string(),
            target_session_id: target.map(str::to_string),
            target_app_id: None,
            target_app_name: None,
            requested_at: UNIX_EPOCH,
        }
    }

    fn session(id: &str, status: NativeSessionStatus) -> NativeSessionEntry {
        NativeSessionEntry {
            id: id.to_string(),
            project_id: "project".to_string(),
            label: id.to_string(),
            command: "codex".to_string(),
            created_at: 0,
            status,
        }
    }

    fn project_node(
        id: &str,
        sessions: Vec<NativeSessionEntry>,
        worktrees: Vec<NativeProjectNode>,
    ) -> NativeProjectNode {
        NativeProjectNode {
            project: NativeProject {
                id: id.to_string(),
                name: id.to_string(),
                path: format!("/tmp/{id}"),
                parent_project_id: None,
                sort_order: Some(0),
                is_folder: None,
                worktree_branch: None,
                workspaces_enabled: None,
                mcp_blocked: None,
            },
            sessions,
            worktrees,
        }
    }

    fn known(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn write_presents_on_known_target() {
        let approval = approval(McpApprovalKind::Write, "caller", Some("target"));
        assert_eq!(
            approval.presentation_session_id(&known(&["caller", "target"])),
            "target"
        );
    }

    #[test]
    fn write_falls_back_to_caller_when_target_is_unknown() {
        let approval = approval(McpApprovalKind::Write, "caller", Some("gone"));
        assert_eq!(
            approval.presentation_session_id(&known(&["caller"])),
            "caller"
        );
    }

    #[test]
    fn browser_presents_on_caller() {
        let approval = approval(McpApprovalKind::Browser, "caller", None);
        assert_eq!(
            approval.presentation_session_id(&known(&["caller"])),
            "caller"
        );
    }

    #[test]
    fn attention_overlay_promotes_live_session_and_leaves_others() {
        let idle = session("idle", NativeSessionStatus::Idle);
        let busy = session("busy", NativeSessionStatus::Busy);
        let exited = session("exited", NativeSessionStatus::Exited);
        let child = project_node("child", vec![busy], vec![]);
        let parent = project_node("parent", vec![idle, exited], vec![child]);

        let overlaid = apply_attention_to_nodes(&[parent], &known(&["idle", "busy", "exited"]));

        assert_eq!(
            overlaid[0].sessions[0].status,
            NativeSessionStatus::Attention
        );
        assert_eq!(overlaid[0].sessions[1].status, NativeSessionStatus::Exited);
        assert_eq!(
            overlaid[0].worktrees[0].sessions[0].status,
            NativeSessionStatus::Attention
        );
    }

    #[test]
    fn empty_pending_ids_are_identity() {
        let node = project_node(
            "project",
            vec![session("s", NativeSessionStatus::Idle)],
            vec![],
        );
        let overlaid = apply_attention_to_nodes(&[node.clone()], &HashSet::new());
        assert_eq!(overlaid, vec![node]);
    }

    #[test]
    fn reconcile_adds_new_and_drops_withdrawn() {
        let mut queue = McpApprovalQueue::new();
        let presented = |id: &str| PresentedApproval {
            id: id.to_string(),
            kind: "write".to_string(),
            title: "t".to_string(),
            body: "b".to_string(),
            caller_session_id: "caller".to_string(),
            target_session_id: Some("target".to_string()),
            requested_at_unix_ms: 1,
        };
        let added = queue.reconcile(&[presented("a"), presented("b")], false);
        assert_eq!(added.len(), 2);
        assert_eq!(
            queue.pending_count_for_session("target", &known(&["caller", "target"])),
            2
        );

        // Second reconcile with only "b": "a" is dropped, no duplicates.
        let added = queue.reconcile(&[presented("b")], false);
        assert!(added.is_empty());
        assert_eq!(
            queue.pending_count_for_session("target", &known(&["caller", "target"])),
            1
        );

        // Unknown kinds are tracked as owned but never become prompts.
        let mut unknown = presented("c");
        unknown.kind = "future-kind".to_string();
        let added = queue.reconcile(&[unknown, presented("b")], false);
        assert!(added.is_empty());
        assert_eq!(
            queue.pending_count_for_session("target", &known(&["caller", "target"])),
            1
        );
    }

    #[test]
    fn scoped_and_local_sources_own_their_rows_independently() {
        let mut queue = McpApprovalQueue::new();
        let presented = |id: &str| PresentedApproval {
            id: id.to_string(),
            kind: "browser".to_string(),
            title: "t".to_string(),
            body: "b".to_string(),
            caller_session_id: "caller".to_string(),
            target_session_id: None,
            requested_at_unix_ms: 1,
        };
        queue.reconcile(&[presented("local")], false);
        queue.reconcile(&[presented("scoped")], true);
        // A scoped refresh must not drop the local row and vice versa.
        queue.reconcile(&[presented("scoped")], true);
        assert!(queue
            .pending_for_session("caller", &known(&["caller"]))
            .is_some());
        queue.reconcile(&[presented("local")], false);
        assert_eq!(queue.pending.len(), 2);
    }

    #[test]
    fn answer_routes_and_first_answer_wins() {
        let mut queue = McpApprovalQueue::new();
        let presented = PresentedApproval {
            id: "a".to_string(),
            kind: "write".to_string(),
            title: "t".to_string(),
            body: "b".to_string(),
            caller_session_id: "caller".to_string(),
            target_session_id: None,
            requested_at_unix_ms: 1,
        };
        queue.reconcile(&[presented], false);
        assert_eq!(queue.answer("a"), Some(AnswerRoute::HostOwned));
        // Repeat while in flight: still routed, not duplicated.
        assert_eq!(queue.answer("a"), Some(AnswerRoute::HostOwned));
        assert_eq!(queue.pending.len(), 1);
        queue.settle_answer("a");
        assert_eq!(queue.answer("a"), None);
        assert!(queue.pending.is_empty());
    }

    #[test]
    fn approval_message_prefers_host_copy_then_kind_copy() {
        let mut queue = McpApprovalQueue::new();
        let presented = PresentedApproval {
            id: "a".to_string(),
            kind: "write".to_string(),
            title: "Host title".to_string(),
            body: "Host body".to_string(),
            caller_session_id: "caller".to_string(),
            target_session_id: Some("target".to_string()),
            requested_at_unix_ms: 1,
        };
        let added = queue.reconcile(&[presented], false);
        let display = |id: &str| format!("name:{id}");
        assert_eq!(
            queue.approval_message(&added[0], &display),
            ("Host title".to_string(), "Host body".to_string())
        );
        queue.messages.clear();
        let (title, _) = queue.approval_message(&added[0], &display);
        assert!(title.contains("name:caller"));
        assert!(title.contains("name:target"));
    }
}
