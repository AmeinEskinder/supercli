//! Connector token storage. Tokens live in the OS keychain under the
//! Host's service, account `connector:{name}` — the connector process
//! itself only ever sees its token via the `UNPEEL_CONNECTOR_TOKEN` env
//! var. `disconnect` deletes the entry (revocation is one verb).

use std::sync::{Arc, OnceLock};
use unpeel_client::{CredentialError, CredentialStore, KeyringStore, MemoryStore};

/// Keychain service for Host-side connector tokens.
pub const CONNECTOR_KEYCHAIN_SERVICE: &str = "com.unpeel.host";

/// Env var forcing the in-memory connector token store. Used by tests and
/// headless setups that must not touch the OS keychain.
pub const CONNECTORS_KEYCHAIN_ENV: &str = "UNPEEL_CONNECTORS_KEYCHAIN";

fn account_for_connector(name: &str) -> String {
    format!("connector:{name}")
}

/// Production keychain store for connector tokens.
pub fn keychain_store() -> KeyringStore {
    KeyringStore::with_service(CONNECTOR_KEYCHAIN_SERVICE)
}

fn memory_fallback() -> Arc<MemoryStore> {
    static MEM: OnceLock<Arc<MemoryStore>> = OnceLock::new();
    MEM.get_or_init(|| Arc::new(MemoryStore::default())).clone()
}

/// Open the connector token store: the OS keychain when it answers,
/// otherwise a process-lifetime in-memory store. The second return value
/// is a user-facing notice set only when falling back — tokens will not
/// survive a restart in that mode, and the CLI says so.
///
/// `UNPEEL_CONNECTORS_KEYCHAIN=memory` forces the in-memory store; the
/// connector CLI tests use it so they never touch the real keychain.
pub fn open_connector_store() -> (Arc<dyn CredentialStore>, Option<String>) {
    if std::env::var(CONNECTORS_KEYCHAIN_ENV).as_deref() == Ok("memory") {
        return (memory_fallback(), None);
    }
    let keyring = keychain_store();
    match keyring.get_secret("__probe__") {
        Ok(_) => (Arc::new(keyring), None),
        Err(e) => (
            memory_fallback(),
            Some(format!(
                "System keychain unavailable ({e}); connector tokens will not be remembered after this process exits."
            )),
        ),
    }
}

/// Store (or rotate) a connector's token. Called by `connector connect`
/// after the auth flow completes.
pub fn store_connector_token(
    store: &dyn CredentialStore,
    name: &str,
    token: &str,
) -> Result<(), CredentialError> {
    store.set_secret(&account_for_connector(name), token.as_bytes())
}

/// Load a connector's token for injection into its process environment.
/// `Ok(None)` means the connector was never connected.
pub fn load_connector_token(
    store: &dyn CredentialStore,
    name: &str,
) -> Result<Option<String>, CredentialError> {
    match store.get_secret(&account_for_connector(name))? {
        None => Ok(None),
        Some(bytes) => String::from_utf8(bytes)
            .map_err(|e| CredentialError::Corrupt(e.to_string()))
            .map(Some),
    }
}

/// Revoke a connector: delete its token. Detaching its tools from sessions
/// is the session layer's job; without a token a spawned process gets an
/// empty env var and its own auth fails.
pub fn delete_connector_token(
    store: &dyn CredentialStore,
    name: &str,
) -> Result<(), CredentialError> {
    store.delete_secret(&account_for_connector(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use unpeel_client::MemoryStore;

    #[test]
    fn token_lifecycle() {
        let store = MemoryStore::default();
        assert_eq!(load_connector_token(&store, "gmail").unwrap(), None);
        store_connector_token(&store, "gmail", "oauth-token-1").unwrap();
        assert_eq!(
            load_connector_token(&store, "gmail").unwrap(),
            Some("oauth-token-1".to_string())
        );
        // Rotation overwrites.
        store_connector_token(&store, "gmail", "oauth-token-2").unwrap();
        assert_eq!(
            load_connector_token(&store, "gmail").unwrap(),
            Some("oauth-token-2".to_string())
        );
        delete_connector_token(&store, "gmail").unwrap();
        assert_eq!(load_connector_token(&store, "gmail").unwrap(), None);
    }
}
