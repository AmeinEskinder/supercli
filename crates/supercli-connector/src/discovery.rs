//! Connector discovery: scan registrar sources for `connector.toml`
//! manifests. One bad manifest never fails the whole scan — it is
//! reported per-connector so `doctor` can surface it.

use std::path::{Path, PathBuf};

use crate::manifest::{parse_manifest, ConnectorManifest, ManifestError};

#[derive(Debug)]
pub struct DiscoveredConnector {
    /// Directory containing `connector.toml` (and the executable).
    pub dir: PathBuf,
    pub manifest: ConnectorManifest,
}

#[derive(Debug)]
pub struct DiscoveryError {
    pub dir: PathBuf,
    pub error: ManifestError,
}

/// Scan `roots` for installed connectors: each immediate subdirectory
/// containing a `connector.toml` is parsed. Returns the valid connectors
/// plus per-directory errors.
pub fn discover(roots: &[&Path]) -> (Vec<DiscoveredConnector>, Vec<DiscoveryError>) {
    let mut found = Vec::new();
    let mut errors = Vec::new();
    for root in roots {
        let entries = match std::fs::read_dir(root) {
            Ok(entries) => entries,
            Err(_) => continue, // A missing source is not an error.
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let manifest_path = dir.join("connector.toml");
            if !manifest_path.is_file() {
                continue;
            }
            match std::fs::read_to_string(&manifest_path) {
                Ok(text) => match parse_manifest(&text) {
                    Ok(manifest) => found.push(DiscoveredConnector { dir, manifest }),
                    Err(error) => errors.push(DiscoveryError { dir, error }),
                },
                Err(e) => errors.push(DiscoveryError {
                    dir,
                    error: ManifestError::InvalidToml(e.to_string()),
                }),
            }
        }
    }
    // Deterministic order for tests and `doctor` output.
    found.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    (found, errors)
}

/// Default install roots: the bundled connectors shipped with the Host,
/// then the user's `~/.supercli/connectors`. User installs shadow bundled
/// ones by name (last write wins at enable time).
pub fn default_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.join("connectors"));
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".supercli").join("connectors"));
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_manifest(dir: &Path, name: &str, toml: &str) {
        let dir = dir.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("connector.toml"), toml).unwrap();
    }

    const GOOD: &str = r#"
[connector]
name = "gmail"
version = "1.2.0"
display_name = "Gmail"
description = "Mail."
kind = "mcp-stdio"

[tools]
provides = ["mail.search"]
"#;

    #[test]
    fn discovers_valid_and_reports_invalid() {
        let root = std::env::temp_dir().join(format!("unpeel-conn-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        write_manifest(&root, "gmail", GOOD);
        write_manifest(&root, "broken", "[[[ not toml");
        write_manifest(&root, "empty", "[connector]\nname=\"x\"\n"); // missing fields
        std::fs::create_dir_all(root.join("not-a-connector")).unwrap(); // no manifest

        let (found, errors) = discover(&[root.as_path()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].manifest.name, "gmail");
        assert_eq!(errors.len(), 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_root_is_not_an_error() {
        let (found, errors) = discover(&[Path::new("/nonexistent-unpeel-root-xyz")]);
        assert!(found.is_empty());
        assert!(errors.is_empty());
    }
}
