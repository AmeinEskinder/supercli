//! Pure native-store policies ported from the Swift app layer.
//!
//! Ports the test-covered pure functions from the legacy Swift store,
//! `FeatureFlags.swift`, and `HostManagementState.swift`:
//!
//! - feature gating and computer-use containment (`FeatureFlags.swift`)
//! - phone-fit resize overrides (legacy store's `phoneResizeOverrides`)
//! - predicted resume insertion index (`RemoteResumePlacementTests`)
//! - superseded restart ghost detection (`RestartGhostTests`)
//! - session title resolution and pending-write decoding
//!   (`SharedOrganizationReconciliationTests`)
//! - pin override application and reconciliation
//!   (`SharedOrganizationReconciliationTests`)
//! - management-state equality gating (`HostManagementState`)
//!
//! All UI, persistence, and Host I/O stay with the callers; what lives here
//! is the decision logic plus its tests.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Feature flags (FeatureFlags.swift)
// ---------------------------------------------------------------------------

/// A user-facing optional feature, toggleable in Settings ▸ Features.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppFeature {
    /// Stable id; also the UserDefaults key suffix. Never rename once shipped.
    pub key: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub env_override: Option<&'static str>,
    pub legacy_env_overrides: &'static [&'static str],
    pub default_on: bool,
    /// Still being shaped: listed under the Features tab's Experimental
    /// section instead of the shipped list.
    pub experimental: bool,
}

impl AppFeature {
    pub fn defaults_key(&self) -> String {
        format!("supercli.experimental.{}", self.key)
    }

    pub fn env_overrides(&self) -> Vec<&'static str> {
        let mut overrides: Vec<&'static str> = self.env_override.into_iter().collect();
        overrides.extend(self.legacy_env_overrides.iter().copied());
        overrides
    }

    pub const REMOTE_WORKSPACES: AppFeature = AppFeature {
        key: "remoteWorkspaces",
        title: "Remote workspaces",
        summary: "Add and control workspaces on other machines.",
        env_override: Some("SUPERCLI_DEV_REMOTE_WORKSPACES"),
        legacy_env_overrides: &[],
        default_on: true,
        experimental: false,
    };
    pub const WORKTREES: AppFeature = AppFeature {
        key: "worktrees",
        title: "Git worktrees",
        summary: "Run sessions in an isolated git worktree of a project.",
        env_override: Some("SUPERCLI_DEV_WORKTREES"),
        legacy_env_overrides: &[],
        default_on: true,
        experimental: false,
    };
    pub const SESSIONS_MCP: AppFeature = AppFeature {
        key: "sessionsMcp",
        title: "Sessions use",
        summary: "Let an agent session see your other sessions.",
        env_override: Some("SUPERCLI_DEV_SESSIONS_MCP"),
        legacy_env_overrides: &[],
        default_on: true,
        experimental: false,
    };
    pub const WORKSPACES: AppFeature = AppFeature {
        key: "profiles",
        title: "Workspaces",
        summary: "Use extra, fully separate workspaces on this Mac.",
        env_override: Some("SUPERCLI_DEV_WORKSPACES"),
        legacy_env_overrides: &["SUPERCLI_DEV_PROFILES"],
        default_on: true,
        experimental: false,
    };
    /// Legacy preference identity retained for decoding saved settings.
    /// Computer use is retired in every build.
    pub const COMPUTER_USE: AppFeature = AppFeature {
        key: "computerUse",
        title: "Computer use",
        summary: "Supercli computer use has been retired.",
        env_override: None,
        legacy_env_overrides: &[],
        default_on: false,
        experimental: true,
    };
    pub const BROWSER_MCP: AppFeature = AppFeature {
        key: "browserMcp",
        title: "Browser use",
        summary: "Let agent sessions drive a real browser.",
        env_override: Some("SUPERCLI_DEV_BROWSER_MCP"),
        legacy_env_overrides: &[],
        default_on: true,
        experimental: true,
    };

    /// Everything shown in Settings ▸ Features, in display order.
    pub fn all() -> Vec<&'static AppFeature> {
        vec![
            &Self::REMOTE_WORKSPACES,
            &Self::WORKTREES,
            &Self::SESSIONS_MCP,
            &Self::WORKSPACES,
            &Self::BROWSER_MCP,
        ]
    }
}

/// Computer use is retired in every build.
pub fn computer_use_available() -> bool {
    false
}

pub fn computer_use_controllable() -> bool {
    false
}

pub fn is_feature_available(feature: &AppFeature) -> bool {
    feature.key != AppFeature::COMPUTER_USE.key
}

/// Every feature this build offers a toggle for, in display order.
pub fn available_features() -> Vec<&'static AppFeature> {
    AppFeature::all()
        .into_iter()
        .filter(|feature| is_feature_available(feature))
        .collect()
}

/// The shipped features: the Features tab's plain list.
pub fn available_shipped_features() -> Vec<&'static AppFeature> {
    available_features()
        .into_iter()
        .filter(|feature| !feature.experimental)
        .collect()
}

/// The features still marked experimental: the tab's Experimental section.
pub fn available_experimental_features() -> Vec<&'static AppFeature> {
    available_features()
        .into_iter()
        .filter(|feature| feature.experimental)
        .collect()
}

