//! True-gap decisions from the iOS `RemoteConnectionStore.swift`
//! (`clients/legacy/ios/SupercliIOS/Sources/SupercliIOS/`).
//!
//! Audit verdict per Swift behaviour (checked against `origin/next` first;
//! only true gaps are ported here):
//!
//! | Swift behaviour | Existing target | Verdict |
//! |---|---|---|
//! | `PairedMacRecord` | `pairing::PairedHostRecord` | covered |
//! | `PairedMacRecord.directClient(token:)` | `pairing::client_for_paired_host` | covered |
//! | `KeychainReadResult` tri-state | `CredentialStore::get_secret() -> Result<Option<Vec<u8>>, CredentialError>` (`credentials.rs`) | covered (idiomatic) |
//! | `RemoteKeychain.isValid` / `relayCredentialState` | `credentials::relay_credentials_for_host` (wss + non-empty token + 32-byte E2E key) | covered |
//! | `RemoteKeychain.saveRelayCredentials` (refuse-invalid) | `host_store::RemoteHostStore::adopt` (validates before save, fail-closed) | covered |
//! | `RemoteKeychain.deleteRelayCredentials` / `deleteToken` | `credentials::delete_host_secrets` | covered |
//! | `RemoteKeychain` legacy keychain slots | — | dropped: iOS-Keychain-only legacy slots; the Rust store is a fresh namespace with nothing to migrate |
//! | `RelayCredentialRefreshMarker` | **this module** | **ported (true gap)** |
//! | `RelayCredentialRepairOutcome` / `RelayCredentialRepair.evaluating` | **this module** (`evaluate_credential_repair`) | **ported (true gap)**; the async fetch stays platform work |
//! | `RelayCredentialRepair.attemptIfBoundToActiveDirectClient` | **this module** (generation capture / `is_current`) + platform async | **ported kernel (true gap)** |
//! | `RemoteConnectionPollProof.isTransportAuthenticated` | **this module** (`is_transport_authenticated`) | **ported (true gap)** |
//! | `RemoteConnectionPollProof` / `RemoteConnectionPollResult` (rest) | `remote_runtime.rs` generation capture/reject | covered; `@MainActor` orchestration dropped |
//! | `RemoteDirectClientGeneration` / `RemoteRelayClientGeneration` | **this module** | **ported (true gap)**; `HostRegistry` caches live clients but has no staleness guards |
//! | `RelayDirectEndpointRefresh.validatedHTTPMobileEndpoint` | **this module** (`validated_relay_direct_endpoint`) | **ported (true gap)** |
//! | `RelayDirectEndpointRefresh.prepare` (rest) | generation binding (this module) + `pairing::upsert_paired_host` | ported kernel / covered |
//! | `RemoteDirectRestore.attempt` | **this module** (generation `is_current` checks) + platform probe/adopt | **ported kernel (true gap)**; async stays platform work |
//! | `RelayFallbackRetryPolicy` (12 s cooldown) | **this module** | **ported (true gap)**; `ReconnectPolicy` is generic backoff, not the relay-fallback cooldown |
//! | `PairedMacCollection` | `pairing::upsert_paired_host` / `remove_paired_host` | covered |
//! | `PairedMacHydration.resolve` | **this module** (`resolve_paired_host_hydration`) | **ported (true gap)**; `load_paired_host_records` fails the whole load on `Unavailable` instead of retaining records |
//! | `PairedMacHydration.directActivation` | **this module** (`direct_activation`) | **ported (true gap)** |
//! | `LegacyPairingMigrationResult` / `migrateLegacyStorageIfNeeded` | — | dropped: iOS-Keychain-only legacy slots |
//! | `RemotePairingPresentationPolicy.needsPairing` | — | dropped: pairing-sheet presentation policy owned by the Dart UI layer |
//! | `PreparedRemotePairingCommit` / `RemotePairingCommit.prepare` | `host_store::RemoteHostStore::adopt` (validates, saves secrets fail-closed with `?`, upserts, pins from pairing) | covered |
//! | `RemoteConnectionStore` (`ObservableObject`, `Mode`, bootstrap polls, relay-fallback orchestration, push-token fan-out, `@Published` state) | `remote_runtime.rs`, `remote_connection.rs`, `direct_transport::PushTokenRegistrationRoute`, `pairing::device_identity` | covered; SwiftUI/`@MainActor` orchestration dropped |
//! | `PairingError` | `host_store::HostStoreError` | covered |
//!
//! Platform seams (Keychain, UserDefaults) are injected as traits/closures;
//! the Security-framework calls themselves stay in Swift. Secret-bearing
//! types redact in `Debug` and are never logged.

use std::time::{Duration, Instant};

use crate::pairing::PairedHostRecord;
use crate::relay::RelayCredentials;

// ---------------------------------------------------------------------------
// RelayCredentialRefreshMarker
// ---------------------------------------------------------------------------

