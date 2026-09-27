//! Pure session-cache bookkeeping.
//!
//! Port of the UI-agnostic structs in `TerminalSessionCache.swift`
//! (`ios/SupercliIOS`): `SessionLRUIndex`, `TerminalVisibilityLeaseTracker`,
//! and `TerminalStreamLeaseTracker`. The `@MainActor` cache itself (which
//! owns ghostty surfaces and UIKit views) is not portable and stays out.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Pure LRU bookkeeping for the per-session terminal cache. Generic over the
/// payload so the ordering/eviction/prune logic is unit-testable without ever
/// touching renderers or surfaces.
#[derive(Debug, Clone)]
pub struct SessionLruIndex<Entry> {
    capacity: usize,
    /// Least-recently-used first, most-recently-used last.
    order: Vec<String>,
    storage: HashMap<String, Entry>,
}

impl<Entry> SessionLruIndex<Entry> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            order: Vec::new(),
            storage: HashMap::new(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn keys(&self) -> &[String] {
        &self.order
    }

    /// Returns the entry and marks it most-recently-used.
    pub fn lookup(&mut self, id: &str) -> Option<&Entry> {
        if self.storage.contains_key(id) {
            self.touch(id);
            self.storage.get(id)
        } else {
            None
        }
    }

    /// Reads without touching recency (for inspection/iteration).
    pub fn peek(&self, id: &str) -> Option<&Entry> {
        self.storage.get(id)
    }

    /// Inserts (or replaces) an entry as most-recently-used. Returns the
    /// entries evicted to stay within capacity — never the one just inserted.
    pub fn insert(&mut self, id: String, entry: Entry) -> Vec<(String, Entry)> {
        self.storage.insert(id.clone(), entry);
        self.touch(&id);
        let mut evicted = Vec::new();
        while self.order.len() > self.capacity {
            let oldest = self.order.remove(0);
            if let Some(dropped) = self.storage.remove(&oldest) {
                evicted.push((oldest, dropped));
            }
        }
        evicted
    }

    pub fn remove(&mut self, id: &str) -> Option<Entry> {
        self.order.retain(|k| k != id);
        self.storage.remove(id)
    }

    pub fn remove_all(&mut self) -> Vec<(String, Entry)> {
        let ids: Vec<String> = self.order.to_vec();
        let removed: Vec<(String, Entry)> = ids
            .into_iter()
            .filter_map(|id| self.storage.remove(&id).map(|e| (id, e)))
            .collect();
        self.order.clear();
        removed
    }

    /// Drops every entry whose id is not in `ids` (session killed on the
    /// host), except `keeping` (the on-screen session must not be torn down
    /// under a transiently stale/empty session list).
    pub fn retain_only(
        &mut self,
        ids: &HashSet<String>,
        keeping: Option<&str>,
    ) -> Vec<(String, Entry)> {
        let doomed: Vec<String> = self
            .order
            .iter()
            .filter(|id| Some(id.as_str()) != keeping && !ids.contains(*id))
            .cloned()
            .collect();
        doomed
            .into_iter()
            .filter_map(|id| self.remove(&id).map(|e| (id, e)))
            .collect()
    }

    /// Drops everything except `id` (memory pressure: keep only the visible
    /// session's terminal).
    pub fn remove_all_except(&mut self, id: Option<&str>) -> Vec<(String, Entry)> {
        let doomed: Vec<String> = self
            .order
            .iter()
            .filter(|k| Some(k.as_str()) != id)
            .cloned()
            .collect();
        doomed
            .into_iter()
            .filter_map(|k| self.remove(&k).map(|e| (k, e)))
            .collect()
    }

    fn touch(&mut self, id: &str) {
        self.order.retain(|k| k != id);
        self.order.push(id.to_string());
    }
}

/// Token-gated ownership for the one terminal currently on screen. The token
/// distinguishes two lifetimes even when they render the same session (notably
/// a connection-epoch remount), so the old disappear cannot hide the
/// replacement from prune/memory-pressure protection.
#[derive(Debug, Clone)]
pub struct TerminalVisibilityLeaseTracker<Token: Eq + Hash + Clone> {
    session_id: Option<String>,
    owner: Option<Token>,
}

impl<Token: Eq + Hash + Clone> Default for TerminalVisibilityLeaseTracker<Token> {
    fn default() -> Self {
        Self {
            session_id: None,
            owner: None,
        }
    }
}

impl<Token: Eq + Hash + Clone> TerminalVisibilityLeaseTracker<Token> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn acquire(&mut self, session_id: String, owner: Token) {
        self.session_id = Some(session_id);
        self.owner = Some(owner);
    }

    /// Returns true only when both session and mount token match.
    pub fn release(&mut self, session_id: &str, owner: &Token) -> bool {
        if self.session_id.as_deref() == Some(session_id) && self.owner.as_ref() == Some(owner) {
            self.session_id = None;
            self.owner = None;
            true
        } else {
            false
        }
    }
}

