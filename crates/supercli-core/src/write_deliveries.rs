//! Durable write-ahead log for PTY write idempotency.
//!
//! `RecentWriteIds` (in `session_host`) is an in-memory set: it closes the
//! race between two transports in the same process, but it cannot survive a
//! crash. The window is: bytes delivered to the PTY, then a crash/OOM/panic
//! before `record_applied` runs. On retry the write_id is absent from memory,
//! so the bytes would be delivered to the PTY a second time — a duplicate
//! write, the failure mode this product must never produce silently.
//!
//! This module persists a per-session `write-deliveries.jsonl`:
//! - `{"write_id": "...", "state": "delivering", ...}` is appended **and
//!   fsync'd BEFORE any byte reaches the PTY** (write-ahead).
//! - `{"write_id": "...", "state": "applied", ...}` is appended after the
//!   PTY write succeeds and the in-memory id is recorded.
//!
//! On a retry, a `delivering` record with no later `applied` record means the
//! previous attempt may or may not have delivered the bytes. The retry
//! resolves as [`DeliveryCheck::OutcomeUnknown`] — the caller must surface it
//! for human review and must NEVER re-deliver. A crash before the
//! write-ahead record is fsync'd leaves no trace, so that retry is safe to
//! deliver (nothing could have reached the PTY yet).

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use super::RecentWriteIds;
use crate::state::current_timestamp_ms;

/// Name of the per-session delivery log inside the session directory.
pub const WRITE_DELIVERIES_FILE: &str = "write-deliveries.jsonl";

/// Compaction threshold: rewrite the log keeping only the latest record per
/// write_id once it grows past this many lines.
const COMPACT_LINES: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum DeliveryState {
    Delivering,
    Applied,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeliveryRecord {
    write_id: String,
    state: DeliveryState,
    ts_ms: u64,
}

/// Result of the pre-delivery idempotency check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryCheck {
    /// Safe to deliver: no prior attempt is on record. The caller must have
    /// already fsync'd the `delivering` record (done inside
    /// [`check_write_delivery`]).
    Proceed,
    /// This write_id is already known-applied (in memory or durably).
    /// Skip delivery; report success without touching the PTY.
    AlreadyApplied,
    /// A `delivering` record exists with no matching `applied`: the bytes may
    /// or may not have reached the PTY. Do NOT re-deliver; surface for review.
    OutcomeUnknown,
}

/// Fault-injection hook for tests: when armed, [`commit_write_applied`]
/// panics after the PTY delivery but before `record_applied`, simulating the
/// crash/OOM window Q1 closes. Never armed in production.
static FAULT_AFTER_DELIVERY: AtomicBool = AtomicBool::new(false);

/// Arm or disarm the post-delivery fault hook. Test-only.
#[doc(hidden)]
#[cfg(test)]
pub fn set_fault_after_delivery(armed: bool) {
    FAULT_AFTER_DELIVERY.store(armed, Ordering::SeqCst);
}

fn fault_after_delivery() -> bool {
    FAULT_AFTER_DELIVERY.load(Ordering::SeqCst)
}

fn log_path(session_dir: &Path) -> std::path::PathBuf {
    session_dir.join(WRITE_DELIVERIES_FILE)
}

fn append_record(session_dir: &Path, record: &DeliveryRecord) -> io::Result<()> {
    fs::create_dir_all(session_dir)?;
    let path = log_path(session_dir);
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    let mut line =
        serde_json::to_vec(record).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    line.push(b'\n');
    file.write_all(&line)?;
    file.sync_all()?;
    maybe_compact(session_dir)?;
    Ok(())
}

/// Scan the log for `write_id`. Returns the latest state on record, or `None`
/// when the write_id was never seen.
fn latest_state(session_dir: &Path, write_id: &str) -> io::Result<Option<DeliveryState>> {
    let path = log_path(session_dir);
    let file = match fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut latest = None;
    for line in BufReader::new(file).lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record: DeliveryRecord = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            // A torn tail line from a crash mid-append is not a record.
            Err(_) => continue,
        };
        if record.write_id == write_id {
            latest = Some(record.state);
        }
    }
    Ok(latest)
}

