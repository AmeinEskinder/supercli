//! Sharded grant storage (S2).
//!
//! `persist_grant` was serializing all concurrent approvals on the
//! app-state.json exclusive lock (load entire JSON + mutate + save entire
//! JSON under flock). Grants have no hash-chain semantics — the chain lives
//! in action-reviews.jsonl, not here — so they get their own file (`grants.json`)
//! with its own lock. This shards the contention: grant persists no longer
//! block non-grant app-state edits, and the critical section is smaller
//! (grants file is tiny vs. full app-state).
//!
//! ## Security: crash safety and write order (S2 point 4)
//!
//! A grant is a *remembered approval* — it lets future identical requests
//! skip the user prompt. It is NOT the authorization for the current request.
//!
//! Write order:
//! 1. User approves via phone → `hub.answer()` wakes the MCP thread.
//! 2. `hub.request()` returns `ok=true` (explicit user approval).
//! 3. `persist_grant()` writes the grant to `grants.json` (temp + fsync +
//!    rename + fsync dir, all under the grants lock).
//! 4. MCP returns success to the caller.
//!
//! Crash safety argument:
//! - Crash before (3): The current request was user-approved, but the grant
//!   wasn't saved. On restart, the next identical request re-prompts the
//!   user. Safe: fail closed, no privilege escalation.
//! - Crash during (3): The temp+rename is atomic. The directory fsync makes
//!   the rename durable. A reader sees either the old file or the new file,
//!   never a half-written grant. Safe.
//! - Crash after (3): Grant is durable. Current request already completed.
//!   Safe.
//!
//! A grant can NEVER be 'applied' without the user's explicit approval,
//! because `persist_grant` is only called after `hub.request()` returns
//! `ok=true`. The review-log write-ahead (with hash chain) is for tool
//! execution, not for grants — they are separate concerns.
//!
//! ## Backward compatibility
//!
//! `already_granted` checks both the new `grants.json` and the legacy
//! `app-state.json` locations, so pre-S2 grants continue to work.

use serde_json::{Map, Value};
use std::path::PathBuf;

/// Get the grants file path.
fn grants_path() -> PathBuf {
    crate::app_paths::grants_path()
}

/// Acquire an exclusive lock on the grants file.
fn lock_grants() -> Result<crate::app_state::FileLock, String> {
    let path = grants_path();
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
    // Retry loop for optimistic concurrency
    loop {
        // OUTSIDE lock: load, mutate, serialize
        let original_bytes = std::fs::read(grants_path()).unwrap_or_default();
        let mut map = serde_json::from_slice::<Value>(&original_bytes)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let outcome = mutate(&mut map)?;
        let new_bytes = serde_json::to_vec_pretty(&Value::Object(map))
            .map_err(|e| e.to_string())?;

        // Acquire lock
        let _lock = lock_grants()?;

        // Re-read under lock; check for conflicts
        let current_bytes = std::fs::read(grants_path()).unwrap_or_default();
        if current_bytes == original_bytes {
            // No conflict: write the pre-serialized bytes
            write_grants_bytes(&new_bytes)?;
            return Ok(outcome);
        }
        // Conflict: another writer changed the file. Retry.
        // (Lock is released here via _lock drop)
    }
}

/// Write pre-serialized grants bytes with crash safety.
/// Caller must hold the grants lock.
fn write_grants_bytes(body: &[u8]) -> Result<(), String> {
    let path = grants_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.unpeel-tmp");
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
        let dir = std::env::temp_dir().join(format!("grant-test-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u32));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn persist_test_grant(caller: &str, target: &str) {
        edit_grants(|map| {
            let key = "mcp_write_approvals";
            let entry = map.entry(key.to_string()).or_insert(Value::Object(Map::new()));
            if let Value::Object(obj) = entry {
                let caller_entry = obj.entry(caller.to_string()).or_insert(Value::Array(vec![]));
                if let Value::Array(arr) = caller_entry {
                    if !arr.iter().any(|v| v.as_str() == Some(target)) {
                        arr.push(Value::String(target.to_string()));
                    }
                }
            }
            Ok::<(), String>(())
        }).unwrap();
    }

    /// Concurrent writers: N threads each persist a grant. All must succeed,
    /// and the final file must contain all grants (no lost updates).
    #[test]
    fn concurrent_writers_no_lost_updates() {
        let home = test_home();
        std::env::set_var("UNPEEL_HOME", &home);
        
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
            assert!(grant_exists("mcp_write_approvals", &caller, Some(&target)),
                    "Grant missing for {} -> {}", caller, target);
        }
        
        std::fs::remove_dir_all(&home).ok();
    }

    /// Torn temp file: corrupt temp (crash during write) must not corrupt main.
    #[test]
    fn torn_temp_file_does_not_corrupt() {
        let home = test_home();
        std::env::set_var("UNPEEL_HOME", &home);
        
        persist_test_grant("alice", "bob");
        assert!(grant_exists("mcp_write_approvals", "alice", Some("bob")));
        
        let path = grants_path();
        let tmp = path.with_extension("json.unpeel-tmp");
        std::fs::write(&tmp, b"not valid json{{{").unwrap();
        
        assert!(grant_exists("mcp_write_approvals", "alice", Some("bob")));
        
        std::fs::remove_file(&tmp).ok();
        std::fs::remove_dir_all(&home).ok();
    }

    /// Rename is atomic: file is always valid JSON.
    #[test]
    fn rename_is_atomic() {
        let home = test_home();
        std::env::set_var("UNPEEL_HOME", &home);
        
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
