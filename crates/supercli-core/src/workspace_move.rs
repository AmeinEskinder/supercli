//! Move a top-level project — and the sessions filed under it — from one
//! local workspace home to another.
//!
//! Port of
//! `clients/legacy/native/SupercliNative/Sources/SupercliNative/ProjectWorkspaceMove.swift`.
//! Workspaces are isolated `SUPERCLI_HOME` trees; this is a same-machine file
//! transfer of the shared on-disk contract, not a protocol verb.
//!
//! Sessions keep their ids. Live hosts keep running: a same-volume rename of
//! `app-sessions/<id>` leaves the PTY, output file descriptor, and session
//! socket inode intact, which is the same survival model as an app restart.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value};

use crate::app_state;
use crate::session_host::{process_start_time_ms, PidIdentity};

/// A local workspace the project context menu can file into.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkspaceMoveTarget {
    /// Normalized home path — stable identity across instances.
    pub id: String,
    pub name: String,
    /// Home path as the destination instance receives it (suite hashing and
    /// `selectLocalWorkspace` both want this spelling).
    pub home: String,
}

/// Outcome of [`move_project`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveOutcome {
    pub root_project_id: String,
    pub root_project_name: String,
    pub project_ids: HashSet<String>,
    pub session_ids: Vec<String>,
}

/// Failure modes of [`move_project`], with the user-facing copy the Swift
/// `MoveError.errorDescription` produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveError {
    SameHome,
    ProjectNotFound,
    DestAlreadyHasProject,
    DestAlreadyHasPath,
    DestAlreadyHasSession(String),
    DestStateWriteFailed,
    SourceStateWriteFailed,
    Io(String),
}

impl std::fmt::Display for MoveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveError::SameHome => write!(f, "That project is already in this workspace."),
            MoveError::ProjectNotFound => write!(f, "Couldn't find that project."),
            MoveError::DestAlreadyHasProject => {
                write!(f, "The other workspace already has this project.")
            }
            MoveError::DestAlreadyHasPath => {
                write!(
                    f,
                    "The other workspace already has a project at that folder."
                )
            }
            MoveError::DestAlreadyHasSession(_) => {
                write!(f, "The other workspace already has one of these sessions.")
            }
            MoveError::DestStateWriteFailed => {
                write!(f, "Couldn't update the destination workspace.")
            }
            MoveError::SourceStateWriteFailed => write!(
                f,
                "The project landed in the other workspace, but this workspace still lists it."
            ),
            MoveError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for MoveError {}

/// One row of the `projects` array in `app-state.json`, kept raw so keys this
/// build does not model survive the move.
#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub path: String,
    pub parent_project_id: Option<String>,
    pub raw: Value,
}

