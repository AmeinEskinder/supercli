//! Connector registry: publish signed bundles, resolve and fetch them.
//!
//! A registry is a directory (local path; `SUPERCLI_CONNECTOR_REGISTRY`
//! overrides the default `~/.supercli/registry/connectors`):
//! ```text
//! <registry>/
//!   index.json            # name -> versions -> {bundle, sha256, pubkey, key_id}
//!   bundles/<name>-<version>.supercli-connector
//!   bundles/<name>-<version>.supercli-connector.sig
//! ```
//! The first publish of a name pins its publisher public key; later
//! publishes for the same name must carry a signature from the same key
//! (key rotation is deleting the name's entry and re-publishing, a
//! deliberate manual act). `install --registry` verifies the bundle
//! against the pinned key before unpacking — this is the signature
//! verification `docs/connectors.md` requires at install time.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::bundle::{bundle_sha256, BundleError};
use crate::signing::{read_signature, SigningError};

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("I/O: {0}")]
    Io(String),
    #[error("bundle: {0}")]
    Bundle(#[from] BundleError),
    #[error("signing: {0}")]
    Signing(#[from] SigningError),
    #[error("no such connector {0:?} in the registry")]
    UnknownConnector(String),
    #[error("no version {1:?} of {0:?} in the registry (have: {2})")]
    UnknownVersion(String, String, String),
    #[error(
        "publisher key mismatch for {0:?}: the registry pins a different \
         public key for this name (re-publish only after deliberately \
         clearing the name's entry)"
    )]
    KeyMismatch(String),
    #[error("bundle {0} fails sha256 (have {1}, want {2})")]
    ShaMismatch(String, String, String),
}

