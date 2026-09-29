//! Sidebar ordering kernel ported from the legacy Swift store module
//! (`SupercliStore`, `clients/legacy/native/SupercliNative`).
//!
//! Covers the native sidebar's project tree build, drag-reorder overlays,
//! pinned-partition assembly, and the pure ordering math the store's
//! `SupercliStore` keeps in Foundation-only helpers:
//! `rebuildTree`, `applyProjectOrderOverlay`, `pruneProjectOrderOverlays`,
//! `applySessionOrderOverlay`, `sessionsSortedByRecentActivity`,
//! `projectInsertionOrder`, `projectSiblingMove`, `projectSiblingInsertion`,
//! `combinedSessionOrder`, `applyingRelativeIDOrder`, `replacingSessionID`,
//! `inserting(_:below:in:)`, `orderedPinnedSessions`, `orderedSessions`,
//! `applyPinnedRecordOrder`, `rebuildPins` (merge/group/order kernel),
//! `effectiveProjectID`, `renderedPinnedItems` (local scope),
//! `advertisedSessionOrder`, `toggleProjectExpanded` (expansion set),
//! `isDateSorted`/`setSessionDateSorted` (sort-mode set), and the
//! `projectOrderKey`/`sessionOrderKey`/`pinnedOrderKey` key formats.
//!
//! The session/project row types are lightweight local views
//! (`SidebarSession`, `SidebarProject`, `SidebarPinRecord`): the crate does
//! not depend on `supercli-core`, and the lifecycle status reuses
//! [`crate::remote_dto_adapters::SessionStatus`] (which already mirrors the
//! Swift `SessionStatus`) instead of introducing a duplicate enum.
//!
//! Deliberately NOT ported (not Foundation-only / not portable):
//! `UserDefaults` persistence, the `app-state.json` locked writes
//! (`editPresetStateAnnouncing`), the `~/.supercli/*-order.json` shared-file
//! reads/writes, the remote-Host verb paths (`commitRemoteProjectOrder`,
//! `routesProjectVerbThroughHost`), SwiftUI animations/transactions, the
//! `sidebarLists` truncation window, archive-section logic, pane-layout
//! reconciliation, and activity logging. Those stay with the UI/host layers.
//!
//! Web-safe: std + serde/serde_json only; compiles for
//! `wasm32-unknown-unknown`.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::remote_dto_adapters::SessionStatus;

/// A project row as the sidebar ordering kernel sees it.
///
/// Field-for-field view of the Swift `Project` (Models.swift) pieces the
/// ordering logic reads: identity, parent link, file `sort_order`, folder
/// and worktree markers, and the cross-frontend `pinned_at` group marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidebarProject {
    pub id: String,
    #[serde(default)]
    pub parent_project_id: Option<String>,
    /// File `sort_order`; absent sorts as 0, exactly `sortOrder ?? 0`.
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub is_folder: bool,
    #[serde(default)]
    pub worktree_branch: Option<String>,
    /// Cross-frontend pin marker for plain groups (the pin truth; the
    /// `SidebarPinRecord` is only an ordering entry).
    #[serde(default)]
    pub pinned_at: Option<u64>,
}

impl SidebarProject {
    /// Only plain organizational child groups accept a running session drop.
    /// Worktree children need an explicit restart/resume to change checkout,
    /// while top-level projects remain reorder targets rather than filing
    /// targets. Mirrors Swift `Project.acceptsSessionDrop`.
    pub fn accepts_session_drop(&self) -> bool {
        self.parent_project_id.is_some() && self.worktree_branch.is_none() && self.is_folder
    }

    /// True when this project is a git-worktree checkout of its parent.
    /// Mirrors Swift `Project.isWorktree`.
    pub fn is_worktree(&self) -> bool {
        self.worktree_branch.is_some() && self.parent_project_id.is_some()
    }
}

/// A session row as the sidebar ordering kernel sees it: identity, launch
/// project, optional project-override filing marker, creation stamp, the
/// canonical recent-activity stamp, and lifecycle status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarSession {
    pub id: String,
    pub project_id: String,
    /// `project-override.json` marker: display + ordering only.
    pub project_override_id: Option<String>,
    pub created_at: i64,
    /// Canonical recent timestamp (creation floor, latest activity, final
    /// manifest update). Running heartbeats are deliberately absent.
    pub lifecycle_at_ms: Option<i64>,
    pub status: SessionStatus,
}

/// One `pinned_sessions` record: the durable mixed pinned order (pinned
/// sessions and pinned child groups interleaved) for a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidebarPinRecord {
    pub key: String,
    pub project_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub pinned_at: u64,
}

impl SidebarPinRecord {
    /// `key` is `"session:<session-id>"`; pins sort newest-first by
    /// `pinned_at`. Mirrors Swift `PinnedSidebarSession.key(forSessionID:)`.
    pub fn key_for_session_id(session_id: &str) -> String {
        format!("session:{session_id}")
    }

    /// A pinned child GROUP gets a record with `key = "project:<group-id>"`
    /// and no `session_id`. Mirrors Swift
    /// `PinnedSidebarSession.key(forProjectID:)`.
    pub fn key_for_project_id(project_id: &str) -> String {
        format!("project:{project_id}")
    }

    /// The pinned child group's project id for a `"project:"` record; `None`
    /// for ordinary session pins. Mirrors Swift
    /// `PinnedSidebarSession.pinnedProjectID`.
    pub fn pinned_project_id(&self) -> Option<&str> {
        if self.session_id.is_some() {
            return None;
        }
        let id = self.key.strip_prefix("project:")?;
        if id.is_empty() {
            None
        } else {
            Some(id)
        }
    }

    /// The sidebar row this record ranks: the session id, or the pinned
    /// child group's project id. Mirrors Swift
    /// `PinnedSidebarSession.orderTargetID`.
    pub fn order_target_id(&self) -> Option<&str> {
        self.session_id
            .as_deref()
            .or_else(|| self.pinned_project_id())
    }
}

/// A top-level project plus its sessions and worktree children, ready for
/// the sidebar to render. Mirrors Swift `ProjectNode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarProjectNode {
    pub project: SidebarProject,
    pub sessions: Vec<SidebarSession>,
    pub worktrees: Vec<SidebarProjectNode>,
}

impl SidebarProjectNode {
    pub fn id(&self) -> &str {
        &self.project.id
    }

    /// Mirrors Swift `ProjectNode.hasAnyContent`.
    pub fn has_any_content(&self) -> bool {
        !self.sessions.is_empty() || !self.worktrees.is_empty()
    }
}

/// One row in a project's regular (non-pinned) sidebar section: a session
/// or a child folder (group / worktree). Custom order interleaves them.
/// Mirrors Swift `SidebarMixedItem`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarMixedItem {
    Session(SidebarSession),
    Folder(SidebarProjectNode),
}

impl SidebarMixedItem {
    pub fn id(&self) -> &str {
        match self {
            SidebarMixedItem::Session(session) => &session.id,
            SidebarMixedItem::Folder(node) => node.id(),
        }
    }
}

/// Native drag-reorder overlay keys, merged over the file/derived order at
/// read time. Mirrors Swift `projectOrderKey(forParent:)`.
pub fn project_order_key(parent_id: Option<&str>) -> String {
    match parent_id {
        Some(parent) => format!("supercli.native.projectOrder.{parent}"),
        None => "supercli.native.projectOrder".to_string(),
    }
}

/// Mirrors Swift `sessionOrderKey(_:)`.
pub fn session_order_key(project_id: &str) -> String {
    format!("supercli.native.sessionOrder.{project_id}")
}

/// Legacy native pin-order fallback key. Mirrors Swift `pinnedOrderKey(_:)`.
pub fn pinned_order_key(project_id: &str) -> String {
    format!("supercli.native.pinnedOrder.{project_id}")
}

/// Overlay precedence shared by the project, session, and pinned overlays:
/// an in-flight drag preview outranks everything, then the cross-frontend
/// shared order when it knows this sibling set, then the local overlay.
/// Mirrors the `preview ?? shared(non-empty) ?? local` chains in
/// `applyProjectOrderOverlay` / `applySessionOrderOverlay` /
/// `applyPinnedOrderOverlay`.
pub fn resolve_overlay<'a>(
    preview: Option<&'a [String]>,
    shared: Option<&'a [String]>,
    local: Option<&'a [String]>,
) -> Option<&'a [String]> {
    if let Some(preview) = preview {
        return Some(preview);
    }
    match shared {
        Some(shared) if !shared.is_empty() => Some(shared),
        _ => local,
    }
}

/// Like [`resolve_overlay`], but an empty winning overlay also yields
/// `None` (the Swift call sites `guard !overlay.isEmpty else { return base }`).
pub fn effective_overlay<'a>(
    preview: Option<&'a [String]>,
    shared: Option<&'a [String]>,
    local: Option<&'a [String]>,
) -> Option<&'a [String]> {
    match resolve_overlay(preview, shared, local) {
        Some(overlay) if !overlay.is_empty() => Some(overlay),
        _ => None,
    }
}

enum RestPlacement {
    /// Project variant: overlay ids first, unknown/new projects append in
    /// base order — matching `add_project`'s max(`sort_order`)+1 append.
    Last,
    /// Session variant: ids NOT in the overlay are newer than every overlay
    /// entry (the overlay snapshots the whole visible list at drag time), so
    /// they keep base order ABOVE the hand-ordered block — preserving "new
    /// sessions appear at the top".
    First,
}

/// Shared rank-application math: overlay ids, restricted to ids present in
/// `base`, in overlay order; every other base id keeps its relative order in
/// the rest block. Unknown overlay ids are skipped, NOT GC'd at read time: a
/// project/session can be merely not-yet-known at read time, and stale ids
/// drop out when the next drag persists a fresh order.
fn apply_rank_overlay(
    base: &[String],
    overlay: &[String],
    placement: RestPlacement,
) -> Vec<String> {
    let base_ids: HashSet<&str> = base.iter().map(String::as_str).collect();
    let known: Vec<&str> = overlay
        .iter()
        .map(String::as_str)
        .filter(|id| base_ids.contains(id))
        .collect();
    if known.is_empty() {
        return base.to_vec();
    }
    // Later duplicates overwrite, exactly `for (i, id) in known.enumerated()
    // { rank[id] = i }`.
    let mut rank: HashMap<&str, usize> = HashMap::new();
    for (index, id) in known.iter().enumerate() {
        rank.insert(id, index);
    }
    // Stable sort: Swift's `sorted` keeps base relative order on rank ties.
    let mut ordered: Vec<String> = base
        .iter()
        .filter(|id| rank.contains_key(id.as_str()))
        .cloned()
        .collect();
    ordered.sort_by_key(|id| rank[id.as_str()]);
    let rest: Vec<String> = base
        .iter()
        .filter(|id| !rank.contains_key(id.as_str()))
        .cloned()
        .collect();
    match placement {
        RestPlacement::First => {
            let mut out = rest;
            out.extend(ordered);
            out
        }
        RestPlacement::Last => {
            let mut out = ordered;
            out.extend(rest);
            out
        }
    }
}