/// Integer key-value seam behind the credential-refresh marker
/// (UserDefaults on iOS). Unit-testable without the record store.
pub trait MarkerKv {
    fn get_int(&self, key: &str) -> Option<i64>;
    fn set_int(&mut self, key: &str, value: i64);
    fn remove(&mut self, key: &str);
}

/// Versioned freshness marker for per-Host relay credentials.
///
/// Markerless pairings (older builds) keep their pairing and stable device
/// ids; the next healthy authenticated Direct connection rotates and
/// re-saves the relay secret once. A failed relay handshake clears the
/// marker; returning to Direct repairs it without a re-pair.
pub struct RelayCredentialRefreshMarker;

impl RelayCredentialRefreshMarker {
    /// Current marker version.
    pub const CURRENT_VERSION: i64 = 1;
    /// The selected Host is healthy over Direct but does not implement the
    /// recovery route: keep using the structurally valid credential rather
    /// than disabling Link or retrying the mutating GET every minute. A
    /// relay failure clears this marker.
    pub const RECOVERY_UNAVAILABLE_VERSION: i64 = -1;
    pub const KEY_PREFIX: &str = "supercli.ios.relayCredentialVersion.";

    pub fn key(mac_id: &str) -> String {
        format!("{}{mac_id}", Self::KEY_PREFIX)
    }

    pub fn is_current(mac_id: &str, store: &dyn MarkerKv) -> bool {
        store.get_int(&Self::key(mac_id)) == Some(Self::CURRENT_VERSION)
    }

    pub fn mark_current(mac_id: &str, store: &mut dyn MarkerKv) {
        store.set_int(&Self::key(mac_id), Self::CURRENT_VERSION);
    }

    pub fn mark_recovery_unavailable(mac_id: &str, store: &mut dyn MarkerKv) {
        store.set_int(&Self::key(mac_id), Self::RECOVERY_UNAVAILABLE_VERSION);
    }

    pub fn mark_stale(mac_id: &str, store: &mut dyn MarkerKv) {
        store.remove(&Self::key(mac_id));
    }

    /// Whether the relay credential for `mac_id` needs a refresh over the
    /// healthy Direct channel. `credentials` is the load-path result
    /// (see [`crate::credentials::relay_credentials_for_host`]);
    /// `temporarily_unavailable` is an inconclusive protected-data read,
    /// which never needs one: a healthy Direct poll does not authorize
    /// rotating a secret that may merely be hidden behind lock state.
    pub fn needs_refresh(
        mac_id: &str,
        credentials: Option<&RelayCredentials>,
        temporarily_unavailable: bool,
        store: &dyn MarkerKv,
    ) -> bool {
        if temporarily_unavailable {
            return false;
        }
        if credentials.is_none() {
            return true;
        }
        let version = store.get_int(&Self::key(mac_id)).unwrap_or(0);
        version != Self::CURRENT_VERSION && version != Self::RECOVERY_UNAVAILABLE_VERSION
    }
}

// ---------------------------------------------------------------------------
// Relay-learned Direct endpoint validation — `RelayDirectEndpointRefresh`
// ---------------------------------------------------------------------------

fn strip_brackets(host: &str) -> &str {
    if let Some(inner) = host.strip_prefix('[') {
        if let Some(end) = inner.find(']') {
            return &inner[..end];
        }
    }
    host
}

fn strip_http_scheme(endpoint: &str) -> Option<&str> {
    if endpoint.len() >= 7 && endpoint[..7].eq_ignore_ascii_case("http://") {
        Some(&endpoint[7..])
    } else {
        None
    }
}

/// Validate an endpoint learned inside an authenticated E2E Relay bootstrap
/// before it becomes the stored Direct endpoint.
///
/// This is deliberately stricter than the pairing-time endpoint check in
/// [`crate::pairing`]: the relay is the trust boundary here (Bonjour/TXT
/// data never enters this path), so loopback, unspecified, and link-local
/// hosts are rejected outright — a relay-learned endpoint pointing at the
/// phone itself must never become the Direct target. The returned spelling
/// is always canonical `http://` (the certificate pin, not the stored
/// scheme, decides the wire scheme).
pub fn validated_relay_direct_endpoint(endpoint: &str) -> Option<String> {
    // A TLS-capable Host may advertise `https://`; the stored endpoint is
    // always the canonical `http://` spelling.
    let canonical = crate::direct_transport::canonical_stored_endpoint(endpoint);
    let rest = strip_http_scheme(&canonical)?;
    if rest.contains('@') {
        return None; // no userinfo
    }
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    // The exact path match also excludes any query or fragment.
    if path != "/mobile" {
        return None;
    }
    let (host, port_str) = authority.rsplit_once(':')?;
    let host = strip_brackets(host).to_lowercase();
    if host.is_empty() {
        return None;
    }
    if host == "localhost"
        || host == "0.0.0.0"
        || host == "::"
        || host == "::1"
        || host.starts_with("127.")
        || host.starts_with("169.254.")
        || host.starts_with("fe80:")
    {
        return None;
    }
    let port: u16 = port_str.parse().ok()?;
    if port == 0 {
        return None;
    }
    Some(canonical)
}

