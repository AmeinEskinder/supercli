//! Workspace registry: multiple isolated app instances on one machine.
//!
//! Moved from the Dart client (`clients/supercli-app/lib/screens/workspace_registry.dart`)
//! per Amein's rule: one implementation, in Rust. The Dart client keeps only UI
//! bindings via `supercli-client-ffi`.
//!
//! A workspace is a separate running instance with its own SUPERCLI_HOME. The
//! released registry lives in the REAL home (`~/.supercli/profiles.json`), never
//! the instance's supercliDir. The on-disk spelling is a released compatibility
//! contract: the collection is encoded as `profiles` even though every source
//! and UI surface calls the product concept Workspaces now.
//!
//! Only the portable logic is here (record codec, slugify, path normalization,
//! list-order keys + persisted order, create/rename/remove, environment
//! context, pid-file liveness, launcher env construction). Platform I/O is
//! injected via the [`WorkspaceRegistryIo`] trait.
//!
//! DROPPED: `SupercliWorkspaceLauncher.showWindow(home:)` — AppKit/UI-only
//! (asks a running GUI instance to show its window); no portable behavior.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A workspace record: one isolated app instance.
///
/// The absolute `home` is minted once at create; rename never moves it (hook
/// configs may already point into it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRecord {
    pub id: String,
    pub name: String,
    pub home: PathBuf,
    pub created_at_ms: u64,
}

/// Platform I/O for the workspace registry. Injected for testability.
pub trait WorkspaceRegistryIo {
    /// Read the registry file bytes, or None if missing.
    fn read_registry(&self) -> Option<Vec<u8>>;
    /// Write the registry file bytes atomically.
    fn write_registry(&mut self, data: &[u8]) -> std::io::Result<()>;
    /// Create a directory (including parents).
    fn create_directory(&mut self, path: &Path) -> std::io::Result<()>;
    /// True if a path exists.
    fn path_exists(&self, path: &Path) -> bool;
    /// Delete a directory tree.
    fn delete_directory(&mut self, path: &Path) -> std::io::Result<()>;
    /// Current time in milliseconds since epoch.
    fn now_ms(&self) -> u64;
    /// Generate a random UUID (lowercased).
    fn new_uuid(&self) -> String;
    /// Read the persisted default-workspace display alias, if any.
    /// (Swift: UserDefaults `supercli.native.defaultWorkspaceName`.)
    fn read_default_workspace_name(&self) -> Option<String> {
        None
    }
    /// Persist the default-workspace display alias.
    fn write_default_workspace_name(&mut self, _name: &str) -> std::io::Result<()> {
        Ok(())
    }
    /// Read the persisted unified workspace-list order (kind-prefixed keys).
    /// (Swift: AppDefaults `supercli.native.workspaceOrder`.)
    fn read_workspace_order(&self) -> Vec<String> {
        Vec::new()
    }
    /// Persist the unified workspace-list order.
    fn write_workspace_order(&mut self, _keys: &[String]) -> std::io::Result<()> {
        Ok(())
    }
}

/// Error for workspace operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceError(pub String);

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WorkspaceError: {}", self.0)
    }
}

impl std::error::Error for WorkspaceError {}

