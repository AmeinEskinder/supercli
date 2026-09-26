//! Group commit for grant writes (Phase 13 v2).
//!
//! ## Problem
//!
//! Each `persist_grant` does:
//! 1. Append to grant-audit.jsonl + fsync
//! 2. Write grants.json (temp + fsync + rename + fsync dir)
//!
//! With N concurrent writers, that's N fsyncs to each file. The disk IOPS
//! becomes the bottleneck; throughput does not scale with concurrency.
//!
//! ## Solution: Group Commit
//!
//! One writer thread drains a queue of pending grant writes and does ONE
//! fsync per batch:
//! - N callers submit their (audit entry, grant mutation) to the queue
//! - Writer drains the queue, appends all N audit entries, fsyncs ONCE
//! - Writer applies all N mutations to the grants map, writes once, fsyncs ONCE
//! - Each caller is acknowledged only after its batch's fsync completes
//!
//! Durability per request is unchanged (each caller waits for fsync), but
//! throughput scales with concurrency because N requests share 2 fsyncs
//! instead of 2N.

use serde_json::{Map, Value};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Condvar, Mutex};

/// A pending grant write.
struct PendingGrant {
    /// Audit entry data (actor, scope, tool, grant_key)
    audit_actor: String,
    audit_scope: String,
    audit_tool: String,
    audit_grant_key: String,
    /// Grant mutation to apply (kind, caller, target)
    kind: String,
    caller: String,
    target: Option<String>,
    /// Channel to signal completion (caller waits on this).
    ack: std::sync::mpsc::Sender<Result<(), String>>,
}

/// Global grant write queue.
struct GrantQueue {
    queue: Mutex<VecDeque<PendingGrant>>,
    condvar: Condvar,
    /// Handle to the writer thread. If the writer dies, `submit` fails fast
    /// instead of hanging forever. The handle is set once at init.
    writer_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// Explicit home directory for this queue's writer. Captured at
    /// construction so the writer does not race on the process-global
    /// SUPERCLI_HOME env var (see concurrent_grouped_writes test).
    home: PathBuf,
}

impl GrantQueue {
    fn new(home: PathBuf) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            condvar: Condvar::new(),
            writer_handle: Mutex::new(None),
            home,
        }
    }

    /// Submit a grant write and wait for it to be committed (fsynced).
    ///
    /// Blocks until the writer acknowledges the batch as durable, or until
    /// the writer thread dies (the ack channel disconnects → error). There
    /// is deliberately no timeout: a timeout would create ambiguous
    /// acknowledgment (the write might still become durable after the
    /// caller gave up). Callers are acknowledged only after their batch's
    /// fsync completes.
    ///
    /// If the writer thread has died, fails immediately with an error
    /// (never hangs). The caller must treat this as Ambiguous (the write
    /// may or may not have been persisted).
    fn submit(
        &self,
        audit_actor: String,
        audit_scope: String,
        audit_tool: String,
        audit_grant_key: String,
        kind: String,
        caller: String,
        target: Option<String>,
    ) -> Result<(), String> {
        // Fail fast if the writer is dead. Otherwise we'd enqueue and hang
        // forever waiting for an ack that will never come.
        {
            let handle_guard = self.writer_handle.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(handle) = handle_guard.as_ref() {
                if handle.is_finished() {
                    return Err(
                        "grant writer thread died; cannot persist grant (Ambiguous)".to_string()
                    );
                }
            }
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let pending = PendingGrant {
            audit_actor,
            audit_scope,
            audit_tool,
            audit_grant_key,
            kind,
            caller,
            target,
            ack: tx,
        };

        // Enqueue.
        {
            let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
            q.push_back(pending);
        }
        self.condvar.notify_one();

        // Wait for the writer to commit. Disconnect means the writer died.
        rx.recv()
            .map_err(|_| "grant writer died before acknowledging".to_string())?
    }

    /// Drain all pending grants (called by writer thread).
    fn drain(&self) -> Vec<PendingGrant> {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        let mut batch = Vec::new();
        while let Some(p) = q.pop_front() {
            batch.push(p);
        }
        batch
    }

    /// Wait for work (called by writer thread). The caller must have drained
    /// first (see writer_loop) so a notify sent before this wait is not lost.
    fn wait_for_work(&self) {
        let q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = self.condvar.wait(q).unwrap_or_else(|e| e.into_inner());
    }
}

static GRANT_QUEUE: std::sync::OnceLock<GrantQueue> = std::sync::OnceLock::new();

