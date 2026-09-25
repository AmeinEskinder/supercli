//! Grant audit log with hash chain (Phase 13 v2 security fix).
//!
//! ## Problem
//!
//! A grant is made durable in `grants.json` BEFORE anything is recorded in
//! the tamper-evident hash chain. This means a durable permission can exist
//! without an audit record, e.g.:
//! 1. User approves → `persist_grant()` writes `grants.json` (durable)
//! 2. Crash before tool execution → no review-log entry
//! 3. On restart: grant exists, but no audit record of the approval
//!
//! ## Fix
//!
//! Append a `grant_created` chain entry (actor, scope, tool) with write-ahead
//! fsync BEFORE writing `grants.json`. The grant audit log
//! (`~/.unpeel/grant-audit.jsonl`) has its own hash chain, separate from the
//! per-session review logs.
//!
//! ## Startup reconciliation
//!
//! On startup, `reconcile_grants()`:
//! - Chain entry without grant: stays revoked (fail closed; do NOT silently
//!   re-create the grant). The approval was recorded but the grant wasn't
//!   persisted — the user must re-approve.
//! - Grant without chain entry: QUARANTINE the grant (move to
//!   `grants.json.quarantined`) and report a doctor error. This is a
//!   tamper-evidence violation — a grant exists without an audit record.
//!
//! ## Doctor check
//!
//! `doctor` verifies `grants ⊆ chain`: every grant in `grants.json` must have
//! a corresponding `grant_created` entry in the audit log.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

const AUDIT_FILE: &str = "grant-audit.jsonl";
const QUARANTINE_FILE: &str = "grants.json.quarantined";
const GENESIS_PREV_HASH: &str = "0";

/// Cross-process lock for the grant audit log.
///
/// Uses `flock(2)` on `grant-audit.jsonl.lock` (Unix) to serialize the
/// read-tail + append + fsync sequence across processes. Without this,
/// two processes (e.g. Host and CLI) can both read the same prev_hash
/// and append entries with the same prev_hash, forking the hash chain.
///
/// The lock is held for the duration of `record_grants_created_batch`.
/// Dropping the guard releases the flock (kernel releases on process exit,
/// so a crashed holder cannot wedge the lock).
struct AuditLock {
    #[cfg(unix)]
    _file: std::fs::File,
    #[cfg(not(unix))]
    _path: PathBuf,
}

/// Acquire the exclusive cross-process lock for the audit log.
/// Blocks until acquired (with a 30s fail-closed timeout).
fn acquire_audit_lock(audit_path: &std::path::Path) -> Result<AuditLock, String> {
    let lock_path = audit_path.with_extension("jsonl.lock");
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&lock_path)
            .map_err(|e| format!("open audit lock {}: {e}", lock_path.display()))?;
        // Non-blocking flock with poll + 30s timeout (fail closed).
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(30);
        loop {
            // SAFETY: flock with LOCK_EX|LOCK_NB on a valid fd.
            let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if ret == 0 {
                return Ok(AuditLock { _file: file });
            }
            let err = std::io::Error::last_os_error();
            // EWOULDBLOCK means locked by another process; retry.
            if err.raw_os_error() != Some(libc::EWOULDBLOCK) {
                return Err(format!("flock audit lock: {err}"));
            }
            if start.elapsed() > timeout {
                return Err("timeout acquiring audit lock (fail closed)".to_string());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    #[cfg(not(unix))]
    {
        // Non-Unix fallback: create-exclusive.
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => Ok(AuditLock { _path: lock_path }),
            Err(e) => Err(format!("acquire audit lock: {e}")),
        }
    }
}

/// A grant audit entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantAuditEntry {
    pub entry_id: String,
    pub ts_ms: u64,
    /// e.g. "human:device-123", "human:paired-device", or "unknown"
    /// (defensive: production always identifies the answerer).
    pub actor: String,
    /// e.g. "write", "browser", "computer"
    pub scope: String,
    /// Tool or grant kind
    pub tool: String,
    /// The grant key (e.g. "write:session-a:session-b")
    pub grant_key: String,
    pub prev_hash: String,
    pub entry_hash: String,
}