/// Reference-counts renderer streaming by mounted lifetime. A set makes
/// repeated appear callbacks idempotent; a late disappear from an old mount
/// only releases its own token.
#[derive(Debug, Clone)]
pub struct TerminalStreamLeaseTracker<Token: Eq + Hash + Clone> {
    owners: HashSet<Token>,
}

impl<Token: Eq + Hash + Clone> Default for TerminalStreamLeaseTracker<Token> {
    fn default() -> Self {
        Self {
            owners: HashSet::new(),
        }
    }
}

impl<Token: Eq + Hash + Clone> TerminalStreamLeaseTracker<Token> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }

    /// True only for the transition that should start the renderer.
    pub fn acquire(&mut self, owner: Token) -> bool {
        let inserted = self.owners.insert(owner);
        inserted && self.owners.len() == 1
    }

    /// True only for the transition that should stop the renderer.
    pub fn release(&mut self, owner: &Token) -> bool {
        if self.owners.remove(owner) {
            self.owners.is_empty()
        } else {
            false
        }
    }

    pub fn remove_all(&mut self) {
        self.owners.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lru_lookup_marks_most_recently_used() {
        let mut index = SessionLruIndex::new(3);
        index.insert("a".to_string(), 1);
        index.insert("b".to_string(), 2);
        index.insert("c".to_string(), 3);
        assert_eq!(index.lookup("a"), Some(&1));
        assert_eq!(index.keys(), &["b", "c", "a"]);
    }

    #[test]
    fn lru_evicts_oldest_beyond_capacity() {
        let mut index = SessionLruIndex::new(2);
        index.insert("a".to_string(), 1);
        index.insert("b".to_string(), 2);
        let evicted = index.insert("c".to_string(), 3);
        assert_eq!(evicted, vec![("a".to_string(), 1)]);
        assert_eq!(index.keys(), &["b", "c"]);
    }

    #[test]
    fn lru_replace_does_not_evict_itself() {
        let mut index = SessionLruIndex::new(2);
        index.insert("a".to_string(), 1);
        index.insert("b".to_string(), 2);
        let evicted = index.insert("a".to_string(), 10);
        assert!(evicted.is_empty());
        assert_eq!(index.lookup("a"), Some(&10));
    }

    #[test]
    fn lru_retain_only_spares_keeping() {
        let mut index = SessionLruIndex::new(5);
        for (id, v) in [("a", 1), ("b", 2), ("c", 3)] {
            index.insert(id.to_string(), v);
        }
        let live: HashSet<String> = ["a".to_string()].into_iter().collect();
        let removed = index.retain_only(&live, Some("b"));
        let mut removed_ids: Vec<String> = removed.into_iter().map(|(id, _)| id).collect();
        removed_ids.sort();
        // "c" is gone (not live); "b" is spared as `keeping`.
        assert_eq!(removed_ids, vec!["c".to_string()]);
        assert_eq!(index.len(), 2);
    }

    #[test]
    fn lru_remove_all_except_keeps_visible() {
        let mut index = SessionLruIndex::new(5);
        for (id, v) in [("a", 1), ("b", 2), ("c", 3)] {
            index.insert(id.to_string(), v);
        }
        let removed = index.remove_all_except(Some("b"));
        assert_eq!(removed.len(), 2);
        assert_eq!(index.keys(), &["b".to_string()]);
    }

    #[test]
    fn visibility_lease_requires_matching_token() {
        let mut tracker: TerminalVisibilityLeaseTracker<u64> =
            TerminalVisibilityLeaseTracker::new();
        tracker.acquire("s1".to_string(), 7);
        assert_eq!(tracker.session_id(), Some("s1"));
        // Wrong owner: no release.
        assert!(!tracker.release("s1", &8));
        assert_eq!(tracker.session_id(), Some("s1"));
        // Wrong session: no release.
        assert!(!tracker.release("s2", &7));
        assert!(tracker.release("s1", &7));
        assert_eq!(tracker.session_id(), None);
    }

    #[test]
    fn stream_lease_start_stop_transitions() {
        let mut tracker: TerminalStreamLeaseTracker<u64> = TerminalStreamLeaseTracker::new();
        // First acquire starts the renderer.
        assert!(tracker.acquire(1));
        // Second owner does not re-trigger start.
        assert!(!tracker.acquire(2));
        // Releasing one of two does not stop.
        assert!(!tracker.release(&1));
        // Releasing the last stops.
        assert!(tracker.release(&2));
        assert!(tracker.is_empty());
        // Unknown owner is a no-op.
        assert!(!tracker.release(&99));
    }
}
