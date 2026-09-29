//! Background workspace pool: policy and state-machine logic.
//!
//! Moved from the Dart client (`clients/supercli-app/lib/screens/workspace_pool.dart`)
//! per Amein's rule: one implementation, in Rust. The Dart client keeps only UI
//! bindings via `supercli-client-ffi`.
//!
//! Port of `WorkspacePool.swift`. The pool keeps one live, read-only background
//! connection per known workspace with a cached latest bootstrap snapshot, so the
//! swipe pager / footer dots / selector can peek real sidebar content before a
//! scope switch.
//!
//! Only the portable policy is here: every pure function and every synchronous
//! state machine (reconciliation, remote-slot accounting, attention-edge latches,
//! optimistic organization holds, exponential backoff). The async entry loops are
//! platform runtime machinery with no portable equivalent and are intentionally
//! not modeled: the host drives the same decisions through these functions.

use std::collections::{HashMap, HashSet};

/// Tunables mirroring `WorkspacePool`'s initializer defaults.
pub mod policy {
    /// Low-cadence poll interval: ~25s. Mirrors `pollIntervalNanoseconds`.
    pub const POLL_INTERVAL_MS: u64 = 25_000;
    /// First backoff step for unreachable hosts. Mirrors `backoffBaseNanoseconds`.
    pub const BACKOFF_BASE_MS: u64 = 5_000;
    /// Backoff ceiling. Mirrors `backoffCapNanoseconds`.
    pub const BACKOFF_CAP_MS: u64 = 300_000;
    /// Target-list reconcile cadence. Mirrors `maintenanceIntervalNanoseconds`.
    pub const MAINTENANCE_INTERVAL_MS: u64 = 30_000;
    /// Minimum gap between caller-initiated immediate refreshes.
    /// Mirrors `immediateRefreshThrottleSeconds`.
    pub const IMMEDIATE_REFRESH_THROTTLE_SECS: f64 = 2.0;
    /// Cap on concurrent live remote (ssh/paired) connections.
    /// Mirrors `maxLiveRemoteConnections` (clamped to >= 1).
    pub const MAX_LIVE_REMOTE_CONNECTIONS: usize = 4;
    /// How long an optimistic reorder hold pins the cached snapshot.
    /// Mirrors `organizationHoldSeconds`.
    pub const ORGANIZATION_HOLD_SECS: f64 = 15.0;
}

/// One pooled workspace. Mirrors `WorkspacePool.Target`.
///
/// The stable `key` IS the shared workspace-order key
/// (`local:<normalized home>`, `host:<hostID>`, `ssh:<id>`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkspacePoolTarget {
    pub key: String,
    pub name: String,
    /// Transport discriminator: one of `local`, `ssh`, `direct`, `link`.
    pub transport_kind: String,
    /// True for ssh/paired transports: subject to the concurrency cap.
    pub is_remote: bool,
    /// Expected host identity; a bootstrap whose host id differs retires the
    /// entry and latches the fingerprint (fail closed).
    pub expected_host_id: Option<String>,
    /// Change detection token.
    pub fingerprint: String,
}

/// Minimal project projection the pool's pure logic reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PooledProject {
    pub id: String,
    pub parent_project_id: Option<String>,
    /// Mixed session order (may contain child-group ids).
    pub session_order: Option<Vec<String>>,
}

impl PooledProject {
    pub fn with_session_order(&self, order: Vec<String>) -> Self {
        Self {
            id: self.id.clone(),
            parent_project_id: self.parent_project_id.clone(),
            session_order: Some(order),
        }
    }
}

/// Minimal session projection the pool's pure logic reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PooledSession {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub command: String,
    /// Lifecycle status: `running` | `exited`.
    pub status: String,
    /// Activity: `starting` | `working` | `blocked` | `idle` | `done`.
    pub activity: String,
    pub archived: bool,
}

impl PooledSession {
    /// Swift `acceptSnapshot` attention predicate: running, blocked, live.
    pub fn needs_attention(&self) -> bool {
        self.status == "running" && self.activity == "blocked" && !self.archived
    }

    /// Swift `acceptSnapshot` notification title: empty title falls back to
    /// the raw command.
    pub fn attention_title(&self) -> &str {
        if self.title.is_empty() {
            &self.command
        } else {
            &self.title
        }
    }
}

/// Minimal bootstrap projection for the pool cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PooledSnapshot {
    pub projects: Vec<PooledProject>,
    pub sessions: Vec<PooledSession>,
    /// Capture timestamp. Advances on every poll even when nothing changed;
    /// [`Self::is_equivalent_to`] normalizes it away.
    pub captured_at_unix_ms: u64,
}