/// Rewrite the log keeping only the latest record per write_id, once it grows
/// past [`COMPACT_LINES`] lines. Keeps the file bounded across a long-lived
/// session; the latest state per write_id is all the check needs.
fn maybe_compact(session_dir: &Path) -> io::Result<()> {
    let path = log_path(session_dir);
    let content = match fs::read(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    let lines: Vec<&[u8]> = content
        .split(|&b| b == b'\n')
        .filter(|l| !l.trim_ascii().is_empty())
        .collect();
    if lines.len() <= COMPACT_LINES {
        return Ok(());
    }
    let mut latest: std::collections::HashMap<String, DeliveryRecord> =
        std::collections::HashMap::new();
    for line in lines {
        if let Ok(record) = serde_json::from_slice::<DeliveryRecord>(line) {
            latest.insert(record.write_id.clone(), record);
        }
    }
    let tmp = session_dir.join(format!("{WRITE_DELIVERIES_FILE}.tmp"));
    {
        let mut out = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)?;
        for record in latest.values() {
            let mut line = serde_json::to_vec(record)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            line.push(b'\n');
            out.write_all(&line)?;
        }
        out.sync_all()?;
    }
    fs::rename(&tmp, &path)?;
    // Fsync the directory entry so the rename survives a crash.
    if let Ok(dir) = fs::File::open(session_dir) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Pre-delivery idempotency check. Must run under the runtime lock that
/// serializes check → PTY write → commit.
///
/// On [`DeliveryCheck::Proceed`] the `delivering` record has already been
/// fsync'd: a crash from this point on leaves a durable trace, so the retry
/// can never mistake it for a never-attempted write.
pub fn check_write_delivery(
    recent: &mut RecentWriteIds,
    session_dir: &Path,
    write_id: &str,
) -> io::Result<DeliveryCheck> {
    if recent.contains(write_id) {
        return Ok(DeliveryCheck::AlreadyApplied);
    }
    match latest_state(session_dir, write_id)? {
        Some(DeliveryState::Applied) => {
            // Durably applied but evicted from the bounded in-memory set:
            // still applied. Re-populate memory so the fast path hits next.
            recent.record_applied(write_id);
            Ok(DeliveryCheck::AlreadyApplied)
        }
        Some(DeliveryState::Delivering) => Ok(DeliveryCheck::OutcomeUnknown),
        None => {
            append_record(
                session_dir,
                &DeliveryRecord {
                    write_id: write_id.to_string(),
                    state: DeliveryState::Delivering,
                    ts_ms: current_timestamp_ms(),
                },
            )?;
            Ok(DeliveryCheck::Proceed)
        }
    }
}

/// Post-delivery commit: record the write_id in memory and durably.
///
/// The fault-injection hook fires here — after the caller delivered the bytes
/// to the PTY, before anything is recorded — so tests can prove the retry
/// path never re-delivers.
pub fn commit_write_applied(
    recent: &mut RecentWriteIds,
    session_dir: &Path,
    write_id: &str,
) -> io::Result<()> {
    if fault_after_delivery() {
        panic!("injected fault: crash between PTY delivery and record_applied");
    }
    recent.record_applied(write_id);
    append_record(
        session_dir,
        &DeliveryRecord {
            write_id: write_id.to_string(),
            state: DeliveryState::Applied,
            ts_ms: current_timestamp_ms(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that arm the process-global fault hook: without this,
    /// a concurrently running test calling `commit_write_applied` could
    /// observe another test's armed hook.
    static HOOK_SERIAL: Mutex<()> = Mutex::new(());

    fn test_session_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("up-wdel-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let sdir = dir.join("sess");
        fs::create_dir_all(&sdir).unwrap();
        sdir
    }

    #[test]
    fn normal_write_proceeds_then_is_idempotent() {
        let _serial = HOOK_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let sdir = test_session_dir("normal");
        let mut recent = RecentWriteIds::default();
        assert_eq!(
            check_write_delivery(&mut recent, &sdir, "w1").unwrap(),
            DeliveryCheck::Proceed
        );
        commit_write_applied(&mut recent, &sdir, "w1").unwrap();
        // Retry hits the in-memory set.
        assert_eq!(
            check_write_delivery(&mut recent, &sdir, "w1").unwrap(),
            DeliveryCheck::AlreadyApplied
        );
        // A fresh process (empty memory) still sees the durable applied record.
        let mut fresh = RecentWriteIds::default();
        assert_eq!(
            check_write_delivery(&mut fresh, &sdir, "w1").unwrap(),
            DeliveryCheck::AlreadyApplied
        );
    }

    #[test]
    fn crash_between_delivery_and_commit_resolves_outcome_unknown_never_redelivers() {
        let _serial = HOOK_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let sdir = test_session_dir("crash");
        let mut recent = RecentWriteIds::default();
        let write_id = "w-crash-1";

        // First attempt: pre-delivery check passes and fsyncs `delivering`.
        assert_eq!(
            check_write_delivery(&mut recent, &sdir, write_id).unwrap(),
            DeliveryCheck::Proceed
        );
        // Bytes reach the PTY here (the caller owns that step)...
        // ...then the crash lands before anything is recorded.
        set_fault_after_delivery(true);
        let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            commit_write_applied(&mut recent, &sdir, write_id).unwrap();
        }));
        set_fault_after_delivery(false);
        assert!(crashed.is_err(), "fault hook must panic like a crash");

        // In-memory was never updated: without the durable log this retry
        // would re-deliver (the Q1 bug).
        assert!(!recent.contains(write_id));

        // Retry: the durable `delivering` record (no `applied`) resolves as
        // OutcomeUnknown — the caller must surface it, never re-deliver.
        assert_eq!(
            check_write_delivery(&mut recent, &sdir, write_id).unwrap(),
            DeliveryCheck::OutcomeUnknown
        );
        // And it stays OutcomeUnknown on every subsequent retry.
        assert_eq!(
            check_write_delivery(&mut recent, &sdir, write_id).unwrap(),
            DeliveryCheck::OutcomeUnknown
        );
    }

    #[test]
    fn torn_tail_line_is_ignored() {
        let sdir = test_session_dir("torn");
        fs::write(
            log_path(&sdir),
            b"{\"write_id\":\"w9\",\"state\":\"delivering\",\"ts_ms\":1}\n{\"write_id\":\"w9\",\"sta",
        )
        .unwrap();
        let mut recent = RecentWriteIds::default();
        // The torn tail is skipped; the complete `delivering` record governs.
        assert_eq!(
            check_write_delivery(&mut recent, &sdir, "w9").unwrap(),
            DeliveryCheck::OutcomeUnknown
        );
    }
}