fn audit_path() -> PathBuf {
    crate::app_paths::unpeel_home().join(AUDIT_FILE)
}

fn quarantine_path() -> PathBuf {
    crate::app_paths::unpeel_home().join(QUARANTINE_FILE)
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Get the hash of the last entry, or None if the log is empty/missing.
fn last_entry_hash() -> Result<Option<String>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let path = audit_path();
    let mut file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    if len == 0 {
        return Ok(None);
    }
    // Read only the tail: the last line is at most a few KB (one JSON entry).
    // Seek back up to 8 KiB and find the last non-empty line.
    let tail_len = len.min(8192);
    file.seek(SeekFrom::End(-(tail_len as i64)))
        .map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; tail_len as usize];
    file.read_exact(&mut buf).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&buf);
    // Find the last non-empty line. If the file was truncated mid-line by a
    // crash, the last line may be partial — verification will catch it, but
    // for chaining we need the last COMPLETE line. Walk backwards.
    let mut lines: Vec<&str> = text.lines().collect();
    while let Some(line) = lines.pop() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // If we read a partial tail (didn't start at a line boundary) and
        // this is the first line of our buffer, it may be truncated. Only
        // trust it if we read from the start of the file.
        if lines.is_empty() && tail_len < len {
            // First line of a partial tail — may be truncated, skip it.
            continue;
        }
        let entry: GrantAuditEntry =
            serde_json::from_str(line).map_err(|e| format!("parse: {e}"))?;
        return Ok(Some(entry.entry_hash));
    }
    Ok(None)
}

/// Build a single audit entry, chaining off `prev_hash`.
fn build_entry(
    actor: &str,
    scope: &str,
    tool: &str,
    grant_key: &str,
    prev_hash: String,
) -> Result<GrantAuditEntry, String> {
    let mut entry = GrantAuditEntry {
        entry_id: uuid::Uuid::new_v4().to_string(),
        ts_ms: now_ms(),
        actor: actor.to_string(),
        scope: scope.to_string(),
        tool: tool.to_string(),
        grant_key: grant_key.to_string(),
        prev_hash,
        entry_hash: String::new(),
    };

    // Compute hash over canonical JSON (without entry_hash).
    let mut canonical = serde_json::to_value(&entry).map_err(|e| e.to_string())?;
    if let Some(obj) = canonical.as_object_mut() {
        obj.remove("entry_hash");
    }
    let canonical_bytes = serde_json::to_vec(&canonical).map_err(|e| e.to_string())?;
    entry.entry_hash = sha256_hex(&canonical_bytes);
    Ok(entry)
}

/// Record a grant creation in the audit log with write-ahead fsync.
///
/// This MUST be called BEFORE writing the grant to `grants.json`.
/// Returns the entry on success.
pub fn record_grant_created(
    actor: &str,
    scope: &str,
    tool: &str,
    grant_key: &str,
) -> Result<GrantAuditEntry, String> {
    let batch = record_grants_created_batch(&[(actor, scope, tool, grant_key)])?;
    Ok(batch.into_iter().next().unwrap())
}

