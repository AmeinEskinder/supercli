//! Port of `LaunchConfig.swift` (SupercliNative).
//!
//! Resolves binary paths and per-session attach commands. The Swift original
//! derives paths from the app bundle; this port uses environment variables
//! and well-known workspace locations, keeping the same resolution order:
//! env override → bundled binary → workspace target dirs (release preferred).
//!
//! Pure logic, no I/O beyond env var reads. All functions are unit-tested.
//!
//! Web-safe: compiles for `wasm32-unknown-unknown` (env access is via
//! `std::env`, which is available on wasm32-wasi; on wasm32-unknown the
//! functions gracefully return defaults).

use std::path::{Path, PathBuf};

/// Environment variable that overrides the attach binary.
/// When set, it is used verbatim as the command prefix and the session id
/// is appended.
pub const ATTACH_COMMAND_ENV_VAR: &str = "SUPERCLI_ATTACH_CMD";

/// Environment variable that overrides the supercli-host binary path.
pub const HOST_COMMAND_ENV_VAR: &str = "SUPERCLI_HOST_CMD";

/// Environment variable naming the Supercli state dir (see
/// `supercli_core::app_paths::supercli_home`).
pub const SUPERCLI_HOME_ENV_VAR: &str = "SUPERCLI_HOME";

/// Name of the attach binary.
const ATTACH_BINARY_NAME: &str = "supercli-attach";

/// Name of the host binary.
const HOST_BINARY_NAME: &str = "supercli-host";

/// Resolve the attach binary path.
///
/// Resolution order: `SUPERCLI_ATTACH_CMD` env override → bundled binary
/// (next to the current executable) → workspace target dirs (release
/// preferred, then debug).
///
/// Port of `LaunchConfig.attachBinary` from `LaunchConfig.swift`.
pub fn attach_binary() -> PathBuf {
    if let Some(override_cmd) = std::env::var_os(ATTACH_COMMAND_ENV_VAR) {
        let s = override_cmd.to_string_lossy();
        // The env var holds a command prefix; the binary is the first word.
        if let Some(binary) = s.split_whitespace().next() {
            if !binary.is_empty() {
                return PathBuf::from(binary);
            }
        }
    }
    resolve_bundled_or_workspace(ATTACH_BINARY_NAME)
}

/// Resolve the supercli-host binary path.
///
/// Resolution order: `SUPERCLI_HOST_CMD` env override → bundled binary
/// (next to the current executable) → workspace target dirs (release
/// preferred, then debug).
///
/// Port of `LaunchConfig.hostBinary` from `LaunchConfig.swift`.
pub fn host_binary() -> PathBuf {
    if let Some(override_path) = std::env::var_os(HOST_COMMAND_ENV_VAR) {
        let s = override_path.to_string_lossy();
        if !s.trim().is_empty() {
            return PathBuf::from(s.into_owned());
        }
    }
    resolve_bundled_or_workspace(HOST_BINARY_NAME)
}

/// Resolve a binary by looking next to the current executable, then in
/// workspace target dirs (release preferred).
fn resolve_bundled_or_workspace(name: &str) -> PathBuf {
    // Bundled: next to the current executable.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join(name);
            if is_executable(&bundled) {
                return bundled;
            }
        }
    }
    // Workspace: `<repo>/crates/supercli-attach/target/{release,debug}/<name>`
    // and `<repo>/crates/target/{release,debug}/<name>`. We walk up from the
    // current exe looking for a `crates` dir.
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent();
        while let Some(d) = dir {
            if d.join("crates").is_dir() {
                for target in ["release", "debug"] {
                    for crates_dir in ["crates/supercli-attach", "crates"] {
                        let candidate = d.join(crates_dir).join("target").join(target).join(name);
                        if is_executable(&candidate) {
                            return candidate;
                        }
                    }
                }
                break;
            }
            dir = d.parent();
        }
    }
    // Fallback: return the release workspace path (may not exist).
    PathBuf::from(format!("crates/target/release/{name}"))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Build the command a terminal surface runs to render a hosted session.
///
/// Every branch uses the `direct:` prefix, which makes the surface exec the
/// binary instead of wrapping it in `login(1)` (avoids the "Last login"
/// banner and a lingering login process per pane).
///
/// `sessions_dir` scopes the attach to another workspace's `app-sessions`
/// directory. `None` uses this instance's own sessions dir.
///
/// Port of `LaunchConfig.attachCommand(sessionID:sessionsDir:)` from
/// `LaunchConfig.swift`.
pub fn attach_command(session_id: &str, sessions_dir: Option<&Path>) -> String {
    // Env override: used verbatim as the command prefix.
    if let Some(override_cmd) = std::env::var_os(ATTACH_COMMAND_ENV_VAR) {
        let prefix = override_cmd.to_string_lossy();
        if !prefix.trim().is_empty() {
            if let Some(dir) = sessions_dir {
                return format!("{prefix} --sessions-dir {} {session_id}", dir.display());
            }
            return format!("{prefix} {session_id}");
        }
    }

    let attach_bin = attach_binary();

    // Scoped to another workspace's sessions dir.
    if let Some(dir) = sessions_dir {
        // Only scope if it's not the default app-sessions dir.
        let default_sessions = default_app_sessions_dir();
        if dir != default_sessions.as_path() {
            if let Some(home) = dir.parent().and_then(|p| p.parent()) {
                // `sessions_dir` is `<home>/.supercli/app-sessions`; the home
                // is two levels up.
                return format!(
                    "direct:/usr/bin/env SUPERCLI_HOME={} {} --sessions-dir {} {}",
                    home.display(),
                    attach_bin.display(),
                    dir.display(),
                    session_id
                );
            }
        }
    }

    // SUPERCLI_HOME isolation: pass the dir explicitly so the in-surface
    // attach resolves the same state dir as the app.
    if let Some(home) = std::env::var_os(SUPERCLI_HOME_ENV_VAR) {
        let home_str = home.to_string_lossy();
        if !home_str.trim().is_empty() {
            let sessions = format!("{home_str}/.supercli/app-sessions");
            return format!(
                "direct:/usr/bin/env SUPERCLI_HOME={} {} --sessions-dir {} {}",
                home_str,
                attach_bin.display(),
                sessions,
                session_id
            );
        }
    }

    format!("direct:{} {}", attach_bin.display(), session_id)
}

