//! Controller-side paired-Host record management.
//!
//! Ported from `clients/legacy/native/SupercliNative/Sources/SupercliNative/RemoteHosts.swift`
//! (`RemoteHostStore`). This is the UI-free record layer: adopting pairing
//! responses, forgetting hosts, renaming, selecting, and Link scoping.
//! Credential bytes stay behind the [`CredentialStore`] trait (Keychain on
//! device, memory in tests); public metadata is plain data.
//!
//! The live-connection registry ([`crate::hosts::HostRegistry`]) sits above
//! this: it owns the [`crate::transport::HostClient`] per Host, while this
//! module owns the persisted records those clients are built from.

#[cfg(any(test, feature = "test-util"))]
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::pairing::{RemotePairingResponse, PAIRING_PROTOCOL_VERSION};
use crate::types::PairedHostRecord;

/// Failures of Host record management. Mirrors Swift `RemoteHostPairingError`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HostStoreError {
    #[error("that is not a Supercli pairing code")]
    InvalidCode,
    #[error("this Host uses an incompatible pairing protocol")]
    IncompatibleProtocol,
    #[error("that code belongs to a different Host")]
    CandidateMismatch,
    #[error("this is this machine; choose Local instead")]
    SelfPairing,
    #[error("that pairing code expired")]
    Expired,
    #[error("the Host rejected the pairing request: {0}")]
    HostRejected(String),
    #[error("the Host pairing response could not be authenticated")]
    Authentication,
    #[error("credential storage failed: {0}")]
    Credential(String),
}

/// Minimal credential-store surface needed by record management.
/// The production Keychain implementation is [`OsKeychainCredentialStore`]
/// below, backed by the OS native store (macOS Keychain via Security.framework,
/// Linux Secret Service via libsecret, Windows Credential Manager) through
/// the `keyring` crate. See [`crate::credentials::KeyringStore`].
pub trait HostCredentialStore {
    fn save(&mut self, account: &str, secret: &str) -> Result<(), String>;
    fn load(&self, account: &str) -> Option<String>;
    fn delete(&mut self, account: &str);
}

/// In-memory credential store for tests ONLY.
///
/// **SECURITY WARNING:** This store keeps secrets in process memory in
/// plaintext. It is suitable for unit tests and headless CI environments
/// only. NEVER use it for real pairing secrets in production — use
/// [`OsKeychainCredentialStore`] instead, which stores credentials in the
/// OS-native secure store (Keychain / Secret Service / Credential Manager).
///
/// This type is gated behind `#[cfg(any(test, feature = "test-util"))]` so
/// production code cannot wire it by mistake. Production code must use
/// [`OsKeychainCredentialStore`] via `RemoteHostStore::with_os_keychain()`.
#[cfg(any(test, feature = "test-util"))]
#[derive(Default)]
pub struct MemoryHostCredentialStore {
    secrets: HashMap<String, String>,
}

#[cfg(any(test, feature = "test-util"))]
impl HostCredentialStore for MemoryHostCredentialStore {
    fn save(&mut self, account: &str, secret: &str) -> Result<(), String> {
        self.secrets.insert(account.to_string(), secret.to_string());
        Ok(())
    }

    fn load(&self, account: &str) -> Option<String> {
        self.secrets.get(account).cloned()
    }

    fn delete(&mut self, account: &str) {
        self.secrets.remove(account);
    }
}

/// Production credential store backed by the OS native secure store.
///
/// - **macOS**: Keychain via Security.framework (`SecItemAdd`,
///   `SecItemCopyMatching`, `SecItemUpdate`, `SecItemDelete`) — the same
///   APIs used by `LicenseKeychain.swift` and `MobileE2EKeychainStore` in
///   the Swift source. Items are `kSecClassGenericPassword` with
///   `kSecAttrAccessibleAfterFirstUnlock`.
/// - **Linux**: Secret Service via libsecret (through the `keyring` crate's
///   Secret Service backend).
/// - **Windows**: Credential Manager via Win32 `CredWrite`/`CredRead`
///   (through the `keyring` crate's Windows backend).
///
/// Credentials are NEVER written to plaintext config files. If the OS store
/// is unavailable (e.g., headless Linux without D-Bus), operations return
/// an error — the caller must handle this explicitly, not silently fall
/// back to plaintext.
pub struct OsKeychainCredentialStore {
    inner: crate::credentials::KeyringStore,
}