/// Record a batch of grant creations with a SINGLE fsync (group commit).
///
/// All entries are chained correctly (each entry's prev_hash is the previous
/// entry's hash), appended in one write, and fsynced once. This is the
/// write-ahead path used by the group-commit writer: durability per request
/// is unchanged (the caller waits for this fsync), but N concurrent requests
/// share one fsync instead of N.
pub fn record_grants_created_batch(
    items: &[(&str, &str, &str, &str)],
) -> Result<Vec<GrantAuditEntry>, String> {
    let path = audit_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // Cross-process serialization: acquire exclusive flock on the audit
    // lockfile before reading the tail and appending. Without this, two
    // processes (e.g. Host and CLI) can both read the same prev_hash and
    // append entries with the same prev_hash, forking the chain.
    // The lock is held for the read-tail + append + fsync sequence.
    let _lock = acquire_audit_lock(&path)?;

    let mut prev_hash = last_entry_hash()
        .map_err(|e| format!("read audit log: {e}"))?
        .unwrap_or_else(|| GENESIS_PREV_HASH.to_string());

    let mut entries = Vec::with_capacity(items.len());
    let mut buf = String::new();
    for (actor, scope, tool, grant_key) in items {
        let entry = build_entry(actor, scope, tool, grant_key, prev_hash)?;
        prev_hash = entry.entry_hash.clone();
        let mut line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
        line.push('\n');
        buf.push_str(&line);
        entries.push(entry);
    }

    // Single append + single fsync for the whole batch.
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    file.write_all(buf.as_bytes())
        .map_err(|e| format!("write: {e}"))?;
    file.flush().map_err(|e| format!("flush: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("fsync {}: {e}", path.display()))?;

    Ok(entries)
}

/// Verify the grant audit chain. Returns the number of entries verified.
pub fn verify_grant_audit() -> Result<usize, String> {
    let path = audit_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.to_string()),
    };
    let mut prev_hash = GENESIS_PREV_HASH.to_string();
    let mut count = 0;

    for (idx, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: GrantAuditEntry =
            serde_json::from_str(line).map_err(|e| format!("line {idx}: parse: {e}"))?;

        // Verify prev_hash links.
        if entry.prev_hash != prev_hash {
            return Err(format!(
                "line {idx}: prev_hash mismatch (expected {prev_hash}, got {})",
                entry.prev_hash
            ));
        }

        // Verify entry_hash.
        let mut canonical = serde_json::to_value(&entry).map_err(|e| e.to_string())?;
        if let Some(obj) = canonical.as_object_mut() {
            obj.remove("entry_hash");
        }
        let canonical_bytes = serde_json::to_vec(&canonical).map_err(|e| e.to_string())?;
        let computed = sha256_hex(&canonical_bytes);
        if computed != entry.entry_hash {
            return Err(format!("line {idx}: entry_hash mismatch (tamper detected)"));
        }

        prev_hash = entry.entry_hash;
        count += 1;
    }

    Ok(count)
}

/// Get all grant keys that have audit entries.
fn audited_grant_keys() -> Result<std::collections::HashSet<String>, String> {
    let path = audit_path();
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut keys = std::collections::HashSet::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<GrantAuditEntry>(line) {
            keys.insert(entry.grant_key);
        }
    }
    Ok(keys)
}

/// Startup reconciliation.
///
/// - Grant without audit entry → quarantine (move to quarantine file), return error.
/// - Audit entry without grant → stays revoked (do NOT re-create). This is
///   fail-closed: the approval was recorded but the grant wasn't persisted.
///
/// Returns Ok(()) if all grants have audit entries, Err with details otherwise.
pub fn reconcile_grants() -> Result<(), String> {
    let grants = crate::grant_store::load_grants_for_reconcile();
    // Flatten to canonical keys (kind:caller:target) so the comparison is
    // against the same key space the audit log records.
    let grant_keys = crate::grant_store::flatten_grant_keys(&grants);
    let audited = audited_grant_keys()?;

    let mut orphaned = Vec::new();
    for key in &grant_keys {
        if !audited.contains(key) {
            orphaned.push(key.clone());
        }
    }

    if !orphaned.is_empty() {
        // Quarantine the orphaned grants (crash-safe: temp + fsync + rename
        // + dir fsync, same durability as grants.json itself). The file
        // records the flattened keys for forensics; the grants are removed
        // from the live file below.
        let quarantine = quarantine_path();
        let payload = serde_json::json!({
            "quarantined_at_ms": now_ms(),
            "reason": "grant without audit entry (tamper-evidence violation)",
            "keys": orphaned,
        });
        let qjson = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
        crash_safe_write(&quarantine, qjson.as_bytes())
            .map_err(|e| format!("write quarantine: {e}"))?;

        // Remove orphaned grants from the main file.
        crate::grant_store::remove_grants(&orphaned)?;

        return Err(format!(
            "SECURITY: {} grant(s) without audit entry quarantined to {}. \
             This indicates tampering or a crash during grant creation. \
             Run `unpeel doctor` for details.",
            orphaned.len(),
            quarantine.display(),
        ));
    }

    Ok(())
}