/// Whether a feature is currently enabled — env override first (dev escape
/// hatch), then this workspace's own stored preference, then the feature's
/// built-in default. The caller supplies the environment and stored values
/// so this stays pure and testable.
pub fn is_feature_enabled(
    feature: &AppFeature,
    env: &dyn Fn(&str) -> Option<String>,
    stored: &dyn Fn(&str) -> Option<bool>,
) -> bool {
    if !is_feature_available(feature) {
        return false;
    }
    if feature
        .env_overrides()
        .iter()
        .any(|name| env(name).as_deref() == Some("1"))
    {
        return true;
    }
    if let Some(own) = stored(&feature.defaults_key()) {
        return own;
    }
    feature.default_on
}

// ---------------------------------------------------------------------------
// Phone-fit resize overrides
// ---------------------------------------------------------------------------

/// The phone-driven terminal grid a session is resized to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhoneResizeOverride {
    pub cols: u32,
    pub rows: u32,
}

/// A Host-published session summary carrying the fields the phone-fit rule
/// reads. The DTO must tolerate missing additive fields, so both are
/// `Option`.
#[derive(Debug, Clone)]
pub struct PhoneFitSummary {
    pub id: String,
    pub running: bool,
    pub phone_fit_columns: Option<u32>,
    pub phone_fit_rows: Option<u32>,
}

/// Derive phone resize overrides from Host-published session summaries.
///
/// Only running sessions with both dimensions get overrides; dimensions are
/// clamped to `2...300` columns and `2...120` rows. Locally cleared ids stay
/// cleared while the Host still publishes a fit; the clear marker is dropped
/// once the Host stops publishing the fit.
pub fn phone_resize_overrides(
    summaries: &[PhoneFitSummary],
    locally_cleared: &HashSet<String>,
) -> (HashMap<String, PhoneResizeOverride>, HashSet<String>) {
    let mut overrides = HashMap::new();
    let mut surviving_clears = HashSet::new();
    for summary in summaries {
        let (Some(cols), Some(rows)) = (summary.phone_fit_columns, summary.phone_fit_rows) else {
            continue;
        };
        if !summary.running {
            continue;
        }
        if locally_cleared.contains(&summary.id) {
            surviving_clears.insert(summary.id.clone());
            continue;
        }
        overrides.insert(
            summary.id.clone(),
            PhoneResizeOverride {
                cols: cols.clamp(2, 300),
                rows: rows.clamp(2, 120),
            },
        );
    }
    (overrides, surviving_clears)
}

// ---------------------------------------------------------------------------
// Predicted resume insertion index
// ---------------------------------------------------------------------------

/// The landing slot a Host-routed Resume row is moved to while the Host
/// replaces the Session. Mirrors the Host's running-row sort: unranked rows
/// newest-first, then shared-order ranks.
///
/// `order` is the flattened row order (session and folder ids); `shared_order`
/// is the shared ordering whose positions are ranks.
pub fn predicted_resume_insertion_index(
    order: &[String],
    source_id: &str,
    source_created_at: i64,
    shared_order: &[String],
    is_running_row: &dyn Fn(&str) -> bool,
    is_session_row: &dyn Fn(&str) -> bool,
    created_at: &dyn Fn(&str) -> i64,
) -> usize {
    let mut rank: HashMap<&str, usize> = HashMap::new();
    for (index, id) in shared_order.iter().enumerate() {
        rank.insert(id.as_str(), index);
    }
    let source_rank = rank.get(source_id).copied();
    let mut last_running: Option<usize> = None;
    for (index, id) in order.iter().enumerate() {
        if !is_running_row(id) {
            continue;
        }
        last_running = Some(index);
        let row_rank = rank.get(id.as_str()).copied();
        let sorts_after_source = match (source_rank, row_rank) {
            (Some(source), Some(row)) => row > source,
            // Unranked rows precede every ranked row.
            (Some(_), None) => false,
            (None, Some(_)) => true,
            (None, None) => created_at(id) < source_created_at,
        };
        if sorts_after_source {
            return index;
        }
    }
    if let Some(last_running) = last_running {
        return last_running + 1;
    }
    order
        .iter()
        .position(|id| is_session_row(id))
        .unwrap_or(order.len())
}

// ---------------------------------------------------------------------------
// Restart ghosts
// ---------------------------------------------------------------------------

/// A candidate row for restart-ghost detection.
#[derive(Debug, Clone)]
pub struct RestartGhostCandidate {
    pub id: String,
    pub project_id: String,
    pub created_at: i64,
    pub is_live: bool,
}

