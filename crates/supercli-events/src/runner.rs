//! The hook dispatcher: synchronous `before_*`/`validate` interception and
//! the observer path.
//!
//! Safety contract (design §4):
//!
//! 1. `before_*`/`validate` run synchronously, ordered by priority, each
//!    time-boxed (`timeout_ms`, default 2000 ms). They may patch the
//!    proposed doc or reject it with a reason. A crash, timeout, or bad
//!    output FAILS CLOSED (reject).
//! 2. Tighten-only: the effective decision is the maximum strictness across
//!    all handlers and the caller's proposed decision. A hook can never
//!    loosen.
//! 3. `after_*`/`on_*` are observers: they run after the write-ahead
//!    record + audit fsync, see a frozen doc, cannot change the outcome
//!    (patches are ignored and logged as contract violations), and are
//!    delivered at-least-once via the outbox with the event id as the
//!    idempotency key.
//! 4. Every hook run appends exactly one hash-chained audit entry with
//!    actor `hook:<name>`.
//! 5. Recursion guard: depth >= [`MAX_HOOK_DEPTH`] proceeds hookless; the
//!    skip is audit-logged.

use crate::handlers::{classify_exec, execute};
use crate::registry::{HookHandler, HookRegistry};
use crate::{
    event_id, DocEvent, DocType, HandlerContext, HandlerInput, HookDecision, RunOutcome,
    MAX_HOOK_DEPTH,
};
use std::path::Path;
use std::sync::{Arc, Mutex};
use supercli_core::action_reviews::{record_review, Actor, ReviewDecision};
use supercli_core::browser_engine::sha256_hex;

/// Connector namespace used for hook-run audit entries in the shared
/// review log.
const HOOK_CONNECTOR: &str = "hook";

/// SHA-256 hex of the canonical JSON of a hook run. This is what the
/// audit chain authenticates via `args_hash`: decision, patch, timing,
/// priority, outcome, and event id are all covered.
fn hook_args_hash(
    event_id: &str,
    entity: DocType,
    event: DocEvent,
    doc_id: &str,
    run: &HandlerRun,
    patch: Option<&serde_json::Value>,
) -> String {
    let canonical = serde_json::json!({
        "event_id": event_id,
        "entity": entity.as_str(),
        "event": event.as_str(),
        "doc_id": doc_id,
        "handler": run.name,
        "decision": run.decision.as_str(),
        "patch": patch.cloned().unwrap_or(serde_json::Value::Null),
        "priority": run.priority,
        "elapsed_ms": run.elapsed_ms,
        "outcome": run.outcome.as_str(),
        "message": run.message,
    });
    // serde_json without preserve_order emits object keys in sorted
    // order; the byte form is the canonical form the hash covers.
    sha256_hex(canonical.to_string().as_bytes())
}

/// Keys a patch may never change: structural identity of the doc.
/// Out-of-scope keys are dropped and reported (per-entity allowlists are
/// enumerated when emission points are wired; design §13.1).
const RESERVED_PATCH_KEYS: &[&str] = &["id", "type", "entity", "event", "tool"];

/// Merge a handler patch into the proposed doc. Returns the patched doc
/// plus any dropped (reserved) keys.
pub fn apply_patch(
    mut doc: serde_json::Value,
    patch: &serde_json::Value,
) -> (serde_json::Value, Vec<String>) {
    let mut dropped = Vec::new();
    if let (Some(doc_obj), Some(patch_obj)) = (doc.as_object_mut(), patch.as_object()) {
        for (k, v) in patch_obj {
            if RESERVED_PATCH_KEYS.contains(&k.as_str()) {
                dropped.push(k.clone());
                continue;
            }
            doc_obj.insert(k.clone(), v.clone());
        }
    }
    (doc, dropped)
}

/// One handler's contribution to a sync dispatch.
#[derive(Debug, Clone)]
pub struct HandlerRun {
    pub name: String,
    pub priority: u32,
    pub decision: HookDecision,
    pub outcome: RunOutcome,
    pub elapsed_ms: u64,
    pub message: String,
    /// Reserved keys the handler tried to patch (dropped).
    pub dropped_keys: Vec<String>,
    /// `true` for observers that returned a non-empty patch (ignored).
    pub observer_patch_ignored: bool,
    /// The patch this run contributed (sync events only, after the
    /// handler ran OK). The dispatcher applies these in execution order.
    #[allow(dead_code)]
    pub patch: Option<serde_json::Value>,
}

