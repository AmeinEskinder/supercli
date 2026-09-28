//! Port of iOS `RemoteConnectionStore.swift` — multi-Mac pairing storage.
//!
//! Owns WHICH Mac this phone talks to. Records live in a macID-keyed
//! collection with one ACTIVE Mac at a time; switching bumps `epoch` so
//! every consumer reloads. Bearer tokens live in the platform keychain
//! (via [`TokenStore`]); the record list and active-Mac pointer live in
//! abstract key-value storage (via [`RecordStore`]).
//!
//! Two modes:
//! - [`ConnectionMode::Paired`]: the production path. A QR pairing payload
//!   is exchanged for a per-device bearer token persisted in the keychain.
//! - [`ConnectionMode::DevBridge`]: simulator/dev fallback — a localhost
//!   dev bridge, unauthenticated.
//!
//! Only an explicit "not found" keychain read prunes a record;
//! protected-data failures retain order, active identity, and device id
//! for unlock-time hydration.

use serde::{Deserialize, Serialize};

/// Persisted pairing record. Everything except the bearer token, which
/// lives in the keychain.
///
/// One record IS one workspace: every workspace has its own Host identity
/// and pairing, so the paired list is presented as "Workspaces".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairedMacRecord {
    pub mac_id: String,
    pub mac_name: String,
    /// The Host's `/mobile` endpoint URL as a string.
    pub endpoint: String,
    pub device_id: String,
    pub paired_at_unix_ms: i64,
    /// The workspace's app-color hue, refreshed from the active
    /// connection's bootstrap. `None` for records stored before this
    /// field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint_hue: Option<f64>,
    /// Lowercase hex SHA-256 of the Host's self-signed TLS leaf, as last
    /// advertised by pairing or bootstrap. `None` for older records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_server_certificate_fingerprint: Option<String>,
    /// Non-nil once the Host is known to serve TLS on `/mobile`: every
    /// Direct request then goes over pinned HTTPS. `None` is the legacy
    /// plaintext path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_tls_fingerprint: Option<String>,
}

impl PairedMacRecord {
    /// Returns true when Direct requests for this record must use pinned
    /// HTTPS (the Host is known to serve TLS on `/mobile`).
    pub fn requires_pinned_tls(&self) -> bool {
        self.direct_tls_fingerprint
            .as_deref()
            .map(|f| !f.is_empty())
            .unwrap_or(false)
    }
}

/// Typed result of one keychain read.
///
/// Mirrors Swift's `KeychainReadResult`: only an explicit "not found"
/// means the credential is gone; a temporarily-unavailable read (e.g.
/// protected data locked) must never prune pairing state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeychainReadResult<T> {
    Found(T),
    NotFound,
    TemporarilyUnavailable(String),
}

impl<T> KeychainReadResult<T> {
    pub fn value(&self) -> Option<&T> {
        match self {
            KeychainReadResult::Found(v) => Some(v),
            _ => None,
        }
    }

