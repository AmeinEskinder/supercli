//! Connector bundles: `pack` builds a distributable signed
//! `.unpeel-connector` archive; `install`/`publish` verify it.
//!
//! Bundle layout (gzip'd tar):
//! - `connector.toml` (required — the manifest is the trust boundary)
//! - `connector` (the MCP executable; required for `mcp-stdio`)
//! - `config.json` (optional non-secret configuration)
//!
//! The Ed25519 signature ships as a sidecar `<bundle>.sig` (see
//! [`crate::signing`]) signing the raw bundle bytes.

use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::manifest::{parse_manifest, ConnectorKind};
use crate::signing::{sign_bundle, verify_bundle, BundleSignature, SigningError};

#[derive(Debug, Error)]
pub enum BundleError {
    #[error("I/O: {0}")]
    Io(String),
    #[error("signing: {0}")]
    Signing(#[from] SigningError),
    #[error("manifest: {0}")]
    Manifest(String),
    #[error("refusing to pack {0:?}: not a connector dir (no connector.toml)")]
    NotAConnector(String),
    #[error("archive: {0}")]
    Archive(String),
}

/// Files packed into every bundle, in fixed order.
fn bundle_files(
    dir: &Path,
    kind: ConnectorKind,
) -> Result<Vec<(&'static str, PathBuf)>, BundleError> {
    let manifest_path = dir.join("connector.toml");
    if !manifest_path.is_file() {
        return Err(BundleError::NotAConnector(dir.display().to_string()));
    }
    let mut files = vec![("connector.toml", manifest_path)];
    let exe = dir.join("connector");
    if kind == ConnectorKind::McpStdio {
        if !exe.is_file() {
            return Err(BundleError::Io(format!(
                "mcp-stdio connector is missing its `connector` executable in {}",
                dir.display()
            )));
        }
        files.push(("connector", exe));
    } else if exe.is_file() {
        files.push(("connector", exe));
    }
    let config = dir.join("config.json");
    if config.is_file() {
        files.push(("config.json", config));
    }
    Ok(files)
}

pub fn bundle_file_name(name: &str, version: &str) -> String {
    format!("{name}-{version}.unpeel-connector")
}

/// Pack a connector dir into a signed bundle. Returns
/// `(bundle_path, sig_path)`. The bundle is written to `out_dir`
/// (created if missing).
pub fn pack(dir: &Path, out_dir: &Path, key_id: &str) -> Result<(PathBuf, PathBuf), BundleError> {
    let manifest_text = std::fs::read_to_string(dir.join("connector.toml"))
        .map_err(|e| BundleError::Io(e.to_string()))?;
    let manifest =
        parse_manifest(&manifest_text).map_err(|e| BundleError::Manifest(e.to_string()))?;

    let files = bundle_files(dir, manifest.kind)?;
    std::fs::create_dir_all(out_dir).map_err(|e| BundleError::Io(e.to_string()))?;

    let bundle_name = bundle_file_name(&manifest.name, &manifest.version.to_string());
    let bundle_path = out_dir.join(&bundle_name);

    let tar_gz = {
        let enc = GzEncoder::new(Vec::new(), Compression::default());
        let mut tar = tar::Builder::new(enc);
        for (arc_name, path) in &files {
            let mut file = std::fs::File::open(path).map_err(|e| BundleError::Io(e.to_string()))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|e| BundleError::Io(e.to_string()))?;
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(if *arc_name == "connector" {
                0o755
            } else {
                0o644
            });
            header.set_cksum();
            tar.append_data(&mut header, arc_name, bytes.as_slice())
                .map_err(|e| BundleError::Archive(e.to_string()))?;
        }
        let enc = tar
            .into_inner()
            .map_err(|e| BundleError::Archive(e.to_string()))?;
        enc.finish()
            .map_err(|e| BundleError::Archive(e.to_string()))?
    };
    std::fs::write(&bundle_path, &tar_gz).map_err(|e| BundleError::Io(e.to_string()))?;

    let sig = sign_bundle(key_id, &tar_gz)?;
    let sig_path = out_dir.join(format!("{bundle_name}.sig"));
    std::fs::write(
        &sig_path,
        serde_json::to_string_pretty(&sig).map_err(|e| BundleError::Io(e.to_string()))?,
    )
    .map_err(|e| BundleError::Io(e.to_string()))?;
    Ok((bundle_path, sig_path))
}

/// SHA-256 hex digest of a bundle, recorded in the registry index.
pub fn bundle_sha256(bundle: &[u8]) -> String {
    hex_of(Sha256::digest(bundle))
}