// ---------------------------------------------------------------------------
// RelayFallbackRetryPolicy
// ---------------------------------------------------------------------------

/// Retry policy for the relay-fallback ladder. Relay failures may take
/// longer than the cooldown itself, so the next attempt is derived from
/// completion time — a slow failure cannot immediately retry.
pub struct RelayFallbackRetryPolicy;

impl RelayFallbackRetryPolicy {
    /// Delay after a failed relay-fallback attempt.
    pub const FAILURE_DELAY: Duration = Duration::from_secs(12);

    pub fn can_attempt(now: Instant, retry_after: Instant) -> bool {
        now >= retry_after
    }

    pub fn retry_after_failure(completed_at: Instant) -> Instant {
        completed_at + Self::FAILURE_DELAY
    }
}

// ---------------------------------------------------------------------------
// RelayCredentialRepair — outcome classification
// ---------------------------------------------------------------------------

/// Terminal outcome of one authenticated Direct credential-repair attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayCredentialRepairOutcome {
    Refreshed,
    RecoveryUnavailable,
    FetchFailed,
    InvalidResponse,
    PersistenceFailed,
}

/// What can go wrong fetching fresh relay credentials over Direct. The
/// 404 case is load-bearing: headless Hosts do not implement the recovery
/// route, and that must not be retried like a transient failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialRepairFetchError {
    HttpStatus(u16),
    Transport(String),
}

/// Whether fetched relay credentials are usable for `expected_mac_id`.
/// Repair-path counterpart of
/// [`crate::credentials::relay_credentials_for_host`] (which validates from
/// stored secrets at load time): same rules — right Host, `wss://` URL,
/// non-empty token, 32-byte E2E key — applied to already-constructed
/// credentials before they are persisted.
fn fetched_relay_credentials_valid(credentials: &RelayCredentials, expected_mac_id: &str) -> bool {
    credentials.mac_id == expected_mac_id
        && credentials.relay_url.to_lowercase().starts_with("wss://")
        && !credentials.relay_token.is_empty()
        && credentials.e2e_key().is_some()
}

/// Classify one repair fetch result. Kept separate from the connection
/// store so missing/stale/write-failure behavior is completely
/// deterministic in tests. The injected `persist` keeps the write-failure
/// path testable without touching the real keychain: callers must never
/// claim Relay is ready after a failed write.
pub fn evaluate_credential_repair(
    result: Result<RelayCredentials, CredentialRepairFetchError>,
    expected_mac_id: &str,
    persist: impl FnOnce(&RelayCredentials) -> bool,
) -> RelayCredentialRepairOutcome {
    match result {
        Err(CredentialRepairFetchError::HttpStatus(404)) => {
            RelayCredentialRepairOutcome::RecoveryUnavailable
        }
        Err(_) => RelayCredentialRepairOutcome::FetchFailed,
        Ok(credentials) => {
            if !fetched_relay_credentials_valid(&credentials, expected_mac_id) {
                return RelayCredentialRepairOutcome::InvalidResponse;
            }
            // The recovery route rotates the Host credential before
            // replying: never advertise the old keychain value as ready
            // after a failed write.
            if !persist(&credentials) {
                return RelayCredentialRepairOutcome::PersistenceFailed;
            }
            RelayCredentialRepairOutcome::Refreshed
        }
    }
}

// ---------------------------------------------------------------------------
// Poll transport authentication — `RemoteConnectionPollProof`
// ---------------------------------------------------------------------------

/// Whether the Host identity behind a bootstrap poll was authenticated by
/// the transport itself: the E2E relay (device key) or a pinned TLS
/// session. A plaintext LAN reply is not, so it may upgrade a Host to TLS
/// but never strip a pin the phone already holds. Feeds
/// [`crate::direct_transport::apply_direct_transport_decision`]'s
/// `authenticated` parameter.
pub fn is_transport_authenticated(
    is_relay_client: bool,
    pinned_tls_fingerprint: Option<&str>,
) -> bool {
    is_relay_client || pinned_tls_fingerprint.is_some()
}

// ---------------------------------------------------------------------------
// Client-generation guards — `RemoteDirectClientGeneration`,
// `RemoteRelayClientGeneration`
// ---------------------------------------------------------------------------

/// Transport identity snapshot of one client. The store captures this
/// before awaiting a poll; reading the live client afterwards could
/// accidentally attribute a stale Mac A success to a newly adopted Mac B
/// client. The bearer is redacted in `Debug` and never logged.
#[derive(Clone, PartialEq, Eq)]
pub struct ClientIdentitySnapshot {
    pub is_relay: bool,
    pub base_url: String,
    pub auth_token: String,
    pub tls_fingerprint: Option<String>,
    pub relay_session_id: Option<u64>,
}