    pub fn unavailable_status(&self) -> Option<&str> {
        match self {
            KeychainReadResult::TemporarilyUnavailable(s) => Some(s),
            _ => None,
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> KeychainReadResult<U> {
        match self {
            KeychainReadResult::Found(v) => KeychainReadResult::Found(f(v)),
            KeychainReadResult::NotFound => KeychainReadResult::NotFound,
            KeychainReadResult::TemporarilyUnavailable(s) => {
                KeychainReadResult::TemporarilyUnavailable(s)
            }
        }
    }
}

/// Pure macID-keyed collection helpers. Replacing a record preserves list
/// order; new Macs append.
pub mod paired_mac_collection {
    use super::PairedMacRecord;

    /// Replace the record with the same macID in place (preserving order),
    /// or append when it's a new Mac.
    pub fn upserting(records: &[PairedMacRecord], record: PairedMacRecord) -> Vec<PairedMacRecord> {
        let mut out: Vec<PairedMacRecord> = records.to_vec();
        match out.iter().position(|r| r.mac_id == record.mac_id) {
            Some(i) => out[i] = record,
            None => out.push(record),
        }
        out
    }

    /// Drop exactly the record with `mac_id`; a missing id is a no-op.
    pub fn removing(records: &[PairedMacRecord], mac_id: &str) -> Vec<PairedMacRecord> {
        records
            .iter()
            .filter(|r| r.mac_id != mac_id)
            .cloned()
            .collect()
    }
}

/// Outcome of resolving persisted records against typed keychain reads.
#[derive(Debug, Clone, PartialEq)]
pub struct PairedMacHydrationResult {
    pub records: Vec<PairedMacRecord>,
    pub active_record: Option<PairedMacRecord>,
    pub active_token: Option<String>,
    pub unavailable_statuses: Vec<String>,
}

impl PairedMacHydrationResult {
    pub fn is_temporarily_unavailable(&self) -> bool {
        !self.unavailable_statuses.is_empty()
    }
}

/// Resolve persisted records against typed keychain reads.
///
/// Only an explicit [`KeychainReadResult::NotFound`] prunes a record;
/// protected-data failures retain its order, active identity, and stable
/// device id for unlock-time hydration. An empty-but-found bearer is
/// conclusively unusable but is NOT an explicit missing item, so the
/// record is retained (never rewritten as a destructive unpair).
pub mod paired_mac_hydration {
    use std::collections::HashMap;

    use super::{KeychainReadResult, PairedMacHydrationResult, PairedMacRecord};

    pub fn resolve(
        records: &[PairedMacRecord],
        preferred_active_mac_id: Option<&str>,
        read_token: impl Fn(&str) -> KeychainReadResult<String>,
    ) -> PairedMacHydrationResult {
        let mut retained: Vec<PairedMacRecord> = Vec::new();
        let mut tokens: HashMap<&str, String> = HashMap::new();
        let mut unavailable_statuses: Vec<String> = Vec::new();

        for record in records {
            match read_token(&record.mac_id) {
                KeychainReadResult::Found(token) if !token.is_empty() => {
                    tokens.insert(record.mac_id.as_str(), token);
                    retained.push(record.clone());
                }
                KeychainReadResult::Found(_) => {
                    // Empty Bearer <redacted> conclusively unusable, but not an explicit
                    // missing Keychain item. Keep the stable pairing record.
                    retained.push(record.clone());
                }
                KeychainReadResult::NotFound => {}
                KeychainReadResult::TemporarilyUnavailable(status) => {
                    retained.push(record.clone());
                    unavailable_statuses.push(status);
                }
            }
        }

        let active = preferred_active_mac_id
            .and_then(|id| retained.iter().find(|r| r.mac_id == id))
            .or_else(|| retained.first())
            .cloned();
        let active_token = active
            .as_ref()
            .and_then(|r| tokens.get(r.mac_id.as_str()).cloned());

        PairedMacHydrationResult {
            records: retained,
            active_record: active,
            active_token,
            unavailable_statuses,
        }
    }

    /// Deterministic unlock-time adoption seam. A previously unavailable
    /// Bearer <redacted> one new Direct client generation; the persisted record
    /// (including its stable device id) is reused exactly and no pairing
    /// exchange is involved. Returns the record, its token, and the
    /// bumped epoch.
    pub fn direct_activation(
        hydration: &PairedMacHydrationResult,
        current_epoch: i64,
    ) -> Option<(PairedMacRecord, String, i64)> {
        let record = hydration.active_record.clone()?;
        let token = hydration.active_token.clone()?;
        if token.is_empty() {
            return None;
        }
        Some((record, token, current_epoch.wrapping_add(1)))
    }
}

/// One-time migration outcome from the pre-multi-Mac (single fixed slot)
/// storage scheme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyPairingMigrationResult {
    NoLegacyRecord,
    Completed,
    RetryNeeded,
    TemporarilyUnavailable(String),
}

impl LegacyPairingMigrationResult {
    pub fn needs_retry(&self) -> bool {
        matches!(
            self,
            LegacyPairingMigrationResult::RetryNeeded
                | LegacyPairingMigrationResult::TemporarilyUnavailable(_)
        )
    }
}

/// When the pairing sheet must be shown: unpaired, hydration settled, and
/// no dev-bridge fallback available.
pub mod remote_pairing_presentation_policy {
    pub fn needs_pairing(
        is_paired: bool,
        keychain_hydration_pending: bool,
        dev_bridge_available: bool,
    ) -> bool {
        !is_paired && !keychain_hydration_pending && !dev_bridge_available
    }
}

/// Retry policy for the relay-fallback ladder.
pub mod relay_fallback_retry_policy {
    /// Seconds to wait after a failed relay-fallback attempt.
    pub const FAILURE_DELAY_SECS: i64 = 12;

    pub fn can_attempt(now_unix_secs: i64, retry_after_unix_secs: i64) -> bool {
        now_unix_secs >= retry_after_unix_secs
    }

    pub fn retry_after_failure_unix_secs(completed_at_unix_secs: i64) -> i64 {
        completed_at_unix_secs.saturating_add(FAILURE_DELAY_SECS)
    }
}

/// Outcome of one authenticated Direct relay-credential repair attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayCredentialRepairOutcome {
    Refreshed,
    RecoveryUnavailable,
    FetchFailed,
    InvalidResponse,
    PersistenceFailed,
}

/// Pure evaluation half of the relay-credential repair: maps a fetch
/// result to an outcome without touching the network.
pub mod relay_credential_repair {
    use super::{RelayCredentialRepairOutcome, RelayCredentials};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum RepairFetchError {
        /// The Host answered 404: it does not implement the recovery route.
        NotFound,
        Other(String),
    }

    /// Validate that fetched credentials belong to the expected Mac.
    /// (Structural check; the real keychain validity check lives in the
    /// platform layer.)
    pub fn evaluate(
        result: Result<RelayCredentials, RepairFetchError>,
        expected_mac_id: &str,
        persist: impl FnOnce(&RelayCredentials) -> bool,
    ) -> RelayCredentialRepairOutcome {
        match result {
            Err(RepairFetchError::NotFound) => RelayCredentialRepairOutcome::RecoveryUnavailable,
            Err(_) => RelayCredentialRepairOutcome::FetchFailed,
            Ok(credentials) => {
                if credentials.mac_id != expected_mac_id || !credentials.is_structurally_valid() {
                    return RelayCredentialRepairOutcome::InvalidResponse;
                }
                if !persist(&credentials) {
                    // The Host may already have rotated. Do not keep
                    // advertising the now-ambiguous old value as ready.
                    return RelayCredentialRepairOutcome::PersistenceFailed;
                }
                RelayCredentialRepairOutcome::Refreshed
            }
        }
    }
}

/// Relay credentials fetched from the Host's recovery route.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RelayCredentials {
    pub mac_id: String,
    pub relay_token: String,
    pub relay_url: String,
}

impl RelayCredentials {
    pub fn is_structurally_valid(&self) -> bool {
        !self.mac_id.is_empty() && !self.relay_token.is_empty() && !self.relay_url.is_empty()
    }
}

/// Per-Mac relay-credential version marker. Tracks whether the stored
/// relay credentials were refreshed by the current recovery scheme, or
/// marked as "recovery unavailable" (Host 404s the recovery route — keep
/// the structurally valid credential instead of retrying a mutating GET
/// every minute).
pub struct RelayCredentialRefreshMarker;

