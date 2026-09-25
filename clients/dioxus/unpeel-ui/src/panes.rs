//! The recursive split-tree pane model, ported from
//! `clients/native/UnpeelNative/Sources/UnpeelNative/PaneLayoutState.swift`
//! and `PaneLayoutController.swift`.
//!
//! The normative cross-implementation contract — operation semantics, durable
//! schema, v1 migration, and the spatial/equalize algorithms — is
//! `protocol/pane-layout-operations-v1.json`; this module implements that
//! contract, and `fixture_tests::fixture_replay_all_cases` replays all 44
//! fixture cases against the real codec and layout operations.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MINIMUM_SPLIT_RATIO: f64 = 0.1;
pub const MAXIMUM_SPLIT_RATIO: f64 = 0.9;
/// The session-leaf cap; canonicalize trims trailing preorder leaves.
pub const SESSION_LEAF_CAP: usize = 8;
pub const DURABLE_VERSION: u32 = 2;

pub fn clamp_ratio(ratio: f64) -> f64 {
    if !ratio.is_finite() {
        return 0.5;
    }
    ratio.clamp(MINIMUM_SPLIT_RATIO, MAXIMUM_SPLIT_RATIO)
}

/// What a pane shows. Mirrors `PaneContent`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PaneContent {
    Session { id: String },
    Launcher { project_id: String },
}

impl PaneContent {
    pub fn session_id(&self) -> Option<&str> {
        match self {
            PaneContent::Session { id } => Some(id),
            PaneContent::Launcher { .. } => None,
        }
    }

    pub fn is_launcher(&self) -> bool {
        matches!(self, PaneContent::Launcher { .. })
    }
}

/// Mirrors `Pane`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pane {
    pub id: String,
    pub content: PaneContent,
}

/// Mirrors `SplitDirection`. A horizontal split lays left|right; a vertical
/// split lays top/bottom with the LEFT child on top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// A drop/split edge. Mirrors `PaneEdge`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneEdge {
    Left,
    Right,
    Up,
    Down,
}

impl PaneEdge {
    pub fn split_direction(&self) -> SplitDirection {
        match self {
            PaneEdge::Left | PaneEdge::Right => SplitDirection::Horizontal,
            PaneEdge::Up | PaneEdge::Down => SplitDirection::Vertical,
        }
    }

    /// The new leaf becomes the left child for left/up edges, the right
    /// child for right/down edges.
    pub fn new_leaf_is_left_child(&self) -> bool {
        matches!(self, PaneEdge::Left | PaneEdge::Up)
    }
}

/// Where a dragged session would land. Mirrors `PaneDropTarget`
/// (presentation state only — never persisted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaneDropTarget {
    Pane { pane_id: String, edge: PaneEdge },
    GroupEdge(PaneEdge),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneSplitBranch {
    Left,
    Right,
}

/// Addresses a split node inside a group's tree. The empty path is the root.
/// Mirrors `PaneSplitPath`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaneSplitPath {
    pub components: Vec<PaneSplitBranch>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaneSplit {
    pub direction: SplitDirection,
    pub ratio: f64,
    pub left: Box<PaneNode>,
    pub right: Box<PaneNode>,
}

impl PaneSplit {
    pub fn new(direction: SplitDirection, ratio: f64, left: PaneNode, right: PaneNode) -> Self {
        Self {
            direction,
            ratio: clamp_ratio(ratio),
            left: Box::new(left),
            right: Box::new(right),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaneNode {
    Leaf(Pane),
    Split(PaneSplit),
}

impl PaneNode {
    /// Leaves in preorder (left-first). The normative leaf order: drives
    /// sidebar rows, representative promotion, and cap trimming.
    pub fn leaves(&self) -> Vec<&Pane> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    fn collect_leaves<'a>(&'a self, out: &mut Vec<&'a Pane>) {
        match self {
            PaneNode::Leaf(pane) => out.push(pane),
            PaneNode::Split(split) => {
                split.left.collect_leaves(out);
                split.right.collect_leaves(out);
            }
        }
    }

    pub fn leaves_mut(&mut self) -> Vec<&mut Pane> {
        let mut out = Vec::new();
        self.collect_leaves_mut(&mut out);
        out
    }

    fn collect_leaves_mut<'a>(&'a mut self, out: &mut Vec<&'a mut Pane>) {
        match self {
            PaneNode::Leaf(pane) => out.push(pane),
            PaneNode::Split(split) => {
                split.left.collect_leaves_mut(out);
                split.right.collect_leaves_mut(out);
            }
        }
    }

    pub fn session_leaves(&self) -> Vec<&Pane> {
        self.leaves()
            .into_iter()
            .filter(|p| p.content.session_id().is_some())
            .collect()
    }

    pub fn contains_launcher(&self) -> bool {
        self.leaves().iter().any(|p| p.content.is_launcher())
    }

    pub fn leaf(&self, pane_id: &str) -> Option<&Pane> {
        self.leaves().into_iter().find(|p| p.id == pane_id)
    }

    pub fn path_to_pane(&self, pane_id: &str) -> Option<PaneSplitPath> {
        match self {
            PaneNode::Leaf(pane) => {
                if pane.id == pane_id {
                    Some(PaneSplitPath::default())
                } else {
                    None
                }
            }
            PaneNode::Split(split) => {
                if let Some(mut left) = split.left.path_to_pane(pane_id) {
                    let mut components = vec![PaneSplitBranch::Left];
                    components.append(&mut left.components);
                    return Some(PaneSplitPath { components });
                }
                if let Some(mut right) = split.right.path_to_pane(pane_id) {
                    let mut components = vec![PaneSplitBranch::Right];
                    components.append(&mut right.components);
                    return Some(PaneSplitPath { components });
                }
                None
            }
        }
    }

    fn node_at_path_mut(&mut self, path: &PaneSplitPath) -> Option<&mut PaneNode> {
        let mut node = self;
        for branch in &path.components {
            node = match node {
                PaneNode::Split(split) => match branch {
                    PaneSplitBranch::Left => &mut split.left,
                    PaneSplitBranch::Right => &mut split.right,
                },
                PaneNode::Leaf(_) => return None,
            };
        }
        Some(node)
    }

    /// Artificial grid dimensions for the focus-neighbor spatial query:
    /// leaf = 1x1; horizontal split sums widths / maxes heights; vertical
    /// split sums heights / maxes widths.
    fn grid_size(&self) -> (u32, u32) {
        match self {
            PaneNode::Leaf(_) => (1, 1),
            PaneNode::Split(split) => {
                let (lw, lh) = split.left.grid_size();
                let (rw, rh) = split.right.grid_size();
                match split.direction {
                    SplitDirection::Horizontal => (lw + rw, lh.max(rh)),
                    SplitDirection::Vertical => (lw.max(rw), lh + rh),
                }
            }
        }
    }
}

/// One pane group: a window's worth of splits. Mirrors the durable
/// `DurablePaneGroup` plus the live launcher snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct PaneGroup {
    pub id: String,
    pub representative_pane_id: String,
    pub root: PaneNode,
    /// Pre-insert tree snapshot while a launcher leaf is live. Mirrors the
    /// Swift insertLauncher/removeLauncher snapshot discipline.
    launcher_snapshot: Option<PaneNode>,
}

impl PaneGroup {
    pub fn new(id: String, representative_pane_id: String, root: PaneNode) -> Self {
        Self {
            id,
            representative_pane_id,
            root,
            launcher_snapshot: None,
        }
    }

