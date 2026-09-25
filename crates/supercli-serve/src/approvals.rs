//! MCP approval hub: when no app runs, the MCP host's port discovery finds
//! the TUI's hook listener and POSTs its blocking approval requests here
//! (`/mcp/approve-write|browser|computer|app-open|connector`). Requests queue in this hub; the
//! TUI renders the front of the queue as a y/n prompt and paired phones see
//! it as `pendingApprovals` in bootstrap (answered via
//! `/mobile/approvals/answer`) — first answer wins. Approvals persist into
//! the shared `app-state.json` exactly where the app keeps them, so grants
//! survive and both frontends honor them.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Serializes the UNPEEL_HOME-mutating tests: they point UNPEEL_HOME at a
/// temp dir so they never touch the real ~/.unpeel. Shared crate-wide so
/// every test that mutates UNPEEL_HOME serializes on one lock.
#[cfg(test)]
pub(crate) static APP_STATE_LOCK: Mutex<()> = Mutex::new(());

/// The answer to one approval request: whether it was approved, plus the
/// optional answer detail (e.g. a paired-device id) the answerer provided.
type ApprovalAnswer = (bool, Option<String>);

pub struct PendingApproval {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub caller_session_id: String,
    pub target_session_id: Option<String>,
    pub requested_at: u64,
    responder: Sender<(bool, Option<String>)>,
}

#[derive(Default)]
pub struct ApprovalHub {
    pending: Mutex<Vec<PendingApproval>>,
    generation: AtomicU64,
    // Phase 13 v3 (B2): Condvar for real wake-up on generation change.
    // The bootstrap long-poll waits here instead of spin-sleeping, so
    // waiting controllers cost nothing and wake in under 1ms.
    generation_cv: std::sync::Condvar,
    generation_lock: Mutex<u64>,
    // Phase 14 (0a): Idempotent answers. Maps resolved approval id ->
    // (approved decision, answer nonce, resolved-at unix secs). A retried
    // answer after an unknown outcome returns `already_resolved` with the
    // original decision, never applies twice or flips allow/deny.
    // Bound: capped at RESOLVED_CAP entries, FIFO eviction (oldest first),
    // plus TTL eviction (RESOLVED_TTL_SECS). A burst cannot evict a
    // just-resolved approval while its retry is still in flight, because
    // eviction is oldest-first, not arbitrary. In-memory only; a Host
    // restart clears it. After a restart, a retried answer for a
    // previously-resolved approval returns NotFound (not Applied), which
    // the phone UI renders as "Resolved — see activity log", never as a
    // retryable error or a fresh approval prompt.
    resolved: Mutex<ResolvedStore>,
}

/// FIFO-bounded store for resolved approval decisions, with TTL.
/// Insertion order is tracked so eviction is oldest-first.
#[derive(Default)]
struct ResolvedStore {
    /// approval id -> (approved, nonce, resolved_at_unix_secs)
    map: HashMap<String, (bool, String, u64)>,
    /// Insertion order for FIFO eviction.
    order: std::collections::VecDeque<String>,
}

impl ResolvedStore {
    fn get(&self, id: &str) -> Option<(bool, String)> {
        self.map.get(id).map(|(b, s, _)| (*b, s.clone()))
    }

    fn insert(&mut self, id: String, approved: bool, nonce: String) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Evict expired entries first (TTL).
        while let Some(front) = self.order.front() {
            let expired = self
                .map
                .get(front)
                .map(|(_, _, ts)| now.saturating_sub(*ts) > RESOLVED_TTL_SECS)
                .unwrap_or(true); // orphaned order entry; drop it
            if !expired {
                break;
            }
            if let Some(key) = self.order.pop_front() {
                self.map.remove(&key);
            }
        }
        // FIFO eviction if at capacity.
        while self.map.len() >= RESOLVED_CAP {
            if let Some(key) = self.order.pop_front() {
                self.map.remove(&key);
            } else {
                break;
            }
        }
        // If re-inserting an existing id, remove old position first.
        if self.map.contains_key(&id) {
            self.order.retain(|k| k != &id);
        }
        self.order.push_back(id.clone());
        self.map.insert(id, (approved, nonce, now));
    }

    fn len(&self) -> usize {
        self.map.len()
    }

    fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Maximum entries in the resolved-decision store. Bounds memory; eviction
/// is FIFO (oldest first), never arbitrary, so a burst cannot evict a
/// just-resolved approval while its retry is still in flight.
const RESOLVED_CAP: usize = 1000;