impl RelayCredentialRefreshMarker {
    pub const CURRENT_VERSION: i64 = 1;
    pub const RECOVERY_UNAVAILABLE_VERSION: i64 = -1;

    pub fn key(mac_id: &str) -> String {
        format!("supercli.ios.relayCredentialVersion.{mac_id}")
    }

    pub fn is_current(version: i64) -> bool {
        version == Self::CURRENT_VERSION
    }

    /// Returns true when the credential needs a refresh attempt.
    /// A temporarily-unavailable read never needs a refresh (the read was
    /// inconclusive); missing/invalid credentials always do.
    pub fn needs_refresh(
        credential_available: bool,
        credential_temporarily_unavailable: bool,
        stored_version: Option<i64>,
    ) -> bool {
        if credential_temporarily_unavailable {
            return false;
        }
        if !credential_available {
            return true;
        }
        match stored_version {
            Some(v) => v != Self::CURRENT_VERSION && v != Self::RECOVERY_UNAVAILABLE_VERSION,
            None => true,
        }
    }
}

/// Which Mac the client talks to.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionMode {
    /// Simulator/dev fallback: localhost dev bridge, unauthenticated.
    DevBridge,
    /// Paired with a real Mac over the LAN.
    Paired(PairedMacRecord),
}

/// Abstract bearer-token storage (the platform keychain on iOS).
pub trait TokenStore {
    fn read_token(&self, mac_id: &str) -> KeychainReadResult<String>;
    /// Returns false when the write failed; callers must not claim
    /// success (the Host may already have rotated the credential).
    fn save_token(&mut self, mac_id: &str, token: &str) -> bool;
    fn delete_token(&mut self, mac_id: &str);
}

/// Abstract record-list storage (UserDefaults on iOS).
pub trait RecordStore {
    fn load_records(&self) -> Vec<PairedMacRecord>;
    fn save_records(&mut self, records: &[PairedMacRecord]);
    fn load_active_mac_id(&self) -> Option<String>;
    fn save_active_mac_id(&mut self, mac_id: Option<&str>);
    fn load_device_id(&self) -> Option<String>;
    fn save_device_id(&mut self, device_id: &str);
}

pub const RECORDS_KEY: &str = "supercli.ios.pairedMacs";
pub const ACTIVE_MAC_ID_KEY: &str = "supercli.ios.activeMacID";
pub const LEGACY_RECORD_KEY: &str = "supercli.ios.pairedMac";
pub const DEVICE_ID_KEY: &str = "supercli.ios.deviceID";

/// How long a relay-credential rotation attempt stays suppressed after a
/// failure while Direct is healthy (seconds).
pub const RELAY_CREDENTIAL_RETRY_DELAY_SECS: i64 = 60;
/// Longer suppression when the keychain itself was unavailable.
pub const RELAY_CREDENTIAL_UNAVAILABLE_RETRY_DELAY_SECS: i64 = 15 * 60;

/// The multi-Mac connection store.
///
/// `S` is the bearer-token keychain, `R` the record-list storage. The
/// store owns mode/epoch/paired-list/active-Mac; bearer tokens never
/// leave the keychain except as short-lived locals for client
/// construction.
pub struct RemoteConnectionStore<S, R> {
    tokens: S,
    records: R,
    mode: ConnectionMode,
    /// Bumped whenever the client identity changes; consumers that
    /// capture the client at creation key their identity on this.
    epoch: i64,
    pairing_error: Option<String>,
    paired_macs: Vec<PairedMacRecord>,
    active_mac_id: Option<String>,
    using_relay: bool,
    has_relay_credentials: bool,
    relay_credentials_need_refresh: bool,
    keychain_hydration_pending: bool,
    relay_credential_fetch_retry_after_unix_secs: i64,
    relay_fallback_retry_after_unix_secs: i64,
    dev_bridge_available: bool,
}

impl<S: TokenStore, R: RecordStore> RemoteConnectionStore<S, R> {
    pub fn new(tokens: S, records: R, dev_bridge_available: bool) -> Self {
        let mut store = Self {
            tokens,
            records,
            mode: ConnectionMode::DevBridge,
            epoch: 0,
            pairing_error: None,
            paired_macs: Vec::new(),
            active_mac_id: None,
            using_relay: false,
            has_relay_credentials: false,
            relay_credentials_need_refresh: true,
            keychain_hydration_pending: false,
            relay_credential_fetch_retry_after_unix_secs: 0,
            relay_fallback_retry_after_unix_secs: 0,
            dev_bridge_available,
        };
        store.hydrate(false);
        store
    }