    /// Insert a session leaf by splitting `target_pane_id` on `edge` at
    /// ratio 0.5. The new leaf is the left child for left/up edges, the
    /// right child for right/down edges.
    pub fn insert_session(
        &mut self,
        session_id: &str,
        target_pane_id: &str,
        edge: PaneEdge,
        new_pane_id: String,
    ) -> bool {
        let Some(path) = self.root.path_to_pane(target_pane_id) else {
            return false;
        };
        let Some(node) = self.root.node_at_path_mut(&path) else {
            return false;
        };
        let PaneNode::Leaf(target) = node else {
            return false;
        };
        let target = std::mem::replace(
            target,
            Pane {
                id: String::new(),
                content: PaneContent::Session { id: String::new() },
            },
        );
        let new_leaf = PaneNode::Leaf(Pane {
            id: new_pane_id,
            content: PaneContent::Session {
                id: session_id.to_string(),
            },
        });
        let target_leaf = PaneNode::Leaf(target);
        let (left, right) = if edge.new_leaf_is_left_child() {
            (new_leaf, target_leaf)
        } else {
            (target_leaf, new_leaf)
        };
        *node = PaneNode::Split(PaneSplit::new(edge.split_direction(), 0.5, left, right));
        self.launcher_snapshot = None; // any explicit geometry mutation binds the launcher
        true
    }

    /// Insert at the group's root edge; the new leaf's share is
    /// 1/(sessionLeafCount+1).
    pub fn insert_session_at_group_edge(
        &mut self,
        session_id: &str,
        edge: PaneEdge,
        new_pane_id: String,
    ) -> bool {
        let count = self.root.session_leaves().len();
        // The ratio is the left child's share: the new leaf gets 1/(n+1)
        // when it lands on the left, the existing content keeps n/(n+1)
        // when the new leaf lands on the right.
        let ratio = if edge.new_leaf_is_left_child() {
            1.0 / (count as f64 + 1.0)
        } else {
            count as f64 / (count as f64 + 1.0)
        };
        let new_leaf = PaneNode::Leaf(Pane {
            id: new_pane_id,
            content: PaneContent::Session {
                id: session_id.to_string(),
            },
        });
        let old_root = std::mem::replace(
            &mut self.root,
            PaneNode::Leaf(Pane {
                id: String::new(),
                content: PaneContent::Session { id: String::new() },
            }),
        );
        let (left, right) = if edge.new_leaf_is_left_child() {
            (new_leaf, old_root)
        } else {
            (old_root, new_leaf)
        };
        self.root = PaneNode::Split(PaneSplit::new(edge.split_direction(), ratio, left, right));
        self.launcher_snapshot = None;
        true
    }

    /// Detach a leaf and collapse its parent split to the surviving
    /// sibling. Returns `false` when the group dissolves (fewer than 2
    /// session leaves and not exactly 1 session + 1 live launcher).
    pub fn detach_pane(&mut self, pane_id: &str) -> bool {
        let Some(path) = self.root.path_to_pane(pane_id) else {
            return false;
        };
        if path.components.is_empty() {
            return false; // single-leaf group: detaching the root dissolves it
        }
        let parent_path = PaneSplitPath {
            components: path.components[..path.components.len() - 1].to_vec(),
        };
        let last = *path.components.last().unwrap();
        let Some(parent) = self.root.node_at_path_mut(&parent_path) else {
            return false;
        };
        let PaneNode::Split(split) = parent else {
            return false;
        };
        let survivor = match last {
            PaneSplitBranch::Left => std::mem::replace(
                &mut split.right,
                Box::new(PaneNode::Leaf(Pane {
                    id: String::new(),
                    content: PaneContent::Session { id: String::new() },
                })),
            ),
            PaneSplitBranch::Right => std::mem::replace(
                &mut split.left,
                Box::new(PaneNode::Leaf(Pane {
                    id: String::new(),
                    content: PaneContent::Session { id: String::new() },
                })),
            ),
        };
        *parent = *survivor;
        self.launcher_snapshot = None;
        // Promote the representative when it was detached.
        if self.representative_pane_id == pane_id {
            if let Some(first) = self.root.session_leaves().first() {
                self.representative_pane_id = first.id.clone();
            }
        }
        self.is_viable()
    }

    /// Whether the group survives: >= 2 session leaves, or exactly 1
    /// session plus a live launcher.
    pub fn is_viable(&self) -> bool {
        let sessions = self.root.session_leaves().len();
        sessions >= 2 || (sessions == 1 && self.root.contains_launcher())
    }

    /// Equalize: each split's ratio = leftWeight/(leftWeight+rightWeight);
    /// leaf weight 1, same-direction split = sum of children's weights,
    /// perpendicular split = 1. Recurses into all splits.
    pub fn equalize(&mut self) {
        equalize_node(&mut self.root);
    }

    /// Exchange the positions of two leaves; pane ids travel with their
    /// leaves, so the representative id is unaffected.
    pub fn swap_panes(&mut self, pane_id: &str, other_pane_id: &str) -> bool {
        if pane_id == other_pane_id {
            return false;
        }
        let (Some(pa), Some(pb)) = (
            self.root.path_to_pane(pane_id),
            self.root.path_to_pane(other_pane_id),
        ) else {
            return false;
        };
        // Two distinct leaf paths are disjoint (neither is a prefix of the
        // other), so swapping via placeholders is safe: replacing a leaf
        // with a leaf never invalidates the other path.
        let placeholder = || {
            PaneNode::Leaf(Pane {
                id: String::new(),
                content: PaneContent::Session { id: String::new() },
            })
        };
        let a = std::mem::replace(self.root.node_at_path_mut(&pa).unwrap(), placeholder());
        let b = std::mem::replace(self.root.node_at_path_mut(&pb).unwrap(), placeholder());
        *self.root.node_at_path_mut(&pa).unwrap() = b;
        *self.root.node_at_path_mut(&pb).unwrap() = a;
        true
    }

    /// Resize the split at `path` to `ratio` (clamped).
    pub fn resize_split(&mut self, path: &PaneSplitPath, ratio: f64) -> bool {
        let Some(node) = self.root.node_at_path_mut(path) else {
            return false;
        };
        let PaneNode::Split(split) = node else {
            return false;
        };
        split.ratio = clamp_ratio(ratio);
        self.launcher_snapshot = None;
        true
    }

    /// Insert a launcher leaf (snapshots the pre-insert tree first).
    pub fn insert_launcher(
        &mut self,
        project_id: &str,
        target_pane_id: &str,
        edge: PaneEdge,
        new_pane_id: String,
    ) -> bool {
        if self.launcher_snapshot.is_none() {
            self.launcher_snapshot = Some(self.root.clone());
        }
        let Some(path) = self.root.path_to_pane(target_pane_id) else {
            return false;
        };
        let Some(node) = self.root.node_at_path_mut(&path) else {
            return false;
        };
        let PaneNode::Leaf(target) = node else {
            return false;
        };
        let target = std::mem::replace(
            target,
            Pane {
                id: String::new(),
                content: PaneContent::Session { id: String::new() },
            },
        );
        let new_leaf = PaneNode::Leaf(Pane {
            id: new_pane_id,
            content: PaneContent::Launcher {
                project_id: project_id.to_string(),
            },
        });
        let target_leaf = PaneNode::Leaf(target);
        let (left, right) = if edge.new_leaf_is_left_child() {
            (new_leaf, target_leaf)
        } else {
            (target_leaf, new_leaf)
        };
        *node = PaneNode::Split(PaneSplit::new(edge.split_direction(), 0.5, left, right));
        true
    }

