//! `hooks` CLI command logic (design §10).
//!
//! These are pure functions over the registry/dispatcher; the argument
//! parsing and terminal output live in `supercli-cli` (wired later).
//!
//! - `hooks list` — every registered handler with entity/event/priority,
//!   the target, and whether it is disabled and why.
//! - `hooks test <name>` — dry-run one handler against a synthetic doc.
//!   Never touches real documents: `dry_run` is set in the context and the
//!   doc is synthetic.
//! - `hooks trace [--limit N]` — recent handler runs from the dispatcher's
//!   in-memory trace ring.

use crate::handlers::{classify_exec, execute};
use crate::registry::{HandlerTarget, HookRegistry};
use crate::runner::HandlerRun;
use crate::{DocEvent, DocType, HandlerContext, HandlerInput};

/// One row of `hooks list` output.
#[derive(Debug, Clone)]
pub struct ListRow {
    pub name: String,
    pub entity: String,
    pub event: String,
    pub priority: u32,
    pub target: String,
    pub source: String,
    pub disabled: bool,
    pub disabled_reason: Option<String>,
}

/// List all registered handlers plus disabled ones (with reasons).
pub fn list(registry: &HookRegistry) -> Vec<ListRow> {
    let mut rows: Vec<ListRow> = Vec::new();
    for (_entity, _event, handlers) in registry.all_pairs() {
        for h in handlers {
            rows.push(ListRow {
                name: h.name.clone(),
                entity: h.entity.as_str().to_string(),
                event: h.event.as_str().to_string(),
                priority: h.priority,
                target: match &h.target {
                    HandlerTarget::Command(argv) => format!("command {}", argv.join(" ")),
                    HandlerTarget::Webhook(url) => format!("webhook {url}"),
                },
                source: if h.global {
                    "global".to_string()
                } else {
                    "project".to_string()
                },
                disabled: false,
                disabled_reason: None,
            });
        }
    }
    for d in &registry.disabled {
        rows.push(ListRow {
            name: d.name.clone(),
            entity: String::new(),
            event: String::new(),
            priority: 0,
            target: String::new(),
            source: if d.global {
                "global".to_string()
            } else {
                "project".to_string()
            },
            disabled: true,
            disabled_reason: Some(d.reason.clone()),
        });
    }
    rows.sort_by(|a, b| {
        a.entity
            .cmp(&b.entity)
            .then(a.event.cmp(&b.event))
            .then(a.priority.cmp(&b.priority))
            .then(a.name.cmp(&b.name))
    });
    rows
}

/// Error from `hooks test`.
#[derive(Debug)]
pub enum TestError {
    NotFound(String),
    Disabled(String),
}

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestError::NotFound(n) => write!(f, "no handler named {n:?}"),
            TestError::Disabled(r) => write!(f, "handler is disabled: {r}"),
        }
    }
}

/// Dry-run one handler against a synthetic doc.
///
/// The doc is synthetic (`{"id": "dry-run", ...}` merged with the
/// caller-supplied fields); `dry_run: true` is set in the context so the
/// handler knows this is not a real dispatch. Never touches real docs.
pub fn test(
    registry: &HookRegistry,
    name: &str,
    entity: DocType,
    event: DocEvent,
    doc_fields: &serde_json::Value,
) -> Result<HandlerRun, TestError> {
    for d in &registry.disabled {
        if d.name == name {
            return Err(TestError::Disabled(d.reason.clone()));
        }
    }
    let handler = registry
        .all_pairs()
        .into_iter()
        .flat_map(|(_, _, hs)| hs.to_vec())
        .find(|h| h.name == name)
        .ok_or_else(|| TestError::NotFound(name.to_string()))?;

    let mut doc = serde_json::json!({"id": "dry-run", "dry_run": true});
    if let (Some(d), Some(f)) = (doc.as_object_mut(), doc_fields.as_object()) {
        for (k, v) in f {
            d.insert(k.clone(), v.clone());
        }
    }
    let ctx = HandlerContext {
        actor: "human:cli".to_string(),
        depth: 0,
        dry_run: true,
    };
    let input = HandlerInput {
        event_id: format!("dry-run:{}:{}", entity.as_str(), name),
        entity: handler.entity.as_str().to_string(),
        event: handler.event.as_str().to_string(),
        doc,
        context: ctx,
    };
    let exec = execute(&handler.target, &input, handler.timeout_ms);
    let (outcome, decision, fail_reason) = classify_exec(&exec);
    let message = fail_reason.unwrap_or_default().to_string();
    let message = if message.is_empty() {
        exec.output
            .as_ref()
            .map(|o| o.message.clone())
            .unwrap_or_default()
    } else {
        message
    };
    // Surface the entity/event the caller asked to test (they may differ
    // from the handler's registration; the handler still runs).
    let _ = (entity, event);
    Ok(HandlerRun {
        name: handler.name.clone(),
        priority: handler.priority,
        decision,
        outcome,
        elapsed_ms: exec.elapsed_ms,
        message,
        dropped_keys: Vec::new(),
        observer_patch_ignored: false,
        patch: exec.output.as_ref().and_then(|o| {
            if o.is_patch_empty() {
                None
            } else {
                Some(o.patch.clone())
            }
        }),
    })
}