/// Ids of dead rows superseded by a live restart replacement: a dead row is
/// a ghost only when a live row shares its nonzero `(project_id,
/// created_at)` pair. Live rows are never ghosts; zero timestamps never
/// group; project boundaries matter.
pub fn superseded_restart_ghost_ids(candidates: &[RestartGhostCandidate]) -> HashSet<String> {
    let mut live_keys = HashSet::new();
    for candidate in candidates {
        if candidate.is_live && candidate.created_at > 0 {
            live_keys.insert((candidate.project_id.clone(), candidate.created_at));
        }
    }
    if live_keys.is_empty() {
        return HashSet::new();
    }
    candidates
        .iter()
        .filter(|candidate| {
            !candidate.is_live
                && candidate.created_at > 0
                && live_keys.contains(&(candidate.project_id.clone(), candidate.created_at))
        })
        .map(|candidate| candidate.id.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// Session title resolution
// ---------------------------------------------------------------------------

/// A durable shared title marker (title.json).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedTitleMarker {
    pub title: Option<String>,
    /// The writer's durable ordering timestamp.
    pub updated_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTitleResolution {
    pub title: Option<String>,
    pub should_publish_native: bool,
}

pub fn normalized_session_title(title: Option<&str>) -> Option<String> {
    let normalized = title.unwrap_or("").trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

/// Shared markers are the durable App/TUI contract. A failed native write
/// retries only when its durable intent timestamp is newer. Timestamp-less
/// legacy pending bits defer to any valid marker, preventing an old native
/// fallback from overwriting a later TUI/CLI rename after relaunch.
pub fn resolved_session_title(
    shared_marker: Option<&SharedTitleMarker>,
    native_title: Option<&str>,
    pending_write_at: Option<u64>,
) -> SessionTitleResolution {
    let shared_title =
        normalized_session_title(shared_marker.and_then(|marker| marker.title.as_deref()));
    let native_title = normalized_session_title(native_title);
    let Some(native_title) = native_title else {
        return SessionTitleResolution {
            title: shared_title,
            should_publish_native: false,
        };
    };
    let Some(shared_title) = shared_title else {
        return SessionTitleResolution {
            title: Some(native_title),
            should_publish_native: true,
        };
    };
    let fresh_retry = match (
        pending_write_at,
        shared_marker.and_then(|marker| marker.updated_at),
    ) {
        (Some(pending), Some(shared_updated_at)) => pending > 0 && pending > shared_updated_at,
        _ => false,
    };
    if fresh_retry {
        return SessionTitleResolution {
            title: Some(native_title),
            should_publish_native: true,
        };
    }
    SessionTitleResolution {
        title: Some(shared_title),
        should_publish_native: false,
    }
}

/// Decode the persisted pending-title-writes value conservatively:
/// - JSON-encoded `[String: UInt64]` (current) decodes as-is;
/// - a plist-style dictionary keeps only u64 timestamps;
/// - the first uncommitted implementation stored only a pending bit as
///   `[String]`; zero means "unknown age": it may retry when no marker
///   exists, but it can never overwrite a valid shared marker.
pub fn decoded_pending_title_writes(stored: Option<&serde_json::Value>) -> HashMap<String, u64> {
    let Some(stored) = stored else {
        return HashMap::new();
    };
    if let Some(object) = stored.as_object() {
        return object
            .iter()
            .filter_map(|(key, value)| value.as_u64().map(|timestamp| (key.clone(), timestamp)))
            .collect();
    }
    if let Some(array) = stored.as_array() {
        return array
            .iter()
            .filter_map(|value| value.as_str())
            .map(|id| (id.to_string(), 0u64))
            .collect();
    }
    HashMap::new()
}

// ---------------------------------------------------------------------------
// Pin overrides
// ---------------------------------------------------------------------------

/// A pinned sidebar session record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PinnedSidebarSession {
    pub key: String,
    #[serde(rename = "project_id")]
    pub project_id: String,
    #[serde(rename = "session_id", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "pinned_at")]
    pub pinned_at: u64,
}

impl PinnedSidebarSession {
    pub fn key_for_session_id(session_id: &str) -> String {
        format!("session:{session_id}")
    }

    pub fn key_for_project_id(project_id: &str) -> String {
        format!("project:{project_id}")
    }
}

/// The native pin overlay: adds and removals not yet confirmed by shared
/// state. `removed_at` is absent in every previously shipped overlay; a
/// legacy tombstone is retired as soon as readable shared state confirms
/// either the pin or its absence.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativePinOverrides {
    #[serde(default)]
    pub added: Vec<PinnedSidebarSession>,
    #[serde(default, rename = "removedKeys")]
    pub removed_keys: Vec<String>,
    #[serde(default, rename = "removedAt")]
    pub removed_at: HashMap<String, u64>,
}

/// Apply pin add/remove intents as raw-JSON edits to the shared pin map,
/// preserving unknown fields. Returns false (leaving the object untouched)
/// when the stored shape is corrupt, so the intent stays pending for a
/// later retry.
pub fn apply_pin_overrides(
    overrides: &NativePinOverrides,
    object: &mut serde_json::Map<String, serde_json::Value>,
) -> bool {
    use serde_json::Value;

    let mut additions: HashMap<String, &PinnedSidebarSession> = HashMap::new();
    for pin in &overrides.added {
        // Newest wins on duplicate keys, matching Swift's uniquingKeysWith.
        additions.insert(pin.key.clone(), pin);
    }
    let mut target_keys: HashSet<String> = overrides.removed_keys.iter().cloned().collect();
    target_keys.extend(additions.keys().cloned());
    if target_keys.is_empty() {
        return true;
    }

    // Group the stored pins by project, normalizing the legacy flat shape.
    let mut grouped: HashMap<String, Vec<serde_json::Map<String, Value>>> = HashMap::new();
    match object.get("pinned_sessions") {
        None | Some(Value::Null) => {}
        Some(Value::Object(groups)) => {
            for (project_id, raw_rows) in groups {
                let Some(rows) = raw_rows.as_array() else {
                    return false;
                };
                let mut normalized = Vec::with_capacity(rows.len());
                for row in rows {
                    let Some(row) = row.as_object() else {
                        return false;
                    };
                    normalized.push(row.clone());
                }
                grouped.insert(project_id.clone(), normalized);
            }
        }
        Some(Value::Array(raw_rows)) => {
            // Legacy app-state.json stored one flat array.
            for raw_row in raw_rows {
                let Some(row) = raw_row.as_object() else {
                    return false;
                };
                let Some(project_id) = row.get("project_id").and_then(Value::as_str) else {
                    return false;
                };
                grouped
                    .entry(project_id.to_string())
                    .or_default()
                    .push(row.clone());
            }
        }
        // A corrupt/unknown shape must not be replaced with an empty map;
        // returning false keeps the intent for a later retry.
        Some(_) => return false,
    }

    // Remove targeted rows, remembering the first prior row per key so an
    // add can restore its unknown fields.
    let mut prior_rows: HashMap<String, serde_json::Map<String, Value>> = HashMap::new();
    for rows in grouped.values_mut() {
        rows.retain(|row| {
            let key = row
                .get("key")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    row.get("session_id")
                        .and_then(Value::as_str)
                        .map(PinnedSidebarSession::key_for_session_id)
                });
            let Some(key) = key else {
                return true;
            };
            if !target_keys.contains(&key) {
                return true;
            }
            prior_rows.entry(key).or_insert_with(|| row.clone());
            false
        });
    }

    let mut additions: Vec<&&PinnedSidebarSession> = additions.values().collect();
    additions.sort_by(|a, b| a.key.cmp(&b.key));
    for pin in additions {
        let mut row = prior_rows.remove(&pin.key).unwrap_or_default();
        row.insert("key".to_string(), Value::String(pin.key.clone()));
        row.insert(
            "project_id".to_string(),
            Value::String(pin.project_id.clone()),
        );
        match &pin.session_id {
            Some(session_id) => {
                row.insert("session_id".to_string(), Value::String(session_id.clone()));
            }
            None => {
                row.remove("session_id");
            }
        }
        row.insert("pinned_at".to_string(), Value::Number(pin.pinned_at.into()));
        grouped.entry(pin.project_id.clone()).or_default().push(row);
    }

    let grouped_value: serde_json::Map<String, Value> = grouped
        .into_iter()
        .map(|(project_id, rows)| {
            (
                project_id,
                Value::Array(rows.into_iter().map(Value::Object).collect()),
            )
        })
        .collect();
    object.insert("pinned_sessions".to_string(), Value::Object(grouped_value));
    true
}

