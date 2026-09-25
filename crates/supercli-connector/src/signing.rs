//! Ed25519 signing for connector bundles.
//!
//! A publisher generates a keypair once (`unpeel connector keygen`); `pack`
//! signs the bundle bytes with the secret key and ships the signature in a
//! `.sig` sidecar; `install` verifies the bundle against a trusted public
//! key (from the registry index, `--pubkey`, or a pinned key) before it
//! copies anything. Unsigned installs are refused only with
//! `--require-signature`; otherwise they warn, exactly like the pre-signing
//! behavior documented in `docs/connectors.md`.
//!
//! Key file formats (deliberately simple, documented here):
//! - `<id>.key`: `unpeel-ed25519-secret-v1:<base64 32-byte seed>`, mode 0600.
//! - `<id>.pub`: `unpeel-ed25519-pub-v1:<base64 32-byte public key>`.
//! - `<bundle>.sig`: JSON `{"key_id": "...", "pubkey": "<base64 32 bytes>",
//!   "signature": "<base64 64 bytes>"}` signing the raw bundle bytes.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SigningError {
    #[error("I/O: {0}")]
    Io(String),
    #[error("bad key file: {0}")]
    BadKey(String),
    #[error("signature verification failed")]
    BadSignature,
    #[error("refusing to overwrite existing key {0:?} (delete it first if you mean to rotate)")]
    KeyExists(String),
}

const SECRET_PREFIX: &str = "unpeel-ed25519-secret-v1:";
const PUB_PREFIX: &str = "unpeel-ed25519-pub-v1:";

/// Directory holding publisher keypairs: `~/.supercli/connector-keys/`
/// (`SUPERCLI_CONNECTOR_KEYS_DIR` overrides for tests/dev).
pub fn keys_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SUPERCLI_CONNECTOR_KEYS_DIR") {
        let path = PathBuf::from(dir);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".supercli").join("connector-keys"),
        None => PathBuf::from(".supercli-connector-keys"),
    }
}

fn is_valid_key_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
        && !id.ends_with('-')
}

/// Generate a fresh Ed25519 keypair under `keys_dir()`. Refuses to
/// overwrite an existing key id.
pub fn keygen(key_id: &str) -> Result<(PathBuf, PathBuf), SigningError> {
    if !is_valid_key_id(key_id) {
        return Err(SigningError::BadKey(format!(
            "bad key id {key_id:?}: lowercase letters, digits, dashes"
        )));
    }
    let dir = keys_dir();
    std::fs::create_dir_all(&dir).map_err(|e| SigningError::Io(e.to_string()))?;
    let secret_path = dir.join(format!("{key_id}.key"));
    let pub_path = dir.join(format!("{key_id}.pub"));
    if secret_path.exists() || pub_path.exists() {
        return Err(SigningError::KeyExists(key_id.to_string()));
    }
    let signing = {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).map_err(|e| SigningError::Io(e.to_string()))?;
        SigningKey::from_bytes(&seed)
    };
    let secret_line = format!("{SECRET_PREFIX}{}", B64.encode(signing.to_bytes()));
    std::fs::write(&secret_path, secret_line).map_err(|e| SigningError::Io(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&secret_path, std::fs::Permissions::from_mode(0o600));
    }
    let pub_line = format!(
        "{PUB_PREFIX}{}",
        B64.encode(signing.verifying_key().to_bytes())
    );
    std::fs::write(&pub_path, pub_line).map_err(|e| SigningError::Io(e.to_string()))?;
    Ok((secret_path, pub_path))
}

/// Load a secret key by id.
pub fn load_secret_key(key_id: &str) -> Result<SigningKey, SigningError> {
    let path = keys_dir().join(format!("{key_id}.key"));
    let text = std::fs::read_to_string(&path).map_err(|_| {
        SigningError::BadKey(format!("no key {key_id:?} (run `unpeel connector keygen`)"))
    })?;
    let b64 = text
        .trim()
        .strip_prefix(SECRET_PREFIX)
        .ok_or_else(|| SigningError::BadKey(format!("{}: unknown format", path.display())))?;
    let bytes = B64
        .decode(b64.trim())
        .map_err(|e| SigningError::BadKey(e.to_string()))?;
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| SigningError::BadKey("secret key is not 32 bytes".to_string()))?;
    Ok(SigningKey::from_bytes(&seed))
}

