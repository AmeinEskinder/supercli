//! UI-agnostic state logic from the iOS preview store.
//!
//! Port of the portable parts of `RemotePreviewStore.swift`
//! (`clients/legacy/ios/SupercliIOS`). The `@MainActor @Observable` class
//! itself is SwiftUI-bound and stays out; what ports is the pure logic:
//! - Host capability gates (`supportsResumableArtifactUpload`,
//!   `supportsSessionReorder` — `supportsSessionCreation` already lives in
//!   `protocol.rs`);
//! - the MCP approval ID bookkeeping (answered/revealed sets, filtering,
//!   pruning, per-session presentation resolution);
//! - workspace resolution (current workspace from the advertised list).
//!
//! Everything here is side-effect-free and unit-tested.

use std::collections::{HashMap, HashSet};

use super::protocol::{capabilities, HostProtocolDescriptor};

/// Whether this Host supports capability-gated resumable image upload.
/// Missing descriptors are legacy Hosts and must keep using the shipped
/// one-shot upload route.
///
/// Port of `RemotePreviewStore.supportsResumableArtifactUpload`.
pub fn supports_resumable_artifact_upload(descriptor: Option<&HostProtocolDescriptor>) -> bool {
    match descriptor {
        None => false,
        Some(d) => d.is_compatible() && d.supports(capabilities::ARTIFACT_UPLOAD_RESUMABLE),
    }
}

/// Whether this Host supports capability-gated hold-to-reorder for sidebar
/// sessions. Missing descriptors are legacy Hosts with no session-order
/// route — the rows keep their long-press-for-organize behavior without
/// drag tracking.
///
/// Port of `RemotePreviewStore.supportsSessionReorder`.
pub fn supports_session_reorder(descriptor: Option<&HostProtocolDescriptor>) -> bool {
    match descriptor {
        None => false,
        Some(d) => d.is_compatible() && d.supports(capabilities::SESSION_ORDER_SET),
    }
}

/// Session that should show the in-pane prompt and the attention badge.
/// Write grants present on the destination so the user sees where input
/// would land; other kinds have no destination and present on the caller.
/// A missing/unknown destination falls back to the caller.
///
/// Port of `RemotePendingApproval.presentationSessionID(knownIDs:)`.
pub fn presentation_session_id(
    target_session_id: Option<&str>,
    caller_session_id: &str,
    known_ids: &HashSet<String>,
) -> String {
    if let Some(target) = target_session_id {
        if known_ids.contains(target) {
            return target.to_string();
        }
    }
    caller_session_id.to_string()
}

/// Tracks which MCP approval prompts this Controller has already answered
/// or revealed, so polling never re-shows them.
///
/// Port of the `answeredApprovalIDs` / `revealedApprovalIDs` bookkeeping in
/// `RemotePreviewStore`.
#[derive(Debug, Clone, Default)]
pub struct ApprovalTracker {
    /// Approval ids answered from this phone. The answer POST wins the race
    /// against the next bootstrap poll, so answered prompts hide immediately.
    answered: HashSet<String>,
    /// Approval ids this Controller has already opened the presentation
    /// session for. New ids reveal once; a later poll must not yank the user
    /// back if they navigated away while the prompt is still pending.
    revealed: HashSet<String>,
}