    /// Remove a launcher leaf: restore the snapshot iff its session-leaf id
    /// set equals the launcher-stripped tree's; otherwise detach normally.
    pub fn remove_launcher(&mut self, pane_id: &str) -> bool {
        let snapshot = self.launcher_snapshot.clone();
        if let Some(snapshot) = snapshot {
            let snapshot_ids: HashSet<&str> = snapshot
                .session_leaves()
                .iter()
                .map(|p| p.id.as_str())
                .collect();
            let mut stripped = self.root.clone();
            // Strip launcher leaves via the detach path on a clone.
            let launcher_ids: Vec<String> = stripped
                .leaves()
                .into_iter()
                .filter(|p| p.content.is_launcher())
                .map(|p| p.id.clone())
                .collect();
            let mut ok = true;
            // Detach each launcher on the clone.
            let mut tmp = PaneGroup::new(
                self.id.clone(),
                self.representative_pane_id.clone(),
                stripped,
            );
            for id in &launcher_ids {
                if tmp.root.path_to_pane(id).is_some() {
                    // detach without viability checks on the scratch copy
                    detach_unchecked(&mut tmp.root, id);
                } else {
                    ok = false;
                }
            }
            stripped = tmp.root;
            let stripped_ids: HashSet<&str> = stripped
                .session_leaves()
                .iter()
                .map(|p| p.id.as_str())
                .collect();
            if ok && snapshot_ids == stripped_ids {
                self.root = snapshot;
                self.launcher_snapshot = None;
                return true;
            }
        }
        // Fall back to a plain detach of the launcher leaf.
        let detached = detach_unchecked(&mut self.root, pane_id);
        if detached {
            self.launcher_snapshot = None;
        }
        detached
    }

    /// Bind a launcher leaf to a real session (keeps geometry, swaps content).
    pub fn bind_launcher(&mut self, pane_id: &str, session_id: &str) -> bool {
        let Some(path) = self.root.path_to_pane(pane_id) else {
            return false;
        };
        let Some(node) = self.root.node_at_path_mut(&path) else {
            return false;
        };
        let PaneNode::Leaf(pane) = node else {
            return false;
        };
        if !pane.content.is_launcher() {
            return false;
        }
        pane.content = PaneContent::Session {
            id: session_id.to_string(),
        };
        true
    }

    /// Pure spatial query: the nearest leaf in `direction` from `pane_id`
    /// over the artificial grid subdivision. Returns the neighbor's pane id.
    pub fn focus_neighbor(&self, pane_id: &str, direction: FocusDirection) -> Option<String> {
        // Build the rect of every leaf in grid coordinates.
        let mut rects: HashMap<String, (u32, u32, u32, u32)> = HashMap::new();
        assign_rects(&self.root, 0, 0, &mut rects);
        let &(x, y, w, h) = rects.get(pane_id)?;
        let cx = x as f64 + w as f64 / 2.0;
        let cy = y as f64 + h as f64 / 2.0;
        let mut best: Option<(f64, String)> = None;
        for (id, &(ox, oy, ow, oh)) in &rects {
            if id == pane_id {
                continue;
            }
            let ocx = ox as f64 + ow as f64 / 2.0;
            let ocy = oy as f64 + oh as f64 / 2.0;
            let (dx, dy) = (ocx - cx, ocy - cy);
            let in_direction = match direction {
                FocusDirection::Left => dx < 0.0 && dy.abs() <= dx.abs(),
                FocusDirection::Right => dx > 0.0 && dy.abs() <= dx.abs(),
                FocusDirection::Up => dy < 0.0 && dx.abs() <= dy.abs(),
                FocusDirection::Down => dy > 0.0 && dx.abs() <= dy.abs(),
            };
            if !in_direction {
                continue;
            }
            let dist = dx * dx + dy * dy;
            if best.as_ref().is_none_or(|(d, _)| dist < *d) {
                best = Some((dist, id.clone()));
            }
        }
        best.map(|(_, id)| id)
    }
}

