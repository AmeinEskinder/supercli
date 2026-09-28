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
//! (`~/.supercli/grant-audit.jsonl`) has its own hash chain, separate from the
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
//! ## Revocation
//!
//! `supercli grants revoke` removes the grant from `grants.json` FIRST, then
//! appends a chained `grant_revoked` entry (actor, key, time) to the audit
//! log. The explicit entry is what lets the chain distinguish a legitimate
//! revoke from a deleted grant: without it, "audit entry without grant" is
//! ambiguous (crash during creation vs. tampered store).
//!
//! Ordering note: the permission removal lands before the audit entry (fail
//! closed — a crash leaves the grant revoked). A crash between the two is
//! fail-visible: doctor reports the key as anomalous.
//!
//! ## Doctor check
//!
//! `doctor` verifies `grants ⊆ created`: every grant in `grants.json` must
//! have a corresponding `grant_created` entry, and a live grant's latest
//! creation must postdate any revocation. It also classifies created keys
//! with no live grant: `grant_revoked` present → legitimate revocation;
//! absent → anomaly (possible deletion).

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
            .truncate(false)
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

/// Audit event types recorded in grant-audit.jsonl.
pub const EVENT_GRANT_CREATED: &str = "grant_created";
pub const EVENT_GRANT_REVOKED: &str = "grant_revoked";

fn default_event() -> String {
    EVENT_GRANT_CREATED.to_string()
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
    /// "grant_created" or "grant_revoked". Entries written before the
    /// revoke-tightening have no such field on disk and default to
    /// "grant_created".
    #[serde(default = "default_event")]
    pub event: String,
    pub prev_hash: String,
    pub entry_hash: String,
}

fn audit_path() -> PathBuf {
    crate::app_paths::supercli_home().join(AUDIT_FILE)
}