/// One row of `hooks trace` output.
#[derive(Debug, Clone)]
pub struct TraceRow {
    pub event_id: String,
    pub entity: String,
    pub event: String,
    pub handler: String,
    pub decision: String,
    pub elapsed_ms: u64,
    pub outcome: String,
    pub message: String,
    pub violation: Option<String>,
}

/// One hook run from the durable audit log (action-reviews.jsonl).
///
/// The audit log is the durable lineage: every hook run appends exactly one
/// hash-chained entry with actor `hook:<name>`. The `tool` field encodes
/// `<Entity>:<event>:<handler>`; the `args_hash` authenticates the full
/// run details (event id, decision, patch, timing).
#[derive(Debug, Clone)]
pub struct AuditTraceRow {
    /// When the run was recorded (unix ms).
    pub ts_ms: u64,
    /// The handler name (from actor `hook:<name>`).
    pub handler: String,
    /// Entity (from tool field).
    pub entity: String,
    /// Event (from tool field).
    pub event: String,
    /// Hook decision mapped to review decision: Approved (allow/escalate)
    /// or Denied (reject).
    pub decision: String,
    /// SHA-256 of the canonical run details (event id, decision, patch,
    /// timing, priority, outcome). Use to verify a specific run.
    pub args_hash: String,
    /// Hash-chain entry hash (for chain verification).
    pub entry_hash: String,
    /// Previous entry's hash (chain link).
    pub prev_hash: String,
}

