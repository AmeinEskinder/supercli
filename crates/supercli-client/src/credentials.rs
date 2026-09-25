//! Platform credential storage for pairing secrets.
//!
//! `pair()` returns an auth token and relay credentials that must live in
//! the platform keychain immediately after pairing — never in a config
//! file. This module abstracts the platform store behind
//! [`CredentialStore`] so the UI layer can inject a mock in tests and the
//! production app uses [`KeyringStore`] (macOS Keychain, Windows
//! Credential Manager, Linux Secret Service via the `keyring` crate).
//!
//! Secrets are stored as one JSON blob per Host under the service
//! `com.unpeel.controller`, account `host:{host_id}`. The non-secret
//! [`PairedHostRecord`] list and the Controller's device identity also live
//! in the keychain (accounts `paired-hosts`, `device-identity`) — one
//! storage seam, no home-dir convention, and it works inside the
//! iOS/Android app sandboxes.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::pairing::PairedHostRecord;
use crate::relay::RelayCredentials;

/// Keychain service under which all Controller secrets are stored.
pub const KEYCHAIN_SERVICE: &str = "com.unpeel.controller";

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("credential store unavailable: {0}")]
    Unavailable(String),
    #[error("credential store failed: {0}")]
    Store(String),
    #[error("corrupt credential blob: {0}")]
    Corrupt(String),
}

/// The secrets pairing produces for one Host. Everything here is
/// keychain-only: the Host stores the raw E2E key and only the SHA-256 of
/// the relay token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostSecrets {
    pub auth_token: String,
    pub relay_token: String,
    /// Base64 of the 32-byte relay E2E key (matches `e2eKeyB64` on the
    /// wire; kept as the wire string so no re-encoding can drift).
    pub e2e_key_b64: String,
    /// The relay's `wss://` URL from the pairing response. `None` for
    /// Hosts paired before this field existed: they can't use the relay
    /// fallback until they're paired again. `Option` + serde default
    /// keeps old keychain blobs readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_url: Option<String>,
}

impl HostSecrets {
    pub fn e2e_key(&self) -> Option<[u8; 32]> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&self.e2e_key_b64)
            .ok()?;
        if bytes.len() != 32 {
            return None;
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Some(key)
    }
}

/// Rebuild the relay credentials for a paired Host from its stored
/// secrets, for the Direct→relay fallback.
///
/// Returns `None` when the secrets predate the stored relay URL (paired
/// before the fallback existed — re-pair to enable it), when the stored
/// URL isn't `wss://`, or when the E2E key is malformed. The mac id is the
/// record's `host_id` (see
/// [`PairedHostRecord::from_pairing_response`](crate::pairing::PairedHostRecord::from_pairing_response)).
pub fn relay_credentials_for_host(
    record: &PairedHostRecord,
    secrets: &HostSecrets,
) -> Option<RelayCredentials> {
    let relay_url = secrets.relay_url.as_deref()?;
    if !relay_url.to_lowercase().starts_with("wss://") {
        return None;
    }
    if secrets.e2e_key().is_none() || secrets.relay_token.is_empty() {
        return None;
    }
    Some(RelayCredentials {
        relay_url: relay_url.to_string(),
        mac_id: record.host_id.clone(),
        relay_token: secrets.relay_token.clone(),
        e2e_key_b64: secrets.e2e_key_b64.clone(),
    })
}

/// Platform secret storage. Keys are namespaced by the implementation;
/// callers use the typed helpers below.
pub trait CredentialStore: Send + Sync {
    fn set_secret(&self, account: &str, secret: &[u8]) -> Result<(), CredentialError>;
    fn get_secret(&self, account: &str) -> Result<Option<Vec<u8>>, CredentialError>;
    fn delete_secret(&self, account: &str) -> Result<(), CredentialError>;
}

/// Production store backed by the OS keychain.
#[derive(Debug, Clone)]
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    /// Store under the Controller's keychain service.
    pub fn new() -> Self {
        Self::with_service(KEYCHAIN_SERVICE)
    }

    /// Store under a custom keychain service (e.g. the Host's connector
    /// namespace).
    pub fn with_service(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry, CredentialError> {
        keyring::Entry::new(&self.service, account)
            .map_err(|e| CredentialError::Unavailable(e.to_string()))
    }
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for KeyringStore {
    fn set_secret(&self, account: &str, secret: &[u8]) -> Result<(), CredentialError> {
        let secret_str =
            std::str::from_utf8(secret).map_err(|e| CredentialError::Store(e.to_string()))?;
        self.entry(account).and_then(|entry| {
            entry
                .set_password(secret_str)
                .map_err(|e| CredentialError::Store(e.to_string()))
        })
    }

    fn get_secret(&self, account: &str) -> Result<Option<Vec<u8>>, CredentialError> {
        match self.entry(account)?.get_password() {
            Ok(password) => Ok(Some(password.into_bytes())),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CredentialError::Store(e.to_string())),
        }
    }

    fn delete_secret(&self, account: &str) -> Result<(), CredentialError> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CredentialError::Store(e.to_string())),
        }
    }
}