impl PooledSnapshot {
    pub fn with_captured_at(&self, captured_at_unix_ms: u64) -> Self {
        Self {
            projects: self.projects.clone(),
            sessions: self.sessions.clone(),
            captured_at_unix_ms,
        }
    }

    /// Published-cache equivalence: full equality with the capture timestamp
    /// normalized away. Mirrors `isEquivalent(to:)`.
    pub fn is_equivalent_to(&self, other: &Self) -> bool {
        self.with_captured_at(other.captured_at_unix_ms) == *other
    }

    /// Replace only the relative positions named by `ordered_ids`.
    /// Mirrors `applyingProjectOrder(parentID:orderedIDs:)`.
    pub fn applying_project_order(&self, parent_id: Option<&str>, ordered_ids: &[String]) -> Self {
        let sibling_ids: HashSet<&str> = self
            .projects
            .iter()
            .filter(|p| p.parent_project_id.as_deref() == parent_id)
            .map(|p| p.id.as_str())
            .collect();
        let preferred: Vec<&str> = ordered_ids
            .iter()
            .map(String::as_str)
            .filter(|id| sibling_ids.contains(id))
            .collect();
        let reordered = applying_relative_order(&preferred, &self.projects, |p| p.id.as_str());
        Self {
            projects: reordered,
            sessions: self.sessions.clone(),
            captured_at_unix_ms: self.captured_at_unix_ms,
        }
    }

    /// Optimistically reorder one project's session summaries and its mixed
    /// `sessionOrder` field, preserving child-folder slots.
    /// Mirrors `applyingSessionOrder(projectID:orderedIDs:)`.
    pub fn applying_session_order(&self, project_id: &str, ordered_ids: &[String]) -> Self {
        let member_ids: HashSet<&str> = self
            .sessions
            .iter()
            .filter(|s| s.project_id == project_id)
            .map(|s| s.id.as_str())
            .collect();
        let preferred: Vec<&str> = ordered_ids
            .iter()
            .map(String::as_str)
            .filter(|id| member_ids.contains(id))
            .collect();
        let reordered_sessions =
            applying_relative_order(&preferred, &self.sessions, |s| s.id.as_str());
        let reordered_projects: Vec<PooledProject> = self
            .projects
            .iter()
            .map(|p| {
                if p.id == project_id {
                    if let Some(order) = &p.session_order {
                        let reordered = applying_relative_order(&preferred, order, |s| s.as_str());
                        return p.with_session_order(reordered);
                    }
                }
                p.clone()
            })
            .collect();
        Self {
            projects: reordered_projects,
            sessions: reordered_sessions,
            captured_at_unix_ms: self.captured_at_unix_ms,
        }
    }
}

/// Replace only the relative positions named by `preferred_ids`; values whose
/// ids are not preferred keep their slots. Unknown preferred ids are ignored.
/// Mirrors `RemoteBootstrapSnapshot.applyingRelativeOrder`.
pub fn applying_relative_order<T: Clone>(
    preferred_ids: &[&str],
    values: &[T],
    id_of: impl Fn(&T) -> &str,
) -> Vec<T> {
    let by_id: HashMap<&str, &T> = values.iter().map(|v| (id_of(v), v)).collect();
    let preferred: Vec<&T> = preferred_ids
        .iter()
        .filter_map(|id| by_id.get(id).copied())
        .collect();
    let replacing_ids: HashSet<&str> = preferred.iter().map(|v| id_of(v)).collect();
    let mut it = preferred.into_iter();
    values
        .iter()
        .map(|v| {
            if replacing_ids.contains(id_of(v)) {
                it.next().cloned().unwrap_or_else(|| v.clone())
            } else {
                v.clone()
            }
        })
        .collect()
}

/// Exponential backoff delay for an unreachable host: base doubling per
/// consecutive failure, exponent clamped to 16, result capped.
///
/// Mirrors `backoffDelayNanoseconds(_:)` (units converted to milliseconds).
pub fn backoff_delay_ms(consecutive_failures: u32, base_ms: u64, cap_ms: u64) -> u64 {
    let exponent = (consecutive_failures.saturating_sub(1)).min(16);
    let multiplier: u64 = 1 << exponent;
    let uncapped = base_ms.saturating_mul(multiplier);
    // Overflow guard mirroring `multipliedReportingOverflow`: on overflow
    // the delay is the cap. `saturating_mul` already saturates, so detect by
    // checking the division round-trip.
    let delay = if multiplier != 0 && uncapped / multiplier != base_ms {
        cap_ms
    } else {
        uncapped
    };
    delay.min(cap_ms)
}

/// Per-(workspace, session) notification latch: a session notifies once per
/// blocked EDGE. Mirrors `WorkspacePool.AttentionLatch`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AttentionLatch {
    /// True once the first snapshot has been observed.
    pub seeded: bool,
    /// Session ids currently latched as blocked (already notified or seeded).
    pub notified_session_ids: HashSet<String>,
}