/// Disk-only transfer between two `SUPERCLI_HOME` trees. Does not touch
/// per-user overlays (the store copies those after a successful move). Live
/// session dirs are renamed in place — hosts are not stopped.
///
/// Ordering is fail-closed: sessions move first with rollback on failure,
/// then the destination state is updated (rollback on failure), and only
/// then is the source state stripped. A failure after the destination write
/// surfaces [`MoveError::SourceStateWriteFailed`] — the project is in the
/// destination but still listed in the source.
pub fn move_project(
    project_id: &str,
    source_home: &Path,
    dest_home: &Path,
) -> Result<MoveOutcome, MoveError> {
    let source = standardized_path(source_home);
    let dest = standardized_path(dest_home);
    if source == dest {
        return Err(MoveError::SameHome);
    }

    let source_state_path = source.join("app-state.json");
    let dest_state_path = dest.join("app-state.json");
    let source_projects = load_projects(&source_state_path);
    let root = source_projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or(MoveError::ProjectNotFound)?;
    let root_name = root.name.clone();
    let root_path = root.path.clone();

    let subtree_ids = descendant_project_ids(project_id, &source_projects);
    let moving_projects: Vec<&ProjectRow> = source_projects
        .iter()
        .filter(|p| subtree_ids.contains(&p.id))
        .collect();

    let dest_projects = load_projects(&dest_state_path);
    let dest_ids: HashSet<&str> = dest_projects.iter().map(|p| p.id.as_str()).collect();
    if dest_ids.contains(project_id) {
        return Err(MoveError::DestAlreadyHasProject);
    }
    let dest_paths: HashSet<String> = dest_projects
        .iter()
        .filter(|p| p.parent_project_id.is_none())
        .map(|p| normalized_path(&p.path))
        .collect();
    if dest_paths.contains(&normalized_path(&root_path)) {
        return Err(MoveError::DestAlreadyHasPath);
    }
    if moving_projects
        .iter()
        .any(|p| dest_ids.contains(p.id.as_str()))
    {
        return Err(MoveError::DestAlreadyHasProject);
    }

    let known_source_ids: HashSet<String> = source_projects.iter().map(|p| p.id.clone()).collect();
    let session_ids =
        collect_session_ids(&source, &subtree_ids, &known_source_ids).map_err(MoveError::Io)?;

    let dest_sessions_dir = dest.join("app-sessions");
    for session_id in &session_ids {
        if dest_sessions_dir.join(session_id).exists() {
            return Err(MoveError::DestAlreadyHasSession(session_id.clone()));
        }
    }

    std::fs::create_dir_all(&dest_sessions_dir)
        .map_err(|e| MoveError::Io(format!("create {}: {e}", dest_sessions_dir.display())))?;
    let source_sessions_dir = source.join("app-sessions");
    let mut moved: Vec<String> = Vec::new();
    let move_result: Result<(), MoveError> = (|| {
        for session_id in &session_ids {
            let src_dir = source_sessions_dir.join(session_id);
            if !src_dir.exists() {
                continue;
            }
            let dest_dir = dest_sessions_dir.join(session_id);
            std::fs::rename(&src_dir, &dest_dir)
                .map_err(|e| MoveError::Io(format!("rename {}: {e}", src_dir.display())))?;
            moved.push(session_id.clone());
        }
        Ok(())
    })();
    if let Err(e) = move_result {
        rollback_moved_sessions(&moved, &dest_sessions_dir, &source);
        return Err(e);
    }

    let source_orders = load_session_orders(&source);
    let source_project_order = load_project_order(&source);
    let source_state = load_json_object(&source_state_path);

    let dest_wrote = app_state::edit_at(&dest_state_path, |object| {
        merge_projects(&moving_projects, object);
        merge_pinned_sessions(&source_state, &subtree_ids, object);
        merge_string_map("session_sort_modes", &source_state, &subtree_ids, object);
        // Disk-carried folder colors (workspaces without a native overlay);
        // the per-user copy moves in the overlay transfer.
        merge_string_map("project_colors", &source_state, &subtree_ids, object);
        let session_id_set: HashSet<String> = session_ids.iter().cloned().collect();
        merge_mcp_orchestrators(&source_state, &session_id_set, object);
        merge_blocked_projects(&source_state, &subtree_ids, object);
        let active = object.get("active_project_id");
        if active.is_none() || active == Some(&Value::Null) {
            object.insert(
                "active_project_id".to_string(),
                Value::String(project_id.to_string()),
            );
        }
        Ok::<(), String>(())
    });
    if dest_wrote.is_err() {
        rollback_moved_sessions(&moved, &dest_sessions_dir, &source);
        return Err(MoveError::DestStateWriteFailed);
    }

    let mut adding: HashMap<String, Vec<String>> = HashMap::new();
    for id in &subtree_ids {
        if let Some(ids) = source_orders.get(id) {
            adding.insert(id.clone(), ids.clone());
        }
    }
    merge_session_orders(&dest, &adding);
    append_project_order(
        &dest,
        &ordered_project_ids(&subtree_ids, &source_project_order, project_id),
    );

    let source_wrote = app_state::edit_at(&source_state_path, |object| {
        strip_projects(&subtree_ids, object);
        strip_pinned_sessions(&subtree_ids, object);
        strip_string_map("session_sort_modes", &subtree_ids, object);
        strip_string_map("project_colors", &subtree_ids, object);
        let session_id_set: HashSet<String> = session_ids.iter().cloned().collect();
        strip_mcp_orchestrators(&session_id_set, object);
        strip_blocked_projects(&subtree_ids, object);
        let clear_active = object
            .get("active_project_id")
            .and_then(|v| v.as_str())
            .map(|id| subtree_ids.contains(id))
            .unwrap_or(false);
        if clear_active {
            object.insert("active_project_id".to_string(), Value::Null);
        }
        Ok::<(), String>(())
    });
    strip_session_orders(&source, &subtree_ids);
    strip_project_order(&source, &subtree_ids);
    if source_wrote.is_err() {
        return Err(MoveError::SourceStateWriteFailed);
    }

    Ok(MoveOutcome {
        root_project_id: project_id.to_string(),
        root_project_name: root_name,
        project_ids: subtree_ids,
        session_ids,
    })
}

/// Session ids that would move with `project_id` — used to stop hosts before
/// the transfer.
pub fn session_ids_for_project(project_id: &str, home: &Path) -> Vec<String> {
    let home = standardized_path(home);
    let projects = load_projects(&home.join("app-state.json"));
    if !projects.iter().any(|p| p.id == project_id) {
        return Vec::new();
    }
    let subtree = descendant_project_ids(project_id, &projects);
    let known: HashSet<String> = projects.iter().map(|p| p.id.clone()).collect();
    collect_session_ids(&home, &subtree, &known).unwrap_or_default()
}

/// The subset of [`session_ids_for_project`] whose manifests describe a live
/// session.
pub fn live_session_ids(project_id: &str, home: &Path) -> Vec<String> {
    session_ids_for_project(project_id, home)
        .into_iter()
        .filter(|id| session_is_live(home, id))
        .collect()
}