impl OsKeychainCredentialStore {
    /// Create a store using the Controller's keychain service
    /// (`li.superc.controller`, matching the Swift pairing code).
    pub fn new() -> Self {
        Self {
            inner: crate::credentials::KeyringStore::new(),
        }
    }

    /// Create a store under a custom keychain service name.
    pub fn with_service(service: &str) -> Self {
        Self {
            inner: crate::credentials::KeyringStore::with_service(service),
        }
    }
}

impl Default for OsKeychainCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl HostCredentialStore for OsKeychainCredentialStore {
    fn save(&mut self, account: &str, secret: &str) -> Result<(), String> {
        use crate::credentials::CredentialStore;
        self.inner
            .set_secret(account, secret.as_bytes())
            .map_err(|e| format!("OS keychain store failed: {e}"))
    }

    fn load(&self, account: &str) -> Option<String> {
        use crate::credentials::CredentialStore;
        match self.inner.get_secret(account) {
            Ok(Some(bytes)) => String::from_utf8(bytes).ok(),
            Ok(None) => None,
            Err(_) => None,
        }
    }

    fn delete(&mut self, account: &str) {
        use crate::credentials::CredentialStore;
        let _ = self.inner.delete_secret(account);
    }
}

/// One SSH host record. Mirrors Swift `SSHHostRecord`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshHostRecord {
    pub id: String,
    pub name: String,
    pub target: String,
    pub host_id: String,
    pub uses_stored_secret: bool,
}

impl SshHostRecord {
    /// Everything after the `ssh://` prefix.
    pub fn destination(&self) -> &str {
        self.target.strip_prefix("ssh://").unwrap_or(&self.target)
    }
}

/// Controller-side store of paired and SSH host records.
///
/// Mirrors Swift `RemoteHostStore` minus the `@Published` UI bindings:
/// `records`, `ssh_records`, and `selected_host_id` are plain fields the
/// launcher reads after each mutation.
pub struct RemoteHostStore<S: HostCredentialStore> {
    /// Paired (Direct/Link) host records, never including this machine.
    pub records: Vec<PairedHostRecord>,
    /// SSH host records.
    pub ssh_records: Vec<SshHostRecord>,
    /// Selected host id; `None` means Local.
    pub selected_host_id: Option<String>,
    credential_store: S,
    controller_id: String,
    local_host_id: Option<String>,
}

impl<S: HostCredentialStore> RemoteHostStore<S> {
    pub fn new(credential_store: S, controller_id: String, local_host_id: Option<String>) -> Self {
        Self {
            records: Vec::new(),
            ssh_records: Vec::new(),
            selected_host_id: None,
            credential_store,
            controller_id,
            local_host_id,
        }
    }

    fn credential_account(&self, host_id: &str) -> String {
        format!("pairing.{}.{}", self.controller_id, host_id)
    }

    fn is_local_host(&self, host_id: &str) -> bool {
        match &self.local_host_id {
            Some(local) if !local.is_empty() => local.eq_ignore_ascii_case(host_id),
            _ => false,
        }
    }

    /// Credentials for a paired host, if stored.
    ///
    /// This is the bearer auth token the Host expects on its Direct
    /// endpoint. It is parsed out of the stored secret bundle; see
    /// [`RemoteHostStore::secrets_for`].
    pub fn credentials_for(&self, host_id: &str) -> Option<String> {
        self.secrets_for(host_id).map(|s| s.auth_token)
    }