impl std::fmt::Debug for ClientIdentitySnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientIdentitySnapshot")
            .field("is_relay", &self.is_relay)
            .field("base_url", &self.base_url)
            .field("auth_token", &"<redacted>")
            .field("tls_fingerprint", &self.tls_fingerprint)
            .field("relay_session_id", &self.relay_session_id)
            .finish()
    }
}

/// Exact identity of one Direct client generation. `mac_id` alone is not a
/// sufficient guard: re-pairing the same Mac rotates its bearer and bumps
/// the connection epoch while an older request may still be suspended.
/// The transport is part of the generation: a plaintext client and its
/// pinned-TLS successor for the same Host are different generations.
#[derive(Clone, PartialEq, Eq)]
pub struct DirectClientGeneration {
    epoch: u64,
    mac_id: String,
    endpoint: String,
    auth_token: String,
    tls_fingerprint: Option<String>,
}

impl std::fmt::Debug for DirectClientGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectClientGeneration")
            .field("epoch", &self.epoch)
            .field("mac_id", &self.mac_id)
            .field("endpoint", &self.endpoint)
            .field("auth_token", &"<redacted>")
            .field("tls_fingerprint", &self.tls_fingerprint)
            .finish()
    }
}

impl DirectClientGeneration {
    /// Capture the generation behind `candidate`, bound to the store's
    /// current Direct client. Returns `None` when either client is already
    /// a relay client or diverges from the active record — a stale poll
    /// from such a client must never enter the repair path.
    pub fn capture(
        candidate: &ClientIdentitySnapshot,
        active: &ClientIdentitySnapshot,
        mac_id: &str,
        endpoint: &str,
        record_tls_fingerprint: Option<&str>,
        active_token: &str,
        epoch: u64,
    ) -> Option<Self> {
        if candidate.is_relay || active.is_relay {
            return None;
        }
        for client in [candidate, active] {
            if client.base_url != endpoint
                || client.auth_token != active_token
                || client.tls_fingerprint.as_deref() != record_tls_fingerprint
            {
                return None;
            }
        }
        Some(Self {
            epoch,
            mac_id: mac_id.to_string(),
            endpoint: endpoint.to_string(),
            auth_token: active_token.to_string(),
            tls_fingerprint: record_tls_fingerprint.map(str::to_string),
        })
    }

    /// Whether this generation is still the store's current Direct
    /// generation. A bootstrap completion for a superseded generation is
    /// inert: it must not trigger relay fallback (or credential repair)
    /// for a newly selected Host.
    pub fn is_current(
        &self,
        epoch: u64,
        active: &ClientIdentitySnapshot,
        mac_id: &str,
        endpoint: &str,
        record_tls_fingerprint: Option<&str>,
        active_token: &str,
    ) -> bool {
        self.epoch == epoch
            && self.mac_id == mac_id
            && self.endpoint == endpoint
            && self.auth_token == active_token
            && self.tls_fingerprint.as_deref() == record_tls_fingerprint
            && !active.is_relay
            && active.base_url == endpoint
            && active.auth_token == active_token
            && active.tls_fingerprint.as_deref() == record_tls_fingerprint
    }
}

/// Exact identity of one relay-backed client generation. The relay session
/// closes the same-Mac ABA case where an old Relay→Direct probe returns
/// after re-pair and after the newer generation has itself moved back onto
/// the relay.
#[derive(Clone, PartialEq, Eq)]
pub struct RelayClientGeneration {
    epoch: u64,
    mac_id: String,
    endpoint: String,
    auth_token: String,
    relay_session_id: u64,
}

impl std::fmt::Debug for RelayClientGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelayClientGeneration")
            .field("epoch", &self.epoch)
            .field("mac_id", &self.mac_id)
            .field("endpoint", &self.endpoint)
            .field("auth_token", &"<redacted>")
            .field("relay_session_id", &self.relay_session_id)
            .finish()
    }
}

impl RelayClientGeneration {
    /// Capture the generation behind the store's current relay client.
    /// Returns `None` when the active client is not on the relay or
    /// diverges from the active record.
    pub fn capture(
        active: &ClientIdentitySnapshot,
        mac_id: &str,
        endpoint: &str,
        active_token: &str,
        epoch: u64,
    ) -> Option<Self> {
        let relay_session_id = active.relay_session_id?;
        if !active.is_relay {
            return None;
        }
        if active.base_url != endpoint || active.auth_token != active_token {
            return None;
        }
        Some(Self {
            epoch,
            mac_id: mac_id.to_string(),
            endpoint: endpoint.to_string(),
            auth_token: active_token.to_string(),
            relay_session_id,
        })
    }

    pub fn is_current(
        &self,
        epoch: u64,
        active: &ClientIdentitySnapshot,
        mac_id: &str,
        endpoint: &str,
        active_token: &str,
    ) -> bool {
        self.epoch == epoch
            && self.mac_id == mac_id
            && self.endpoint == endpoint
            && self.auth_token == active_token
            && active.is_relay
            && active.base_url == endpoint
            && active.auth_token == active_token
            && active.relay_session_id == Some(self.relay_session_id)
    }
}