/// Decode the registry JSON bytes to records.
/// Returns an empty vec on invalid JSON (fail-open for reads).
pub fn decode_registry(data: &[u8]) -> Vec<WorkspaceRecord> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
        return Vec::new();
    };
    value
        .get("profiles")
        .and_then(|p| p.as_array())
        .map(|profiles| {
            profiles
                .iter()
                .filter_map(|e| {
                    Some(WorkspaceRecord {
                        id: e.get("id")?.as_str()?.to_string(),
                        name: e.get("name")?.as_str()?.to_string(),
                        home: PathBuf::from(e.get("home")?.as_str()?),
                        created_at_ms: e.get("createdAt").and_then(|v| v.as_u64()).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Encode records to the registry JSON bytes.
pub fn encode_registry(workspaces: &[WorkspaceRecord]) -> Vec<u8> {
    let profiles: Vec<serde_json::Value> = workspaces
        .iter()
        .map(|w| {
            serde_json::json!({
                "id": w.id,
                "name": w.name,
                "home": w.home.to_string_lossy(),
                "createdAt": w.created_at_ms,
            })
        })
        .collect();
    let doc = serde_json::json!({ "version": 1, "profiles": profiles });
    serde_json::to_vec(&doc).unwrap_or_default()
}

/// Load the registry, returning empty on any failure.
pub fn load_registry(io: &dyn WorkspaceRegistryIo) -> Vec<WorkspaceRecord> {
    io.read_registry()
        .map(|d| decode_registry(&d))
        .unwrap_or_default()
}

/// Save the registry.
pub fn save_registry(
    io: &mut dyn WorkspaceRegistryIo,
    workspaces: &[WorkspaceRecord],
) -> std::io::Result<()> {
    io.write_registry(&encode_registry(workspaces))
}

/// Create a workspace record. Returns the new record.
/// Errors on empty name.
pub fn create_workspace(
    io: &mut dyn WorkspaceRegistryIo,
    registry_dir: &Path,
    name: &str,
) -> Result<WorkspaceRecord, WorkspaceError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(WorkspaceError("Give the workspace a name.".into()));
    }
    // Re-read right before mutating: another instance may have edited the
    // registry (atomic last-writer-wins is the concurrency model).
    let mut workspaces = load_registry(io);
    let slug = unique_slug(io, registry_dir, trimmed, &workspaces);
    let home = registry_dir.join("profiles").join(&slug);
    io.create_directory(&home)
        .map_err(|e| WorkspaceError(format!("create workspace dir: {e}")))?;
    let record = WorkspaceRecord {
        id: io.new_uuid().to_lowercase(),
        name: trimmed.to_string(),
        home,
        created_at_ms: io.now_ms(),
    };
    workspaces.push(record.clone());
    save_registry(io, &workspaces).map_err(|e| WorkspaceError(format!("save registry: {e}")))?;
    Ok(record)
}

/// Rename a workspace. No-op on empty name or unknown id.
pub fn rename_workspace(
    io: &mut dyn WorkspaceRegistryIo,
    id: &str,
    name: &str,
) -> std::io::Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let mut workspaces = load_registry(io);
    let Some(index) = workspaces.iter().position(|w| w.id == id) else {
        return Ok(());
    };
    workspaces[index].name = trimmed.to_string();
    save_registry(io, &workspaces)
}

/// Forget a workspace. `delete_data` also removes its home dir — but only if
/// the home is under the managed root.
pub fn remove_workspace(
    io: &mut dyn WorkspaceRegistryIo,
    registry_dir: &Path,
    id: &str,
    delete_data: bool,
) -> std::io::Result<()> {
    let mut workspaces = load_registry(io);
    let Some(index) = workspaces.iter().position(|w| w.id == id) else {
        return Ok(());
    };
    let record = workspaces.remove(index);
    save_registry(io, &workspaces)?;
    if delete_data {
        let normalized = normalize_path(&record.home);
        let root = normalize_path(&registry_dir.join("profiles"));
        if normalized.starts_with(&root) {
            let _ = io.delete_directory(&record.home);
        }
    }
    Ok(())
}

/// Convert a display name to a URL-safe slug.
///
/// Mirrors the native registry's slugify: lowercase ASCII alphanumerics,
/// runs of anything else collapse to one dash, no leading/trailing dashes.
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = true; // suppress leading dashes
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "workspace".to_string()
    } else {
        slug
    }
}

/// Normalize a path: expand tilde, absolutize.
pub fn normalize_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    let expanded = if let Some(rest) = s.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/{rest}")
    } else {
        s.into_owned()
    };
    let p = PathBuf::from(expanded);
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}

fn unique_slug(
    io: &dyn WorkspaceRegistryIo,
    registry_dir: &Path,
    name: &str,
    existing: &[WorkspaceRecord],
) -> String {
    let base = slugify(name);
    let taken: HashSet<PathBuf> = existing.iter().map(|w| normalize_path(&w.home)).collect();
    let mut candidate = base.clone();
    let mut counter = 2;
    loop {
        let path = registry_dir.join("profiles").join(&candidate);
        if !taken.contains(&normalize_path(&path)) && !io.path_exists(&path) {
            return candidate;
        }
        candidate = format!("{base}-{counter}");
        counter += 1;
    }
}

/// Controller-local, user-chosen display order for the unified workspace list.
///
/// Keys are kind-prefixed so local homes and remote host ids can never
/// collide: `local:<normalized home>`, `host:<hostID>`, `ssh:<id>`.
/// Unknown keys keep their natural build order after the saved ones.
pub mod list_order {
    use super::normalize_path;
    use std::path::Path;