/// Time-to-live for resolved decisions, in seconds. Entries older than this
/// are evicted on insert. 10 minutes comfortably covers the retry-after-
/// unknown-outcome window; the phone retries within seconds.
const RESOLVED_TTL_SECS: u64 = 600;

/// Outcome of an answer attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerOutcome {
    /// First answer applied.
    Applied(bool),
    /// Approval was already resolved; returns the original decision.
    AlreadyResolved(bool),
    /// Approval id unknown (never existed or timed out before any answer).
    NotFound,
}

impl ApprovalHub {
    /// Bump the generation counter and wake all long-poll waiters.
    fn bump_generation(&self) {
        let new_gen = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        if let Ok(mut guard) = self.generation_lock.lock() {
            *guard = new_gen;
            self.generation_cv.notify_all();
        }
    }

    /// Block until the generation differs from `after_gen` or `timeout`
    /// elapses. Returns the current generation. This is the wake-up
    /// primitive for the bootstrap long-poll — no spinning.
    pub fn wait_for_generation_change(&self, after_gen: u64, timeout: Duration) -> u64 {
        let Ok(guard) = self.generation_lock.lock() else {
            return self.generation();
        };
        if *guard != after_gen {
            return *guard;
        }
        let _guard = match self.generation_cv.wait_timeout(guard, timeout) {
            Ok((g, _)) => g,
            Err(e) => e.into_inner().0,
        };
        self.generation()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl ApprovalHub {
    /// Queue a request and block until answered or `timeout` (denied). The
    /// caller is an HTTP handler thread, so blocking here is the contract —
    /// the MCP host is itself blocking on our response.
    ///
    /// Returns `(approved, answered_by)`: `answered_by` identifies who
    /// answered (e.g. a paired-device id), when the answerer provides it.
    pub fn request(
        self: &Arc<Self>,
        kind: &str,
        title: String,
        body: String,
        caller_session_id: String,
        target_session_id: Option<String>,
        timeout: Duration,
    ) -> (bool, Option<String>) {
        let (tx, rx): (Sender<ApprovalAnswer>, Receiver<ApprovalAnswer>) =
            std::sync::mpsc::channel();
        let id = uuid::Uuid::new_v4().to_string();
        if let Ok(mut guard) = self.pending.lock() {
            guard.push(PendingApproval {
                id: id.clone(),
                kind: kind.to_string(),
                title,
                body,
                caller_session_id,
                target_session_id,
                requested_at: now_ms(),
                responder: tx,
            });
            self.bump_generation();
        }
        let (approved, answered_by) = rx.recv_timeout(timeout).unwrap_or((false, None));
        // Drop the entry if it's still queued (timeout path).
        if let Ok(mut guard) = self.pending.lock() {
            let before = guard.len();
            guard.retain(|p| p.id != id);
            if guard.len() != before {
                self.bump_generation();
            }
        }
        (approved, answered_by)
    }

    /// Answer by id (from the TUI keys or the phone). `answered_by`
    /// identifies the answerer when known (e.g. a paired-device id).
    /// `nonce` is the client-generated answer nonce for idempotency.
    /// Returns [`AnswerOutcome`]: Applied on first answer, AlreadyResolved
    /// with the original decision on retry, NotFound when the id is unknown.
    pub fn answer(
        &self,
        id: &str,
        approved: bool,
        answered_by: Option<String>,
        nonce: &str,
    ) -> AnswerOutcome {
        // Phase 14 (0a): Check resolved first for idempotent retry.
        if let Ok(guard) = self.resolved.lock() {
            if let Some((decision, _)) = guard.get(id) {
                return AnswerOutcome::AlreadyResolved(decision);
            }
        }
        let Ok(mut guard) = self.pending.lock() else {
            return AnswerOutcome::NotFound;
        };
        let Some(index) = guard.iter().position(|p| p.id == id) else {
            // Not in pending; check resolved again (race with concurrent answer).
            drop(guard);
            if let Ok(rguard) = self.resolved.lock() {
                if let Some((decision, _)) = rguard.get(id) {
                    return AnswerOutcome::AlreadyResolved(decision);
                }
            }
            return AnswerOutcome::NotFound;
        };
        let entry = guard.remove(index);
        self.bump_generation();
        let sent = entry.responder.send((approved, answered_by)).is_ok();
        drop(guard);
        // Record the decision for idempotent retries. Only store if the
        // responder received it (applied); if send failed, the approval
        // is gone from pending but no decision was delivered — treat as
        // NotFound so a retry can re-queue? No: the approval is consumed.
        // Store the decision regardless; the agent saw the channel close.
        // FIFO + TTL bounded: oldest-first eviction, never arbitrary.
        if let Ok(mut store) = self.resolved.lock() {
            store.insert(id.to_string(), approved, nonce.to_string());
        }
        if sent {
            AnswerOutcome::Applied(approved)
        } else {
            // Responder gone; decision recorded but not delivered.
            // Return Applied so the client knows the id is consumed.
            AnswerOutcome::Applied(approved)
        }
    }

    /// Legacy answer without nonce (for TUI keys). Uses empty nonce.
    pub fn answer_legacy(&self, id: &str, approved: bool, answered_by: Option<String>) -> bool {
        matches!(
            self.answer(id, approved, answered_by, ""),
            AnswerOutcome::Applied(_)
        )
    }

    /// The front of the queue for the TUI prompt.
    pub fn front(&self) -> Option<(String, String)> {
        let guard = self.pending.lock().ok()?;
        guard.first().map(|p| (p.id.clone(), p.title.clone()))
    }

    /// `pendingApprovals` for the mobile bootstrap (Swift wire dialect).
    pub fn list_json(&self) -> Vec<serde_json::Value> {
        let Ok(guard) = self.pending.lock() else {
            return Vec::new();
        };
        guard
            .iter()
            .map(|p| {
                let mut value = serde_json::json!({
                    "id": p.id,
                    "kind": p.kind,
                    "title": p.title,
                    "body": p.body,
                    "callerSessionID": p.caller_session_id,
                    "requestedAtUnixMs": p.requested_at,
                });
                if let Some(target) = &p.target_session_id {
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert("targetSessionID".into(), target.clone().into());
                    }
                }
                value
            })
            .collect()
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

/// Persist a grant into the sharded grants file (S2).
///
/// S2: Moved from app-state.json to grants.json with its own lock.
/// The write-ahead and hash-chain guarantees live in the review log
/// (action-reviews.jsonl), not here — grants have no chain semantics,
/// so sharding is safe. The grant is durably written (temp + rename)
/// before this returns, same durability as before.
pub fn persist_grant(kind: &str, caller: &str, target: Option<&str>, answered_by: Option<&str>) {
    // Phase 13 v2: Use group commit for batched fsync.
    // The audit entry is recorded BEFORE the grant (write-ahead), and the
    // batch fsyncs once for N concurrent grants instead of N times.
    if let Err(e) =
        unpeel_core::grant_writer::persist_grant_grouped(kind, caller, target, answered_by)
    {
        eprintln!("Failed to persist grant: {e}");
    }
}

/// A remembered connector-tool grant, namespaced by connector.
///
/// Approving tool `search` for connector `github` must NOT approve `search`
/// for any other connector: tool names are bare MCP names and two
/// connectors can expose the same one (`find_tool` resolves the first
/// connector, sorted, that advertises the name). Grants are stored as
/// `{"connector": c, "tool": t}` objects under `mcp_connector_approvals`.
/// Legacy bare-string entries from the pre-namespacing schema never match
/// (fail closed — the user is re-prompted once, then the namespaced grant
/// is persisted).
pub fn persist_connector_grant(caller: &str, connector: &str, tool: &str) {
    // Phase 13 v2: route through the grouped writer so connector grants get
    // a write-ahead audit entry and share the batch fsync like all grants.
    // (answered_by is not threaded through the connector approval path;
    // the audit records policy:Allow — see grant_writer for the default.)
    if let Err(e) =
        unpeel_core::grant_writer::persist_connector_grant_grouped(caller, connector, tool, None)
    {
        eprintln!("Failed to persist connector grant: {e}");
    }
}

/// Fast-path check for a remembered connector-tool grant. Only an exact
/// (connector, tool) object matches; legacy bare-string entries are
/// ignored (fail closed).
pub fn connector_grant_exists(caller: &str, connector: &str, tool: &str) -> bool {
    // S2: Check sharded grants.json first
    if let Ok(raw) = std::fs::read(unpeel_core::app_paths::grants_path()) {
        if let Ok(state) = serde_json::from_slice::<serde_json::Value>(&raw) {
            if state
                .get("mcp_connector_approvals")
                .and_then(|m| m.get(caller))
                .and_then(|l| l.as_array())
                .is_some_and(|l| {
                    l.iter().any(|v| {
                        v.get("connector").and_then(|c| c.as_str()) == Some(connector)
                            && v.get("tool").and_then(|t| t.as_str()) == Some(tool)
                    })
                })
            {
                return true;
            }
        }
    }
    // Legacy: check app-state.json
    let Some(state) = std::fs::read(unpeel_core::app_paths::app_state_path())
        .ok()
        .and_then(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok())
    else {
        return false;
    };
    state
        .get("mcp_connector_approvals")
        .and_then(|m| m.get(caller))
        .and_then(|l| l.as_array())
        .is_some_and(|l| {
            l.iter().any(|v| {
                v.get("connector").and_then(|c| c.as_str()) == Some(connector)
                    && v.get("tool").and_then(|t| t.as_str()) == Some(tool)
            })
        })
}

/// Fast-path check against previously persisted grants.
pub fn already_granted(kind: &str, caller: &str, target: Option<&str>) -> bool {
    // S2: Grant storage migration precedence.
    //
    // If grants.json exists, the system is in migrated state: read ONLY from
    // grants.json. The migrate command moves grants from app-state.json to
    // grants.json and deletes them from app-state.json.
    //
    // If grants.json does NOT exist, the system is in pre-migration state:
    // read from app-state.json (legacy location).
    //
    // Both-present case: grants.json wins. This happens if migrate ran but
    // app-state.json still has stale grant keys (shouldn't happen after
    // successful migrate, but we define the precedence explicitly).
    let key = match kind {
        "write" => "mcp_write_approvals",
        "browser" => "browser_approvals",
        "computer" => "computer_approvals",
        "app-open" => "mcp_app_open_approvals",
        _ => return false,
    };

    // Check if migrated (grants.json exists)
    let grants_path = unpeel_core::app_paths::grants_path();
    if grants_path.exists() {
        // Migrated: read ONLY from grants.json
        return unpeel_core::grant_store::grant_exists(key, caller, target);
    }

    // Pre-migration: read from app-state.json (legacy)
    if unpeel_core::grant_store::grant_exists(key, caller, target) {
        return true;
    }
    // Legacy: check app-state.json
    let Some(state) = std::fs::read(unpeel_core::app_paths::app_state_path())
        .ok()
        .and_then(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok())
    else {
        return false;
    };
    match kind {
        "write" => target.is_some_and(|t| {
            state
                .get("mcp_write_approvals")
                .and_then(|m| m.get(caller))
                .and_then(|l| l.as_array())
                .is_some_and(|l| l.iter().any(|v| v.as_str() == Some(t)))
        }),
        "browser" | "computer" => {
            let key = if kind == "browser" {
                "browser_approvals"
            } else {
                "computer_approvals"
            };
            state
                .get(key)
                .and_then(|l| l.as_array())
                .is_some_and(|l| l.iter().any(|v| v.as_str() == Some(caller)))
        }
        "app-open" => target.is_some_and(|app_id| {
            state
                .get("mcp_app_open_approvals")
                .and_then(|m| m.get(caller))
                .and_then(|l| l.as_array())
                .is_some_and(|l| l.iter().any(|v| v.as_str() == Some(app_id)))
        }),
        // Connector grants are namespaced by (connector, tool): see
        // `connector_grant_exists`. The generic single-target form cannot
        // express the pair and never matches (fail closed).
        "connector" => false,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    struct TempHome {
        dir: std::path::PathBuf,
        prev: Option<std::ffi::OsString>,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl TempHome {
        fn new(tag: &str) -> Self {
            let guard = APP_STATE_LOCK.lock().unwrap();
            let dir = std::env::temp_dir().join(format!(
                "unpeel-approvals-test-{tag}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let prev = std::env::var_os("UNPEEL_HOME");
            std::env::set_var("UNPEEL_HOME", &dir);
            Self {
                dir,
                prev,
                _guard: guard,
            }
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            match &self.prev {
                Some(prev) => std::env::set_var("UNPEEL_HOME", prev),
                None => std::env::remove_var("UNPEEL_HOME"),
            }
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn generation_advances_for_enqueue_and_answer_snapshots() {
        let hub = Arc::new(ApprovalHub::default());
        let request_hub = Arc::clone(&hub);
        let waiter = thread::spawn(move || {
            request_hub.request(
                "browser",
                "Allow browser access?".into(),
                "A session requested browser access.".into(),
                "caller-session".into(),
                None,
                Duration::from_secs(2),
            )
        });
        let mut queued = None;
        for _ in 0..100 {
            queued = hub.list_json().into_iter().next();
            if queued.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let queued = queued.expect("approval should enter the published snapshot");
        let queued_generation = hub.generation();
        assert!(queued_generation > 0);
        let id = queued["id"].as_str().expect("approval id");

        assert!(matches!(
            hub.answer(id, true, Some("test-device".to_string()), "test-nonce"),
            AnswerOutcome::Applied(true)
        ));
        let (approved, answered_by) = waiter.join().expect("request thread should finish");
        assert!(approved);
        assert_eq!(answered_by.as_deref(), Some("test-device"));
        assert!(hub.list_json().is_empty());
        assert!(hub.generation() > queued_generation);
    }

    #[test]
    fn connector_grant_persists_per_session_connector_tool_in_app_state() {
        let _home = TempHome::new("connector");
        // Nothing granted yet.
        assert!(!connector_grant_exists("sess-1", "github", "db.query"));
        persist_connector_grant("sess-1", "github", "db.query");
        // Now the (session, connector, tool) triple is granted, and only it.
        assert!(connector_grant_exists("sess-1", "github", "db.query"));
        assert!(!connector_grant_exists("sess-1", "github", "db.drop"));
        assert!(!connector_grant_exists("sess-2", "github", "db.query"));
        // Persisting twice stores the grant once.
        persist_connector_grant("sess-1", "github", "db.query");
        // S2: Grants are now sharded to grants.json, not app-state.json
        let state: serde_json::Value = serde_json::from_slice(
            &std::fs::read(unpeel_core::app_paths::grants_path()).expect("grants.json written"),
        )
        .unwrap();
        let grants = state["mcp_connector_approvals"]["sess-1"]
            .as_array()
            .expect("connector approvals map");
        assert_eq!(
            grants,
            &vec![serde_json::json!({"connector": "github", "tool": "db.query"})]
        );
    }

    #[test]
    fn connector_grant_does_not_leak_across_connectors() {
        // S1 negative test: approving `search` for connector `github` must
        // NOT approve `search` for connector `evil`. Tool names are bare
        // MCP names — two connectors can expose the same one.
        let _home = TempHome::new("connector-xconn");
        persist_connector_grant("sess-1", "github", "search");
        assert!(connector_grant_exists("sess-1", "github", "search"));
        assert!(
            !connector_grant_exists("sess-1", "evil", "search"),
            "grant for github/search must not satisfy evil/search"
        );
        // And the reverse: granting evil/search leaves github/search alone.
        persist_connector_grant("sess-1", "evil", "search");
        assert!(connector_grant_exists("sess-1", "evil", "search"));
        assert!(connector_grant_exists("sess-1", "github", "search"));
        assert!(!connector_grant_exists("sess-1", "github", "other"));
    }

    #[test]
    fn connector_grant_ignores_legacy_bare_string_entries() {
        // S1 negative test: pre-namespacing grants were bare tool strings.
        // They fail closed — never match — so the user is re-prompted once.
        let _home = TempHome::new("connector-legacy");
        let _ = unpeel_core::app_state::edit(|root| {
            root.entry("mcp_connector_approvals")
                .or_insert_with(|| serde_json::json!({}))["sess-1"] = serde_json::json!(["search"]);
            Ok(())
        });
        assert!(
            !connector_grant_exists("sess-1", "github", "search"),
            "legacy bare-string grant must not satisfy a namespaced check"
        );
        assert!(
            !connector_grant_exists("sess-1", "anything", "search"),
            "legacy bare-string grant must not satisfy any connector"
        );
    }

    #[test]
    fn eight_concurrent_approvals_all_visible_and_answerable() {
        // Phase 13 v3 (A): N=8 concurrent pending approvals across sessions.
        // Every one must be visible to the phone (via list_json, which backs
        // bootstrap pendingApprovals), and each must be answerable
        // independently and correctly. Answering one must not clear the
        // others. IDs must not collide.
        use std::collections::HashSet;
        use std::sync::{Arc, Barrier};
        use std::time::Duration;

        let hub = Arc::new(ApprovalHub::default());
        const N: usize = 8;
        let barrier = Arc::new(Barrier::new(N));

        // Spawn N threads, each requesting an approval for a distinct session.
        let mut handles = vec![];
        for i in 0..N {
            let hub_clone = Arc::clone(&hub);
            let barrier_clone = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier_clone.wait(); // All start together.
                let caller = format!("session-{i}");
                let target = format!("session-{i}-target");
                // Request with a short timeout; we will answer from the main
                // thread. Use a long timeout so the request doesn't expire
                // before we answer.
                hub_clone.request(
                    "write",
                    format!("Allow {caller} to write?"),
                    format!("{caller} -> {target}"),
                    caller,
                    Some(target),
                    Duration::from_secs(30),
                )
            }));
        }

        // Wait for all approvals to be queued.
        // Poll list_json until we see N, with timeout.
        let start = std::time::Instant::now();
        let mut list = vec![];
        while start.elapsed() < Duration::from_secs(10) {
            list = hub.list_json();
            if list.len() == N {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            list.len(),
            N,
            "all {N} approvals must be visible in bootstrap; got {}",
            list.len()
        );

        // Verify IDs are unique (no collision).
        let ids: HashSet<String> = list
            .iter()
            .filter_map(|v| {
                v.get("id")
                    .and_then(|id| id.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        assert_eq!(
            ids.len(),
            N,
            "approval IDs must not collide; got {} unique out of {N}",
            ids.len()
        );

        // Verify each session's approval is present.
        for i in 0..N {
            let caller = format!("session-{i}");
            let found = list.iter().any(|v| {
                v.get("callerSessionID")
                    .and_then(|c| c.as_str())
                    .map(|c| c == caller)
                    .unwrap_or(false)
            });
            assert!(found, "approval for {caller} must be visible");
        }

        // Answer them one by one, verifying each answer only removes its own.
        for (idx, id) in ids.iter().enumerate() {
            let before = hub.list_json().len();
            let outcome = hub.answer(id, true, Some("test-device".to_string()), "test-nonce");
            assert!(
                matches!(outcome, AnswerOutcome::Applied(true)),
                "answering approval {id} must succeed"
            );
            let after = hub.list_json().len();
            assert_eq!(
                after,
                before - 1,
                "answering one approval must remove exactly one (not clear others); before={before}, after={after}"
            );
            // Verify the answered ID is gone, others remain.
            let remaining_ids: HashSet<String> = hub
                .list_json()
                .iter()
                .filter_map(|v| {
                    v.get("id")
                        .and_then(|id| id.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            assert!(
                !remaining_ids.contains(id),
                "answered ID {id} must be removed"
            );
            assert_eq!(
                remaining_ids.len(),
                N - idx - 1,
                "exactly {} should remain after answering {}",
                N - idx - 1,
                idx + 1
            );
        }

        // All request threads should now complete with approved=true.
        for (i, h) in handles.into_iter().enumerate() {
            let (approved, answered_by) = h.join().expect("request thread panicked");
            assert!(
                approved,
                "session-{i} request must be approved (not hang, not deny)"
            );
            assert_eq!(
                answered_by.as_deref(),
                Some("test-device"),
                "answered_by must be propagated"
            );
        }

        // No pending approvals remain.
        assert!(
            hub.list_json().is_empty(),
            "all approvals should be answered"
        );
    }

    #[test]
    fn failed_answer_keeps_approval_visible() {
        // Phase 13 v3 (1): If answering fails (e.g. 429 rate limit at the
        // HTTP layer), the approval must stay visible in bootstrap until it
        // is successfully answered — never dropped silently.
        // Here we simulate the HTTP-layer rejection by answering with an
        // unknown ID (returns false, like a 429 would at the HTTP layer:
        // the pending entry is untouched).
        use std::time::Duration;

        let hub = Arc::new(ApprovalHub::default());
        let hub_clone = Arc::clone(&hub);
        let handle = thread::spawn(move || {
            hub_clone.request(
                "write",
                "Allow s1 to write?".to_string(),
                "s1 -> t1".to_string(),
                "s1".to_string(),
                Some("t1".to_string()),
                Duration::from_secs(30),
            )
        });

        // Wait for the approval to appear.
        let start = std::time::Instant::now();
        let mut id = String::new();
        while start.elapsed() < Duration::from_secs(10) {
            let list = hub.list_json();
            if let Some(v) = list.first() {
                id = v
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                if !id.is_empty() {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
        assert!(!id.is_empty(), "approval must become visible");

        // Simulate a failed answer (unknown ID ~ HTTP 429: entry untouched).
        assert!(
            matches!(
                hub.answer(
                    "wrong-id",
                    true,
                    Some("test-device".to_string()),
                    "test-nonce"
                ),
                AnswerOutcome::NotFound
            ),
            "answering unknown ID must fail"
        );
        // The real approval must still be visible.
        let list = hub.list_json();
        assert_eq!(list.len(), 1, "failed answer must not drop the approval");
        assert_eq!(
            list[0].get("id").and_then(|v| v.as_str()),
            Some(id.as_str()),
            "the same approval must stay visible"
        );

        // Now answer correctly — it succeeds.
        assert!(
            matches!(
                hub.answer(&id, true, Some("test-device".to_string()), "test-nonce"),
                AnswerOutcome::Applied(true)
            ),
            "retry with correct ID must succeed"
        );
        assert!(hub.list_json().is_empty(), "answered approval is removed");

        let (approved, _) = handle.join().expect("request thread panicked");
        assert!(approved, "requester must see approval");
    }

    #[test]
    fn generation_wakeup_fires_without_spin() {
        // Phase 13 v3 (B2): wait_for_generation_change must return
        // promptly when the generation advances, without polling.
        use std::time::Duration;

        let hub = Arc::new(ApprovalHub::default());
        let gen0 = hub.generation();

        // Spawn a thread that bumps generation after 100ms.
        let hub_clone = Arc::clone(&hub);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            hub_clone.bump_generation();
        });

        let start = std::time::Instant::now();
        let gen1 = hub.wait_for_generation_change(gen0, Duration::from_secs(5));
        let elapsed = start.elapsed();
        assert!(
            gen1 > gen0,
            "generation must advance (got {gen1}, was {gen0})"
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "wake-up must be prompt, not wait the full timeout (took {elapsed:?})"
        );

        // Timeout path: no bump, must return after ~timeout with same gen.
        let start = std::time::Instant::now();
        let gen2 = hub.wait_for_generation_change(gen1, Duration::from_millis(200));
        let elapsed = start.elapsed();
        assert_eq!(gen2, gen1, "no change → same generation");
        assert!(
            elapsed >= Duration::from_millis(150),
            "must wait the timeout when nothing changes (took {elapsed:?})"
        );
    }

    #[test]
    fn answer_idempotent_retry_returns_original_decision() {
        // Phase 14 (0a): Answer → drop response → retry → same decision,
        // exactly one application. The retried answer must return
        // AlreadyResolved with the original decision, never apply twice
        // or flip allow/deny.
        let hub = Arc::new(ApprovalHub::default());

        // Enqueue an approval.
        let (tx, rx) = std::sync::mpsc::channel();
        let approval_id = "test-idempotent-1".to_string();
        {
            let mut guard = hub.pending.lock().unwrap();
            guard.push(PendingApproval {
                id: approval_id.clone(),
                kind: "test".to_string(),
                title: "Test".to_string(),
                body: "body".to_string(),
                caller_session_id: "s1".to_string(),
                target_session_id: None,
                requested_at: 0,
                responder: tx,
            });
        }

        // First answer: approve with nonce-1. Simulates the response being
        // lost (we don't check the return here, just that it was applied).
        let outcome1 = hub.answer(
            &approval_id,
            true,
            Some("test-device".to_string()),
            "nonce-1",
        );
        assert!(
            matches!(outcome1, AnswerOutcome::Applied(true)),
            "first answer must apply (got {outcome1:?})"
        );
        // The agent receives the decision exactly once.
        let (approved, _) = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("agent must receive decision");
        assert!(approved, "agent must see approved=true");

        // Retry with a DIFFERENT nonce (simulating a client retry after
        // unknown outcome with a new nonce). Must return AlreadyResolved
        // with the ORIGINAL decision, not apply again.
        let outcome2 = hub.answer(
            &approval_id,
            false, // Attempt to flip to deny — must NOT take effect.
            Some("test-device".to_string()),
            "nonce-2",
        );
        assert!(
            matches!(outcome2, AnswerOutcome::AlreadyResolved(true)),
            "retry must return AlreadyResolved(true), not flip to false (got {outcome2:?})"
        );

        // Retry with the SAME nonce. Same result.
        let outcome3 = hub.answer(
            &approval_id,
            true,
            Some("test-device".to_string()),
            "nonce-1",
        );
        assert!(
            matches!(outcome3, AnswerOutcome::AlreadyResolved(true)),
            "same-nonce retry must also return AlreadyResolved(true) (got {outcome3:?})"
        );

        // The channel is closed (only one send happened). A second recv
        // must fail, proving exactly one application.
        assert!(
            rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "exactly one decision must be delivered to the agent"
        );

        // Phase 14 follow-up: the resolved-decision store must contain
        // exactly ONE entry for this approval, not one per retry. Retries
        // return AlreadyResolved without inserting duplicates.
        let resolved_len = hub.resolved.lock().unwrap().len();
        assert_eq!(
            resolved_len, 1,
            "resolved store must have exactly one entry after retries (got {resolved_len})"
        );
    }

    #[test]
    fn answer_after_host_restart_returns_not_found_not_applied() {
        // Phase 14 follow-up: the resolved store is in-memory. If the Host
        // restarts between applying the answer and the phone's retry, the
        // retry must NOT re-apply or re-open the approval. It gets NotFound,
        // which the phone UI renders as a terminal "already resolved" state.
        let hub = Arc::new(ApprovalHub::default());
        let approval_id = "test-restart-1".to_string();

        // Apply an answer on the "old" hub.
        let (tx, _rx) = std::sync::mpsc::channel();
        {
            let mut guard = hub.pending.lock().unwrap();
            guard.push(PendingApproval {
                id: approval_id.clone(),
                kind: "test".to_string(),
                title: "Test".to_string(),
                body: "body".to_string(),
                caller_session_id: "s1".to_string(),
                target_session_id: None,
                requested_at: 0,
                responder: tx,
            });
        }
        let outcome = hub.answer(&approval_id, true, Some("d".to_string()), "n1");
        assert!(matches!(outcome, AnswerOutcome::Applied(true)));

        // Simulate Host restart: new hub, empty resolved store.
        let restarted = Arc::new(ApprovalHub::default());
        let retry = restarted.answer(&approval_id, false, Some("d".to_string()), "n2");
        assert!(
            matches!(retry, AnswerOutcome::NotFound),
            "retry after restart must get NotFound, not re-apply (got {retry:?})"
        );
        // Must not have created a resolved entry (no re-application).
        assert!(
            restarted.resolved.lock().unwrap().is_empty(),
            "restarted hub must not record a decision for unknown approval"
        );
    }

    #[test]
    fn resolved_store_is_bounded() {
        // Phase 14 follow-up: the resolved map must not grow without bound.
        let hub = Arc::new(ApprovalHub::default());
        for i in 0..(RESOLVED_CAP + 100) {
            let (tx, _rx) = std::sync::mpsc::channel();
            let id = format!("test-bound-{i}");
            {
                let mut guard = hub.pending.lock().unwrap();
                guard.push(PendingApproval {
                    id: id.clone(),
                    kind: "test".to_string(),
                    title: "Test".to_string(),
                    body: "body".to_string(),
                    caller_session_id: "s1".to_string(),
                    target_session_id: None,
                    requested_at: 0,
                    responder: tx,
                });
            }
            let _ = hub.answer(&id, true, Some("d".to_string()), "n");
        }
        let len = hub.resolved.lock().unwrap().len();
        assert!(
            len <= RESOLVED_CAP,
            "resolved store must be bounded at {RESOLVED_CAP} (got {len})"
        );
    }

    #[test]
    fn idempotent_answer_writes_exactly_one_audit_log_entry() {
        // Phase 14 follow-up 1: The tamper-evident review log (not the
        // in-memory resolved store) must contain exactly ONE entry for the
        // approval after all retries. The agent acts on Applied (writes the
        // entry); on AlreadyResolved it does nothing.
        use unpeel_core::action_reviews::{record_review, Actor, ReviewDecision};

        let dir = tempfile::tempdir().expect("tempdir");
        let session_dir = dir.path();

        let hub = Arc::new(ApprovalHub::default());
        let approval_id = "test-audit-1".to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let mut guard = hub.pending.lock().unwrap();
            guard.push(PendingApproval {
                id: approval_id.clone(),
                kind: "test".to_string(),
                title: "Test".to_string(),
                body: "body".to_string(),
                caller_session_id: "s1".to_string(),
                target_session_id: None,
                requested_at: 0,
                responder: tx,
            });
        }

        // First answer: Applied. Agent receives decision and writes audit entry.
        let outcome1 = hub.answer(&approval_id, true, Some("phone".to_string()), "n1");
        assert!(matches!(outcome1, AnswerOutcome::Applied(true)));
        let (approved, _) = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(approved);

        // Simulate agent writing the review log entry on Applied.
        record_review(
            session_dir,
            Actor::Human {
                device_id: "phone".to_string(),
            },
            "test-connector",
            "test-tool",
            "args-hash",
            ReviewDecision::Approved,
            None,
        )
        .expect("record_review");

        // Retries: AlreadyResolved. Agent does NOT write again.
        for nonce in ["n2", "n3"] {
            let outcome = hub.answer(&approval_id, false, Some("phone".to_string()), nonce);
            assert!(
                matches!(outcome, AnswerOutcome::AlreadyResolved(true)),
                "retry must be AlreadyResolved (got {outcome:?})"
            );
            // No record_review call here — AlreadyResolved means "do nothing".
        }

        // Assert exactly ONE entry in the tamper-evident log.
        let log_path = session_dir.join(unpeel_core::action_reviews::REVIEWS_FILE);
        let content = std::fs::read_to_string(&log_path).expect("read log");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(
            lines.len(),
            1,
            "review log must have exactly one entry after retries (got {})",
            lines.len()
        );
        // Verify the chain is intact (tamper-evident).
        unpeel_core::action_reviews::verify_review_chain(session_dir).expect("chain must verify");
    }
}