fn grant_queue() -> &'static GrantQueue {
    GRANT_QUEUE.get_or_init(|| {
        let q = GrantQueue::new(crate::app_paths::supercli_home());
        // Spawn the writer thread and store the handle for liveness checks.
        // If the writer dies, submit() fails fast instead of hanging.
        let handle = std::thread::spawn(|| {
            writer_loop();
        });
        {
            let mut guard = q.writer_handle.lock().unwrap_or_else(|e| e.into_inner());
            *guard = Some(handle);
        }
        q
    })
}

/// The writer thread: drains the queue and commits batches.
///
/// A panicking batch must not kill the writer (that would hang all future
/// submitters): each batch commit is panic-contained, and a failed batch
/// acks all its callers with an error before the loop continues.
fn writer_loop() {
    writer_loop_for(grant_queue())
}

/// Writer loop for an explicit queue (used by tests with isolated homes).
/// The queue's `home` is used for all writes, never the process-global env.
fn writer_loop_for(queue: &GrantQueue) {
    loop {
        // Drain first: if a submitter notified while we were committing the
        // previous batch, the queue is already non-empty and we must not
        // wait (a condvar notify sent before wait is lost).
        let batch = queue.drain();
        if batch.is_empty() {
            queue.wait_for_work();
            continue;
        }

        // Panic-contain the batch commit so one bad batch cannot take down
        // the writer and hang all future submitters.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            commit_batch_at(batch, &queue.home)
        }));
        match result {
            Ok(r) => {
                let _ = r;
            }
            Err(_) => {
                // commit_batch panicked: the batch (and its ack Senders) was
                // dropped during unwind, so every caller in that batch sees a
                // channel disconnect → "grant writer died" error. The writer
                // itself survives and keeps draining.
                eprintln!("grant writer: batch commit panicked; batch callers errored");
            }
        }
    }
}

/// Commit a batch of grants with group fsync.
///
/// 1. Append all audit entries (one file append, one fsync).
/// 2. Apply all grant mutations to the map.
/// 3. Write grants.json once (temp + fsync + rename + fsync dir).
/// 4. Ack all callers.
/// Same as `commit_batch` but with an explicit home directory.
fn commit_batch_at(batch: Vec<PendingGrant>, home: &std::path::Path) -> Result<(), String> {
    if batch.is_empty() {
        return Ok(());
    }

    // Step 1: Append all audit entries with a SINGLE fsync (group commit).
    // Write-ahead: the audit batch is durable before grants.json is written.
    let items: Vec<(&str, &str, &str, &str)> = batch
        .iter()
        .map(|p| {
            (
                p.audit_actor.as_str(),
                p.audit_scope.as_str(),
                p.audit_tool.as_str(),
                p.audit_grant_key.as_str(),
            )
        })
        .collect();
    if let Err(e) = crate::grant_audit::record_grants_created_batch_at(home, &items) {
        // Ack all with error.
        for p in batch {
            let _ = p.ack.send(Err(format!("audit failed: {e}")));
        }
        return Err(e);
    }

    // Step 2: Apply all mutations to the grants map (in memory).
    // We use edit_grants with a closure that applies all mutations.
    let mutations: Vec<(String, String, Option<String>)> = batch
        .iter()
        .map(|p| (p.kind.clone(), p.caller.clone(), p.target.clone()))
        .collect();

    let edit_result = crate::grant_store::edit_grants_at(home, |root| {
        for (kind, caller, target) in &mutations {
            apply_grant_mutation(root, kind, caller, target.as_deref());
        }
        Ok(())
    });

    // Step 3: Ack all callers.
    let ack_result = match edit_result {
        Ok(()) => Ok(()),
        Err(e) => Err(e.to_string()),
    };
    for p in batch {
        let _ = p.ack.send(ack_result.clone());
    }

    ack_result.map_err(|e| e)
}