/// Read hook-run lineage from the audit log (action-reviews.jsonl).
///
/// Returns hook entries (actor `hook:*`, connector `hook`) newest-first,
/// capped at `limit`. Each row carries the timing (ts_ms), handler
/// decision, and audit hashes for verification.
pub fn trace_audit(session_dir: &std::path::Path, limit: usize) -> Vec<AuditTraceRow> {
    let path = session_dir.join("action-reviews.jsonl");
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut rows: Vec<AuditTraceRow> = Vec::new();
    for line in content.lines().rev() {
        if rows.len() >= limit.max(1) {
            break;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // Hook entries have actor "hook:<name>" and connector "hook".
        let actor = v.get("actor").and_then(|a| a.as_str()).unwrap_or("");
        let connector = v.get("connector").and_then(|c| c.as_str()).unwrap_or("");
        if connector != "hook" || !actor.starts_with("hook:") {
            continue;
        }
        let handler = actor.strip_prefix("hook:").unwrap_or("").to_string();
        // Tool field: "<Entity>:<event>:<handler>".
        let tool = v.get("tool").and_then(|t| t.as_str()).unwrap_or("");
        let mut parts = tool.splitn(3, ':');
        let entity = parts.next().unwrap_or("").to_string();
        let event = parts.next().unwrap_or("").to_string();
        // Decision: Approved (allow/escalate) or Denied (reject).
        let decision = v
            .get("decision")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();
        rows.push(AuditTraceRow {
            ts_ms: v.get("ts_ms").and_then(|t| t.as_u64()).unwrap_or(0),
            handler,
            entity,
            event,
            decision,
            args_hash: v
                .get("args_hash")
                .and_then(|h| h.as_str())
                .unwrap_or("")
                .to_string(),
            entry_hash: v
                .get("entry_hash")
                .and_then(|h| h.as_str())
                .unwrap_or("")
                .to_string(),
            prev_hash: v
                .get("prev_hash")
                .and_then(|h| h.as_str())
                .unwrap_or("")
                .to_string(),
        });
    }
    rows
}

/// Filter audit trace rows by event lineage.
///
/// Matches `filter` against entity, event, handler, or the args_hash /
/// entry_hash prefix. Used by `hooks trace <id>` to show the full
/// lineage for one event.
pub fn filter_trace(rows: Vec<AuditTraceRow>, filter: &str) -> Vec<AuditTraceRow> {
    let f = filter.to_lowercase();
    rows.into_iter()
        .filter(|r| {
            r.entity.to_lowercase().contains(&f)
                || r.event.to_lowercase().contains(&f)
                || r.handler.to_lowercase().contains(&f)
                || r.args_hash.to_lowercase().starts_with(&f)
                || r.entry_hash.to_lowercase().starts_with(&f)
        })
        .collect()
}

/// Format a unix-ms timestamp as human-readable relative time.
pub fn format_relative_time(ts_ms: u64, now_ms: u64) -> String {
    let diff_s = now_ms.saturating_sub(ts_ms) / 1000;
    if diff_s < 60 {
        format!("{diff_s}s ago")
    } else if diff_s < 3600 {
        format!("{}m ago", diff_s / 60)
    } else if diff_s < 86400 {
        format!("{}h ago", diff_s / 3600)
    } else {
        format!("{}d ago", diff_s / 86400)
    }
}

/// Recent handler runs, newest first, capped at `limit`.
pub fn trace(dispatcher: &crate::runner::Dispatcher, limit: usize) -> Vec<TraceRow> {
    let mut entries = dispatcher.trace();
    entries.reverse();
    entries
        .into_iter()
        .take(limit.max(1))
        .map(|e| TraceRow {
            event_id: e.event_id,
            entity: e.entity,
            event: e.event,
            handler: e.handler,
            decision: e.decision,
            elapsed_ms: e.elapsed_ms,
            outcome: e.outcome,
            message: e.message,
            violation: e.violation,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::HookRegistry;
    use crate::runner::SyncParams;
    use crate::{HookDecision, RunOutcome};
    use std::io::Write;

    fn registry_with(body: &str) -> (tempfile::TempDir, HookRegistry) {
        let d = tempfile::TempDir::new().unwrap();
        let p = d.path().join("hooks.toml");
        let mut f = std::fs::File::create(&p).unwrap();
        // An empty file is a load error (fail-closed); tests that want an
        // empty registry write the table header explicitly.
        let body = if body.trim().is_empty() {
            "[doc_events]\n"
        } else {
            body
        };
        f.write_all(body.as_bytes()).unwrap();
        let reg = HookRegistry::load(&p, &d.path().join("missing.toml")).unwrap();
        (d, reg)
    }

    #[test]
    fn list_shows_handlers_and_disabled() {
        let body = r#"
[doc_events.Session]
before_save = [
  { name = "ok", command = ["sh", "-c", "true"], priority = 10 },
  { name = "bad", command = ["does-not-exist-xyz"], priority = 20 },
]
"#;
        let (_d, reg) = registry_with(body);
        let rows = list(&reg);
        assert_eq!(rows.len(), 2);
        let ok = rows.iter().find(|r| r.name == "ok").unwrap();
        assert!(!ok.disabled);
        assert_eq!(ok.source, "global");
        assert!(ok.target.starts_with("command "));
        let bad = rows.iter().find(|r| r.name == "bad").unwrap();
        assert!(bad.disabled);
        assert!(bad.disabled_reason.as_ref().unwrap().contains("not found"));
    }

    #[test]
    fn test_dry_run_never_touches_real_docs() {
        let body = r#"
[doc_events.ToolCall]
before_execute = [
  { name = "checker", command = ["sh", "-c", "cat > /dev/null; printf '{\"decision\":\"escalate\",\"message\":\"dry\"}'"], priority = 10 },
]
"#;
        let (_d, reg) = registry_with(body);
        let run = test(
            &reg,
            "checker",
            DocType::ToolCall,
            DocEvent::BeforeExecute,
            &serde_json::json!({"tool": "write_file"}),
        )
        .unwrap();
        assert_eq!(run.decision, HookDecision::Escalate);
        assert_eq!(run.outcome, RunOutcome::Ok);
        assert_eq!(run.message, "dry");
    }

    #[test]
    fn test_unknown_handler_errors() {
        let (_d, reg) = registry_with("");
        assert!(matches!(
            test(
                &reg,
                "nope",
                DocType::Session,
                DocEvent::BeforeSave,
                &serde_json::json!({})
            ),
            Err(TestError::NotFound(_))
        ));
    }

    #[test]
    fn trace_returns_newest_first() {
        use crate::runner::{AuditSink, Dispatcher};
        let body = r#"
[doc_events.Session]
before_save = [
  { name = "t1", command = ["sh", "-c", "cat > /dev/null; printf '{\"decision\":\"allow\"}'"], priority = 10 },
]
"#;
        let (_d, reg) = registry_with(body);
        let tmp = tempfile::TempDir::new().unwrap();
        let disp = Dispatcher::new(reg, AuditSink::new(tmp.path()));
        let ctx = HandlerContext {
            actor: "human:x".to_string(),
            depth: 0,
            dry_run: true,
        };
        for seq in 1..=3 {
            disp.run_sync(SyncParams {
                entity: DocType::Session,
                event: DocEvent::BeforeSave,
                doc_id: "s-t",
                doc: serde_json::json!({"id": "s-t"}),
                proposed: HookDecision::Allow,
                ctx: &ctx,
                seq,
            });
        }
        let rows = trace(&disp, 2);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].event_id.ends_with(":3"));
        assert!(rows[1].event_id.ends_with(":2"));
    }
}

#[cfg(test)]
mod trace_tests {
    use super::*;

    fn write_audit_log(dir: &std::path::Path, entries: &[serde_json::Value]) {
        let path = dir.join("action-reviews.jsonl");
        let mut content = String::new();
        for e in entries {
            content.push_str(&serde_json::to_string(e).unwrap());
            content.push('\n');
        }
        std::fs::write(&path, content).unwrap();
    }

    fn hook_entry(handler: &str, entity: &str, event: &str, ts_ms: u64) -> serde_json::Value {
        serde_json::json!({
            "review_id": format!("r-{handler}"),
            "ts_ms": ts_ms,
            "actor": format!("hook:{handler}"),
            "connector": "hook",
            "tool": format!("{entity}:{event}:{handler}"),
            "args_hash": format!("args-{handler}"),
            "decision": "Approved",
            "prev_hash": "prev",
            "entry_hash": format!("entry-{handler}"),
        })
    }

    #[test]
    fn trace_audit_reads_hook_entries_newest_first() {
        let dir = tempfile::TempDir::new().unwrap();
        write_audit_log(
            dir.path(),
            &[
                hook_entry("h1", "Session", "on_update", 1000),
                hook_entry("h2", "Session", "on_change", 2000),
                // Non-hook entry should be skipped.
                serde_json::json!({
                    "review_id": "r-x",
                    "ts_ms": 1500,
                    "actor": "human:dev",
                    "connector": "tool",
                    "tool": "write_file",
                    "args_hash": "x",
                    "decision": "Approved",
                    "prev_hash": "p",
                    "entry_hash": "e",
                }),
            ],
        );
        let rows = trace_audit(dir.path(), 10);
        assert_eq!(rows.len(), 2);
        // Newest first.
        assert_eq!(rows[0].handler, "h2");
        assert_eq!(rows[1].handler, "h1");
        assert_eq!(rows[0].entity, "Session");
        assert_eq!(rows[0].event, "on_change");
        assert_eq!(rows[0].decision, "Approved");
        assert_eq!(rows[0].entry_hash, "entry-h2");
    }

    #[test]
    fn filter_trace_matches_handler_and_hash() {
        let dir = tempfile::TempDir::new().unwrap();
        write_audit_log(
            dir.path(),
            &[
                hook_entry("notify", "Session", "on_update", 1000),
                hook_entry("audit", "Turn", "on_change", 2000),
            ],
        );
        let rows = trace_audit(dir.path(), 10);
        let filtered = filter_trace(rows.clone(), "notify");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].handler, "notify");

        let filtered = filter_trace(rows.clone(), "entry-audit");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].handler, "audit");

        let filtered = filter_trace(rows, "nonexistent");
        assert!(filtered.is_empty());
    }

    #[test]
    fn format_relative_time_human_readable() {
        let now = 1_000_000_000;
        assert_eq!(format_relative_time(now - 30_000, now), "30s ago");
        assert_eq!(format_relative_time(now - 120_000, now), "2m ago");
        assert_eq!(format_relative_time(now - 3_600_000, now), "1h ago");
        assert_eq!(format_relative_time(now - 172_800_000, now), "2d ago");
    }
}