/// All project ids in the subtree rooted at `root_id`, including `root_id`.
/// Pure graph traversal over `(id, parent_id)` pairs.
pub fn descendant_project_ids(root_id: &str, projects: &[ProjectRow]) -> HashSet<String> {
    let parent_of: HashMap<&str, Option<&str>> = projects
        .iter()
        .map(|p| (p.id.as_str(), p.parent_project_id.as_deref()))
        .collect();
    let mut result = HashSet::new();
    let mut stack = vec![root_id.to_string()];
    result.insert(root_id.to_string());
    while let Some(current) = stack.pop() {
        for (id, parent) in &parent_of {
            if *parent == Some(current.as_str()) && !result.contains(*id) {
                result.insert((*id).to_string());
                stack.push((*id).to_string());
            }
        }
    }
    result
}

/// Whether the session's manifest describes a live session: state `running`
/// with a pid that is not provably recycled. Mirrors the Swift
/// `sessionIsLive` exactly, including its lenient manifest decode (older and
/// minimal manifests must not fail the read).
pub fn session_is_live(home: &Path, session_id: &str) -> bool {
    let manifest_path = standardized_path(home)
        .join("app-sessions")
        .join(session_id)
        .join("manifest.json");
    let data = match std::fs::read(&manifest_path) {
        Ok(d) => d,
        Err(_) => return false,
    };
    let manifest: Value = match serde_json::from_slice(&data) {
        Ok(v) => v,
        Err(_) => return false,
    };
    if manifest.get("state").and_then(|v| v.as_str()) != Some("running") {
        return false;
    }
    let pid = manifest
        .get("pid")
        .and_then(|v| v.as_u64())
        .map(|p| p as u32);
    if hosted_child_process_exists(pid) == Some(false) {
        return false;
    }
    let pid_started_at = manifest.get("pid_started_at").and_then(|v| v.as_u64());
    let session_id_in_manifest = manifest
        .get("session")
        .and_then(|s| s.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    manifest_pid_identity_lenient(pid, pid_started_at, session_id_in_manifest)
        != PidIdentity::NotOurs
}

/// `kill(pid, 0)` existence probe with the Swift tri-state: `None` when the
/// pid is absent/invalid or the kernel query is inconclusive, `Some(true)`
/// when the process exists (or we lack permission to signal it, which still
/// proves existence), `Some(false)` only on `ESRCH`.
fn hosted_child_process_exists(pid: Option<u32>) -> Option<bool> {
    let pid = pid?;
    if pid <= 1 {
        return None;
    }
    #[cfg(unix)]
    {
        let rc = unsafe { libc::kill(pid as i32, 0) };
        if rc == 0 {
            return Some(true);
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(code) if code == libc::EPERM => Some(true),
            Some(code) if code == libc::ESRCH => Some(false),
            _ => None,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        None
    }
}

/// [`crate::session_host::manifest_pid_identity`] over a leniently-decoded
/// manifest: only `pid`, `pid_started_at`, and `session.id` are read, so
/// minimal and legacy manifests decode instead of failing the liveness read.
fn manifest_pid_identity_lenient(
    pid: Option<u32>,
    pid_started_at: Option<u64>,
    session_id: &str,
) -> PidIdentity {
    const PID_START_TOLERANCE_MS: u64 = 10_000;
    let Some(pid) = pid else {
        return PidIdentity::Unknown;
    };
    if let (Some(recorded), Some(actual)) = (pid_started_at, process_start_time_ms(pid)) {
        return if actual.abs_diff(recorded) <= PID_START_TOLERANCE_MS {
            PidIdentity::Matches
        } else {
            PidIdentity::NotOurs
        };
    }
    match process_argv_mentions(pid, session_id) {
        Some(true) => PidIdentity::Matches,
        _ => PidIdentity::Unknown,
    }
}

/// Whether `ps` reports a command line for `pid` containing `needle`.
/// `None` when the kernel query fails or reports nothing.
fn process_argv_mentions(pid: u32, needle: &str) -> Option<bool> {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let command = String::from_utf8_lossy(&output.stdout);
    if command.trim().is_empty() {
        return None;
    }
    Some(command.contains(needle))
}

// MARK: - Discovery

fn load_projects(state_path: &Path) -> Vec<ProjectRow> {
    let object = load_json_object(state_path);
    let empty = Vec::new();
    let rows = object
        .get("projects")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    rows.iter()
        .filter_map(|raw| {
            let id = raw.get("id")?.as_str()?.to_string();
            let name = raw.get("name")?.as_str()?.to_string();
            let path = raw.get("path")?.as_str()?.to_string();
            let parent_project_id = raw
                .get("parent_project_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            Some(ProjectRow {
                id,
                name,
                path,
                parent_project_id,
                raw: raw.clone(),
            })
        })
        .collect()
}

fn load_json_object(path: &Path) -> Map<String, Value> {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return Map::new(),
    };
    serde_json::from_slice::<Value>(&data)
        .ok()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// Session ids filed under `subtree_ids`, sorted. A session belongs to the
/// subtree when its bucket — the `project-override.json` target when valid,
/// else the manifest's project — is in the subtree. Mirrors
/// `SupercliStore.rebuildTree`/`removeProject` bucket keying: a valid
/// override target wins over the manifest project.
fn collect_session_ids(
    home: &Path,
    subtree_ids: &HashSet<String>,
    known_project_ids: &HashSet<String>,
) -> Result<Vec<String>, String> {
    let sessions_dir = home.join("app-sessions");
    let entries = match std::fs::read_dir(&sessions_dir) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };
    let mut ids = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        // Swift skips hidden files (.skipsHiddenFiles) and non-directories.
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) if !n.starts_with('.') => n.to_string(),
            _ => continue,
        };
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if !is_dir {
            continue;
        }
        match session_bucket(&path, known_project_ids) {
            Some(bucket) if subtree_ids.contains(&bucket) => ids.push(name),
            _ => {}
        }
    }
    ids.sort();
    Ok(ids)
}

