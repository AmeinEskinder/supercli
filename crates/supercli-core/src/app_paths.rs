use std::path::{Path, PathBuf};

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// The real per-user Supercli root, deliberately ignoring `SUPERCLI_HOME`.
///
/// Machine-wide coordination (the workspace registry and Host-service
/// supervisor) lives here. A scoped workspace process must still be able to
/// find that coordination root without accidentally treating its own
/// isolated state directory as the machine root.
pub fn real_supercli_home() -> PathBuf {
    home_dir().join(".supercli")
}

/// The Supercli state dir: `~/.supercli`, or the directory named by `SUPERCLI_HOME`
/// when that env var is set and non-empty. The native app sets it for blank
/// dev instances and spawns hosts with the env inherited, so app + host agree
/// on one isolated state dir.
/// The machine-wide workspace registry: one file at the REAL `~/.supercli`,
/// legacy wire name `profiles.json` with a top-level `profiles` array whose
/// records carry `id`, `name`, and the workspace's absolute `home`. Written
/// by the app and `supercli workspaces`; read here only.
pub fn workspace_registry_path(real_home: &Path) -> PathBuf {
    real_home.join("profiles.json")
}

/// One registered workspace, as far as core needs to know it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRegistryRecord {
    pub id: String,
    pub name: String,
    pub home: PathBuf,
}

/// Every registered workspace, or an empty list when the registry is
/// absent or unreadable. Unknown keys are ignored; a record without a home
/// is skipped.
pub fn read_workspace_registry(real_home: &Path) -> Vec<WorkspaceRegistryRecord> {
    let Ok(raw) = std::fs::read(workspace_registry_path(real_home)) else {
        return Vec::new();
    };
    parse_workspace_registry(&raw)
}