fn quarantine_path() -> PathBuf {
    crate::app_paths::supercli_home().join(QUARANTINE_FILE)
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

fn last_entry_hash_at(path: &std::path::Path) -> Result<Option<String>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = match std::fs::File::open(path) {
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
    event: &str,
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
        event: event.to_string(),
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
    record_grants_created_batch_at(&crate::app_paths::supercli_home(), items)
}

/// A single audit entry to append (chaining resolved by the appender).
struct PendingEntry {
    event: String,
    actor: String,
    scope: String,
    tool: String,
    grant_key: String,
}

/// Append entries to the audit log with correct chaining and a single fsync.
///
/// Holds the cross-process audit lock for the read-tail + append + fsync
/// sequence so concurrent writers (Host and CLI) cannot fork the chain.
/// Without this, two processes can both read the same prev_hash and append
/// entries with the same prev_hash, forking the hash chain.
fn append_entries_at(
    home: &std::path::Path,
    pending: Vec<PendingEntry>,
) -> Result<Vec<GrantAuditEntry>, String> {
    let path = home.join(AUDIT_FILE);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // Cross-process serialization: acquire exclusive flock on the audit
    // lockfile before reading the tail and appending. The lock is held for
    // the read-tail + append + fsync sequence.
    let _lock = acquire_audit_lock(&path)?;

    let mut prev_hash = last_entry_hash_at(&path)
        .map_err(|e| format!("read audit log: {e}"))?
        .unwrap_or_else(|| GENESIS_PREV_HASH.to_string());

    let mut entries = Vec::with_capacity(pending.len());
    let mut buf = String::new();
    for p in pending {
        let entry = build_entry(
            &p.event,
            &p.actor,
            &p.scope,
            &p.tool,
            &p.grant_key,
            prev_hash,
        )?;
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

/// Same as `record_grants_created_batch` but with an explicit home directory,
/// for tests that must not mutate the process-global SUPERCLI_HOME env var.
pub fn record_grants_created_batch_at(
    home: &std::path::Path,
    items: &[(&str, &str, &str, &str)],
) -> Result<Vec<GrantAuditEntry>, String> {
    append_entries_at(
        home,
        items
            .iter()
            .map(|(actor, scope, tool, grant_key)| PendingEntry {
                event: EVENT_GRANT_CREATED.to_string(),
                actor: actor.to_string(),
                scope: scope.to_string(),
                tool: tool.to_string(),
                grant_key: grant_key.to_string(),
            })
            .collect(),
    )
}

/// Append a `grant_revoked` entry to the audit log.
///
/// Records actor, grant key, and timestamp as a new chained entry (same
/// hash-chain discipline as creation entries, so the hash continuity of the
/// log is preserved). This is the explicit revocation record: without it the
/// chain cannot distinguish a legitimate revoke from a deleted grant.
pub fn record_grant_revoked(actor: &str, grant_key: &str) -> Result<GrantAuditEntry, String> {
    record_grant_revoked_at(&crate::app_paths::supercli_home(), actor, grant_key)
}

/// Same as `record_grant_revoked` but with an explicit home directory.
pub fn record_grant_revoked_at(
    home: &std::path::Path,
    actor: &str,
    grant_key: &str,
) -> Result<GrantAuditEntry, String> {
    // scope/tool mirror the creation convention (the grant kind prefix, also
    // the first component of the canonical key); the `event` field carries
    // the revoke semantics.
    let kind = grant_key.split(':').next().unwrap_or("");
    let entries = append_entries_at(
        home,
        vec![PendingEntry {
            event: EVENT_GRANT_REVOKED.to_string(),
            actor: actor.to_string(),
            scope: kind.to_string(),
            tool: kind.to_string(),
            grant_key: grant_key.to_string(),
        }],
    )?;
    Ok(entries.into_iter().next().unwrap())
}

/// Revoke a grant: remove it from `grants.json`, then append the chained
/// `grant_revoked` audit entry.
///
/// Ordering is deliberate: the permission removal lands first (fail closed —
/// a crash leaves the grant revoked), then the audit entry. A crash between
/// the two is fail-visible: doctor flags the created-without-revoke gap as
/// an anomaly instead of silently treating the grant as revoked.
///
/// Quarantining (`reconcile_grants`) intentionally does NOT go through here:
/// a quarantined grant is not a user revocation, and the quarantine file is
/// its own forensic record.
pub fn revoke_grant(actor: &str, grant_key: &str) -> Result<(), String> {
    crate::grant_store::remove_grants(std::slice::from_ref(&grant_key.to_string()))?;
    record_grant_revoked(actor, grant_key)?;
    Ok(())
}

/// Verify the grant audit chain. Returns the number of entries verified.
pub fn verify_grant_audit() -> Result<usize, String> {
    verify_grant_audit_at(&crate::app_paths::supercli_home())
}

/// Same as `verify_grant_audit` but with an explicit home directory.
pub fn verify_grant_audit_at(home: &std::path::Path) -> Result<usize, String> {
    let path = home.join(AUDIT_FILE);
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
        // Parse the raw JSON first. The entry_hash was computed over the
        // stored bytes (minus entry_hash) at write time. Entries written
        // before the `event` field existed hash WITHOUT it; re-serializing
        // through the struct would add the defaulted field and break their
        // hashes. Hashing the raw value keeps both generations verifiable.
        let raw: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("line {idx}: parse: {e}"))?;
        let entry: GrantAuditEntry = serde_json::from_value(raw.clone())
            .map_err(|e| format!("line {idx}: schema: {e}"))?;

        // Verify prev_hash links.
        if entry.prev_hash != prev_hash {
            return Err(format!(
                "line {idx}: prev_hash mismatch (expected {prev_hash}, got {})",
                entry.prev_hash
            ));
        }

        // Verify entry_hash over the raw stored bytes (minus entry_hash).
        let mut canonical = raw;
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

/// Read every audit entry (typed). A malformed line is an error — the log is
/// tamper-evident and must parse cleanly.
fn read_all_entries() -> Result<Vec<GrantAuditEntry>, String> {
    let path = audit_path();
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut entries = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: GrantAuditEntry =
            serde_json::from_str(line).map_err(|e| format!("line {idx}: parse: {e}"))?;
        entries.push(entry);
    }
    Ok(entries)
}

/// Grant keys with a `grant_created` entry, and keys with a `grant_revoked`
/// entry. Entries predating the `event` field count as created.
fn audit_key_sets() -> Result<
    (
        std::collections::HashSet<String>,
        std::collections::HashSet<String>,
    ),
    String,
> {
    let mut created = std::collections::HashSet::new();
    let mut revoked = std::collections::HashSet::new();
    for entry in read_all_entries()? {
        if entry.event == EVENT_GRANT_REVOKED {
            revoked.insert(entry.grant_key);
        } else {
            created.insert(entry.grant_key);
        }
    }
    Ok((created, revoked))
}

/// Revocation consistency report for doctor.
///
/// - `revoked_clean`: keys with a `grant_created` entry, no live grant, and
///   a `grant_revoked` entry — legitimate revocations.
/// - `anomalous`: keys with a `grant_created` entry, no live grant, and NO
///   `grant_revoked` entry. The chain cannot tell a legitimate revoke from a
///   deleted grant here (possible deletion, or a crash between the creation
///   write-ahead and the grants.json write).
/// - `revoked_but_present`: live grants whose latest `grant_revoked` entry is
///   newer than their latest `grant_created` entry — the grant reappeared
///   without re-approval (possible tamper).
#[derive(Debug, Default)]
pub struct RevocationConsistency {
    pub revoked_clean: Vec<String>,
    pub anomalous: Vec<String>,
    pub revoked_but_present: Vec<String>,
}

/// Check revocation consistency (see `RevocationConsistency`).
pub fn check_revocation_consistency() -> Result<RevocationConsistency, String> {
    let grants = crate::grant_store::load_grants_for_reconcile();
    let live: std::collections::HashSet<String> = crate::grant_store::flatten_grant_keys(&grants);
    let entries = read_all_entries()?;

    // Latest event index per key, per event type.
    let mut last_created: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut last_revoked: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (idx, entry) in entries.iter().enumerate() {
        if entry.event == EVENT_GRANT_REVOKED {
            last_revoked.insert(entry.grant_key.clone(), idx);
        } else {
            last_created.insert(entry.grant_key.clone(), idx);
        }
    }

    let mut report = RevocationConsistency::default();
    for (key, &created_idx) in &last_created {
        let revoked_idx = last_revoked.get(key).copied();
        if live.contains(key) {
            // Live grant: its latest creation must postdate any revocation.
            if let Some(ridx) = revoked_idx {
                if ridx > created_idx {
                    report.revoked_but_present.push(key.clone());
                }
            }
        } else if revoked_idx.is_some() {
            report.revoked_clean.push(key.clone());
        } else {
            report.anomalous.push(key.clone());
        }
    }
    report.revoked_clean.sort();
    report.anomalous.sort();
    report.revoked_but_present.sort();
    Ok(report)
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
    // Only grant_created entries authorize a grant; grant_revoked entries
    // are history, not authorization.
    let (audited, _) = audit_key_sets()?;

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
             Run `supercli doctor` for details.",
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

/// Doctor check: every grant must have a grant_created audit entry
/// (grants ⊆ created), and a live grant's latest creation must postdate any
/// revocation (see `check_revocation_consistency`).
pub fn doctor_check_grants_subset() -> Result<(), String> {
    let grants = crate::grant_store::load_grants_for_reconcile();
    let grant_keys = crate::grant_store::flatten_grant_keys(&grants);
    let (audited, _) = audit_key_sets()?;

    let mut missing = Vec::new();
    for key in &grant_keys {
        if !audited.contains(key) {
            missing.push(key.clone());
        }
    }

    if !missing.is_empty() {
        return Err(format!(
            "grants without audit entries (tamper-evidence violation): {missing:?}"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_paths::TEST_SUPERCLI_HOME_LOCK;

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

    /// Run `f` with SUPERCLI_HOME pointed at a scratch dir, serialized against
    /// all other SUPERCLI_HOME-mutating tests, restoring the previous value.
    fn with_test_home(label: &str, f: impl for<'a> FnOnce(&'a PathBuf)) {
        let _lock = TEST_SUPERCLI_HOME_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = test_home(label);
        let prev = std::env::var_os("SUPERCLI_HOME");
        std::env::set_var("SUPERCLI_HOME", &dir);
        f(&dir);
        match &prev {
            Some(p) => std::env::set_var("SUPERCLI_HOME", p),
            None => std::env::remove_var("SUPERCLI_HOME"),
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

            // Doctor is satisfied on the subset (grants ⊆ created holds
            // vacuously)...
            doctor_check_grants_subset().unwrap();

            // ...but the revocation consistency check flags it: there is no
            // grant_revoked entry, so the chain cannot tell a legitimate
            // revoke from a deleted grant.
            let rep = check_revocation_consistency().unwrap();
            assert_eq!(rep.anomalous, vec!["write:ghost:target".to_string()]);
            assert!(rep.revoked_clean.is_empty());
            assert!(rep.revoked_but_present.is_empty());
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

    #[test]
    fn revoke_appends_grant_revoked_entry() {
        with_test_home("revoke-entry", |_dir| {
            // The production order: audit entry first, then the grant.
            record_grant_created("human:device-7", "write", "write", "write:alice:bob").unwrap();
            crate::grant_store::edit_grants(|root| {
                crate::grant_writer::apply_grant_mutation(root, "write", "alice", Some("bob"));
                Ok::<(), String>(())
            })
            .unwrap();

            revoke_grant("human:cli", "write:alice:bob").unwrap();

            // The grant is gone...
            let grants = crate::grant_store::load_grants_for_reconcile();
            let keys = crate::grant_store::flatten_grant_keys(&grants);
            assert!(!keys.contains("write:alice:bob"), "{keys:?}");

            // ...and the audit log ends with a chained grant_revoked entry
            // carrying actor, key, and time.
            let entries = read_all_entries().unwrap();
            assert_eq!(entries.len(), 2);
            let revoked = entries.last().unwrap();
            assert_eq!(revoked.event, EVENT_GRANT_REVOKED);
            assert_eq!(revoked.actor, "human:cli");
            assert_eq!(revoked.grant_key, "write:alice:bob");
            assert!(revoked.ts_ms > 0);
            // Chained off the creation entry.
            assert_eq!(revoked.prev_hash, entries[0].entry_hash);

            // Chain still verifies (the hash covers the new event field).
            assert_eq!(verify_grant_audit().unwrap(), 2);

            // Doctor: consistent — a legitimate revocation, nothing anomalous.
            doctor_check_grants_subset().unwrap();
            let rep = check_revocation_consistency().unwrap();
            assert_eq!(rep.revoked_clean, vec!["write:alice:bob".to_string()]);
            assert!(rep.anomalous.is_empty());
            assert!(rep.revoked_but_present.is_empty());

            // Startup reconciliation stays quiet on a clean revoke.
            reconcile_grants().unwrap();
        });
    }

    #[test]
    fn doctor_flags_live_grant_revoked_after_creation() {
        with_test_home("revoked-present", |_dir| {
            // created -> revoked -> the grant reappears without re-approval
            // (hand-edit or tamper): the latest event for the key is a
            // revocation, so the live grant is unauthorized.
            record_grant_created("human:device-8", "write", "write", "write:eve:mall").unwrap();
            crate::grant_store::edit_grants(|root| {
                crate::grant_writer::apply_grant_mutation(root, "write", "eve", Some("mall"));
                Ok::<(), String>(())
            })
            .unwrap();
            revoke_grant("human:cli", "write:eve:mall").unwrap();
            // Re-add the grant without a new approval.
            crate::grant_store::edit_grants(|root| {
                crate::grant_writer::apply_grant_mutation(root, "write", "eve", Some("mall"));
                Ok::<(), String>(())
            })
            .unwrap();

            let rep = check_revocation_consistency().unwrap();
            assert_eq!(
                rep.revoked_but_present,
                vec!["write:eve:mall".to_string()]
            );
            assert!(rep.anomalous.is_empty());
        });
    }

    #[test]
    fn legacy_entries_without_event_field_still_verify() {
        with_test_home("legacy", |_dir| {
            // Simulate an entry written by a binary predating the `event`
            // field: no "event" key on disk, hash over the event-less
            // canonical form.
            let mut obj = serde_json::json!({
                "entry_id": "legacy-1",
                "ts_ms": 1234567890u64,
                "actor": "human:device-1",
                "scope": "write",
                "tool": "write",
                "grant_key": "write:a:b",
                "prev_hash": "0",
            });
            let hash = sha256_hex(&serde_json::to_vec(&obj).unwrap());
            obj["entry_hash"] = serde_json::json!(hash);
            std::fs::write(
                audit_path(),
                format!("{}\n", serde_json::to_string(&obj).unwrap()),
            )
            .unwrap();

            // The chain verifies with the event defaulting to grant_created...
            assert_eq!(verify_grant_audit().unwrap(), 1);

            // ...and the key counts as a created entry: a matching live grant
            // passes the subset check and is not anomalous.
            crate::grant_store::edit_grants(|root| {
                crate::grant_writer::apply_grant_mutation(root, "write", "a", Some("b"));
                Ok::<(), String>(())
            })
            .unwrap();
            doctor_check_grants_subset().unwrap();
            let rep = check_revocation_consistency().unwrap();
            assert!(rep.anomalous.is_empty());
            assert!(rep.revoked_but_present.is_empty());

            // New entries chain onto the legacy entry without breaking it.
            record_grant_revoked("human:cli", "write:a:b").unwrap();
            assert_eq!(verify_grant_audit().unwrap(), 2);
        });
    }
}