    /// Re-run hydration against current storage state. `bump_epoch`
    /// mirrors the Swift `bumpEpochOnAdoption` (unlock-time rehydration).
    pub fn hydrate(&mut self, bump_epoch: bool) {
        let loaded = self.records.load_records();
        let stored_active_id = self.records.load_active_mac_id();
        let hydration =
            paired_mac_hydration::resolve(&loaded, stored_active_id.as_deref(), |mac_id| {
                self.tokens.read_token(mac_id)
            });
        // Only explicit not-found records are pruned from storage. Never
        // persist pruning for temporarily-unavailable reads.
        let remains_pending = hydration.is_temporarily_unavailable();
        let active_record = hydration.active_record.clone();
        let active_token = hydration.active_token.clone();
        if hydration.records.len() != loaded.len() {
            self.records.save_records(&hydration.records);
        }
        self.paired_macs = hydration.records;

        let Some(record) = active_record else {
            if remains_pending {
                // Preserve any previously selected identity; do not claim
                // the user is unpaired while protected data is locked.
                self.active_mac_id = stored_active_id;
            } else {
                let changed =
                    self.active_mac_id.is_some() || matches!(self.mode, ConnectionMode::Paired(_));
                self.active_mac_id = None;
                self.records.save_active_mac_id(None);
                self.using_relay = false;
                self.has_relay_credentials = false;
                self.relay_credentials_need_refresh = true;
                self.mode = ConnectionMode::DevBridge;
                if bump_epoch && changed {
                    self.epoch = self.epoch.wrapping_add(1);
                }
            }
            self.keychain_hydration_pending = remains_pending;
            return;
        };

        self.active_mac_id = Some(record.mac_id.clone());
        if stored_active_id.as_deref() != Some(record.mac_id.as_str()) {
            self.records.save_active_mac_id(Some(&record.mac_id));
        }

        // Deterministic unlock-time adoption: a usable bearer token yields
        // one new epoch; the persisted record is reused exactly.
        let activation_epoch = match active_token {
            Some(token) if !token.is_empty() => Some(self.epoch.wrapping_add(1)),
            _ => None,
        };
        let Some(epoch) = activation_epoch else {
            // Empty Bearer <redacted> unusable but not an explicit missing item.
            // Retain the record; protected-data failures keep the pairing
            // sheet suppressed.
            self.keychain_hydration_pending = remains_pending;
            return;
        };

        let already_active = match &self.mode {
            ConnectionMode::Paired(current) => current == &record,
            ConnectionMode::DevBridge => false,
        };
        if !already_active {
            self.mode = ConnectionMode::Paired(record);
            self.using_relay = false;
            self.relay_credential_fetch_retry_after_unix_secs = 0;
            self.relay_fallback_retry_after_unix_secs = 0;
            if bump_epoch {
                self.epoch = epoch;
            }
        }

        // Relay credential state is reloaded on every active-Mac change.
        // (Simplified: the full marker/validity check lives in the
        // platform layer; here we track availability only.)
        self.keychain_hydration_pending = remains_pending;
    }

    /// Re-run only the deferred reads after unlock/foreground. Returns
    /// true when hydration is now conclusive.
    pub fn retry_keychain_hydration_if_needed(&mut self) -> bool {
        if !self.keychain_hydration_pending {
            return true;
        }
        self.hydrate(true);
        !self.keychain_hydration_pending
    }

    pub fn mode(&self) -> &ConnectionMode {
        &self.mode
    }

    pub fn epoch(&self) -> i64 {
        self.epoch
    }

    pub fn paired_macs(&self) -> &[PairedMacRecord] {
        &self.paired_macs
    }

    pub fn active_mac_id(&self) -> Option<&str> {
        self.active_mac_id.as_deref()
    }

    pub fn using_relay(&self) -> bool {
        self.using_relay
    }

    pub fn pairing_error(&self) -> Option<&str> {
        self.pairing_error.as_deref()
    }

    pub fn paired_mac_name(&self) -> Option<&str> {
        match &self.mode {
            ConnectionMode::Paired(record) => Some(record.mac_name.as_str()),
            ConnectionMode::DevBridge => None,
        }
    }

    pub fn needs_pairing(&self) -> bool {
        remote_pairing_presentation_policy::needs_pairing(
            matches!(self.mode, ConnectionMode::Paired(_)),
            self.keychain_hydration_pending,
            self.dev_bridge_available,
        )
    }

    /// Switch the active Mac. Only explicit found/non-empty token reads
    /// switch; temporarily-unavailable reads defer to unlock-time
    /// hydration instead of dropping pairing state.
    pub fn switch_to(&mut self, mac_id: &str) -> bool {
        if self.active_mac_id.as_deref() == Some(mac_id) {
            return false;
        }
        let Some(record) = self
            .paired_macs
            .iter()
            .find(|r| r.mac_id == mac_id)
            .cloned()
        else {
            return false;
        };
        match self.tokens.read_token(mac_id) {
            KeychainReadResult::Found(token) if !token.is_empty() => {
                self.active_mac_id = Some(mac_id.to_string());
                self.records.save_active_mac_id(Some(mac_id));
                self.using_relay = false;
                self.relay_credential_fetch_retry_after_unix_secs = 0;
                self.relay_fallback_retry_after_unix_secs = 0;
                self.mode = ConnectionMode::Paired(record);
                self.epoch = self.epoch.wrapping_add(1);
                true
            }
            KeychainReadResult::TemporarilyUnavailable(_) => {
                self.keychain_hydration_pending = true;
                false
            }
            KeychainReadResult::Found(_) | KeychainReadResult::NotFound => false,
        }
    }

    /// Record a successful pairing commit: persist the token first
    /// (fail-closed), then upsert the record and adopt it.
    ///
    /// Returns false when the token could not be saved — the caller must
    /// NOT claim success (the Host has already rotated the credential).
    pub fn commit_pairing(&mut self, record: PairedMacRecord, token: &str) -> bool {
        if token.is_empty() {
            self.pairing_error = Some("empty bearer token".to_string());
            return false;
        }
        if !self.tokens.save_token(&record.mac_id, token) {
            self.pairing_error = Some(
                "Could not save the new Mac access securely. Generate a fresh pairing code and try again."
                    .to_string(),
            );
            return false;
        }
        self.paired_macs = paired_mac_collection::upserting(&self.paired_macs, record.clone());
        self.records.save_records(&self.paired_macs);
        self.active_mac_id = Some(record.mac_id.clone());
        self.records.save_active_mac_id(Some(&record.mac_id));
        self.mode = ConnectionMode::Paired(record);
        self.using_relay = false;
        self.pairing_error = None;
        self.epoch = self.epoch.wrapping_add(1);
        true
    }