/// In-memory store for tests and headless environments. Contents are lost
/// when the process exits — never use it for real pairing secrets.
#[derive(Debug, Default)]
pub struct MemoryStore {
    inner: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
}

impl CredentialStore for MemoryStore {
    fn set_secret(&self, account: &str, secret: &[u8]) -> Result<(), CredentialError> {
        self.inner
            .lock()
            .map_err(|e| CredentialError::Store(e.to_string()))?
            .insert(account.to_string(), secret.to_vec());
        Ok(())
    }

    fn get_secret(&self, account: &str) -> Result<Option<Vec<u8>>, CredentialError> {
        Ok(self
            .inner
            .lock()
            .map_err(|e| CredentialError::Store(e.to_string()))?
            .get(account)
            .cloned())
    }

    fn delete_secret(&self, account: &str) -> Result<(), CredentialError> {
        self.inner
            .lock()
            .map_err(|e| CredentialError::Store(e.to_string()))?
            .remove(account);
        Ok(())
    }
}

fn account_for_host(host_id: &str) -> String {
    format!("host:{host_id}")
}

/// Persist one Host's pairing secrets to the keychain. Overwrites any
/// previous secrets for the Host.
pub fn store_host_secrets(
    store: &dyn CredentialStore,
    host_id: &str,
    secrets: &HostSecrets,
) -> Result<(), CredentialError> {
    let blob = serde_json::to_vec(secrets).map_err(|e| CredentialError::Store(e.to_string()))?;
    store.set_secret(&account_for_host(host_id), &blob)
}

/// Load one Host's pairing secrets. `Ok(None)` means the Host was never
/// paired (or was unpaired) on this device.
pub fn load_host_secrets(
    store: &dyn CredentialStore,
    host_id: &str,
) -> Result<Option<HostSecrets>, CredentialError> {
    match store.get_secret(&account_for_host(host_id))? {
        None => Ok(None),
        Some(blob) => serde_json::from_slice(&blob)
            .map_err(|e| CredentialError::Corrupt(e.to_string()))
            .map(Some),
    }
}

/// Remove one Host's secrets — the unpair operation. Idempotent.
pub fn delete_host_secrets(
    store: &dyn CredentialStore,
    host_id: &str,
) -> Result<(), CredentialError> {
    store.delete_secret(&account_for_host(host_id))
}

/// Open the Controller's credential store: the OS keychain when it
/// answers, otherwise a process-lifetime in-memory store. The second return
/// value is a user-facing notice set only when falling back — pairing will
/// not survive an app restart in that mode, and the UI should say so.
pub fn open_controller_store() -> (Arc<dyn CredentialStore>, Option<String>) {
    let keyring = KeyringStore::new();
    match keyring.get_secret("__probe__") {
        Ok(_) => (Arc::new(keyring), None),
        Err(e) => (
            Arc::new(MemoryStore::default()),
            Some(format!(
                "System keychain unavailable ({e}); pairing will not be remembered after the app restarts."
            )),
        ),
    }
}

/// Convenience: what the app keeps per paired Host — the non-secret record
/// plus its keychain secrets, if present.
pub struct PairedHost {
    pub record: PairedHostRecord,
    pub secrets: Option<HostSecrets>,
}

impl PairedHost {
    pub fn load(
        store: &dyn CredentialStore,
        record: PairedHostRecord,
    ) -> Result<Self, CredentialError> {
        let secrets = load_host_secrets(store, &record.host_id)?;
        Ok(Self { record, secrets })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn test_secrets() -> HostSecrets {
        HostSecrets {
            auth_token: "auth-1".to_string(),
            relay_token: "relay-1".to_string(),
            e2e_key_b64: base64::engine::general_purpose::STANDARD.encode([3u8; 32]),
            relay_url: None,
        }
    }

    #[test]
    fn memory_store_roundtrip() {
        let store = MemoryStore::default();
        let secrets = test_secrets();
        assert_eq!(load_host_secrets(&store, "h1").unwrap(), None);
        store_host_secrets(&store, "h1", &secrets).unwrap();
        assert_eq!(load_host_secrets(&store, "h1").unwrap(), Some(secrets));
        // Hosts are namespaced from each other.
        assert_eq!(load_host_secrets(&store, "h2").unwrap(), None);
        delete_host_secrets(&store, "h1").unwrap();
        assert_eq!(load_host_secrets(&store, "h1").unwrap(), None);
        // Deleting twice is fine (idempotent unpair).
        delete_host_secrets(&store, "h1").unwrap();
    }

    #[test]
    fn corrupt_blob_is_an_error_not_a_panic() {
        let store = MemoryStore::default();
        store.set_secret("host:h1", b"not json").unwrap();
        assert!(matches!(
            load_host_secrets(&store, "h1"),
            Err(CredentialError::Corrupt(_))
        ));
    }

    #[test]
    fn e2e_key_decodes() {
        let secrets = test_secrets();
        assert_eq!(secrets.e2e_key(), Some([3u8; 32]));
        let mut bad = secrets.clone();
        bad.e2e_key_b64 = "!!!".to_string();
        assert_eq!(bad.e2e_key(), None);
    }
}