/// Crash-safe file write: temp + fsync + rename + dir fsync.
fn crash_safe_write(path: &std::path::Path, data: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .map_err(|e| format!("open {}: {e}", tmp.display()))?;
        f.write_all(data)
            .map_err(|e| format!("write {}: {e}", tmp.display()))?;
        f.sync_all()
            .map_err(|e| format!("fsync {}: {e}", tmp.display()))?;
    }
    std::fs::rename(&tmp, path)
        .map_err(|e| format!("rename {} -> {}: {e}", tmp.display(), path.display()))?;
    if let Some(parent) = path.parent() {
        let dir = std::fs::File::open(parent)
            .map_err(|e| format!("open dir {}: {e}", parent.display()))?;
        dir.sync_all()
            .map_err(|e| format!("fsync dir {}: {e}", parent.display()))?;
    }
    Ok(())
}

/// Doctor check: every grant must have an audit entry (grants ⊆ chain).
pub fn doctor_check_grants_subset() -> Result<(), String> {
    let grants = crate::grant_store::load_grants_for_reconcile();
    let grant_keys = crate::grant_store::flatten_grant_keys(&grants);
    let audited = audited_grant_keys()?;

    let mut missing = Vec::new();
    for key in &grant_keys {
        if !audited.contains(key) {
            missing.push(key.clone());
        }
    }

    if !missing.is_empty() {
        return Err(format!(
            "grants without audit entries (tamper-evidence violation): {:?}",
            missing
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_paths::TEST_UNPEEL_HOME_LOCK;

    fn test_home(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "grant-audit-test-{}-{}-{}",
            std::process::id(),
            label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Run `f` with UNPEEL_HOME pointed at a scratch dir, serialized against
    /// all other UNPEEL_HOME-mutating tests, restoring the previous value.
    fn with_test_home(label: &str, f: impl for<'a> FnOnce(&'a PathBuf)) {
        let _lock = TEST_UNPEEL_HOME_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = test_home(label);
        let prev = std::env::var_os("UNPEEL_HOME");
        std::env::set_var("UNPEEL_HOME", &dir);
        f(&dir);
        match &prev {
            Some(p) => std::env::set_var("UNPEEL_HOME", p),
            None => std::env::remove_var("UNPEEL_HOME"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn audit_chain_verifies() {
        with_test_home("chain", |_dir| {
            // Record two entries.
            record_grant_created("human:device-1", "write", "mcp", "write:a:b").unwrap();
            record_grant_created("human:device-1", "browser", "mcp", "browser:c").unwrap();

            // Verify.
            let count = verify_grant_audit().unwrap();
            assert_eq!(count, 2);

            // Tamper detection: flip a byte.
            let path = audit_path();
            let content = std::fs::read_to_string(&path).unwrap();
            let tampered = content.replacen("write", "WRITEX", 1);
            std::fs::write(&path, tampered).unwrap();
            assert!(verify_grant_audit().is_err());
        });
    }

    #[test]
    fn flatten_grant_keys_matches_audit_format() {
        with_test_home("flatten", |_dir| {
            // Write grants directly (bypassing the writer), then flatten.
            crate::grant_store::edit_grants(|root| {
                crate::grant_writer::apply_grant_mutation(root, "write", "alice", Some("bob"));
                crate::grant_writer::apply_grant_mutation(root, "browser", "carol", None);
                crate::grant_writer::apply_grant_mutation(
                    root,
                    "app-open",
                    "dave",
                    Some("com.example.app"),
                );
                Ok::<(), String>(())
            })
            .unwrap();
            // Connector grant via the writer's mutation (same storage shape
            // as persist_connector_grant used before the audit existed).
            crate::grant_store::edit_grants(|root| {
                crate::grant_writer::apply_grant_mutation(
                    root,
                    "connector",
                    "erin",
                    Some("github:search"),
                );
                Ok::<(), String>(())
            })
            .unwrap();

            let grants = crate::grant_store::load_grants_for_reconcile();
            let keys = crate::grant_store::flatten_grant_keys(&grants);
            assert!(keys.contains("write:alice:bob"), "{keys:?}");
            assert!(keys.contains("browser:carol"), "{keys:?}");
            assert!(keys.contains("app-open:dave:com.example.app"), "{keys:?}");
            assert!(keys.contains("connector:erin:github:search"), "{keys:?}");
            assert_eq!(keys.len(), 4);
        });
    }

    #[test]
    fn reconcile_quarantines_grant_without_audit_entry() {
        with_test_home("quarantine", |dir| {
            // A grant with NO audit entry (tamper or pre-audit grant).
            crate::grant_store::edit_grants(|root| {
                crate::grant_writer::apply_grant_mutation(root, "write", "mallory", Some("victim"));
                Ok::<(), String>(())
            })
            .unwrap();

            let result = reconcile_grants();
            assert!(result.is_err(), "orphaned grant must fail reconciliation");
            let err = result.unwrap_err();
            assert!(err.contains("quarantined"), "{err}");

            // The grant is gone from the live file...
            let grants = crate::grant_store::load_grants_for_reconcile();
            let keys = crate::grant_store::flatten_grant_keys(&grants);
            assert!(!keys.contains("write:mallory:victim"), "{keys:?}");

            // ...and recorded in the quarantine file.
            let qpath = dir.join(QUARANTINE_FILE);
            assert!(qpath.exists(), "quarantine file must exist");
            let qcontent = std::fs::read_to_string(&qpath).unwrap();
            assert!(qcontent.contains("write:mallory:victim"), "{qcontent}");
            // Quarantine file is valid JSON (crash-safe write produced a
            // complete file, not a torn one).
            let _: serde_json::Value = serde_json::from_str(&qcontent).unwrap();

            // Doctor agrees: nothing left to flag.
            doctor_check_grants_subset().unwrap();
        });
    }

    #[test]
    fn reconcile_leaves_audit_without_grant_revoked() {
        with_test_home("revoked", |_dir| {
            // Audit entry WITHOUT a grant: the approval was recorded but the
            // grant write never landed (crash between audit fsync and
            // grants.json write). Fail closed: no error, no silent re-create.
            record_grant_created("human:device-9", "write", "write", "write:ghost:target").unwrap();

            reconcile_grants().expect("audit-only entry must not fail reconciliation");

            // The grant was NOT re-created.
            let grants = crate::grant_store::load_grants_for_reconcile();
            let keys = crate::grant_store::flatten_grant_keys(&grants);
            assert!(!keys.contains("write:ghost:target"), "{keys:?}");

            // Doctor is satisfied (grants ⊆ chain holds vacuously).
            doctor_check_grants_subset().unwrap();
        });
    }

    #[test]
    fn reconcile_passes_when_grants_match_chain() {
        with_test_home("match", |_dir| {
            // The production order: audit entry first, then the grant.
            record_grant_created("human:device-2", "write", "write", "write:alice:bob").unwrap();
            crate::grant_store::edit_grants(|root| {
                crate::grant_writer::apply_grant_mutation(root, "write", "alice", Some("bob"));
                Ok::<(), String>(())
            })
            .unwrap();

            reconcile_grants().unwrap();
            doctor_check_grants_subset().unwrap();
        });
    }

    #[test]
    fn doctor_flags_grant_without_audit_entry() {
        with_test_home("doctor", |_dir| {
            crate::grant_store::edit_grants(|root| {
                crate::grant_writer::apply_grant_mutation(root, "browser", "sneaky", None);
                Ok::<(), String>(())
            })
            .unwrap();

            let err = doctor_check_grants_subset().unwrap_err();
            assert!(err.contains("browser:sneaky"), "{err}");
        });
    }
}