/// Project-order overlay application: hand-ordered projects first in overlay
/// order, unknown/new projects appended in base order. This is the pure
/// kernel of Swift `applyProjectOrderOverlay`.
pub fn apply_project_order(base: &[String], overlay: &[String]) -> Vec<String> {
    apply_rank_overlay(base, overlay, RestPlacement::Last)
}

/// Session-order overlay application: sessions missing from the overlay
/// stay newest-first ABOVE the hand-ordered block. This is the pure kernel
/// of Swift `applySessionOrderOverlay` (without its date-sort branch).
pub fn apply_session_order(base: &[String], overlay: &[String]) -> Vec<String> {
    apply_rank_overlay(base, overlay, RestPlacement::First)
}

/// Port of Swift's static `orderedPinnedSessions`: pin records rank by their
/// sidebar row id (the session id, or the pinned child group's project id
/// for `"project:"` records). The shared order wins when it contains pin
/// ranks; the local overlay is the fallback. Unranked records keep their
/// relative order below the ranked block, so a freshly pinned row lands at
/// the bottom of the pin list.
pub fn ordered_pinned_records(
    base: &[SidebarPinRecord],
    shared_order: Option<&[String]>,
    local_order: Option<&[String]>,
) -> Vec<SidebarPinRecord> {
    let base_ids: HashSet<&str> = base
        .iter()
        .filter_map(|pin| pin.order_target_id())
        .collect();
    let shared_known: Vec<&str> = shared_order
        .unwrap_or(&[])
        .iter()
        .map(String::as_str)
        .filter(|id| base_ids.contains(id))
        .collect();
    let overlay: Vec<&str> = if shared_known.is_empty() {
        local_order
            .unwrap_or(&[])
            .iter()
            .map(String::as_str)
            .collect()
    } else {
        shared_known
    };
    if overlay.is_empty() {
        return base.to_vec();
    }
    let known: Vec<&str> = overlay
        .into_iter()
        .filter(|id| base_ids.contains(id))
        .collect();
    if known.is_empty() {
        return base.to_vec();
    }
    let mut rank: HashMap<&str, usize> = HashMap::new();
    for (index, id) in known.iter().enumerate() {
        rank.insert(id, index);
    }
    let is_ranked = |pin: &SidebarPinRecord| -> bool {
        pin.order_target_id()
            .is_some_and(|target| rank.contains_key(target))
    };
    // Stable sorts keep base relative order on ties, as Swift's `sorted`.
    let mut ordered: Vec<SidebarPinRecord> =
        base.iter().filter(|pin| is_ranked(pin)).cloned().collect();
    ordered.sort_by_key(|pin| rank[pin.order_target_id().unwrap_or_default()]);
    let rest: Vec<SidebarPinRecord> = base.iter().filter(|pin| !is_ranked(pin)).cloned().collect();
    ordered.extend(rest);
    ordered
}

/// Exact gap insertion used by the project drag. The dragged id is removed,
/// then inserted above (`below = false`) or below (`below = true`) the
/// target's slot. Port of Swift's static `projectInsertionOrder`.
pub fn project_insertion_order(
    ids: &[String],
    dragged_id: &str,
    target_id: &str,
    below: bool,
) -> Option<Vec<String>> {
    if dragged_id == target_id || !ids.iter().any(|id| id == dragged_id) {
        return None;
    }
    let mut result: Vec<String> = ids
        .iter()
        .filter(|id| id.as_str() != dragged_id)
        .cloned()
        .collect();
    let target_index = result.iter().position(|id| id == target_id)?;
    result.insert(target_index + usize::from(below), dragged_id.to_string());
    Some(result)
}

/// Shared sibling-reorder math behind `moveProject` / `moveSession` /
/// `movePinnedSession`: the dragged id takes the target's slot among
/// siblings. A cross-parent pair is a no-op for the caller to enforce (the
/// Swift guards compare `parentProjectID` before calling this math).
pub fn sibling_move_to_target_slot(
    ids: &[String],
    dragged_id: &str,
    target_id: &str,
) -> Option<Vec<String>> {
    if dragged_id == target_id {
        return None;
    }
    let from = ids.iter().position(|id| id == dragged_id)?;
    let to = ids.iter().position(|id| id == target_id)?;
    let mut out = ids.to_vec();
    out.remove(from);
    // `to` is the target's ORIGINAL index: after the removal the dragged id
    // occupies exactly the slot the target held — Swift `projectSiblingMove`.
    out.insert(to, dragged_id.to_string());
    Some(out)
}

/// One combined shared rank list: pinned ids then regular ids, deduplicated.
/// Each frontend filters it into pinned/running/stopped buckets, so
/// publishing one bucket must preserve the ranks of the others. Port of
/// Swift's static `combinedSessionOrder`.
pub fn combined_session_order(pinned_ids: &[String], regular_ids: &[String]) -> Vec<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    pinned_ids
        .iter()
        .chain(regular_ids.iter())
        .filter(|id| seen.insert(id.as_str()))
        .cloned()
        .collect()
}

/// Replace only the slots occupied by `preferred` ids. This lets a held
/// session order move around child-folder ids in a mixed sidebar rank
/// without moving those folders themselves. Port of Swift's static
/// `applyingRelativeIDOrder`.
pub fn applying_relative_id_order(preferred: &[String], base: &[String]) -> Vec<String> {
    let known: HashSet<&str> = base.iter().map(String::as_str).collect();
    let replacing: HashSet<&str> = preferred.iter().map(String::as_str).collect();
    let mut slots = preferred.iter().filter(|id| known.contains(id.as_str()));
    base.iter()
        .map(|id| {
            if replacing.contains(id.as_str()) {
                slots.next().cloned().unwrap_or_else(|| id.clone())
            } else {
                id.clone()
            }
        })
        .collect()
}

/// Swap one id for another in an order (session id rotation across
/// restart/replace). Port of Swift's static `replacingSessionID`.
pub fn replacing_session_id(
    order: Option<&[String]>,
    old_id: &str,
    new_id: &str,
) -> Option<Vec<String>> {
    let order = order?;
    let mut out = order.to_vec();
    let rank = out.iter().position(|id| id == old_id)?;
    out[rank] = new_id.to_string();
    Some(out)
}

/// Place `ids` immediately after `host_id`. Existing occurrences of those
/// ids are removed first. If the host is missing, the ids append. Port of
/// Swift's static `inserting(_:below:in:)`.
pub fn insert_ids_below(ids: &[String], host_id: &str, list: &[String]) -> Vec<String> {
    let moving: Vec<&String> = ids.iter().filter(|id| id.as_str() != host_id).collect();
    if moving.is_empty() {
        return list.to_vec();
    }
    let moving_ids: HashSet<&str> = moving.iter().map(|id| id.as_str()).collect();
    let mut result: Vec<String> = list
        .iter()
        .filter(|id| !moving_ids.contains(id.as_str()))
        .cloned()
        .collect();
    match result.iter().position(|id| id == host_id) {
        Some(index) => {
            let owned: Vec<String> = moving.into_iter().cloned().collect();
            result.splice(index + 1..index + 1, owned);
        }
        None => result.extend(moving.into_iter().cloned()),
    }
    result
}

/// Port of Swift's private `orderedSessions`: ranked sessions take rank
/// order; unranked sessions (newer than the manual-order snapshot) keep
/// newest-first ABOVE the ranked block. An empty manual order is plain
/// newest-first.
pub fn order_sessions_manual(
    sessions: &[SidebarSession],
    manual_order: &[String],
) -> Vec<SidebarSession> {
    let mut out = sessions.to_vec();
    if manual_order.is_empty() {
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        return out;
    }
    let mut rank: HashMap<&str, usize> = HashMap::new();
    for (index, id) in manual_order.iter().enumerate() {
        rank.insert(id.as_str(), index);
    }
    // Stable sort: `createdAt` ties and rank ties keep input order.
    out.sort_by(
        |a, b| match (rank.get(a.id.as_str()), rank.get(b.id.as_str())) {
            (Some(ra), Some(rb)) => ra.cmp(rb),
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, None) => b.created_at.cmp(&a.created_at),
        },
    );
    out
}

fn is_working(session: &SidebarSession, restarting_ids: &HashSet<String>) -> bool {
    matches!(
        session.status,
        SessionStatus::Starting | SessionStatus::Busy
    ) || restarting_ids.contains(session.id.as_str())
}

/// Pure shared rank for the Recent page shape and per-group "Recently
/// updated" mode. A live-but-idle session is NOT privileged over a more
/// recent exited one; only work currently in progress gets the leading
/// tier. Id is the deterministic final tie-break across rescans/frontends.
/// Port of Swift's static `sessionsSortedByRecentActivity`.
pub fn sessions_sorted_by_recent_activity(
    sessions: &[SidebarSession],
    restarting_ids: &HashSet<String>,
) -> Vec<SidebarSession> {
    let mut out = sessions.to_vec();
    out.sort_by(|a, b| {
        let a_working = is_working(a, restarting_ids);
        let b_working = is_working(b, restarting_ids);
        if a_working != b_working {
            return b_working.cmp(&a_working);
        }
        let a_stamp = a.created_at.max(a.lifecycle_at_ms.unwrap_or(0));
        let b_stamp = b.created_at.max(b.lifecycle_at_ms.unwrap_or(0));
        if a_stamp != b_stamp {
            return b_stamp.cmp(&a_stamp);
        }
        a.id.cmp(&b.id)
    });
    out
}

/// A project-override marker files the session under another project
/// (group/worktree folder) — display + ordering only, and only when the
/// target still exists; a stale marker falls back to the manifest project
/// instead of orphaning the row. Port of Swift's `effectiveProjectID`.
pub fn effective_project_id(
    session: &SidebarSession,
    known_project_ids: &HashSet<String>,
) -> String {
    match session.project_override_id.as_deref() {
        Some(target) if known_project_ids.contains(target) => target.to_string(),
        _ => session.project_id.clone(),
    }
}

/// Scan inputs for [`SidebarOrderState::build_tree`]. The shared orders are
/// the cross-frontend lists (read by the Swift from the shared files inside
/// the overlay helpers); the local overlays live on the state itself.
#[derive(Debug)]
pub struct TreeScan<'a> {
    /// Sessions whose removal is in flight vanish from the sidebar
    /// immediately; the kill/cleanup then runs silently in the background.
    pub removing_ids: &'a HashSet<String>,
    /// Restart snapshots keep their row throughout the teardown + respawn so
    /// it never blinks out; injected only once the live scan stops producing
    /// the row. Mirrors Swift `restartPlaceholders`.
    pub restart_placeholders: &'a [SidebarSession],
    /// Sessions with a host-owned restart transition in flight; ranked
    /// "working" by the date-sort rank. Mirrors Swift `restartingSessionIDs`.
    pub restarting_ids: &'a HashSet<String>,
    /// Cross-frontend project orders by parent (`None` = top level).
    pub shared_project_orders: &'a HashMap<Option<String>, Vec<String>>,
    /// Cross-frontend session orders by project.
    pub shared_session_orders: &'a HashMap<String, Vec<String>>,
}

