//! Caller→targets approval-pair maps.
//!
//! Ported from `UnpeelStore.swift` (`SupercliStore`) — the MCP security,
//! Computer MCP access, and Browser MCP access sections. The Swift store
//! keeps four such maps (`mcpWriteApprovals`, `mcpAppOpenApprovals`,
//! `computerApprovals`, `browserApprovals`); the pair-shaped ones share one
//! approve/revoke/prune/carry discipline, factored here so every caller gets
//! identical semantics.
//!
//! Semantics (from the Swift originals):
//! - Approvals are directional and per pair: `caller -> [target]`.
//! - `approve` is idempotent: approving an already-approved pair is a no-op.
//! - `revoke` removes the pair; an emptied caller entry is dropped entirely.
//! - `prune_for_removed` drops a caller's whole entry when its session is
//!   removed.
//! - `carry` merges a snapshot's pairs for `from` into `to` (used when a
//!   session restarts under a new id), deduplicating.

use std::collections::HashMap;

/// The approval map: caller id -> ordered approved target ids.
pub type ApprovalPairs = HashMap<String, Vec<String>>;

/// Remember an approval for a caller→target pair. Idempotent.
pub fn approve_pair(map: &mut ApprovalPairs, caller: &str, target: &str) {
    let targets = map.entry(caller.to_string()).or_default();
    if !targets.iter().any(|t| t == target) {
        targets.push(target.to_string());
    }
}

/// Forget an approval for a caller→target pair. Drops the caller entry when
/// its last target is revoked. No-op when the pair was never approved.
pub fn revoke_pair(map: &mut ApprovalPairs, caller: &str, target: &str) {
    let Some(targets) = map.get_mut(caller) else {
        return;
    };
    if !targets.iter().any(|t| t == target) {
        return;
    }
    targets.retain(|t| t != target);
    if targets.is_empty() {
        map.remove(caller);
    }
}

/// Drop a caller's whole approval entry, e.g. when its session is removed.
pub fn prune_pairs_for_removed(map: &mut ApprovalPairs, caller: &str) {
    map.remove(caller);
}

/// Carry approvals from an old id to a new one (session restart): merge the
/// snapshot's pairs for `from` into `map[to]`, deduplicating. No-op when the
/// snapshot holds nothing for `from`.
pub fn carry_pairs(map: &mut ApprovalPairs, snapshot: &ApprovalPairs, from: &str, to: &str) {
    let Some(source) = snapshot.get(from) else {
        return;
    };
    if source.is_empty() {
        return;
    }
    let carried = map.entry(to.to_string()).or_default();
    for target in source {
        if !carried.iter().any(|t| t == target) {
            carried.push(target.clone());
        }
    }
}

/// True when `caller` has approved `target`.
pub fn pair_approved(map: &ApprovalPairs, caller: &str, target: &str) -> bool {
    map.get(caller)
        .is_some_and(|targets| targets.iter().any(|t| t == target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approve_is_idempotent() {
        let mut map = ApprovalPairs::new();
        approve_pair(&mut map, "a", "x");
        approve_pair(&mut map, "a", "x");
        assert_eq!(map["a"], vec!["x".to_string()]);
    }

    #[test]
    fn approve_multiple_targets_preserves_order() {
        let mut map = ApprovalPairs::new();
        approve_pair(&mut map, "a", "x");
        approve_pair(&mut map, "a", "y");
        approve_pair(&mut map, "b", "z");
        assert_eq!(map["a"], vec!["x".to_string(), "y".to_string()]);
        assert_eq!(map["b"], vec!["z".to_string()]);
    }

    #[test]
    fn revoke_removes_pair_and_drops_emptied_caller() {
        let mut map = ApprovalPairs::new();
        approve_pair(&mut map, "a", "x");
        approve_pair(&mut map, "a", "y");
        revoke_pair(&mut map, "a", "x");
        assert_eq!(map["a"], vec!["y".to_string()]);
        revoke_pair(&mut map, "a", "y");
        assert!(!map.contains_key("a"));
    }

    #[test]
    fn revoke_unknown_pair_is_noop() {
        let mut map = ApprovalPairs::new();
        approve_pair(&mut map, "a", "x");
        revoke_pair(&mut map, "a", "nope");
        revoke_pair(&mut map, "ghost", "x");
        assert_eq!(map["a"], vec!["x".to_string()]);
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn prune_drops_whole_caller_entry() {
        let mut map = ApprovalPairs::new();
        approve_pair(&mut map, "a", "x");
        approve_pair(&mut map, "b", "y");
        prune_pairs_for_removed(&mut map, "a");
        assert!(!map.contains_key("a"));
        assert!(map.contains_key("b"));
        // Pruning an absent caller is a no-op.
        prune_pairs_for_removed(&mut map, "ghost");
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn carry_merges_snapshot_pairs_deduplicated() {
        let mut map = ApprovalPairs::new();
        approve_pair(&mut map, "new", "keep");
        let mut snapshot = ApprovalPairs::new();
        approve_pair(&mut snapshot, "old", "keep");
        approve_pair(&mut snapshot, "old", "extra");
        carry_pairs(&mut map, &snapshot, "old", "new");
        assert_eq!(map["new"], vec!["keep".to_string(), "extra".to_string()]);
    }

    #[test]
    fn carry_with_empty_snapshot_is_noop() {
        let mut map = ApprovalPairs::new();
        let snapshot = ApprovalPairs::new();
        carry_pairs(&mut map, &snapshot, "old", "new");
        assert!(map.is_empty());
    }

    #[test]
    fn pair_approved_queries() {
        let mut map = ApprovalPairs::new();
        approve_pair(&mut map, "a", "x");
        assert!(pair_approved(&map, "a", "x"));
        assert!(!pair_approved(&map, "a", "y"));
        assert!(!pair_approved(&map, "b", "x"));
    }
}
