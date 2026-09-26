//! Sharded grant storage (S2), with group commit and audit (Phase 13 v2).
//!
//! `persist_grant` was serializing all concurrent approvals on the
//! app-state.json exclusive lock (load entire JSON + mutate + save entire
//! JSON under flock). Grants get their own file (`grants.json`) with its own
//! lock, plus a tamper-evident audit log (`grant-audit.jsonl`, see
//! `grant_audit.rs`). This shards the contention: grant persists no longer
//! block non-grant app-state edits, and the critical section is smaller
//! (grants file is tiny vs. full app-state).
//!
//! ## Security: crash safety and write order (Phase 13 v2)
//!
//! A grant is a *remembered approval* — it lets future identical requests
//! skip the user prompt. It is NOT the authorization for the current request.
//!
//! Write order (enforced by `grant_writer`, the sole production writer):
//! 1. User approves via phone → `hub.answer()` wakes the MCP thread.
//! 2. `hub.request()` returns `ok=true` (explicit user approval).
//! 3. `grant_writer` appends a `grant_created` audit entry (actor, scope,
//!    tool, grant key) to `grant-audit.jsonl` and fsyncs — BEFORE the grant
//!    is written. With group commit, N concurrent grants share one audit
//!    fsync; each caller is acked only after its batch is durable.
//! 4. The writer applies all batched mutations and writes `grants.json`
//!    once (temp + fsync + rename + fsync dir, under the grants lock).
//! 5. MCP returns success to the caller.
//!
//! Crash safety argument:
//! - Crash before (3): The current request was user-approved, but nothing
//!   was persisted. On restart, the next identical request re-prompts the
//!   user. Safe: fail closed, no privilege escalation.
//! - Crash between (3) and (4): The audit entry is durable but the grant is
//!   not. On startup, reconciliation leaves the audit entry revoked — it is
//!   NOT silently re-created. The user re-approves. Safe: fail closed.
//! - Crash during (4): The temp+rename is atomic. The directory fsync makes
//!   the rename durable. A reader sees either the old file or the new file,
//!   never a half-written grant. Safe.
//! - Crash after (4): Grant and audit entry are both durable. Safe.
//!
//! Startup reconciliation (`grant_audit::reconcile_grants`, run by both the
//! supervisor and the worker): a grant WITHOUT an audit entry is
//! quarantined to `grants.json.quarantined` and reported (tamper-evidence
//! violation); an audit entry without a grant stays revoked. `supercli doctor`
//! verifies `grants ⊆ chain` at any time.
//!
//! A grant can NEVER be 'applied' without the user's explicit approval,
//! because `persist_grant` is only called after `hub.request()` returns
//! `ok=true`. The review-log write-ahead (with hash chain) is for tool
//! execution, not for grants — they are separate concerns; the grant audit
//! chain covers grant creation.
//!
//! ## Backward compatibility
//!
//! `already_granted` checks both the new `grants.json` and the legacy
//! `app-state.json` locations, so pre-S2 grants continue to work. Legacy
//! grants predate the audit log: on first startup with reconciliation they
//! are quarantined (grant without chain entry) — the user re-approves once
//! and the new grant is audited. This is intentional fail-closed behavior.

use serde_json::{Map, Value};
use std::path::PathBuf;

/// Get the grants file path.
fn grants_path() -> PathBuf {
    crate::app_paths::grants_path()
}