/// Apply a single grant mutation to the map (same logic as persist_grant).
pub(crate) fn apply_grant_mutation(
    root: &mut Map<String, Value>,
    kind: &str,
    caller: &str,
    target: Option<&str>,
) {
    match kind {
        "write" => {
            let Some(target) = target else { return };
            let map = root
                .entry("mcp_write_approvals")
                .or_insert_with(|| serde_json::json!({}));
            if let Some(list) = map
                .as_object_mut()
                .map(|m| {
                    m.entry(caller.to_string())
                        .or_insert_with(|| serde_json::json!([]))
                })
                .and_then(|v| v.as_array_mut())
            {
                if !list.iter().any(|v| v.as_str() == Some(target)) {
                    list.push(target.into());
                }
            }
        }
        "browser" | "computer" => {
            let key = if kind == "browser" {
                "browser_approvals"
            } else {
                "computer_approvals"
            };
            let list = root.entry(key).or_insert_with(|| serde_json::json!([]));
            if let Some(array) = list.as_array_mut() {
                if !array.iter().any(|v| v.as_str() == Some(caller)) {
                    array.push(caller.into());
                }
            }
        }
        "app-open" => {
            let Some(app_id) = target else { return };
            let map = root
                .entry("mcp_app_open_approvals")
                .or_insert_with(|| serde_json::json!({}));
            if let Some(list) = map
                .as_object_mut()
                .map(|m| {
                    m.entry(caller.to_string())
                        .or_insert_with(|| serde_json::json!([]))
                })
                .and_then(|v| v.as_array_mut())
            {
                if !list.iter().any(|v| v.as_str() == Some(app_id)) {
                    list.push(app_id.into());
                }
            }
        }
        "connector" => {
            // target is "connector_name:tool_name" (see persist_connector_grant_grouped).
            let Some(target) = target else { return };
            let (connector, tool) = match target.split_once(':') {
                Some(pair) => pair,
                None => return,
            };
            let map = root
                .entry("mcp_connector_approvals")
                .or_insert_with(|| serde_json::json!({}));
            if let Some(list) = map
                .as_object_mut()
                .map(|m| {
                    m.entry(caller.to_string())
                        .or_insert_with(|| serde_json::json!([]))
                })
                .and_then(|v| v.as_array_mut())
            {
                let entry = serde_json::json!({"connector": connector, "tool": tool});
                if !list.iter().any(|v| v == &entry) {
                    list.push(entry);
                }
            }
        }
        _ => {}
    }
}

/// Submit a grant for group commit. Blocks until the grant is durably committed.
pub fn persist_grant_grouped(
    kind: &str,
    caller: &str,
    target: Option<&str>,
    answered_by: Option<&str>,
) -> Result<(), String> {
    let grant_key = match kind {
        "write" => target.map(|t| {
            format!(
                "write:{}:{}",
                crate::grant_store::escape_component(caller),
                crate::grant_store::escape_component(t)
            )
        }),
        "browser" | "computer" => Some(format!(
            "{kind}:{}",
            crate::grant_store::escape_component(caller)
        )),
        "app-open" => target.map(|t| {
            format!(
                "app-open:{}:{}",
                crate::grant_store::escape_component(caller),
                crate::grant_store::escape_component(t)
            )
        }),
        _ => None,
    };

    let (audit_actor, audit_scope, audit_tool, audit_grant_key) = if let Some(key) = grant_key {
        // answered_by is Some in production (the mobile endpoint passes
        // "paired-device"; the TUI passes its own id). None is defensive:
        // "unknown" is honest, unlike "policy:Allow" which would falsely
        // claim a policy made the decision.
        let actor = answered_by
            .map(|d| format!("human:{d}"))
            .unwrap_or_else(|| "unknown".to_string());
        (actor, kind.to_string(), kind.to_string(), key)
    } else {
        // No audit needed for unknown kinds (should not happen).
        return Ok(());
    };

    grant_queue().submit(
        audit_actor,
        audit_scope,
        audit_tool,
        audit_grant_key,
        kind.to_string(),
        caller.to_string(),
        target.map(|s| s.to_string()),
    )
}

/// Submit a connector grant for group commit. Blocks until durably committed.
///
/// Key components are %-escaped (see `grant_store::escape_component`).
pub fn persist_connector_grant_grouped(
    caller: &str,
    connector: &str,
    tool: &str,
    answered_by: Option<&str>,
) -> Result<(), String> {
    // The storage path splits "connector:tool" on ':'; reject colons here
    // (fail closed) rather than storing a corrupted grant. The audit key
    // uses %-escaping so it stays unambiguous regardless.
    if connector.contains(':') || tool.contains(':') {
        return Err("connector/tool names must not contain ':'".to_string());
    }
    let grant_key = format!(
        "connector:{}:{}:{}",
        crate::grant_store::escape_component(caller),
        crate::grant_store::escape_component(connector),
        crate::grant_store::escape_component(tool)
    );
    let actor = answered_by
        .map(|d| format!("human:{d}"))
        .unwrap_or_else(|| "unknown".to_string());

    grant_queue().submit(
        actor,
        "connector".to_string(),
        "connector".to_string(),
        grant_key,
        "connector".to_string(),
        caller.to_string(),
        Some(format!("{connector}:{tool}")),
    )
}