/// Read the manifest out of a packed bundle without unpacking it.
/// Used by the registry so the connector name/version come from the
/// signed manifest, never from the bundle file name (which misparses
/// names containing dashes).
pub fn bundle_manifest(bundle: &[u8]) -> Result<crate::manifest::ConnectorManifest, BundleError> {
    let gz = flate2::read::GzDecoder::new(bundle);
    let mut tar = tar::Archive::new(gz);
    for entry in tar
        .entries()
        .map_err(|e| BundleError::Archive(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| BundleError::Archive(e.to_string()))?;
        let name = entry
            .path()
            .map_err(|e| BundleError::Archive(e.to_string()))?
            .to_string_lossy()
            .into_owned();
        if name != "connector.toml" {
            continue;
        }
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .map_err(|e| BundleError::Archive(e.to_string()))?;
        return parse_manifest(&text).map_err(|e| BundleError::Manifest(e.to_string()));
    }
    Err(BundleError::Archive(
        "bundle contains no connector.toml".to_string(),
    ))
}

fn hex_of(digest: impl AsRef<[u8]>) -> String {
    digest.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// Unpack a bundle into `dest` (created if missing) after verifying its
/// signature against `expected_pubkey_b64`. The manifest inside is
/// re-parsed; the caller validates it further.
pub fn unpack_verified(
    bundle: &[u8],
    sig: &BundleSignature,
    expected_pubkey_b64: &str,
    dest: &Path,
) -> Result<(), BundleError> {
    verify_bundle(bundle, sig, expected_pubkey_b64)?;
    let gz = flate2::read::GzDecoder::new(bundle);
    let mut tar = tar::Archive::new(gz);
    std::fs::create_dir_all(dest).map_err(|e| BundleError::Io(e.to_string()))?;
    // Only the three known member names may land in the destination; the
    // archive is unpacked entry-by-entry, never with `unpack()` (which
    // would honor hostile paths).
    for entry in tar
        .entries()
        .map_err(|e| BundleError::Archive(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| BundleError::Archive(e.to_string()))?;
        let name = entry
            .path()
            .map_err(|e| BundleError::Archive(e.to_string()))?
            .to_string_lossy()
            .into_owned();
        if !matches!(
            name.as_str(),
            "connector.toml" | "connector" | "config.json"
        ) {
            return Err(BundleError::Archive(format!(
                "bundle contains unexpected file {name:?}"
            )));
        }
        let dest_path = dest.join(&name);
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| BundleError::Archive(e.to_string()))?;
        std::fs::write(&dest_path, &bytes).map_err(|e| BundleError::Io(e.to_string()))?;
        if name == "connector" {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    std::fs::set_permissions(&dest_path, std::fs::Permissions::from_mode(0o755));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::keygen;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const MANIFEST: &str = r#"
[connector]
name = "packme"
version = "1.0.0"
display_name = "Pack Me"
description = "Pack test."
kind = "mcp-stdio"

[tools]
provides = ["packme.echo"]
"#;

    fn fixture() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("unpeel-pack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("connector.toml"), MANIFEST).unwrap();
        let exe = dir.join("connector");
        std::fs::write(&exe, "#!/bin/sh\necho hi\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    }

    #[test]
    fn pack_sign_verify_unpack_roundtrip() {
        let _guard = ENV_LOCK.lock().unwrap();
        let keys = std::env::temp_dir().join(format!("unpeel-pack-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&keys);
        std::env::set_var("UNPEEL_CONNECTOR_KEYS_DIR", &keys);
        keygen("packkey").expect("keygen");

        let dir = fixture();
        let out = dir.join("out");
        let (bundle_path, sig_path) = pack(&dir, &out, "packkey").expect("pack");
        assert!(bundle_path.is_file());
        assert!(sig_path.is_file());

        let bundle = std::fs::read(&bundle_path).unwrap();
        let sig_text = std::fs::read_to_string(&sig_path).unwrap();
        let sig: BundleSignature = serde_json::from_str(&sig_text).unwrap();
        let dest = dir.join("unpacked");
        unpack_verified(&bundle, &sig, &sig.pubkey, &dest).expect("unpack");
        assert!(dest.join("connector.toml").is_file());
        assert!(dest.join("connector").is_file());

        // Tampered bundle fails verification.
        let mut tampered = bundle.clone();
        tampered[20] ^= 0xff;
        assert!(unpack_verified(&tampered, &sig, &sig.pubkey, &dir.join("bad")).is_err());

        std::env::remove_var("UNPEEL_CONNECTOR_KEYS_DIR");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&keys);
    }
}