/// In-memory sidebar ordering state: the drag-reorder overlays, in-flight
/// drag previews, per-project sort mode, and project expansion.
///
/// This is the portable kernel of the ordering state `SupercliStore` keeps
/// across `AppDefaults`, `sessionOrderPreviews`, `projectOrderPreview`,
/// `dateSortedProjectIDs`, and `expandedProjectIDs`. Persistence (defaults,
/// shared files, app-state lock) and the remote-Host verb paths are left to
/// the caller: mutating methods return exactly what must be persisted.
#[derive(Debug, Default)]
pub struct SidebarOrderState {
    /// Local project-order overlays by parent (`None` = top level).
    project_order: HashMap<Option<String>, Vec<String>>,
    /// Local regular-session-order overlays by project.
    session_order: HashMap<String, Vec<String>>,
    /// Legacy local pinned-partition overlays by project (migration
    /// fallback; the combined shared order is the primary rank).
    pinned_order: HashMap<String, Vec<String>>,
    /// In-flight project drag preview: (parent, visible ids, dragged id).
    /// Outranks every persisted order while set.
    project_preview: Option<(Option<String>, Vec<String>, String)>,
    /// In-flight session drag previews by project (combined pin + regular
    /// order, exactly as the preview renders).
    session_previews: HashMap<String, Vec<String>>,
    /// Projects rendering the date-sorted (recent-activity) list. A manual
    /// reorder commit flips the project back to custom order.
    date_sorted: HashSet<String>,
    /// Expanded project ids in the sidebar tree.
    expanded: HashSet<String>,
}

/// What a session-preview commit must persist. Mirrors the local branch of
/// Swift `commitSessionReorder`: the preview is dropped first, then the
/// visible partition orders are captured for persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPreviewCommit {
    /// True when the committed drag was in the pinned partition.
    pub pinned_partition: bool,
    /// Visible pinned-partition ids (mixed sessions + pinned groups).
    pub pinned_ids: Vec<String>,
    /// Visible regular-section ids.
    pub regular_ids: Vec<String>,
    /// True when committing a regular-section drag flipped a date-sorted
    /// list back to custom order (hand-ordering a date-sorted list IS
    /// choosing custom order; pinned-section commits never flip).
    pub flipped_date_sort: bool,
}

impl SidebarOrderState {
    /// Projects in display order for one sibling set: file `sort_order`
    /// first, then the drag-reorder overlay (preview > shared > local).
    /// Pure kernel of Swift `applyProjectOrderOverlay`.
    pub fn apply_project_order(
        &self,
        base: &[SidebarProject],
        parent_id: Option<&str>,
        shared: Option<&[String]>,
    ) -> Vec<SidebarProject> {
        let preview = self
            .project_preview
            .as_ref()
            .filter(|(parent, _, _)| parent.as_deref() == parent_id)
            .map(|(_, ids, _)| ids.as_slice());
        let local = self
            .project_order
            .get(&parent_id.map(str::to_string))
            .map(Vec::as_slice);
        let ordered_ids = match effective_overlay(preview, shared, local) {
            Some(overlay) => {
                let base_ids: Vec<String> = base.iter().map(|project| project.id.clone()).collect();
                apply_project_order(&base_ids, overlay)
            }
            None => base.iter().map(|project| project.id.clone()).collect(),
        };
        let position: HashMap<&str, usize> = ordered_ids
            .iter()
            .enumerate()
            .map(|(index, id)| (id.as_str(), index))
            .collect();
        let mut out = base.to_vec();
        // Stable: ids are unique, so every project has a position.
        out.sort_by_key(|project| position[project.id.as_str()]);
        out
    }

    /// Sessions in display order for a project: newest-first, then the
    /// drag-reorder overlay (preview > shared > local) with new sessions
    /// above the hand-ordered block. A date-sorted project without an
    /// in-flight preview renders the recent-activity rank instead — the
    /// stored manual order survives for a switch back. Pure kernel of Swift
    /// `applySessionOrderOverlay`.
    pub fn apply_session_order(
        &self,
        base: &[SidebarSession],
        project_id: &str,
        shared: Option<&[String]>,
        restarting_ids: &HashSet<String>,
    ) -> Vec<SidebarSession> {
        let preview = self.session_previews.get(project_id).map(Vec::as_slice);
        if self.date_sorted.contains(project_id) && preview.is_none() {
            return sessions_sorted_by_recent_activity(base, restarting_ids);
        }
        let local = self.session_order.get(project_id).map(Vec::as_slice);
        let Some(overlay) = effective_overlay(preview, shared, local) else {
            return base.to_vec();
        };
        let base_ids: Vec<String> = base.iter().map(|session| session.id.clone()).collect();
        let ordered_ids = apply_session_order(&base_ids, overlay);
        let position: HashMap<&str, usize> = ordered_ids
            .iter()
            .enumerate()
            .map(|(index, id)| (id.as_str(), index))
            .collect();
        let mut out = base.to_vec();
        out.sort_by_key(|session| position[session.id.as_str()]);
        out
    }

    /// Pinned records for a project with the pinned-order overlay applied:
    /// the in-flight preview or shared combined order wins when it carries
    /// pin ranks, else the legacy local pinned overlay. Pure kernel of
    /// Swift `applyPinnedOrderOverlay` / `orderedPinnedSessions`.
    pub fn apply_pinned_order(
        &self,
        base: &[SidebarPinRecord],
        project_id: &str,
        shared: Option<&[String]>,
    ) -> Vec<SidebarPinRecord> {
        let preview = self.session_previews.get(project_id).map(Vec::as_slice);
        let local = self.pinned_order.get(project_id).map(Vec::as_slice);
        let shared_order = match (preview, shared) {
            (Some(preview), _) => Some(preview),
            (None, shared) => shared,
        };
        ordered_pinned_records(base, shared_order, local)
    }

    /// Build the sidebar project tree from a scan. Mirrors Swift
    /// `rebuildTree` minus its store side effects (indexes, activity log,
    /// selection repair, titlebar refresh): sessions with removal in flight
    /// are filtered, restart placeholders are injected, sessions file under
    /// their effective project, each project's sessions sort newest-first
    /// with the session overlay applied, children render as inline folder
    /// rows (worktree checkouts and plain groups) under their `sort_order`
    /// with the project overlay applied.
    pub fn build_tree(
        &self,
        projects: &[SidebarProject],
        sessions: &[SidebarSession],
        scan: &TreeScan<'_>,
    ) -> Vec<SidebarProjectNode> {
        let mut live: Vec<SidebarSession> = sessions
            .iter()
            .filter(|session| !scan.removing_ids.contains(session.id.as_str()))
            .cloned()
            .collect();
        if !scan.restart_placeholders.is_empty() {
            let present: HashSet<String> = live.iter().map(|session| session.id.clone()).collect();
            for snapshot in scan.restart_placeholders {
                if !present.contains(snapshot.id.as_str()) {
                    live.push(snapshot.clone());
                }
            }
        }

        let known_ids: HashSet<String> =
            projects.iter().map(|project| project.id.clone()).collect();
        let mut by_project: HashMap<String, Vec<SidebarSession>> = HashMap::new();
        for session in &live {
            by_project
                .entry(effective_project_id(session, &known_ids))
                .or_default()
                .push(session.clone());
        }
        // Newest-first, exactly `sortSessionsNewestFirst`
        // (`b.created_at - a.created_at`): new launches land at the TOP of
        // the regular list.
        let mut ordered_by_project: HashMap<String, Vec<SidebarSession>> = HashMap::new();
        for (project_id, sessions) in &by_project {
            let mut newest_first = sessions.clone();
            newest_first.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            let shared = scan
                .shared_session_orders
                .get(project_id)
                .map(Vec::as_slice);
            ordered_by_project.insert(
                project_id.clone(),
                self.apply_session_order(&newest_first, project_id, shared, scan.restarting_ids),
            );
        }

        let mut children_of: HashMap<String, Vec<SidebarProject>> = HashMap::new();
        let mut top_level: Vec<SidebarProject> = Vec::new();
        for project in projects {
            match project.parent_project_id.as_deref() {
                Some(parent) => children_of
                    .entry(parent.to_string())
                    .or_default()
                    .push(project.clone()),
                None => top_level.push(project.clone()),
            }
        }
        top_level.sort_by_key(|project| project.sort_order.unwrap_or(0));
        let shared_top = scan.shared_project_orders.get(&None).map(Vec::as_slice);
        let ordered_top = self.apply_project_order(&top_level, None, shared_top);
        ordered_top
            .iter()
            .map(|project| self.build_node(project, &children_of, &ordered_by_project, scan))
            .collect()
    }

    fn build_node(
        &self,
        project: &SidebarProject,
        children_of: &HashMap<String, Vec<SidebarProject>>,
        sessions_by_project: &HashMap<String, Vec<SidebarSession>>,
        scan: &TreeScan<'_>,
    ) -> SidebarProjectNode {
        // Worktree checkouts AND plain groups (organizational child folders)
        // render as inline folder rows.
        let mut child_projects: Vec<SidebarProject> = children_of
            .get(&project.id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|child| child.worktree_branch.is_some() || child.is_folder)
            .collect();
        child_projects.sort_by_key(|child| child.sort_order.unwrap_or(0));
        let shared = scan
            .shared_project_orders
            .get(&Some(project.id.clone()))
            .map(Vec::as_slice);
        let ordered_children = self.apply_project_order(&child_projects, Some(&project.id), shared);
        let worktrees = ordered_children
            .iter()
            .map(|child| self.build_node(child, children_of, sessions_by_project, scan))
            .collect();
        SidebarProjectNode {
            project: project.clone(),
            sessions: sessions_by_project
                .get(&project.id)
                .cloned()
                .unwrap_or_default(),
            worktrees,
        }
    }
}

impl SidebarOrderState {
    /// In-memory move for the project drag path: compute the exact gap
    /// insertion and store it as the preview, which outranks every persisted
    /// order until commit/cancel. Returns true when a preview was stored or
    /// changed. Mirrors Swift `previewProjectMove` (minus the remote-scope
    /// projection and the tree rebuild, which are caller concerns).
    pub fn preview_project_move(
        &mut self,
        parent_id: Option<&str>,
        displayed_ids: &[String],
        dragged_id: &str,
        target_id: &str,
        below: bool,
    ) -> bool {
        let Some(ids) = project_insertion_order(displayed_ids, dragged_id, target_id, below) else {
            return false;
        };
        let key = parent_id.map(str::to_string);
        let unchanged = self
            .project_preview
            .as_ref()
            .is_some_and(|(parent, preview_ids, _)| parent == &key && preview_ids == &ids);
        if unchanged {
            return false;
        }
        self.project_preview = Some((key, ids, dragged_id.to_string()));
        true
    }

    /// Persist the final project drag preview exactly once: capture the
    /// visible sibling order (which still carries the live preview) and drop
    /// the preview's precedence. Returns `(parent_id, final_ids)` for the
    /// caller to persist. Mirrors the local branch of Swift
    /// `commitProjectReorder`.
    pub fn commit_project_preview(&mut self) -> Option<(Option<String>, Vec<String>)> {
        let (parent, ids, _) = self.project_preview.take()?;
        Some((parent, ids))
    }

