//! License storage via the shared keychain.
//!
//! Rust replacement for `LicenseKeychain.swift`. The Swift original stored
//! the license key in the macOS keychain via the Security framework.
//!
//! **This module does not duplicate keychain bindings.** It reuses the
//! `CredentialStore` abstraction from `supercli-client`
//! (via `supercli-connector`'s keychain module), which already owns the
//! Security-framework integration, the in-memory fallback, and the
//! `SUPERCLI_CONNECTORS_KEYCHAIN` override. The license just uses its own
//! service/account namespace within that store.

use super::PlatformError;
use supercli_client::{CredentialStore, KeyringStore};

/// Keychain service for the app license (mirrors `LicenseKeychain.swift`'s
/// service identifier).
pub const LICENSE_KEYCHAIN_SERVICE: &str = "com.supercli.app.license";

/// Account name under which the license key is stored.
pub const LICENSE_KEYCHAIN_ACCOUNT: &str = "license-key";

fn map_err(context: &str, e: supercli_client::CredentialError) -> PlatformError {
    PlatformError::Platform(format!("{context}: {e}"))
}

/// Open the license store: the OS keychain namespaced to the license
/// service. Reuses the same `KeyringStore` type the connector uses.
pub fn license_store() -> KeyringStore {
    KeyringStore::with_service(LICENSE_KEYCHAIN_SERVICE)
}

/// Store the license key in the OS keychain.
pub fn store_license_key(license_key: &str) -> Result<(), PlatformError> {
    license_store()
        .set_secret(LICENSE_KEYCHAIN_ACCOUNT, license_key.as_bytes())
        .map_err(|e| map_err("store license key", e))
}

/// Load the license key from the OS keychain. Returns `Ok(None)` when no
/// license is stored.
pub fn load_license_key() -> Result<Option<String>, PlatformError> {
    match license_store()
        .get_secret(LICENSE_KEYCHAIN_ACCOUNT)
        .map_err(|e| map_err("load license key", e))?
    {
        None => Ok(None),
        Some(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|e| PlatformError::Platform(format!("license key is not UTF-8: {e}"))),
    }
}

/// Delete the stored license key. No-op when none is stored.
pub fn delete_license_key() -> Result<(), PlatformError> {
    license_store()
        .delete_secret(LICENSE_KEYCHAIN_ACCOUNT)
        .map_err(|e| map_err("delete license key", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use supercli_client::MemoryStore;

    #[test]
    fn service_constants() {
        assert_eq!(LICENSE_KEYCHAIN_SERVICE, "com.supercli.app.license");
        assert_eq!(LICENSE_KEYCHAIN_ACCOUNT, "license-key");
    }

    #[test]
    fn license_round_trip_memory_store() {
        // Exercises the shared CredentialStore path (the same trait the
        // OS keychain implements) without touching the real keychain.
        let store = MemoryStore::default();
        let key = "test-license-key-123";
        store
            .set_secret(LICENSE_KEYCHAIN_ACCOUNT, key.as_bytes())
            .unwrap();
        let loaded = store.get_secret(LICENSE_KEYCHAIN_ACCOUNT).unwrap();
        assert_eq!(loaded, Some(key.as_bytes().to_vec()));
        store.delete_secret(LICENSE_KEYCHAIN_ACCOUNT).unwrap();
        assert_eq!(store.get_secret(LICENSE_KEYCHAIN_ACCOUNT).unwrap(), None);
    }
}
