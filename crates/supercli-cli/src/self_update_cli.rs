//! `supercli self-update` — check for and apply updates.
//!
//! Phase 14 (4): `--check` only. Reads a local manifest file (no network),
//! compares the installed version against the manifest version, and
//! reports whether an update is available. `--apply` and `--rollback`
//! are not implemented in this phase (see docs/self-update-design.md).
//!
//! Exit codes: 0 = up-to-date (or check completed, no update needed),
//! 2 = usage/config error, 3 = update available.

pub const HELP: &str = "\
supercli self-update — check for updates (Phase 14: --check only)

  supercli self-update --check [--manifest PATH] [--json]

Reads a local update manifest (JSON) and compares its version against
the installed version. No network calls are made in this phase: the
manifest must be a local file path (a `file://` URL is also accepted);
any `https://` manifest is rejected.

Exit codes: 0 = up-to-date, 2 = usage or manifest error,
3 = update available.

Manifest format:
  { \"version\": \"0.9.0\", \"artifacts\": { ... } }
";

/// Installed version of this binary.
const INSTALLED_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Exit code when an update is available.
pub const EXIT_UPDATE_AVAILABLE: i32 = 3;

#[derive(Debug)]
struct Manifest {
    version: String,
}

/// Read and parse the manifest. Rejects non-local manifests.
fn read_manifest(path: &str) -> Result<Manifest, String> {
    // Reject network manifests explicitly — no network in this phase.
    if path.starts_with("https://") || path.starts_with("http://") {
        return Err(format!(
            "network updates not enabled in this build: manifest must be a local file path, got {path:?}"
        ));
    }
    // Accept file:// URLs by stripping the scheme.
    let fs_path = path.strip_prefix("file://").unwrap_or(path);
    let text = std::fs::read_to_string(fs_path)
        .map_err(|e| format!("cannot read manifest {fs_path:?}: {e}"))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("manifest {fs_path:?} is not valid JSON: {e}"))?;
    let version = value
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("manifest {fs_path:?} has no string \"version\" field"))?;
    Ok(Manifest {
        version: version.to_string(),
    })
}

/// Compare installed vs manifest version.
/// Returns `Ok(true)` if an update is available.
fn update_available(installed: &str, manifest: &str) -> Result<bool, String> {
    let installed_v = semver::Version::parse(installed)
        .map_err(|e| format!("installed version {installed:?} is not valid semver: {e}"))?;
    let manifest_v = semver::Version::parse(manifest)
        .map_err(|e| format!("manifest version {manifest:?} is not valid semver: {e}"))?;
    Ok(manifest_v > installed_v)
}

/// Run `supercli self-update`. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let mut check = false;
    let mut manifest_path: Option<String> = None;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--check" => check = true,
            "--manifest" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("supercli self-update: --manifest requires a PATH");
                    return 2;
                }
                manifest_path = Some(args[i].clone());
            }
            "--json" => json = true,
            "--help" | "-h" | "help" => {
                println!("{HELP}");
                return 0;
            }
            "--apply" => {
                eprintln!("supercli self-update: --apply is not implemented in this phase (see docs/self-update-design.md)");
                return 2;
            }
            "--rollback" => {
                eprintln!("supercli self-update: --rollback is not implemented in this phase (see docs/self-update-design.md)");
                return 2;
            }
            other => {
                eprintln!("supercli self-update: unknown argument {other:?}\n{HELP}");
                return 2;
            }
        }
        i += 1;
    }
    if !check {
        eprintln!("supercli self-update: --check is required in this phase\n{HELP}");
        return 2;
    }
    let manifest_path = match manifest_path {
        Some(p) => p,
        None => {
            eprintln!("supercli self-update --check: --manifest PATH is required");
            return 2;
        }
    };

    let manifest = match read_manifest(&manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("supercli self-update: {e}");
            return 2;
        }
    };
    let available = match update_available(INSTALLED_VERSION, &manifest.version) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("supercli self-update: {e}");
            return 2;
        }
    };

    if json {
        let out = serde_json::json!({
            "installed": INSTALLED_VERSION,
            "manifest_version": manifest.version,
            "update_available": available,
        });
        println!("{}", serde_json::to_string(&out).unwrap());
    } else if available {
        println!(
            "update available: {} -> {} (manifest: {})",
            INSTALLED_VERSION, manifest.version, manifest_path
        );
    } else {
        println!(
            "up to date: installed {} matches manifest {}",
            INSTALLED_VERSION, manifest.version
        );
    }
    if available {
        EXIT_UPDATE_AVAILABLE
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write_manifest(dir: &Path, version: &str) -> String {
        let path = dir.join("manifest.json");
        std::fs::write(
            &path,
            serde_json::json!({ "version": version, "artifacts": {} }).to_string(),
        )
        .unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn check_reports_update_available_exit_3() {
        let dir = tempfile::tempdir().unwrap();
        // Manifest newer than any plausible installed version.
        let mp = write_manifest(dir.path(), "99.0.0");
        let code = run(&["--check".to_string(), "--manifest".to_string(), mp]);
        assert_eq!(code, EXIT_UPDATE_AVAILABLE);
    }

    #[test]
    fn check_reports_up_to_date_exit_0() {
        let dir = tempfile::tempdir().unwrap();
        let mp = write_manifest(dir.path(), INSTALLED_VERSION);
        let code = run(&["--check".to_string(), "--manifest".to_string(), mp]);
        assert_eq!(code, 0);
    }

    #[test]
    fn check_rejects_https_manifest() {
        let code = run(&[
            "--check".to_string(),
            "--manifest".to_string(),
            "https://example.com/manifest.json".to_string(),
        ]);
        assert_eq!(code, 2);
    }

    #[test]
    fn check_accepts_file_url_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let mp = write_manifest(dir.path(), INSTALLED_VERSION);
        let file_url = format!("file://{mp}");
        let code = run(&["--check".to_string(), "--manifest".to_string(), file_url]);
        assert_eq!(code, 0);
    }

    #[test]
    fn check_missing_manifest_is_usage_error() {
        let code = run(&["--check".to_string()]);
        assert_eq!(code, 2);
    }

    #[test]
    fn check_malformed_manifest_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, "not json").unwrap();
        let code = run(&[
            "--check".to_string(),
            "--manifest".to_string(),
            bad.to_string_lossy().to_string(),
        ]);
        assert_eq!(code, 2);
    }

    #[test]
    fn apply_and_rollback_are_rejected_this_phase() {
        assert_eq!(run(&["--apply".to_string()]), 2);
        assert_eq!(run(&["--rollback".to_string()]), 2);
    }
}