    /// Roll back a project drag that produced no accepted drop. The
    /// persisted order was never touched. Mirrors Swift
    /// `cancelProjectReorder`'s local branch.
    pub fn cancel_project_preview(&mut self) -> bool {
        self.project_preview.take().is_some()
    }

    /// In-memory move for the session drag path (regular or pinned
    /// partition): rows reorder live, but no shared/local state is written
    /// until [`SidebarOrderState::commit_session_preview`]. Returns true
    /// when the preview was stored or changed. Mirrors Swift
    /// `previewSessionMove` / `previewPinnedSessionMove` (minus archived
    /// filtering and the refresh, which are caller concerns).
    pub fn preview_session_move(
        &mut self,
        project_id: &str,
        pinned_ids: &[String],
        regular_ids: &[String],
        dragged_id: &str,
        target_id: &str,
        in_pinned_partition: bool,
    ) -> bool {
        let (pinned, regular) = if in_pinned_partition {
            let Some(ids) = sibling_move_to_target_slot(pinned_ids, dragged_id, target_id) else {
                return false;
            };
            (ids, regular_ids.to_vec())
        } else {
            let Some(ids) = sibling_move_to_target_slot(regular_ids, dragged_id, target_id) else {
                return false;
            };
            (pinned_ids.to_vec(), ids)
        };
        let preview = combined_session_order(&pinned, &regular);
        if self
            .session_previews
            .get(project_id)
            .is_some_and(|current| *current == preview)
        {
            return false;
        }
        self.session_previews
            .insert(project_id.to_string(), preview);
        true
    }

    /// Persist the final session drag preview exactly once. The preview is
    /// removed first; the caller captures the visible partition orders from
    /// the rendered tree and passes them in for persistence. Mirrors the
    /// local branch of Swift `commitSessionReorder`, including the
    /// date-sort flip: hand-ordering a date-sorted list IS choosing custom
    /// order; pinned-section commits never flip.
    pub fn commit_session_preview(
        &mut self,
        project_id: &str,
        pinned_partition: bool,
        visible_pinned_ids: Vec<String>,
        visible_regular_ids: Vec<String>,
    ) -> Option<SessionPreviewCommit> {
        self.session_previews.remove(project_id)?;
        let flipped_date_sort = !pinned_partition && self.date_sorted.remove(project_id);
        Some(SessionPreviewCommit {
            pinned_partition,
            pinned_ids: visible_pinned_ids,
            regular_ids: visible_regular_ids,
            flipped_date_sort,
        })
    }

    /// Roll back a session drag that produced no accepted drop. The
    /// persisted order was never touched. Mirrors Swift
    /// `cancelSessionReorder`'s local branch.
    pub fn cancel_session_preview(&mut self, project_id: &str) -> bool {
        self.session_previews.remove(project_id).is_some()
    }

    /// Durable sibling-order write for projects (tests and non-drag
    /// callers): store the local overlay; an empty order removes it.
    /// Returns the overlay key for the caller to persist alongside. Mirrors
    /// the local-overlay half of Swift `setProjectOrder`.
    pub fn set_project_order(&mut self, parent_id: Option<&str>, ids: Vec<String>) -> String {
        let key = parent_id.map(str::to_string);
        if ids.is_empty() {
            self.project_order.remove(&key);
        } else {
            self.project_order.insert(key.clone(), ids);
        }
        project_order_key(parent_id)
    }

    /// Durable regular-section session order: stores the regular ids as the
    /// local overlay and returns the combined shared rank (pinned partition
    /// ranks + regular ids) for the caller to persist — publishing one
    /// bucket must preserve the ranks of the others. Mirrors Swift
    /// `setSessionOrder`'s local branch.
    pub fn set_session_order(
        &mut self,
        project_id: &str,
        pinned_ids: &[String],
        regular_ids: Vec<String>,
    ) -> Vec<String> {
        if regular_ids.is_empty() {
            self.session_order.remove(project_id);
        } else {
            self.session_order
                .insert(project_id.to_string(), regular_ids.clone());
        }
        combined_session_order(pinned_ids, &regular_ids)
    }

    /// Durable pinned-partition order (session ids AND pinned child-group
    /// ids): stores the mixed order as the local overlay and returns the
    /// combined shared rank for the caller to persist. The raw-JSON
    /// `pinned_sessions` record rewrite is the caller's job via
    /// [`apply_pinned_record_order`]. Mirrors Swift `setPinnedOrder`'s local
    /// branch.
    pub fn set_pinned_order(
        &mut self,
        project_id: &str,
        pinned_ids: Vec<String>,
        regular_ids: &[String],
    ) -> Vec<String> {
        if pinned_ids.is_empty() {
            self.pinned_order.remove(project_id);
        } else {
            self.pinned_order
                .insert(project_id.to_string(), pinned_ids.clone());
        }
        combined_session_order(&pinned_ids, regular_ids)
    }

    /// Drop removed ids from every project-order overlay; an overlay that
    /// becomes empty — or belongs to a removed parent — is dropped. Port of
    /// Swift `pruneProjectOrderOverlays` over the in-memory overlays.
    pub fn prune_project_overlays(&mut self, removed_ids: &HashSet<String>) {
        if removed_ids.is_empty() {
            return;
        }
        self.project_order.retain(|parent, ids| {
            if parent.as_deref().is_some_and(|id| removed_ids.contains(id)) {
                return false;
            }
            ids.retain(|id| !removed_ids.contains(id.as_str()));
            !ids.is_empty()
        });
    }

    /// Mirror of Swift `toggleProjectExpanded`. Returns the new expanded
    /// state. (Collapsing's "keep hidden row visible" pin drop is a UI
    /// concern and is not modeled here.)
    pub fn toggle_expanded(&mut self, project_id: &str) -> bool {
        if self.expanded.remove(project_id) {
            false
        } else {
            self.expanded.insert(project_id.to_string());
            true
        }
    }

    pub fn is_expanded(&self, project_id: &str) -> bool {
        self.expanded.contains(project_id)
    }

    /// Mirror of Swift `isDateSorted` / `setSessionDateSorted`.
    pub fn is_date_sorted(&self, project_id: &str) -> bool {
        self.date_sorted.contains(project_id)
    }

    pub fn set_date_sorted(&mut self, project_id: &str, date_sorted: bool) {
        if date_sorted {
            self.date_sorted.insert(project_id.to_string());
        } else {
            self.date_sorted.remove(project_id);
        }
    }

    /// Rebuild the per-project pin record groups from the file's
    /// `pinned_sessions` arrays plus the merged pin set (file pins reconciled
    /// with pending native intents by the caller — the merge itself is
    /// `reconciledPinOverrides` in `store_policies`).
    ///
    /// A record is kept only while its target still belongs to that
    /// project: a session pin follows the session's effective group; a
    /// `"project:"` group record needs the group to still exist under this
    /// parent WITH its cross-frontend `pinned_at` marker (a TUI unpin drops
    /// the marker, which retires the record's rank too). The file's array
    /// order is the base; merged-only records append oldest-first; then the
    /// pinned-order overlay applies. Port of Swift `rebuildPins`' pure
    /// kernel.
    pub fn rebuild_pin_groups(
        &self,
        file_pins: &HashMap<String, Vec<SidebarPinRecord>>,
        merged: &[SidebarPinRecord],
        sessions: &HashMap<String, SidebarSession>,
        projects: &HashMap<String, SidebarProject>,
        known_project_ids: &HashSet<String>,
        shared_session_orders: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, Vec<SidebarPinRecord>> {
        let merged_by_key: HashMap<&str, &SidebarPinRecord> =
            merged.iter().map(|pin| (pin.key.as_str(), pin)).collect();
        let mut grouped: HashMap<String, Vec<SidebarPinRecord>> = HashMap::new();
        let mut placed: HashSet<&str> = HashSet::new();
        // Deterministic file-project iteration for a stable base order.
        let mut file_projects: Vec<&String> = file_pins.keys().collect();
        file_projects.sort();
        for file_project_id in file_projects {
            let file_list = &file_pins[file_project_id];
            for file_pin in file_list {
                let Some(pin) = merged_by_key.get(file_pin.key.as_str()) else {
                    continue;
                };
                if pin.project_id != *file_project_id
                    || !placed.insert(pin.key.as_str())
                    || !pin_record_is_valid(pin, sessions, projects, known_project_ids)
                {
                    continue;
                }
                grouped
                    .entry(pin.project_id.clone())
                    .or_default()
                    .push((*pin).clone());
            }
        }
        let mut unplaced: Vec<&SidebarPinRecord> = merged
            .iter()
            .filter(|pin| {
                !placed.contains(pin.key.as_str())
                    && pin_record_is_valid(pin, sessions, projects, known_project_ids)
            })
            .collect();
        // Newly-pinned records append below the file-ordered block, oldest
        // first, so a freshly pinned row lands at the bottom of the pin list.
        unplaced.sort_by(|a, b| {
            a.pinned_at
                .cmp(&b.pinned_at)
                .then_with(|| a.key.cmp(&b.key))
        });
        for pin in unplaced {
            grouped
                .entry(pin.project_id.clone())
                .or_default()
                .push(pin.clone());
        }
        for (project_id, pins) in grouped.iter_mut() {
            let shared = shared_session_orders.get(project_id).map(Vec::as_slice);
            *pins = self.apply_pinned_order(pins, project_id, shared);
        }
        grouped
    }
}

/// A pin record is kept only while its target still belongs to that
/// project. Port of Swift `rebuildPins`' `recordIsValid`.
fn pin_record_is_valid(
    pin: &SidebarPinRecord,
    sessions: &HashMap<String, SidebarSession>,
    projects: &HashMap<String, SidebarProject>,
    known_project_ids: &HashSet<String>,
) -> bool {
    if let Some(session_id) = pin.session_id.as_deref() {
        let Some(session) = sessions.get(session_id) else {
            return false;
        };
        return effective_project_id(session, known_project_ids) == pin.project_id;
    }
    let Some(group_id) = pin.pinned_project_id() else {
        return false;
    };
    let Some(group) = projects.get(group_id) else {
        return false;
    };
    group.accepts_session_drop()
        && group.parent_project_id.as_deref() == Some(pin.project_id.as_str())
        && group.pinned_at.is_some()
    // (Swift compares `group.parentProjectID == pin.projectID`; the
    // Option<String> vs String comparison above is the same check.)
}

/// The pinned partition as ONE mixed list: pinned sessions AND pinned child
/// groups, ordered by the project's pin records. Records whose target is
/// gone are skipped; a pinned group with no ordering record yet (pinned from
/// the TUI, marker only) ranks after every record-ranked row, keeping its
/// current relative order. An in-flight drag preview re-ranks the assembled
/// rows so recordless groups can ride the preview too. A rendered pinned
/// session never drops out of the partition even if its record is missing.
/// Port of Swift `renderedPinnedItems`' local branch.
pub fn assemble_pinned_items(
    pins: &[SidebarPinRecord],
    sessions: &[SidebarSession],
    pinned_folders: &[SidebarProjectNode],
    preview: Option<&[String]>,
) -> Vec<SidebarMixedItem> {
    let mut sessions_by_row: HashMap<&str, &SidebarSession> = HashMap::new();
    for session in sessions {
        sessions_by_row.insert(session.id.as_str(), session);
    }
    let mut folders_by_id: HashMap<&str, &SidebarProjectNode> = HashMap::new();
    for folder in pinned_folders {
        folders_by_id.insert(folder.id(), folder);
    }
    let mut items: Vec<SidebarMixedItem> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for pin in pins {
        let Some(target_id) = pin.order_target_id() else {
            continue;
        };
        if seen.contains(target_id) {
            continue;
        }
        if let Some(folder) = folders_by_id.get(target_id) {
            seen.insert(target_id);
            items.push(SidebarMixedItem::Folder((*folder).clone()));
        } else if let Some(session) = sessions_by_row.get(target_id) {
            seen.insert(target_id);
            items.push(SidebarMixedItem::Session((*session).clone()));
        }
    }
    // Defensive: a rendered pinned session must never drop out of the
    // partition even if its record is somehow missing.
    for session in sessions {
        if seen.insert(session.id.as_str()) {
            items.push(SidebarMixedItem::Session(session.clone()));
        }
    }
    for folder in pinned_folders {
        if seen.insert(folder.id()) {
            items.push(SidebarMixedItem::Folder(folder.clone()));
        }
    }
    if let Some(preview) = preview {
        let mut rank: HashMap<&str, usize> = HashMap::new();
        for (index, id) in preview.iter().enumerate() {
            rank.entry(id.as_str()).or_insert(index);
        }
        // Stable sort: unranked rows keep their assembled relative order,
        // exactly the Swift `lhs.offset < rhs.offset` tie-break.
        items.sort_by_key(|item| rank.get(item.id()).copied().unwrap_or(usize::MAX));
    }
    items
}

/// Shared session-order list to advertise on bootstrap when it actually
/// interleaves a child group/worktree with sessions. Date-sorted projects
/// keep the folders-first default unless a live drag preview is in flight.
/// Port of Swift `advertisedSessionOrder`'s decision kernel (`stored` is the
/// resolved shared/local order).
pub fn advertised_session_order(
    folder_ids: &HashSet<String>,
    date_sorted: bool,
    preview: Option<&[String]>,
    stored: Option<&[String]>,
) -> Option<Vec<String>> {
    if folder_ids.is_empty() {
        return None;
    }
    if date_sorted && preview.is_none() {
        return None;
    }
    let order = preview.or(stored).unwrap_or(&[]);
    if order.iter().any(|id| folder_ids.contains(id.as_str())) {
        Some(order.to_vec())
    } else {
        None
    }
}

/// Find a node by project id, searching worktrees recursively. Port of
/// Swift `findNode`.
pub fn find_node<'a>(
    nodes: &'a [SidebarProjectNode],
    project_id: &str,
) -> Option<&'a SidebarProjectNode> {
    for node in nodes {
        if node.id() == project_id {
            return Some(node);
        }
        if let Some(found) = find_node(&node.worktrees, project_id) {
            return Some(found);
        }
    }
    None
}