fn session_bucket(dir: &Path, known_project_ids: &HashSet<String>) -> Option<String> {
    if let Ok(data) = std::fs::read(dir.join("project-override.json")) {
        if let Ok(Value::Object(obj)) = serde_json::from_slice::<Value>(&data) {
            if let Some(Value::String(override_id)) = obj.get("project_id") {
                if known_project_ids.contains(override_id) {
                    return Some(override_id.clone());
                }
            }
        }
    }
    let data = std::fs::read(dir.join("manifest.json")).ok()?;
    let manifest: Value = serde_json::from_slice(&data).ok()?;
    manifest
        .get("session")?
        .get("project_id")?
        .as_str()
        .map(|s| s.to_string())
}

// MARK: - JSON merge/strip helpers (raw Value level, like the Swift code)

fn merge_projects(rows: &[&ProjectRow], object: &mut Map<String, Value>) {
    let projects = object
        .entry("projects".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Value::Array(arr) = projects {
        let existing: HashSet<String> = arr
            .iter()
            .filter_map(|v| v.get("id")?.as_str().map(|s| s.to_string()))
            .collect();
        for row in rows {
            if !existing.contains(&row.id) {
                arr.push(row.raw.clone());
            }
        }
    }
}

fn strip_projects(ids: &HashSet<String>, object: &mut Map<String, Value>) {
    if let Some(Value::Array(arr)) = object.get_mut("projects") {
        arr.retain(|v| {
            v.get("id")
                .and_then(|id| id.as_str())
                .map(|id| !ids.contains(id))
                .unwrap_or(true)
        });
    }
}

/// `pinned_sessions` is either grouped `{project_id: [pin]}` or a flat `[pin]`
/// array (legacy). Returns the grouped form in both cases.
fn grouped_pins(raw: Option<&Value>) -> HashMap<String, Vec<Value>> {
    let mut result: HashMap<String, Vec<Value>> = HashMap::new();
    match raw {
        Some(Value::Object(map)) => {
            for (key, value) in map {
                if let Value::Array(arr) = value {
                    result.insert(
                        key.clone(),
                        arr.iter().filter(|v| v.is_object()).cloned().collect(),
                    );
                }
            }
        }
        Some(Value::Array(rows)) => {
            for row in rows {
                if let Some(pid) = row.get("project_id").and_then(|v| v.as_str()) {
                    if !pid.is_empty() {
                        result.entry(pid.to_string()).or_default().push(row.clone());
                    }
                }
            }
        }
        _ => {}
    }
    result
}

fn merge_pinned_sessions(
    source: &Map<String, Value>,
    project_ids: &HashSet<String>,
    dest: &mut Map<String, Value>,
) {
    let mut dest_pins = grouped_pins(dest.get("pinned_sessions"));
    let source_pins = grouped_pins(source.get("pinned_sessions"));
    for id in project_ids {
        if let Some(rows) = source_pins.get(id) {
            if !rows.is_empty() {
                dest_pins.insert(id.clone(), rows.clone());
            }
        }
    }
    dest.insert(
        "pinned_sessions".to_string(),
        dest_pins
            .into_iter()
            .map(|(k, v)| (k, Value::Array(v)))
            .collect::<Map<String, Value>>()
            .into(),
    );
}

fn strip_pinned_sessions(project_ids: &HashSet<String>, object: &mut Map<String, Value>) {
    let mut pins = grouped_pins(object.get("pinned_sessions"));
    for id in project_ids {
        pins.remove(id);
    }
    object.insert(
        "pinned_sessions".to_string(),
        pins.into_iter()
            .map(|(k, v)| (k, Value::Array(v)))
            .collect::<Map<String, Value>>()
            .into(),
    );
}

fn merge_string_map(
    key: &str,
    source: &Map<String, Value>,
    keys: &HashSet<String>,
    dest: &mut Map<String, Value>,
) {
    let mut map = dest
        .get(key)
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    let source_map = source
        .get(key)
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    for id in keys {
        if let Some(value) = source_map.get(id) {
            map.insert(id.clone(), value.clone());
        }
    }
    dest.insert(key.to_string(), Value::Object(map));
}

fn strip_string_map(key: &str, keys: &HashSet<String>, object: &mut Map<String, Value>) {
    let mut map = object
        .get(key)
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    for id in keys {
        map.remove(id);
    }
    object.insert(key.to_string(), Value::Object(map));
}

fn merge_mcp_orchestrators(
    source: &Map<String, Value>,
    session_ids: &HashSet<String>,
    dest: &mut Map<String, Value>,
) {
    let mut map = dest
        .get("mcp_orchestrators")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    let source_map = source
        .get("mcp_orchestrators")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    for id in session_ids {
        if let Some(value) = source_map.get(id) {
            map.insert(id.clone(), value.clone());
        }
    }
    dest.insert("mcp_orchestrators".to_string(), Value::Object(map));
}

fn strip_mcp_orchestrators(session_ids: &HashSet<String>, object: &mut Map<String, Value>) {
    let mut map = object
        .get("mcp_orchestrators")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    for id in session_ids {
        map.remove(id);
    }
    object.insert("mcp_orchestrators".to_string(), Value::Object(map));
}

fn merge_blocked_projects(
    source: &Map<String, Value>,
    project_ids: &HashSet<String>,
    dest: &mut Map<String, Value>,
) {
    let mut ids: Vec<String> = dest
        .get("mcp_blocked_projects")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let source_ids: HashSet<String> = source
        .get("mcp_blocked_projects")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    for id in project_ids {
        if source_ids.contains(id) && !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    dest.insert(
        "mcp_blocked_projects".to_string(),
        Value::Array(ids.into_iter().map(Value::String).collect()),
    );
}

fn strip_blocked_projects(project_ids: &HashSet<String>, object: &mut Map<String, Value>) {
    let ids: Vec<Value> = object
        .get("mcp_blocked_projects")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|v| {
                    v.as_str()
                        .map(|id| !project_ids.contains(id))
                        .unwrap_or(true)
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    object.insert("mcp_blocked_projects".to_string(), Value::Array(ids));
}

// MARK: - Order files

fn load_session_orders(home: &Path) -> HashMap<String, Vec<String>> {
    let data = match std::fs::read(home.join("session-order.json")) {
        Ok(d) => d,
        Err(_) => return HashMap::new(),
    };
    let value: Value = match serde_json::from_slice(&data) {
        Ok(v) => v,
        Err(_) => return HashMap::new(),
    };
    let mut result = HashMap::new();
    if let Value::Object(map) = value {
        for (key, value) in map {
            // Swift `as? [String]` requires every element to be a String.
            if let Value::Array(arr) = value {
                let mut ids = Vec::with_capacity(arr.len());
                let mut all_strings = true;
                for item in &arr {
                    match item.as_str() {
                        Some(s) => ids.push(s.to_string()),
                        None => {
                            all_strings = false;
                            break;
                        }
                    }
                }
                if all_strings {
                    result.insert(key, ids);
                }
            }
        }
    }
    result
}

fn load_project_order(home: &Path) -> Vec<String> {
    let data = match std::fs::read(home.join("project-order.json")) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    serde_json::from_slice::<Value>(&data)
        .ok()
        .and_then(|v| {
            v.as_array().and_then(|arr| {
                let mut ids = Vec::with_capacity(arr.len());
                for item in arr {
                    ids.push(item.as_str()?.to_string());
                }
                Some(ids)
            })
        })
        .unwrap_or_default()
}

fn merge_session_orders(home: &Path, adding: &HashMap<String, Vec<String>>) {
    if adding.is_empty() {
        return;
    }
    let url = home.join("session-order.json");
    let Ok(_lock) = app_state::lock_exclusive(&url) else {
        return;
    };
    let mut root = load_session_orders(home);
    for (project_id, ids) in adding {
        if !ids.is_empty() {
            root.insert(project_id.clone(), ids.clone());
        }
    }
    if let Ok(value) = serde_json::to_value(&root) {
        write_json_atomic(&url, &value);
    }
}

fn strip_session_orders(home: &Path, project_ids: &HashSet<String>) {
    let url = home.join("session-order.json");
    let Ok(_lock) = app_state::lock_exclusive(&url) else {
        return;
    };
    let mut root = load_session_orders(home);
    for id in project_ids {
        root.remove(id);
    }
    if let Ok(value) = serde_json::to_value(&root) {
        write_json_atomic(&url, &value);
    }
}

fn append_project_order(home: &Path, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    let url = home.join("project-order.json");
    let Ok(_lock) = app_state::lock_exclusive(&url) else {
        return;
    };
    let mut order = load_project_order(home);
    for id in ids {
        if !order.contains(id) {
            order.push(id.clone());
        }
    }
    if let Ok(value) = serde_json::to_value(&order) {
        write_json_atomic(&url, &value);
    }
}

fn strip_project_order(home: &Path, ids: &HashSet<String>) {
    let url = home.join("project-order.json");
    let Ok(_lock) = app_state::lock_exclusive(&url) else {
        return;
    };
    let order: Vec<String> = load_project_order(home)
        .into_iter()
        .filter(|id| !ids.contains(id))
        .collect();
    if let Ok(value) = serde_json::to_value(&order) {
        write_json_atomic(&url, &value);
    }
}

/// Source order filtered to the subtree, root first, then any subtree ids
/// missing from the source order in sorted order.
fn ordered_project_ids(
    subtree_ids: &HashSet<String>,
    source_order: &[String],
    root_id: &str,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for id in source_order {
        if subtree_ids.contains(id) && seen.insert(id.clone()) {
            ids.push(id.clone());
        }
    }
    if seen.insert(root_id.to_string()) {
        ids.insert(0, root_id.to_string());
    }
    let mut rest: Vec<&String> = subtree_ids
        .iter()
        .filter(|id| seen.insert((*id).clone()))
        .collect();
    rest.sort();
    for id in rest {
        ids.push(id.clone());
    }
    ids
}

/// Atomic write via temp file + rename (Swift `.atomic` write).
fn write_json_atomic(path: &Path, value: &Value) {
    let data = match serde_json::to_vec(value) {
        Ok(d) => d,
        Err(_) => return,
    };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, &data).is_err() {
        return;
    }
    let _ = std::fs::rename(&tmp, path);
}

fn rollback_moved_sessions(moved: &[String], dest_sessions_dir: &Path, source_home: &Path) {
    let source_sessions = source_home.join("app-sessions");
    let _ = std::fs::create_dir_all(&source_sessions);
    for session_id in moved {
        let dest_dir = dest_sessions_dir.join(session_id);
        let src_dir = source_sessions.join(session_id);
        if dest_dir.exists() && !src_dir.exists() {
            let _ = std::fs::rename(&dest_dir, &src_dir);
        }
    }
}

/// Lexical path normalization (`..`/`.` resolved without touching the
/// filesystem), like Swift's `standardizedFileURL`.
fn standardized_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            _ => out.push(comp.as_os_str()),
        }
    }
    out
}