impl ApprovalTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an answer. Returns true if this id was newly answered.
    pub fn mark_answered(&mut self, id: &str) -> bool {
        self.answered.insert(id.to_string())
    }

    /// Record a reveal. Returns true if this id was newly revealed.
    pub fn mark_revealed(&mut self, id: &str) -> bool {
        self.revealed.insert(id.to_string())
    }

    pub fn is_answered(&self, id: &str) -> bool {
        self.answered.contains(id)
    }

    pub fn was_revealed(&self, id: &str) -> bool {
        self.revealed.contains(id)
    }

    /// IDs waiting on the Mac, minus ones already answered from this phone.
    ///
    /// Port of `RemotePreviewStore.pendingApprovals`.
    pub fn pending<'a>(&self, advertised: &'a [ApprovalSummary]) -> Vec<&'a ApprovalSummary> {
        advertised
            .iter()
            .filter(|a| !self.answered.contains(&a.id))
            .collect()
    }

    /// Prune answered ids once the Mac's bootstrap stops reporting them.
    /// Returns the number of ids pruned.
    pub fn prune_answered(&mut self, advertised_ids: &HashSet<String>) -> usize {
        let before = self.answered.len();
        self.answered.retain(|id| advertised_ids.contains(id));
        before - self.answered.len()
    }

    /// Session ids the Host snapshot knows about.
    pub fn known_session_ids<'a>(
        session_ids: impl IntoIterator<Item = &'a str>,
    ) -> HashSet<String> {
        session_ids.into_iter().map(str::to_string).collect()
    }

    /// Does `session_id` need MCP approval attention (badge / in-pane prompt)?
    pub fn session_needs_attention(
        &self,
        session_id: &str,
        advertised: &[ApprovalSummary],
        known_ids: &HashSet<String>,
    ) -> bool {
        self.pending(advertised).iter().any(|a| {
            presentation_session_id(
                a.target_session_id.as_deref(),
                &a.caller_session_id,
                known_ids,
            ) == session_id
        })
    }

    /// First pending approval presenting on `session_id`, if any.
    pub fn pending_approval_for<'a>(
        &self,
        session_id: &str,
        advertised: &'a [ApprovalSummary],
        known_ids: &HashSet<String>,
    ) -> Option<&'a ApprovalSummary> {
        self.pending(advertised).into_iter().find(|a| {
            presentation_session_id(
                a.target_session_id.as_deref(),
                &a.caller_session_id,
                known_ids,
            ) == session_id
        })
    }

    /// Number of pending approvals presenting on `session_id`.
    pub fn pending_approval_count(
        &self,
        session_id: &str,
        advertised: &[ApprovalSummary],
        known_ids: &HashSet<String>,
    ) -> usize {
        self.pending(advertised)
            .iter()
            .filter(|a| {
                presentation_session_id(
                    a.target_session_id.as_deref(),
                    &a.caller_session_id,
                    known_ids,
                ) == session_id
            })
            .count()
    }
}

/// Minimal approval summary for the tracker (mirrors the fields
/// `RemotePreviewStore` reads off `RemotePendingApproval`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalSummary {
    pub id: String,
    pub caller_session_id: String,
    pub target_session_id: Option<String>,
}

/// Minimal workspace summary for resolution (mirrors the fields
/// `RemotePreviewStore` reads off `RemoteWorkspaceSummary`).
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
    pub is_current: bool,
}

/// The workspace currently being served over this connection — the Mac's
/// own workspace by default, or the one this device switched to. Resolved
/// from the Host's authoritative `isCurrent` flag in the latest bootstrap.
///
/// Port of `RemotePreviewStore.currentWorkspace`.
pub fn current_workspace(workspaces: &[WorkspaceSummary]) -> Option<&WorkspaceSummary> {
    workspaces
        .iter()
        .find(|w| w.is_current)
        .or_else(|| workspaces.first())
}

/// Whether the connected Mac advertises more than one local workspace, so a
/// workspace picker is worth showing.
///
/// Port of `RemotePreviewStore.hasMultipleWorkspaces`.
pub fn has_multiple_workspaces(workspaces: &[WorkspaceSummary]) -> bool {
    workspaces.len() > 1
}