    pub fn local_key(home: &Path) -> String {
        format!("local:{}", normalize_path(home).to_string_lossy())
    }

    pub fn paired_key(host_id: &str) -> String {
        format!("host:{host_id}")
    }

    pub fn ssh_key(id: &str) -> String {
        format!("ssh:{id}")
    }

    /// Stable sort by saved position; unsaved keys follow in build order.
    pub fn apply<T>(rows: Vec<T>, saved_order: &[String], key: impl Fn(&T) -> String) -> Vec<T> {
        let position: std::collections::HashMap<&str, usize> = saved_order
            .iter()
            .enumerate()
            .map(|(i, k)| (k.as_str(), i))
            .collect();
        let mut indexed: Vec<(usize, T)> = rows.into_iter().enumerate().collect();
        indexed.sort_by(|(ia, a), (ib, b)| {
            let pa = position.get(key(a).as_str());
            let pb = position.get(key(b).as_str());
            match (pa, pb) {
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => ia.cmp(ib),
            }
        });
        indexed.into_iter().map(|(_, v)| v).collect()
    }
}

/// Load the persisted unified workspace-list order.
pub fn load_workspace_order(io: &dyn WorkspaceRegistryIo) -> Vec<String> {
    io.read_workspace_order()
}

/// Save the unified workspace-list order. Saving rewrites the full list, so
/// removed workspaces age out on their own.
pub fn save_workspace_order(
    io: &mut dyn WorkspaceRegistryIo,
    keys: &[String],
) -> std::io::Result<()> {
    io.write_workspace_order(keys)
}