/// Reconcile the native pin overlay against readable shared state using
/// durable timestamps.
///
/// An added overlay is pending only while it is newer than what the shared
/// file proves: a same/newer shared pin confirms the write; a newer shared
/// file that omits it represents an external unpin. Legacy removals had no
/// timestamp: once shared state is readable it is the only safe authority —
/// presence means a later repin, absence means the old unpin already landed.
pub fn reconciled_pin_overrides(
    overrides: &NativePinOverrides,
    shared_pins: &HashMap<String, PinnedSidebarSession>,
    shared_state_modified_at: Option<u64>,
) -> NativePinOverrides {
    let mut reconciled = overrides.clone();

    reconciled.added.retain(|native_pin| {
        if let Some(shared_pin) = shared_pins.get(&native_pin.key) {
            let confirmed_at = shared_pin
                .pinned_at
                .max(shared_state_modified_at.unwrap_or(0));
            return confirmed_at < native_pin.pinned_at;
        }
        match shared_state_modified_at {
            Some(modified_at) => modified_at < native_pin.pinned_at,
            None => true,
        }
    });

    let mut seen = HashSet::new();
    reconciled.removed_keys.retain(|key| {
        if !seen.insert(key.clone()) {
            return false;
        }
        let Some(removed_at) = reconciled.removed_at.get(key).copied() else {
            return shared_state_modified_at.is_none();
        };
        if let Some(shared_pin) = shared_pins.get(key) {
            let repinned_at = shared_pin
                .pinned_at
                .max(shared_state_modified_at.unwrap_or(0));
            return repinned_at <= removed_at;
        }
        match shared_state_modified_at {
            Some(modified_at) => modified_at < removed_at,
            None => true,
        }
    });
    let retained: HashSet<String> = reconciled.removed_keys.iter().cloned().collect();
    reconciled
        .removed_at
        .retain(|key, _| retained.contains(key));
    reconciled
}

// ---------------------------------------------------------------------------
// Host management state (HostManagementState.swift)
// ---------------------------------------------------------------------------

/// The small model Settings observes directly. Device polling never
/// invalidates the terminal/sidebar store, and a repeated response is a
/// publication no-op: `apply` only publishes when the value differs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostManagementValue {
    pub devices: Vec<String>,
    pub endpoint: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct HostManagementState {
    value: HostManagementValue,
    /// Number of times a differing value was published. Tests assert the
    /// equality gate through this instead of Combine publishers.
    pub publications: u64,
}

