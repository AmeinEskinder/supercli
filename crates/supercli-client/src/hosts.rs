//! Multi-Host connection registry.
//!
//! The Dioxus launchers keep every paired Host connected at once: switching
//! Hosts is a view change, never a teardown. [`HostRegistry`] owns the live
//! [`HostClient`] per Host plus its last bootstrap snapshot, so switching
//! back to a Host shows its cached sessions instantly while a background
//! refresh revalidates them.
//!
//! The registry is deliberately UI-free: each launcher keeps its own
//! per-Host view state (open session, terminal threads, transcript) keyed
//! by the same host id.

use std::collections::HashMap;

use crate::dto::BootstrapSnapshot;
use crate::pairing::PairedHostRecord;
use crate::transport::HostClient;

/// One live Host connection: the client plus the last bootstrap snapshot.
///
/// The snapshot cache is what makes Host switching feel instant — the new
/// view renders the cached sessions/approvals immediately, and the launcher
/// revalidates with a background `bootstrap()`.
#[derive(Clone)]
pub struct LiveHost {
    /// The paired record (id, name, endpoint, device).
    pub record: PairedHostRecord,
    /// The authenticated client for this Host's `/mobile` endpoint.
    pub client: HostClient,
    /// Last successful bootstrap; `None` until the first one lands.
    pub snapshot: Option<BootstrapSnapshot>,
    /// Last per-Host error (e.g. a failed background refresh), if any.
    pub last_error: Option<String>,
}

/// Owns every live Host connection and tracks which one is in view.
///
/// Invariants:
/// - `connect` is idempotent: connecting an already-connected Host just
///   switches to it, without tearing down or rebuilding anything.
/// - `switch` never touches connections; it only moves the active pointer.
/// - `disconnect` tears down exactly one Host; the others keep running.
#[derive(Clone, Default)]
pub struct HostRegistry {
    hosts: HashMap<String, LiveHost>,
    active_host_id: Option<String>,
}