/// Direct (non-grouped) grant persist: the pre-group-commit path.
///
/// Audit write-ahead with per-call fsync, then a direct `edit_grants`.
/// Used for the "before" benchmark comparison and as a fallback.
/// NOTE: under concurrency the audit chain can fork here (read-tail +
/// append is not serialized); the grouped writer is the sole production
/// appender and does not have this race.
/// The direct (non-grouped) grant persistence path has been REMOVED from
/// production code (Phase 13 v2, per security review).
///
/// The direct path allowed concurrent callers to interleave audit-chain
/// appends, forking the hash chain (306/321 errors at c8/c16 in benchmarks).
/// All production grant writes MUST go through `persist_grant_grouped`,
/// which serializes via the single writer thread AND the cross-process
/// audit flock.
///
/// The benchmark example (`s2_bench.rs`) contains an inlined copy of the
/// old direct logic for before/after comparison only. It is not used in
/// production.
#[deprecated(
    note = "REMOVED: Use persist_grant_grouped. Direct path forks audit chain under concurrency."
)]
pub fn persist_grant_direct(
    _kind: &str,
    _caller: &str,
    _target: Option<&str>,
    _answered_by: Option<&str>,
) -> Result<(), String> {
    Err("persist_grant_direct removed: use persist_grant_grouped".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn concurrent_grouped_writes_do_not_fork_audit_chain() {
        // Phase 13 v2 (a): Two concurrent writers must not fork the audit
        // chain. This test fails if the chain can be forked (duplicate
        // prev_hash values).
        //
        // Isolation: uses an explicit per-test home directory passed to a
        // dedicated GrantQueue, NOT the process-global SUPERCLI_HOME env var.
        // (The old version used set_var without the TEST_SUPERCLI_HOME_LOCK,
        // racing with other tests that mutate the env var.)
        use std::sync::Arc;
        let dir = std::env::temp_dir().join(format!(
            "grant-fork-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Create a dedicated queue with explicit home (not the global static).
        let queue = Arc::new(GrantQueue::new(dir.clone()));
        // Spawn the writer thread for this queue.
        let writer_queue = Arc::clone(&queue);
        let writer_handle = std::thread::spawn(move || {
            writer_loop_for(&writer_queue);
        });
        {
            let mut guard = queue.writer_handle.lock().unwrap_or_else(|e| e.into_inner());
            *guard = Some(writer_handle);
        }

        // Spawn 8 threads, each doing 50 grouped writes concurrently.
        let mut handles = vec![];
        for t in 0..8 {
            let q = Arc::clone(&queue);
            handles.push(std::thread::spawn(move || {
                for i in 0..50 {
                    let caller = format!("test-caller-{t}-{i}");
                    let target = format!("test-target-{t}-{i}");
                    let grant_key = format!(
                        "write:{}:{}",
                        crate::grant_store::escape_component(&caller),
                        crate::grant_store::escape_component(&target)
                    );
                    q.submit(
                        "human:test-device".to_string(),
                        "write".to_string(),
                        "write".to_string(),
                        grant_key,
                        "write".to_string(),
                        caller,
                        Some(target),
                    )
                    .expect("grouped write must succeed");
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        // Verify: the audit chain must have no forks.
        // A fork means two entries with the same prev_hash (both pointing
        // to the same parent).
        let audit_path = dir.join("grant-audit.jsonl");
        let content = std::fs::read_to_string(&audit_path).unwrap();
        let mut seen_prev_hashes = HashSet::new();
        let mut count = 0;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let entry: serde_json::Value = serde_json::from_str(line).unwrap();
            let prev_hash = entry["prev_hash"].as_str().unwrap();
            // Genesis prev_hash "0" appears once; all others must be unique.
            if prev_hash != "0" {
                assert!(
                    seen_prev_hashes.insert(prev_hash.to_string()),
                    "FORK DETECTED: duplicate prev_hash {prev_hash} — chain is forked!"
                );
            }
            count += 1;
        }
        assert_eq!(count, 400, "expected 400 audit entries");

        // Also verify the chain cryptographically (explicit home, not env).
        crate::grant_audit::verify_grant_audit_at(&dir).expect("chain must verify");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn direct_path_is_removed() {
        // Phase 13 v2 (a): The direct path must be gone from production.
        // This test ensures persist_grant_direct returns an error.
        let result = persist_grant_direct("write", "a", Some("b"), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("removed"));
    }

    #[test]
    fn submit_fails_fast_if_writer_dead() {
        // Phase 13 v2 (b): If the writer thread dies, submit() must fail
        // immediately with an error, not hang forever waiting for ack.
        //
        // We test this by creating a GrantQueue with a dead (finished)
        // writer handle. The submit should return an error, not block.
        let q = GrantQueue::new(std::env::temp_dir());
        // Spawn a thread that exits immediately, giving us a finished handle.
        let dead_handle = std::thread::spawn(|| {
            // Exit immediately.
        });
        // Wait for it to finish, then check is_finished (without joining,
        // which would move the handle).
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(dead_handle.is_finished(), "thread should have finished");
        {
            let mut guard = q.writer_handle.lock().unwrap();
            *guard = Some(dead_handle);
        }
        // Now submit should fail fast (not hang).
        let start = std::time::Instant::now();
        let result = q.submit(
            "human:test".to_string(),
            "write".to_string(),
            "write".to_string(),
            "write:a:b".to_string(),
            "write".to_string(),
            "a".to_string(),
            Some("b".to_string()),
        );
        let elapsed = start.elapsed();
        assert!(result.is_err(), "submit must fail if writer is dead");
        let err = result.unwrap_err();
        assert!(
            err.contains("died") || err.contains("Ambiguous"),
            "error should indicate writer death: {err}"
        );
        // Must fail fast, not hang. 5 seconds is generous; it should be ms.
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "submit hung for {elapsed:?} instead of failing fast"
        );
    }

    #[test]
    fn mid_batch_panic_errors_all_waiters_no_hang_no_false_ack() {
        // Phase 13 v3 (1): Callers already enqueued when the writer dies
        // mid-batch must each get an error (→ Ambiguous), none may hang,
        // and nothing may be acked as success unless its fsync completed.
        //
        // Simulates: N callers submit → writer drains batch → writer panics
        // AFTER the batch write but BEFORE fsync/ack (we simulate by
        // draining and dropping the batch without acking).
        //
        // Each caller must see a channel disconnect → error, not hang,
        // not Ok.
        use std::sync::{Arc, Barrier};

        let q = Arc::new(GrantQueue::new(std::env::temp_dir()));
        // Mark writer as alive (so submit doesn't fail fast).
        {
            let alive_handle = std::thread::spawn(|| {
                // Sleep long enough for the test to complete.
                std::thread::sleep(std::time::Duration::from_secs(30));
            });
            let mut guard = q.writer_handle.lock().unwrap();
            *guard = Some(alive_handle);
        }

        const N: usize = 10;
        let barrier = Arc::new(Barrier::new(N + 1));
        let mut handles = vec![];

        for i in 0..N {
            let q_clone = Arc::clone(&q);
            let b_clone = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                // Wait for all submitters to be ready, then submit together.
                b_clone.wait();
                let start = std::time::Instant::now();
                let result = q_clone.submit(
                    format!("human:test-{i}"),
                    "write".to_string(),
                    "write".to_string(),
                    format!("write:caller-{i}:target-{i}"),
                    "write".to_string(),
                    format!("caller-{i}"),
                    Some(format!("target-{i}")),
                );
                let elapsed = start.elapsed();
                (result, elapsed)
            }));
        }

        // Wait for all submitters to be ready, then let them submit.
        barrier.wait();
        // Give them time to enqueue.
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Simulate writer death mid-batch: drain the batch (as the writer
        // would), then DROP it without acking (simulating panic before
        // fsync/ack). The PendingGrant structs are dropped, their ack
        // Senders are dropped, and each caller's recv() gets disconnect.
        let batch = q.drain();
        assert_eq!(batch.len(), N, "all {N} callers should be enqueued");
        // Drop the batch without acking — this is the "panic before fsync".
        drop(batch);

        // Each caller must get an error (not hang, not Ok).
        for (idx, h) in handles.into_iter().enumerate() {
            let (result, elapsed) = h.join().expect("submitter thread panicked");
            // Must not hang: 10 seconds is generous.
            assert!(
                elapsed < std::time::Duration::from_secs(10),
                "caller {idx} hung for {elapsed:?} instead of erroring"
            );
            // Must be an error (Ambiguous), never Ok.
            assert!(
                result.is_err(),
                "caller {idx} got Ok, but writer died before fsync — must be error (Ambiguous)"
            );
            let err = result.unwrap_err();
            // The error should indicate writer death/disconnect.
            assert!(
                err.contains("died") || err.contains("disconnect") || err.contains("Ambiguous"),
                "caller {idx} error should indicate writer death: {err}"
            );
        }
    }
}
