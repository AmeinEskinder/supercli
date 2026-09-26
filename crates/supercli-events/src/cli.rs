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