// ---------------------------------------------------------------------------
// PairedMacHydration
// ---------------------------------------------------------------------------

/// Result of resolving persisted records against typed secret reads.
#[derive(Debug, Clone)]
pub struct PairedHostHydrationResult {
    pub records: Vec<PairedHostRecord>,
    pub active_record: Option<PairedHostRecord>,
    pub active_token: Option<String>,
    pub unavailable_statuses: Vec<i32>,
}

impl PairedHostHydrationResult {
    pub fn is_temporarily_unavailable(&self) -> bool {
        !self.unavailable_statuses.is_empty()
    }
}

/// Resolve persisted records against typed secret reads.
///
/// The read shape mirrors [`crate::credentials::CredentialStore::get_secret`]:
/// `Ok(Some)` is a conclusive find, `Ok(None)` is an explicit not-found,
/// and `Err(status)` is an inconclusive protected-data failure carrying the
/// platform status.
///
/// Only an explicit not-found prunes a record; protected-data failures
/// retain its order, active identity, and stable device id for unlock-time
/// hydration. An empty bearer is unusable but not a not-found: the record
/// is kept so an inconclusive/corrupt value is never rewritten as a
/// destructive unpair operation.
pub fn resolve_paired_host_hydration(
    records: Vec<PairedHostRecord>,
    preferred_active_mac_id: Option<&str>,
    read_token: impl Fn(&str) -> Result<Option<String>, i32>,
) -> PairedHostHydrationResult {
    let mut retained: Vec<PairedHostRecord> = Vec::with_capacity(records.len());
    let mut tokens: Vec<(String, String)> = Vec::new();
    let mut unavailable_statuses: Vec<i32> = Vec::new();
    for record in records {
        match read_token(&record.host_id) {
            Ok(Some(token)) if !token.is_empty() => {
                tokens.push((record.host_id.clone(), token));
                retained.push(record);
            }
            Ok(Some(_)) => {
                // Empty bearer: conclusively unusable, but not an explicit
                // missing item. Keep the stable record.
                retained.push(record);
            }
            Ok(None) => {}
            Err(status) => {
                unavailable_statuses.push(status);
                retained.push(record);
            }
        }
    }
    let active_record = preferred_active_mac_id
        .and_then(|id| retained.iter().find(|r| r.host_id == id))
        .or_else(|| retained.first())
        .cloned();
    let active_token = active_record.as_ref().and_then(|record| {
        tokens
            .iter()
            .find(|(id, _)| id == &record.host_id)
            .map(|(_, token)| token.clone())
    });
    PairedHostHydrationResult {
        records: retained,
        active_record,
        active_token,
        unavailable_statuses,
    }
}