fn parse_workspace_registry(raw: &[u8]) -> Vec<WorkspaceRegistryRecord> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    value
        .get("profiles")
        .and_then(|records| records.as_array())
        .map(|records| {
            records
                .iter()
                .filter_map(|record| {
                    let home = record.get("home")?.as_str()?.trim();
                    if home.is_empty() {
                        return None;
                    }
                    Some(WorkspaceRegistryRecord {
                        id: record
                            .get("id")
                            .and_then(|id| id.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        name: record
                            .get("name")
                            .and_then(|name| name.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        home: PathBuf::from(home),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `home` is a workspace registered in `real_home`'s registry
/// (compared after canonicalizing both sides when they exist).
pub fn is_registered_workspace_home(real_home: &Path, home: &Path) -> bool {
    let target = normalized_path(home);
    read_workspace_registry(real_home)
        .iter()
        .any(|record| normalized_path(&record.home) == target)
}

fn normalized_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The root that per-user integration state belongs to. Provider configs
/// (`~/.claude.json`, `~/.codex/config.toml`, …) are per user, not per
/// workspace, so the integration markers, hook scripts, and MCP shim they
/// point at live in one place for every local workspace of one account:
/// the real `~/.supercli` whenever this process's `SUPERCLI_HOME` is a
/// registered workspace of that machine, and the home itself otherwise. A
/// blank dev instance or a test home is never registered, so it stays
/// private with no flag; installing from any registered workspace installs
/// for all of them.
pub fn machine_home() -> PathBuf {
    let own = supercli_home();
    let real = real_supercli_home();
    let registered = own != real && is_registered_workspace_home(&real, &own);
    machine_home_from(own, real, registered)
}

pub fn machine_home_from(own_home: PathBuf, real_home: PathBuf, registered: bool) -> PathBuf {
    if registered {
        real_home
    } else {
        own_home
    }
}

pub fn supercli_home() -> PathBuf {
    if let Some(override_dir) = std::env::var_os("SUPERCLI_HOME") {
        let trimmed = override_dir.to_string_lossy().trim().to_string();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    real_supercli_home()
}

/// Test-only lock serializing mutation of the process-global `SUPERCLI_HOME`.
///
/// Rust runs tests in one process in parallel; `std::env::set_var` is
/// process-global, so two tests pointing `SUPERCLI_HOME` at different scratch
/// dirs race. Any test that sets `SUPERCLI_HOME` must hold this lock for the
/// whole time the override is in effect (acquire before set, release after
/// restore). Production code never touches this.
#[cfg(test)]
pub static TEST_SUPERCLI_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Ensure `~/.supercli` exists and is private to the current user (mode `0700`).
/// Session artifacts underneath (`output.bin`, `manifest.json`, `launch.json`)
/// can contain echoed secrets and the first typed prompt; a `0700` parent makes
/// them unreadable by other local users even though the files themselves are
/// created under the default umask. Idempotent and cheap; safe to call at every
/// host startup.
pub fn ensure_supercli_home() -> std::io::Result<PathBuf> {
    let home = supercli_home();
    std::fs::create_dir_all(&home)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&home) {
            let mut perms = metadata.permissions();
            if perms.mode() & 0o777 != 0o700 {
                perms.set_mode(0o700);
                let _ = std::fs::set_permissions(&home, perms);
            }
        }
    }
    Ok(home)
}

pub fn app_state_path() -> PathBuf {
    supercli_home().join("app-state.json")
}

/// Path to the grants file (sharded from app-state.json for concurrency).
/// S2: persist_grant was serializing on the app-state.json exclusive lock;
/// grants have no chain semantics, so they get their own file+lock.
pub fn grants_path() -> PathBuf {
    supercli_home().join("grants.json")
}

pub fn activity_state_path() -> PathBuf {
    supercli_home().join("activity-state.json")
}

pub fn app_sessions_root() -> PathBuf {
    supercli_home().join("app-sessions")
}

pub fn worktrees_root() -> PathBuf {
    supercli_home().join("worktrees")
}

#[cfg(test)]
mod machine_home_tests {
    use super::*;

    #[test]
    fn a_registered_workspace_shares_the_machine_home_and_others_keep_their_own() {
        let real = PathBuf::from("/Users/me/.supercli");
        let workspace = PathBuf::from("/Users/me/.supercli/profiles/work");
        let blank = PathBuf::from("/tmp/upblank");
        assert_eq!(machine_home_from(workspace, real.clone(), true), real);
        assert_eq!(machine_home_from(blank.clone(), real.clone(), false), blank);
        assert_eq!(machine_home_from(real.clone(), real.clone(), false), real);
    }

    #[test]
    fn registry_parsing_tolerates_unknown_keys_and_skips_homeless_records() {
        let raw = br#"{"version":1,"profiles":[
            {"id":"a","name":"Work","home":"/Users/me/.supercli/profiles/work","extra":true},
            {"id":"b","name":"Broken"},
            {"id":"c","name":"Blank","home":"   "}
        ]}"#;
        let records = parse_workspace_registry(raw);
        assert_eq!(
            records,
            vec![WorkspaceRegistryRecord {
                id: "a".into(),
                name: "Work".into(),
                home: PathBuf::from("/Users/me/.supercli/profiles/work"),
            }]
        );
        assert!(parse_workspace_registry(b"not json").is_empty());
        assert!(parse_workspace_registry(b"{}").is_empty());
    }

    #[test]
    fn registered_homes_are_matched_from_the_real_registry_file() {
        let real = std::env::temp_dir().join(format!("upmh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&real);
        let workspace = real.join("profiles").join("work");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(
            workspace_registry_path(&real),
            format!(
                r#"{{"version":1,"profiles":[{{"id":"w","name":"Work","home":"{}"}}]}}"#,
                workspace.display()
            ),
        )
        .unwrap();
        assert!(is_registered_workspace_home(&real, &workspace));
        assert!(!is_registered_workspace_home(
            &real,
            &real.join("elsewhere")
        ));
        assert_eq!(read_workspace_registry(&real)[0].name, "Work");
        let _ = std::fs::remove_dir_all(&real);
    }
}