/// Symlink-resolving normalization, like Swift's
/// `resolvingSymlinksInPath().path`. Falls back to the input when the path
/// does not exist.
fn normalized_path(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Scratch workspace-home pair, cleaned up on drop. Uses `tempfile` (a
    /// dev-dependency) so parallel tests never share a directory.
    struct Homes {
        _dir: tempfile::TempDir,
        source: PathBuf,
        dest: PathBuf,
    }

    impl Homes {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("source");
            let dest = dir.path().join("dest");
            fs::create_dir_all(&source).unwrap();
            fs::create_dir_all(&dest).unwrap();
            Self {
                _dir: dir,
                source,
                dest,
            }
        }
    }

    fn project(id: &str, name: &str, path: &str, parent: Option<&str>, folder: bool) -> Value {
        let mut row = serde_json::json!({"id": id, "name": name, "path": path});
        if let Some(parent) = parent {
            row["parent_project_id"] = Value::String(parent.to_string());
        }
        if folder {
            row["is_folder"] = Value::Bool(true);
        }
        row
    }

    fn pin(session_id: &str, project_id: &str) -> Value {
        serde_json::json!({
            "key": format!("session:{session_id}"),
            "project_id": project_id,
            "session_id": session_id,
            "pinned_at": 1,
        })
    }

    fn write_app_state(home: &Path, object: &Value) {
        fs::create_dir_all(home).unwrap();
        let data = serde_json::to_vec_pretty(object).unwrap();
        fs::write(home.join("app-state.json"), data).unwrap();
    }

    fn write_session(
        home: &Path,
        id: &str,
        project_id: &str,
        override_project: Option<&str>,
        state: &str,
        pid: Option<u32>,
    ) {
        let dir = home.join("app-sessions").join(id);
        fs::create_dir_all(&dir).unwrap();
        let mut manifest = serde_json::json!({
            "session": {"id": id, "project_id": project_id},
            "state": state,
            "updated_at": 1,
        });
        if let Some(pid) = pid {
            manifest["pid"] = Value::from(pid);
        }
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        if let Some(override_project) = override_project {
            let data = serde_json::to_vec_pretty(&serde_json::json!({
                "project_id": override_project, "moved_at": 1,
            }))
            .unwrap();
            fs::write(dir.join("project-override.json"), data).unwrap();
        }
    }

    fn session_exists(home: &Path, id: &str) -> bool {
        home.join("app-sessions").join(id).exists()
    }

    fn load_object(path: &Path) -> Map<String, Value> {
        let data = fs::read(path).unwrap();
        serde_json::from_slice::<Value>(&data)
            .unwrap()
            .as_object()
            .unwrap()
            .clone()
    }

    fn load_array(path: &Path) -> Vec<String> {
        let data = fs::read(path).unwrap();
        serde_json::from_slice::<Value>(&data)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    }

    fn write_json_file(path: &Path, value: &Value) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }

    /// Port of `testMovesProjectSessionsPinsAndOrdersAndLeavesNeighbors`.
    #[test]
    fn moves_project_sessions_pins_and_orders_and_leaves_neighbors() {
        let homes = Homes::new();
        write_app_state(
            &homes.source,
            &serde_json::json!({
                "projects": [
                    project("keep", "Stay", "/tmp/stay", None, false),
                    project("move", "Flatsome", "/tmp/flatsome", None, false),
                    project("group", "Research", "/tmp/flatsome", Some("move"), true),
                ],
                "pinned_sessions": {
                    "move": [pin("sess-move", "move")],
                    "keep": [pin("sess-keep", "keep")],
                },
                "session_sort_modes": {"move": "date", "keep": "date"},
                "mcp_orchestrators": {
                    "sess-move": {"role": "write", "reach": "project"},
                    "sess-keep": {"role": "read", "reach": "project"},
                },
                "mcp_blocked_projects": ["move", "keep"],
                "presets": [],
            }),
        );
        write_app_state(
            &homes.dest,
            &serde_json::json!({
                "projects": [project("other", "Other", "/tmp/other", None, false)],
                "pinned_sessions": {},
                "presets": [],
            }),
        );
        write_session(&homes.source, "sess-move", "move", None, "exited", None);
        write_session(
            &homes.source,
            "sess-group",
            "move",
            Some("group"),
            "exited",
            None,
        );
        write_session(&homes.source, "sess-keep", "keep", None, "exited", None);
        write_json_file(
            &homes.source.join("session-order.json"),
            &serde_json::json!({"move": ["sess-move"], "keep": ["sess-keep"], "group": ["sess-group"]}),
        );
        write_json_file(
            &homes.source.join("project-order.json"),
            &serde_json::json!(["keep", "move", "group"]),
        );
        write_json_file(
            &homes.dest.join("project-order.json"),
            &serde_json::json!(["other"]),
        );

        let outcome = move_project("move", &homes.source, &homes.dest).unwrap();

        assert_eq!(outcome.root_project_name, "Flatsome");
        assert_eq!(outcome.root_project_id, "move");
        assert_eq!(
            outcome.project_ids,
            HashSet::from(["move".to_string(), "group".to_string()])
        );
        assert_eq!(outcome.session_ids, vec!["sess-group", "sess-move"]);

        let dest_state = load_object(&homes.dest.join("app-state.json"));
        let dest_projects: HashSet<String> = dest_state["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            dest_projects,
            HashSet::from(["other".to_string(), "move".to_string(), "group".to_string()])
        );
        let dest_pins = dest_state["pinned_sessions"].as_object().unwrap();
        assert!(dest_pins.contains_key("move"));
        assert!(!dest_pins.contains_key("keep"));
        assert_eq!(
            dest_state["session_sort_modes"]["move"],
            Value::String("date".to_string())
        );
        let dest_grants = dest_state["mcp_orchestrators"].as_object().unwrap();
        assert!(dest_grants.contains_key("sess-move"));
        assert!(!dest_grants.contains_key("sess-keep"));
        assert_eq!(
            dest_state["mcp_blocked_projects"],
            serde_json::json!(["move"])
        );

        let source_state = load_object(&homes.source.join("app-state.json"));
        let source_projects: Vec<String> = source_state["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(source_projects, vec!["keep"]);
        let source_pins = source_state["pinned_sessions"].as_object().unwrap();
        assert!(source_pins.contains_key("keep"));
        assert!(!source_pins.contains_key("move"));
        assert_eq!(
            source_state["mcp_blocked_projects"],
            serde_json::json!(["keep"])
        );

        assert!(session_exists(&homes.dest, "sess-move"));
        assert!(session_exists(&homes.dest, "sess-group"));
        assert!(!session_exists(&homes.source, "sess-move"));
        assert!(session_exists(&homes.source, "sess-keep"));

        let dest_orders = load_object(&homes.dest.join("session-order.json"));
        assert_eq!(dest_orders["move"], serde_json::json!(["sess-move"]));
        assert_eq!(dest_orders["group"], serde_json::json!(["sess-group"]));
        assert_eq!(
            load_array(&homes.dest.join("project-order.json")),
            vec!["other", "move", "group"]
        );
    }

    /// Port of `testLeavesSessionFiledOutOfTheSubtree`.
    #[test]
    fn leaves_session_filed_out_of_the_subtree() {
        let homes = Homes::new();
        write_app_state(
            &homes.source,
            &serde_json::json!({
                "projects": [
                    project("move", "A", "/tmp/a", None, false),
                    project("other", "B", "/tmp/b", None, false),
                ],
                "presets": [],
            }),
        );
        write_app_state(
            &homes.dest,
            &serde_json::json!({"projects": [], "presets": []}),
        );
        // Filed under "other" via a valid override — not part of the move.
        write_session(
            &homes.source,
            "launched-here",
            "move",
            Some("other"),
            "exited",
            None,
        );
        write_session(&homes.source, "stays-with-a", "move", None, "exited", None);

        let outcome = move_project("move", &homes.source, &homes.dest).unwrap();

        assert_eq!(outcome.session_ids, vec!["stays-with-a"]);
        assert!(session_exists(&homes.dest, "stays-with-a"));
        assert!(session_exists(&homes.source, "launched-here"));
    }

    /// Port of `testRefusesDestPathCollisionAndLeavesSourceIntact`.
    #[test]
    fn refuses_dest_path_collision_and_leaves_source_intact() {
        let homes = Homes::new();
        write_app_state(
            &homes.source,
            &serde_json::json!({
                "projects": [project("move", "A", "/tmp/same", None, false)],
                "presets": [],
            }),
        );
        write_app_state(
            &homes.dest,
            &serde_json::json!({
                "projects": [project("existing", "Existing", "/tmp/same", None, false)],
                "presets": [],
            }),
        );
        write_session(&homes.source, "sess", "move", None, "exited", None);

        let err = move_project("move", &homes.source, &homes.dest).unwrap_err();
        assert_eq!(err, MoveError::DestAlreadyHasPath);
        assert!(session_exists(&homes.source, "sess"));
        assert!(!session_exists(&homes.dest, "sess"));
        let source_state = load_object(&homes.source.join("app-state.json"));
        let ids: Vec<String> = source_state["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids, vec!["move"]);
    }

    /// Port of `testRefusesDestSessionIdCollision`.
    #[test]
    fn refuses_dest_session_id_collision() {
        let homes = Homes::new();
        write_app_state(
            &homes.source,
            &serde_json::json!({
                "projects": [project("move", "A", "/tmp/a", None, false)],
                "presets": [],
            }),
        );
        write_app_state(
            &homes.dest,
            &serde_json::json!({
                "projects": [project("other", "B", "/tmp/b", None, false)],
                "presets": [],
            }),
        );
        write_session(&homes.source, "shared", "move", None, "exited", None);
        write_session(&homes.dest, "shared", "other", None, "exited", None);

        let err = move_project("move", &homes.source, &homes.dest).unwrap_err();
        assert_eq!(err, MoveError::DestAlreadyHasSession("shared".to_string()));
        assert!(session_exists(&homes.source, "shared"));
    }

    /// Port of `testMovesRunningSessionDirWithoutStopping`.
    #[test]
    fn moves_running_session_dir_without_stopping() {
        let homes = Homes::new();
        write_app_state(
            &homes.source,
            &serde_json::json!({
                "projects": [project("move", "A", "/tmp/a", None, false)],
                "presets": [],
            }),
        );
        write_app_state(
            &homes.dest,
            &serde_json::json!({"projects": [], "presets": []}),
        );
        // The test process's own pid: provably alive, argv does not mention
        // the session id, so identity is Unknown (never NotOurs) — live.
        write_session(
            &homes.source,
            "live",
            "move",
            None,
            "running",
            Some(std::process::id()),
        );

        assert_eq!(live_session_ids("move", &homes.source), vec!["live"]);
        move_project("move", &homes.source, &homes.dest).unwrap();
        assert!(!session_exists(&homes.source, "live"));
        assert!(session_exists(&homes.dest, "live"));
        let manifest = load_object(
            &homes
                .dest
                .join("app-sessions")
                .join("live")
                .join("manifest.json"),
        );
        assert_eq!(manifest["state"], Value::String("running".to_string()));
    }

    /// Port of `testCollectsChildGroupsAndWorktrees`.
    #[test]
    fn collects_child_groups_and_worktrees() {
        let projects = vec![
            ProjectRow {
                id: "root".to_string(),
                name: "R".to_string(),
                path: "/r".to_string(),
                parent_project_id: None,
                raw: Value::Null,
            },
            ProjectRow {
                id: "group".to_string(),
                name: "G".to_string(),
                path: "/r".to_string(),
                parent_project_id: Some("root".to_string()),
                raw: Value::Null,
            },
            ProjectRow {
                id: "wt".to_string(),
                name: "W".to_string(),
                path: "/r-wt".to_string(),
                parent_project_id: Some("root".to_string()),
                raw: Value::Null,
            },
            ProjectRow {
                id: "other".to_string(),
                name: "O".to_string(),
                path: "/o".to_string(),
                parent_project_id: None,
                raw: Value::Null,
            },
        ];
        assert_eq!(
            descendant_project_ids("root", &projects),
            HashSet::from(["root".to_string(), "group".to_string(), "wt".to_string()])
        );
    }

    /// `sameHome` refusal.
    #[test]
    fn refuses_same_home() {
        let homes = Homes::new();
        let err = move_project("move", &homes.source, &homes.source).unwrap_err();
        assert_eq!(err, MoveError::SameHome);
    }

    /// Unknown project id.
    #[test]
    fn refuses_unknown_project() {
        let homes = Homes::new();
        write_app_state(
            &homes.source,
            &serde_json::json!({"projects": [], "presets": []}),
        );
        write_app_state(
            &homes.dest,
            &serde_json::json!({"projects": [], "presets": []}),
        );
        let err = move_project("nope", &homes.source, &homes.dest).unwrap_err();
        assert_eq!(err, MoveError::ProjectNotFound);
    }
}