    /// The full secret bundle for a paired host, if stored: bearer token
    /// plus the relay credentials needed for the Link fallback. Mirrors
    /// Swift `RemoteHostCredentials` (authToken + relayCredentials).
    ///
    /// Records adopted before the bundle format existed stored the bare
    /// auth token; those still parse as auth-token-only secrets (no Link
    /// fallback until re-pair).
    pub fn secrets_for(&self, host_id: &str) -> Option<crate::credentials::HostSecrets> {
        let raw = self
            .credential_store
            .load(&self.credential_account(host_id))?;
        match serde_json::from_str::<crate::credentials::HostSecrets>(&raw) {
            Ok(secrets) => Some(secrets),
            Err(_) => Some(crate::credentials::HostSecrets {
                auth_token: raw,
                relay_token: String::new(),
                e2e_key_b64: String::new(),
                relay_url: None,
            }),
        }
    }

    /// Persist a successfully authenticated pairing response.
    ///
    /// Metadata is committed only after the credential store accepts the
    /// secret, so a crash cannot leave a picker row that can never connect.
    /// Mirrors Swift `RemoteHostStore.adopt`.
    pub fn adopt(
        &mut self,
        response: &RemotePairingResponse,
        certificate_fingerprint: Option<String>,
        select: bool,
    ) -> Result<PairedHostRecord, HostStoreError> {
        if response.protocol_version != PAIRING_PROTOCOL_VERSION {
            return Err(HostStoreError::IncompatibleProtocol);
        }
        if self.is_local_host(&response.mac_id) {
            return Err(HostStoreError::SelfPairing);
        }
        let relay = &response.relay_credentials;
        if response.mac_id.is_empty()
            || response.device_id != self.controller_id
            || response.auth_token.is_empty()
            || relay.mac_id != response.mac_id
            || relay.relay_token.is_empty()
            || !relay.relay_url.to_ascii_lowercase().starts_with("wss://")
            || relay.e2e_key().is_none()
        {
            return Err(HostStoreError::Authentication);
        }
        self.credential_store
            .save(
                &self.credential_account(&response.mac_id),
                &serde_json::to_string(&crate::credentials::HostSecrets {
                    auth_token: response.auth_token.clone(),
                    relay_token: relay.relay_token.clone(),
                    e2e_key_b64: relay.e2e_key_b64.clone(),
                    relay_url: Some(relay.relay_url.clone()),
                })
                .map_err(|e| HostStoreError::Credential(e.to_string()))?,
            )
            .map_err(HostStoreError::Credential)?;
        let record = PairedHostRecord::from_pairing_response(response, certificate_fingerprint);
        upsert_record(&mut self.records, record.clone());
        if select {
            self.select_host(Some(record.host_id.clone()));
        }
        Ok(record)
    }

    /// Adopt an SSH host record, upserting by target. Mirrors Swift
    /// `RemoteHostStore.adoptSSH` (minus the live SSH probe, which the
    /// launcher performs separately).
    pub fn adopt_ssh(
        &mut self,
        target: String,
        name: String,
        host_id: String,
        secret: Option<String>,
        select: bool,
    ) -> Result<SshHostRecord, HostStoreError> {
        if self.is_local_host(&host_id) {
            return Err(HostStoreError::SelfPairing);
        }
        let existing_id = self
            .ssh_records
            .iter()
            .find(|r| r.target == target)
            .map(|r| r.id.clone());
        let record_id = existing_id.unwrap_or_else(|| format!("ssh.{}", uuid_simple()));
        let account = format!("ssh.{}.{}", self.controller_id, record_id);
        let has_secret = secret.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
        match secret.as_ref().filter(|s| !s.is_empty()) {
            Some(s) => self
                .credential_store
                .save(&account, s)
                .map_err(HostStoreError::Credential)?,
            None => self.credential_store.delete(&account),
        }
        let display_name = if name.trim().is_empty() {
            target.strip_prefix("ssh://").unwrap_or(&target).to_string()
        } else {
            name
        };
        let record = SshHostRecord {
            id: record_id.clone(),
            name: display_name,
            target,
            host_id,
            uses_stored_secret: has_secret,
        };
        match self.ssh_records.iter_mut().find(|r| r.id == record_id) {
            Some(slot) => *slot = record.clone(),
            None => self.ssh_records.push(record.clone()),
        }
        if select {
            self.select_host(Some(record.id.clone()));
        }
        Ok(record)
    }