fn assign_rects(
    node: &PaneNode,
    x: u32,
    y: u32,
    out: &mut HashMap<String, (u32, u32, u32, u32)>,
) -> (u32, u32) {
    match node {
        PaneNode::Leaf(pane) => {
            out.insert(pane.id.clone(), (x, y, 1, 1));
            (1, 1)
        }
        PaneNode::Split(split) => {
            let (lw, lh) = split.left.grid_size();
            let (rw, rh) = split.right.grid_size();
            match split.direction {
                SplitDirection::Horizontal => {
                    assign_rects(&split.left, x, y, out);
                    assign_rects(&split.right, x + lw, y, out);
                    (lw + rw, lh.max(rh))
                }
                SplitDirection::Vertical => {
                    assign_rects(&split.left, x, y, out);
                    assign_rects(&split.right, x, y + lh, out);
                    (lw.max(rw), lh + rh)
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDirection {
    Left,
    Right,
    Up,
    Down,
}

fn equalize_node(node: &mut PaneNode) {
    if let PaneNode::Split(split) = node {
        let lw = child_weight(&split.left, split.direction);
        let rw = child_weight(&split.right, split.direction);
        let total = lw + rw;
        if total > 0.0 {
            split.ratio = clamp_ratio(lw / total);
        }
        equalize_node(&mut split.left);
        equalize_node(&mut split.right);
    }
}

fn child_weight(node: &PaneNode, parent_direction: SplitDirection) -> f64 {
    match node {
        PaneNode::Leaf(_) => 1.0,
        PaneNode::Split(split) => {
            if split.direction == parent_direction {
                child_weight(&split.left, parent_direction)
                    + child_weight(&split.right, parent_direction)
            } else {
                1.0
            }
        }
    }
}

/// Detach without viability checks (used for scratch copies).
fn detach_unchecked(root: &mut PaneNode, pane_id: &str) -> bool {
    let Some(path) = root.path_to_pane(pane_id) else {
        return false;
    };
    if path.components.is_empty() {
        return false;
    }
    let parent_path = PaneSplitPath {
        components: path.components[..path.components.len() - 1].to_vec(),
    };
    let last = *path.components.last().unwrap();
    let Some(parent) = root.node_at_path_mut(&parent_path) else {
        return false;
    };
    let PaneNode::Split(split) = parent else {
        return false;
    };
    let survivor = match last {
        PaneSplitBranch::Left => std::mem::replace(
            &mut split.right,
            Box::new(PaneNode::Leaf(Pane {
                id: String::new(),
                content: PaneContent::Session { id: String::new() },
            })),
        ),
        PaneSplitBranch::Right => std::mem::replace(
            &mut split.left,
            Box::new(PaneNode::Leaf(Pane {
                id: String::new(),
                content: PaneContent::Session { id: String::new() },
            })),
        ),
    };
    *parent = *survivor;
    true
}

/// Canonical lowercase UUID; non-UUID ids are replaced at restore (which
/// would break determinism, so callers must pass canonical ids).
pub fn canonical_id(value: &str) -> String {
    let v = value.trim().to_lowercase();
    if is_uuid(&v) {
        v
    } else {
        // Deterministic fallback: hash into a v4-shaped UUID.
        let mut h: u128 = 0xcbf29ce484222325;
        for b in v.bytes() {
            h = h.wrapping_mul(0x100000001b3).wrapping_add(b as u128);
            h ^= h >> 29;
            h = h.wrapping_mul(0xbf58476d1ce4e5b9);
        }
        let hex = format!("{h:032x}");
        format!(
            "{}-{}-4{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[13..16],
            &hex[16..20],
            &hex[20..32]
        )
    }
}

fn is_uuid(v: &str) -> bool {
    let b = v.as_bytes();
    b.len() == 36
        && b[8] == b'-'
        && b[13] == b'-'
        && b[18] == b'-'
        && b[23] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| [8, 13, 18, 23].contains(&i) || c.is_ascii_hexdigit())
}

/// The durable layout: `{ version, groups: [{ id, representativePaneID, root }] }`.
/// Mirrors the contract's durable projection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DurableLayout {
    pub version: u32,
    #[serde(default)]
    pub groups: Vec<DurablePaneGroup>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DurablePaneGroup {
    pub id: String,
    #[serde(alias = "representativePaneID")]
    pub representative_pane_id: String,
    pub root: DurablePaneNode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DurablePaneNode {
    Pane(DurablePane),
    Split(DurableSplit),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DurablePane {
    pub id: String,
    #[serde(alias = "sessionID")]
    pub session_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DurableSplit {
    pub direction: SplitDirection,
    pub ratio: f64,
    pub left: Box<DurablePaneNode>,
    pub right: Box<DurablePaneNode>,
}

impl DurableLayout {
    /// Encode groups; the durable projection strips launcher leaves via the
    /// removal algorithm, and while a launcher is live with a snapshot the
    /// snapshot is what gets encoded. Groups with fewer than 2 session
    /// leaves are omitted.
    pub fn encode(groups: &[PaneGroup]) -> Self {
        let mut out = Vec::new();
        for group in groups {
            let tree = if group.launcher_snapshot.is_some() && group.root.contains_launcher() {
                group.launcher_snapshot.clone().unwrap()
            } else {
                let mut stripped = group.root.clone();
                let launcher_ids: Vec<String> = stripped
                    .leaves()
                    .into_iter()
                    .filter(|p| p.content.is_launcher())
                    .map(|p| p.id.clone())
                    .collect();
                for id in launcher_ids {
                    detach_unchecked(&mut stripped, &id);
                }
                stripped
            };
            if tree.session_leaves().len() < 2 {
                continue;
            }
            out.push(DurablePaneGroup {
                id: group.id.clone(),
                representative_pane_id: group.representative_pane_id.clone(),
                root: durable_node(&tree),
            });
        }
        Self {
            version: DURABLE_VERSION,
            groups: out,
        }
    }

    /// Decode through the real codec: canonicalize ids, run v1 migration,
    /// trim trailing preorder leaves past the cap.
    pub fn decode(value: &serde_json::Value) -> Result<Vec<PaneGroup>, String> {
        // v1 flat shape?
        if let Some(groups) = value.get("groups") {
            if groups
                .as_array()
                .is_some_and(|gs| gs.iter().any(|g| g.get("panes").is_some()))
            {
                return decode_v1(value);
            }
        }
        let layout: DurableLayout =
            serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        let mut groups = Vec::new();
        let mut seen_sessions: HashSet<String> = HashSet::new();
        for g in layout.groups {
            let root = live_node(&g.root);
            let mut group = PaneGroup::new(
                canonical_id(&g.id),
                canonical_id(&g.representative_pane_id),
                root,
            );
            // Cross-group session dedup: the first occurrence wins; later
            // duplicates are detached before the group's own canonicalization.
            let dups: Vec<String> = group
                .root
                .session_leaves()
                .iter()
                .filter_map(|p| p.content.session_id())
                .filter(|id| !seen_sessions.insert((*id).to_string()))
                .map(|id| {
                    group
                        .root
                        .session_leaves()
                        .iter()
                        .find(|p| p.content.session_id() == Some(id))
                        .map(|p| p.id.clone())
                        .unwrap()
                })
                .collect();
            for id in dups {
                detach_unchecked(&mut group.root, &id);
            }
            canonicalize_group(&mut group);
            if group.is_viable() {
                groups.push(group);
            }
        }
        Ok(groups)
    }
}

fn durable_node(node: &PaneNode) -> DurablePaneNode {
    match node {
        PaneNode::Leaf(pane) => DurablePaneNode::Pane(DurablePane {
            id: pane.id.clone(),
            session_id: pane.content.session_id().unwrap_or("").to_string(),
        }),
        PaneNode::Split(split) => DurablePaneNode::Split(DurableSplit {
            direction: split.direction,
            ratio: split.ratio,
            left: Box::new(durable_node(&split.left)),
            right: Box::new(durable_node(&split.right)),
        }),
    }
}

fn live_node(node: &DurablePaneNode) -> PaneNode {
    match node {
        DurablePaneNode::Pane(p) => PaneNode::Leaf(Pane {
            id: canonical_id(&p.id),
            content: PaneContent::Session {
                id: p.session_id.clone(),
            },
        }),
        DurablePaneNode::Split(s) => PaneNode::Split(PaneSplit::new(
            s.direction,
            s.ratio,
            live_node(&s.left),
            live_node(&s.right),
        )),
    }
}

fn canonicalize_group(group: &mut PaneGroup) {
    // Representative must be a live leaf; else promote the first session leaf.
    if group.root.leaf(&group.representative_pane_id).is_none() {
        if let Some(first) = group.root.session_leaves().first() {
            group.representative_pane_id = first.id.clone();
        }
    }
    // Trim trailing preorder leaves past the session-leaf cap.
    loop {
        let session_ids: Vec<String> = group
            .root
            .session_leaves()
            .iter()
            .map(|p| p.id.clone())
            .collect();
        if session_ids.len() <= SESSION_LEAF_CAP {
            break;
        }
        let victim = session_ids.last().unwrap().clone();
        if !detach_unchecked(&mut group.root, &victim) {
            break;
        }
    }
}

/// v1 migration: panes fold right-leaning —
/// node(i) = split(horizontal, clamp(f_i / (f_i + ... + f_n)), pane_i, node(i+1)).
fn decode_v1(value: &serde_json::Value) -> Result<Vec<PaneGroup>, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct V1Pane {
        id: String,
        #[serde(default, alias = "sessionId", alias = "sessionID")]
        session_id: String,
        #[serde(default)]
        fraction: f64,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct V1Group {
        id: String,
        #[serde(alias = "representativePaneID")]
        representative_pane_id: String,
        #[serde(default)]
        panes: Vec<V1Pane>,
    }
    #[derive(Deserialize)]
    struct V1Layout {
        #[serde(default)]
        groups: Vec<V1Group>,
    }
    let layout: V1Layout = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    let mut groups = Vec::new();
    for g in layout.groups {
        if g.panes.len() < 2 {
            continue;
        }
        fn fold(panes: &[V1Pane], index: usize) -> PaneNode {
            let pane = &panes[index];
            let leaf = PaneNode::Leaf(Pane {
                id: canonical_id(&pane.id),
                content: PaneContent::Session {
                    id: pane.session_id.clone(),
                },
            });
            if index + 1 >= panes.len() {
                return leaf;
            }
            let remaining: f64 = panes[index..]
                .iter()
                .map(|p| {
                    if p.fraction.is_finite() {
                        p.fraction.max(0.0)
                    } else {
                        0.0
                    }
                })
                .sum();
            let fraction = if pane.fraction.is_finite() {
                pane.fraction.max(0.0)
            } else {
                0.0
            };
            let ratio = if remaining > 0.0 {
                clamp_ratio(fraction / remaining)
            } else {
                0.5
            };
            PaneNode::Split(PaneSplit::new(
                SplitDirection::Horizontal,
                ratio,
                leaf,
                fold(panes, index + 1),
            ))
        }
        let root = fold(&g.panes, 0);
        let mut group = PaneGroup::new(
            canonical_id(&g.id),
            canonical_id(&g.representative_pane_id),
            root,
        );
        canonicalize_group(&mut group);
        if group.is_viable() {
            groups.push(group);
        }
    }
    Ok(groups)
}

/// A whole-layout reconcile: drop session ids that are no longer eligible
/// (closed sessions); dissolve groups that become non-viable. Promotes the
/// representative when it was detached, like `detach_pane` does.
pub fn reconcile(groups: &mut Vec<PaneGroup>, eligible_session_ids: &HashSet<String>) {
    for group in groups.iter_mut() {
        let dead: Vec<String> = group
            .root
            .session_leaves()
            .iter()
            .filter(|p| {
                p.content
                    .session_id()
                    .is_some_and(|id| !eligible_session_ids.contains(id))
            })
            .map(|p| p.id.clone())
            .collect();
        for id in dead {
            detach_unchecked(&mut group.root, &id);
            if group.representative_pane_id == id
                && group.root.leaf(&group.representative_pane_id).is_none()
            {
                if let Some(first) = group.root.session_leaves().first() {
                    group.representative_pane_id = first.id.clone();
                }
            }
        }
        group.launcher_snapshot = None;
    }
    groups.retain(|g| g.is_viable());
}

/// Operation errors for layout-level pane operations. The `code()` strings
/// match the contract fixture's `expect.error` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneOpError {
    DuplicateSession,
    CapacityReached,
    PaneNotFound,
    SplitNotFound,
    PanesBelongToDifferentGroups,
    GroupNotFound,
}

impl PaneOpError {
    pub fn code(&self) -> &'static str {
        match self {
            PaneOpError::DuplicateSession => "duplicateSession",
            PaneOpError::CapacityReached => "capacityReached",
            PaneOpError::PaneNotFound => "paneNotFound",
            PaneOpError::SplitNotFound => "splitNotFound",
            PaneOpError::PanesBelongToDifferentGroups => "panesBelongToDifferentGroups",
            PaneOpError::GroupNotFound => "groupNotFound",
        }
    }
}

/// Where an `insertSession` operation targets.
#[derive(Clone, Debug)]
pub enum InsertTarget {
    /// Split at an existing pane.
    Pane { pane_id: String },
    /// Insert at a group's root edge.
    GroupEdge { group_id: String },
    /// The target session is solo (not in any group): create a fresh group
    /// holding the solo session and the new session.
    BesideSolo {
        session_id: String,
        new_group_id: String,
        new_representative_pane_id: String,
    },
}

fn find_group_with_session<'a>(groups: &'a [PaneGroup], session_id: &str) -> Option<&'a PaneGroup> {
    groups.iter().find(|g| {
        g.root
            .session_leaves()
            .iter()
            .any(|p| p.content.session_id() == Some(session_id))
    })
}

fn find_group_with_pane_mut<'a>(
    groups: &'a mut [PaneGroup],
    pane_id: &str,
) -> Option<&'a mut PaneGroup> {
    groups
        .iter_mut()
        .find(|g| g.root.path_to_pane(pane_id).is_some())
}

fn find_group_mut<'a>(groups: &'a mut [PaneGroup], group_id: &str) -> Option<&'a mut PaneGroup> {
    groups.iter_mut().find(|g| g.id == group_id)
}

