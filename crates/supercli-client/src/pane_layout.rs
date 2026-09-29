//! Pane layout state: the recursive split-tree pane model.
//!
//! Port of `clients/legacy/native/SupercliNative/Sources/SupercliNative/PaneLayoutState.swift`.
//! The normative cross-implementation contract — operation semantics, durable
//! schema, v1 migration, and the spatial/equalize algorithms.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

/// Simple rectangle replacing `CGRect` for layout calculations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn min_x(&self) -> f64 {
        self.x
    }
    pub fn min_y(&self) -> f64 {
        self.y
    }
    pub fn max_x(&self) -> f64 {
        self.x + self.width
    }
    pub fn max_y(&self) -> f64 {
        self.y + self.height
    }
}

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneContent {
    Session { id: String },
    Launcher { project_id: String },
}

impl PaneContent {
    pub fn session_id(&self) -> Option<&str> {
        match self {
            PaneContent::Session { id } => Some(id),
            _ => None,
        }
    }

    pub fn is_launcher(&self) -> bool {
        matches!(self, PaneContent::Launcher { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    pub id: String,
    pub content: PaneContent,
}

impl Pane {
    pub fn new(id: impl Into<String>, content: PaneContent) -> Self {
        let id = id.into();
        Self {
            id: PaneStableId::canonical(&id).unwrap_or_else(PaneStableId::make),
            content,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    pub fn new_leaf_is_left_child(&self) -> bool {
        match self {
            PaneEdge::Left | PaneEdge::Up => true,
            PaneEdge::Right | PaneEdge::Down => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneDropTarget {
    Pane { pane_id: String, edge: PaneEdge },
    GroupEdge(PaneEdge),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaneSplitBranch {
    Left,
    Right,
}

/// Addresses a split node inside a group's tree. The empty path is the root.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PaneSplitPath {
    pub components: Vec<PaneSplitBranch>,
}

impl PaneSplitPath {
    pub fn new(components: Vec<PaneSplitBranch>) -> Self {
        Self { components }
    }

    pub fn root() -> Self {
        Self { components: vec![] }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaneSplit {
    pub direction: SplitDirection,
    pub ratio: f64,
    pub left: PaneNode,
    pub right: PaneNode,
}

impl PaneSplit {
    pub fn new(direction: SplitDirection, ratio: f64, left: PaneNode, right: PaneNode) -> Self {
        Self {
            direction,
            ratio: Self::clamped_ratio(ratio),
            left,
            right,
        }
    }

    pub fn clamped_ratio(ratio: f64) -> f64 {
        if !ratio.is_finite() {
            return 0.5;
        }
        ratio
            .max(PaneLayoutState::MINIMUM_SPLIT_RATIO)
            .min(PaneLayoutState::MAXIMUM_SPLIT_RATIO)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaneNode {
    Leaf(Pane),
    Split(Box<PaneSplit>),
}

impl PaneNode {
    /// Leaves in preorder (left-first). Preorder is the normative leaf order.
    pub fn leaves(&self) -> Vec<Pane> {
        match self {
            PaneNode::Leaf(pane) => vec![pane.clone()],
            PaneNode::Split(split) => {
                let mut result = split.left.leaves();
                result.extend(split.right.leaves());
                result
            }
        }
    }

    pub fn session_leaves(&self) -> Vec<Pane> {
        self.leaves()
            .into_iter()
            .filter(|p| p.content.session_id().is_some())
            .collect()
    }

    pub fn contains_launcher(&self) -> bool {
        self.leaves().iter().any(|p| p.content.is_launcher())
    }

    pub fn leaf(&self, pane_id: &str) -> Option<Pane> {
        self.leaves().into_iter().find(|p| p.id == pane_id)
    }

    pub fn path_to_pane(&self, pane_id: &str) -> Option<PaneSplitPath> {
        match self {
            PaneNode::Leaf(pane) => {
                if pane.id == pane_id {
                    Some(PaneSplitPath::root())
                } else {
                    None
                }
            }
            PaneNode::Split(split) => {
                if let Some(mut left) = split.left.path_to_pane(pane_id) {
                    let mut components = vec![PaneSplitBranch::Left];
                    components.append(&mut left.components);
                    return Some(PaneSplitPath::new(components));
                }
                if let Some(mut right) = split.right.path_to_pane(pane_id) {
                    let mut components = vec![PaneSplitBranch::Right];
                    components.append(&mut right.components);
                    return Some(PaneSplitPath::new(components));
                }
                None
            }
        }
    }

    pub fn node_at(&self, path: &PaneSplitPath) -> Option<&PaneNode> {
        let mut current = self;
        for component in &path.components {
            match current {
                PaneNode::Split(split) => {
                    current = match component {
                        PaneSplitBranch::Left => &split.left,
                        PaneSplitBranch::Right => &split.right,
                    };
                }
                _ => return None,
            }
        }
        Some(current)
    }

    pub fn replacing_node_at(&self, path: &PaneSplitPath, with: PaneNode) -> PaneNode {
        let first = match path.components.first() {
            Some(f) => *f,
            None => return with,
        };
        let split = match self {
            PaneNode::Split(s) => s.as_ref().clone(),
            _ => return self.clone(),
        };
        let rest = PaneSplitPath::new(path.components[1..].to_vec());
        let mut updated = split;
        match first {
            PaneSplitBranch::Left => {
                updated.left = updated.left.replacing_node_at(&rest, with);
            }
            PaneSplitBranch::Right => {
                updated.right = updated.right.replacing_node_at(&rest, with);
            }
        }
        PaneNode::Split(Box::new(updated))
    }

    /// Removes a leaf; the surviving sibling replaces its parent split.
    /// Returns None when the tree empties.
    pub fn removing_leaf(&self, pane_id: &str) -> Option<PaneNode> {
        match self {
            PaneNode::Leaf(pane) => {
                if pane.id == pane_id {
                    None
                } else {
                    Some(self.clone())
                }
            }
            PaneNode::Split(split) => {
                // Direct child check
                if let PaneNode::Leaf(pane) = &split.left {
                    if pane.id == pane_id {
                        return Some(split.right.clone());
                    }
                }
                if let PaneNode::Leaf(pane) = &split.right {
                    if pane.id == pane_id {
                        return Some(split.left.clone());
                    }
                }
                let mut updated = split.as_ref().clone();
                match split.left.removing_leaf(pane_id) {
                    Some(left) => updated.left = left,
                    None => return Some(split.right.clone()),
                }
                match split.right.removing_leaf(pane_id) {
                    Some(right) => updated.right = right,
                    None => return Some(split.left.clone()),
                }
                Some(PaneNode::Split(Box::new(updated)))
            }
        }
    }

    pub fn updating_leaf(&self, pane_id: &str, transform: &impl Fn(&mut Pane)) -> PaneNode {
        match self {
            PaneNode::Leaf(pane) => {
                if pane.id != pane_id {
                    return self.clone();
                }
                let mut updated = pane.clone();
                transform(&mut updated);
                PaneNode::Leaf(updated)
            }
            PaneNode::Split(split) => {
                let mut updated = split.as_ref().clone();
                updated.left = updated.left.updating_leaf(pane_id, transform);
                updated.right = updated.right.updating_leaf(pane_id, transform);
                PaneNode::Split(Box::new(updated))
            }
        }
    }

    /// Wraps the target leaf in a 0.5 split with the new leaf on the edge side.
    pub fn splitting_leaf(&self, pane_id: &str, adding: Pane, edge: PaneEdge) -> Option<PaneNode> {
        let path = self.path_to_pane(pane_id)?;
        let target = self.node_at(&path)?.clone();
        let (left, right) = if edge.new_leaf_is_left_child() {
            (PaneNode::Leaf(adding), target)
        } else {
            (target, PaneNode::Leaf(adding))
        };
        let split = PaneSplit::new(edge.split_direction(), 0.5, left, right);
        Some(self.replacing_node_at(&path, PaneNode::Split(Box::new(split))))
    }

    /// Equalizes every split: ratio = leftWeight / totalWeight.
    pub fn equalized(&self) -> PaneNode {
        match self {
            PaneNode::Leaf(_) => self.clone(),
            PaneNode::Split(split) => {
                let left_weight = split.left.weight_for_direction(split.direction);
                let right_weight = split.right.weight_for_direction(split.direction);
                let ratio = left_weight as f64 / (left_weight + right_weight) as f64;
                PaneNode::Split(Box::new(PaneSplit::new(
                    split.direction,
                    ratio,
                    split.left.equalized(),
                    split.right.equalized(),
                )))
            }
        }
    }

    fn weight_for_direction(&self, direction: SplitDirection) -> usize {
        match self {
            PaneNode::Leaf(_) => 1,
            PaneNode::Split(split) => {
                if split.direction != direction {
                    return 1;
                }
                split.left.weight_for_direction(direction)
                    + split.right.weight_for_direction(direction)
            }
        }
    }

    /// Structure-only identity: tree shape, directions, leaf ids, and leaf
    /// content — deliberately excluding ratios.
    pub fn structural_identity(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.hash_structure(&mut hasher);
        hasher.finish()
    }

    fn hash_structure<H: std::hash::Hasher>(&self, hasher: &mut H) {
        match self {
            PaneNode::Leaf(pane) => {
                0u8.hash(hasher);
                pane.id.hash(hasher);
                match &pane.content {
                    PaneContent::Session { id } => id.hash(hasher),
                    PaneContent::Launcher { project_id } => {
                        format!("launcher:{}", project_id).hash(hasher)
                    }
                }
            }
            PaneNode::Split(split) => {
                1u8.hash(hasher);
                (split.direction as u8).hash(hasher);
                split.left.hash_structure(hasher);
                split.right.hash_structure(hasher);
            }
        }
    }

    // MARK: Spatial layout

    /// Grid dimensions: leaf = 1x1; horizontal sums widths, vertical sums heights.
    pub fn grid_dimensions(&self) -> (f64, f64) {
        match self {
            PaneNode::Leaf(_) => (1.0, 1.0),
            PaneNode::Split(split) => {
                let (lw, lh) = split.left.grid_dimensions();
                let (rw, rh) = split.right.grid_dimensions();
                match split.direction {
                    SplitDirection::Horizontal => (lw + rw, lh.max(rh)),
                    SplitDirection::Vertical => (lw.max(rw), lh + rh),
                }
            }
        }
    }

    /// Leaf rects from recursive ratio subdivision of `bounds`.
    pub fn leaf_slots(&self, bounds: Rect) -> Vec<(Pane, Rect)> {
        match self {
            PaneNode::Leaf(pane) => vec![(pane.clone(), bounds)],
            PaneNode::Split(split) => {
                let (left_bounds, right_bounds) = match split.direction {
                    SplitDirection::Horizontal => (
                        Rect::new(
                            bounds.min_x(),
                            bounds.min_y(),
                            bounds.width * split.ratio,
                            bounds.height,
                        ),
                        Rect::new(
                            bounds.min_x() + bounds.width * split.ratio,
                            bounds.min_y(),
                            bounds.width * (1.0 - split.ratio),
                            bounds.height,
                        ),
                    ),
                    SplitDirection::Vertical => (
                        Rect::new(
                            bounds.min_x(),
                            bounds.min_y(),
                            bounds.width,
                            bounds.height * split.ratio,
                        ),
                        Rect::new(
                            bounds.min_x(),
                            bounds.min_y() + bounds.height * split.ratio,
                            bounds.width,
                            bounds.height * (1.0 - split.ratio),
                        ),
                    ),
                };
                let mut result = split.left.leaf_slots(left_bounds);
                result.extend(split.right.leaf_slots(right_bounds));
                result
            }
        }
    }

    /// The neighboring leaf in a direction, using grid-dimension bounds.
    pub fn spatial_neighbor(&self, pane_id: &str, direction: PaneEdge) -> Option<Pane> {
        let (w, h) = self.grid_dimensions();
        let slots = self.leaf_slots(Rect::new(0.0, 0.0, w, h));
        let reference = slots.iter().find(|(p, _)| p.id == pane_id)?;
        let mut best: Option<(Pane, f64)> = None;
        for (pane, bounds) in &slots {
            if pane.id == pane_id {
                continue;
            }
            let qualifies = match direction {
                PaneEdge::Left => bounds.max_x() <= reference.1.min_x(),
                PaneEdge::Right => bounds.min_x() >= reference.1.max_x(),
                PaneEdge::Up => bounds.max_y() <= reference.1.min_y(),
                PaneEdge::Down => bounds.min_y() >= reference.1.max_y(),
            };
            if !qualifies {
                continue;
            }
            let dx = bounds.min_x() - reference.1.min_x();
            let dy = bounds.min_y() - reference.1.min_y();
            let distance = (dx * dx + dy * dy).sqrt();
            match &best {
                Some((_, d)) if distance >= *d => {}
                _ => best = Some((pane.clone(), distance)),
            }
        }
        best.map(|(p, _)| p)
    }
}

// ---------------------------------------------------------------------------
// PaneGroup
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct PaneGroup {
    pub id: String,
    pub representative_pane_id: String,
    pub root: PaneNode,
    /// The exact tree from before a transient launcher was inserted.
    /// Presentation-only; never durable.
    pub pre_launcher_root: Option<PaneNode>,
}

impl PaneGroup {
    pub fn new(
        id: impl Into<String>,
        representative_pane_id: impl Into<String>,
        root: PaneNode,
    ) -> Self {
        let id = id.into();
        let rep = representative_pane_id.into();
        Self {
            id: PaneStableId::canonical(&id).unwrap_or_else(PaneStableId::make),
            representative_pane_id: PaneStableId::canonical(&rep).unwrap_or(rep),
            root,
            pre_launcher_root: None,
        }
    }

    pub fn panes(&self) -> Vec<Pane> {
        self.root.leaves()
    }

    pub fn session_ids(&self) -> Vec<String> {
        self.root
            .session_leaves()
            .iter()
            .filter_map(|p| p.content.session_id().map(|s| s.to_string()))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// PaneLocation, PaneLayoutChange, PaneLayoutError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneLocation {
    pub group_id: String,
    pub pane_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneLayoutChange {
    pub group_id: String,
    pub removed_pane_ids: Vec<String>,
    pub released_session_ids: Vec<String>,
    pub representative_pane_id: Option<String>,
    pub dissolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PaneLayoutError {
    #[error("invalid session id")]
    InvalidSessionId,
    #[error("same session")]
    SameSession,
    #[error("duplicate session: {0}")]
    DuplicateSession(String),
    #[error("pane not found: {0}")]
    PaneNotFound(String),
    #[error("group not found: {0}")]
    GroupNotFound(String),
    #[error("split not found in group: {0}")]
    SplitNotFound(String),
    #[error("invalid ratio")]
    InvalidRatio,
    #[error("launcher already present in group: {0}")]
    LauncherAlreadyPresent(String),
    #[error("pane is not a launcher: {0}")]
    PaneIsNotLauncher(String),
    #[error("panes belong to different groups")]
    PanesBelongToDifferentGroups,
    #[error("group at capacity")]
    GroupAtCapacity,
}

// ---------------------------------------------------------------------------
// PaneLayoutState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct PaneLayoutState {
    pub groups: Vec<PaneGroup>,
}

impl PaneLayoutState {
    pub const MAXIMUM_SESSION_LEAF_COUNT: usize = 8;
    pub const MINIMUM_SPLIT_RATIO: f64 = 0.1;
    pub const MAXIMUM_SPLIT_RATIO: f64 = 0.9;

    pub fn new(groups: Vec<PaneGroup>) -> Self {
        let mut state = Self { groups: vec![] };
        state.canonicalize(groups);
        state
    }

    fn canonicalize(&mut self, groups: Vec<PaneGroup>) {
        // The Swift init runs canonicalize(groups); preserve insertion order.
        self.groups = groups;
    }

    fn index_of_group(&self, group_id: &str) -> Result<usize, PaneLayoutError> {
        self.groups
            .iter()
            .position(|g| g.id == group_id)
            .ok_or_else(|| PaneLayoutError::GroupNotFound(group_id.to_string()))
    }

    fn validate_new_session(&self, session_id: &str) -> Result<(), PaneLayoutError> {
        if session_id.is_empty() {
            return Err(PaneLayoutError::InvalidSessionId);
        }
        if self.location_of_session(session_id).is_some() {
            return Err(PaneLayoutError::DuplicateSession(session_id.to_string()));
        }
        Ok(())
    }

    fn validate_capacity(&self, group_index: usize) -> Result<(), PaneLayoutError> {
        if self.groups[group_index].root.session_leaves().len() >= Self::MAXIMUM_SESSION_LEAF_COUNT
        {
            return Err(PaneLayoutError::GroupAtCapacity);
        }
        Ok(())
    }

    // MARK: Queries

    pub fn group_containing_session(&self, session_id: &str) -> Option<&PaneGroup> {
        let location = self.location_of_session(session_id)?;
        self.groups.iter().find(|g| g.id == location.group_id)
    }

    pub fn group(&self, group_id: &str) -> Option<&PaneGroup> {
        self.groups.iter().find(|g| g.id == group_id)
    }

    pub fn pane(&self, pane_id: &str) -> Option<Pane> {
        for group in &self.groups {
            if let Some(pane) = group.root.leaf(pane_id) {
                return Some(pane);
            }
        }
        None
    }

    pub fn location_of_session(&self, session_id: &str) -> Option<PaneLocation> {
        for group in &self.groups {
            if let Some(pane) = group
                .root
                .leaves()
                .into_iter()
                .find(|p| p.content.session_id() == Some(session_id))
            {
                return Some(PaneLocation {
                    group_id: group.id.clone(),
                    pane_id: pane.id,
                });
            }
        }
        None
    }

    pub fn location_of_pane(&self, pane_id: &str) -> Option<PaneLocation> {
        for group in &self.groups {
            if group.root.leaf(pane_id).is_some() {
                return Some(PaneLocation {
                    group_id: group.id.clone(),
                    pane_id: pane_id.to_string(),
                });
            }
        }
        None
    }

    pub fn spatial_neighbor(&self, pane_id: &str, direction: PaneEdge) -> Option<Pane> {
        let location = self.location_of_pane(pane_id)?;
        let group = self.group(&location.group_id)?;
        group.root.spatial_neighbor(pane_id, direction)
    }

    // MARK: Insert

    #[allow(clippy::too_many_arguments)]
    pub fn create_group(
        &mut self,
        representative_session_id: &str,
        session_id: &str,
        edge: PaneEdge,
        new_group_id: Option<String>,
        new_representative_pane_id: Option<String>,
        new_pane_id: Option<String>,
    ) -> Result<PaneLocation, PaneLayoutError> {
        self.validate_new_session(session_id)?;
        if representative_session_id.is_empty() {
            return Err(PaneLayoutError::InvalidSessionId);
        }
        if representative_session_id == session_id {
            return Err(PaneLayoutError::SameSession);
        }
        if self
            .location_of_session(representative_session_id)
            .is_some()
        {
            return Err(PaneLayoutError::DuplicateSession(
                representative_session_id.to_string(),
            ));
        }

        let representative = Pane::new(
            new_representative_pane_id.unwrap_or_else(PaneStableId::make),
            PaneContent::Session {
                id: representative_session_id.to_string(),
            },
        );
        let inserted = Pane::new(
            new_pane_id.unwrap_or_else(PaneStableId::make),
            PaneContent::Session {
                id: session_id.to_string(),
            },
        );
        let (left, right) = if edge.new_leaf_is_left_child() {
            (
                PaneNode::Leaf(inserted.clone()),
                PaneNode::Leaf(representative.clone()),
            )
        } else {
            (
                PaneNode::Leaf(representative.clone()),
                PaneNode::Leaf(inserted.clone()),
            )
        };
        let split = PaneSplit::new(edge.split_direction(), 0.5, left, right);
        let group = PaneGroup::new(
            new_group_id.unwrap_or_else(PaneStableId::make),
            representative.id.clone(),
            PaneNode::Split(Box::new(split)),
        );
        let location = PaneLocation {
            group_id: group.id.clone(),
            pane_id: inserted.id.clone(),
        };
        self.groups.push(group);
        Ok(location)
    }

    /// Splits the target session's leaf, or creates a group when that session
    /// is currently solo.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_session(
        &mut self,
        session_id: &str,
        target_session_id: &str,
        edge: PaneEdge,
        new_group_id: Option<String>,
        new_representative_pane_id: Option<String>,
        new_pane_id: Option<String>,
    ) -> Result<PaneLocation, PaneLayoutError> {
        self.validate_new_session(session_id)?;
        if target_session_id.is_empty() {
            return Err(PaneLayoutError::InvalidSessionId);
        }
        if session_id == target_session_id {
            return Err(PaneLayoutError::SameSession);
        }

        if let Some(target) = self.location_of_session(target_session_id) {
            return self.insert_session_splitting(session_id, &target.pane_id, edge, new_pane_id);
        }
        self.create_group(
            target_session_id,
            session_id,
            edge,
            new_group_id,
            new_representative_pane_id,
            new_pane_id,
        )
    }

    pub fn insert_session_splitting(
        &mut self,
        session_id: &str,
        target_pane_id: &str,
        edge: PaneEdge,
        new_pane_id: Option<String>,
    ) -> Result<PaneLocation, PaneLayoutError> {
        self.validate_new_session(session_id)?;
        let location = self
            .location_of_pane(target_pane_id)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(target_pane_id.to_string()))?;
        let group_index = self.index_of_group(&location.group_id)?;
        self.validate_capacity(group_index)?;

        let pane = Pane::new(
            new_pane_id.unwrap_or_else(PaneStableId::make),
            PaneContent::Session {
                id: session_id.to_string(),
            },
        );
        let pane_id = pane.id.clone();
        let root = self.groups[group_index]
            .root
            .splitting_leaf(target_pane_id, pane, edge)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(target_pane_id.to_string()))?;
        self.groups[group_index].pre_launcher_root = None;
        self.groups[group_index].root = root;
        Ok(PaneLocation {
            group_id: self.groups[group_index].id.clone(),
            pane_id,
        })
    }

    /// Splits the group's root; the new leaf's share is 1/(sessionLeafCount+1).
    pub fn insert_session_at_group_edge(
        &mut self,
        session_id: &str,
        edge: PaneEdge,
        group_id: &str,
        new_pane_id: Option<String>,
    ) -> Result<PaneLocation, PaneLayoutError> {
        self.validate_new_session(session_id)?;
        let group_index = self.index_of_group(group_id)?;
        self.validate_capacity(group_index)?;

        let pane = Pane::new(
            new_pane_id.unwrap_or_else(PaneStableId::make),
            PaneContent::Session {
                id: session_id.to_string(),
            },
        );
        let pane_id = pane.id.clone();
        let share = 1.0 / (self.groups[group_index].root.session_leaves().len() + 1) as f64;
        let root = self.groups[group_index].root.clone();
        let (left, right) = if edge.new_leaf_is_left_child() {
            (PaneNode::Leaf(pane), root)
        } else {
            (root, PaneNode::Leaf(pane))
        };
        let split = PaneSplit::new(
            edge.split_direction(),
            if edge.new_leaf_is_left_child() {
                share
            } else {
                1.0 - share
            },
            left,
            right,
        );
        self.groups[group_index].pre_launcher_root = None;
        self.groups[group_index].root = PaneNode::Split(Box::new(split));
        Ok(PaneLocation {
            group_id: self.groups[group_index].id.clone(),
            pane_id,
        })
    }

    // MARK: Launcher lifecycle

    pub fn insert_launcher(
        &mut self,
        project_id: &str,
        target_pane_id: &str,
        edge: PaneEdge,
        new_pane_id: Option<String>,
    ) -> Result<PaneLocation, PaneLayoutError> {
        let location = self
            .location_of_pane(target_pane_id)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(target_pane_id.to_string()))?;
        let group_index = self.index_of_group(&location.group_id)?;
        self.validate_capacity(group_index)?;
        if self.groups[group_index].root.contains_launcher() {
            return Err(PaneLayoutError::LauncherAlreadyPresent(
                location.group_id.clone(),
            ));
        }

        let launcher = Pane::new(
            new_pane_id.unwrap_or_else(PaneStableId::make),
            PaneContent::Launcher {
                project_id: project_id.to_string(),
            },
        );
        let pane_id = launcher.id.clone();
        let snapshot = self.groups[group_index].root.clone();
        let root = snapshot
            .splitting_leaf(target_pane_id, launcher, edge)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(target_pane_id.to_string()))?;
        self.groups[group_index].pre_launcher_root = Some(snapshot);
        self.groups[group_index].root = root;
        Ok(PaneLocation {
            group_id: self.groups[group_index].id.clone(),
            pane_id,
        })
    }

    /// Splits the target session's leaf, or creates a session + launcher group
    /// when that session is currently solo.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_launcher_beside(
        &mut self,
        project_id: &str,
        target_session_id: &str,
        edge: PaneEdge,
        new_group_id: Option<String>,
        new_representative_pane_id: Option<String>,
        new_pane_id: Option<String>,
    ) -> Result<PaneLocation, PaneLayoutError> {
        if target_session_id.is_empty() {
            return Err(PaneLayoutError::InvalidSessionId);
        }

        if let Some(target) = self.location_of_session(target_session_id) {
            return self.insert_launcher(project_id, &target.pane_id, edge, new_pane_id);
        }

        let representative = Pane::new(
            new_representative_pane_id.unwrap_or_else(PaneStableId::make),
            PaneContent::Session {
                id: target_session_id.to_string(),
            },
        );
        let launcher = Pane::new(
            new_pane_id.unwrap_or_else(PaneStableId::make),
            PaneContent::Launcher {
                project_id: project_id.to_string(),
            },
        );
        let (left, right) = if edge.new_leaf_is_left_child() {
            (
                PaneNode::Leaf(launcher.clone()),
                PaneNode::Leaf(representative.clone()),
            )
        } else {
            (
                PaneNode::Leaf(representative.clone()),
                PaneNode::Leaf(launcher.clone()),
            )
        };
        let split = PaneSplit::new(edge.split_direction(), 0.5, left, right);
        let group = PaneGroup::new(
            new_group_id.unwrap_or_else(PaneStableId::make),
            representative.id.clone(),
            PaneNode::Split(Box::new(split)),
        );
        let location = PaneLocation {
            group_id: group.id.clone(),
            pane_id: launcher.id.clone(),
        };
        self.groups.push(group);
        Ok(location)
    }

    pub fn bind_launcher(
        &mut self,
        pane_id: &str,
        to_session_id: &str,
    ) -> Result<(), PaneLayoutError> {
        self.validate_new_session(to_session_id)?;
        let location = self
            .location_of_pane(pane_id)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(pane_id.to_string()))?;
        let group_index = self.index_of_group(&location.group_id)?;
        let is_launcher = self.groups[group_index]
            .root
            .leaf(pane_id)
            .map(|p| p.content.is_launcher())
            .unwrap_or(false);
        if !is_launcher {
            return Err(PaneLayoutError::PaneIsNotLauncher(pane_id.to_string()));
        }
        let session_id = to_session_id.to_string();
        self.groups[group_index].root =
            self.groups[group_index].root.updating_leaf(pane_id, &|p| {
                p.content = PaneContent::Session {
                    id: session_id.clone(),
                };
            });
        self.groups[group_index].pre_launcher_root = None;
        Ok(())
    }

    pub fn remove_launcher(&mut self, pane_id: &str) -> Result<PaneLayoutChange, PaneLayoutError> {
        let location = self
            .location_of_pane(pane_id)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(pane_id.to_string()))?;
        let group_index = self.index_of_group(&location.group_id)?;
        let is_launcher = self.groups[group_index]
            .root
            .leaf(pane_id)
            .map(|p| p.content.is_launcher())
            .unwrap_or(false);
        if !is_launcher {
            return Err(PaneLayoutError::PaneIsNotLauncher(pane_id.to_string()));
        }
        self.detach_pane(pane_id)
    }

    // MARK: Detach / close

    pub fn detach_pane(&mut self, pane_id: &str) -> Result<PaneLayoutChange, PaneLayoutError> {
        let location = self
            .location_of_pane(pane_id)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(pane_id.to_string()))?;
        let group_index = self.index_of_group(&location.group_id)?;
        let original = self.groups[group_index].clone();
        let removed_pane = original
            .root
            .leaf(pane_id)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(pane_id.to_string()))?;

        let new_root = match original.root.removing_leaf(pane_id) {
            Some(root) => root,
            None => {
                let group = self.groups.remove(group_index);
                return Ok(PaneLayoutChange {
                    group_id: original.id,
                    removed_pane_ids: group.panes().iter().map(|p| p.id.clone()).collect(),
                    released_session_ids: group.session_ids(),
                    representative_pane_id: None,
                    dissolved: true,
                });
            }
        };

        let session_leaves = new_root.session_leaves();
        let can_remain_grouped = session_leaves.len() >= 2
            || (session_leaves.len() == 1 && new_root.contains_launcher());
        if !can_remain_grouped {
            let group = self.groups.remove(group_index);
            return Ok(PaneLayoutChange {
                group_id: original.id,
                removed_pane_ids: group.panes().iter().map(|p| p.id.clone()).collect(),
                released_session_ids: group.session_ids(),
                representative_pane_id: None,
                dissolved: true,
            });
        }

        let mut updated = original.clone();
        if removed_pane.content.is_launcher() {
            if let Some(snapshot) = &updated.pre_launcher_root {
                let snapshot_ids: HashSet<String> = snapshot
                    .session_leaves()
                    .iter()
                    .map(|p| p.id.clone())
                    .collect();
                let live_ids: HashSet<String> =
                    session_leaves.iter().map(|p| p.id.clone()).collect();
                if snapshot_ids == live_ids {
                    updated.root = snapshot.clone();
                } else {
                    updated.root = new_root;
                }
            } else {
                updated.root = new_root;
            }
            updated.pre_launcher_root = None;
        } else {
            updated.root = new_root;
            if let Some(snapshot) = updated.pre_launcher_root.clone() {
                let pruned = snapshot.removing_leaf(pane_id);
                let count = pruned
                    .as_ref()
                    .map(|n| n.session_leaves().len())
                    .unwrap_or(0);
                updated.pre_launcher_root = if count >= 2 { pruned } else { None };
            }
        }
        // Promote representative if it lost its session.
        let rep_has_session = updated
            .root
            .leaf(&updated.representative_pane_id)
            .and_then(|p| p.content.session_id().map(|_| ()))
            .is_some();
        if !rep_has_session {
            updated.representative_pane_id = updated.root.session_leaves()[0].id.clone();
        }
        self.groups[group_index] = updated.clone();
        Ok(PaneLayoutChange {
            group_id: original.id,
            removed_pane_ids: vec![pane_id.to_string()],
            released_session_ids: removed_pane
                .content
                .session_id()
                .map(|s| vec![s.to_string()])
                .unwrap_or_default(),
            representative_pane_id: Some(updated.representative_pane_id),
            dissolved: false,
        })
    }

    pub fn close_group(&mut self, group_id: &str) -> Result<PaneLayoutChange, PaneLayoutError> {
        let group_index = self.index_of_group(group_id)?;
        let group = self.groups.remove(group_index);
        Ok(PaneLayoutChange {
            group_id: group.id.clone(),
            removed_pane_ids: group.panes().iter().map(|p| p.id.clone()).collect(),
            released_session_ids: group.session_ids(),
            representative_pane_id: None,
            dissolved: true,
        })
    }

    // MARK: Geometry

    pub fn resize_split(
        &mut self,
        group_id: &str,
        path: &PaneSplitPath,
        ratio: f64,
    ) -> Result<f64, PaneLayoutError> {
        if !ratio.is_finite() {
            return Err(PaneLayoutError::InvalidRatio);
        }
        let group_index = self.index_of_group(group_id)?;
        let node = self.groups[group_index]
            .root
            .node_at(path)
            .ok_or_else(|| PaneLayoutError::SplitNotFound(group_id.to_string()))?;
        let mut split = match node {
            PaneNode::Split(s) => s.as_ref().clone(),
            _ => return Err(PaneLayoutError::SplitNotFound(group_id.to_string())),
        };
        let applied = PaneSplit::clamped_ratio(ratio);
        if split.ratio == applied {
            return Ok(applied);
        }
        split.ratio = applied;
        self.groups[group_index].pre_launcher_root = None;
        let new_root = self.groups[group_index]
            .root
            .replacing_node_at(path, PaneNode::Split(Box::new(split)));
        self.groups[group_index].root = new_root;
        Ok(applied)
    }

    pub fn equalize(&mut self, group_id: &str) -> Result<(), PaneLayoutError> {
        let group_index = self.index_of_group(group_id)?;
        self.groups[group_index].pre_launcher_root = None;
        let root = self.groups[group_index].root.equalized();
        self.groups[group_index].root = root;
        Ok(())
    }

    /// Exchanges the positions of two leaves in the same group.
    pub fn swap_panes(
        &mut self,
        pane_id: &str,
        other_pane_id: &str,
    ) -> Result<bool, PaneLayoutError> {
        let source = self
            .location_of_pane(pane_id)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(pane_id.to_string()))?;
        let target = self
            .location_of_pane(other_pane_id)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(other_pane_id.to_string()))?;
        if source.group_id != target.group_id {
            return Err(PaneLayoutError::PanesBelongToDifferentGroups);
        }
        if pane_id == other_pane_id {
            return Ok(false);
        }

        let group_index = self.index_of_group(&source.group_id)?;
        let root = self.groups[group_index].root.clone();
        let first = root
            .leaf(pane_id)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(pane_id.to_string()))?;
        let second = root
            .leaf(other_pane_id)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(other_pane_id.to_string()))?;
        let first_path = root
            .path_to_pane(pane_id)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(pane_id.to_string()))?;
        let second_path = root
            .path_to_pane(other_pane_id)
            .ok_or_else(|| PaneLayoutError::PaneNotFound(other_pane_id.to_string()))?;
        self.groups[group_index].pre_launcher_root = None;
        let new_root = root
            .replacing_node_at(&first_path, PaneNode::Leaf(second))
            .replacing_node_at(&second_path, PaneNode::Leaf(first));
        self.groups[group_index].root = new_root;
        Ok(true)
    }

    // MARK: Reconcile

    /// Drops sessions no longer eligible, collapses around them, promotes the
    /// first remaining session leaf when the representative disappears, and
    /// dissolves non-transient groups with fewer than two sessions.
    pub fn reconcile(&mut self, eligible_session_ids: &HashSet<String>) -> Vec<PaneLayoutChange> {
        let mut reconciled: Vec<PaneGroup> = Vec::new();
        let mut changes: Vec<PaneLayoutChange> = Vec::new();

        for original in self.groups.clone() {
            let ineligible: Vec<Pane> = original
                .root
                .session_leaves()
                .into_iter()
                .filter(|pane| match &pane.content {
                    PaneContent::Session { id } => !eligible_session_ids.contains(id),
                    _ => false,
                })
                .collect();
            let mut new_root: Option<PaneNode> = Some(original.root.clone());
            for pane in &ineligible {
                new_root = new_root.and_then(|r| r.removing_leaf(&pane.id));
            }

            let session_leaves = new_root
                .as_ref()
                .map(|r| r.session_leaves())
                .unwrap_or_default();
            let has_launcher = new_root
                .as_ref()
                .map(|r| r.contains_launcher())
                .unwrap_or(false);
            let can_remain_grouped =
                session_leaves.len() >= 2 || (session_leaves.len() == 1 && has_launcher);

            let new_root = match new_root {
                Some(r) if can_remain_grouped => r,
                _ => {
                    changes.push(PaneLayoutChange {
                        group_id: original.id.clone(),
                        removed_pane_ids: original.panes().iter().map(|p| p.id.clone()).collect(),
                        released_session_ids: original.session_ids(),
                        representative_pane_id: None,
                        dissolved: true,
                    });
                    continue;
                }
            };

            let mut updated = original.clone();
            updated.root = new_root;
            let rep_has_session = updated
                .root
                .leaf(&updated.representative_pane_id)
                .and_then(|p| p.content.session_id().map(|_| ()))
                .is_some();
            if !rep_has_session {
                updated.representative_pane_id = updated.root.session_leaves()[0].id.clone();
            }
            if updated.pre_launcher_root.is_some() && has_launcher {
                let mut pruned = updated.pre_launcher_root.clone();
                for pane in &ineligible {
                    pruned = pruned.and_then(|r| r.removing_leaf(&pane.id));
                }
                let pruned_ids: HashSet<String> = pruned
                    .as_ref()
                    .map(|r| r.session_leaves().iter().map(|p| p.id.clone()).collect())
                    .unwrap_or_default();
                let live_ids: HashSet<String> = updated
                    .root
                    .session_leaves()
                    .iter()
                    .map(|p| p.id.clone())
                    .collect();
                let count = pruned
                    .as_ref()
                    .map(|r| r.session_leaves().len())
                    .unwrap_or(0);
                updated.pre_launcher_root = if count >= 2 && pruned_ids == live_ids {
                    pruned
                } else {
                    None
                };
            } else {
                updated.pre_launcher_root = None;
            }
            let retained_ids: HashSet<String> =
                updated.panes().iter().map(|p| p.id.clone()).collect();
            if updated != original {
                changes.push(PaneLayoutChange {
                    group_id: original.id.clone(),
                    removed_pane_ids: original
                        .panes()
                        .iter()
                        .map(|p| p.id.clone())
                        .filter(|id| !retained_ids.contains(id))
                        .collect(),
                    released_session_ids: original
                        .session_ids()
                        .into_iter()
                        .filter(|id| !eligible_session_ids.contains(id))
                        .collect(),
                    representative_pane_id: Some(updated.representative_pane_id.clone()),
                    dissolved: false,
                });
            }
            reconciled.push(updated);
        }

        self.groups = reconciled;
        changes
    }
}