/// Pre-order flattening of the project tree (used as the fallback id list
/// when publishing the shared project order). Port of Swift
/// `flattenedProjectOrderIDs`.
pub fn flattened_project_order_ids(nodes: &[SidebarProjectNode]) -> Vec<String> {
    fn append(nodes: &[SidebarProjectNode], ids: &mut Vec<String>) {
        for node in nodes {
            ids.push(node.id().to_string());
            append(&node.worktrees, ids);
        }
    }
    let mut ids = Vec::new();
    append(nodes, &mut ids);
    ids
}

/// Raw-JSON reorder of one project's `pinned_sessions` rows, for use inside
/// the app-state lock. Rows not named by the order (for example archived
/// pinned sessions, whose durable pin must survive) keep their relative
/// order below the ranked block. Each record keeps its key, `pinned_at`, and
/// unknown fields; only the array order changes. A ranked pinned group with
/// no record yet (pinned from the TUI, marker only) gains its ordering
/// record here so the dragged position is durable. Unknown shapes return
/// false and leave the object untouched. Port of Swift's static
/// `applyPinnedRecordOrder`.
pub fn apply_pinned_record_order(
    ordered_target_ids: &[String],
    project_id: &str,
    group_ids: &HashSet<String>,
    new_record_stamp: u64,
    object: &mut serde_json::Value,
) -> bool {
    if ordered_target_ids.is_empty() {
        return true;
    }
    let mut grouped: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    match object.get("pinned_sessions") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::Object(map)) => {
            for (project, rows) in map {
                let serde_json::Value::Array(rows) = rows else {
                    return false;
                };
                if !rows.iter().all(|row| row.is_object()) {
                    return false;
                }
                grouped.insert(project.clone(), rows.clone());
            }
        }
        Some(serde_json::Value::Array(rows)) => {
            // Legacy app-state.json stored one flat array.
            for row in rows {
                if !row.is_object() {
                    return false;
                }
                let Some(project) = row.get("project_id").and_then(|v| v.as_str()) else {
                    return false;
                };
                grouped
                    .entry(project.to_string())
                    .or_default()
                    .push(row.clone());
            }
        }
        Some(_) => return false,
    }

    // First occurrence wins, exactly `where rank[id] == nil`.
    let mut rank: HashMap<&str, usize> = HashMap::new();
    for (index, id) in ordered_target_ids.iter().enumerate() {
        rank.entry(id.as_str()).or_insert(index);
    }
    let mut ranked_rows: Vec<(usize, serde_json::Value)> = Vec::new();
    let mut rest: Vec<serde_json::Value> = Vec::new();
    let mut placed_targets: HashSet<String> = HashSet::new();
    for row in grouped.get(project_id).cloned().unwrap_or_default() {
        let target = pin_row_target_id(&row);
        let ranked_index = target.as_deref().and_then(|t| rank.get(t)).copied();
        match (target, ranked_index) {
            (Some(t), Some(index)) if placed_targets.insert(t.clone()) => {
                ranked_rows.push((index, row));
            }
            _ => rest.push(row),
        }
    }
    for id in ordered_target_ids {
        if group_ids.contains(id.as_str()) && !placed_targets.contains(id.as_str()) {
            let Some(index) = rank.get(id.as_str()).copied() else {
                continue;
            };
            placed_targets.insert(id.clone());
            ranked_rows.push((
                index,
                serde_json::json!({
                    "key": SidebarPinRecord::key_for_project_id(id),
                    "project_id": project_id,
                    "pinned_at": new_record_stamp,
                }),
            ));
        }
    }
    if ranked_rows.is_empty() {
        return true;
    }
    ranked_rows.sort_by_key(|(index, _)| *index);
    let mut ordered: Vec<serde_json::Value> = ranked_rows.into_iter().map(|(_, row)| row).collect();
    ordered.extend(rest);
    let mut map = serde_json::Map::new();
    // Deterministic key order for a stable serialization.
    let mut projects: Vec<String> = grouped.keys().cloned().collect();
    projects.sort();
    for project in projects {
        let rows = if project == project_id {
            ordered.clone()
        } else {
            grouped[&project].clone()
        };
        map.insert(project, serde_json::Value::Array(rows));
    }
    if !grouped.contains_key(project_id) {
        map.insert(project_id.to_string(), serde_json::Value::Array(ordered));
    }
    object["pinned_sessions"] = serde_json::Value::Object(map);
    true
}