/// Layout-level session insert with contract error classification.
pub fn layout_insert_session(
    groups: &mut Vec<PaneGroup>,
    session_id: &str,
    target: InsertTarget,
    edge: PaneEdge,
    new_pane_id: String,
) -> Result<(), PaneOpError> {
    if find_group_with_session(groups, session_id).is_some() {
        return Err(PaneOpError::DuplicateSession);
    }
    match target {
        InsertTarget::Pane { pane_id } => {
            let group =
                find_group_with_pane_mut(groups, &pane_id).ok_or(PaneOpError::PaneNotFound)?;
            if group.root.session_leaves().len() >= SESSION_LEAF_CAP {
                return Err(PaneOpError::CapacityReached);
            }
            if group.insert_session(session_id, &pane_id, edge, new_pane_id) {
                Ok(())
            } else {
                Err(PaneOpError::PaneNotFound)
            }
        }
        InsertTarget::GroupEdge { group_id } => {
            let group = find_group_mut(groups, &group_id).ok_or(PaneOpError::GroupNotFound)?;
            if group.root.session_leaves().len() >= SESSION_LEAF_CAP {
                return Err(PaneOpError::CapacityReached);
            }
            if group.insert_session_at_group_edge(session_id, edge, new_pane_id) {
                Ok(())
            } else {
                Err(PaneOpError::GroupNotFound)
            }
        }
        InsertTarget::BesideSolo {
            session_id: solo_id,
            new_group_id,
            new_representative_pane_id,
        } => {
            if find_group_with_session(groups, &solo_id).is_some() {
                return Err(PaneOpError::DuplicateSession);
            }
            let solo_leaf = PaneNode::Leaf(Pane {
                id: new_representative_pane_id.clone(),
                content: PaneContent::Session { id: solo_id },
            });
            let new_leaf = PaneNode::Leaf(Pane {
                id: new_pane_id,
                content: PaneContent::Session {
                    id: session_id.to_string(),
                },
            });
            let (left, right) = if edge.new_leaf_is_left_child() {
                (new_leaf, solo_leaf)
            } else {
                (solo_leaf, new_leaf)
            };
            let root = PaneNode::Split(PaneSplit::new(edge.split_direction(), 0.5, left, right));
            groups.push(PaneGroup::new(
                new_group_id,
                new_representative_pane_id,
                root,
            ));
            Ok(())
        }
    }
}

/// Layout-level detach with contract error classification. A group that
/// dissolves is removed from the layout.
pub fn layout_detach_pane(groups: &mut Vec<PaneGroup>, pane_id: &str) -> Result<(), PaneOpError> {
    let index = groups
        .iter()
        .position(|g| g.root.path_to_pane(pane_id).is_some())
        .ok_or(PaneOpError::PaneNotFound)?;
    if groups[index].detach_pane(pane_id) {
        Ok(())
    } else {
        // The detach dissolved the group.
        groups.remove(index);
        Ok(())
    }
}

/// Close (remove) a whole group.
pub fn layout_close_group(groups: &mut Vec<PaneGroup>, group_id: &str) -> Result<(), PaneOpError> {
    let before = groups.len();
    groups.retain(|g| g.id != group_id);
    if groups.len() == before {
        Err(PaneOpError::GroupNotFound)
    } else {
        Ok(())
    }
}

/// Layout-level split resize with contract error classification.
pub fn layout_resize_split(
    groups: &mut [PaneGroup],
    group_id: &str,
    path: &PaneSplitPath,
    ratio: f64,
) -> Result<(), PaneOpError> {
    let group = find_group_mut(groups, group_id).ok_or(PaneOpError::GroupNotFound)?;
    if group.resize_split(path, ratio) {
        Ok(())
    } else {
        Err(PaneOpError::SplitNotFound)
    }
}

/// Layout-level equalize.
pub fn layout_equalize(groups: &mut [PaneGroup], group_id: &str) -> Result<(), PaneOpError> {
    let group = find_group_mut(groups, group_id).ok_or(PaneOpError::GroupNotFound)?;
    group.equalize();
    Ok(())
}

/// Layout-level pane swap with contract error classification.
pub fn layout_swap_panes(
    groups: &mut [PaneGroup],
    pane_id: &str,
    other_pane_id: &str,
) -> Result<(), PaneOpError> {
    let a = groups
        .iter()
        .position(|g| g.root.path_to_pane(pane_id).is_some());
    let b = groups
        .iter()
        .position(|g| g.root.path_to_pane(other_pane_id).is_some());
    match (a, b) {
        (Some(ai), Some(bi)) if ai == bi => {
            if groups[ai].swap_panes(pane_id, other_pane_id) {
                Ok(())
            } else {
                Err(PaneOpError::PaneNotFound)
            }
        }
        (Some(_), Some(_)) => Err(PaneOpError::PanesBelongToDifferentGroups),
        _ => Err(PaneOpError::PaneNotFound),
    }
}

/// Layout-level launcher insert.
pub fn layout_insert_launcher(
    groups: &mut [PaneGroup],
    project_id: &str,
    target_pane_id: &str,
    edge: PaneEdge,
    new_pane_id: String,
) -> Result<(), PaneOpError> {
    let group =
        find_group_with_pane_mut(groups, target_pane_id).ok_or(PaneOpError::PaneNotFound)?;
    if group.insert_launcher(project_id, target_pane_id, edge, new_pane_id) {
        Ok(())
    } else {
        Err(PaneOpError::PaneNotFound)
    }
}