/// Default registry location.
pub fn default_registry_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SUPERCLI_CONNECTOR_REGISTRY") {
        let path = PathBuf::from(dir);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home)
            .join(".supercli")
            .join("registry")
            .join("connectors"),
        None => PathBuf::from(".supercli-connector-registry"),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegistryIndex {
    #[serde(default)]
    pub connectors: BTreeMap<String, RegistryConnector>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegistryConnector {
    #[serde(default)]
    pub versions: BTreeMap<String, RegistryVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryVersion {
    pub bundle: String,
    pub sha256: String,
    pub pubkey: String,
    pub key_id: String,
}

fn index_path(registry: &Path) -> PathBuf {
    registry.join("index.json")
}

/// Read the registry index (empty when the registry does not exist yet).
pub fn read_index(registry: &Path) -> Result<RegistryIndex, RegistryError> {
    let path = index_path(registry);
    if !path.is_file() {
        return Ok(RegistryIndex::default());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| RegistryError::Io(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| RegistryError::Io(format!("bad index.json: {e}")))
}

/// Publish a packed bundle (+ its `.sig` sidecar) into the registry.
/// Verifies the signature is self-consistent, pins the publisher key on
/// first publish, and refuses a key change afterwards.
pub fn publish(
    registry: &Path,
    bundle_path: &Path,
    sig_path: &Path,
) -> Result<(String, String), RegistryError> {
    let bundle = std::fs::read(bundle_path).map_err(|e| RegistryError::Io(e.to_string()))?;
    let sig = read_signature(sig_path)?;
    // Self-consistency: the signature must verify under the pubkey it
    // claims. The registry then pins that pubkey for the connector name.
    crate::signing::verify_bundle(&bundle, &sig, &sig.pubkey)?;

    // The name and version come from the signed manifest inside the
    // bundle, never from the file name (which misparses names containing
    // dashes, and could claim anything).
    let manifest = crate::bundle::bundle_manifest(&bundle)
        .map_err(|e| RegistryError::Io(format!("bad bundle manifest: {e}")))?;
    let name = manifest.name.clone();
    let version = manifest.version.to_string();

    let file_name = crate::bundle::bundle_file_name(&name, &version);

    // Key pinning is checked BEFORE anything is written: a mismatched
    // key must be refused without touching the registry's existing
    // bundles (the dest file names are deterministic, so writing first
    // and cleaning up on mismatch would delete the good files).
    let mut index = read_index(registry)?;
    let entry = index.connectors.entry(name.to_string()).or_default();
    if let Some(first) = entry.versions.values().next() {
        if first.pubkey != sig.pubkey {
            return Err(RegistryError::KeyMismatch(name.to_string()));
        }
    }

    let bundles_dir = registry.join("bundles");
    std::fs::create_dir_all(&bundles_dir).map_err(|e| RegistryError::Io(e.to_string()))?;
    let dest_bundle = bundles_dir.join(&file_name);
    let dest_sig = bundles_dir.join(format!("{file_name}.sig"));
    std::fs::write(&dest_bundle, &bundle).map_err(|e| RegistryError::Io(e.to_string()))?;
    std::fs::write(
        &dest_sig,
        serde_json::to_string_pretty(&sig).map_err(|e| RegistryError::Io(e.to_string()))?,
    )
    .map_err(|e| RegistryError::Io(e.to_string()))?;

    let sha256 = bundle_sha256(&bundle);
    entry.versions.insert(
        version.to_string(),
        RegistryVersion {
            bundle: format!("bundles/{file_name}"),
            sha256,
            pubkey: sig.pubkey.clone(),
            key_id: sig.key_id.clone(),
        },
    );
    let text =
        serde_json::to_string_pretty(&index).map_err(|e| RegistryError::Io(e.to_string()))?;
    // Atomic write: the index is the registry's trust root.
    let tmp = index_path(registry).with_extension("tmp");
    std::fs::write(&tmp, text).map_err(|e| RegistryError::Io(e.to_string()))?;
    #[cfg(unix)]
    {
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644));
    }
    std::fs::rename(&tmp, index_path(registry)).map_err(|e| RegistryError::Io(e.to_string()))?;
    Ok((name.to_string(), version.to_string()))
}

/// Resolve a connector name (optionally `name@version`; version defaults
/// to the highest semver in the index) to its registry entry.
pub fn resolve(
    registry: &Path,
    name: &str,
    version: Option<&str>,
) -> Result<(String, RegistryVersion), RegistryError> {
    let index = read_index(registry)?;
    let connector = index
        .connectors
        .get(name)
        .ok_or_else(|| RegistryError::UnknownConnector(name.to_string()))?;
    if let Some(v) = version {
        let entry = connector.versions.get(v).ok_or_else(|| {
            RegistryError::UnknownVersion(
                name.to_string(),
                v.to_string(),
                connector
                    .versions
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        })?;
        return Ok((v.to_string(), entry.clone()));
    }
    // Highest semver wins; unparseable versions sort below parseable ones.
    let mut versions: Vec<(&String, &RegistryVersion)> = connector.versions.iter().collect();
    versions.sort_by(
        |a, b| match (semver::Version::parse(a.0), semver::Version::parse(b.0)) {
            (Ok(pa), Ok(pb)) => pa.cmp(&pb),
            (Ok(_), Err(_)) => std::cmp::Ordering::Greater,
            (Err(_), Ok(_)) => std::cmp::Ordering::Less,
            (Err(_), Err(_)) => a.0.cmp(b.0),
        },
    );
    versions
        .last()
        .map(|(v, e)| ((*v).clone(), (*e).clone()))
        .ok_or_else(|| RegistryError::UnknownConnector(name.to_string()))
}

/// Fetch a resolved entry's bundle bytes, verifying the sha256 recorded
/// in the index. Signature verification against the pinned pubkey happens
/// at unpack time (`unpack_verified`).
pub fn fetch(
    registry: &Path,
    entry: &RegistryVersion,
) -> Result<(Vec<u8>, PathBuf), RegistryError> {
    let bundle_path = registry.join(&entry.bundle);
    let bundle = std::fs::read(&bundle_path).map_err(|e| RegistryError::Io(e.to_string()))?;
    let sha = bundle_sha256(&bundle);
    if sha != entry.sha256 {
        return Err(RegistryError::ShaMismatch(
            entry.bundle.clone(),
            sha,
            entry.sha256.clone(),
        ));
    }
    let sig_path = bundle_path.with_extension("unpeel-connector.sig");
    Ok((bundle, sig_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::pack;
    use crate::signing::keygen;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const MANIFEST: &str = r#"
[connector]
name = "regme"
version = "2.1.0"
display_name = "Reg Me"
description = "Registry test."
kind = "mcp-stdio"

[tools]
provides = ["regme.echo"]
"#;

    #[test]
    fn publish_resolve_fetch_roundtrip() {
        let _guard = ENV_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join(format!("unpeel-reg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let keys = base.join("keys");
        std::env::set_var("SUPERCLI_CONNECTOR_KEYS_DIR", &keys);
        keygen("regkey").expect("keygen");

        let src = base.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("connector.toml"), MANIFEST).unwrap();
        std::fs::write(src.join("connector"), "#!/bin/sh\n").unwrap();

        let out = base.join("out");
        let (bundle_path, sig_path) = pack(&src, &out, "regkey").expect("pack");
        let registry = base.join("registry");
        let (name, version) = publish(&registry, &bundle_path, &sig_path).expect("publish");
        assert_eq!((name.as_str(), version.as_str()), ("regme", "2.1.0"));

        let (v, entry) = resolve(&registry, "regme", None).expect("resolve latest");
        assert_eq!(v, "2.1.0");
        let (bundle, sig_path) = fetch(&registry, &entry).expect("fetch");
        assert!(sig_path.is_file());
        assert_eq!(crate::bundle::bundle_sha256(&bundle), entry.sha256);

        // A second publish under a different key is refused.
        keygen("evil").expect("keygen evil");
        let (bundle2, sig2) = pack(&src, &out, "evil").expect("pack evil");
        assert!(matches!(
            publish(&registry, &bundle2, &sig2),
            Err(RegistryError::KeyMismatch(_))
        ));

        std::env::remove_var("SUPERCLI_CONNECTOR_KEYS_DIR");
        let _ = std::fs::remove_dir_all(&base);
    }
}