/// Index workspaces by id, mirroring the lookup tables the store builds.
pub fn index_workspaces_by_id(
    workspaces: &[WorkspaceSummary],
) -> HashMap<String, &WorkspaceSummary> {
    workspaces.iter().map(|w| (w.id.clone(), w)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approval(id: &str, caller: &str, target: Option<&str>) -> ApprovalSummary {
        ApprovalSummary {
            id: id.to_string(),
            caller_session_id: caller.to_string(),
            target_session_id: target.map(str::to_string),
        }
    }

    #[test]
    fn presentation_session_prefers_known_target_falls_back_to_caller() {
        let known: HashSet<String> = ["s1".to_string(), "s2".to_string()].into_iter().collect();
        // Known target wins.
        assert_eq!(presentation_session_id(Some("s2"), "s1", &known), "s2");
        // Unknown target falls back to caller.
        assert_eq!(presentation_session_id(Some("s9"), "s1", &known), "s1");
        // No target falls back to caller.
        assert_eq!(presentation_session_id(None, "s1", &known), "s1");
    }

    #[test]
    fn pending_hides_answered_approvals() {
        let mut tracker = ApprovalTracker::new();
        let advertised = vec![approval("a1", "s1", None), approval("a2", "s1", Some("s2"))];
        assert_eq!(tracker.pending(&advertised).len(), 2);
        assert!(tracker.mark_answered("a1"));
        assert!(!tracker.mark_answered("a1"), "second mark is not new");
        let pending = tracker.pending(&advertised);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "a2");
    }

    #[test]
    fn prune_answered_drops_ids_the_mac_stopped_reporting() {
        let mut tracker = ApprovalTracker::new();
        tracker.mark_answered("a1");
        tracker.mark_answered("a2");
        let advertised: HashSet<String> = ["a2".to_string()].into_iter().collect();
        assert_eq!(tracker.prune_answered(&advertised), 1);
        assert!(!tracker.is_answered("a1"));
        assert!(tracker.is_answered("a2"));
    }

    #[test]
    fn session_attention_and_lookup_use_presentation_session() {
        let tracker = ApprovalTracker::new();
        let known: HashSet<String> = ["s1".to_string(), "s2".to_string()].into_iter().collect();
        let advertised = vec![
            approval("a1", "s1", Some("s2")), // presents on s2
            approval("a2", "s1", None),       // presents on s1
        ];
        assert!(tracker.session_needs_attention("s2", &advertised, &known));
        assert!(tracker.session_needs_attention("s1", &advertised, &known));
        assert!(!tracker.session_needs_attention("s3", &advertised, &known));
        assert_eq!(
            tracker
                .pending_approval_for("s2", &advertised, &known)
                .unwrap()
                .id,
            "a1"
        );
        assert_eq!(tracker.pending_approval_count("s1", &advertised, &known), 1);
        assert_eq!(tracker.pending_approval_count("s2", &advertised, &known), 1);
    }

    #[test]
    fn revealed_ids_reveal_once() {
        let mut tracker = ApprovalTracker::new();
        assert!(tracker.mark_revealed("a1"));
        assert!(!tracker.mark_revealed("a1"));
        assert!(tracker.was_revealed("a1"));
        assert!(!tracker.was_revealed("a2"));
    }

    #[test]
    fn current_workspace_prefers_is_current_falls_back_to_first() {
        let workspaces = vec![
            WorkspaceSummary {
                id: "w1".to_string(),
                name: "A".to_string(),
                is_current: false,
            },
            WorkspaceSummary {
                id: "w2".to_string(),
                name: "B".to_string(),
                is_current: true,
            },
        ];
        assert_eq!(current_workspace(&workspaces).unwrap().id, "w2");
        let workspaces = vec![WorkspaceSummary {
            id: "w1".to_string(),
            name: "A".to_string(),
            is_current: false,
        }];
        assert_eq!(current_workspace(&workspaces).unwrap().id, "w1");
        let empty: Vec<WorkspaceSummary> = vec![];
        assert!(current_workspace(&empty).is_none());
    }

    #[test]
    fn has_multiple_workspaces_counts() {
        let one = vec![WorkspaceSummary {
            id: "w1".to_string(),
            name: "A".to_string(),
            is_current: true,
        }];
        let two = vec![
            WorkspaceSummary {
                id: "w1".to_string(),
                name: "A".to_string(),
                is_current: true,
            },
            WorkspaceSummary {
                id: "w2".to_string(),
                name: "B".to_string(),
                is_current: false,
            },
        ];
        assert!(!has_multiple_workspaces(&one));
        assert!(has_multiple_workspaces(&two));
        assert!(!has_multiple_workspaces(&[]));
    }
}