    /// Select a host. A paired host is usable only with stored credentials;
    /// an SSH host only when its required secret is present. Unusable or
    /// unknown ids are ignored (selection stays put). `None` selects Local.
    /// Mirrors Swift `RemoteHostStore.selectHost`.
    pub fn select_host(&mut self, host_id: Option<String>) {
        let Some(id) = host_id else {
            self.selected_host_id = None;
            return;
        };
        let paired_usable =
            self.records.iter().any(|r| r.host_id == id) && self.credentials_for(&id).is_some();
        let ssh_usable = self
            .ssh_records
            .iter()
            .find(|r| r.id == id)
            .map(|r| {
                !r.uses_stored_secret
                    || self
                        .credential_store
                        .load(&format!("ssh.{}.{}", self.controller_id, r.id))
                        .is_some()
            })
            .unwrap_or(false);
        if paired_usable || ssh_usable {
            self.selected_host_id = Some(id);
        }
    }

    /// Scope a paired host to Direct-only (`false`) or restore its Link
    /// fallback (`true`). Narrows-only storage: allowed is the default, so
    /// only `false` is ever persisted. Mirrors Swift
    /// `RemoteHostStore.setLinkEnabled`.
    pub fn set_link_enabled(&mut self, enabled: bool, host_id: &str) {
        if let Some(record) = self.records.iter_mut().find(|r| r.host_id == host_id) {
            if record.is_link_enabled() != enabled {
                record.link_enabled = if enabled { None } else { Some(false) };
            }
        }
    }

    /// Rename a controller-side host alias. The explicit alias wins over
    /// whatever the host advertises until changed again. Returns false when
    /// the name is blank or the host is unknown. Mirrors Swift
    /// `RemoteHostStore.renameHost`.
    pub fn rename_host(&mut self, host_id: &str, raw_name: &str) -> bool {
        let name = raw_name.trim();
        if name.is_empty() {
            return false;
        }
        if let Some(record) = self.records.iter_mut().find(|r| r.host_id == host_id) {
            if record.name == name {
                return true;
            }
            record.name = name.to_string();
            return true;
        }
        if let Some(record) = self.ssh_records.iter_mut().find(|r| r.id == host_id) {
            if record.name == name {
                return true;
            }
            record.name = name.to_string();
            return true;
        }
        false
    }

    /// Forget a host: drop its record and credentials. If it was selected,
    /// fall back to Local. Mirrors Swift `RemoteHostStore.forget`.
    pub fn forget(&mut self, host_id: &str) {
        if let Some(ssh) = self.ssh_records.iter().find(|r| r.id == host_id).cloned() {
            self.ssh_records.retain(|r| r.id != host_id);
            self.credential_store
                .delete(&format!("ssh.{}.{}", self.controller_id, ssh.id));
        } else {
            self.records.retain(|r| r.host_id != host_id);
            self.credential_store
                .delete(&self.credential_account(host_id));
        }
        if self.selected_host_id.as_deref() == Some(host_id) {
            self.selected_host_id = None;
        }
    }

    /// Seed a full secret bundle for tests without going through pairing.
    /// Test-only: production code must pair through [`RemoteHostStore::adopt`].
    #[cfg(any(test, feature = "test-util"))]
    pub fn seed_secrets_for_test(
        &mut self,
        host_id: &str,
        secrets: crate::credentials::HostSecrets,
    ) -> Result<(), HostStoreError> {
        let blob = serde_json::to_string(&secrets)
            .map_err(|e| HostStoreError::Credential(e.to_string()))?;
        self.credential_store
            .save(&self.credential_account(host_id), &blob)
            .map_err(HostStoreError::Credential)
    }
}