/// Optional user alias for the implicit/default workspace. Keeping this
/// separate from `display_name` preserves None as the default-instance
/// sentinel. Mirrors `SupercliWorkspaceContext.defaultWorkspaceName`
/// (trims; empty reads as None).
pub fn default_workspace_name(io: &dyn WorkspaceRegistryIo) -> Option<String> {
    io.read_default_workspace_name().and_then(|name| {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// Rename the default workspace's display alias. Returns false on empty
/// name (no-op). Mirrors `SupercliWorkspaceContext.renameDefaultWorkspace`.
pub fn rename_default_workspace(
    io: &mut dyn WorkspaceRegistryIo,
    raw_name: &str,
) -> std::io::Result<bool> {
    let name = raw_name.trim();
    if name.is_empty() {
        return Ok(false);
    }
    io.write_default_workspace_name(name)?;
    Ok(true)
}

/// Which workspace THIS process is, resolved from its `SUPERCLI_HOME`
/// environment value against the registry.
///
/// `supercli_home` is `None` when the variable is unset — pass
/// `std::env::var("SUPERCLI_HOME").ok()` at the real call site; tests inject
/// values directly. Mirrors `SupercliWorkspaceContext`.
pub mod context {
    use super::{
        default_workspace_name, load_registry, normalize_path, WorkspaceRecord, WorkspaceRegistryIo,
    };
    use std::path::Path;

    /// The env var that selects the workspace instance.
    pub const HOME_ENV_VAR: &str = "SUPERCLI_HOME";

    /// True when this process is the default instance (no `SUPERCLI_HOME`,
    /// or a blank one). Mirrors `SupercliWorkspaceContext.isDefaultInstance`.
    pub fn is_default_instance(supercli_home: Option<&str>) -> bool {
        supercli_home.is_none_or(|h| h.trim().is_empty())
    }

    /// Registry entry for this instance's `SUPERCLI_HOME`; None for the
    /// default instance and for unregistered homes (dev-blank runs).
    /// Re-reads the registry per call so renames from another instance apply
    /// live. Mirrors `SupercliWorkspaceContext.currentWorkspace`.
    pub fn current_workspace(
        io: &dyn WorkspaceRegistryIo,
        supercli_home: Option<&str>,
    ) -> Option<WorkspaceRecord> {
        if is_default_instance(supercli_home) {
            return None;
        }
        let home = normalize_path(Path::new(supercli_home.unwrap_or_default()));
        load_registry(io)
            .into_iter()
            .find(|w| normalize_path(&w.home) == home)
    }

    /// Display name for this instance: None for the default instance; the
    /// registry name when registered; the `SUPERCLI_HOME` dir name for
    /// unregistered homes so even those are tellable apart.
    /// Mirrors `SupercliWorkspaceContext.displayName`.
    pub fn display_name(
        io: &dyn WorkspaceRegistryIo,
        supercli_home: Option<&str>,
    ) -> Option<String> {
        if is_default_instance(supercli_home) {
            return None;
        }
        if let Some(workspace) = current_workspace(io, supercli_home) {
            return Some(workspace.name);
        }
        Path::new(supercli_home.unwrap_or_default())
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
    }

    /// The single choke point for the name this instance advertises to
    /// phones (pairing, bootstrap, Bonjour): workspace name for an isolated
    /// instance, otherwise the default-workspace alias, otherwise the
    /// machine's local host name, otherwise "Mac".
    /// Mirrors `SupercliWorkspaceContext.advertisedHostName`.
    pub fn advertised_host_name(
        io: &dyn WorkspaceRegistryIo,
        supercli_home: Option<&str>,
        local_host_name: Option<&str>,
    ) -> String {
        display_name(io, supercli_home)
            .or_else(|| default_workspace_name(io))
            .or_else(|| local_host_name.map(|s| s.to_string()))
            .unwrap_or_else(|| "Mac".to_string())
    }
}

/// Launching and liveness of workspace instances.
///
/// A per-home `app.pid` (written at startup) is the running marker; identity
/// is verified against the kernel-reported process start time before trusting
/// it — same pid-reuse discipline as the hosted-session manifests.
///
/// DROPPED: `SupercliWorkspaceLauncher.showWindow(home:)` — AppKit/UI-only.
/// Its entire purpose is to make a running GUI instance show its window
/// (POST /show-window to that home's hook-server ports); there is no
/// portable behavior to keep.
pub mod launcher {
    use super::{WorkspaceError, WorkspaceRecord};
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;

    /// Pid-reuse tolerance: recorded vs kernel-reported start times must agree
    /// within this window. Mirrors `pidStartToleranceMs`.
    pub const PID_START_TOLERANCE_MS: u64 = 10_000;

    /// Env marker for a windowless (menu-bar agent) launch.
    pub const LAUNCH_HIDDEN_ENV_VAR: &str = "SUPERCLI_LAUNCH_HIDDEN";

    /// Contents of `<home>/app.pid`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct AppPidFile {
        pub pid: u32,
        pub pid_started_at: Option<u64>,
    }

    pub fn pid_file_path(home: &Path) -> PathBuf {
        home.join("app.pid")
    }

    /// Pid of the live instance owning `home`, or None (missing/stale/
    /// unverifiable pidfile). `process_start_time_ms` is injected for
    /// testability — pass the host's process-start-time lookup in
    /// production. Mirrors `SupercliWorkspaceLauncher.runningPid`.
    pub fn running_pid(
        home: &Path,
        process_start_time_ms: impl Fn(u32) -> Option<u64>,
    ) -> Option<u32> {
        let data = std::fs::read(pid_file_path(home)).ok()?;
        let file: AppPidFile = serde_json::from_slice(&data).ok()?;
        if file.pid <= 1 {
            return None;
        }
        let actual = process_start_time_ms(file.pid)?;
        let recorded = file.pid_started_at?;
        if actual.abs_diff(recorded) <= PID_START_TOLERANCE_MS {
            Some(file.pid)
        } else {
            None
        }
    }

    /// Written by every instance for its own home at startup (atomic).
    /// Mirrors `SupercliWorkspaceLauncher.writeOwnPidFile`.
    pub fn write_own_pid_file(
        home: &Path,
        pid: u32,
        process_start_time_ms: impl Fn(u32) -> Option<u64>,
    ) -> std::io::Result<()> {
        std::fs::create_dir_all(home)?;
        let file = AppPidFile {
            pid,
            pid_started_at: process_start_time_ms(pid),
        };
        let data = serde_json::to_vec(&file).map_err(std::io::Error::other)?;
        let tmp = home.join("app.pid.tmp");
        std::fs::write(&tmp, &data)?;
        std::fs::rename(&tmp, pid_file_path(home))?;
        Ok(())
    }

    /// Mirrors `SupercliWorkspaceLauncher.removeOwnPidFile`.
    pub fn remove_own_pid_file(home: &Path) -> std::io::Result<()> {
        let path = pid_file_path(home);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// True when another live process already owns this instance's home.
    /// Mirrors `SupercliWorkspaceLauncher.otherInstanceOwnsCurrentHome`.
    pub fn other_instance_owns_home(
        home: &Path,
        own_pid: u32,
        process_start_time_ms: impl Fn(u32) -> Option<u64>,
    ) -> bool {
        running_pid(home, process_start_time_ms).is_some_and(|pid| pid != own_pid)
    }

    /// Build the child environment for launching a workspace instance:
    /// sets `SUPERCLI_HOME`, strips test/snapshot vars so they never leak
    /// into a user-facing instance, and sets (or removes) the hidden-launch
    /// marker. Pure and directly testable.
    pub fn launch_env(
        workspace_home: &Path,
        base_env: &HashMap<String, String>,
        hidden: bool,
    ) -> HashMap<String, String> {
        let mut env: HashMap<String, String> = base_env
            .iter()
            .filter(|(k, _)| {
                !k.starts_with("SUPERCLI_TEST_") && !k.starts_with("SUPERCLI_SNAPSHOT")
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        env.insert(
            super::context::HOME_ENV_VAR.to_string(),
            workspace_home.to_string_lossy().into_owned(),
        );
        if hidden {
            env.insert(LAUNCH_HIDDEN_ENV_VAR.to_string(), "1".to_string());
        } else {
            // Never inherit a hidden marker from a hidden-launched parent.
            env.remove(LAUNCH_HIDDEN_ENV_VAR);
        }
        env
    }

    /// Launch a workspace as a second instance of the app binary.
    /// Direct-exec on purpose: `open`/NSWorkspace neither forwards env nor
    /// starts a second instance of an already-running bundle id.
    /// `hidden` starts the instance WINDOWLESS (menu-bar agent state).
    /// `executable` is the app binary (Swift: `Bundle.main.executableURL`).
    /// Refuses while the workspace is already running.
    /// Mirrors `SupercliWorkspaceLauncher.launch(_:hidden:)`.
    pub fn launch_workspace(
        workspace: &WorkspaceRecord,
        executable: &Path,
        base_env: &HashMap<String, String>,
        hidden: bool,
        process_start_time_ms: impl Fn(u32) -> Option<u64>,
    ) -> Result<(), WorkspaceError> {
        if running_pid(&workspace.home, &process_start_time_ms).is_some() {
            return Err(WorkspaceError(format!(
                "{} is already running.",
                workspace.name
            )));
        }
        std::fs::create_dir_all(&workspace.home)
            .map_err(|e| WorkspaceError(format!("create workspace home: {e}")))?;
        let env = launch_env(&workspace.home, base_env, hidden);
        std::process::Command::new(executable)
            .env_clear()
            .envs(&env)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| WorkspaceError(format!("launch workspace: {e}")))?;
        // No wait: the child is a full GUI app that outlives us.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct MemIo {
        registry: Option<Vec<u8>>,
        dirs: HashSet<PathBuf>,
        now: u64,
        uuid_counter: RefCell<u32>,
        default_name: Option<String>,
        order: Vec<String>,
    }

    impl MemIo {
        fn new() -> Self {
            Self {
                registry: None,
                dirs: HashSet::new(),
                now: 1_700_000_000_000,
                uuid_counter: RefCell::new(0),
                default_name: None,
                order: Vec::new(),
            }
        }
    }

    impl WorkspaceRegistryIo for MemIo {
        fn read_registry(&self) -> Option<Vec<u8>> {
            self.registry.clone()
        }
        fn write_registry(&mut self, data: &[u8]) -> std::io::Result<()> {
            self.registry = Some(data.to_vec());
            Ok(())
        }
        fn create_directory(&mut self, path: &Path) -> std::io::Result<()> {
            self.dirs.insert(path.to_path_buf());
            Ok(())
        }
        fn path_exists(&self, path: &Path) -> bool {
            self.dirs.contains(path)
        }
        fn delete_directory(&mut self, path: &Path) -> std::io::Result<()> {
            self.dirs.remove(path);
            Ok(())
        }
        fn now_ms(&self) -> u64 {
            self.now
        }
        fn new_uuid(&self) -> String {
            let mut c = self.uuid_counter.borrow_mut();
            *c += 1;
            format!("uuid-{c:04}")
        }
        fn read_default_workspace_name(&self) -> Option<String> {
            self.default_name.clone()
        }
        fn write_default_workspace_name(&mut self, name: &str) -> std::io::Result<()> {
            self.default_name = Some(name.to_string());
            Ok(())
        }
        fn read_workspace_order(&self) -> Vec<String> {
            self.order.clone()
        }
        fn write_workspace_order(&mut self, keys: &[String]) -> std::io::Result<()> {
            self.order = keys.to_vec();
            Ok(())
        }
    }

    #[test]
    fn slugify_matches_dart() {
        assert_eq!(slugify("My Workspace"), "my-workspace");
        assert_eq!(slugify("  Leading"), "leading");
        assert_eq!(slugify("Trailing  "), "trailing");
        assert_eq!(slugify("a--b__c"), "a-b-c");
        assert_eq!(slugify("!!!"), "workspace");
        assert_eq!(slugify(""), "workspace");
        assert_eq!(slugify("Café"), "caf");
    }

    #[test]
    fn codec_round_trip() {
        let records = vec![WorkspaceRecord {
            id: "id1".into(),
            name: "Work".into(),
            home: PathBuf::from("/Users/me/.supercli/profiles/work"),
            created_at_ms: 123,
        }];
        let bytes = encode_registry(&records);
        let back = decode_registry(&bytes);
        assert_eq!(back, records);
        // Invalid JSON -> empty
        assert!(decode_registry(b"not json").is_empty());
    }

    #[test]
    fn create_rename_remove() {
        let mut io = MemIo::new();
        let dir = PathBuf::from("/real");
        let rec = create_workspace(&mut io, &dir, "My Work").unwrap();
        assert_eq!(rec.name, "My Work");
        assert_eq!(rec.home, PathBuf::from("/real/profiles/my-work"));
        // Duplicate name gets a numeric suffix
        let rec2 = create_workspace(&mut io, &dir, "My Work").unwrap();
        assert_eq!(rec2.home, PathBuf::from("/real/profiles/my-work-2"));

        rename_workspace(&mut io, &rec.id, "Renamed").unwrap();
        let loaded = load_registry(&io);
        assert_eq!(
            loaded.iter().find(|r| r.id == rec.id).unwrap().name,
            "Renamed"
        );
        // Empty rename is a no-op
        rename_workspace(&mut io, &rec.id, "   ").unwrap();
        let loaded = load_registry(&io);
        assert_eq!(
            loaded.iter().find(|r| r.id == rec.id).unwrap().name,
            "Renamed"
        );

        // Remove without deleting data keeps the dir
        remove_workspace(&mut io, &dir, &rec.id, false).unwrap();
        assert!(io.dirs.contains(&PathBuf::from("/real/profiles/my-work")));
        // Remove with delete_data removes a managed home
        remove_workspace(&mut io, &dir, &rec2.id, true).unwrap();
        assert!(!io.dirs.contains(&PathBuf::from("/real/profiles/my-work-2")));

        // Empty name errors
        assert!(create_workspace(&mut io, &dir, "   ").is_err());
    }

    #[test]
    fn remove_never_deletes_unmanaged_home() {
        let mut io = MemIo::new();
        let dir = PathBuf::from("/real");
        // Hand-register a record pointing outside the managed root
        let outside = PathBuf::from("/elsewhere/data");
        io.dirs.insert(outside.clone());
        let rec = WorkspaceRecord {
            id: "x".into(),
            name: "X".into(),
            home: outside.clone(),
            created_at_ms: 0,
        };
        save_registry(&mut io, std::slice::from_ref(&rec)).unwrap();
        remove_workspace(&mut io, &dir, "x", true).unwrap();
        // The outside dir must survive
        assert!(io.dirs.contains(&outside));
        assert!(load_registry(&io).is_empty());
    }

    #[test]
    fn list_order_keys_and_apply() {
        use list_order::*;
        assert_eq!(paired_key("h1"), "host:h1");
        assert_eq!(ssh_key("s1"), "ssh:s1");
        assert!(local_key(Path::new("/a/b")).starts_with("local:"));

        let rows = vec!["a", "b", "c", "d"];
        let saved = vec!["k3".to_string(), "k1".to_string()];
        let key = |s: &&str| match *s {
            "a" => "k1".to_string(),
            "b" => "k2".to_string(),
            "c" => "k3".to_string(),
            _ => "k9".to_string(),
        };
        let out = apply(rows, &saved, key);
        // k3 first, k1 second, then unsaved in build order (b=k2, d=k9)
        assert_eq!(out, vec!["c", "a", "b", "d"]);
    }

    #[test]
    fn load_returns_empty_on_missing_or_corrupt() {
        let io = MemIo::new();
        assert!(load_registry(&io).is_empty());
        let mut io2 = MemIo::new();
        io2.registry = Some(b"{{{".to_vec());
        assert!(load_registry(&io2).is_empty());
    }

    // ------------------------------------------------------------------
    // Gap 3: environment context, persisted ordering, PID management,
    // launcher.
    // ------------------------------------------------------------------

    #[test]
    fn context_resolves_instance_from_env() {
        use context::*;
        let mut io = MemIo::new();
        let dir = PathBuf::from("/real");
        let rec = create_workspace(&mut io, &dir, "Isolated").unwrap();

        assert!(is_default_instance(None));
        assert!(is_default_instance(Some("")));
        assert!(is_default_instance(Some("   ")));
        assert!(!is_default_instance(Some("/real/profiles/isolated")));

        // Registered home resolves to its record.
        let home = rec.home.to_string_lossy().into_owned();
        let found = current_workspace(&io, Some(&home)).expect("registered");
        assert_eq!(found.name, "Isolated");
        // Default instance and unregistered homes: None.
        assert!(current_workspace(&io, None).is_none());
        assert!(current_workspace(&io, Some("/tmp/dev-blank")).is_none());

        // displayName: registry name when registered...
        assert_eq!(display_name(&io, Some(&home)).as_deref(), Some("Isolated"));
        // ...dir basename for unregistered homes...
        assert_eq!(
            display_name(&io, Some("/tmp/dev-blank")).as_deref(),
            Some("dev-blank")
        );
        // ...None for the default instance.
        assert_eq!(display_name(&io, None), None);
    }

    #[test]
    fn default_workspace_name_trims_and_rejects_empty() {
        let mut io = MemIo::new();
        assert_eq!(default_workspace_name(&io), None);
        assert!(!rename_default_workspace(&mut io, "   ").unwrap());
        assert_eq!(default_workspace_name(&io), None);
        assert!(rename_default_workspace(&mut io, "  Main Mac  ").unwrap());
        assert_eq!(default_workspace_name(&io).as_deref(), Some("Main Mac"));
        // A stored blank collapses to None.
        io.default_name = Some("   ".into());
        assert_eq!(default_workspace_name(&io), None);
    }

    #[test]
    fn advertised_host_name_precedence() {
        use context::*;
        let mut io = MemIo::new();
        let dir = PathBuf::from("/real");
        let rec = create_workspace(&mut io, &dir, "Isolated").unwrap();
        let home = rec.home.to_string_lossy().into_owned();

        // Isolated instance: workspace name wins over everything.
        rename_default_workspace(&mut io, "Alias").unwrap();
        assert_eq!(
            advertised_host_name(&io, Some(&home), Some("MacBook")),
            "Isolated"
        );
        // Default instance: alias, then machine name, then "Mac".
        assert_eq!(advertised_host_name(&io, None, Some("MacBook")), "Alias");
        let io2 = MemIo::new();
        assert_eq!(advertised_host_name(&io2, None, Some("MacBook")), "MacBook");
        assert_eq!(advertised_host_name(&io2, None, None), "Mac");
    }

    #[test]
    fn workspace_order_persists_round_trip() {
        let mut io = MemIo::new();
        assert!(load_workspace_order(&io).is_empty());
        let keys = vec!["host:abc".to_string(), "local:/x".to_string()];
        save_workspace_order(&mut io, &keys).unwrap();
        assert_eq!(load_workspace_order(&io), keys);
        // Saving rewrites the full list (removed workspaces age out).
        save_workspace_order(&mut io, &["ssh:1".to_string()]).unwrap();
        assert_eq!(load_workspace_order(&io), vec!["ssh:1".to_string()]);
    }

    #[test]
    #[cfg(feature = "native-host")]
    fn pid_file_liveness_verifies_start_time() {
        use launcher::*;
        let home = std::env::temp_dir().join(format!(
            "supercli-pid-test-{}-{}",
            std::process::id(),
            crate::state::current_timestamp_ms()
        ));
        std::fs::create_dir_all(&home).unwrap();
        let pid = std::process::id();

        // No pidfile: not running.
        assert_eq!(
            running_pid(&home, crate::session_host::process_start_time_ms),
            None
        );

        // Positive path only where the kernel reports start times.
        if crate::session_host::process_start_time_ms(pid).is_some() {
            write_own_pid_file(&home, pid, crate::session_host::process_start_time_ms).unwrap();
            assert_eq!(
                running_pid(&home, crate::session_host::process_start_time_ms),
                Some(pid)
            );
            assert!(!other_instance_owns_home(
                &home,
                pid,
                crate::session_host::process_start_time_ms
            ));
            assert!(other_instance_owns_home(
                &home,
                pid + 1,
                crate::session_host::process_start_time_ms
            ));
            remove_own_pid_file(&home).unwrap();
            assert_eq!(
                running_pid(&home, crate::session_host::process_start_time_ms),
                None
            );
        }

        // Stale pid (start time mismatch or missing process): None.
        let stale = AppPidFile {
            pid: u32::MAX - 7,
            pid_started_at: Some(1),
        };
        std::fs::write(pid_file_path(&home), serde_json::to_vec(&stale).unwrap()).unwrap();
        assert_eq!(
            running_pid(&home, crate::session_host::process_start_time_ms),
            None
        );

        // pid <= 1 is never trusted.
        let root = AppPidFile {
            pid: 1,
            pid_started_at: Some(1),
        };
        std::fs::write(pid_file_path(&home), serde_json::to_vec(&root).unwrap()).unwrap();
        assert_eq!(
            running_pid(&home, |_| Some(1)),
            None,
            "pid 1 rejected even with matching start time"
        );

        // Corrupt pidfile: None.
        std::fs::write(pid_file_path(&home), b"not json").unwrap();
        assert_eq!(
            running_pid(&home, crate::session_host::process_start_time_ms),
            None
        );

        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn launch_env_sanitizes_and_sets_home() {
        use launcher::*;
        let base: std::collections::HashMap<String, String> = [
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("SUPERCLI_TEST_MODE".to_string(), "1".to_string()),
            ("SUPERCLI_SNAPSHOT_X".to_string(), "y".to_string()),
            (LAUNCH_HIDDEN_ENV_VAR.to_string(), "1".to_string()),
        ]
        .into_iter()
        .collect();
        let home = Path::new("/real/profiles/w");

        let env = launch_env(home, &base, true);
        assert_eq!(
            env.get("SUPERCLI_HOME").map(String::as_str),
            Some("/real/profiles/w")
        );
        assert!(
            !env.contains_key("SUPERCLI_TEST_MODE"),
            "test vars never leak"
        );
        assert!(
            !env.contains_key("SUPERCLI_SNAPSHOT_X"),
            "snapshot vars never leak"
        );
        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin"));
        assert_eq!(
            env.get(LAUNCH_HIDDEN_ENV_VAR).map(String::as_str),
            Some("1")
        );

        // Non-hidden launch removes an inherited hidden marker.
        let env2 = launch_env(home, &base, false);
        assert!(!env2.contains_key(LAUNCH_HIDDEN_ENV_VAR));
        assert_eq!(
            env2.get("SUPERCLI_HOME").map(String::as_str),
            Some("/real/profiles/w")
        );
    }

    #[test]
    #[cfg(feature = "native-host")]
    fn launch_refuses_while_running() {
        use launcher::*;
        let home = std::env::temp_dir().join(format!(
            "supercli-launch-test-{}-{}",
            std::process::id(),
            crate::state::current_timestamp_ms()
        ));
        std::fs::create_dir_all(&home).unwrap();
        let workspace = WorkspaceRecord {
            id: "w1".into(),
            name: "Busy".into(),
            home: home.clone(),
            created_at_ms: 0,
        };
        let base = std::collections::HashMap::new();

        if crate::session_host::process_start_time_ms(std::process::id()).is_some() {
            // Mark the home as owned by this process: launch must refuse.
            write_own_pid_file(
                &home,
                std::process::id(),
                crate::session_host::process_start_time_ms,
            )
            .unwrap();
            let err = launch_workspace(
                &workspace,
                Path::new("/bin/true"),
                &base,
                false,
                crate::session_host::process_start_time_ms,
            )
            .expect_err("must refuse while running");
            assert!(err.0.contains("already running"), "got: {err}");
            remove_own_pid_file(&home).unwrap();
        }

        // Nothing running: launch proceeds (spawns a no-op that exits at once).
        if Path::new("/bin/true").exists() {
            launch_workspace(
                &workspace,
                Path::new("/bin/true"),
                &base,
                false,
                crate::session_host::process_start_time_ms,
            )
            .expect("launch succeeds when idle");
        }

        std::fs::remove_dir_all(&home).ok();
    }
}