    /// Remove a pairing entirely: record list, active pointer, and
    /// keychain token.
    pub fn unpair(&mut self, mac_id: &str) {
        self.paired_macs = paired_mac_collection::removing(&self.paired_macs, mac_id);
        self.records.save_records(&self.paired_macs);
        self.tokens.delete_token(mac_id);
        if self.active_mac_id.as_deref() == Some(mac_id) {
            self.active_mac_id = self.paired_macs.first().map(|r| r.mac_id.clone());
            self.records
                .save_active_mac_id(self.active_mac_id.as_deref());
            self.mode = match self.paired_macs.first() {
                Some(record) => ConnectionMode::Paired(record.clone()),
                None => ConnectionMode::DevBridge,
            };
            self.using_relay = false;
            self.epoch = self.epoch.wrapping_add(1);
        }
    }

    /// Whether a relay-fallback attempt may start now.
    pub fn relay_fallback_can_attempt(&self, now_unix_secs: i64) -> bool {
        relay_fallback_retry_policy::can_attempt(
            now_unix_secs,
            self.relay_fallback_retry_after_unix_secs,
        )
    }

    /// Record a failed relay-fallback attempt (starts the cooldown).
    pub fn note_relay_fallback_failure(&mut self, completed_at_unix_secs: i64) {
        self.relay_fallback_retry_after_unix_secs =
            relay_fallback_retry_policy::retry_after_failure_unix_secs(completed_at_unix_secs);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn record(mac_id: &str) -> PairedMacRecord {
        PairedMacRecord {
            mac_id: mac_id.to_string(),
            mac_name: "Mac".to_string(),
            endpoint: "http://192.168.1.10:4485".to_string(),
            device_id: "device-1".to_string(),
            paired_at_unix_ms: 1,
            tint_hue: None,
            remote_server_certificate_fingerprint: None,
            direct_tls_fingerprint: None,
        }
    }

    // MARK: - PairedMacCollection

    #[test]
    fn upserting_appends_new_mac() {
        let records = vec![record("a")];
        let out = paired_mac_collection::upserting(&records, record("b"));
        let ids: Vec<&str> = out.iter().map(|r| r.mac_id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
    }

    #[test]
    fn upserting_replaces_in_place_preserving_order() {
        let records = vec![record("a"), record("b"), record("c")];
        let mut updated = record("b");
        updated.mac_name = "Renamed".to_string();
        updated.endpoint = "http://10.0.0.9:1234".to_string();
        let out = paired_mac_collection::upserting(&records, updated);
        let ids: Vec<&str> = out.iter().map(|r| r.mac_id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c"]);
        assert_eq!(out[1].mac_name, "Renamed");
        assert_eq!(out[1].endpoint, "http://10.0.0.9:1234");
    }

    #[test]
    fn removing_drops_only_that_mac() {
        let records = vec![record("a"), record("b")];
        let out = paired_mac_collection::removing(&records, "a");
        let ids: Vec<&str> = out.iter().map(|r| r.mac_id.as_str()).collect();
        assert_eq!(ids, ["b"]);
        let out = paired_mac_collection::removing(&records, "missing");
        let ids: Vec<&str> = out.iter().map(|r| r.mac_id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert!(paired_mac_collection::removing(&[record("a")], "a").is_empty());
    }

    #[test]
    fn record_array_round_trips() {
        let records = vec![record("a"), record("b")];
        let data = serde_json::to_vec(&records).unwrap();
        let decoded: Vec<PairedMacRecord> = serde_json::from_slice(&data).unwrap();
        assert_eq!(decoded, records);
    }

    #[test]
    fn record_optional_fields_survive_round_trip() {
        let mut r = record("a");
        r.tint_hue = Some(0.5);
        r.remote_server_certificate_fingerprint = Some("ab".to_string());
        r.direct_tls_fingerprint = Some("cd".to_string());
        assert!(r.requires_pinned_tls());
        let data = serde_json::to_vec(&r).unwrap();
        let decoded: PairedMacRecord = serde_json::from_slice(&data).unwrap();
        assert_eq!(decoded, r);

        let plain = record("b");
        assert!(!plain.requires_pinned_tls());
        // Optional fields are skipped when absent (backwards compatible).
        let json = serde_json::to_string(&plain).unwrap();
        assert!(!json.contains("tint_hue"));
    }

    // MARK: - KeychainReadResult

    #[test]
    fn keychain_read_result_map() {
        let found: KeychainReadResult<String> = KeychainReadResult::Found("a".to_string());
        assert_eq!(found.value(), Some(&"a".to_string()));
        assert_eq!(found.map(|s| s.len()), KeychainReadResult::Found(1));

        let missing: KeychainReadResult<String> = KeychainReadResult::NotFound;
        assert_eq!(missing.value(), None);
        assert_eq!(
            missing.map(|s: String| s.len()),
            KeychainReadResult::NotFound
        );

        let locked: KeychainReadResult<String> =
            KeychainReadResult::TemporarilyUnavailable("locked".to_string());
        assert_eq!(locked.unavailable_status(), Some("locked"));
    }

    // MARK: - Hydration

    #[test]
    fn hydration_prunes_only_explicitly_missing_token() {
        let missing = record("mac-missing");
        let retained = record("mac-retained");
        let hydration = paired_mac_hydration::resolve(
            &[missing.clone(), retained.clone()],
            Some("mac-missing"),
            |mac_id| {
                if mac_id == "mac-missing" {
                    KeychainReadResult::NotFound
                } else {
                    KeychainReadResult::Found("bearer-retained".to_string())
                }
            },
        );
        assert_eq!(hydration.records, vec![retained.clone()]);
        assert_eq!(hydration.active_record, Some(retained));
        assert_eq!(hydration.active_token.as_deref(), Some("bearer-retained"));
        assert!(!hydration.is_temporarily_unavailable());
    }

    #[test]
    fn hydration_temporarily_unavailable_preserves_records_and_marks_pending() {
        let available = record("mac-a");
        let protected = record("mac-b");
        let hydration = paired_mac_hydration::resolve(
            &[available.clone(), protected.clone()],
            Some("mac-b"),
            |mac_id| {
                if mac_id == "mac-b" {
                    KeychainReadResult::TemporarilyUnavailable(
                        "interaction-not-allowed".to_string(),
                    )
                } else {
                    KeychainReadResult::Found("bearer-a".to_string())
                }
            },
        );
        // Both records retained; active stays the preferred one.
        assert_eq!(hydration.records.len(), 2);
        assert_eq!(hydration.active_record, Some(protected));
        // No token for the protected Mac, so no active token.
        assert_eq!(hydration.active_token, None);
        assert!(hydration.is_temporarily_unavailable());
        assert_eq!(
            hydration.unavailable_statuses,
            vec!["interaction-not-allowed".to_string()]
        );
    }

    #[test]
    fn hydration_empty_bearer_retains_record_without_token() {
        let rec = record("mac-empty");
        let hydration = paired_mac_hydration::resolve(&[rec.clone()], Some("mac-empty"), |_| {
            KeychainReadResult::Found(String::new())
        });
        // Empty Bearer <redacted> conclusively unusable but NOT an explicit missing
        // item: the record is retained, never pruned.
        assert_eq!(hydration.records, vec![rec.clone()]);
        assert_eq!(hydration.active_record, Some(rec));
        assert_eq!(hydration.active_token, None);
        assert!(!hydration.is_temporarily_unavailable());
        // And direct activation refuses the empty token.
        assert!(paired_mac_hydration::direct_activation(&hydration, 7).is_none());
    }

    #[test]
    fn hydration_prefers_stored_active_falls_back_to_first() {
        let a = record("a");
        let b = record("b");
        let hydration = paired_mac_hydration::resolve(&[a.clone(), b.clone()], Some("b"), |_| {
            KeychainReadResult::Found("t".to_string())
        });
        assert_eq!(hydration.active_record, Some(b.clone()));

        let hydration =
            paired_mac_hydration::resolve(&[a.clone(), b.clone()], Some("missing"), |_| {
                KeychainReadResult::Found("t".to_string())
            });
        assert_eq!(hydration.active_record, Some(a.clone()));

        let hydration = paired_mac_hydration::resolve(&[a.clone()], None, |_| {
            KeychainReadResult::Found("t".to_string())
        });
        assert_eq!(hydration.active_record, Some(a));
    }

    #[test]
    fn direct_activation_bumps_epoch() {
        let rec = record("a");
        let hydration = paired_mac_hydration::resolve(&[rec.clone()], Some("a"), |_| {
            KeychainReadResult::Found("tok".to_string())
        });
        let (r, token, epoch) = paired_mac_hydration::direct_activation(&hydration, 41).unwrap();
        assert_eq!(r, rec);
        assert_eq!(token, "tok");
        assert_eq!(epoch, 42);
    }

    // MARK: - Legacy migration result

    #[test]
    fn legacy_migration_result_needs_retry() {
        assert!(!LegacyPairingMigrationResult::NoLegacyRecord.needs_retry());
        assert!(!LegacyPairingMigrationResult::Completed.needs_retry());
        assert!(LegacyPairingMigrationResult::RetryNeeded.needs_retry());
        assert!(
            LegacyPairingMigrationResult::TemporarilyUnavailable("x".to_string()).needs_retry()
        );
    }

    // MARK: - Presentation policy

    #[test]
    fn needs_pairing_policy() {
        use remote_pairing_presentation_policy::needs_pairing;
        assert!(needs_pairing(false, false, false));
        assert!(!needs_pairing(true, false, false));
        // Hydration pending suppresses the sheet (don't claim unpaired).
        assert!(!needs_pairing(false, true, false));
        // Dev bridge available: no pairing needed.
        assert!(!needs_pairing(false, false, true));
    }

    // MARK: - Relay fallback retry policy

    #[test]
    fn relay_fallback_retry_policy() {
        use relay_fallback_retry_policy as p;
        assert!(p::can_attempt(100, 100));
        assert!(p::can_attempt(101, 100));
        assert!(!p::can_attempt(99, 100));
        assert_eq!(
            p::retry_after_failure_unix_secs(1000),
            1000 + p::FAILURE_DELAY_SECS
        );
        assert_eq!(p::FAILURE_DELAY_SECS, 12);
    }

    // MARK: - Relay credential repair

    fn creds(mac_id: &str) -> RelayCredentials {
        RelayCredentials {
            mac_id: mac_id.to_string(),
            relay_token: "tok".to_string(),
            relay_url: "wss://relay.example".to_string(),
        }
    }

    #[test]
    fn repair_evaluate_404_is_recovery_unavailable() {
        use relay_credential_repair::{evaluate, RepairFetchError};
        let outcome = evaluate(Err(RepairFetchError::NotFound), "m", |_| {
            panic!("must not persist on 404")
        });
        assert_eq!(outcome, RelayCredentialRepairOutcome::RecoveryUnavailable);
    }

    #[test]
    fn repair_evaluate_other_error_is_fetch_failed() {
        use relay_credential_repair::{evaluate, RepairFetchError};
        let outcome = evaluate(
            Err(RepairFetchError::Other("boom".to_string())),
            "m",
            |_| panic!("must not persist on fetch failure"),
        );
        assert_eq!(outcome, RelayCredentialRepairOutcome::FetchFailed);
    }

    #[test]
    fn repair_evaluate_wrong_mac_is_invalid_response() {
        use relay_credential_repair::evaluate;
        let outcome = evaluate(Ok(creds("other-mac")), "m", |_| {
            panic!("must not persist mismatched credentials")
        });
        assert_eq!(outcome, RelayCredentialRepairOutcome::InvalidResponse);
    }

    #[test]
    fn repair_evaluate_structurally_invalid_is_invalid_response() {
        use relay_credential_repair::evaluate;
        let mut c = creds("m");
        c.relay_token = String::new();
        assert!(!c.is_structurally_valid());
        let outcome = evaluate(Ok(c), "m", |_| panic!("must not persist"));
        assert_eq!(outcome, RelayCredentialRepairOutcome::InvalidResponse);
    }

    #[test]
    fn repair_evaluate_persist_failure() {
        use relay_credential_repair::evaluate;
        let outcome = evaluate(Ok(creds("m")), "m", |_| false);
        assert_eq!(outcome, RelayCredentialRepairOutcome::PersistenceFailed);
    }

    #[test]
    fn repair_evaluate_success() {
        use relay_credential_repair::evaluate;
        let outcome = evaluate(Ok(creds("m")), "m", |_| true);
        assert_eq!(outcome, RelayCredentialRepairOutcome::Refreshed);
    }

    // MARK: - Refresh marker

    #[test]
    fn refresh_marker_needs_refresh() {
        use RelayCredentialRefreshMarker as M;
        assert_eq!(M::key("m"), "supercli.ios.relayCredentialVersion.m");
        assert!(M::is_current(M::CURRENT_VERSION));
        assert!(!M::is_current(0));
        // Temporarily-unavailable read: never refresh (inconclusive).
        assert!(!M::needs_refresh(true, true, Some(M::CURRENT_VERSION)));
        // Missing credentials always need refresh.
        assert!(M::needs_refresh(false, false, None));
        assert!(M::needs_refresh(false, false, Some(M::CURRENT_VERSION)));
        // Current version: no refresh.
        assert!(!M::needs_refresh(true, false, Some(M::CURRENT_VERSION)));
        // Recovery-unavailable marker: no refresh loop.
        assert!(!M::needs_refresh(
            true,
            false,
            Some(M::RECOVERY_UNAVAILABLE_VERSION)
        ));
        // Stale/unknown version: refresh.
        assert!(M::needs_refresh(true, false, Some(0)));
        assert!(M::needs_refresh(true, false, None));
    }

    // MARK: - Store

    #[derive(Default)]
    struct MockTokens {
        map: HashMap<String, String>,
        unavailable: Vec<String>,
        fail_save: bool,
    }

    impl TokenStore for MockTokens {
        fn read_token(&self, mac_id: &str) -> KeychainReadResult<String> {
            if self.unavailable.iter().any(|m| m == mac_id) {
                return KeychainReadResult::TemporarilyUnavailable("locked".to_string());
            }
            match self.map.get(mac_id) {
                Some(t) => KeychainReadResult::Found(t.clone()),
                None => KeychainReadResult::NotFound,
            }
        }

        fn save_token(&mut self, mac_id: &str, token: &str) -> bool {
            if self.fail_save {
                return false;
            }
            self.map.insert(mac_id.to_string(), token.to_string());
            true
        }

        fn delete_token(&mut self, mac_id: &str) {
            self.map.remove(mac_id);
        }
    }

    #[derive(Default)]
    struct MockRecords {
        records: Vec<PairedMacRecord>,
        active_mac_id: Option<String>,
        device_id: Option<String>,
    }

    impl RecordStore for MockRecords {
        fn load_records(&self) -> Vec<PairedMacRecord> {
            self.records.clone()
        }
        fn save_records(&mut self, records: &[PairedMacRecord]) {
            self.records = records.to_vec();
        }
        fn load_active_mac_id(&self) -> Option<String> {
            self.active_mac_id.clone()
        }
        fn save_active_mac_id(&mut self, mac_id: Option<&str>) {
            self.active_mac_id = mac_id.map(|s| s.to_string());
        }
        fn load_device_id(&self) -> Option<String> {
            self.device_id.clone()
        }
        fn save_device_id(&mut self, device_id: &str) {
            self.device_id = Some(device_id.to_string());
        }
    }

    fn paired_store() -> RemoteConnectionStore<MockTokens, MockRecords> {
        let mut tokens = MockTokens::default();
        tokens
            .map
            .insert("mac-1".to_string(), "bearer-1".to_string());
        let records = MockRecords {
            records: vec![record("mac-1")],
            active_mac_id: Some("mac-1".to_string()),
            ..Default::default()
        };
        RemoteConnectionStore::new(tokens, records, false)
    }

    #[test]
    fn store_hydrates_paired_mode_from_storage() {
        let store = paired_store();
        assert!(matches!(store.mode(), ConnectionMode::Paired(_)));
        assert_eq!(store.active_mac_id(), Some("mac-1"));
        assert_eq!(store.paired_macs().len(), 1);
        assert_eq!(store.paired_mac_name(), Some("Mac"));
        assert!(!store.needs_pairing());
    }

    #[test]
    fn store_empty_starts_unpaired_and_needs_pairing() {
        let store =
            RemoteConnectionStore::new(MockTokens::default(), MockRecords::default(), false);
        assert!(matches!(store.mode(), ConnectionMode::DevBridge));
        assert!(store.needs_pairing());
        // Dev bridge available: no pairing sheet.
        let store = RemoteConnectionStore::new(MockTokens::default(), MockRecords::default(), true);
        assert!(!store.needs_pairing());
    }

    #[test]
    fn store_prunes_record_with_explicitly_missing_token() {
        let mut tokens = MockTokens::default();
        tokens.map.insert("kept".to_string(), "t".to_string());
        let records = MockRecords {
            records: vec![record("gone"), record("kept")],
            active_mac_id: Some("gone".to_string()),
            ..Default::default()
        };
        let store = RemoteConnectionStore::new(tokens, records, false);
        // "gone" pruned; active falls back to "kept".
        let ids: Vec<&str> = store
            .paired_macs()
            .iter()
            .map(|r| r.mac_id.as_str())
            .collect();
        assert_eq!(ids, ["kept"]);
        assert_eq!(store.active_mac_id(), Some("kept"));
    }

    #[test]
    fn store_locked_keychain_preserves_identity_and_suppresses_pairing() {
        let tokens = MockTokens {
            unavailable: vec!["mac-1".to_string()],
            ..Default::default()
        };
        let records = MockRecords {
            records: vec![record("mac-1")],
            active_mac_id: Some("mac-1".to_string()),
            ..Default::default()
        };
        let mut store = RemoteConnectionStore::new(tokens, records, false);
        // Record retained, identity preserved, pairing sheet suppressed.
        assert_eq!(store.paired_macs().len(), 1);
        assert_eq!(store.active_mac_id(), Some("mac-1"));
        assert!(!store.needs_pairing());
        // Retry while still locked stays pending (returns false).
        assert!(!store.retry_keychain_hydration_if_needed());
    }

    #[test]
    fn store_switch_to_changes_active_and_bumps_epoch() {
        let mut store = paired_store();
        // Add a second Mac.
        assert!(store.commit_pairing(record("mac-2"), "bearer-2"));
        let epoch_after_pair = store.epoch();
        assert_eq!(store.active_mac_id(), Some("mac-2"));

        assert!(store.switch_to("mac-1"));
        assert_eq!(store.active_mac_id(), Some("mac-1"));
        assert_eq!(store.epoch(), epoch_after_pair.wrapping_add(1));
        assert!(!store.using_relay());

        // Switching to the already-active Mac is a no-op.
        assert!(!store.switch_to("mac-1"));
        // Unknown Mac id is a no-op.
        assert!(!store.switch_to("nope"));
    }

    #[test]
    fn store_switch_to_with_locked_keychain_defers() {
        let mut store = paired_store();
        store.tokens.unavailable = vec!["mac-1".to_string()];
        // Active is mac-1; add mac-2 with a good token then lock mac-2.
        store
            .tokens
            .map
            .insert("mac-2".to_string(), "t2".to_string());
        store.paired_macs = paired_mac_collection::upserting(store.paired_macs(), record("mac-2"));
        store.tokens.unavailable = vec!["mac-2".to_string()];
        assert!(!store.switch_to("mac-2"));
        assert_eq!(store.active_mac_id(), Some("mac-1"));
    }

    #[test]
    fn store_commit_pairing_fail_closed_on_token_write_failure() {
        let mut store = paired_store();
        store.tokens.fail_save = true;
        let before = store.paired_macs().len();
        assert!(!store.commit_pairing(record("mac-9"), "bearer-9"));
        // Nothing activated, nothing persisted.
        assert_eq!(store.paired_macs().len(), before);
        assert_eq!(store.active_mac_id(), Some("mac-1"));
        assert!(store.pairing_error().is_some());
    }

    #[test]
    fn store_commit_pairing_rejects_empty_bearer() {
        let mut store = paired_store();
        assert!(!store.commit_pairing(record("mac-9"), ""));
        assert!(store.paired_macs().iter().all(|r| r.mac_id != "mac-9"));
    }

    #[test]
    fn store_unpair_removes_record_token_and_repoints() {
        let mut store = paired_store();
        assert!(store.commit_pairing(record("mac-2"), "bearer-2"));
        store.unpair("mac-2");
        let ids: Vec<&str> = store
            .paired_macs()
            .iter()
            .map(|r| r.mac_id.as_str())
            .collect();
        assert_eq!(ids, ["mac-1"]);
        assert_eq!(store.active_mac_id(), Some("mac-1"));
        // Token deleted from the keychain.
        assert_eq!(
            store.tokens.read_token("mac-2"),
            KeychainReadResult::NotFound
        );

        // Unpairing the last Mac drops to dev bridge.
        store.unpair("mac-1");
        assert!(store.paired_macs().is_empty());
        assert!(matches!(store.mode(), ConnectionMode::DevBridge));
        assert!(store.needs_pairing());
    }

    #[test]
    fn store_relay_fallback_cooldown() {
        let store = paired_store();
        assert!(store.relay_fallback_can_attempt(1000));
        let mut store = store;
        store.note_relay_fallback_failure(1000);
        assert!(!store.relay_fallback_can_attempt(1005));
        assert!(store.relay_fallback_can_attempt(1000 + 12));
    }

    #[test]
    fn retry_delays_match_swift_constants() {
        assert_eq!(RELAY_CREDENTIAL_RETRY_DELAY_SECS, 60);
        assert_eq!(RELAY_CREDENTIAL_UNAVAILABLE_RETRY_DELAY_SECS, 15 * 60);
    }

    #[test]
    fn storage_keys_match_ios_conventions() {
        assert_eq!(RECORDS_KEY, "supercli.ios.pairedMacs");
        assert_eq!(ACTIVE_MAC_ID_KEY, "supercli.ios.activeMacID");
        assert_eq!(LEGACY_RECORD_KEY, "supercli.ios.pairedMac");
        assert_eq!(DEVICE_ID_KEY, "supercli.ios.deviceID");
    }
}