/// Acquire an exclusive lock on the grants file.
fn lock_grants_at(home: &std::path::Path) -> Result<crate::app_state::FileLock, String> {
    let path = home.join("grants.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::app_state::lock_exclusive(&path)
}

/// Load the grants file, or return an empty object if missing/corrupt.
fn load_grants() -> Map<String, Value> {
    std::fs::read(grants_path())
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// Edit the grants file with minimal lock hold time (S2 optimization).
///
/// Optimistic concurrency control:
/// 1. OUTSIDE lock: load file, apply mutation, serialize to bytes.
/// 2. Acquire exclusive lock.
/// 3. Re-read file; if unchanged since step 1, write the pre-serialized bytes.
/// 4. If changed, release lock and retry (another writer won the race).
///
/// The lock is held only for: re-read + compare + write + fsync(temp) +
/// rename + fsync(dir). JSON parsing and serialization (the CPU-heavy work)
/// happen outside the lock.
///
/// Crash safety is preserved: temp + fsync(temp) + rename + fsync(dir) all
/// happen under the lock, so readers see atomic updates.
pub fn edit_grants<T>(
    mutate: impl Fn(&mut Map<String, Value>) -> Result<T, String>,
) -> Result<T, String> {
    edit_grants_at(&crate::app_paths::supercli_home(), mutate)
}

/// Same as `edit_grants` but with an explicit home directory,
/// for tests that must not mutate the process-global SUPERCLI_HOME env var.
pub fn edit_grants_at<T>(
    home: &std::path::Path,
    mutate: impl Fn(&mut Map<String, Value>) -> Result<T, String>,
) -> Result<T, String> {
    let grants_path = home.join("grants.json");
    // Retry loop for optimistic concurrency
    loop {
        // OUTSIDE lock: load, mutate, serialize
        let original_bytes = std::fs::read(&grants_path).unwrap_or_default();
        let mut map = serde_json::from_slice::<Value>(&original_bytes)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let outcome = mutate(&mut map)?;
        let new_bytes =
            serde_json::to_vec_pretty(&Value::Object(map)).map_err(|e| e.to_string())?;

        // Acquire lock
        let _lock = lock_grants_at(home)?;

        // Re-read under lock; check for conflicts
        let current_bytes = std::fs::read(&grants_path).unwrap_or_default();
        if current_bytes == original_bytes {
            // No conflict: write the pre-serialized bytes
            write_grants_bytes_at(home, &new_bytes)?;
            return Ok(outcome);
        }
        // Conflict: another writer changed the file. Retry.
        // (Lock is released here via _lock drop)
    }
}

/// Write pre-serialized grants bytes with crash safety.
/// Caller must hold the grants lock.
fn write_grants_bytes_at(home: &std::path::Path, body: &[u8]) -> Result<(), String> {
    let path = home.join("grants.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.supercli-tmp");
    {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        use std::io::Write;
        f.write_all(body).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| format!("fsync temp: {e}"))?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        let dir = std::fs::File::open(parent).map_err(|e| e.to_string())?;
        dir.sync_all().map_err(|e| format!("fsync dir: {e}"))?;
    }
    Ok(())
}

/// Check if a grant exists in the sharded grants file.
pub fn grant_exists(key: &str, caller: &str, target: Option<&str>) -> bool {
    let map = load_grants();
    match key {
        "mcp_write_approvals" | "mcp_app_open_approvals" => {
            let Some(t) = target else { return false };
            map.get(key)
                .and_then(|m| m.get(caller))
                .and_then(|l| l.as_array())
                .is_some_and(|l| l.iter().any(|v| v.as_str() == Some(t)))
        }
        "browser_approvals" | "computer_approvals" => map
            .get(key)
            .and_then(|l| l.as_array())
            .is_some_and(|l| l.iter().any(|v| v.as_str() == Some(caller))),
        "mcp_connector_approvals" => {
            // Handled by connector_grant_exists which has its own logic
            false
        }
        _ => false,
    }
}

#[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;

    fn test_home() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "grant-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u32
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn persist_test_grant(caller: &str, target: &str) {
        edit_grants(|map| {
            let key = "mcp_write_approvals";
            let entry = map
                .entry(key.to_string())
                .or_insert(Value::Object(Map::new()));
            if let Value::Object(obj) = entry {
                let caller_entry = obj
                    .entry(caller.to_string())
                    .or_insert(Value::Array(vec![]));
                if let Value::Array(arr) = caller_entry {
                    if !arr.iter().any(|v| v.as_str() == Some(target)) {
                        arr.push(Value::String(target.to_string()));
                    }
                }
            }
            Ok::<(), String>(())
        })
        .unwrap();
    }

    /// Serialize SUPERCLI_HOME mutation against all other tests that touch it.
    /// Returns the home dir and holds the lock via the returned guard.
    fn locked_test_home() -> (PathBuf, std::sync::MutexGuard<'static, ()>) {
        let guard = crate::app_paths::TEST_SUPERCLI_HOME_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let home = test_home();
        std::env::set_var("SUPERCLI_HOME", &home);
        (home, guard)
    }

    /// Concurrent writers: N threads each persist a grant. All must succeed,
    /// and the final file must contain all grants (no lost updates).
    #[test]
    fn concurrent_writers_no_lost_updates() {
        let (home, _guard) = locked_test_home();

        let n_threads = 8;
        let barrier = Arc::new(Barrier::new(n_threads));
        let mut handles = vec![];

        for i in 0..n_threads {
            let b = barrier.clone();
            handles.push(thread::spawn(move || {
                b.wait();
                let caller = format!("session-{}", i);
                let target = format!("target-{}", i);
                persist_test_grant(&caller, &target);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        for i in 0..n_threads {
            let caller = format!("session-{}", i);
            let target = format!("target-{}", i);
            assert!(
                grant_exists("mcp_write_approvals", &caller, Some(&target)),
                "Grant missing for {} -> {}",
                caller,
                target
            );
        }

        std::fs::remove_dir_all(&home).ok();
    }

    /// Torn temp file: corrupt temp (crash during write) must not corrupt main.
    #[test]
    fn torn_temp_file_does_not_corrupt() {
        let (home, _guard) = locked_test_home();

        persist_test_grant("alice", "bob");
        assert!(grant_exists("mcp_write_approvals", "alice", Some("bob")));

        let path = grants_path();
        let tmp = path.with_extension("json.supercli-tmp");
        std::fs::write(&tmp, b"not valid json{{{").unwrap();

        assert!(grant_exists("mcp_write_approvals", "alice", Some("bob")));

        std::fs::remove_file(&tmp).ok();
        std::fs::remove_dir_all(&home).ok();
    }

    /// Rename is atomic: file is always valid JSON.
    #[test]
    fn rename_is_atomic() {
        let (home, _guard) = locked_test_home();

        persist_test_grant("alice", "bob");
        persist_test_grant("charlie", "dave");

        assert!(grant_exists("mcp_write_approvals", "alice", Some("bob")));
        assert!(grant_exists("mcp_write_approvals", "charlie", Some("dave")));

        let path = grants_path();
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert!(parsed.is_object());

        std::fs::remove_dir_all(&home).ok();
    }
}

/// Load grants for reconciliation (returns the raw map).
/// Used by `grant_audit::reconcile_grants()`.
pub fn load_grants_for_reconcile() -> Map<String, Value> {
    load_grants()
}

/// Escape a key component so the flattened form is unambiguous.
/// Colons are the separator; `%` is the escape character.
/// In practice components (UUID session IDs, bundle IDs, connector names)
/// never contain colons, but escaping makes the invariant explicit.
pub(crate) fn escape_component(s: &str) -> String {
    s.replace('%', "%25").replace(':', "%3A")
}

/// Reverse of `escape_component`.
fn unescape_component(s: &str) -> String {
    s.replace("%3A", ":").replace("%25", "%")
}

/// Flatten the grants map into canonical grant keys.
///
/// The canonical key format matches the `grant_key` recorded in the audit
/// log by `grant_writer`:
/// - `write:{caller}:{target}` for each target in `mcp_write_approvals[caller]`
/// - `browser:{caller}` for each caller in `browser_approvals`
/// - `computer:{caller}` for each caller in `computer_approvals`
/// - `app-open:{caller}:{app_id}` for each app in `mcp_app_open_approvals[caller]`
/// - `connector:{caller}:{connector}:{tool}` for each entry in
///   `mcp_connector_approvals[caller]`
///
/// Components are %-escaped (see `escape_component`).
pub fn flatten_grant_keys(root: &Map<String, Value>) -> std::collections::HashSet<String> {
    let mut keys = std::collections::HashSet::new();

    if let Some(map) = root.get("mcp_write_approvals").and_then(|v| v.as_object()) {
        for (caller, list) in map {
            if let Some(arr) = list.as_array() {
                for target in arr.iter().filter_map(|v| v.as_str()) {
                    keys.insert(format!(
                        "write:{}:{}",
                        escape_component(caller),
                        escape_component(target)
                    ));
                }
            }
        }
    }

    for (json_key, kind) in [
        ("browser_approvals", "browser"),
        ("computer_approvals", "computer"),
    ] {
        if let Some(arr) = root.get(json_key).and_then(|v| v.as_array()) {
            for caller in arr.iter().filter_map(|v| v.as_str()) {
                keys.insert(format!("{kind}:{}", escape_component(caller)));
            }
        }
    }

    if let Some(map) = root
        .get("mcp_app_open_approvals")
        .and_then(|v| v.as_object())
    {
        for (caller, list) in map {
            if let Some(arr) = list.as_array() {
                for app_id in arr.iter().filter_map(|v| v.as_str()) {
                    keys.insert(format!(
                        "app-open:{}:{}",
                        escape_component(caller),
                        escape_component(app_id)
                    ));
                }
            }
        }
    }

    if let Some(map) = root
        .get("mcp_connector_approvals")
        .and_then(|v| v.as_object())
    {
        for (caller, list) in map {
            if let Some(arr) = list.as_array() {
                for entry in arr {
                    let connector = entry.get("connector").and_then(|v| v.as_str());
                    let tool = entry.get("tool").and_then(|v| v.as_str());
                    if let (Some(c), Some(t)) = (connector, tool) {
                        keys.insert(format!(
                            "connector:{}:{}:{}",
                            escape_component(caller),
                            escape_component(c),
                            escape_component(t)
                        ));
                    }
                    // Legacy bare-string entries never match an audit key
                    // (fail closed — they are ignored here and never match
                    // at check time either).
                }
            }
        }
    }

    keys
}

/// Remove specific grant keys (used for quarantining).
/// Each key is in format "kind:caller:target" or "kind:caller".
pub fn remove_grants(keys: &[String]) -> Result<(), String> {
    edit_grants(|root| {
        for key in keys {
            // Keys are escaped; split on ':' then unescape each component.
            let parts: Vec<String> = key.split(':').map(unescape_component).collect();
            let parts_ref: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
            match parts_ref.as_slice() {
                ["write", caller, target] => {
                    if let Some(map) = root
                        .get_mut("mcp_write_approvals")
                        .and_then(|v| v.as_object_mut())
                    {
                        if let Some(list) = map.get_mut(*caller).and_then(|v| v.as_array_mut()) {
                            list.retain(|v| v.as_str() != Some(*target));
                        }
                    }
                }
                ["browser", caller] => {
                    if let Some(list) = root
                        .get_mut("browser_approvals")
                        .and_then(|v| v.as_array_mut())
                    {
                        list.retain(|v| v.as_str() != Some(*caller));
                    }
                }
                ["computer", caller] => {
                    if let Some(list) = root
                        .get_mut("computer_approvals")
                        .and_then(|v| v.as_array_mut())
                    {
                        list.retain(|v| v.as_str() != Some(*caller));
                    }
                }
                ["app-open", caller, app_id] => {
                    if let Some(map) = root
                        .get_mut("mcp_app_open_approvals")
                        .and_then(|v| v.as_object_mut())
                    {
                        if let Some(list) = map.get_mut(*caller).and_then(|v| v.as_array_mut()) {
                            list.retain(|v| v.as_str() != Some(*app_id));
                        }
                    }
                }
                ["connector", caller, connector, tool] => {
                    if let Some(map) = root
                        .get_mut("mcp_connector_approvals")
                        .and_then(|v| v.as_object_mut())
                    {
                        if let Some(list) = map.get_mut(*caller).and_then(|v| v.as_array_mut()) {
                            list.retain(|v| {
                                let c = v.get("connector").and_then(|x| x.as_str());
                                let t = v.get("tool").and_then(|x| x.as_str());
                                !(c == Some(*connector) && t == Some(*tool))
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    Ok(())
}