/// Outcome of advancing one workspace's attention bookkeeping.
#[derive(Debug, Clone)]
pub struct AttentionAdvance {
    /// Latch to store for the workspace.
    pub latch: AttentionLatch,
    /// Session ids to notify, in sorted order (deterministic).
    pub notify_session_ids: Vec<String>,
    /// Whether the workspace currently carries any blocked live session.
    pub has_attention: bool,
}

/// Advances attention bookkeeping for one accepted snapshot.
///
/// - First contact seeds silently: stale attention at launch is a badge,
///   never a banner.
/// - Later polls notify exactly once per blocked edge.
pub fn advance_attention(
    latch: &AttentionLatch,
    blocked_ids: &HashSet<String>,
    is_foreground: bool,
) -> AttentionAdvance {
    if !latch.seeded || is_foreground {
        return AttentionAdvance {
            latch: AttentionLatch {
                seeded: true,
                notified_session_ids: blocked_ids.clone(),
            },
            notify_session_ids: Vec::new(),
            has_attention: !blocked_ids.is_empty(),
        };
    }
    let mut newly_blocked: Vec<String> = blocked_ids
        .difference(&latch.notified_session_ids)
        .cloned()
        .collect();
    newly_blocked.sort();
    AttentionAdvance {
        latch: AttentionLatch {
            seeded: true,
            notified_session_ids: blocked_ids.clone(),
        },
        notify_session_ids: newly_blocked,
        has_attention: !blocked_ids.is_empty(),
    }
}

/// A held optimistic project reorder for one workspace.
#[derive(Debug, Clone)]
pub struct ProjectOrderHold {
    /// Parent whose children were reordered; None for roots.
    pub parent_id: Option<String>,
    pub ids: Vec<String>,
    /// Wall-clock hold time, milliseconds since epoch.
    pub held_at_ms: u64,
}

/// A held optimistic session reorder for one project.
#[derive(Debug, Clone)]
pub struct SessionOrderHold {
    pub ids: Vec<String>,
    pub held_at_ms: u64,
}

/// Optimistic organization holds for one workspace.
#[derive(Debug, Clone, Default)]
pub struct OrganizationHolds {
    pub project_hold: Option<ProjectOrderHold>,
    pub session_holds: HashMap<String, SessionOrderHold>,
}

impl OrganizationHolds {
    pub fn is_empty(&self) -> bool {
        self.project_hold.is_none() && self.session_holds.is_empty()
    }

    /// Records a foreground project reorder and pins it over the cached
    /// snapshot immediately. Mirrors `holdProjectOrder`.
    pub fn hold_project_order(
        &self,
        snapshot: &PooledSnapshot,
        parent_id: Option<String>,
        ordered_ids: Vec<String>,
        now_ms: u64,
    ) -> (Self, PooledSnapshot) {
        let next = Self {
            project_hold: Some(ProjectOrderHold {
                parent_id: parent_id.clone(),
                ids: ordered_ids.clone(),
                held_at_ms: now_ms,
            }),
            session_holds: self.session_holds.clone(),
        };
        let updated = snapshot.applying_project_order(parent_id.as_deref(), &ordered_ids);
        (next, updated)
    }

    /// Records a foreground session reorder and pins it over the cached
    /// snapshot immediately. Mirrors `holdSessionOrder`.
    pub fn hold_session_order(
        &self,
        snapshot: &PooledSnapshot,
        project_id: String,
        ordered_ids: Vec<String>,
        now_ms: u64,
    ) -> (Self, PooledSnapshot) {
        let mut session_holds = self.session_holds.clone();
        session_holds.insert(
            project_id.clone(),
            SessionOrderHold {
                ids: ordered_ids.clone(),
                held_at_ms: now_ms,
            },
        );
        let next = Self {
            project_hold: self.project_hold.clone(),
            session_holds,
        };
        let updated = snapshot.applying_session_order(&project_id, &ordered_ids);
        (next, updated)
    }

