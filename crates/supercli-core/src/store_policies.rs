//! Pure native-store policies ported from the Swift app layer.
//!
//! Ports the test-covered pure functions from the legacy Swift store
//! and `HostManagementState.swift`:
//!
//! - phone-fit resize overrides (legacy store's `phoneResizeOverrides`)
//! - predicted resume insertion index (`RemoteResumePlacementTests`)
//! - superseded restart ghost detection (`RestartGhostTests`)
//! - session title resolution and pending-write decoding
//!   (`SharedOrganizationReconciliationTests`)
//! - management-state equality gating (`HostManagementState`)
//!
//! All UI, persistence, and Host I/O stay with the callers; what lives here
//! is the decision logic plus its tests.

use std::collections::{HashMap, HashSet};

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