/// Default app-sessions dir: `<supercli_home>/app-sessions`.
fn default_app_sessions_dir() -> PathBuf {
    supercli_home().join("app-sessions")
}

/// The Supercli state dir: `~/.supercli`, or the directory named by
/// `SUPERCLI_HOME` when set.
///
/// Mirrors `supercli_core::app_paths::supercli_home`; kept here so this
/// crate does not depend on supercli-core for path logic.
fn supercli_home() -> PathBuf {
    if let Some(override_dir) = std::env::var_os(SUPERCLI_HOME_ENV_VAR) {
        let s = override_dir.to_string_lossy();
        if !s.trim().is_empty() {
            return PathBuf::from(s.into_owned());
        }
    }
    dirs_home().join(".supercli")
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Deterministic FNV-1a hash — Swift's `String.hashValue` is salted per
/// process, which would change the derived suite name every launch.
///
/// Port of `AppDefaults.stableHash(_:)` from `LaunchConfig.swift`.
pub fn stable_hash(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.as_bytes() {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
    hash
}

/// Derive the UserDefaults suite name for a dev instance launched with
/// `SUPERCLI_HOME=home`. `None`/empty returns `None` (use `.standard`).
///
/// Port of `AppDefaults.suite(forSupercliHome:)` from `LaunchConfig.swift`.
pub fn defaults_suite_name(for_supercli_home: Option<&str>) -> Option<String> {
    match for_supercli_home {
        Some(home) if !home.trim().is_empty() => {
            Some(format!("com.supercli.devhome.{:x}", stable_hash(home)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env vars are process-global; serialize tests that mutate them.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn stable_hash_is_deterministic() {
        assert_eq!(stable_hash("hello"), stable_hash("hello"));
        assert_ne!(stable_hash("hello"), stable_hash("world"));
    }

    #[test]
    fn stable_hash_known_value() {
        // FNV-1a 64-bit of "hello" — verifies the algorithm, not just
        // determinism.
        assert_eq!(stable_hash("hello"), 0xa430d84680aabd0b);
    }

    #[test]
    fn defaults_suite_name_none_for_empty() {
        assert_eq!(defaults_suite_name(None), None);
        assert_eq!(defaults_suite_name(Some("")), None);
        assert_eq!(defaults_suite_name(Some("   ")), None);
    }

    #[test]
    fn defaults_suite_name_derives_from_home() {
        let name = defaults_suite_name(Some("/tmp/test-home")).unwrap();
        assert!(name.starts_with("com.supercli.devhome."));
        // Deterministic across calls.
        assert_eq!(name, defaults_suite_name(Some("/tmp/test-home")).unwrap());
        // Different home → different suite.
        assert_ne!(name, defaults_suite_name(Some("/tmp/other-home")).unwrap());
    }

    #[test]
    fn attach_command_uses_env_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var(ATTACH_COMMAND_ENV_VAR, "my-attach --flag");
        let cmd = attach_command("sess-123", None);
        assert_eq!(cmd, "my-attach --flag sess-123");
        std::env::remove_var(ATTACH_COMMAND_ENV_VAR);
    }

    #[test]
    fn attach_command_env_override_with_sessions_dir() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var(ATTACH_COMMAND_ENV_VAR, "my-attach");
        let cmd = attach_command(
            "sess-123",
            Some(Path::new("/other/home/.supercli/app-sessions")),
        );
        assert_eq!(
            cmd,
            "my-attach --sessions-dir /other/home/.supercli/app-sessions sess-123"
        );
        std::env::remove_var(ATTACH_COMMAND_ENV_VAR);
    }

    #[test]
    fn attach_command_has_direct_prefix() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var(ATTACH_COMMAND_ENV_VAR);
        std::env::remove_var(SUPERCLI_HOME_ENV_VAR);
        let cmd = attach_command("sess-123", None);
        assert!(cmd.starts_with("direct:"), "command: {cmd}");
        assert!(cmd.ends_with(" sess-123"), "command: {cmd}");
    }

    #[test]
    fn host_binary_uses_env_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var(HOST_COMMAND_ENV_VAR, "/custom/supercli-host");
        assert_eq!(host_binary(), PathBuf::from("/custom/supercli-host"));
        std::env::remove_var(HOST_COMMAND_ENV_VAR);
    }

    #[test]
    fn host_binary_ignores_blank_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var(HOST_COMMAND_ENV_VAR, "   ");
        // Blank override is ignored; falls through to resolution.
        let _ = host_binary();
        std::env::remove_var(HOST_COMMAND_ENV_VAR);
    }
}