/// Layout-level launcher removal.
pub fn layout_remove_launcher(groups: &mut [PaneGroup], pane_id: &str) -> Result<(), PaneOpError> {
    let group = find_group_with_pane_mut(groups, pane_id).ok_or(PaneOpError::PaneNotFound)?;
    if group.remove_launcher(pane_id) {
        Ok(())
    } else {
        Err(PaneOpError::PaneNotFound)
    }
}

/// Layout-level launcher bind.
pub fn layout_bind_launcher(
    groups: &mut [PaneGroup],
    pane_id: &str,
    session_id: &str,
) -> Result<(), PaneOpError> {
    let group = find_group_with_pane_mut(groups, pane_id).ok_or(PaneOpError::PaneNotFound)?;
    if group.bind_launcher(pane_id, session_id) {
        Ok(())
    } else {
        Err(PaneOpError::PaneNotFound)
    }
}

/// Layout-level focus-neighbor query.
pub fn layout_focus_neighbor(
    groups: &[PaneGroup],
    pane_id: &str,
    direction: FocusDirection,
) -> Result<Option<String>, PaneOpError> {
    let group = groups
        .iter()
        .find(|g| g.root.path_to_pane(pane_id).is_some())
        .ok_or(PaneOpError::PaneNotFound)?;
    Ok(group.focus_neighbor(pane_id, direction))
}

/// A live leaf for the `expectLiveLeaves` projection: session leaves and
/// live launcher leaves in preorder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveLeaf {
    Session { pane_id: String, session_id: String },
    Launcher { pane_id: String, project_id: String },
}

/// The live-leaf projection of a group (preorder over the live tree,
/// including launcher leaves).
pub fn live_leaves(group: &PaneGroup) -> Vec<LiveLeaf> {
    group
        .root
        .leaves()
        .into_iter()
        .map(|p| match &p.content {
            PaneContent::Session { id } => LiveLeaf::Session {
                pane_id: p.id.clone(),
                session_id: id.clone(),
            },
            PaneContent::Launcher { project_id } => LiveLeaf::Launcher {
                pane_id: p.id.clone(),
                project_id: project_id.clone(),
            },
        })
        .collect()
}

/// Dioxus component: recursive split renderer for the desktop launcher.
/// Each leaf renders through `render_leaf`.
pub mod component {
    use super::*;
    use dioxus::prelude::*;