/// The sidebar row a pin row ranks: the `session_id`, or the pinned child
/// group's project id decoded from a `"project:"` / `"session:"` key.
/// Port of Swift `applyPinnedRecordOrder`'s `rowTargetID`.
fn pin_row_target_id(row: &serde_json::Value) -> Option<String> {
    if let Some(session_id) = row.get("session_id").and_then(|v| v.as_str()) {
        return Some(session_id.to_string());
    }
    let key = row.get("key")?.as_str()?;
    if let Some(id) = key.strip_prefix("project:") {
        return Some(id.to_string());
    }
    if let Some(id) = key.strip_prefix("session:") {
        return Some(id.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn session(id: &str, project: &str, created_at: i64, status: SessionStatus) -> SidebarSession {
        SidebarSession {
            id: id.to_string(),
            project_id: project.to_string(),
            project_override_id: None,
            created_at,
            lifecycle_at_ms: None,
            status,
        }
    }

    fn project(id: &str, parent: Option<&str>, sort_order: Option<i64>) -> SidebarProject {
        SidebarProject {
            id: id.to_string(),
            parent_project_id: parent.map(str::to_string),
            sort_order,
            is_folder: false,
            worktree_branch: None,
            pinned_at: None,
        }
    }

    fn folder(id: &str, parent: &str, pinned_at: Option<u64>) -> SidebarProject {
        SidebarProject {
            id: id.to_string(),
            parent_project_id: Some(parent.to_string()),
            sort_order: None,
            is_folder: true,
            worktree_branch: None,
            pinned_at,
        }
    }

    fn pin(
        key: &str,
        project_id: &str,
        session_id: Option<&str>,
        pinned_at: u64,
    ) -> SidebarPinRecord {
        SidebarPinRecord {
            key: key.to_string(),
            project_id: project_id.to_string(),
            session_id: session_id.map(str::to_string),
            pinned_at,
        }
    }

    fn ids(items: &[SidebarMixedItem]) -> Vec<String> {
        items.iter().map(|item| item.id().to_string()).collect()
    }

    fn str_ids(items: &[SidebarSession]) -> Vec<String> {
        items.iter().map(|s| s.id.clone()).collect()
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    fn empty_scan<'a>(
        removing: &'a HashSet<String>,
        placeholders: &'a [SidebarSession],
        restarting: &'a HashSet<String>,
        shared_projects: &'a HashMap<Option<String>, Vec<String>>,
        shared_sessions: &'a HashMap<String, Vec<String>>,
    ) -> TreeScan<'a> {
        TreeScan {
            removing_ids: removing,
            restart_placeholders: placeholders,
            restarting_ids: restarting,
            shared_project_orders: shared_projects,
            shared_session_orders: shared_sessions,
        }
    }

    // --- overlay precedence -------------------------------------------------

    #[test]
    fn overlay_precedence_preview_beats_shared_beats_local() {
        let preview = strings(&["p"]);
        let shared = strings(&["s"]);
        let local = strings(&["l"]);
        assert_eq!(
            resolve_overlay(Some(&preview), Some(&shared), Some(&local)),
            Some(&preview[..])
        );
        assert_eq!(
            resolve_overlay(None, Some(&shared), Some(&local)),
            Some(&shared[..])
        );
        assert_eq!(resolve_overlay(None, None, Some(&local)), Some(&local[..]));
        assert_eq!(resolve_overlay(None, None, None), None);
        // An empty shared order falls through to the local overlay (older
        // files contain top-level ids only).
        let empty: Vec<String> = vec![];
        assert_eq!(
            resolve_overlay(None, Some(&empty), Some(&local)),
            Some(&local[..])
        );
    }

    #[test]
    fn effective_overlay_rejects_empty_winners() {
        let empty: Vec<String> = vec![];
        let local = strings(&["l"]);
        // An empty preview still outranks shared/local at resolve time, so
        // the effective overlay is None and the caller keeps the base order.
        assert_eq!(effective_overlay(Some(&empty), None, Some(&local)), None);
        assert_eq!(
            effective_overlay(None, None, Some(&local)),
            Some(&local[..])
        );
    }

    #[test]
    fn overlay_key_formats() {
        assert_eq!(project_order_key(None), "supercli.native.projectOrder");
        assert_eq!(
            project_order_key(Some("g1")),
            "supercli.native.projectOrder.g1"
        );
        assert_eq!(session_order_key("p1"), "supercli.native.sessionOrder.p1");
        assert_eq!(pinned_order_key("p1"), "supercli.native.pinnedOrder.p1");
    }

    // --- project order overlay ----------------------------------------------

    #[test]
    fn project_order_overlay_ranks_first_and_appends_new() {
        let base = strings(&["a", "b", "c"]);
        let overlay = strings(&["c", "a", "ghost"]);
        // Unknown overlay ids are skipped, not GC'd at read; new projects
        // append in base order.
        assert_eq!(
            apply_project_order(&base, &overlay),
            strings(&["c", "a", "b"])
        );
    }

    #[test]
    fn project_order_empty_or_disjoint_overlay_keeps_base() {
        let base = strings(&["a", "b"]);
        assert_eq!(apply_project_order(&base, &[]), base);
        assert_eq!(apply_project_order(&base, &strings(&["ghost"])), base);
        assert_eq!(
            apply_project_order(&[], &strings(&["a"])),
            Vec::<String>::new()
        );
    }

    // --- session order overlay ----------------------------------------------

    #[test]
    fn session_order_new_sessions_stay_above_hand_ordered_block() {
        // Base is newest-first: s3 launched after the drag snapshotted.
        let base = strings(&["s3", "s1", "s2"]);
        let overlay = strings(&["s2", "s1"]);
        assert_eq!(
            apply_session_order(&base, &overlay),
            strings(&["s3", "s2", "s1"])
        );
    }

    // --- pinned records ------------------------------------------------------

    #[test]
    fn pin_record_order_target_id() {
        let session_pin = pin("session:s1", "p", Some("s1"), 10);
        assert_eq!(session_pin.order_target_id(), Some("s1"));
        let group_pin = pin("project:g1", "p", None, 11);
        assert_eq!(group_pin.pinned_project_id(), Some("g1"));
        assert_eq!(group_pin.order_target_id(), Some("g1"));
        assert_eq!(SidebarPinRecord::key_for_session_id("s1"), "session:s1");
        assert_eq!(SidebarPinRecord::key_for_project_id("g1"), "project:g1");
        // Malformed records have no target.
        let bad = pin("junk", "p", None, 1);
        assert_eq!(bad.order_target_id(), None);
        assert_eq!(bad.pinned_project_id(), None);
    }

    #[test]
    fn ordered_pinned_records_shared_wins_then_local() {
        let base = vec![
            pin("session:s1", "p", Some("s1"), 1),
            pin("session:s2", "p", Some("s2"), 2),
            pin("session:s3", "p", Some("s3"), 3),
        ];
        let shared = strings(&["s3", "s1"]);
        let local = strings(&["s2", "s3"]);
        // Shared order wins when it carries pin ranks.
        assert_eq!(
            ordered_pinned_records(&base, Some(&shared), Some(&local))
                .iter()
                .map(|p| p.session_id.clone().unwrap())
                .collect::<Vec<_>>(),
            vec!["s3", "s1", "s2"]
        );
        // Legacy local overlay is the fallback; unranked records keep
        // relative order below the ranked block.
        assert_eq!(
            ordered_pinned_records(&base, None, Some(&local))
                .iter()
                .map(|p| p.session_id.clone().unwrap())
                .collect::<Vec<_>>(),
            vec!["s2", "s3", "s1"]
        );
        // No order at all: base order untouched.
        assert_eq!(ordered_pinned_records(&base, None, None), base);
    }

    // --- sibling math --------------------------------------------------------

    #[test]
    fn project_insertion_order_gap_semantics() {
        let ids = strings(&["a", "b", "c"]);
        assert_eq!(
            project_insertion_order(&ids, "a", "c", false),
            Some(strings(&["b", "a", "c"]))
        );
        assert_eq!(
            project_insertion_order(&ids, "a", "c", true),
            Some(strings(&["b", "c", "a"]))
        );
        // Dragging below the last row appends.
        assert_eq!(
            project_insertion_order(&ids, "a", "c", true),
            Some(strings(&["b", "c", "a"]))
        );
        // Degenerate inputs are no-ops.
        assert_eq!(project_insertion_order(&ids, "a", "a", true), None);
        assert_eq!(project_insertion_order(&ids, "ghost", "c", false), None);
        assert_eq!(project_insertion_order(&ids, "a", "ghost", false), None);
    }

    #[test]
    fn sibling_move_takes_target_slot() {
        let ids = strings(&["a", "b", "c"]);
        // Swift `projectSiblingMove`: remove at `from`, insert at the
        // target's ORIGINAL index.
        assert_eq!(
            sibling_move_to_target_slot(&ids, "a", "c"),
            Some(strings(&["b", "c", "a"]))
        );
        assert_eq!(
            sibling_move_to_target_slot(&ids, "c", "a"),
            Some(strings(&["c", "a", "b"]))
        );
        assert_eq!(sibling_move_to_target_slot(&ids, "b", "b"), None);
        assert_eq!(sibling_move_to_target_slot(&ids, "ghost", "a"), None);
        assert_eq!(sibling_move_to_target_slot(&ids, "a", "ghost"), None);
    }

    #[test]
    fn combined_session_order_dedups_pins_first() {
        assert_eq!(
            combined_session_order(&strings(&["p1", "s1"]), &strings(&["s1", "s2"])),
            strings(&["p1", "s1", "s2"])
        );
        assert_eq!(
            combined_session_order(&[], &strings(&["s1"])),
            strings(&["s1"])
        );
    }

    #[test]
    fn applying_relative_id_order_moves_around_foreign_ids() {
        // A held session order moves around child-folder ids without moving
        // the folders themselves: the preferred ids take the replaced slots
        // in preferred order; every other row stays in place.
        let base = strings(&["s1", "folderA", "s2", "s3"]);
        let preferred = strings(&["s3", "s1"]);
        assert_eq!(
            applying_relative_id_order(&preferred, &base),
            strings(&["s3", "folderA", "s2", "s1"])
        );
        // Preferred ids absent from the base are ignored.
        assert_eq!(
            applying_relative_id_order(&strings(&["ghost"]), &base),
            base
        );
    }

    #[test]
    fn replacing_session_id_swaps_in_place() {
        let order = strings(&["a", "b", "c"]);
        assert_eq!(
            replacing_session_id(Some(&order), "b", "b2"),
            Some(strings(&["a", "b2", "c"]))
        );
        assert_eq!(replacing_session_id(Some(&order), "ghost", "b2"), None);
        assert_eq!(replacing_session_id(None, "b", "b2"), None);
    }

    #[test]
    fn insert_ids_below_host() {
        let list = strings(&["a", "b", "c"]);
        assert_eq!(
            insert_ids_below(&strings(&["c"]), "a", &list),
            strings(&["a", "c", "b"])
        );
        // Existing occurrences are removed first, not duplicated.
        assert_eq!(
            insert_ids_below(&strings(&["b", "c"]), "a", &list),
            strings(&["a", "b", "c"])
        );
        // Missing host: append.
        assert_eq!(
            insert_ids_below(&strings(&["x"]), "ghost", &list),
            strings(&["a", "b", "c", "x"])
        );
        // Nothing to move: list untouched.
        assert_eq!(insert_ids_below(&strings(&["a"]), "a", &list), list);
    }

    // --- session sorts -------------------------------------------------------

    #[test]
    fn order_sessions_manual_unranked_newest_first_above() {
        let sessions = vec![
            session("s1", "p", 100, SessionStatus::Exited),
            session("s2", "p", 300, SessionStatus::Exited),
            session("s3", "p", 200, SessionStatus::Exited),
        ];
        // Unranked sessions sort newest-first ABOVE the ranked block.
        let ordered = order_sessions_manual(&sessions, &strings(&["s1"]));
        assert_eq!(str_ids(&ordered), vec!["s2", "s3", "s1"]);
        // Empty manual order: plain newest-first.
        let plain = order_sessions_manual(&sessions, &[]);
        assert_eq!(str_ids(&plain), vec!["s2", "s3", "s1"]);
    }

    #[test]
    fn recent_activity_sort_working_first_then_stamp_then_id() {
        let mut idle = session("idle", "p", 100, SessionStatus::Idle);
        idle.lifecycle_at_ms = Some(900);
        let busy = session("busy", "p", 50, SessionStatus::Busy);
        let mut exited = session("exited", "p", 80, SessionStatus::Exited);
        exited.lifecycle_at_ms = Some(950);
        let restarting = session("restart", "p", 10, SessionStatus::Exited);
        let restarting_ids: HashSet<String> = ["restart".to_string()].into_iter().collect();
        let sorted = sessions_sorted_by_recent_activity(
            &[idle, busy.clone(), exited, restarting.clone()],
            &restarting_ids,
        );
        // Work tier first (busy + restarting), then newest stamp, id breaks
        // the final tie.
        assert_eq!(str_ids(&sorted), vec!["busy", "restart", "exited", "idle"]);
        // A live-but-idle session is NOT privileged over a more recent
        // exited one.
        let sorted2 = sessions_sorted_by_recent_activity(
            &[
                session("a", "p", 1, SessionStatus::Idle),
                session("b", "p", 2, SessionStatus::Exited),
            ],
            &HashSet::new(),
        );
        assert_eq!(str_ids(&sorted2), vec!["b", "a"]);
        let _ = busy;
        let _ = restarting;
    }

    #[test]
    fn effective_project_id_override_with_stale_fallback() {
        let known: HashSet<String> = ["p1".to_string(), "g1".to_string()].into_iter().collect();
        let mut s = session("s", "p1", 1, SessionStatus::Idle);
        assert_eq!(effective_project_id(&s, &known), "p1");
        s.project_override_id = Some("g1".to_string());
        assert_eq!(effective_project_id(&s, &known), "g1");
        // Stale marker falls back to the manifest project instead of
        // orphaning the row.
        s.project_override_id = Some("deleted".to_string());
        assert_eq!(effective_project_id(&s, &known), "p1");
    }

    // --- tree build ----------------------------------------------------------

    #[test]
    fn build_tree_groups_files_and_orders() {
        let projects = vec![
            project("p1", None, Some(2)),
            project("p2", None, Some(1)),
            folder("g1", "p1", None),
        ];
        let mut s1 = session("s1", "p1", 100, SessionStatus::Idle);
        s1.project_override_id = Some("g1".to_string());
        let sessions = vec![
            session("s2", "p1", 300, SessionStatus::Idle),
            session("s3", "p1", 200, SessionStatus::Idle),
            session("s4", "p2", 400, SessionStatus::Idle),
            s1,
        ];
        let removing = HashSet::new();
        let placeholders: Vec<SidebarSession> = vec![];
        let restarting = HashSet::new();
        let shared_projects: HashMap<Option<String>, Vec<String>> = HashMap::new();
        let shared_sessions: HashMap<String, Vec<String>> = HashMap::new();
        let scan = empty_scan(
            &removing,
            &placeholders,
            &restarting,
            &shared_projects,
            &shared_sessions,
        );
        let state = SidebarOrderState::default();
        let nodes = state.build_tree(&projects, &sessions, &scan);
        // Top level sorts by sort_order (p2 before p1).
        assert_eq!(
            nodes.iter().map(|n| n.id()).collect::<Vec<_>>(),
            vec!["p2", "p1"]
        );
        let p1 = find_node(&nodes, "p1").unwrap();
        // Sessions newest-first; the override files s1 under g1 (a folder
        // node, not the p1 session list).
        assert_eq!(str_ids(&p1.sessions), vec!["s2", "s3"]);
        assert!(p1.worktrees.iter().any(|w| w.id() == "g1"));
        let g1 = find_node(&nodes, "g1").unwrap();
        assert_eq!(str_ids(&g1.sessions), vec!["s1"]);
        assert!(find_node(&nodes, "p2").unwrap().has_any_content());
    }

    #[test]
    fn build_tree_honors_project_overlay_and_filters_removing() {
        let projects = vec![project("p1", None, None), project("p2", None, None)];
        let sessions = vec![
            session("dying", "p1", 100, SessionStatus::Idle),
            session("s1", "p1", 200, SessionStatus::Idle),
        ];
        let removing: HashSet<String> = ["dying".to_string()].into_iter().collect();
        let placeholders: Vec<SidebarSession> = vec![];
        let restarting = HashSet::new();
        let mut shared_projects: HashMap<Option<String>, Vec<String>> = HashMap::new();
        shared_projects.insert(None, strings(&["p2", "p1"]));
        let shared_sessions: HashMap<String, Vec<String>> = HashMap::new();
        let scan = empty_scan(
            &removing,
            &placeholders,
            &restarting,
            &shared_projects,
            &shared_sessions,
        );
        let state = SidebarOrderState::default();
        let nodes = state.build_tree(&projects, &sessions, &scan);
        assert_eq!(
            nodes.iter().map(|n| n.id()).collect::<Vec<_>>(),
            vec!["p2", "p1"]
        );
        // Removal in flight vanishes from the sidebar immediately.
        let p1 = find_node(&nodes, "p1").unwrap();
        assert_eq!(str_ids(&p1.sessions), vec!["s1"]);
    }

    #[test]
    fn build_tree_injects_restart_placeholders_and_date_sorts() {
        let projects = vec![project("p1", None, None)];
        let sessions = vec![
            session("old", "p1", 500, SessionStatus::Exited),
            session("live", "p1", 100, SessionStatus::Busy),
        ];
        let removing = HashSet::new();
        let placeholders = vec![session("respawn", "p1", 50, SessionStatus::Starting)];
        let restarting = HashSet::new();
        let shared_projects: HashMap<Option<String>, Vec<String>> = HashMap::new();
        let shared_sessions: HashMap<String, Vec<String>> = HashMap::new();
        let scan = empty_scan(
            &removing,
            &placeholders,
            &restarting,
            &shared_projects,
            &shared_sessions,
        );
        let mut state = SidebarOrderState::default();
        state.set_date_sorted("p1", true);
        let nodes = state.build_tree(&projects, &sessions, &scan);
        let p1 = find_node(&nodes, "p1").unwrap();
        // Date sort: working rows first, then newest stamp; the restart
        // placeholder keeps its row through the respawn gap.
        assert_eq!(str_ids(&p1.sessions), vec!["live", "respawn", "old"]);
        assert!(state.is_date_sorted("p1"));
        state.set_date_sorted("p1", false);
        assert!(!state.is_date_sorted("p1"));
    }

    #[test]
    fn flattened_project_order_ids_is_preorder() {
        let projects = vec![
            project("p1", None, None),
            folder("g1", "p1", None),
            project("p2", None, None),
        ];
        let removing = HashSet::new();
        let placeholders: Vec<SidebarSession> = vec![];
        let restarting = HashSet::new();
        let shared_projects: HashMap<Option<String>, Vec<String>> = HashMap::new();
        let shared_sessions: HashMap<String, Vec<String>> = HashMap::new();
        let scan = empty_scan(
            &removing,
            &placeholders,
            &restarting,
            &shared_projects,
            &shared_sessions,
        );
        let nodes = SidebarOrderState::default().build_tree(&projects, &[], &scan);
        assert_eq!(
            flattened_project_order_ids(&nodes),
            strings(&["p1", "g1", "p2"])
        );
        assert_eq!(find_node(&nodes, "nope"), None);
    }

    // --- drag previews -------------------------------------------------------

    #[test]
    fn project_preview_commit_cancel_cycle() {
        let mut state = SidebarOrderState::default();
        let displayed = strings(&["a", "b", "c"]);
        // No-op drags store nothing.
        assert!(!state.preview_project_move(None, &displayed, "a", "a", true));
        assert!(!state.preview_project_move(None, &displayed, "ghost", "c", true));
        // Gap insertion stores the preview and outranks the base order.
        assert!(state.preview_project_move(None, &displayed, "a", "c", true));
        let projects: Vec<SidebarProject> =
            displayed.iter().map(|id| project(id, None, None)).collect();
        let ordered = state.apply_project_order(&projects, None, None);
        assert_eq!(
            ordered.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
            strings(&["b", "c", "a"])
        );
        // Repeating the same drag is a no-op.
        assert!(!state.preview_project_move(None, &displayed, "a", "c", true));
        // Commit captures the visible order and drops the preview.
        let (parent, ids) = state.commit_project_preview().unwrap();
        assert_eq!(parent, None);
        assert_eq!(ids, strings(&["b", "c", "a"]));
        assert_eq!(state.commit_project_preview(), None);
        // Cancel after commit is a no-op; a fresh preview cancels cleanly.
        assert!(!state.cancel_project_preview());
        assert!(state.preview_project_move(None, &displayed, "b", "a", false));
        assert!(state.cancel_project_preview());
        assert!(!state.cancel_project_preview());
    }

    #[test]
    fn session_preview_commit_flips_date_sort_for_regular_section() {
        let mut state = SidebarOrderState::default();
        state.set_date_sorted("p1", true);
        let pinned = strings(&["pin1"]);
        let regular = strings(&["s1", "s2", "s3"]);
        assert!(state.preview_session_move("p1", &pinned, &regular, "s1", "s3", false));
        // Same drag again: no change.
        assert!(!state.preview_session_move("p1", &pinned, &regular, "s1", "s3", false));
        // Commit the regular section: preview drops, date sort flips off
        // (hand-ordering a date-sorted list chooses custom order).
        let commit = state
            .commit_session_preview("p1", false, pinned.clone(), strings(&["s2", "s3", "s1"]))
            .unwrap();
        assert!(!commit.pinned_partition);
        assert!(commit.flipped_date_sort);
        assert!(!state.is_date_sorted("p1"));
        assert_eq!(
            state.commit_session_preview("p1", false, vec![], vec![]),
            None
        );
    }

    #[test]
    fn session_preview_pinned_partition_never_flips_date_sort() {
        let mut state = SidebarOrderState::default();
        state.set_date_sorted("p1", true);
        let pinned = strings(&["pin1", "pin2"]);
        assert!(state.preview_session_move("p1", &pinned, &[], "pin1", "pin2", true));
        let commit = state
            .commit_session_preview("p1", true, strings(&["pin2", "pin1"]), vec![])
            .unwrap();
        assert!(commit.pinned_partition);
        assert!(!commit.flipped_date_sort);
        assert!(state.is_date_sorted("p1"));
        // Cancel without a preview is a no-op.
        assert!(!state.cancel_session_preview("p1"));
        assert!(state.preview_session_move("p1", &pinned, &[], "pin1", "pin2", true));
        assert!(state.cancel_session_preview("p1"));
    }

    #[test]
    fn set_orders_write_local_overlay_and_return_combined() {
        let mut state = SidebarOrderState::default();
        // set_session_order stores the regular ids locally and returns the
        // combined shared rank (pinned ranks preserved).
        let combined = state.set_session_order("p1", &strings(&["pin1"]), strings(&["s2", "s1"]));
        assert_eq!(combined, strings(&["pin1", "s2", "s1"]));
        // set_pinned_order stores the mixed pinned order locally.
        let combined = state.set_pinned_order("p1", strings(&["pin2", "pin1"]), &strings(&["s1"]));
        assert_eq!(combined, strings(&["pin2", "pin1", "s1"]));
        // Empty orders remove the local overlay.
        state.set_session_order("p1", &[], vec![]);
        state.set_pinned_order("p1", vec![], &[]);
        let base = vec![session("s1", "p1", 1, SessionStatus::Idle)];
        assert_eq!(
            str_ids(&state.apply_session_order(&base, "p1", None, &HashSet::new())),
            vec!["s1"]
        );
        // set_project_order returns the overlay key for the sibling set.
        let key = state.set_project_order(Some("g"), strings(&["b", "a"]));
        assert_eq!(key, "supercli.native.projectOrder.g");
        let projects = vec![project("a", Some("g"), None), project("b", Some("g"), None)];
        let ordered = state.apply_project_order(&projects, Some("g"), None);
        assert_eq!(
            ordered.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
            strings(&["b", "a"])
        );
    }

    #[test]
    fn prune_project_overlays_drops_removed_ids_and_parents() {
        let mut state = SidebarOrderState::default();
        state.set_project_order(None, strings(&["a", "b", "c"]));
        state.set_project_order(Some("gone"), strings(&["x"]));
        state.set_project_order(Some("keep"), strings(&["b", "y"]));
        let removed: HashSet<String> = ["b".to_string(), "gone".to_string()].into_iter().collect();
        state.prune_project_overlays(&removed);
        // "b" pruned from the top-level overlay; the removed parent's whole
        // overlay dropped; sibling overlay pruned.
        let projects = vec![
            project("a", None, None),
            project("b", None, None),
            project("c", None, None),
        ];
        let ordered = state.apply_project_order(&projects, None, None);
        assert_eq!(
            ordered.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
            strings(&["a", "c", "b"])
        );
        let kids = vec![
            project("b", Some("keep"), None),
            project("y", Some("keep"), None),
        ];
        let ordered_kids = state.apply_project_order(&kids, Some("keep"), None);
        assert_eq!(
            ordered_kids
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<_>>(),
            strings(&["y", "b"])
        );
        // Empty prune set is a no-op.
        state.prune_project_overlays(&HashSet::new());
    }

    #[test]
    fn toggle_expanded_flips() {
        let mut state = SidebarOrderState::default();
        assert!(!state.is_expanded("p1"));
        assert!(state.toggle_expanded("p1"));
        assert!(state.is_expanded("p1"));
        assert!(!state.toggle_expanded("p1"));
        assert!(!state.is_expanded("p1"));
    }

    // --- pinned items assembly -----------------------------------------------

    fn pinned_node(id: &str) -> SidebarProjectNode {
        SidebarProjectNode {
            project: folder(id, "p1", Some(9)),
            sessions: vec![],
            worktrees: vec![],
        }
    }

    #[test]
    fn assemble_pinned_items_mixed_order_with_preview_rerank() {
        let pins = vec![
            pin("project:g1", "p1", None, 5),
            pin("session:s1", "p1", Some("s1"), 6),
        ];
        let sessions = vec![
            session("s1", "p1", 100, SessionStatus::Idle),
            session("s2", "p1", 200, SessionStatus::Idle),
        ];
        let folders = vec![pinned_node("g1")];
        // Record order interleaves the folder and the session; the
        // recordless pinned session s2 is defensively kept at the end.
        let items = assemble_pinned_items(&pins, &sessions, &folders, None);
        assert_eq!(ids(&items), strings(&["g1", "s1", "s2"]));
        // A preview re-ranks the assembled rows (recordless rows ride it).
        let preview = strings(&["s2", "g1", "s1"]);
        let items = assemble_pinned_items(&pins, &sessions, &folders, Some(&preview));
        assert_eq!(ids(&items), strings(&["s2", "g1", "s1"]));
        // Unknown record targets are skipped without dropping the row set.
        let ghost_pins = vec![pin("session:ghost", "p1", Some("ghost"), 1)];
        let items = assemble_pinned_items(&ghost_pins, &sessions, &folders, None);
        assert_eq!(ids(&items), strings(&["s1", "s2", "g1"]));
    }

    // --- advertised order -----------------------------------------------------

    #[test]
    fn advertised_session_order_only_when_interleaving_folders() {
        let folders: HashSet<String> = ["g1".to_string()].into_iter().collect();
        let stored = strings(&["s1", "g1", "s2"]);
        assert_eq!(
            advertised_session_order(&folders, false, None, Some(&stored)),
            Some(stored.clone())
        );
        // No folder in the order: nothing to advertise.
        assert_eq!(
            advertised_session_order(&folders, false, None, Some(&strings(&["s1", "s2"]))),
            None
        );
        // Date-sorted lists keep folders-first unless a preview is in flight.
        assert_eq!(
            advertised_session_order(&folders, true, None, Some(&stored)),
            None
        );
        assert_eq!(
            advertised_session_order(&folders, true, Some(&stored), None),
            Some(stored)
        );
        // No folders at all: never advertise.
        assert_eq!(
            advertised_session_order(&HashSet::new(), false, None, Some(&strings(&["g1"]))),
            None
        );
    }

    // --- raw-JSON pinned record reorder ---------------------------------------

    fn record_object() -> serde_json::Value {
        serde_json::json!({
            "pinned_sessions": {
                "p1": [
                    {"key": "session:s1", "project_id": "p1", "session_id": "s1", "pinned_at": 1, "note": "keep"},
                    {"key": "session:s2", "project_id": "p1", "session_id": "s2", "pinned_at": 2},
                    {"key": "session:s9", "project_id": "p1", "session_id": "s9", "pinned_at": 3}
                ],
                "p2": [
                    {"key": "session:x", "project_id": "p2", "session_id": "x", "pinned_at": 4}
                ]
            }
        })
    }

    #[test]
    fn apply_pinned_record_order_reorders_and_keeps_rest() {
        let mut object = record_object();
        let groups: HashSet<String> = HashSet::new();
        assert!(apply_pinned_record_order(
            &strings(&["s2", "s1"]),
            "p1",
            &groups,
            99,
            &mut object
        ));
        let rows = object["pinned_sessions"]["p1"].as_array().unwrap();
        let keys: Vec<&str> = rows.iter().map(|r| r["key"].as_str().unwrap()).collect();
        // Ranked rows first in rank order; unranked rows keep relative order
        // below; unknown fields ("note") survive.
        assert_eq!(keys, vec!["session:s2", "session:s1", "session:s9"]);
        assert_eq!(rows[1]["note"], serde_json::json!("keep"));
        assert_eq!(rows[0]["pinned_at"], serde_json::json!(2));
        // Other projects untouched.
        assert_eq!(
            object["pinned_sessions"]["p2"][0]["key"],
            serde_json::json!("session:x")
        );
    }

    #[test]
    fn apply_pinned_record_order_synthesizes_group_records() {
        let mut object = record_object();
        let groups: HashSet<String> = ["g1".to_string()].into_iter().collect();
        assert!(apply_pinned_record_order(
            &strings(&["g1", "s1"]),
            "p1",
            &groups,
            42,
            &mut object
        ));
        let rows = object["pinned_sessions"]["p1"].as_array().unwrap();
        let keys: Vec<&str> = rows.iter().map(|r| r["key"].as_str().unwrap()).collect();
        // The TUI-pinned group gains its ordering record with the new stamp.
        assert_eq!(keys[0], "project:g1");
        assert_eq!(rows[0]["pinned_at"], serde_json::json!(42));
        assert_eq!(rows[0]["project_id"], serde_json::json!("p1"));
        assert!(rows[0].get("session_id").is_none());
    }

    #[test]
    fn apply_pinned_record_order_legacy_flat_and_unknown_shapes() {
        // Legacy flat array groups rows by project_id.
        let mut flat = serde_json::json!({
            "pinned_sessions": [
                {"key": "session:b", "project_id": "p1", "session_id": "b", "pinned_at": 2},
                {"key": "session:a", "project_id": "p1", "session_id": "a", "pinned_at": 1}
            ]
        });
        let groups: HashSet<String> = HashSet::new();
        assert!(apply_pinned_record_order(
            &strings(&["a", "b"]),
            "p1",
            &groups,
            0,
            &mut flat
        ));
        let rows = flat["pinned_sessions"]["p1"].as_array().unwrap();
        assert_eq!(rows[0]["key"], serde_json::json!("session:a"));
        // Unknown shapes return false and leave the object untouched.
        let mut junk = serde_json::json!({"pinned_sessions": {"p1": [42]}});
        let before = junk.clone();
        assert!(!apply_pinned_record_order(
            &strings(&["a"]),
            "p1",
            &groups,
            0,
            &mut junk
        ));
        assert_eq!(junk, before);
        let mut junk2 = serde_json::json!({"pinned_sessions": [{}]});
        let before2 = junk2.clone();
        assert!(!apply_pinned_record_order(
            &strings(&["a"]),
            "p1",
            &groups,
            0,
            &mut junk2
        ));
        assert_eq!(junk2, before2);
        // Empty order is a no-op success.
        let mut untouched = record_object();
        let before3 = untouched.clone();
        assert!(apply_pinned_record_order(
            &[],
            "p1",
            &groups,
            0,
            &mut untouched
        ));
        assert_eq!(untouched, before3);
        // Null / missing pinned_sessions: true, object gains nothing to rank.
        let mut missing = serde_json::json!({});
        assert!(apply_pinned_record_order(
            &strings(&["a"]),
            "p1",
            &groups,
            0,
            &mut missing
        ));
    }

    // --- pin group rebuild -----------------------------------------------------

    #[test]
    fn rebuild_pin_groups_validity_ordering_and_overlay() {
        // Sessions: s1 lives in p1, s2 filed under g1 via override.
        let mut s2 = session("s2", "p1", 200, SessionStatus::Idle);
        s2.project_override_id = Some("g1".to_string());
        let sessions: HashMap<String, SidebarSession> =
            [session("s1", "p1", 100, SessionStatus::Idle), s2]
                .into_iter()
                .map(|s| (s.id.clone(), s))
                .collect();
        let projects: HashMap<String, SidebarProject> = [
            project("p1", None, None),
            folder("g1", "p1", Some(7)),
            folder("g2", "p1", None), // no pinned_at marker: record invalid
        ]
        .into_iter()
        .map(|p| (p.id.clone(), p))
        .collect();
        let known: HashSet<String> = projects.keys().cloned().collect();
        let file_pins: HashMap<String, Vec<SidebarPinRecord>> = [(
            "p1".to_string(),
            vec![
                pin("session:s1", "p1", Some("s1"), 10),
                // Stale: s2's effective project is g1, not p1.
                pin("session:s2", "p1", Some("s2"), 11),
                // Gone session.
                pin("session:ghost", "p1", Some("ghost"), 12),
                pin("project:g1", "p1", None, 13),
                // g2 lost its cross-frontend marker.
                pin("project:g2", "p1", None, 14),
            ],
        )]
        .into_iter()
        .collect();
        let merged: Vec<SidebarPinRecord> = file_pins["p1"]
            .iter()
            .cloned()
            .chain(std::iter::once(pin("session:s3", "p1", Some("s3"), 5)))
            .collect();
        // s3 is a merged-only record whose session row is gone: invalid.
        let state = SidebarOrderState::default();
        let shared: HashMap<String, Vec<String>> = HashMap::new();
        let grouped =
            state.rebuild_pin_groups(&file_pins, &merged, &sessions, &projects, &known, &shared);
        let keys: Vec<&str> = grouped["p1"].iter().map(|p| p.key.as_str()).collect();
        // File-array order is the base; invalid records (stale project, gone
        // session, markerless group, missing session) are dropped.
        assert_eq!(keys, vec!["session:s1", "project:g1"]);
        // The pinned-order overlay then applies. The overlay carries raw
        // target ids (session ids and group project ids), exactly the mixed
        // row ids `renderedPinnedItems` produces.
        let mut state2 = SidebarOrderState::default();
        state2.set_pinned_order("p1", strings(&["g1", "s1"]), &[]);
        let grouped2 =
            state2.rebuild_pin_groups(&file_pins, &merged, &sessions, &projects, &known, &shared);
        let keys2: Vec<&str> = grouped2["p1"].iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys2, vec!["project:g1", "session:s1"]);
    }

    #[test]
    fn rebuild_pin_groups_unplaced_append_oldest_first() {
        let sessions: HashMap<String, SidebarSession> =
            [session("s1", "p1", 100, SessionStatus::Idle)]
                .into_iter()
                .map(|s| (s.id.clone(), s))
                .collect();
        let projects: HashMap<String, SidebarProject> = [(project("p1", None, None))]
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect();
        let known: HashSet<String> = projects.keys().cloned().collect();
        let file_pins: HashMap<String, Vec<SidebarPinRecord>> = [(
            "p1".to_string(),
            vec![pin("session:s1", "p1", Some("s1"), 10)],
        )]
        .into_iter()
        .collect();
        // Merged-only pins (native overlay) append below, oldest first.
        let merged = vec![
            pin("session:s1", "p1", Some("s1"), 10),
            pin("session:s9", "p1", Some("s9"), 3),
        ];
        let mut sessions2 = sessions.clone();
        sessions2.insert(
            "s9".to_string(),
            session("s9", "p1", 50, SessionStatus::Idle),
        );
        let state = SidebarOrderState::default();
        let shared: HashMap<String, Vec<String>> = HashMap::new();
        let grouped =
            state.rebuild_pin_groups(&file_pins, &merged, &sessions2, &projects, &known, &shared);
        let keys: Vec<&str> = grouped["p1"].iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, vec!["session:s1", "session:s9"]);
    }
}