// ---------------------------------------------------------------------------
// Durable (Codable) types
// ---------------------------------------------------------------------------

/// Host reads it so a pinned App's "the chat next to me" resolves to what
/// the user is looking at. Arrangement only — never focus, never geometry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableSidebarProjection {
    #[serde(default)]
    pub session_ids: Vec<String>,
    #[serde(default)]
    pub beside_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DurablePaneLayout {
    pub version: i32,
    pub groups: Vec<DurablePaneGroup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar: Option<DurableSidebarProjection>,
}

impl DurablePaneLayout {
    pub const CURRENT_VERSION: i32 = 2;

    pub fn new(state: &PaneLayoutState, sidebar: Option<DurableSidebarProjection>) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            groups: state
                .groups
                .iter()
                .filter_map(DurablePaneGroup::from_group)
                .collect(),
            sidebar,
        }
    }

    pub fn restored_state(&self) -> PaneLayoutState {
        PaneLayoutState::new(
            self.groups
                .iter()
                .map(|durable| {
                    PaneGroup::new(
                        durable.id.clone(),
                        durable.representative_pane_id.clone(),
                        durable.root.pane_node(),
                    )
                })
                .collect(),
        )
    }
}

// Custom deserialization for version migration.
impl<'de> Deserialize<'de> for DurablePaneLayout {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            version: i32,
            #[serde(default)]
            groups: serde_json::Value,
            #[serde(default)]
            sidebar: Option<DurableSidebarProjection>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let groups = match raw.version {
            2 => serde_json::from_value::<Vec<DurablePaneGroup>>(raw.groups)
                .map_err(serde::de::Error::custom)?,
            1 => {
                let legacy = serde_json::from_value::<Vec<LegacyDurablePaneGroup>>(raw.groups)
                    .map_err(serde::de::Error::custom)?;
                legacy.into_iter().filter_map(|g| g.migrated()).collect()
            }
            _ => Vec::new(),
        };
        Ok(Self {
            version: raw.version,
            groups,
            sidebar: raw.sidebar,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DurablePaneGroup {
    pub id: String,
    pub representative_pane_id: String,
    pub root: DurablePaneNode,
}

impl DurablePaneGroup {
    pub fn from_group(group: &PaneGroup) -> Option<Self> {
        let candidate: PaneNode = if group.root.contains_launcher() {
            group.pre_launcher_root.clone()?
        } else {
            let mut candidate = group.root.clone();
            for pane in group.root.leaves() {
                if pane.content.is_launcher() {
                    candidate = candidate.removing_leaf(&pane.id)?;
                }
            }
            candidate
        };
        if candidate.session_leaves().len() < 2 {
            return None;
        }
        let representative_pane_id = if candidate.leaf(&group.representative_pane_id).is_some() {
            group.representative_pane_id.clone()
        } else {
            candidate.session_leaves()[0].id.clone()
        };
        Some(Self {
            id: group.id.clone(),
            representative_pane_id,
            root: DurablePaneNode::from(&candidate),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DurablePaneNode {
    Pane {
        id: String,
        session_id: String,
    },
    Split {
        direction: SplitDirection,
        ratio: f64,
        left: Box<DurablePaneNode>,
        right: Box<DurablePaneNode>,
    },
}

impl DurablePaneNode {
    pub fn pane_node(&self) -> PaneNode {
        match self {
            DurablePaneNode::Pane { id, session_id } => PaneNode::Leaf(Pane::new(
                id.clone(),
                PaneContent::Session {
                    id: session_id.clone(),
                },
            )),
            DurablePaneNode::Split {
                direction,
                ratio,
                left,
                right,
            } => PaneNode::Split(Box::new(PaneSplit::new(
                *direction,
                *ratio,
                left.pane_node(),
                right.pane_node(),
            ))),
        }
    }
}

impl From<&PaneNode> for DurablePaneNode {
    fn from(node: &PaneNode) -> Self {
        match node {
            PaneNode::Leaf(pane) => DurablePaneNode::Pane {
                id: pane.id.clone(),
                session_id: pane.content.session_id().unwrap_or("").to_string(),
            },
            PaneNode::Split(split) => DurablePaneNode::Split {
                direction: split.direction,
                ratio: split.ratio,
                left: Box::new(DurablePaneNode::from(&split.left)),
                right: Box::new(DurablePaneNode::from(&split.right)),
            },
        }
    }
}

// Serde for DurablePaneNode with {pane: {...}} / {split: {...}} shape.
impl Serialize for DurablePaneNode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            DurablePaneNode::Pane { id, session_id } => {
                map.serialize_entry(
                    "pane",
                    &serde_json::json!({ "id": id, "sessionID": session_id }),
                )?;
            }
            DurablePaneNode::Split {
                direction,
                ratio,
                left,
                right,
            } => {
                let dir = match direction {
                    SplitDirection::Horizontal => "horizontal",
                    SplitDirection::Vertical => "vertical",
                };
                map.serialize_entry(
                    "split",
                    &serde_json::json!({
                        "direction": dir,
                        "ratio": ratio,
                        "left": left,
                        "right": right,
                    }),
                )?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for DurablePaneNode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        if let Some(pane) = value.get("pane") {
            let id = pane
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| serde::de::Error::missing_field("id"))?;
            let session_id = pane
                .get("sessionID")
                .and_then(|v| v.as_str())
                .ok_or_else(|| serde::de::Error::missing_field("sessionID"))?;
            return Ok(DurablePaneNode::Pane {
                id: id.to_string(),
                session_id: session_id.to_string(),
            });
        }
        let split = value
            .get("split")
            .ok_or_else(|| serde::de::Error::missing_field("split"))?;
        let direction_str = split
            .get("direction")
            .and_then(|v| v.as_str())
            .ok_or_else(|| serde::de::Error::missing_field("direction"))?;
        let direction = match direction_str {
            "horizontal" => SplitDirection::Horizontal,
            "vertical" => SplitDirection::Vertical,
            other => {
                return Err(serde::de::Error::custom(format!(
                    "Unknown split direction {}",
                    other
                )))
            }
        };
        let ratio = split
            .get("ratio")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| serde::de::Error::missing_field("ratio"))?;
        let left: Box<DurablePaneNode> = Box::new(
            serde_json::from_value(
                split
                    .get("left")
                    .ok_or_else(|| serde::de::Error::missing_field("left"))?
                    .clone(),
            )
            .map_err(serde::de::Error::custom)?,
        );
        let right: Box<DurablePaneNode> = Box::new(
            serde_json::from_value(
                split
                    .get("right")
                    .ok_or_else(|| serde::de::Error::missing_field("right"))?
                    .clone(),
            )
            .map_err(serde::de::Error::custom)?,
        );
        Ok(DurablePaneNode::Split {
            direction,
            ratio,
            left,
            right,
        })
    }
}

/// The version-1 flat shape, decoded only for migration. Accepts both the
/// Swift (`sessionID`/`representativePaneID`) and legacy Rust
/// (`sessionId`/`representativePaneId`) key spellings.
#[derive(Debug, Deserialize)]
struct LegacyDurablePaneGroup {
    id: String,
    #[serde(alias = "representativePaneId", alias = "representativePaneID")]
    representative_pane_id: String,
    panes: Vec<LegacyDurablePane>,
}

impl LegacyDurablePaneGroup {
    /// Folds the flat pane list into a right-leaning horizontal chain.
    fn migrated(&self) -> Option<DurablePaneGroup> {
        if self.panes.len() < 2 {
            return None;
        }
        fn fold(panes: &[LegacyDurablePane], index: usize) -> DurablePaneNode {
            let pane = &panes[index];
            let leaf = DurablePaneNode::Pane {
                id: pane.id.clone(),
                session_id: pane.session_id.clone(),
            };
            if index >= panes.len() - 1 {
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
                PaneSplit::clamped_ratio(fraction / remaining)
            } else {
                0.5
            };
            DurablePaneNode::Split {
                direction: SplitDirection::Horizontal,
                ratio,
                left: Box::new(leaf),
                right: Box::new(fold(panes, index + 1)),
            }
        }
        Some(DurablePaneGroup {
            id: self.id.clone(),
            representative_pane_id: self.representative_pane_id.clone(),
            root: fold(&self.panes, 0),
        })
    }
}

#[derive(Debug, Deserialize)]
struct LegacyDurablePane {
    id: String,
    #[serde(alias = "sessionId", alias = "sessionID")]
    session_id: String,
    fraction: f64,
}

// ---------------------------------------------------------------------------
// PaneStableId
// ---------------------------------------------------------------------------

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct PaneStableId;

impl PaneStableId {
    /// Generates a unique id. The Swift implementation uses UUID v4; without a
    /// UUID crate in the web-safe subset we use a monotonic counter prefixed
    /// to avoid collisions with caller-supplied UUIDs.
    pub fn make() -> String {
        let n = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("pane-{:016x}", n)
    }

    /// Returns the lowercased UUID when `value` parses as one, else None.
    pub fn canonical(value: &str) -> Option<String> {
        let v = value.trim();
        // UUID format: 8-4-4-4-12 hex digits
        let parts: Vec<&str> = v.split('-').collect();
        if parts.len() != 5 {
            return None;
        }
        let lens = [8, 4, 4, 4, 12];
        for (part, &len) in parts.iter().zip(lens.iter()) {
            if part.len() != len || !part.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
        }
        Some(v.to_lowercase())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Valid UUIDs for test IDs (Pane::new canonicalizes non-UUID IDs to generated ones,
    // matching the Swift PaneStableID.canonical behavior).
    const G1: &str = "11111111-1111-1111-1111-111111111111";
    const G2: &str = "22222222-2222-2222-2222-222222222222";
    const G9: &str = "99999999-9999-9999-9999-999999999999";
    const P1: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
    const P2: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
    const P3: &str = "cccccccc-cccc-cccc-cccc-cccccccccccc";
    const P4: &str = "dddddddd-dddd-dddd-dddd-dddddddddddd";
    const P5: &str = "eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee";
    const PL1: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";

    fn make_state() -> PaneLayoutState {
        let mut state = PaneLayoutState::new(vec![]);
        state
            .create_group(
                "s1",
                "s2",
                PaneEdge::Right,
                Some(G1.into()),
                Some(P1.into()),
                Some(P2.into()),
            )
            .unwrap();
        state
    }

    #[test]
    fn pane_content_session_id() {
        let c = PaneContent::Session { id: "abc".into() };
        assert_eq!(c.session_id(), Some("abc"));
        assert!(!c.is_launcher());
        let l = PaneContent::Launcher {
            project_id: "p".into(),
        };
        assert_eq!(l.session_id(), None);
        assert!(l.is_launcher());
    }

    #[test]
    fn pane_edge_split_direction() {
        assert_eq!(PaneEdge::Left.split_direction(), SplitDirection::Horizontal);
        assert_eq!(
            PaneEdge::Right.split_direction(),
            SplitDirection::Horizontal
        );
        assert_eq!(PaneEdge::Up.split_direction(), SplitDirection::Vertical);
        assert_eq!(PaneEdge::Down.split_direction(), SplitDirection::Vertical);
        assert!(PaneEdge::Left.new_leaf_is_left_child());
        assert!(PaneEdge::Up.new_leaf_is_left_child());
        assert!(!PaneEdge::Right.new_leaf_is_left_child());
        assert!(!PaneEdge::Down.new_leaf_is_left_child());
    }

    #[test]
    fn clamped_ratio_bounds() {
        assert_eq!(PaneSplit::clamped_ratio(0.05), 0.1);
        assert_eq!(PaneSplit::clamped_ratio(0.95), 0.9);
        assert_eq!(PaneSplit::clamped_ratio(0.5), 0.5);
        assert_eq!(PaneSplit::clamped_ratio(f64::NAN), 0.5);
        assert_eq!(PaneSplit::clamped_ratio(f64::INFINITY), 0.5);
    }

    #[test]
    fn create_group_two_panes() {
        let state = make_state();
        assert_eq!(state.groups.len(), 1);
        let group = &state.groups[0];
        assert_eq!(group.id, G1);
        assert_eq!(group.root.leaves().len(), 2);
        assert_eq!(group.root.session_leaves().len(), 2);
    }

    #[test]
    fn create_group_rejects_duplicates() {
        let mut state = make_state();
        // Duplicate session
        assert!(matches!(
            state.create_group("s1", "s3", PaneEdge::Right, None, None, None),
            Err(PaneLayoutError::DuplicateSession(_))
        ));
        // Same session
        assert!(matches!(
            state.create_group("s9", "s9", PaneEdge::Right, None, None, None),
            Err(PaneLayoutError::SameSession)
        ));
        // Empty representative
        assert!(matches!(
            state.create_group("", "s9", PaneEdge::Right, None, None, None),
            Err(PaneLayoutError::InvalidSessionId)
        ));
    }

    #[test]
    fn insert_session_splits_leaf() {
        let mut state = make_state();
        let loc = state
            .insert_session("s3", "s1", PaneEdge::Right, None, None, Some(P3.into()))
            .unwrap();
        assert_eq!(loc.group_id, G1);
        assert_eq!(loc.pane_id, P3);
        let group = &state.groups[0];
        assert_eq!(group.root.leaves().len(), 3);
        assert!(state.location_of_session("s3").is_some());
    }

    #[test]
    fn insert_session_creates_group_when_solo() {
        let mut state = PaneLayoutState::new(vec![]);
        let loc = state
            .insert_session("s2", "s1", PaneEdge::Down, Some(G9.into()), None, None)
            .unwrap();
        assert_eq!(loc.group_id, G9);
        assert_eq!(state.groups.len(), 1);
    }

    #[test]
    fn detach_pane_collapses_split() {
        let mut state = make_state();
        let change = state.detach_pane(P2).unwrap();
        // Only one session left (s1, no launcher) -> group dissolves per Swift canRemainGrouped rule
        assert!(change.dissolved);
        assert_eq!(change.removed_pane_ids.len(), 2);
        assert!(state.groups.is_empty());
    }

    #[test]
    fn detach_pane_keeps_group_with_two_sessions() {
        let mut state = make_state();
        state
            .insert_session("s3", "s1", PaneEdge::Right, None, None, Some(P3.into()))
            .unwrap();
        let change = state.detach_pane(P3).unwrap();
        assert!(!change.dissolved);
        assert_eq!(state.groups.len(), 1);
        assert_eq!(state.groups[0].root.leaves().len(), 2);
    }

    #[test]
    fn close_group_dissolves() {
        let mut state = make_state();
        let change = state.close_group(G1).unwrap();
        assert!(change.dissolved);
        assert_eq!(change.removed_pane_ids.len(), 2);
        assert!(state.groups.is_empty());
    }

    #[test]
    fn launcher_lifecycle() {
        let mut state = make_state();
        // Insert launcher splitting p1
        let loc = state
            .insert_launcher("proj1", P1, PaneEdge::Down, Some(PL1.into()))
            .unwrap();
        assert_eq!(loc.pane_id, PL1);
        assert!(state.groups[0].root.contains_launcher());
        // Second launcher rejected
        assert!(matches!(
            state.insert_launcher("proj2", P2, PaneEdge::Up, None),
            Err(PaneLayoutError::LauncherAlreadyPresent(_))
        ));
        // Bind launcher to a session
        state.bind_launcher(PL1, "s9").unwrap();
        assert!(!state.groups[0].root.contains_launcher());
        assert!(state.location_of_session("s9").is_some());
    }

    #[test]
    fn remove_launcher_restores_snapshot() {
        let mut state = make_state();
        let before = state.groups[0].root.structural_identity();
        state
            .insert_launcher("proj1", P1, PaneEdge::Down, Some(PL1.into()))
            .unwrap();
        // Remove launcher -> pre-launcher snapshot restored (surviving leaves match)
        let change = state.remove_launcher(PL1).unwrap();
        // Group had 2 sessions + launcher; after removal, 2 sessions remain
        assert!(!change.dissolved);
        let after = state.groups[0].root.structural_identity();
        assert_eq!(before, after);
    }

    #[test]
    fn resize_split_clamps() {
        let mut state = make_state();
        let path = PaneSplitPath::root();
        let applied = state.resize_split(G1, &path, 0.95).unwrap();
        assert_eq!(applied, 0.9);
        let applied = state.resize_split(G1, &path, 0.05).unwrap();
        assert_eq!(applied, 0.1);
        assert!(matches!(
            state.resize_split(G1, &path, f64::NAN),
            Err(PaneLayoutError::InvalidRatio)
        ));
    }

    #[test]
    fn equalize_balances_splits() {
        let mut state = make_state();
        state
            .insert_session("s3", "s1", PaneEdge::Right, None, None, Some(P3.into()))
            .unwrap();
        // Distort then equalize
        let path = state.groups[0].root.path_to_pane(P1).unwrap();
        // Find the root split path (empty = root)
        state.resize_split(G1, &PaneSplitPath::root(), 0.8).unwrap();
        state.equalize(G1).unwrap();
        // After equalize with 3 leaves, ratios should be balanced
        let group = &state.groups[0];
        if let PaneNode::Split(split) = &group.root {
            // 3 leaves: left subtree has 2, right has 1 -> ratio 2/3
            assert!((split.ratio - 2.0 / 3.0).abs() < 1e-9);
        } else {
            panic!("expected split");
        }
        let _ = path;
    }

    #[test]
    fn swap_panes_exchanges_positions() {
        let mut state = make_state();
        let swapped = state.swap_panes(P1, P2).unwrap();
        assert!(swapped);
        // Same pane -> false
        assert!(!state.swap_panes(P1, P1).unwrap());
        // Representative unchanged (ids travel with leaves)
        assert_eq!(state.groups[0].representative_pane_id, P1);
    }

    #[test]
    fn swap_panes_different_groups_rejected() {
        let mut state = make_state();
        state
            .create_group(
                "s3",
                "s4",
                PaneEdge::Right,
                Some(G2.into()),
                Some(P4.into()),
                Some(P5.into()),
            )
            .unwrap();
        assert!(matches!(
            state.swap_panes(P1, P4),
            Err(PaneLayoutError::PanesBelongToDifferentGroups)
        ));
    }

    #[test]
    fn spatial_neighbor_finds_adjacent() {
        let mut state = make_state();
        // p1 | p2 (horizontal split, p2 on right)
        let neighbor = state.spatial_neighbor(P1, PaneEdge::Right);
        assert_eq!(neighbor.map(|p| p.id), Some(P2.to_string()));
        let neighbor = state.spatial_neighbor(P2, PaneEdge::Left);
        assert_eq!(neighbor.map(|p| p.id), Some(P1.to_string()));
        // No neighbor up
        assert!(state.spatial_neighbor(P1, PaneEdge::Up).is_none());
    }

    #[test]
    fn reconcile_drops_ineligible() {
        let mut state = make_state();
        state
            .insert_session("s3", "s1", PaneEdge::Right, None, None, Some(P3.into()))
            .unwrap();
        let eligible: HashSet<String> = ["s1".to_string(), "s2".to_string()].into_iter().collect();
        let changes = state.reconcile(&eligible);
        assert_eq!(changes.len(), 1);
        assert!(state.location_of_session("s3").is_none());
        assert!(state.location_of_session("s1").is_some());
    }

    #[test]
    fn reconcile_dissolves_single_session_group() {
        let mut state = make_state();
        let eligible: HashSet<String> = ["s1".to_string()].into_iter().collect();
        let changes = state.reconcile(&eligible);
        assert_eq!(changes.len(), 1);
        assert!(changes[0].dissolved);
        assert!(state.groups.is_empty());
    }

    #[test]
    fn leaf_slots_subdivide_bounds() {
        let state = make_state();
        let group = &state.groups[0];
        let slots = group.root.leaf_slots(Rect::new(0.0, 0.0, 100.0, 50.0));
        assert_eq!(slots.len(), 2);
        // Horizontal split at 0.5: left is 0-50, right is 50-100
        let left = slots.iter().find(|(p, _)| p.id == P1).unwrap();
        let right = slots.iter().find(|(p, _)| p.id == P2).unwrap();
        assert_eq!(left.1.width, 50.0);
        assert_eq!(right.1.x, 50.0);
    }

    #[test]
    fn structural_identity_ignores_ratio() {
        let mut state = make_state();
        let before = state.groups[0].root.structural_identity();
        state.resize_split(G1, &PaneSplitPath::root(), 0.7).unwrap();
        let after = state.groups[0].root.structural_identity();
        assert_eq!(before, after);
    }

    #[test]
    fn durable_round_trip() {
        let state = make_state();
        let durable = DurablePaneLayout::new(&state, None);
        assert_eq!(durable.version, 2);
        assert_eq!(durable.groups.len(), 1);
        let restored = durable.restored_state();
        assert_eq!(restored.groups.len(), 1);
        assert_eq!(
            restored.groups[0].root.leaves().len(),
            state.groups[0].root.leaves().len()
        );
    }

    #[test]
    fn durable_json_round_trip() {
        let state = make_state();
        let durable = DurablePaneLayout::new(&state, None);
        let json = serde_json::to_string(&durable).unwrap();
        let decoded: DurablePaneLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.version, 2);
        assert_eq!(decoded.groups.len(), 1);
        // Verify the {pane:{}} / {split:{}} wire shape
        assert!(json.contains("\"pane\"") || json.contains("\"split\""));
    }

    #[test]
    fn durable_v1_migration() {
        // v1 flat shape with legacy Rust key spellings
        let json = format!(
            r#"{{
            "version": 1,
            "groups": [{{
                "id": "{G1}",
                "representativePaneId": "{P1}",
                "panes": [
                    {{"id": "{P1}", "sessionId": "s1", "fraction": 0.5}},
                    {{"id": "{P2}", "sessionId": "s2", "fraction": 0.5}}
                ]
            }}]
        }}"#,
        );
        let decoded: DurablePaneLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.version, 1);
        assert_eq!(decoded.groups.len(), 1);
        let restored = decoded.restored_state();
        assert_eq!(restored.groups[0].root.leaves().len(), 2);
    }

    #[test]
    fn durable_v1_swift_key_spellings() {
        let json = format!(
            r#"{{
            "version": 1,
            "groups": [{{
                "id": "{G1}",
                "representativePaneID": "{P1}",
                "panes": [
                    {{"id": "{P1}", "sessionID": "s1", "fraction": 0.5}},
                    {{"id": "{P2}", "sessionID": "s2", "fraction": 0.5}}
                ]
            }}]
        }}"#,
        );
        let decoded: DurablePaneLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.groups.len(), 1);
    }

    #[test]
    fn durable_unknown_version_yields_empty() {
        let json = r#"{"version": 99, "groups": []}"#;
        let decoded: DurablePaneLayout = serde_json::from_str(json).unwrap();
        assert!(decoded.groups.is_empty());
    }

    #[test]
    fn pane_stable_id_canonical() {
        // Valid UUID lowercased
        assert_eq!(
            PaneStableId::canonical("550E8400-E29B-41D4-A716-446655440000"),
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
        // Invalid
        assert_eq!(PaneStableId::canonical("not-a-uuid"), None);
        assert_eq!(PaneStableId::canonical(""), None);
        // Generated ids are unique
        let a = PaneStableId::make();
        let b = PaneStableId::make();
        assert_ne!(a, b);
    }

    #[test]
    fn grid_dimensions() {
        let state = make_state();
        let (w, h) = state.groups[0].root.grid_dimensions();
        assert_eq!((w, h), (2.0, 1.0));
    }

    #[test]
    fn path_to_pane_and_node_at() {
        let state = make_state();
        let root = &state.groups[0].root;
        let path = root.path_to_pane(P2).unwrap();
        assert_eq!(path.components, vec![PaneSplitBranch::Right]);
        let node = root.node_at(&path).unwrap();
        assert!(matches!(node, PaneNode::Leaf(_)));
        // Root path
        let root_path = PaneSplitPath::root();
        assert!(root.node_at(&root_path).is_some());
        // Missing pane
        assert!(root.path_to_pane("nope").is_none());
    }

    #[test]
    fn removing_leaf_collapses() {
        let state = make_state();
        let root = &state.groups[0].root;
        // Remove p1 -> p2's leaf replaces the split
        let collapsed = root.removing_leaf(P1).unwrap();
        assert!(matches!(collapsed, PaneNode::Leaf(_)));
        // Remove unknown -> unchanged
        let unchanged = root.removing_leaf("nope").unwrap();
        assert_eq!(unchanged.structural_identity(), root.structural_identity());
    }

    #[test]
    fn maximum_session_leaf_count_enforced() {
        let mut state = PaneLayoutState::new(vec![]);
        state
            .create_group(
                "s0",
                "s1",
                PaneEdge::Right,
                Some("g".into()),
                Some("p0".into()),
                Some(P1.into()),
            )
            .unwrap();
        for i in 2..8 {
            state
                .insert_session(&format!("s{i}"), "s0", PaneEdge::Right, None, None, None)
                .unwrap();
        }
        // 8 sessions now; 9th should fail
        assert!(matches!(
            state.insert_session("s8", "s0", PaneEdge::Right, None, None, None),
            Err(PaneLayoutError::GroupAtCapacity)
        ));
    }

    #[test]
    fn location_queries() {
        let state = make_state();
        let loc = state.location_of_session("s1").unwrap();
        assert_eq!(loc.group_id, G1);
        assert_eq!(loc.pane_id, P1);
        assert!(state.location_of_session("nope").is_none());
        let ploc = state.location_of_pane(P2).unwrap();
        assert_eq!(ploc.pane_id, P2);
        assert!(state.group_containing_session("s2").is_some());
        assert!(state.pane(P1).is_some());
        assert!(state.pane("nope").is_none());
    }
}