/// Result of a synchronous (`before_*`/`validate`) dispatch.
#[derive(Debug)]
pub struct SyncOutcome {
    /// The tighten-only effective decision.
    pub decision: HookDecision,
    /// The doc with all handler patches applied (in execution order).
    pub patched_doc: serde_json::Value,
    pub runs: Vec<HandlerRun>,
    /// Non-empty when the decision is Reject: the first rejection reason
    /// (or fail-closed reason) encountered.
    pub reject_reason: Option<String>,
}

/// Where audit entries go. In production this is the session dir holding
/// `action-reviews.jsonl`; in tests a temp dir.
#[derive(Clone)]
pub struct AuditSink {
    pub session_dir: std::path::PathBuf,
}

impl AuditSink {
    pub fn new(session_dir: &Path) -> Self {
        AuditSink {
            session_dir: session_dir.to_path_buf(),
        }
    }

    /// Append exactly one hash-chained entry for a hook run, using the
    /// existing shared review log (`record_review`). The hook run is
    /// authenticated by `args_hash`: the SHA-256 of the canonical JSON
    /// carrying event id, decision, patch, timing, priority, and outcome.
    /// Hook decisions map to review decisions (Reject -> Denied; the
    /// tighten-only lattice never produces a hook approval that bypasses
    /// anything, so Allow/Escalate -> Approved with the real decision in
    /// the hashed payload).
    pub fn record(
        &self,
        event_id: &str,
        entity: DocType,
        event: DocEvent,
        doc_id: &str,
        run: &HandlerRun,
        patch: Option<&serde_json::Value>,
    ) {
        let actor = Actor::Hook {
            name: run.name.clone(),
        };
        // Tool identity: which entity/event this hook ran for and which
        // handler ran. Detailed trace metadata lives in the dispatcher's
        // trace ring and the outbox record; the chain authenticates the
        // canonical args hash.
        let tool = format!("{}:{}:{}", entity.as_str(), event.as_str(), run.name);
        let args_hash = hook_args_hash(event_id, entity, event, doc_id, run, patch);
        let decision = match run.decision {
            HookDecision::Reject => ReviewDecision::Denied,
            HookDecision::Allow | HookDecision::Escalate => ReviewDecision::Approved,
        };
        // Audit is best-effort here: a failure to append must not change
        // the dispatch decision (the decision was already made); it is
        // surfaced to stderr so operators notice a broken audit log.
        if let Err(e) = record_review(
            &self.session_dir,
            actor,
            HOOK_CONNECTOR,
            &tool,
            &args_hash,
            decision,
            None,
        ) {
            eprintln!("supercli-events: audit append failed for {event_id}: {e:?}");
        }
    }
}

/// The dispatcher. Cheap to clone; the registry is read-only after load.
#[derive(Clone)]
pub struct Dispatcher {
    registry: Arc<HookRegistry>,
    audit: AuditSink,
    /// Shared trace of recent runs (for `hooks trace`).
    trace: Arc<Mutex<Vec<TraceEntry>>>,
}

#[derive(Debug, Clone)]
pub struct TraceEntry {
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

/// Parameters for one synchronous (`before_*`/`validate`) dispatch.
/// Bundled so the dispatcher signature stays within clippy's arity lint.
pub struct SyncParams<'a> {
    pub entity: DocType,
    pub event: DocEvent,
    pub doc_id: &'a str,
    pub doc: serde_json::Value,
    /// The caller's proposed decision (usually `Allow`); the returned
    /// decision is `max(proposed, all handler decisions)` — tighten-only.
    pub proposed: HookDecision,
    pub ctx: &'a HandlerContext,
    /// Feeds the event id.
    pub seq: u64,
}

/// Parameters for running one sync handler (keeps the arity lint happy).
struct RunOneParams<'a> {
    eid: &'a str,
    entity: DocType,
    event: DocEvent,
    doc_id: &'a str,
    working_doc: &'a serde_json::Value,
    ctx: &'a HandlerContext,
}

