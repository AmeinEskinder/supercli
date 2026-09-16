use std::path::PathBuf;

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// The real per-user Unpeel root, deliberately ignoring `UNPEEL_HOME`.
///
/// Machine-wide coordination (the workspace registry and Host-service
/// supervisor) lives here. A scoped workspace process must still be able to
/// find that coordination root without accidentally treating its own
/// isolated state directory as the machine root.
pub fn real_unpeel_home() -> PathBuf {
    home_dir().join(".unpeel")
}

/// The Unpeel state dir: `~/.unpeel`, or the directory named by `UNPEEL_HOME`
/// when that env var is set and non-empty. The native app sets it for blank
/// dev instances and spawns hosts with the env inherited, so app + host agree
/// on one isolated state dir.
/// Env var the workspace supervisor (and `unpeel --workspace`) exports to a
/// re-homed process so it still knows the machine root it belongs to.
pub const MACHINE_HOME_ENV: &str = "UNPEEL_MACHINE_HOME";

/// The root that per-user integration state belongs to: the machine's
/// `~/.unpeel` for every local workspace of one account, and the isolated
/// `UNPEEL_HOME` itself for a blank dev instance or a test. Provider
/// configs (`~/.claude.json`, `~/.codex/config.toml`, …) are per user, not
/// per workspace, so the markers, hook scripts, and MCP shim they point at
/// live here — installing an integration in one local workspace installs it
/// for all of them.
///
/// Resolution: `UNPEEL_MACHINE_HOME` (set by the supervisor for a workspace
/// worker and by `unpeel --workspace` before it re-homes the process) wins;
/// otherwise this process's own `unpeel_home()`, which keeps tests and blank
/// instances private without any registry lookup.
pub fn machine_home() -> PathBuf {
    machine_home_from(
        std::env::var_os(MACHINE_HOME_ENV).map(|value| value.to_string_lossy().to_string()),
        unpeel_home(),
    )
}

pub fn machine_home_from(machine_env: Option<String>, own_home: PathBuf) -> PathBuf {
    match machine_env.map(|value| value.trim().to_string()) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => own_home,
    }
}

pub fn unpeel_home() -> PathBuf {
    if let Some(override_dir) = std::env::var_os("UNPEEL_HOME") {
        let trimmed = override_dir.to_string_lossy().trim().to_string();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    real_unpeel_home()
}

/// Ensure `~/.unpeel` exists and is private to the current user (mode `0700`).
/// Session artifacts underneath (`output.bin`, `manifest.json`, `launch.json`)
/// can contain echoed secrets and the first typed prompt; a `0700` parent makes
/// them unreadable by other local users even though the files themselves are
/// created under the default umask. Idempotent and cheap; safe to call at every
/// host startup.
pub fn ensure_unpeel_home() -> std::io::Result<PathBuf> {
    let home = unpeel_home();
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
    unpeel_home().join("app-state.json")
}

pub fn activity_state_path() -> PathBuf {
    unpeel_home().join("activity-state.json")
}

pub fn app_sessions_root() -> PathBuf {
    unpeel_home().join("app-sessions")
}

pub fn worktrees_root() -> PathBuf {
    unpeel_home().join("worktrees")
}

#[cfg(test)]
mod machine_home_tests {
    use super::*;

    #[test]
    fn machine_home_prefers_the_exported_root_and_falls_back_to_the_own_home() {
        let own = PathBuf::from("/tmp/ws-home");
        assert_eq!(machine_home_from(None, own.clone()), own);
        assert_eq!(machine_home_from(Some("   ".into()), own.clone()), own);
        assert_eq!(
            machine_home_from(Some(" /Users/me/.unpeel ".into()), own),
            PathBuf::from("/Users/me/.unpeel")
        );
    }
}