impl HostRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Connect a Host (or switch to it if it is already connected) and make
    /// it active. The first bootstrap snapshot is stored when available.
    pub fn connect(
        &mut self,
        record: PairedHostRecord,
        client: HostClient,
        snapshot: Option<BootstrapSnapshot>,
    ) {
        let id = record.host_id.clone();
        self.hosts
            .entry(id.clone())
            .and_modify(|h| {
                // Already connected: keep the live client and its cached
                // snapshot; only refresh the record metadata.
                h.record = record.clone();
                if snapshot.is_some() {
                    h.snapshot = snapshot.clone();
                }
                h.last_error = None;
            })
            .or_insert(LiveHost {
                record,
                client,
                snapshot,
                last_error: None,
            });
        self.active_host_id = Some(id);
    }

    /// Switch the active Host without touching any connection.
    /// Returns `false` when `host_id` is not connected.
    pub fn switch(&mut self, host_id: &str) -> bool {
        if self.hosts.contains_key(host_id) {
            self.active_host_id = Some(host_id.to_string());
            true
        } else {
            false
        }
    }

    /// Tear down exactly one Host connection. When it was the active Host,
    /// another connected Host becomes active if one remains.
    /// Returns `false` when `host_id` was not connected.
    pub fn disconnect(&mut self, host_id: &str) -> bool {
        if self.hosts.remove(host_id).is_none() {
            return false;
        }
        if self.active_host_id.as_deref() == Some(host_id) {
            self.active_host_id = self.hosts.keys().next().cloned();
        }
        true
    }

    /// Tear down every connection.
    pub fn disconnect_all(&mut self) {
        self.hosts.clear();
        self.active_host_id = None;
    }

    /// The active Host, if any.
    pub fn active(&self) -> Option<&LiveHost> {
        self.active_host_id
            .as_deref()
            .and_then(|id| self.hosts.get(id))
    }

    /// The active Host, mutably.
    pub fn active_mut(&mut self) -> Option<&mut LiveHost> {
        let id = self.active_host_id.clone()?;
        self.hosts.get_mut(&id)
    }

    /// The active Host's id, if any.
    pub fn active_id(&self) -> Option<&str> {
        self.active_host_id.as_deref()
    }

    /// Look up a connected Host by id.
    pub fn get(&self, host_id: &str) -> Option<&LiveHost> {
        self.hosts.get(host_id)
    }

    /// Look up a connected Host by id, mutably.
    pub fn get_mut(&mut self, host_id: &str) -> Option<&mut LiveHost> {
        self.hosts.get_mut(host_id)
    }

    /// Whether `host_id` currently has a live connection.
    pub fn contains(&self, host_id: &str) -> bool {
        self.hosts.contains_key(host_id)
    }

    /// Ids of all connected Hosts, in no particular order.
    pub fn host_ids(&self) -> Vec<String> {
        self.hosts.keys().cloned().collect()
    }

    /// Store a fresh bootstrap snapshot for a connected Host.
    /// Returns `false` when `host_id` is not connected.
    pub fn set_snapshot(&mut self, host_id: &str, snapshot: BootstrapSnapshot) -> bool {
        match self.hosts.get_mut(host_id) {
            Some(h) => {
                h.snapshot = Some(snapshot);
                h.last_error = None;
                true
            }
            None => false,
        }
    }

    /// Record a per-Host error (e.g. a failed background refresh).
    /// Returns `false` when `host_id` is not connected.
    pub fn set_error(&mut self, host_id: &str, error: String) -> bool {
        match self.hosts.get_mut(host_id) {
            Some(h) => {
                h.last_error = Some(error);
                true
            }
            None => false,
        }
    }

    /// Number of live connections.
    pub fn len(&self) -> usize {
        self.hosts.len()
    }

    /// Whether no Host is connected.
    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_client() -> HostClient {
        // Construction performs no I/O; the endpoint never gets called.
        HostClient::new("http://127.0.0.1:1", "dummy-token").expect("dummy client")
    }

    fn record(id: &str) -> PairedHostRecord {
        PairedHostRecord {
            host_id: id.to_string(),
            name: format!("{id} host"),
            endpoint: "https://host:8443".to_string(),
            controller_device_id: "device-1".to_string(),
            paired_at_unix_ms: 0,
            certificate_fingerprint: Some("fp".to_string()),
            remote_server_port: None,
            remote_server_certificate_fingerprint: None,
            link_enabled: None,
        }
    }

    fn empty_snapshot() -> BootstrapSnapshot {
        // Every field has a serde default, so `{}` deserializes.
        serde_json::from_value(serde_json::json!({})).expect("empty snapshot")
    }

    #[test]
    fn connect_makes_host_active() {
        let mut reg = HostRegistry::new();
        assert!(reg.is_empty());
        reg.connect(record("a"), dummy_client(), None);
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.active_id(), Some("a"));
        assert!(reg.contains("a"));
        assert_eq!(reg.active().unwrap().record.host_id, "a");
    }

    #[test]
    fn connect_second_host_keeps_first_connected() {
        let mut reg = HostRegistry::new();
        reg.connect(record("a"), dummy_client(), None);
        reg.connect(record("b"), dummy_client(), None);
        assert_eq!(reg.len(), 2);
        assert_eq!(reg.active_id(), Some("b"));
        // The first Host's connection survived: switching back is instant
        // and does not rebuild anything.
        assert!(reg.switch("a"));
        assert_eq!(reg.active_id(), Some("a"));
        assert!(reg.contains("b"));
    }

    #[test]
    fn reconnect_is_idempotent_and_just_switches() {
        let mut reg = HostRegistry::new();
        reg.connect(record("a"), dummy_client(), None);
        reg.connect(record("b"), dummy_client(), None);
        reg.connect(record("a"), dummy_client(), None);
        assert_eq!(reg.len(), 2);
        assert_eq!(reg.active_id(), Some("a"));
    }

    #[test]
    fn switch_unknown_host_fails_without_moving_active() {
        let mut reg = HostRegistry::new();
        reg.connect(record("a"), dummy_client(), None);
        assert!(!reg.switch("zzz"));
        assert_eq!(reg.active_id(), Some("a"));
    }

    #[test]
    fn disconnect_removes_only_that_host() {
        let mut reg = HostRegistry::new();
        reg.connect(record("a"), dummy_client(), None);
        reg.connect(record("b"), dummy_client(), None);
        assert!(reg.disconnect("b"));
        assert_eq!(reg.len(), 1);
        assert!(reg.contains("a"));
        assert!(!reg.disconnect("b"));
    }

    #[test]
    fn disconnect_active_falls_back_to_another_host() {
        let mut reg = HostRegistry::new();
        reg.connect(record("a"), dummy_client(), None);
        reg.connect(record("b"), dummy_client(), None);
        assert!(reg.disconnect("b"));
        assert_eq!(reg.active_id(), Some("a"));
    }

    #[test]
    fn disconnect_last_host_clears_active() {
        let mut reg = HostRegistry::new();
        reg.connect(record("a"), dummy_client(), None);
        assert!(reg.disconnect("a"));
        assert!(reg.is_empty());
        assert_eq!(reg.active_id(), None);
        assert!(reg.active().is_none());
    }

    #[test]
    fn snapshot_and_error_round_trip() {
        let mut reg = HostRegistry::new();
        reg.connect(record("a"), dummy_client(), None);
        assert!(reg.active().unwrap().snapshot.is_none());
        assert!(!reg.set_snapshot("zzz", empty_snapshot()));
        // A snapshot is enough to exercise the cache plumbing.
        assert!(reg.set_snapshot("a", empty_snapshot()));
        assert!(reg.active().unwrap().snapshot.is_some());
        assert!(!reg.set_error("zzz", "boom".to_string()));
        assert!(reg.set_error("a", "boom".to_string()));
        assert_eq!(reg.active().unwrap().last_error.as_deref(), Some("boom"));
        // A fresh snapshot clears the error.
        assert!(reg.set_snapshot("a", empty_snapshot()));
        assert!(reg.active().unwrap().last_error.is_none());
    }
}