    /// Projects an incoming bootstrap through the active holds.
    /// Mirrors `applyingOrganizationHolds(to:key:)`.
    pub fn apply_to(&self, incoming: &PooledSnapshot, now_ms: u64) -> (PooledSnapshot, Self) {
        let mut snapshot = incoming.clone();
        let mut project_hold = self.project_hold.clone();
        let mut session_holds = self.session_holds.clone();

        let hold_ms = (policy::ORGANIZATION_HOLD_SECS * 1000.0).round() as u64;

        if let Some(hold) = &self.project_hold {
            let member_ids: HashSet<&str> = incoming
                .projects
                .iter()
                .filter(|p| p.parent_project_id == hold.parent_id)
                .map(|p| p.id.as_str())
                .collect();
            let expected: Vec<&str> = hold
                .ids
                .iter()
                .map(String::as_str)
                .filter(|id| member_ids.contains(id))
                .collect();
            let expected_set: HashSet<&str> = expected.iter().copied().collect();
            let natural: Vec<&str> = incoming
                .projects
                .iter()
                .filter(|p| expected_set.contains(p.id.as_str()))
                .map(|p| p.id.as_str())
                .collect();
            if natural == expected || now_ms.saturating_sub(hold.held_at_ms) > hold_ms {
                project_hold = None;
            } else {
                snapshot = snapshot.applying_project_order(hold.parent_id.as_deref(), &hold.ids);
            }
        }

        let stale_projects: Vec<String> = session_holds
            .iter()
            .filter_map(|(project_id, hold)| {
                let member_ids: HashSet<&str> = incoming
                    .sessions
                    .iter()
                    .filter(|s| &s.project_id == project_id)
                    .map(|s| s.id.as_str())
                    .collect();
                let expected: Vec<&str> = hold
                    .ids
                    .iter()
                    .map(String::as_str)
                    .filter(|id| member_ids.contains(id))
                    .collect();
                let expected_set: HashSet<&str> = expected.iter().copied().collect();
                let natural: Vec<&str> = incoming
                    .sessions
                    .iter()
                    .filter(|s| expected_set.contains(s.id.as_str()))
                    .map(|s| s.id.as_str())
                    .collect();
                if natural == expected || now_ms.saturating_sub(hold.held_at_ms) > hold_ms {
                    Some(project_id.clone())
                } else {
                    snapshot = snapshot.applying_session_order(project_id, &hold.ids);
                    None
                }
            })
            .collect();
        for project_id in stale_projects {
            session_holds.remove(&project_id);
        }

        (
            snapshot,
            Self {
                project_hold,
                session_holds,
            },
        )
    }
}

/// Outcome of one target-list reconciliation. Mirrors `refreshTargets()`.
#[derive(Debug, Clone)]
pub struct PoolReconcileResult {
    /// Live entries to retire.
    pub retire_keys: Vec<String>,
    /// Targets to start polling.
    pub start_targets: Vec<WorkspacePoolTarget>,
    /// Cached keys to drop entirely.
    pub drop_cache_keys: Vec<String>,
}

/// Re-reads the known-workspace list and converges entries on it, exactly
/// like `refreshTargets()`.
pub fn reconcile_pool_targets(
    entry_fingerprints: &HashMap<String, String>,
    targets: &[WorkspacePoolTarget],
    excluded_keys: &HashSet<String>,
    failed_identity_fingerprints: &HashSet<String>,
    cached_keys: &HashSet<String>,
) -> PoolReconcileResult {
    let by_key: HashMap<&str, &WorkspacePoolTarget> =
        targets.iter().map(|t| (t.key.as_str(), t)).collect();

    let mut retire_keys = Vec::new();
    for (key, fingerprint) in entry_fingerprints {
        let target = by_key.get(key.as_str());
        if target.is_none()
            || excluded_keys.contains(key.as_str())
            || target.is_some_and(|t| t.fingerprint != *fingerprint)
        {
            retire_keys.push(key.clone());
        }
    }
    let retire_set: HashSet<&str> = retire_keys.iter().map(String::as_str).collect();
    let live_keys: HashSet<&str> = entry_fingerprints
        .keys()
        .map(String::as_str)
        .filter(|k| !retire_set.contains(k))
        .collect();

    let drop_cache_keys: Vec<String> = cached_keys
        .iter()
        .filter(|key| !by_key.contains_key(key.as_str()) && !excluded_keys.contains(key.as_str()))
        .cloned()
        .collect();

    let mut start_targets = Vec::new();
    for target in targets {
        if live_keys.contains(target.key.as_str()) {
            continue;
        }
        if excluded_keys.contains(target.key.as_str()) {
            continue;
        }
        if failed_identity_fingerprints.contains(&target.fingerprint) {
            continue;
        }
        start_targets.push(target.clone());
    }

    PoolReconcileResult {
        retire_keys,
        start_targets,
        drop_cache_keys,
    }
}

/// Synchronous remote-connection slot accounting. Mirrors
/// `acquireRemoteSlot` / `releaseRemoteSlot` / `grantNextSlot`.
#[derive(Debug)]
pub struct RemoteSlotPool {
    max_slots: usize,
    holders: HashSet<String>,
    waiters: Vec<String>,
}

impl RemoteSlotPool {
    pub fn new(max_slots: usize) -> Self {
        Self {
            max_slots: max_slots.max(1),
            holders: HashSet::new(),
            waiters: Vec::new(),
        }
    }