/// Parse a `.pub` file (or the bare `unpeel-ed25519-pub-v1:` line) into a
/// verifying key.
pub fn parse_public_key(text: &str) -> Result<VerifyingKey, SigningError> {
    let b64 = text
        .trim()
        .strip_prefix(PUB_PREFIX)
        .ok_or_else(|| SigningError::BadKey("unknown public key format".to_string()))?;
    let bytes = B64
        .decode(b64.trim())
        .map_err(|e| SigningError::BadKey(e.to_string()))?;
    let raw: [u8; 32] = bytes
        .try_into()
        .map_err(|_| SigningError::BadKey("public key is not 32 bytes".to_string()))?;
    VerifyingKey::from_bytes(&raw).map_err(|e| SigningError::BadKey(e.to_string()))
}

/// Load a public key by key id from `keys_dir()`.
pub fn load_public_key(key_id: &str) -> Result<VerifyingKey, SigningError> {
    let path = keys_dir().join(format!("{key_id}.pub"));
    let text = std::fs::read_to_string(&path)
        .map_err(|_| SigningError::BadKey(format!("no public key {key_id:?}")))?;
    parse_public_key(&text)
}

pub fn public_key_base64(key: &VerifyingKey) -> String {
    B64.encode(key.to_bytes())
}

/// The `.sig` sidecar shipped next to a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleSignature {
    pub key_id: String,
    pub pubkey: String,
    pub signature: String,
}

/// Sign raw bundle bytes with the named key.
pub fn sign_bundle(key_id: &str, bundle: &[u8]) -> Result<BundleSignature, SigningError> {
    let secret = load_secret_key(key_id)?;
    let signature = secret.sign(bundle);
    Ok(BundleSignature {
        key_id: key_id.to_string(),
        pubkey: public_key_base64(&secret.verifying_key()),
        signature: B64.encode(signature.to_bytes()),
    })
}

/// Verify bundle bytes against a sidecar signature and an expected public
/// key. The sidecar's embedded pubkey must match the expected key — a
/// signature that verifies under an attacker's key is worthless.
pub fn verify_bundle(
    bundle: &[u8],
    sig: &BundleSignature,
    expected_pubkey_b64: &str,
) -> Result<(), SigningError> {
    if sig.pubkey != expected_pubkey_b64 {
        return Err(SigningError::BadSignature);
    }
    let key = parse_public_key(&format!("{PUB_PREFIX}{}", sig.pubkey))?;
    let raw = B64
        .decode(sig.signature.trim())
        .map_err(|_| SigningError::BadSignature)?;
    let bytes: [u8; 64] = raw.try_into().map_err(|_| SigningError::BadSignature)?;
    let signature = ed25519_dalek::Signature::from_bytes(&bytes);
    key.verify(bundle, &signature)
        .map_err(|_| SigningError::BadSignature)
}

/// Read a `.sig` sidecar file.
pub fn read_signature(path: &Path) -> Result<BundleSignature, SigningError> {
    let text = std::fs::read_to_string(path).map_err(|e| SigningError::Io(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| SigningError::BadKey(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn test_keys_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("unpeel-sign-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("SUPERCLI_CONNECTOR_KEYS_DIR", &dir);
        dir
    }

    #[test]
    fn keygen_sign_verify_roundtrip() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = test_keys_dir("roundtrip");
        keygen("testkey").expect("keygen");
        let bundle = b"fake bundle bytes";
        let sig = sign_bundle("testkey", bundle).expect("sign");
        let pubkey = load_public_key("testkey").expect("load pub");
        verify_bundle(bundle, &sig, &public_key_base64(&pubkey)).expect("verify");
        // Tampered bytes fail.
        assert!(verify_bundle(b"tampered", &sig, &public_key_base64(&pubkey)).is_err());
        // Wrong key fails.
        keygen("other").expect("keygen other");
        let other_pub = load_public_key("other").expect("load other");
        assert!(verify_bundle(bundle, &sig, &public_key_base64(&other_pub)).is_err());
        std::env::remove_var("SUPERCLI_CONNECTOR_KEYS_DIR");
    }

    #[test]
    fn keygen_refuses_overwrite() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = test_keys_dir("overwrite");
        keygen("dup").expect("first");
        assert!(matches!(keygen("dup"), Err(SigningError::KeyExists(_))));
        std::env::remove_var("SUPERCLI_CONNECTOR_KEYS_DIR");
    }
}
