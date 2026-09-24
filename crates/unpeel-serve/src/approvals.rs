//! MCP approval hub: when no app runs, the MCP host's port discovery finds
//! the TUI's hook listener and POSTs its blocking approval requests here
//! (`/mcp/approve-write|browser|computer|app-open|connector`). Requests queue in this hub; the
//! TUI renders the front of the queue as a y/n prompt and paired phones see
//! it as `pendingApprovals` in bootstrap (answered via
//! `/mobile/approvals/answer`) — first answer wins. Approvals persist into
//! the shared `app-state.json` exactly where the app keeps them, so grants
//! survive and both frontends honor them.

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
            self.generation.fetch_add(1, Ordering::AcqRel);
        }
        let (approved, answered_by) = rx.recv_timeout(timeout).unwrap_or((false, None));
        // Drop the entry if it's still queued (timeout path).
        if let Ok(mut guard) = self.pending.lock() {
            let before = guard.len();
            guard.retain(|p| p.id != id);
            if guard.len() != before {
                self.generation.fetch_add(1, Ordering::AcqRel);
            }
        }
        (approved, answered_by)
    }

    /// Answer by id (from the TUI keys or the phone). `answered_by`
    /// identifies the answerer when known (e.g. a paired-device id).
    /// Returns false when the id is unknown (already answered or timed out).
    pub fn answer(&self, id: &str, approved: bool, answered_by: Option<String>) -> bool {
        let Ok(mut guard) = self.pending.lock() else {
            return false;
        };
        let Some(index) = guard.iter().position(|p| p.id == id) else {
            return false;
        };
        let entry = guard.remove(index);
        self.generation.fetch_add(1, Ordering::AcqRel);
        entry.responder.send((approved, answered_by)).is_ok()
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

/// Persist a grant into the shared app-state.json exactly where the app
/// keeps it, so the approval outlives this TUI run and the app honors it.
pub fn persist_grant(kind: &str, caller: &str, target: Option<&str>) {
    let _ = unpeel_core::app_state::edit(|root| {
        match kind {
            "write" => {
                let Some(target) = target else { return Ok(()) };
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
                let Some(app_id) = target else { return Ok(()) };
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
                // Connector grants are namespaced by (connector, tool), not
                // by tool alone: see `persist_connector_grant`. The generic
                // single-target form cannot express the pair, so it is a
                // no-op here by construction — use the dedicated function.
                let _ = target;
                return Ok(());
            }
            _ => {}
        }
        Ok(())
    });
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
    let _ = unpeel_core::app_state::edit(|root| {
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
        Ok(())
    });
}

/// Fast-path check for a remembered connector-tool grant. Only an exact
/// (connector, tool) object matches; legacy bare-string entries are
/// ignored (fail closed).
pub fn connector_grant_exists(caller: &str, connector: &str, tool: &str) -> bool {
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

        assert!(hub.answer(id, true, Some("test-device".to_string())));
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
        let state: serde_json::Value = serde_json::from_slice(
            &std::fs::read(unpeel_core::app_paths::app_state_path())
                .expect("app-state.json written"),
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
}