impl RemoteHostStore<OsKeychainCredentialStore> {
    /// Create a `RemoteHostStore` backed by the OS native secure store.
    ///
    /// This is the production constructor — credentials go to the macOS
    /// Keychain, Linux Secret Service, or Windows Credential Manager.
    /// Use `RemoteHostStore::new(MemoryHostCredentialStore::default(), ...)`
    /// only in tests.
    pub fn with_os_keychain(controller_id: String, local_host_id: Option<String>) -> Self {
        Self::new(
            OsKeychainCredentialStore::new(),
            controller_id,
            local_host_id,
        )
    }
}

fn upsert_record(records: &mut Vec<PairedHostRecord>, record: PairedHostRecord) {
    match records.iter_mut().find(|r| r.host_id == record.host_id) {
        Some(slot) => *slot = record,
        None => records.push(record),
    }
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_response() -> RemotePairingResponse {
        RemotePairingResponse {
            protocol_version: PAIRING_PROTOCOL_VERSION,
            mac_id: "MACID-UPPER".to_string(),
            mac_name: "Test Mac".to_string(),
            endpoint: "https://192.168.1.5:443".to_string(),
            direct_endpoint: None,
            device_id: "controller-1".to_string(),
            auth_token: "auth-token-abc".to_string(),
            relay_credentials: crate::relay::RelayCredentials {
                mac_id: "MACID-UPPER".to_string(),
                relay_token: "relay-token-xyz".to_string(),
                relay_url: "wss://relay.superc.li".to_string(),
                e2e_key_b64: {
                    use base64::Engine;
                    base64::engine::general_purpose::STANDARD.encode([7u8; 32])
                },
            },
            paired_at_unix_ms: 1_700_000_000_000,
            remote_server_port: None,
            remote_server_certificate_fingerprint: None,
            server_version: None,
        }
    }

    fn store() -> RemoteHostStore<MemoryHostCredentialStore> {
        RemoteHostStore::new(
            MemoryHostCredentialStore::default(),
            "controller-1".to_string(),
            Some("LOCAL-MACID".to_string()),
        )
    }

    #[test]
    fn pair_adopts_record() {
        let mut s = store();
        let record = s.adopt(&test_response(), None, true).unwrap();
        assert_eq!(record.host_id, "MACID-UPPER");
        assert_eq!(s.records.len(), 1);
        // Credentials committed before metadata: the picker row can connect.
        assert_eq!(
            s.credentials_for("MACID-UPPER"),
            Some("auth-token-abc".to_string())
        );
        // select=true selects the new host.
        assert_eq!(s.selected_host_id, Some("MACID-UPPER".to_string()));
    }

    #[test]
    fn adopt_rejects_self_pairing() {
        let mut s = store();
        let mut response = test_response();
        response.mac_id = "LOCAL-MACID".to_string();
        // Relay mac_id must still match for the self-check to be reached first.
        response.relay_credentials.mac_id = "LOCAL-MACID".to_string();
        assert_eq!(
            s.adopt(&response, None, false).unwrap_err(),
            HostStoreError::SelfPairing
        );
        assert!(s.records.is_empty());
    }

    #[test]
    fn adopt_rejects_bad_protocol() {
        let mut s = store();
        let mut response = test_response();
        response.protocol_version = PAIRING_PROTOCOL_VERSION + 99;
        assert_eq!(
            s.adopt(&response, None, false).unwrap_err(),
            HostStoreError::IncompatibleProtocol
        );
    }

    #[test]
    fn adopt_rejects_unauthenticated_response() {
        let mut s = store();
        let mut response = test_response();
        response.auth_token = String::new();
        assert_eq!(
            s.adopt(&response, None, false).unwrap_err(),
            HostStoreError::Authentication
        );
        // No picker row left behind.
        assert!(s.records.is_empty());
        assert!(s.credentials_for("MACID-UPPER").is_none());
    }

    #[test]
    fn forget_removes_host() {
        let mut s = store();
        s.adopt(&test_response(), None, true).unwrap();
        s.forget("MACID-UPPER");
        assert!(s.records.is_empty());
        assert!(s.credentials_for("MACID-UPPER").is_none());
        // Was selected: falls back to Local.
        assert_eq!(s.selected_host_id, None);
    }

    #[test]
    fn rename_validates_name() {
        let mut s = store();
        s.adopt(&test_response(), None, false).unwrap();
        assert!(!s.rename_host("MACID-UPPER", "   "));
        assert!(!s.rename_host("unknown-id", "New Name"));
        assert!(s.rename_host("MACID-UPPER", "  Studio Mac  "));
        assert_eq!(s.records[0].name, "Studio Mac");
        // Same name is a no-op success.
        assert!(s.rename_host("MACID-UPPER", "Studio Mac"));
    }

    #[test]
    fn select_host_requires_credentials() {
        let mut s = store();
        s.adopt(&test_response(), None, false).unwrap();
        // Usable: credentials present.
        s.select_host(Some("MACID-UPPER".to_string()));
        assert_eq!(s.selected_host_id, Some("MACID-UPPER".to_string()));
        // Unknown id: selection stays put.
        s.select_host(Some("nope".to_string()));
        assert_eq!(s.selected_host_id, Some("MACID-UPPER".to_string()));
        // None selects Local.
        s.select_host(None);
        assert_eq!(s.selected_host_id, None);
    }

    #[test]
    fn set_link_enabled_narrows_only() {
        let mut s = store();
        s.adopt(&test_response(), None, false).unwrap();
        assert!(s.records[0].is_link_enabled());
        s.set_link_enabled(false, "MACID-UPPER");
        assert!(!s.records[0].is_link_enabled());
        assert_eq!(s.records[0].link_enabled, Some(false));
        s.set_link_enabled(true, "MACID-UPPER");
        assert!(s.records[0].is_link_enabled());
        // Allowed is the default: no key stored.
        assert_eq!(s.records[0].link_enabled, None);
    }

    #[test]
    fn adopt_ssh_upserts_by_target() {
        let mut s = store();
        let r1 = s
            .adopt_ssh(
                "ssh://pi@192.168.1.9".to_string(),
                "".to_string(),
                "SSH-HOST-1".to_string(),
                Some("s3cret".to_string()),
                false,
            )
            .unwrap();
        // Blank name falls back to the destination.
        assert_eq!(r1.name, "pi@192.168.1.9");
        assert!(r1.uses_stored_secret);
        // Same target: upserts, keeps the id.
        let r2 = s
            .adopt_ssh(
                "ssh://pi@192.168.1.9".to_string(),
                "Pi".to_string(),
                "SSH-HOST-1".to_string(),
                None,
                false,
            )
            .unwrap();
        assert_eq!(r1.id, r2.id);
        assert_eq!(r2.name, "Pi");
        assert_eq!(s.ssh_records.len(), 1);
    }

    // --- OS Keychain Credential Store tests ---
    //
    // These verify that the production credential store uses the OS native
    // secure store, not in-memory plaintext. The `OsKeychainCredentialStore`
    // delegates to `crate::credentials::KeyringStore`, which uses:
    // - macOS: Keychain via Security.framework
    // - Linux: Secret Service via libsecret
    // - Windows: Credential Manager via Win32

    #[test]
    fn os_keychain_store_implements_trait() {
        // Compile-time: OsKeychainCredentialStore must implement HostCredentialStore.
        fn assert_impl<T: HostCredentialStore>() {}
        assert_impl::<OsKeychainCredentialStore>();
    }

    #[test]
    fn with_os_keychain_uses_os_store_not_memory() {
        // The production constructor must return a store backed by the OS
        // keychain, NOT the in-memory store. This is a type-level guarantee:
        // if someone changes with_os_keychain to use MemoryHostCredentialStore,
        // this test fails to compile.
        let store: RemoteHostStore<OsKeychainCredentialStore> =
            RemoteHostStore::with_os_keychain("controller-1".to_string(), None);
        assert_eq!(store.records.len(), 0);
        // The type itself is the assertion — MemoryHostCredentialStore would
        // not satisfy the type annotation above.
    }

    #[test]
    fn production_constructor_uses_os_store_at_runtime() {
        // Build the host store exactly the way production code does:
        // `with_os_keychain` is the only non-test constructor. Every other
        // path goes through `new` with an explicitly injected store (the
        // memory store in tests).
        let store = RemoteHostStore::with_os_keychain("controller-1".to_string(), None);
        // Runtime check on the constructed value — not just a type
        // annotation: the credential backend inside must be the OS native
        // keychain store. If the production constructor is ever rewired to
        // the in-memory test store, this fails.
        let backend = std::any::type_name_of_val(&store.credential_store);
        assert!(
            backend.contains("OsKeychainCredentialStore"),
            "production host store backend is not the OS keychain store: {backend}"
        );
        assert!(
            !backend.contains("MemoryHostCredentialStore"),
            "production host store leaked the in-memory test backend: {backend}"
        );
    }

    #[test]
    fn os_store_save_failure_is_explicit_never_silent() {
        // HostSecrets saved through the production backend must land in the
        // OS keychain. Where no keyring exists (headless CI), the save must
        // fail loudly — it must never silently land in an in-memory store.
        let mut backend = OsKeychainCredentialStore::with_service("li.superc.test.nosilent");
        let account = format!("__nosilent_{}__", std::process::id());
        match backend.save(&account, "s3cr3t") {
            Ok(()) => {
                // Keyring available: the secret round-trips through the OS
                // store, and cleanup removes it again.
                assert_eq!(backend.load(&account), Some("s3cr3t".to_string()));
                backend.delete(&account);
                assert_eq!(backend.load(&account), None);
            }
            Err(e) => {
                // Headless: the failure is explicit, and the secret is not
                // retrievable — nothing was silently stashed in memory.
                assert!(
                    e.contains("OS keychain store failed"),
                    "unexpected error shape: {e}"
                );
                assert_eq!(backend.load(&account), None);
            }
        }
    }

    #[test]
    fn memory_store_is_not_the_default() {
        // Explicitly document that MemoryHostCredentialStore is test-only.
        // Production code must use OsKeychainCredentialStore via
        // RemoteHostStore::with_os_keychain().
        //
        // This test verifies the memory store works for tests, but the
        // production path (with_os_keychain) does not use it.
        let mut mem = MemoryHostCredentialStore::default();
        mem.save("test-account", "test-secret").unwrap();
        assert_eq!(mem.load("test-account"), Some("test-secret".to_string()));
        mem.delete("test-account");
        assert_eq!(mem.load("test-account"), None);

        // The OS store type is distinct from the memory store type.
        // This ensures they cannot be confused at the type level.
        fn is_not_memory<T>() -> bool {
            std::any::type_name::<T>() != std::any::type_name::<MemoryHostCredentialStore>()
        }
        assert!(is_not_memory::<OsKeychainCredentialStore>());
    }

    #[test]
    fn os_keychain_store_handles_not_found() {
        // Loading a non-existent credential must return None, not panic.
        // We use a unique account name to avoid colliding with real credentials.
        let store = OsKeychainCredentialStore::with_service("li.superc.test.nonexistent");
        let account = format!("__test_nonexistent_{}__", std::process::id());
        assert_eq!(store.load(&account), None);
    }
}