    #[component]
    pub fn PaneTreeView(root: PaneNode, render_leaf: EventHandler<Pane>) -> Element {
        match root {
            PaneNode::Leaf(pane) => rsx! {
                div { class: "pane-leaf", key: "{pane.id}",
                    {render_leaf.call(pane)}
                }
            },
            PaneNode::Split(split) => {
                let (cls, left_style, right_style) = match split.direction {
                    SplitDirection::Horizontal => (
                        "pane-split pane-split-horizontal",
                        format!("flex: {} 1 0%", split.ratio),
                        format!("flex: {} 1 0%", 1.0 - split.ratio),
                    ),
                    SplitDirection::Vertical => (
                        "pane-split pane-split-vertical",
                        format!("flex: {} 1 0%", split.ratio),
                        format!("flex: {} 1 0%", 1.0 - split.ratio),
                    ),
                };
                rsx! {
                    div { class: "{cls}",
                        div { class: "pane-branch", style: "{left_style}",
                            PaneTreeView { root: (*split.left).clone(), render_leaf }
                        }
                        div { class: "pane-branch", style: "{right_style}",
                            PaneTreeView { root: (*split.right).clone(), render_leaf }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pane(id: &str, session: &str) -> PaneNode {
        PaneNode::Leaf(Pane {
            id: id.to_string(),
            content: PaneContent::Session {
                id: session.to_string(),
            },
        })
    }

    fn leaf_ids(node: &PaneNode) -> Vec<String> {
        node.leaves().iter().map(|p| p.id.clone()).collect()
    }

    #[test]
    fn ratio_clamp() {
        assert_eq!(clamp_ratio(f64::NAN), 0.5);
        assert_eq!(clamp_ratio(0.01), 0.1);
        assert_eq!(clamp_ratio(0.99), 0.9);
        assert_eq!(clamp_ratio(0.4), 0.4);
    }

    #[test]
    fn insert_left_and_right_edges() {
        let mut g = PaneGroup::new("g".into(), "p1".into(), pane("p1", "s1"));
        assert!(g.insert_session("s2", "p1", PaneEdge::Right, "p2".into()));
        assert_eq!(leaf_ids(&g.root), vec!["p1", "p2"]);
        let mut g2 = PaneGroup::new("g".into(), "p1".into(), pane("p1", "s1"));
        assert!(g2.insert_session("s2", "p1", PaneEdge::Left, "p2".into()));
        assert_eq!(leaf_ids(&g2.root), vec!["p2", "p1"]);
        // vertical: left child goes on top
        let mut g3 = PaneGroup::new("g".into(), "p1".into(), pane("p1", "s1"));
        assert!(g3.insert_session("s2", "p1", PaneEdge::Up, "p2".into()));
        assert_eq!(leaf_ids(&g3.root), vec!["p2", "p1"]);
        match &g3.root {
            PaneNode::Split(s) => assert_eq!(s.direction, SplitDirection::Vertical),
            _ => panic!("expected split"),
        }
    }

    #[test]
    fn group_edge_insert_share() {
        let mut g = PaneGroup::new("g".into(), "p1".into(), pane("p1", "s1"));
        g.insert_session("s2", "p1", PaneEdge::Right, "p2".into());
        assert!(g.insert_session_at_group_edge("s3", PaneEdge::Down, "p3".into()));
        match &g.root {
            PaneNode::Split(s) => {
                // Down edge: the new leaf lands on the right/bottom, so the
                // existing two leaves keep 2/3 (fixture: insert.group-edge-right).
                assert!((s.ratio - 2.0 / 3.0).abs() < 1e-9);
                assert_eq!(s.direction, SplitDirection::Vertical);
            }
            _ => panic!("expected split"),
        }
        assert_eq!(leaf_ids(&g.root), vec!["p1", "p2", "p3"]);
        // Left edge: the new leaf lands on the left and takes 1/(n+1).
        let mut h = PaneGroup::new("h".into(), "p1".into(), pane("p1", "s1"));
        h.insert_session("s2", "p1", PaneEdge::Right, "p2".into());
        assert!(h.insert_session_at_group_edge("s3", PaneEdge::Left, "p3".into()));
        match &h.root {
            PaneNode::Split(s) => {
                assert!((s.ratio - 1.0 / 3.0).abs() < 1e-9);
            }
            _ => panic!("expected split"),
        }
        assert_eq!(leaf_ids(&h.root), vec!["p3", "p1", "p2"]);
    }

    #[test]
    fn detach_collapses_parent() {
        let mut g = PaneGroup::new("g".into(), "p1".into(), pane("p1", "s1"));
        g.insert_session("s2", "p1", PaneEdge::Right, "p2".into());
        g.insert_session("s3", "p2", PaneEdge::Right, "p3".into());
        assert!(g.detach_pane("p2"));
        assert_eq!(leaf_ids(&g.root), vec!["p1", "p3"]);
        // Representative promotion when the representative is detached.
        let mut g2 = PaneGroup::new("g".into(), "p9".into(), pane("p9", "s9"));
        g2.insert_session("s8", "p9", PaneEdge::Right, "p8".into());
        g2.detach_pane("p9");
        assert_eq!(g2.representative_pane_id, "p8");
    }

    #[test]
    fn detach_dissolves_below_two_sessions() {
        let mut g = PaneGroup::new("g".into(), "p1".into(), pane("p1", "s1"));
        g.insert_session("s2", "p1", PaneEdge::Right, "p2".into());
        assert!(!g.detach_pane("p2")); // 1 session left → dissolves
                                       // 1 session + 1 live launcher survives.
        let mut g2 = PaneGroup::new("g".into(), "p1".into(), pane("p1", "s1"));
        g2.insert_launcher("proj", "p1", PaneEdge::Right, "l1".into());
        g2.bind_launcher("l1", "s2");
        // now 2 sessions; detach one → 1 session, no launcher → dissolves
        assert!(!g2.detach_pane("p1"));
    }

    #[test]
    fn equalize_weights() {
        // root horizontal [p1 | vertical [p2 / p3]]: left weight 1, right
        // weight 1 (perpendicular) → 0.5; inner vertical 1/(1+1) = 0.5.
        let mut g = PaneGroup::new("g".into(), "p1".into(), pane("p1", "s1"));
        g.insert_session("s2", "p1", PaneEdge::Right, "p2".into());
        g.insert_session("s3", "p2", PaneEdge::Down, "p3".into());
        if let PaneNode::Split(root) = &mut g.root {
            root.ratio = 0.8;
            if let PaneNode::Split(inner) = &mut *root.right {
                inner.ratio = 0.2;
            }
        }
        g.equalize();
        match &g.root {
            PaneNode::Split(root) => {
                assert!((root.ratio - 0.5).abs() < 1e-9);
                match &*root.right {
                    PaneNode::Split(inner) => {
                        assert!((inner.ratio - 0.5).abs() < 1e-9)
                    }
                    _ => panic!("expected inner split"),
                }
            }
            _ => panic!("expected split"),
        }
    }

    #[test]
    fn swap_panes_keeps_representative() {
        let mut g = PaneGroup::new("g".into(), "p1".into(), pane("p1", "s1"));
        g.insert_session("s2", "p1", PaneEdge::Right, "p2".into());
        assert!(g.swap_panes("p1", "p2"));
        assert_eq!(leaf_ids(&g.root), vec!["p2", "p1"]);
        assert_eq!(g.representative_pane_id, "p1");
    }

    #[test]
    fn launcher_snapshot_restore() {
        let mut g = PaneGroup::new("g".into(), "p1".into(), pane("p1", "s1"));
        g.insert_session("s2", "p1", PaneEdge::Right, "p2".into());
        let before = g.root.clone();
        assert!(g.insert_launcher("proj", "p2", PaneEdge::Right, "l1".into()));
        assert!(g.root.contains_launcher());
        assert!(g.remove_launcher("l1"));
        assert_eq!(g.root, before);
        assert!(!g.root.contains_launcher());
    }

    #[test]
    fn launcher_bind_keeps_geometry() {
        let mut g = PaneGroup::new("g".into(), "p1".into(), pane("p1", "s1"));
        g.insert_launcher("proj", "p1", PaneEdge::Right, "l1".into());
        assert!(g.bind_launcher("l1", "s9"));
        let leaf = g.root.leaf("l1").unwrap();
        assert_eq!(leaf.content.session_id(), Some("s9"));
        assert_eq!(g.root.session_leaves().len(), 2);
    }

    #[test]
    fn focus_neighbor_spatial() {
        let mut g = PaneGroup::new("g".into(), "p1".into(), pane("p1", "s1"));
        g.insert_session("s2", "p1", PaneEdge::Right, "p2".into());
        assert_eq!(
            g.focus_neighbor("p1", FocusDirection::Right).as_deref(),
            Some("p2")
        );
        assert_eq!(
            g.focus_neighbor("p2", FocusDirection::Left).as_deref(),
            Some("p1")
        );
        assert_eq!(g.focus_neighbor("p1", FocusDirection::Left), None);
    }

    #[test]
    fn durable_round_trip_strips_launchers() {
        // Contract ids must be canonical lowercase UUIDs (non-UUID ids are
        // replaced at restore); use canonical ids so the round trip is exact.
        let p1 = "00000000-0000-4000-8000-000000000001";
        let p2 = "00000000-0000-4000-8000-000000000002";
        let mut g = PaneGroup::new(
            "11111111-0000-4000-8000-000000000001".into(),
            p1.into(),
            pane(p1, "s1"),
        );
        g.insert_session("s2", p1, PaneEdge::Right, p2.into());
        g.insert_launcher("proj", p2, PaneEdge::Right, "l1".into());
        // Launcher live + snapshot → the snapshot is what gets encoded.
        let encoded = DurableLayout::encode(std::slice::from_ref(&g));
        assert_eq!(encoded.groups.len(), 1);
        let root = &encoded.groups[0].root;
        assert!(!format!("{root:?}").contains("Launcher"));
        let json = serde_json::to_value(&encoded).unwrap();
        let decoded = DurableLayout::decode(&json).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(leaf_ids(&decoded[0].root), vec![p1, p2]);
    }

    #[test]
    fn v1_migration_folds_right_leaning() {
        let v1 = json!({
            "version": 1,
            "groups": [{
                "id": "11111111-0000-4000-8000-000000000001",
                "representativePaneID": "00000000-0000-4000-8000-000000000001",
                "panes": [
                    {"id": "00000000-0000-4000-8000-000000000001", "sessionID": "s1", "fraction": 0.6},
                    {"id": "00000000-0000-4000-8000-000000000002", "sessionID": "s2", "fraction": 0.4}
                ]
            }]
        });
        let groups = DurableLayout::decode(&v1).unwrap();
        assert_eq!(groups.len(), 1);
        match &groups[0].root {
            PaneNode::Split(s) => {
                assert_eq!(s.direction, SplitDirection::Horizontal);
                assert!((s.ratio - 0.6).abs() < 1e-9);
                assert_eq!(
                    leaf_ids(&groups[0].root),
                    vec![
                        "00000000-0000-4000-8000-000000000001",
                        "00000000-0000-4000-8000-000000000002"
                    ]
                );
            }
            _ => panic!("expected split"),
        }
    }

    #[test]
    fn v1_single_pane_group_dropped() {
        let v1 = json!({
            "version": 1,
            "groups": [{
                "id": "g",
                "representativePaneID": "p1",
                "panes": [{"id": "p1", "sessionID": "s1", "fraction": 1.0}]
            }]
        });
        assert!(DurableLayout::decode(&v1).unwrap().is_empty());
    }

    #[test]
    fn reconcile_drops_dead_sessions() {
        let mut g = PaneGroup::new("g".into(), "p1".into(), pane("p1", "s1"));
        g.insert_session("s2", "p1", PaneEdge::Right, "p2".into());
        g.insert_session("s3", "p2", PaneEdge::Right, "p3".into());
        let mut groups = vec![g];
        let eligible: HashSet<String> = ["s1".to_string(), "s3".to_string()].into_iter().collect();
        reconcile(&mut groups, &eligible);
        assert_eq!(groups.len(), 1);
        assert_eq!(leaf_ids(&groups[0].root), vec!["p1", "p3"]);
    }

    #[test]
    fn canonical_id_replaces_non_uuid() {
        let a = canonical_id("not-a-uuid");
        let b = canonical_id("not-a-uuid");
        assert_eq!(a, b);
        assert!(is_uuid(&a));
        assert_eq!(
            canonical_id("AAAAAAAA-0000-4000-8000-000000000001"),
            "aaaaaaaa-0000-4000-8000-000000000001"
        );
    }
}

/// Replays every case in `protocol/pane-layout-operations-v1.json` against
/// the real codec and the layout-level operations. This is the executable
/// form of `PaneLayoutOperationsConformanceTests`.
#[cfg(test)]
mod fixture_tests {
    use super::*;
    use std::collections::HashSet;

    const FIXTURE: &str = include_str!("../../../../protocol/pane-layout-operations-v1.json");

    fn parse_edge(value: &str) -> PaneEdge {
        match value {
            "left" => PaneEdge::Left,
            "right" => PaneEdge::Right,
            "up" => PaneEdge::Up,
            "down" => PaneEdge::Down,
            other => panic!("unknown edge {other}"),
        }
    }

    fn parse_direction(value: &str) -> FocusDirection {
        match value {
            "left" => FocusDirection::Left,
            "right" => FocusDirection::Right,
            "up" => FocusDirection::Up,
            "down" => FocusDirection::Down,
            other => panic!("unknown direction {other}"),
        }
    }

    fn parse_path(value: &serde_json::Value) -> PaneSplitPath {
        let components = value
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|c| match c.as_str() {
                Some("left") => PaneSplitBranch::Left,
                Some("right") => PaneSplitBranch::Right,
                other => panic!("unknown path component {other:?}"),
            })
            .collect();
        PaneSplitPath { components }
    }

    fn str_field(op: &serde_json::Value, name: &str) -> String {
        op.get(name)
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("op missing {name}: {op}"))
            .to_string()
    }

    fn edge_field(op: &serde_json::Value) -> String {
        op.get("edge")
            .or_else(|| op.get("groupEdge"))
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("op missing edge/groupEdge: {op}"))
            .to_string()
    }

    /// Normalize a layout JSON through the real codec so key spellings
    /// (e.g. `representativePaneID`) match the encoder exactly.
    fn normalize_layout(value: &serde_json::Value) -> serde_json::Value {
        let layout: DurableLayout =
            serde_json::from_value(value.clone()).expect("fixture expect.layout must decode");
        serde_json::to_value(&layout).expect("layout must serialize")
    }

    #[test]
    fn fixture_replay_all_cases() {
        let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixture must parse");
        let cases = fixture
            .get("cases")
            .and_then(|c| c.as_array())
            .expect("fixture must have cases");
        assert_eq!(cases.len(), 44, "the contract ships 44 cases");

        for case in cases {
            let id = case
                .get("id")
                .and_then(|v| v.as_str())
                .expect("case must have an id");
            let mut groups = DurableLayout::decode(case.get("initial").expect("initial"))
                .unwrap_or_else(|e| panic!("{id}: initial must decode: {e}"));

            let mut op_error: Option<String> = None;
            let mut focus_result: Option<Option<String>> = None;
            for op in case
                .get("operations")
                .and_then(|o| o.as_array())
                .cloned()
                .unwrap_or_default()
            {
                let op_name = op
                    .get("op")
                    .and_then(|v| v.as_str())
                    .expect("op must have a name");
                let result: Result<Option<String>, PaneOpError> = match op_name {
                    "insertSession" => {
                        let target = if let Some(pane_id) = op.get("targetPaneID") {
                            InsertTarget::Pane {
                                pane_id: pane_id.as_str().unwrap().to_string(),
                            }
                        } else if let Some(group_id) = op.get("groupID") {
                            InsertTarget::GroupEdge {
                                group_id: group_id.as_str().unwrap().to_string(),
                            }
                        } else {
                            InsertTarget::BesideSolo {
                                session_id: str_field(&op, "besideSessionID"),
                                new_group_id: str_field(&op, "newGroupID"),
                                new_representative_pane_id: str_field(
                                    &op,
                                    "newRepresentativePaneID",
                                ),
                            }
                        };
                        layout_insert_session(
                            &mut groups,
                            &str_field(&op, "sessionID"),
                            target,
                            parse_edge(&edge_field(&op)),
                            str_field(&op, "newPaneID"),
                        )
                        .map(|_| None)
                    }
                    "detachPane" => {
                        layout_detach_pane(&mut groups, &str_field(&op, "paneID")).map(|_| None)
                    }
                    "closeGroup" => {
                        layout_close_group(&mut groups, &str_field(&op, "groupID")).map(|_| None)
                    }
                    "resizeSplit" => layout_resize_split(
                        &mut groups,
                        &str_field(&op, "groupID"),
                        &parse_path(op.get("path").unwrap_or(&serde_json::Value::Null)),
                        op.get("ratio").and_then(|v| v.as_f64()).unwrap_or(0.5),
                    )
                    .map(|_| None),
                    "equalize" => {
                        layout_equalize(&mut groups, &str_field(&op, "groupID")).map(|_| None)
                    }
                    "swapPanes" => layout_swap_panes(
                        &mut groups,
                        &str_field(&op, "paneID"),
                        &str_field(&op, "otherPaneID"),
                    )
                    .map(|_| None),
                    "insertLauncher" => layout_insert_launcher(
                        &mut groups,
                        &str_field(&op, "projectID"),
                        &str_field(&op, "targetPaneID"),
                        parse_edge(&edge_field(&op)),
                        str_field(&op, "newPaneID"),
                    )
                    .map(|_| None),
                    "removeLauncher" => {
                        layout_remove_launcher(&mut groups, &str_field(&op, "paneID")).map(|_| None)
                    }
                    "bindLauncher" => layout_bind_launcher(
                        &mut groups,
                        &str_field(&op, "paneID"),
                        &str_field(&op, "sessionID"),
                    )
                    .map(|_| None),
                    "reconcile" => {
                        let eligible: HashSet<String> = op
                            .get("eligibleSessionIDs")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default()
                            .into_iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect();
                        reconcile(&mut groups, &eligible);
                        Ok(None)
                    }
                    "focusNeighbor" => layout_focus_neighbor(
                        &groups,
                        &str_field(&op, "paneID"),
                        parse_direction(&str_field(&op, "direction")),
                    ),
                    other => panic!("{id}: unknown op {other}"),
                };
                match result {
                    Ok(focus) => {
                        if op_name == "focusNeighbor" {
                            focus_result = Some(focus);
                        }
                    }
                    Err(e) => {
                        op_error = Some(e.code().to_string());
                        break;
                    }
                }
            }

            let expect = case.get("expect").expect("expect");
            if let Some(want_error) = expect.get("error").and_then(|v| v.as_str()) {
                assert_eq!(
                    op_error.as_deref(),
                    Some(want_error),
                    "{id}: expected error {want_error}"
                );
                continue;
            }
            assert!(
                op_error.is_none(),
                "{id}: unexpected op error {:?}",
                op_error
            );

            if let Some(want_layout) = expect.get("layout") {
                let actual = serde_json::to_value(DurableLayout::encode(&groups)).expect("encode");
                assert_eq!(
                    actual,
                    normalize_layout(want_layout),
                    "{id}: layout mismatch"
                );
            }
            if expect.get("focusPaneID").is_some() {
                let want: Option<String> = expect
                    .get("focusPaneID")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                assert_eq!(focus_result, Some(want), "{id}: focusPaneID mismatch");
            }
            if let Some(want_leaves) = expect.get("expectLiveLeaves") {
                let group = groups.first().expect("{id}: expected a group");
                let actual: Vec<serde_json::Value> = live_leaves(group)
                    .into_iter()
                    .map(|leaf| match leaf {
                        LiveLeaf::Session {
                            pane_id,
                            session_id,
                        } => {
                            serde_json::json!({"paneID": pane_id, "sessionID": session_id})
                        }
                        LiveLeaf::Launcher {
                            pane_id,
                            project_id,
                        } => {
                            serde_json::json!({"paneID": pane_id, "launcherProjectID": project_id})
                        }
                    })
                    .collect();
                let want: Vec<serde_json::Value> =
                    want_leaves.as_array().cloned().unwrap_or_default();
                assert_eq!(actual, want, "{id}: live leaves mismatch");
            }
        }
    }
}