    pub fn max_slots(&self) -> usize {
        self.max_slots
    }

    pub fn slots_in_use(&self) -> usize {
        self.holders.len()
    }

    pub fn waiters(&self) -> &[String] {
        &self.waiters
    }

    pub fn holds_slot(&self, key: &str) -> bool {
        self.holders.contains(key)
    }

    /// Acquires a slot for `key`. Returns true when granted immediately.
    pub fn try_acquire(&mut self, key: &str) -> bool {
        if self.holders.contains(key) {
            return true;
        }
        if self.holders.len() < self.max_slots {
            self.holders.insert(key.to_string());
            return true;
        }
        if !self.waiters.iter().any(|w| w == key) {
            self.waiters.push(key.to_string());
        }
        false
    }

    /// Releases `key`'s slot and grants the next waiter FIFO.
    /// Returns the waiter key that was granted a slot, if any.
    pub fn release(&mut self, key: &str, valid_keys: &HashSet<String>) -> Option<String> {
        if !self.holders.remove(key) {
            return None;
        }
        while self.holders.len() < self.max_slots && !self.waiters.is_empty() {
            let next = self.waiters.remove(0);
            if valid_keys.is_empty() || valid_keys.contains(&next) {
                self.holders.insert(next.clone());
                return Some(next);
            }
        }
        None
    }

    /// Drops `key` from the waiter queue without granting.
    pub fn cancel_waiter(&mut self, key: &str) {
        self.waiters.retain(|w| w != key);
    }
}

/// Result of accepting one polled bootstrap into the pool cache.
#[derive(Debug, Clone)]
pub struct PoolAcceptResult {
    /// Snapshot to cache (post-hold projection).
    pub snapshot: PooledSnapshot,
    /// False when the incoming snapshot is equivalent to the cached one.
    pub published: bool,
    pub attention: AttentionAdvance,
    /// Surviving holds — thread into the next call.
    pub holds: OrganizationHolds,
}

/// Accepts one polled bootstrap for `key`, exactly like
/// `acceptSnapshot(_:key:name:)` minus `@Published` publishing.
pub fn accept_pooled_snapshot(
    cached: Option<&PooledSnapshot>,
    incoming: &PooledSnapshot,
    holds: &OrganizationHolds,
    latch: &AttentionLatch,
    is_foreground: bool,
    now_ms: u64,
) -> PoolAcceptResult {
    let (projected, surviving_holds) = holds.apply_to(incoming, now_ms);
    if let Some(cached) = cached {
        if cached.is_equivalent_to(&projected) {
            return PoolAcceptResult {
                snapshot: cached.clone(),
                published: false,
                attention: AttentionAdvance {
                    latch: latch.clone(),
                    notify_session_ids: Vec::new(),
                    has_attention: !latch.notified_session_ids.is_empty(),
                },
                holds: surviving_holds,
            };
        }
    }
    let blocked_ids: HashSet<String> = projected
        .sessions
        .iter()
        .filter(|s| s.needs_attention())
        .map(|s| s.id.clone())
        .collect();
    let attention = advance_attention(latch, &blocked_ids, is_foreground);
    PoolAcceptResult {
        snapshot: projected,
        published: true,
        attention,
        holds: surviving_holds,
    }
}