impl Dispatcher {
    pub fn new(registry: HookRegistry, audit: AuditSink) -> Self {
        Dispatcher {
            registry: Arc::new(registry),
            audit,
            trace: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn trace_push(&self, entry: TraceEntry) {
        let mut trace = self.trace.lock().unwrap_or_else(|e| e.into_inner());
        trace.push(entry);
        const CAP: usize = 500;
        if trace.len() > CAP {
            let excess = trace.len() - CAP;
            trace.drain(..excess);
        }
    }

    /// Recent runs, newest last (for `hooks trace`).
    pub fn trace(&self) -> Vec<TraceEntry> {
        self.trace.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Synchronous dispatch for `before_*`/`validate` events.
    ///
    /// `proposed` is the caller's decision (usually `Allow`); the returned
    /// decision is `max(proposed, all handler decisions)` — tighten-only.
    /// `doc_id` identifies the doc; `seq` feeds the event id.
    pub fn run_sync(&self, p: SyncParams<'_>) -> SyncOutcome {
        let SyncParams {
            entity,
            event,
            doc_id,
            doc,
            proposed,
            ctx,
            seq,
        } = p;
        debug_assert!(
            event.is_sync(),
            "run_sync called for observer event {event}"
        );
        let eid = event_id(entity, doc_id, event, seq);

        // Recursion guard: depth >= MAX_HOOK_DEPTH proceeds with defaults,
        // no hooks fire; the skip is audit-logged.
        if ctx.depth >= MAX_HOOK_DEPTH {
            let run = HandlerRun {
                name: "<dispatcher>".to_string(),
                priority: 0,
                decision: proposed,
                outcome: RunOutcome::SkippedRecursionGuard,
                elapsed_ms: 0,
                message: "recursion_guard: depth >= 3, hooks skipped".to_string(),
                dropped_keys: Vec::new(),
                observer_patch_ignored: false,
                patch: None,
            };
            self.audit.record(&eid, entity, event, doc_id, &run, None);
            self.trace_push(TraceEntry {
                event_id: eid,
                entity: entity.as_str().to_string(),
                event: event.as_str().to_string(),
                handler: run.name.clone(),
                decision: proposed.as_str().to_string(),
                elapsed_ms: 0,
                outcome: run.outcome.as_str().to_string(),
                message: run.message.clone(),
                violation: None,
            });
            return SyncOutcome {
                decision: proposed,
                patched_doc: doc,
                runs: vec![run],
                reject_reason: None,
            };
        }

        let mut decision = proposed;
        let mut patched_doc = doc;
        let mut runs = Vec::new();
        let mut reject_reason: Option<String> = None;

        for handler in self.registry.handlers_for(entity, event) {
            let rp = RunOneParams {
                eid: &eid,
                entity,
                event,
                doc_id,
                working_doc: &patched_doc,
                ctx,
            };
            let mut run = self.run_one_sync(handler, &rp);
            // Tighten-only combination.
            decision = decision.combine(run.decision);
            if run.decision.is_reject() && reject_reason.is_none() {
                reject_reason = Some(if run.message.is_empty() {
                    format!("hook {:?} rejected", run.name)
                } else {
                    run.message.clone()
                });
            }
            // Apply this handler's patch (if any) before the next handler
            // runs, so later handlers see earlier patches.
            if let Some(patch) = run.patch.take() {
                let (doc, dropped) = apply_patch(patched_doc, &patch);
                patched_doc = doc;
                run.dropped_keys = dropped;
            }
            runs.push(run);
        }

        SyncOutcome {
            decision,
            patched_doc,
            runs,
            reject_reason,
        }
    }

    /// Run one sync handler: execute, classify (fail-closed), apply its
    /// patch to the working doc, audit exactly once, trace.
    fn run_one_sync(&self, handler: &HookHandler, r: &RunOneParams<'_>) -> HandlerRun {
        let input = HandlerInput {
            event_id: r.eid.to_string(),
            entity: r.entity.as_str().to_string(),
            event: r.event.as_str().to_string(),
            doc: r.working_doc.clone(),
            context: r.ctx.clone(),
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

        let mut run = HandlerRun {
            name: handler.name.clone(),
            priority: handler.priority,
            decision,
            outcome,
            elapsed_ms: exec.elapsed_ms,
            message,
            dropped_keys: Vec::new(),
            observer_patch_ignored: false,
            patch: None,
        };

        // Patches apply only when the handler ran OK. A failed handler
        // failed closed: no patch, decision already Reject.
        let patch_for_audit: Option<serde_json::Value> = if outcome == RunOutcome::Ok {
            exec.output.as_ref().and_then(|o| {
                if o.is_patch_empty() {
                    None
                } else {
                    Some(o.patch.clone())
                }
            })
        } else {
            None
        };
        run.patch = patch_for_audit.clone();

        self.audit.record(
            r.eid,
            r.entity,
            r.event,
            r.doc_id,
            &run,
            patch_for_audit.as_ref(),
        );
        self.trace_push(TraceEntry {
            event_id: r.eid.to_string(),
            entity: r.entity.as_str().to_string(),
            event: r.event.as_str().to_string(),
            handler: run.name.clone(),
            decision: decision.as_str().to_string(),
            elapsed_ms: run.elapsed_ms,
            outcome: outcome.as_str().to_string(),
            message: run.message.clone(),
            violation: if exec.stderr.is_empty() {
                None
            } else {
                Some(format!("stderr: {}", exec.stderr.trim()))
            },
        });
        run
    }
}

/// Observer dispatch: run each registered observer handler against a
/// FROZEN doc. Patches are ignored and logged as contract violations;
/// the outcome can never change. Returns the per-handler runs (for the
/// outbox worker's ack/retry bookkeeping).
pub fn run_observers(
    dispatcher: &Dispatcher,
    entity: DocType,
    event: DocEvent,
    doc_id: &str,
    frozen_doc: &serde_json::Value,
    ctx: &HandlerContext,
    seq: u64,
) -> Vec<HandlerRun> {
    debug_assert!(
        !event.is_sync(),
        "run_observers called for sync event {event}"
    );
    let eid = event_id(entity, doc_id, event, seq);
    let mut runs = Vec::new();
    for handler in dispatcher.registry.handlers_for(entity, event) {
        let input = HandlerInput {
            event_id: eid.clone(),
            entity: entity.as_str().to_string(),
            event: event.as_str().to_string(),
            doc: frozen_doc.clone(),
            context: ctx.clone(),
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
        let patch_ignored = exec
            .output
            .as_ref()
            .map(|o| !o.is_patch_empty())
            .unwrap_or(false);
        let run = HandlerRun {
            name: handler.name.clone(),
            priority: handler.priority,
            // Observers cannot change the outcome: their decision is
            // recorded but never applied.
            decision,
            outcome,
            elapsed_ms: exec.elapsed_ms,
            message,
            dropped_keys: Vec::new(),
            observer_patch_ignored: patch_ignored,
            patch: None,
        };
        dispatcher.audit.record(
            &eid, entity, event, doc_id, &run,
            None, // observer patches are never applied; nothing to hash
        );
        dispatcher.trace_push(TraceEntry {
            event_id: eid.clone(),
            entity: entity.as_str().to_string(),
            event: event.as_str().to_string(),
            handler: run.name.clone(),
            decision: decision.as_str().to_string(),
            elapsed_ms: run.elapsed_ms,
            outcome: outcome.as_str().to_string(),
            message: run.message.clone(),
            violation: patch_ignored
                .then(|| "contract violation: observer returned a patch; ignored".to_string()),
        });
        runs.push(run);
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::HookRegistry;
    use crate::{DocEvent, DocType, HandlerContext, HookDecision};
    use std::io::Write;

    fn ctx() -> HandlerContext {
        HandlerContext {
            actor: "human:dev-1".to_string(),
            depth: 0,
            dry_run: true,
        }
    }

    fn tmp_audit(name: &str) -> (tempfile::TempDir, AuditSink) {
        let d = tempfile::TempDir::new().unwrap();
        let sink = AuditSink::new(d.path());
        let _ = name;
        (d, sink)
    }

    /// Build a registry from inline TOML with the given dir as base.
    fn registry_with(dir: &std::path::Path, body: &str) -> HookRegistry {
        let p = dir.join("hooks.toml");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        HookRegistry::load(&p, &dir.join("missing.toml")).unwrap()
    }

    #[test]
    fn sync_ordering_priority() {
        // End-to-end through run_sync with a TOML registry: handlers are
        // declared out of priority order and must run lowest-first; each
        // appends its name to a log so execution order is observable.
        let d = tempfile::TempDir::new().unwrap();
        let log = d.path().join("order.log");
        let body = format!(
            r#"
[doc_events.Session]
before_save = [
  {{ name = "late", command = ["sh", "-c", "echo late >> {log}; printf '{{\"decision\":\"allow\"}}'"], priority = 100 }},
  {{ name = "early", command = ["sh", "-c", "echo early >> {log}; printf '{{\"decision\":\"allow\"}}'"], priority = 10 }},
  {{ name = "mid", command = ["sh", "-c", "echo mid >> {log}; printf '{{\"decision\":\"allow\"}}'"], priority = 50 }},
]
"#,
            log = log.display()
        );
        let reg = registry_with(d.path(), &body);
        let ordered: Vec<&str> = reg
            .handlers_for(DocType::Session, DocEvent::BeforeSave)
            .iter()
            .map(|h| h.name.as_str())
            .collect();
        assert_eq!(ordered, vec!["early", "mid", "late"]);

        let (_tmp, audit) = tmp_audit("ordering");
        let disp = Dispatcher::new(reg, audit);
        let out = disp.run_sync(SyncParams {
            entity: DocType::Session,
            event: DocEvent::BeforeSave,
            doc_id: "s-ord",
            doc: serde_json::json!({"id": "s-ord"}),
            proposed: HookDecision::Allow,
            ctx: &ctx(),
            seq: 1,
        });
        assert_eq!(out.runs.len(), 3);
        let fired: Vec<&str> = out.runs.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(fired, vec!["early", "mid", "late"]);
        let logged = std::fs::read_to_string(&log).unwrap();
        assert_eq!(logged, "early\nmid\nlate\n");
    }

    #[test]
    fn patch_applied_to_doc() {
        let d = tempfile::TempDir::new().unwrap();
        let body = r#"
[doc_events.Session]
before_insert = [
  { name = "namer", command = ["sh", "-c", "cat > /dev/null; printf '{\"decision\":\"allow\",\"patch\":{\"title\":\"hello\"}}'"], priority = 10 },
]
"#;
        let reg = registry_with(d.path(), body);
        let (_tmp, audit) = tmp_audit("patch");
        let disp = Dispatcher::new(reg, audit);
        let doc = serde_json::json!({"id": "s-1"});
        let out = disp.run_sync(SyncParams {
            entity: DocType::Session,
            event: DocEvent::BeforeInsert,
            doc_id: "s-1",
            doc,
            proposed: HookDecision::Allow,
            ctx: &ctx(),
            seq: 1,
        });
        assert_eq!(out.decision, HookDecision::Allow);
        assert_eq!(out.runs.len(), 1);
        // The dispatcher applied the patch in execution order.
        assert_eq!(out.patched_doc["title"], "hello");
        assert_eq!(out.patched_doc["id"], "s-1");
        assert!(out.runs[0].dropped_keys.is_empty());
    }

    #[test]
    fn reserved_patch_keys_dropped() {
        let doc = serde_json::json!({"id": "s-1", "tool": "write_file"});
        let patch = serde_json::json!({"id": "evil", "tool": "rm_rf", "note": "ok"});
        let (merged, dropped) = apply_patch(doc, &patch);
        assert_eq!(merged["id"], "s-1");
        assert_eq!(merged["tool"], "write_file");
        assert_eq!(merged["note"], "ok");
        assert_eq!(dropped, vec!["id".to_string(), "tool".to_string()]);
    }

    #[test]
    fn reject_blocks_execution() {
        let d = tempfile::TempDir::new().unwrap();
        let body = r#"
[doc_events.ToolCall]
before_execute = [
  { name = "policy", command = ["sh", "-c", "cat > /dev/null; printf '{\"decision\":\"reject\",\"message\":\"no writes to /etc\"}'"], priority = 10 },
]
"#;
        let reg = registry_with(d.path(), body);
        let (_tmp, audit) = tmp_audit("reject");
        let disp = Dispatcher::new(reg, audit);
        let out = disp.run_sync(SyncParams {
            entity: DocType::ToolCall,
            event: DocEvent::BeforeExecute,
            doc_id: "tc-1",
            doc: serde_json::json!({"id": "tc-1", "tool": "write_file"}),
            proposed: HookDecision::Allow,
            ctx: &ctx(),
            seq: 1,
        });
        assert_eq!(out.decision, HookDecision::Reject);
        assert!(out.reject_reason.unwrap().contains("no writes to /etc"));
        // The emission point maps this to NeverRan { reason: "hook_rejected" }
        // and sends no tool bytes — the dispatcher itself never executes.
    }

    #[test]
    fn tighten_only_never_loosens() {
        // Caller proposed Escalate (Ask); hook says Allow -> stays Escalate.
        let d = tempfile::TempDir::new().unwrap();
        let body = r#"
[doc_events.ToolCall]
before_execute = [
  { name = "lax", command = ["sh", "-c", "cat > /dev/null; printf '{\"decision\":\"allow\"}'"], priority = 10 },
]
"#;
        let reg = registry_with(d.path(), body);
        let (_tmp, audit) = tmp_audit("tighten");
        let disp = Dispatcher::new(reg, audit);
        let out = disp.run_sync(SyncParams {
            entity: DocType::ToolCall,
            event: DocEvent::BeforeExecute,
            doc_id: "tc-2",
            doc: serde_json::json!({"id": "tc-2"}),
            proposed: HookDecision::Escalate,
            ctx: &ctx(),
            seq: 1,
        });
        assert_eq!(out.decision, HookDecision::Escalate);
    }

    #[test]
    fn timeout_fails_closed() {
        let d = tempfile::TempDir::new().unwrap();
        let body = r#"
[doc_events.ToolCall]
before_execute = [
  { name = "slow", command = ["sh", "-c", "sleep 5; printf '{\"decision\":\"allow\"}'"], priority = 10, timeout_ms = 200 },
]
"#;
        let reg = registry_with(d.path(), body);
        let (_tmp, audit) = tmp_audit("timeout");
        let disp = Dispatcher::new(reg, audit);
        let out = disp.run_sync(SyncParams {
            entity: DocType::ToolCall,
            event: DocEvent::BeforeExecute,
            doc_id: "tc-3",
            doc: serde_json::json!({"id": "tc-3"}),
            proposed: HookDecision::Allow,
            ctx: &ctx(),
            seq: 1,
        });
        assert_eq!(out.decision, HookDecision::Reject);
        assert_eq!(out.runs[0].outcome, RunOutcome::Timeout);
        assert!(out.reject_reason.unwrap().contains("hook_timeout"));
    }

    #[test]
    fn observers_cannot_mutate() {
        let d = tempfile::TempDir::new().unwrap();
        let body = r#"
[doc_events.ToolCall]
on_execute = [
  { name = "meddler", command = ["sh", "-c", "cat > /dev/null; printf '{\"decision\":\"reject\",\"patch\":{\"success\":false}}'"], priority = 10 },
]
"#;
        let reg = registry_with(d.path(), body);
        let (_tmp, audit) = tmp_audit("observer");
        let disp = Dispatcher::new(reg, audit);
        let frozen = serde_json::json!({"id": "tc-4", "success": true});
        let runs = run_observers(
            &disp,
            DocType::ToolCall,
            DocEvent::OnExecute,
            "tc-4",
            &frozen,
            &ctx(),
            1,
        );
        assert_eq!(runs.len(), 1);
        assert!(runs[0].observer_patch_ignored);
        // The recorded outcome is unchanged: the frozen doc is untouched.
        assert_eq!(frozen["success"], true);
        // And the trace carries the contract violation.
        let trace = disp.trace();
        assert_eq!(trace.len(), 1);
        assert!(trace[0]
            .violation
            .as_ref()
            .unwrap()
            .contains("contract violation"));
    }

    #[test]
    fn exactly_one_audit_entry_per_run() {
        use supercli_core::action_reviews::verify_review_chain;
        let d = tempfile::TempDir::new().unwrap();
        let body = r#"
[doc_events.Session]
before_save = [
  { name = "a1", command = ["sh", "-c", "cat > /dev/null; printf '{\"decision\":\"allow\"}'"], priority = 10 },
  { name = "a2", command = ["sh", "-c", "cat > /dev/null; printf '{\"decision\":\"escalate\"}'"], priority = 20 },
]
"#;
        let reg = registry_with(d.path(), body);
        let (tmp, audit) = tmp_audit("audit-one");
        let disp = Dispatcher::new(reg, audit);
        let out = disp.run_sync(SyncParams {
            entity: DocType::Session,
            event: DocEvent::BeforeSave,
            doc_id: "s-9",
            doc: serde_json::json!({"id": "s-9"}),
            proposed: HookDecision::Allow,
            ctx: &ctx(),
            seq: 7,
        });
        assert_eq!(out.runs.len(), 2);
        assert_eq!(out.decision, HookDecision::Escalate);
        // Count hook audit entries in the shared review log: each run is
        // one `review` entry with connector "hook" and actor "hook:<name>".
        // The event id is inside the hashed args payload; the tool field
        // carries the entity:event:handler identity.
        let log = std::fs::read_to_string(tmp.path().join("action-reviews.jsonl")).unwrap();
        let mut count = 0;
        for line in log.lines() {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            if v.get("connector").and_then(|t| t.as_str()) == Some("hook") {
                count += 1;
                let actor = v.get("actor").and_then(|a| a.as_str()).unwrap();
                assert!(actor.starts_with("hook:"));
                let tool = v.get("tool").and_then(|t| t.as_str()).unwrap();
                assert!(tool.starts_with("Session:before_save:"));
            }
        }
        assert_eq!(count, 2, "exactly one audit entry per handler run");
        // And the whole chain still verifies.
        assert_eq!(verify_review_chain(tmp.path()).unwrap(), 2);
    }

    #[test]
    fn recursion_guard_caps_at_depth_3() {
        let d = tempfile::TempDir::new().unwrap();
        let body = r#"
[doc_events.Session]
before_save = [
  { name = "loop", command = ["sh", "-c", "cat > /dev/null; printf '{\"decision\":\"allow\"}'"], priority = 10 },
]
"#;
        let reg = registry_with(d.path(), body);
        let (_tmp, audit) = tmp_audit("recursion");
        let disp = Dispatcher::new(reg, audit);
        // Simulate a handler that mutates a doc at each level, 5 deep.
        let mut fired_depths = Vec::new();
        for depth in 0..5u32 {
            let c = HandlerContext {
                actor: "human:dev-1".to_string(),
                depth,
                dry_run: true,
            };
            let out = disp.run_sync(SyncParams {
                entity: DocType::Session,
                event: DocEvent::BeforeSave,
                doc_id: "s-loop",
                doc: serde_json::json!({"id": "s-loop"}),
                proposed: HookDecision::Allow,
                ctx: &c,
                seq: 1,
            });
            let real_runs: Vec<_> = out
                .runs
                .iter()
                .filter(|r| r.outcome != RunOutcome::SkippedRecursionGuard)
                .collect();
            if !real_runs.is_empty() {
                fired_depths.push(depth);
            } else {
                // Guard engaged: the run is the dispatcher skip marker.
                assert_eq!(out.runs.len(), 1);
                assert_eq!(out.runs[0].outcome, RunOutcome::SkippedRecursionGuard);
            }
        }
        assert_eq!(fired_depths, vec![0, 1, 2], "hooks fire only below depth 3");
    }

    #[test]
    fn hook_actor_display_parse_roundtrip() {
        use supercli_core::action_reviews::Actor;
        let a = Actor::Hook {
            name: "policy-check".to_string(),
        };
        assert_eq!(a.to_string(), "hook:policy-check");
        let b = Actor::parse("hook:policy-check");
        assert_eq!(b.to_string(), "hook:policy-check");
        // Unknown prefixes still fall back (chain safety).
        assert_eq!(Actor::parse("bogus").to_string(), "human:unidentified");
    }

    #[test]
    fn hook_run_chain_rejects_tampered_byte() {
        use supercli_core::action_reviews::{
            record_review, verify_review_chain, Actor, ReviewDecision,
        };
        let tmp = tempfile::TempDir::new().unwrap();
        // One hook run recorded through the shared review log.
        record_review(
            tmp.path(),
            Actor::Hook {
                name: "policy".to_string(),
            },
            "hook",
            "Session:before_save:policy",
            "deadbeef",
            ReviewDecision::Approved,
            None,
        )
        .unwrap();
        assert_eq!(verify_review_chain(tmp.path()).unwrap(), 1);
        // Flip one byte in the line: the canonical-form check must catch it.
        let path = tmp.path().join("action-reviews.jsonl");
        let mut bytes = std::fs::read(&path).unwrap();
        let pos = bytes.iter().position(|&b| b == b'a').unwrap();
        bytes[pos] = b'b';
        std::fs::write(&path, &bytes).unwrap();
        assert!(verify_review_chain(tmp.path()).is_err());
    }
}