/// Deterministic unlock-time adoption seam. A previously unavailable
/// bearer creates one new Direct client generation; the persisted record
/// (including its stable device id) is reused exactly and no pairing
/// exchange is involved. Returns the record, its bearer, and the bumped
/// epoch — the caller builds the client (e.g. via
/// [`crate::pairing::client_for_paired_host`]).
pub fn direct_activation(
    hydration: &PairedHostHydrationResult,
    current_epoch: u64,
) -> Option<(PairedHostRecord, String, u64)> {
    let record = hydration.active_record.clone()?;
    let token = hydration.active_token.clone()?;
    if token.is_empty() {
        return None;
    }
    Some((record, token, current_epoch.wrapping_add(1)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // 32 zero bytes, base64.
    const E2E_KEY_B64: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

    fn test_relay_credentials() -> RelayCredentials {
        RelayCredentials {
            relay_url: "wss://relay.example.com/link".to_string(),
            mac_id: "mac-1".to_string(),
            relay_token: "relay-token".to_string(),
            e2e_key_b64: E2E_KEY_B64.to_string(),
        }
    }

    fn test_record(host_id: &str) -> PairedHostRecord {
        PairedHostRecord {
            host_id: host_id.to_string(),
            name: format!("Mac {host_id}"),
            endpoint: "http://192.168.1.10:8321/mobile".to_string(),
            controller_device_id: "device-1".to_string(),
            paired_at_unix_ms: 1_700_000_000_000,
            certificate_fingerprint: None,
            remote_server_port: None,
            remote_server_certificate_fingerprint: None,
            link_enabled: None,
        }
    }

    #[derive(Default)]
    struct MemKv {
        ints: HashMap<String, i64>,
    }

    impl MarkerKv for MemKv {
        fn get_int(&self, key: &str) -> Option<i64> {
            self.ints.get(key).copied()
        }
        fn set_int(&mut self, key: &str, value: i64) {
            self.ints.insert(key.to_string(), value);
        }
        fn remove(&mut self, key: &str) {
            self.ints.remove(key);
        }
    }

    // MARK: - RelayCredentialRefreshMarker

    #[test]
    fn marker_current_stale_recovery_lifecycle() {
        let mut kv = MemKv::default();
        assert!(!RelayCredentialRefreshMarker::is_current("mac-1", &kv));
        RelayCredentialRefreshMarker::mark_current("mac-1", &mut kv);
        assert!(RelayCredentialRefreshMarker::is_current("mac-1", &kv));
        RelayCredentialRefreshMarker::mark_stale("mac-1", &mut kv);
        assert!(!RelayCredentialRefreshMarker::is_current("mac-1", &kv));
        RelayCredentialRefreshMarker::mark_recovery_unavailable("mac-1", &mut kv);
        assert!(!RelayCredentialRefreshMarker::is_current("mac-1", &kv));
        // Keys are per-Mac.
        assert!(!RelayCredentialRefreshMarker::is_current("mac-2", &kv));
    }

    #[test]
    fn marker_needs_refresh_matrix() {
        let mut kv = MemKv::default();
        let creds = test_relay_credentials();

        // Missing always needs a refresh...
        assert!(RelayCredentialRefreshMarker::needs_refresh(
            "mac-1", None, false, &kv
        ));
        // ...but an inconclusive read never does.
        assert!(!RelayCredentialRefreshMarker::needs_refresh(
            "mac-1",
            Some(&creds),
            true,
            &kv
        ));
        // Available without a marker needs one (pre-marker builds).
        assert!(RelayCredentialRefreshMarker::needs_refresh(
            "mac-1",
            Some(&creds),
            false,
            &kv
        ));
        // Current marker: no refresh.
        RelayCredentialRefreshMarker::mark_current("mac-1", &mut kv);
        assert!(!RelayCredentialRefreshMarker::needs_refresh(
            "mac-1",
            Some(&creds),
            false,
            &kv
        ));
        // Recovery-unavailable: no repeat refresh.
        RelayCredentialRefreshMarker::mark_recovery_unavailable("mac-1", &mut kv);
        assert!(!RelayCredentialRefreshMarker::needs_refresh(
            "mac-1",
            Some(&creds),
            false,
            &kv
        ));
        // Stale again: refresh.
        RelayCredentialRefreshMarker::mark_stale("mac-1", &mut kv);
        assert!(RelayCredentialRefreshMarker::needs_refresh(
            "mac-1",
            Some(&creds),
            false,
            &kv
        ));
    }

    // MARK: - validated_relay_direct_endpoint

    #[test]
    fn relay_endpoint_accepts_plain_and_canonical_https() {
        assert_eq!(
            validated_relay_direct_endpoint("http://192.168.1.10:8321/mobile"),
            Some("http://192.168.1.10:8321/mobile".to_string())
        );
        // A TLS-capable Host may advertise https://; the stored spelling is
        // always the canonical http://.
        assert_eq!(
            validated_relay_direct_endpoint("https://192.168.1.10:8321/mobile"),
            Some("http://192.168.1.10:8321/mobile".to_string())
        );
    }

    #[test]
    fn relay_endpoint_rejects_loopback_and_link_local() {
        for bad in [
            "http://localhost:8321/mobile",
            "http://127.0.0.1:8321/mobile",
            "http://127.1.2.3:8321/mobile",
            "http://0.0.0.0:8321/mobile",
            "http://[::1]:8321/mobile",
            "http://[::]:8321/mobile",
            "http://169.254.10.20:8321/mobile",
            "http://[fe80::1]:8321/mobile",
            "http://fe80::1:8321/mobile",
        ] {
            assert_eq!(validated_relay_direct_endpoint(bad), None, "for {bad}");
        }
    }

    #[test]
    fn relay_endpoint_rejects_malformed() {
        for bad in [
            "https://192.168.1.10:8321/other",           // wrong path
            "http://192.168.1.10:8321/mobile?x=1",       // query
            "http://192.168.1.10:8321/mobile#frag",      // fragment
            "http://user:pass@192.168.1.10:8321/mobile", // userinfo
            "http://192.168.1.10:8321",                  // no path
            "http://192.168.1.10/mobile",                // no port
            "http://192.168.1.10:0/mobile",              // port 0
            "http://192.168.1.10:99999/mobile",          // port out of range
            "http://:8321/mobile",                       // empty host
            "ftp://192.168.1.10:8321/mobile",            // wrong scheme
            "not a url",
            "",
        ] {
            assert_eq!(validated_relay_direct_endpoint(bad), None, "for {bad}");
        }
    }

    // MARK: - RelayFallbackRetryPolicy

    #[test]
    fn relay_fallback_retry_policy_timing() {
        let completed = Instant::now();
        let retry_after = RelayFallbackRetryPolicy::retry_after_failure(completed);
        assert_eq!(
            retry_after.duration_since(completed),
            RelayFallbackRetryPolicy::FAILURE_DELAY
        );
        assert_eq!(
            RelayFallbackRetryPolicy::FAILURE_DELAY,
            Duration::from_secs(12)
        );
        assert!(!RelayFallbackRetryPolicy::can_attempt(
            completed,
            retry_after
        ));
        assert!(RelayFallbackRetryPolicy::can_attempt(
            retry_after,
            retry_after
        ));
        assert!(RelayFallbackRetryPolicy::can_attempt(
            retry_after + Duration::from_secs(1),
            retry_after
        ));
    }

    // MARK: - evaluate_credential_repair

    #[test]
    fn repair_outcome_matrix() {
        let creds = test_relay_credentials();
        // 404: the Host has no recovery route — not a transient failure.
        assert_eq!(
            evaluate_credential_repair(
                Err(CredentialRepairFetchError::HttpStatus(404)),
                "mac-1",
                |_| panic!("must not persist on 404")
            ),
            RelayCredentialRepairOutcome::RecoveryUnavailable
        );
        // Other failures are transient.
        assert_eq!(
            evaluate_credential_repair(
                Err(CredentialRepairFetchError::HttpStatus(500)),
                "mac-1",
                |_| panic!("must not persist on failure")
            ),
            RelayCredentialRepairOutcome::FetchFailed
        );
        assert_eq!(
            evaluate_credential_repair(
                Err(CredentialRepairFetchError::Transport("dns".to_string())),
                "mac-1",
                |_| panic!("must not persist on failure")
            ),
            RelayCredentialRepairOutcome::FetchFailed
        );
        // Invalid credentials are never persisted.
        let mut wrong_host = creds.clone();
        wrong_host.mac_id = "mac-2".to_string();
        assert_eq!(
            evaluate_credential_repair(Ok(wrong_host), "mac-1", |_| panic!(
                "must not persist invalid credentials"
            )),
            RelayCredentialRepairOutcome::InvalidResponse
        );
        let mut bad_url = creds.clone();
        bad_url.relay_url = "ws://insecure.example.com".to_string();
        assert_eq!(
            evaluate_credential_repair(Ok(bad_url), "mac-1", |_| panic!(
                "must not persist invalid credentials"
            )),
            RelayCredentialRepairOutcome::InvalidResponse
        );
        // A failed write is load-bearing: never claim ready.
        assert_eq!(
            evaluate_credential_repair(Ok(creds.clone()), "mac-1", |_| false),
            RelayCredentialRepairOutcome::PersistenceFailed
        );
        // Happy path.
        assert_eq!(
            evaluate_credential_repair(Ok(creds), "mac-1", |_| true),
            RelayCredentialRepairOutcome::Refreshed
        );
    }

    // MARK: - is_transport_authenticated

    #[test]
    fn transport_authentication_matrix() {
        // Relay (E2E device key) authenticates regardless of pin.
        assert!(is_transport_authenticated(true, None));
        assert!(is_transport_authenticated(true, Some("fp")));
        // Pinned TLS authenticates.
        assert!(is_transport_authenticated(false, Some("fp")));
        // Plaintext LAN reply does not.
        assert!(!is_transport_authenticated(false, None));
    }

    // MARK: - Generation guards

    fn direct_snapshot(token: &str) -> ClientIdentitySnapshot {
        ClientIdentitySnapshot {
            is_relay: false,
            base_url: "http://192.168.1.10:8321/mobile".to_string(),
            auth_token: token.to_string(),
            tls_fingerprint: None,
            relay_session_id: None,
        }
    }

    fn relay_snapshot(token: &str, session: u64) -> ClientIdentitySnapshot {
        ClientIdentitySnapshot {
            is_relay: true,
            base_url: "http://192.168.1.10:8321/mobile".to_string(),
            auth_token: token.to_string(),
            tls_fingerprint: None,
            relay_session_id: Some(session),
        }
    }

    #[test]
    fn direct_generation_capture_and_current() {
        let candidate = direct_snapshot("tok-1");
        let active = direct_snapshot("tok-1");
        let gen = DirectClientGeneration::capture(
            &candidate,
            &active,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            None,
            "tok-1",
            7,
        )
        .expect("capture");
        assert!(gen.is_current(
            7,
            &active,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            None,
            "tok-1"
        ));
        // Epoch moved on: superseded.
        assert!(!gen.is_current(
            8,
            &active,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            None,
            "tok-1"
        ));
        // Token rotated (re-pair): superseded.
        let rotated = direct_snapshot("tok-2");
        assert!(!gen.is_current(
            7,
            &rotated,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            None,
            "tok-2"
        ));
    }

    #[test]
    fn direct_generation_capture_rejects_relay_and_divergence() {
        let direct = direct_snapshot("tok-1");
        let relay = relay_snapshot("tok-1", 99);
        // A relay client never enters the Direct repair path.
        assert!(DirectClientGeneration::capture(
            &relay,
            &direct,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            None,
            "tok-1",
            7
        )
        .is_none());
        assert!(DirectClientGeneration::capture(
            &direct,
            &relay,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            None,
            "tok-1",
            7
        )
        .is_none());
        // Divergent bearer: stale candidate.
        let stale = direct_snapshot("tok-old");
        assert!(DirectClientGeneration::capture(
            &stale,
            &direct,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            None,
            "tok-1",
            7
        )
        .is_none());
        // A pinned-TLS successor is a different generation from plaintext.
        let pinned = ClientIdentitySnapshot {
            tls_fingerprint: Some("fp".to_string()),
            ..direct_snapshot("tok-1")
        };
        assert!(DirectClientGeneration::capture(
            &pinned,
            &direct,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            None,
            "tok-1",
            7
        )
        .is_none());
    }

    #[test]
    fn relay_generation_capture_and_session_binding() {
        let active = relay_snapshot("tok-1", 99);
        let gen = RelayClientGeneration::capture(
            &active,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            "tok-1",
            7,
        )
        .expect("capture");
        assert!(gen.is_current(
            7,
            &active,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            "tok-1"
        ));
        // New relay session (same-Mac ABA): not current.
        let rebound = relay_snapshot("tok-1", 100);
        assert!(!gen.is_current(
            7,
            &rebound,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            "tok-1"
        ));
        // Moved back to Direct: not current.
        let direct = direct_snapshot("tok-1");
        assert!(!gen.is_current(
            7,
            &direct,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            "tok-1"
        ));
        // No session id: no capture.
        let no_session = ClientIdentitySnapshot {
            relay_session_id: None,
            ..relay_snapshot("tok-1", 99)
        };
        assert!(RelayClientGeneration::capture(
            &no_session,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            "tok-1",
            7
        )
        .is_none());
    }

    #[test]
    fn generation_debug_redacts_bearer() {
        let snapshot = direct_snapshot("super-secret-bearer");
        let debug = format!("{snapshot:?}");
        assert!(!debug.contains("super-secret-bearer"));
        assert!(debug.contains("<redacted>"));
        let gen = DirectClientGeneration::capture(
            &snapshot,
            &snapshot,
            "mac-1",
            "http://192.168.1.10:8321/mobile",
            None,
            "super-secret-bearer",
            7,
        )
        .expect("capture");
        let debug = format!("{gen:?}");
        assert!(!debug.contains("super-secret-bearer"));
    }

    // MARK: - Hydration

    #[test]
    fn hydration_prunes_only_on_explicit_not_found() {
        let records = vec![
            test_record("mac-1"),
            test_record("mac-2"),
            test_record("mac-3"),
        ];
        let result = resolve_paired_host_hydration(records, Some("mac-2"), |id| match id {
            "mac-1" => Ok(Some("tok-1".to_string())),
            "mac-2" => Ok(None),
            _ => Err(-25308),
        });
        // mac-2 pruned (explicit not-found); mac-3 retained despite the
        // inconclusive read.
        assert_eq!(result.records.len(), 2);
        assert!(result.records.iter().any(|r| r.host_id == "mac-1"));
        assert!(result.records.iter().any(|r| r.host_id == "mac-3"));
        assert!(result.is_temporarily_unavailable());
        assert_eq!(result.unavailable_statuses, vec![-25308]);
        // Preferred Mac was pruned: fall back to the first retained.
        assert_eq!(result.active_record.unwrap().host_id, "mac-1");
        assert_eq!(result.active_token.as_deref(), Some("tok-1"));
    }

    #[test]
    fn hydration_keeps_empty_bearer_record_without_token() {
        let records = vec![test_record("mac-1")];
        let result = resolve_paired_host_hydration(records, None, |_| Ok(Some(String::new())));
        // Retained (never destructively unpaired), but no usable token.
        assert_eq!(result.records.len(), 1);
        assert!(!result.is_temporarily_unavailable());
        assert_eq!(result.active_record.unwrap().host_id, "mac-1");
        assert_eq!(result.active_token, None);
    }

    #[test]
    fn hydration_empty_when_nothing_retained() {
        let result = resolve_paired_host_hydration(vec![test_record("mac-1")], None, |_| Ok(None));
        assert!(result.records.is_empty());
        assert!(result.active_record.is_none());
        assert!(result.active_token.is_none());
    }

    #[test]
    fn direct_activation_bumps_epoch_and_reuses_record() {
        let records = vec![test_record("mac-1")];
        let hydration =
            resolve_paired_host_hydration(records, None, |_| Ok(Some("tok-1".to_string())));
        let (record, token, epoch) = direct_activation(&hydration, 41).expect("activation");
        assert_eq!(record.host_id, "mac-1");
        assert_eq!(record.controller_device_id, "device-1");
        assert_eq!(token, "tok-1");
        assert_eq!(epoch, 42);
    }

    #[test]
    fn direct_activation_needs_a_usable_token() {
        let records = vec![test_record("mac-1")];
        let hydration = resolve_paired_host_hydration(records, None, |_| Ok(Some(String::new())));
        assert!(direct_activation(&hydration, 41).is_none());
    }
}