/// Identity check for a freshly bootstrapped connection. Returns the
/// fingerprint to latch, or None when the identity matches. Fail closed.
pub fn identity_mismatch_fingerprint(
    expected_host_id: Option<&str>,
    reported_host_id: &str,
    fingerprint: &str,
) -> Option<String> {
    if expected_host_id.is_some_and(|expected| expected != reported_host_id) {
        Some(fingerprint.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(key: &str, fingerprint: &str) -> WorkspacePoolTarget {
        WorkspacePoolTarget {
            key: key.into(),
            name: key.into(),
            transport_kind: "direct".into(),
            is_remote: true,
            expected_host_id: None,
            fingerprint: fingerprint.into(),
        }
    }

    #[test]
    fn backoff_doubles_and_caps() {
        assert_eq!(backoff_delay_ms(1, 5_000, 300_000), 5_000);
        assert_eq!(backoff_delay_ms(2, 5_000, 300_000), 10_000);
        assert_eq!(backoff_delay_ms(3, 5_000, 300_000), 20_000);
        // Exponent clamped at 16: 5000 * 2^16 = 327,680,000 -> capped
        assert_eq!(backoff_delay_ms(100, 5_000, 300_000), 300_000);
    }

    #[test]
    fn relative_order_replaces_only_named_positions() {
        let values = vec!["a", "b", "c", "d"];
        let out = applying_relative_order(&["c", "a"], &values, |v| v);
        assert_eq!(out, vec!["c", "b", "a", "d"]);
        // Unknown ids ignored; single preferred value stays in place
        let out = applying_relative_order(&["zzz", "b"], &values, |v| v);
        assert_eq!(out, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn snapshot_equivalence_ignores_capture_timestamp() {
        let a = PooledSnapshot {
            projects: vec![],
            sessions: vec![],
            captured_at_unix_ms: 1000,
        };
        let b = a.with_captured_at(2000);
        assert!(a.is_equivalent_to(&b));
    }

    #[test]
    fn attention_latch_seeds_silently_then_notifies_on_edge() {
        let latch = AttentionLatch::default();
        let blocked: HashSet<String> = ["s1".into()].into_iter().collect();
        // First contact: silent seed
        let adv = advance_attention(&latch, &blocked, false);
        assert!(adv.notify_session_ids.is_empty());
        assert!(adv.has_attention);
        // Still blocked: no re-notify
        let adv2 = advance_attention(&adv.latch, &blocked, false);
        assert!(adv2.notify_session_ids.is_empty());
        // New blocked session: notify exactly the edge
        let blocked2: HashSet<String> = ["s1".into(), "s2".into()].into_iter().collect();
        let adv3 = advance_attention(&adv2.latch, &blocked2, false);
        assert_eq!(adv3.notify_session_ids, vec!["s2".to_string()]);
        // Foreground: never notify
        let adv4 = advance_attention(&adv3.latch, &blocked2, true);
        assert!(adv4.notify_session_ids.is_empty());
    }

    #[test]
    fn reconcile_retires_vanished_and_fingerprint_changed() {
        let entries: HashMap<String, String> = [
            ("k1".to_string(), "fp1".to_string()),
            ("k2".to_string(), "old".to_string()),
        ]
        .into_iter()
        .collect();
        let targets = vec![
            target("k1", "fp1"),
            target("k2", "new"),
            target("k3", "fp3"),
        ];
        let result = reconcile_pool_targets(
            &entries,
            &targets,
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(result.retire_keys, vec!["k2".to_string()]);
        assert_eq!(result.start_targets.len(), 2);
        assert!(result.start_targets.iter().any(|t| t.key == "k2"));
        assert!(result.start_targets.iter().any(|t| t.key == "k3"));
    }

    #[test]
    fn slot_pool_grants_fifo_and_drops_stale_waiters() {
        let mut pool = RemoteSlotPool::new(1);
        assert!(pool.try_acquire("a"));
        assert!(!pool.try_acquire("b"));
        assert!(!pool.try_acquire("c"));
        // Release grants FIFO
        let granted = pool.release("a", &HashSet::new());
        assert_eq!(granted.as_deref(), Some("b"));
        assert!(pool.holds_slot("b"));
        // Stale waiter (not in valid_keys) is dropped without a slot
        let mut pool2 = RemoteSlotPool::new(1);
        assert!(pool2.try_acquire("x"));
        assert!(!pool2.try_acquire("stale"));
        assert!(!pool2.try_acquire("live"));
        let valid: HashSet<String> = ["live".into()].into_iter().collect();
        let granted = pool2.release("x", &valid);
        assert_eq!(granted.as_deref(), Some("live"));
    }

    #[test]
    fn accept_snapshot_skips_publish_when_equivalent() {
        let snap = PooledSnapshot {
            projects: vec![],
            sessions: vec![],
            captured_at_unix_ms: 1000,
        };
        let incoming = snap.with_captured_at(2000);
        let result = accept_pooled_snapshot(
            Some(&snap),
            &incoming,
            &OrganizationHolds::default(),
            &AttentionLatch::default(),
            false,
            3000,
        );
        assert!(!result.published);
        assert_eq!(result.snapshot.captured_at_unix_ms, 1000);
    }

    #[test]
    fn identity_mismatch_fails_closed() {
        assert_eq!(
            identity_mismatch_fingerprint(Some("h1"), "h2", "fp"),
            Some("fp".to_string())
        );
        assert_eq!(identity_mismatch_fingerprint(Some("h1"), "h1", "fp"), None);
        assert_eq!(identity_mismatch_fingerprint(None, "h9", "fp"), None);
    }

    #[test]
    fn organization_hold_pins_reorder_until_confirmed() {
        let snap = PooledSnapshot {
            projects: vec![
                PooledProject {
                    id: "p1".into(),
                    parent_project_id: None,
                    session_order: None,
                },
                PooledProject {
                    id: "p2".into(),
                    parent_project_id: None,
                    session_order: None,
                },
            ],
            sessions: vec![],
            captured_at_unix_ms: 0,
        };
        let holds = OrganizationHolds::default();
        let (holds, pinned) =
            holds.hold_project_order(&snap, None, vec!["p2".into(), "p1".into()], 1000);
        assert_eq!(pinned.projects[0].id, "p2");
        // Host bootstrap with old order: hold still wins
        let (projected, holds) = holds.apply_to(&snap, 2000);
        assert_eq!(projected.projects[0].id, "p2");
        assert!(holds.project_hold.is_some());
        // Host confirms the new order: hold releases
        let confirmed = snap.applying_project_order(None, &["p2".to_string(), "p1".to_string()]);
        let (projected, holds) = holds.apply_to(&confirmed, 2000);
        assert_eq!(projected.projects[0].id, "p2");
        assert!(holds.project_hold.is_none());
        // Timeout releases host truth
        let (holds, _) = OrganizationHolds::default().hold_project_order(
            &snap,
            None,
            vec!["p2".into(), "p1".into()],
            1000,
        );
        let timeout_ms = 1000 + (policy::ORGANIZATION_HOLD_SECS * 1000.0) as u64 + 1;
        let (projected, holds) = holds.apply_to(&snap, timeout_ms);
        assert_eq!(projected.projects[0].id, "p1");
        assert!(holds.project_hold.is_none());
    }

    // ------------------------------------------------------------------
    // Ported from clients/supercli-app/test/workspace_pool_test.dart
    // (23 Dart tests; the 9 above covered a subset — these close the gap).
    // ------------------------------------------------------------------

    fn pooled_session(
        id: &str,
        activity: &str,
        status: &str,
        archived: bool,
        title: &str,
    ) -> PooledSession {
        PooledSession {
            id: id.into(),
            project_id: "project".into(),
            title: title.into(),
            command: "claude".into(),
            status: status.into(),
            activity: activity.into(),
            archived,
        }
    }

    #[test]
    fn accept_snapshot_caches_and_publishes_on_first_contact() {
        // Dart: 'accepting a snapshot caches it and publishes'.
        let incoming = PooledSnapshot {
            projects: vec![],
            sessions: vec![pooled_session("s1", "blocked", "running", false, "s1")],
            captured_at_unix_ms: 1000,
        };
        let result = accept_pooled_snapshot(
            None,
            &incoming,
            &OrganizationHolds::default(),
            &AttentionLatch::default(),
            false,
            2000,
        );
        assert!(result.published);
        assert_eq!(
            result
                .snapshot
                .sessions
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            vec!["s1"]
        );
        assert!(result.attention.has_attention);
    }

    #[test]
    fn backoff_exponent_clamps_at_16_and_caps() {
        // Dart: 'exponent clamps at 16 and result caps' with the 30/240
        // custom base/cap: [30, 60, 120, 240, 240].
        let delays: Vec<u64> = (1..=5).map(|f| backoff_delay_ms(f, 30, 240)).collect();
        assert_eq!(delays, vec![30, 60, 120, 240, 240]);
        assert!(
            delays[2] > delays[0] * 3 / 2,
            "backoff grows faster than linear"
        );
        // Default policy constants.
        assert_eq!(
            backoff_delay_ms(100, policy::BACKOFF_BASE_MS, policy::BACKOFF_CAP_MS),
            policy::BACKOFF_CAP_MS
        );
        assert_eq!(
            backoff_delay_ms(17, policy::BACKOFF_BASE_MS, policy::BACKOFF_CAP_MS),
            backoff_delay_ms(100, policy::BACKOFF_BASE_MS, policy::BACKOFF_CAP_MS),
            "exponent clamped at 16"
        );
        assert_eq!(
            backoff_delay_ms(1, policy::BACKOFF_BASE_MS, policy::BACKOFF_CAP_MS),
            policy::BACKOFF_BASE_MS
        );
    }

    #[test]
    fn slot_pool_local_targets_bypass_remote_cap() {
        // Dart: 'local targets are not subject to the remote cap'.
        // The pool itself is remote-only; callers gate on `is_remote`
        // (mirrors runEntryLoop's `entry.target.isRemote` guard).
        let mut slots = RemoteSlotPool::new(1);
        assert!(slots.try_acquire("ssh-b"), "one remote holds the only slot");
        let acquire_if_remote = |slots: &mut RemoteSlotPool, key: &str, is_remote: bool| {
            if is_remote {
                slots.try_acquire(key)
            } else {
                true
            }
        };
        assert!(
            acquire_if_remote(&mut slots, "local-a", false),
            "local targets never queue"
        );
        assert!(
            !acquire_if_remote(&mut slots, "ssh-c", true),
            "second remote waits"
        );
    }

    #[test]
    fn slot_pool_retired_waiter_resumes_without_slot() {
        // Dart: 'retired waiter resumes without a slot'.
        let mut slots = RemoteSlotPool::new(1);
        assert!(slots.try_acquire("a"));
        assert!(!slots.try_acquire("b"));
        slots.cancel_waiter("b");
        assert_eq!(slots.release("a", &HashSet::new()), None);
        assert!(!slots.holds_slot("a"));
    }

    #[test]
    fn slot_pool_max_slots_clamps_to_at_least_one() {
        // Dart: 'max slots clamps to at least one'.
        assert_eq!(RemoteSlotPool::new(0).max_slots(), 1);
    }

    #[test]
    fn attention_foreground_never_notifies() {
        // Dart: 'foreground workspace never notifies'.
        let latch = AttentionLatch {
            seeded: true,
            notified_session_ids: HashSet::new(),
        };
        let blocked: HashSet<String> = ["s1".into()].into_iter().collect();
        let adv = advance_attention(&latch, &blocked, true);
        assert!(adv.notify_session_ids.is_empty());
        assert!(adv.has_attention);
        // The foreground latch still records blocked, so leaving the scope
        // does not replay it as new.
        assert!(adv.latch.notified_session_ids.contains("s1"));
    }

    #[test]
    fn attention_archived_sessions_never_raise() {
        // Dart: 'archived sessions never raise attention'.
        assert!(!pooled_session("s1", "blocked", "running", true, "s1").needs_attention());
        assert!(pooled_session("s1", "blocked", "running", false, "s1").needs_attention());
        assert!(!pooled_session("s1", "idle", "running", false, "s1").needs_attention());
        assert!(!pooled_session("s1", "blocked", "exited", false, "s1").needs_attention());
    }

    #[test]
    fn attention_title_falls_back_to_command() {
        // Dart: 'empty title falls back to command for notification text'.
        assert_eq!(
            pooled_session("s1", "blocked", "running", false, "").attention_title(),
            "claude"
        );
        assert_eq!(
            pooled_session("s1", "blocked", "running", false, "My title").attention_title(),
            "My title"
        );
    }

    #[test]
    fn reconcile_lend_retires_entry_but_keeps_cache() {
        // Dart: 'lend retires the pool entry but keeps the cache'.
        let entries: HashMap<String, String> = [("a".to_string(), "fp:a".to_string())]
            .into_iter()
            .collect();
        let targets = vec![WorkspacePoolTarget {
            key: "a".into(),
            name: "Workspace a".into(),
            transport_kind: "local".into(),
            is_remote: false,
            expected_host_id: None,
            fingerprint: "fp:a".into(),
        }];
        let excluded: HashSet<String> = ["a".into()].into_iter().collect();
        let cached: HashSet<String> = ["a".into()].into_iter().collect();
        let result =
            reconcile_pool_targets(&entries, &targets, &excluded, &HashSet::new(), &cached);
        assert_eq!(result.retire_keys, vec!["a".to_string()]);
        assert!(
            result.drop_cache_keys.is_empty(),
            "excluded (runtime-served) keys keep their cache"
        );
        assert!(
            result.start_targets.is_empty(),
            "lent workspace must not be re-polled"
        );

        // The runtime lets go: pooling resumes on the next reconcile.
        let resumed = reconcile_pool_targets(
            &HashMap::new(),
            &targets,
            &HashSet::new(),
            &HashSet::new(),
            &cached,
        );
        assert_eq!(
            resumed
                .start_targets
                .iter()
                .map(|t| t.key.as_str())
                .collect::<Vec<_>>(),
            vec!["a"]
        );
    }

    #[test]
    fn reconcile_identity_latched_fingerprint_not_repolled() {
        // Dart: 'identity-latched fingerprint is not re-polled'.
        let targets = vec![WorkspacePoolTarget {
            key: "a".into(),
            name: "Workspace a".into(),
            transport_kind: "local".into(),
            is_remote: false,
            expected_host_id: None,
            fingerprint: "fp:a".into(),
        }];
        let latched: HashSet<String> = ["fp:a".into()].into_iter().collect();
        let result = reconcile_pool_targets(
            &HashMap::new(),
            &targets,
            &HashSet::new(),
            &latched,
            &HashSet::new(),
        );
        assert!(result.start_targets.is_empty());
    }

    #[test]
    fn policy_constants_mirror_swift_initializer() {
        // Dart: 'defaults mirror the Swift initializer'.
        assert_eq!(policy::POLL_INTERVAL_MS, 25_000);
        assert_eq!(policy::BACKOFF_BASE_MS, 5_000);
        assert_eq!(policy::BACKOFF_CAP_MS, 300_000);
        assert_eq!(policy::MAINTENANCE_INTERVAL_MS, 30_000);
        assert_eq!(policy::IMMEDIATE_REFRESH_THROTTLE_SECS, 2.0);
        assert_eq!(policy::MAX_LIVE_REMOTE_CONNECTIONS, 4);
        assert_eq!(policy::ORGANIZATION_HOLD_SECS, 15.0);
    }
}