impl HostManagementState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn value(&self) -> &HostManagementValue {
        &self.value
    }

    /// Publish `next`, unless it equals the current value.
    pub fn apply(&mut self, next: HostManagementValue) {
        if next == self.value {
            return;
        }
        self.value = next;
        self.publications += 1;
    }

    pub fn update_endpoint(&mut self, endpoint: Option<String>) {
        let mut next = self.value.clone();
        next.endpoint = endpoint;
        self.apply(next);
    }

    pub fn update_error(&mut self, error: Option<String>) {
        let mut next = self.value.clone();
        next.error = error;
        self.apply(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- Feature flags / computer containment (ComputerContainmentTests) --

    #[test]
    fn computer_use_is_retired_in_every_build() {
        assert!(!computer_use_available());
        assert!(!computer_use_controllable());
        assert!(!is_feature_available(&AppFeature::COMPUTER_USE));
    }

    #[test]
    fn shipped_feature_order_excludes_computer_use() {
        let keys: Vec<&str> = available_shipped_features()
            .iter()
            .map(|feature| feature.key)
            .collect();
        assert_eq!(
            keys,
            vec!["remoteWorkspaces", "worktrees", "sessionsMcp", "profiles"]
        );
    }

    #[test]
    fn only_browser_mcp_is_experimental() {
        let keys: Vec<&str> = available_experimental_features()
            .iter()
            .map(|feature| feature.key)
            .collect();
        assert_eq!(keys, vec!["browserMcp"]);
    }

    #[test]
    fn retired_feature_write_fails_without_modifying_state() {
        // setEnabled is a no-op for retired features: the pure gate refuses.
        assert!(!is_feature_available(&AppFeature::COMPUTER_USE));
        // Writing false for a live feature is honored through the stored value.
        let enabled = is_feature_enabled(&AppFeature::WORKTREES, &|_| None, &|key| {
            (key == AppFeature::WORKTREES.defaults_key()).then_some(false)
        });
        assert!(!enabled);
    }

    #[test]
    fn unknown_keys_do_not_enable_features() {
        // An unknown stored key is simply never consulted.
        let enabled = is_feature_enabled(&AppFeature::BROWSER_MCP, &|_| None, &|_| None);
        assert!(enabled);
    }

    #[test]
    fn env_override_forces_feature_on() {
        let enabled = is_feature_enabled(
            &AppFeature::WORKTREES,
            &|name| (name == "SUPERCLI_DEV_WORKTREES").then(|| "1".to_string()),
            &|_| Some(false),
        );
        assert!(enabled);
    }

    // -- Phone fit (PhoneFitProjectionTests) --

    fn fit_summary(
        id: &str,
        running: bool,
        cols: Option<u32>,
        rows: Option<u32>,
    ) -> PhoneFitSummary {
        PhoneFitSummary {
            id: id.to_string(),
            running,
            phone_fit_columns: cols,
            phone_fit_rows: rows,
        }
    }

    #[test]
    fn phone_fit_only_running_sessions_with_both_dimensions() {
        let summaries = vec![
            fit_summary("running", true, Some(80), Some(24)),
            fit_summary("stopped", false, Some(80), Some(24)),
            fit_summary("missing-cols", true, None, Some(24)),
            fit_summary("missing-rows", true, Some(80), None),
        ];
        let (overrides, _) = phone_resize_overrides(&summaries, &HashSet::new());
        assert_eq!(
            overrides.get("running"),
            Some(&PhoneResizeOverride { cols: 80, rows: 24 })
        );
        assert!(!overrides.contains_key("stopped"));
        assert!(!overrides.contains_key("missing-cols"));
        assert!(!overrides.contains_key("missing-rows"));
    }

    #[test]
    fn phone_fit_clamps_dimensions() {
        let summaries = vec![fit_summary("s", true, Some(9_999), Some(0))];
        let (overrides, _) = phone_resize_overrides(&summaries, &HashSet::new());
        assert_eq!(
            overrides.get("s"),
            Some(&PhoneResizeOverride { cols: 300, rows: 2 })
        );
    }

    #[test]
    fn phone_fit_locally_cleared_ids_stay_cleared() {
        let summaries = vec![
            fit_summary("cleared", true, Some(80), Some(24)),
            fit_summary("fresh", true, Some(80), Some(24)),
        ];
        let cleared: HashSet<String> = ["cleared".to_string()].into_iter().collect();
        let (overrides, surviving) = phone_resize_overrides(&summaries, &cleared);
        assert!(!overrides.contains_key("cleared"));
        assert!(overrides.contains_key("fresh"));
        assert_eq!(surviving, cleared);

        // The clear marker disappears once the Host stops publishing the fit.
        let (overrides, surviving) =
            phone_resize_overrides(&[fit_summary("fresh", true, Some(80), Some(24))], &cleared);
        assert!(overrides.contains_key("fresh"));
        assert!(surviving.is_empty());
    }

    // -- Resume placement (RemoteResumePlacementTests) --

    struct ResumeRow {
        running: bool,
        created_at: i64,
    }

    fn resume_index(
        order: &[&str],
        rows: &HashMap<String, ResumeRow>,
        source: &str,
        created_at: i64,
        shared_order: &[&str],
    ) -> usize {
        let order: Vec<String> = order.iter().map(|id| id.to_string()).collect();
        let shared_order: Vec<String> = shared_order.iter().map(|id| id.to_string()).collect();
        predicted_resume_insertion_index(
            &order,
            source,
            created_at,
            &shared_order,
            &|id| rows.get(id).is_some_and(|row| row.running),
            &|id| rows.contains_key(id),
            &|id| rows.get(id).map(|row| row.created_at).unwrap_or(0),
        )
    }

    fn resume_rows(rows: &[(&str, bool, i64)]) -> HashMap<String, ResumeRow> {
        rows.iter()
            .map(|(id, running, created_at)| {
                (
                    id.to_string(),
                    ResumeRow {
                        running: *running,
                        created_at: *created_at,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn unranked_row_sorts_newest_first_among_running_rows() {
        let rows = resume_rows(&[
            ("new", true, 300),
            ("old", true, 100),
            ("stopped", false, 900),
        ]);
        assert_eq!(
            resume_index(&["new", "old", "stopped"], &rows, "s", 200, &[]),
            1
        );
        assert_eq!(
            resume_index(&["new", "old", "stopped"], &rows, "s", 400, &[]),
            0
        );
        assert_eq!(
            resume_index(&["new", "old", "stopped"], &rows, "s", 50, &[]),
            2
        );
    }

    #[test]
    fn ranked_row_takes_its_shared_rank_after_unranked_rows() {
        let rows = resume_rows(&[
            ("fresh", true, 900),
            ("a", true, 100),
            ("c", true, 100),
            ("stopped", false, 50),
        ]);
        let order = ["fresh", "a", "c", "stopped"];
        assert_eq!(resume_index(&order, &rows, "b", 999, &["a", "b", "c"]), 2);
        assert_eq!(resume_index(&order, &rows, "z", 999, &["a", "c", "z"]), 3);
    }

    #[test]
    fn unranked_source_precedes_every_ranked_row() {
        let rows = resume_rows(&[("a", true, 900), ("stopped", false, 50)]);
        assert_eq!(resume_index(&["a", "stopped"], &rows, "s", 1, &["a"]), 0);
    }

    #[test]
    fn no_running_rows_leads_the_stopped_rows_below_folders() {
        let rows = resume_rows(&[("stopped", false, 50)]);
        assert_eq!(resume_index(&["folder", "stopped"], &rows, "s", 1, &[]), 1);
        assert_eq!(resume_index(&[], &HashMap::new(), "s", 1, &[]), 0);
    }

    // -- Restart ghosts (RestartGhostTests) --

    fn ghost_candidate(
        id: &str,
        project: &str,
        created_at: i64,
        live: bool,
    ) -> RestartGhostCandidate {
        RestartGhostCandidate {
            id: id.to_string(),
            project_id: project.to_string(),
            created_at,
            is_live: live,
        }
    }

    #[test]
    fn restart_ghost_needs_live_row_with_same_project_and_created_at() {
        let candidates = vec![
            ghost_candidate("live", "p", 100, true),
            ghost_candidate("ghost", "p", 100, false),
            ghost_candidate("other-project", "q", 100, false),
            ghost_candidate("other-time", "p", 200, false),
            ghost_candidate("zero-live", "p", 0, true),
            ghost_candidate("zero-dead", "p", 0, false),
        ];
        let ghosts = superseded_restart_ghost_ids(&candidates);
        assert_eq!(ghosts, ["ghost".to_string()].into_iter().collect());
    }

    #[test]
    fn restart_ghost_never_removes_live_rows() {
        let candidates = vec![
            ghost_candidate("live-a", "p", 100, true),
            ghost_candidate("live-b", "p", 100, true),
        ];
        assert!(superseded_restart_ghost_ids(&candidates).is_empty());
    }

    #[test]
    fn restart_ghost_multiple_dead_generations_share_one_live_replacement() {
        let candidates = vec![
            ghost_candidate("live", "p", 100, true),
            ghost_candidate("dead-1", "p", 100, false),
            ghost_candidate("dead-2", "p", 100, false),
        ];
        let ghosts = superseded_restart_ghost_ids(&candidates);
        assert_eq!(
            ghosts,
            ["dead-1".to_string(), "dead-2".to_string()]
                .into_iter()
                .collect()
        );
    }

    // -- Title resolution (SharedOrganizationReconciliationTests) --

    #[test]
    fn title_resolution_uses_durable_freshness() {
        assert_eq!(
            resolved_session_title(
                Some(&SharedTitleMarker {
                    title: Some("  Headless rename  ".to_string()),
                    updated_at: Some(300),
                }),
                Some("Stale native rename"),
                Some(200),
            ),
            SessionTitleResolution {
                title: Some("Headless rename".to_string()),
                should_publish_native: false,
            }
        );
        assert_eq!(
            resolved_session_title(
                Some(&SharedTitleMarker {
                    title: Some("Older shared rename".to_string()),
                    updated_at: Some(200),
                }),
                Some("Native write to retry"),
                Some(300),
            ),
            SessionTitleResolution {
                title: Some("Native write to retry".to_string()),
                should_publish_native: true,
            }
        );
    }

    #[test]
    fn legacy_pending_title_never_overwrites_a_valid_shared_marker() {
        assert_eq!(
            resolved_session_title(
                Some(&SharedTitleMarker {
                    title: Some("Shared rename".to_string()),
                    updated_at: Some(400),
                }),
                Some("Legacy pending rename"),
                Some(0),
            ),
            SessionTitleResolution {
                title: Some("Shared rename".to_string()),
                should_publish_native: false,
            }
        );
        assert_eq!(
            resolved_session_title(
                Some(&SharedTitleMarker {
                    title: Some("Shared rename".to_string()),
                    updated_at: None,
                }),
                Some("Timestamped native rename"),
                Some(500),
            ),
            SessionTitleResolution {
                title: Some("Shared rename".to_string()),
                should_publish_native: false,
            }
        );
    }

    #[test]
    fn pending_or_legacy_native_title_publishes_when_no_valid_marker_exists() {
        assert_eq!(
            resolved_session_title(None, Some("  Native rename  "), Some(300)),
            SessionTitleResolution {
                title: Some("Native rename".to_string()),
                should_publish_native: true,
            }
        );
        assert_eq!(
            resolved_session_title(
                Some(&SharedTitleMarker {
                    title: Some("  ".to_string()),
                    updated_at: Some(500),
                }),
                Some("Legacy native rename"),
                None,
            ),
            SessionTitleResolution {
                title: Some("Legacy native rename".to_string()),
                should_publish_native: true,
            }
        );
        assert_eq!(
            resolved_session_title(None, Some("\n"), Some(300)),
            SessionTitleResolution {
                title: None,
                should_publish_native: false,
            }
        );
    }

    #[test]
    fn legacy_pending_title_storage_decodes_conservatively() {
        assert_eq!(
            decoded_pending_title_writes(Some(&json!(["one", "two"]))),
            [("one".to_string(), 0u64), ("two".to_string(), 0u64)]
                .into_iter()
                .collect()
        );
        assert_eq!(
            decoded_pending_title_writes(Some(&json!({ "one": 123u64 }))),
            [("one".to_string(), 123u64)].into_iter().collect()
        );
        assert!(decoded_pending_title_writes(None).is_empty());
        assert!(decoded_pending_title_writes(Some(&json!("junk"))).is_empty());
    }

    // -- Pin overrides (SharedOrganizationReconciliationTests) --

    fn test_pin(id: &str, project_id: &str, pinned_at: u64) -> PinnedSidebarSession {
        PinnedSidebarSession {
            key: PinnedSidebarSession::key_for_session_id(id),
            project_id: project_id.to_string(),
            session_id: Some(id.to_string()),
            pinned_at,
        }
    }

    fn raw_pin(
        id: &str,
        project_id: &str,
        pinned_at: u64,
        extra: serde_json::Value,
    ) -> serde_json::Value {
        let mut row = extra.as_object().cloned().unwrap_or_default();
        row.insert(
            "key".to_string(),
            json!(PinnedSidebarSession::key_for_session_id(id)),
        );
        row.insert("project_id".to_string(), json!(project_id));
        row.insert("session_id".to_string(), json!(id));
        row.insert("pinned_at".to_string(), json!(pinned_at));
        serde_json::Value::Object(row)
    }

    fn grouped_keys(object: &serde_json::Map<String, serde_json::Value>) -> HashSet<String> {
        object
            .get("pinned_sessions")
            .and_then(|value| value.as_object())
            .map(|groups| {
                groups
                    .values()
                    .flat_map(|rows| rows.as_array().cloned().unwrap_or_default())
                    .filter_map(|row| {
                        row.get("key")
                            .and_then(|key| key.as_str())
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn pin_intent_preserves_concurrent_unrelated_shared_pin() {
        let mut object = serde_json::Map::new();
        object.insert(
            "pinned_sessions".to_string(),
            json!({
                "shared-project": [raw_pin("shared", "shared-project", 100, json!({ "future_field": "keep" }))]
            }),
        );
        let native = test_pin("native", "native-project", 200);

        assert!(apply_pin_overrides(
            &NativePinOverrides {
                added: vec![native.clone()],
                ..Default::default()
            },
            &mut object,
        ));

        assert_eq!(
            grouped_keys(&object),
            [
                PinnedSidebarSession::key_for_session_id("shared"),
                native.key,
            ]
            .into_iter()
            .collect()
        );
        let future = object["pinned_sessions"]["shared-project"][0]["future_field"].as_str();
        assert_eq!(future, Some("keep"));
    }

    #[test]
    fn pin_removal_touches_only_its_own_key() {
        let removed_key = PinnedSidebarSession::key_for_session_id("removed");
        let mut object = serde_json::Map::new();
        object.insert(
            "pinned_sessions".to_string(),
            json!({
                "project": [raw_pin("removed", "project", 100, json!({})), raw_pin("untouched", "project", 200, json!({}))]
            }),
        );

        assert!(apply_pin_overrides(
            &NativePinOverrides {
                removed_keys: vec![removed_key.clone()],
                removed_at: [(removed_key.clone(), 300u64)].into_iter().collect(),
                ..Default::default()
            },
            &mut object,
        ));

        assert_eq!(
            grouped_keys(&object),
            [PinnedSidebarSession::key_for_session_id("untouched")]
                .into_iter()
                .collect()
        );
    }

    #[test]
    fn pin_intent_keeps_legacy_flat_pins_and_normalizes_shape() {
        let mut object = serde_json::Map::new();
        object.insert(
            "pinned_sessions".to_string(),
            json!([raw_pin("legacy", "old", 100, json!({}))]),
        );
        let native = test_pin("native", "new", 200);

        assert!(apply_pin_overrides(
            &NativePinOverrides {
                added: vec![native],
                ..Default::default()
            },
            &mut object,
        ));

        assert_eq!(
            object["pinned_sessions"]["old"][0]["session_id"].as_str(),
            Some("legacy")
        );
        assert_eq!(
            object["pinned_sessions"]["new"][0]["session_id"].as_str(),
            Some("native")
        );
    }

    #[test]
    fn malformed_pin_state_leaves_intent_pending() {
        let mut object = serde_json::Map::new();
        object.insert("pinned_sessions".to_string(), json!("not-a-pin-map"));

        assert!(!apply_pin_overrides(
            &NativePinOverrides {
                added: vec![test_pin("native", "project", 200)],
                ..Default::default()
            },
            &mut object,
        ));
        assert_eq!(
            object
                .get("pinned_sessions")
                .and_then(|value| value.as_str()),
            Some("not-a-pin-map")
        );
    }

    #[test]
    fn newer_shared_unpin_retires_stale_native_added_overlay() {
        let reconciled = reconciled_pin_overrides(
            &NativePinOverrides {
                added: vec![test_pin("session", "project", 100)],
                ..Default::default()
            },
            &HashMap::new(),
            Some(200),
        );
        assert!(reconciled.added.is_empty());
    }

    #[test]
    fn pending_native_add_survives_an_older_shared_snapshot() {
        let native_pin = test_pin("session", "project", 300);
        let reconciled = reconciled_pin_overrides(
            &NativePinOverrides {
                added: vec![native_pin.clone()],
                ..Default::default()
            },
            &HashMap::new(),
            Some(200),
        );
        assert_eq!(reconciled.added, vec![native_pin]);
    }

    #[test]
    fn newer_shared_repin_retires_native_removal() {
        let shared_pin = test_pin("session", "group", 100);
        let key = shared_pin.key.clone();
        let reconciled = reconciled_pin_overrides(
            &NativePinOverrides {
                removed_keys: vec![key.clone()],
                removed_at: [(key.clone(), 200u64)].into_iter().collect(),
                ..Default::default()
            },
            &[(key.clone(), shared_pin)].into_iter().collect(),
            Some(300),
        );
        assert!(reconciled.removed_keys.is_empty());
        assert!(reconciled.removed_at.is_empty());
    }

    #[test]
    fn pending_native_removal_beats_an_older_shared_pin() {
        let shared_pin = test_pin("session", "project", 200);
        let key = shared_pin.key.clone();
        let reconciled = reconciled_pin_overrides(
            &NativePinOverrides {
                removed_keys: vec![key.clone()],
                removed_at: [(key.clone(), 300u64)].into_iter().collect(),
                ..Default::default()
            },
            &[(key.clone(), shared_pin)].into_iter().collect(),
            Some(250),
        );
        assert_eq!(reconciled.removed_keys, vec![key.clone()]);
        assert_eq!(reconciled.removed_at.get(&key), Some(&300u64));
    }

    #[test]
    fn legacy_untimestamped_removal_defers_to_readable_shared_state() {
        let key = PinnedSidebarSession::key_for_session_id("session");
        let legacy: NativePinOverrides =
            serde_json::from_str(r#"{"added":[],"removedKeys":["session:session"]}"#).unwrap();
        assert!(legacy.removed_at.is_empty());

        let reconciled = reconciled_pin_overrides(
            &legacy,
            &[(key.clone(), test_pin("session", "project", 400))]
                .into_iter()
                .collect(),
            Some(400),
        );
        assert!(reconciled.removed_keys.is_empty());
    }

    // -- Host management state (StartupPerformanceTests, HostManagementState) --

    #[test]
    fn unchanged_snapshot_does_not_publish() {
        let mut state = HostManagementState::new();
        state.apply(HostManagementValue::default());
        assert_eq!(state.publications, 0);
    }

    #[test]
    fn endpoint_change_publishes_once() {
        let mut state = HostManagementState::new();
        state.update_endpoint(Some("https://host.local".to_string()));
        state.update_endpoint(Some("https://host.local".to_string()));
        assert_eq!(state.publications, 1);
        assert_eq!(
            state.value().endpoint.as_deref(),
            Some("https://host.local")
        );
    }

    #[test]
    fn repeated_identical_error_update_publishes_once() {
        let mut state = HostManagementState::new();
        state.update_error(Some("boom".to_string()));
        state.update_error(Some("boom".to_string()));
        assert_eq!(state.publications, 1);
    }

    #[test]
    fn differing_snapshot_publishes_again() {
        let mut state = HostManagementState::new();
        state.update_error(Some("boom".to_string()));
        state.update_error(None);
        assert_eq!(state.publications, 2);
        assert_eq!(state.value().error, None);
    }
}
