//! Frappe-style document lifecycle events (`doc_events`) for supercli.
//!
//! Users script supercli through document lifecycle hooks declared in
//! `hooks.toml` (global `~/.supercli/hooks.toml`, per-project
//! `.supercli/hooks.toml`). This crate implements the event catalog, the
//! tighten-only decision lattice, the handler registry, the synchronous
//! `before_*`/`validate` dispatcher, the at-least-once observer path, and
//! the hash-chained audit integration. It builds on three existing
//! primitives and invents no parallel event system:
//!
//! - `supercli_core::action_reviews` — the write-ahead review log, hash
//!   chain, and `Actor::Hook` variant (every hook run is one more entry);
//! - `AttemptOutcome::{Executed, Ambiguous, NeverRan}` — maps to the
//!   ToolCall observer events;
//! - the host's outbox/replay pattern — observer deliveries are durable
//!   across Host restarts with event-id dedup.
//!
//! Phase 1 handlers: shell commands and localhost-only webhooks sharing one
//! JSON contract. Phase 2 (embedded scripting) sits behind the
//! `events-script` cargo feature and is not implemented here.

pub mod cli;
pub mod handlers;
pub mod outbox;
pub mod registry;
pub mod runner;

use serde::{Deserialize, Serialize};
use std::fmt;

/// A scriptable supercli entity ("doctype" in Frappe terms).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DocType {
    Session,
    Turn,
    ToolCall,
    Approval,
    Grant,
    Schedule,
    Job,
    Connector,
    Device,
    FileWrite,
    /// Reserved for the idea-surface entity (ships later).
    Idea,
}

impl DocType {
    /// Parse the `Entity` key used in `[doc_events.<Entity>]`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Session" => Some(DocType::Session),
            "Turn" => Some(DocType::Turn),
            "ToolCall" => Some(DocType::ToolCall),
            "Approval" => Some(DocType::Approval),
            "Grant" => Some(DocType::Grant),
            "Schedule" => Some(DocType::Schedule),
            "Job" => Some(DocType::Job),
            "Connector" => Some(DocType::Connector),
            "Device" => Some(DocType::Device),
            "FileWrite" => Some(DocType::FileWrite),
            "Idea" => Some(DocType::Idea),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DocType::Session => "Session",
            DocType::Turn => "Turn",
            DocType::ToolCall => "ToolCall",
            DocType::Approval => "Approval",
            DocType::Grant => "Grant",
            DocType::Schedule => "Schedule",
            DocType::Job => "Job",
            DocType::Connector => "Connector",
            DocType::Device => "Device",
            DocType::FileWrite => "FileWrite",
            DocType::Idea => "Idea",
        }
    }
}

impl fmt::Display for DocType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A lifecycle event. Frappe names are used wherever the meaning matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocEvent {
    // -- Lifecycle (all entities unless noted) --
    BeforeInsert,
    /// Naming-series-style name assignment (Session, Schedule/Job only).
    Autoname,
    AfterInsert,
    BeforeValidate,
    Validate,
    BeforeSave,
    OnUpdate,
    /// After an update, only if a field actually changed.
    OnChange,
    OnTrash,
    AfterDelete,
    /// Session only.
    OnArchive,
    /// Session only.
    OnRestore,
    // -- Approval --
    /// An answer is proposed; the hook may reject it (the approval stays
    /// pending) but can never substitute its own answer.
    BeforeSubmit,
    OnSubmit,
    OnCancel,
    OnExpire,
    // -- ToolCall --
    /// After the write-ahead review fsyncs, before any tool bytes are sent.
    BeforeExecute,
    /// After `AttemptOutcome::Executed { success: true }` is recorded.
    OnExecute,
    /// After `AttemptOutcome::Executed { success: false }` is recorded.
    OnFail,
    /// After `AttemptOutcome::Ambiguous` or `NeverRan` is recorded.
    OnOutcomeUnknown,
}

impl DocEvent {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "before_insert" => Some(DocEvent::BeforeInsert),
            "autoname" => Some(DocEvent::Autoname),
            "after_insert" => Some(DocEvent::AfterInsert),
            "before_validate" => Some(DocEvent::BeforeValidate),
            "validate" => Some(DocEvent::Validate),
            "before_save" => Some(DocEvent::BeforeSave),
            "on_update" => Some(DocEvent::OnUpdate),
            "on_change" => Some(DocEvent::OnChange),
            "on_trash" => Some(DocEvent::OnTrash),
            "after_delete" => Some(DocEvent::AfterDelete),
            "on_archive" => Some(DocEvent::OnArchive),
            "on_restore" => Some(DocEvent::OnRestore),
            "before_submit" => Some(DocEvent::BeforeSubmit),
            "on_submit" => Some(DocEvent::OnSubmit),
            "on_cancel" => Some(DocEvent::OnCancel),
            "on_expire" => Some(DocEvent::OnExpire),
            "before_execute" => Some(DocEvent::BeforeExecute),
            "on_execute" => Some(DocEvent::OnExecute),
            "on_fail" => Some(DocEvent::OnFail),
            "on_outcome_unknown" => Some(DocEvent::OnOutcomeUnknown),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DocEvent::BeforeInsert => "before_insert",
            DocEvent::Autoname => "autoname",
            DocEvent::AfterInsert => "after_insert",
            DocEvent::BeforeValidate => "before_validate",
            DocEvent::Validate => "validate",
            DocEvent::BeforeSave => "before_save",
            DocEvent::OnUpdate => "on_update",
            DocEvent::OnChange => "on_change",
            DocEvent::OnTrash => "on_trash",
            DocEvent::AfterDelete => "after_delete",
            DocEvent::OnArchive => "on_archive",
            DocEvent::OnRestore => "on_restore",
            DocEvent::BeforeSubmit => "before_submit",
            DocEvent::OnSubmit => "on_submit",
            DocEvent::OnCancel => "on_cancel",
            DocEvent::OnExpire => "on_expire",
            DocEvent::BeforeExecute => "before_execute",
            DocEvent::OnExecute => "on_execute",
            DocEvent::OnFail => "on_fail",
            DocEvent::OnOutcomeUnknown => "on_outcome_unknown",
        }
    }

    /// `true` for the synchronous interception events (`before_*` and
    /// `validate`): these run in the mutating call path, may patch the
    /// proposed doc, and may reject. Everything else is an observer.
    pub fn is_sync(self) -> bool {
        matches!(
            self,
            DocEvent::BeforeInsert
                | DocEvent::Autoname
                | DocEvent::BeforeValidate
                | DocEvent::Validate
                | DocEvent::BeforeSave
                | DocEvent::BeforeSubmit
                | DocEvent::BeforeExecute
                | DocEvent::OnTrash
        )
    }
}

impl fmt::Display for DocEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The decision a handler returns. Forms a strictness lattice:
///
/// `allow < escalate < reject`
///
/// Hooks can only TIGHTEN: the effective decision is the maximum strictness
/// across all handlers and the caller's proposed decision. A hook can
/// never turn Ask/Deny into Allow and can never answer an approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookDecision {
    Allow,
    /// Escalate Allow -> Ask (meaningful for policy-gated entities).
    Escalate,
    Reject,
}

impl HookDecision {
    fn strictness(self) -> u8 {
        match self {
            HookDecision::Allow => 0,
            HookDecision::Escalate => 1,
            HookDecision::Reject => 2,
        }
    }

    /// The tighten-only combination: the stricter of the two wins.
    /// Monotonic — strictness never decreases.
    pub fn combine(self, other: HookDecision) -> HookDecision {
        if other.strictness() > self.strictness() {
            other
        } else {
            self
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "allow" => Some(HookDecision::Allow),
            "escalate" => Some(HookDecision::Escalate),
            "reject" => Some(HookDecision::Reject),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            HookDecision::Allow => "allow",
            HookDecision::Escalate => "escalate",
            HookDecision::Reject => "reject",
        }
    }

    /// A rejection reason for this decision, if any.
    pub fn is_reject(self) -> bool {
        self == HookDecision::Reject
    }
}

impl fmt::Display for HookDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a hook run ended (for the audit entry's `outcome` field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// Handler ran and returned a decision.
    Ok,
    /// Handler exceeded `timeout_ms` (fail-closed: treated as reject).
    Timeout,
    /// Handler crashed / bad output / config error (fail-closed).
    Crash,
    /// Skipped by the recursion guard (depth >= 3).
    SkippedRecursionGuard,
}

impl RunOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            RunOutcome::Ok => "ok",
            RunOutcome::Timeout => "timeout",
            RunOutcome::Crash => "crash",
            RunOutcome::SkippedRecursionGuard => "skipped_recursion_guard",
        }
    }
}

/// The raw JSON contract every handler speaks: JSON doc on stdin / POST
/// body in, `{decision, patch, message}` out. Identical for Phase 1 shell
/// handlers, localhost webhooks, and Phase 2 scripts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerInput {
    pub event_id: String,
    pub entity: String,
    pub event: String,
    pub doc: serde_json::Value,
    pub context: HandlerContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerContext {
    pub actor: String,
    pub depth: u32,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerOutput {
    #[serde(default = "default_allow")]
    pub decision: String,
    #[serde(default)]
    pub patch: serde_json::Value,
    #[serde(default)]
    pub message: String,
}

fn default_allow() -> String {
    "allow".to_string()
}

impl HandlerOutput {
    /// Parse handler output. Unknown decision strings fail closed to
    /// `reject` with reason `hook_bad_output` (design §4.1, §6).
    pub fn decision_or_reject(&self) -> (HookDecision, Option<&'static str>) {
        match self.decision.as_str() {
            "allow" => (HookDecision::Allow, None),
            "escalate" => (HookDecision::Escalate, None),
            "reject" => (HookDecision::Reject, None),
            _ => (HookDecision::Reject, Some("hook_bad_output")),
        }
    }

    pub fn is_patch_empty(&self) -> bool {
        self.patch.is_null()
            || self
                .patch
                .as_object()
                .map(|o| o.is_empty())
                .unwrap_or(false)
    }
}

/// Maximum hook recursion depth (Frappe `flags` equivalent). A mutation at
/// depth >= [`MAX_HOOK_DEPTH`] proceeds with defaults and no hooks fire.
pub const MAX_HOOK_DEPTH: u32 = 3;

/// Default per-handler time budget in milliseconds.
pub const DEFAULT_TIMEOUT_MS: u64 = 2000;

/// Default handler priority (lower runs first).
pub const DEFAULT_PRIORITY: u32 = 100;

/// Idempotency-key format: `<Entity>:<doc_id>:<event>:<seq>`.
pub fn event_id(entity: DocType, doc_id: &str, event: DocEvent, seq: u64) -> String {
    format!("{}:{}:{}:{}", entity, doc_id, event, seq)
}

/// Non-invasive emission helper for integrating doc events into existing
/// code paths (ToolCall, Approval, Session).
///
/// If no `hooks.toml` exists or no handlers are registered for the
/// entity/event, this is a fast no-op (registry load fails or returns
/// empty). Otherwise, it dispatches synchronously via the Dispatcher.
///
/// This is intentionally simple: production Host integration would thread
/// a shared Dispatcher, but this helper lets existing call sites emit
/// events without restructuring.
pub mod emit {
    use crate::registry::{default_paths, HookRegistry};
    use crate::runner::{AuditSink, Dispatcher, SyncOutcome, SyncParams};
    use crate::{DocEvent, DocType, HandlerContext, HookDecision};
    use std::path::Path;

    /// Emit a synchronous (`before_*`/`validate`) event.
    ///
    /// Returns `None` if no handlers are registered (fast path).
    /// Returns `Some(SyncOutcome)` if handlers ran.
    pub fn emit_sync(
        entity: DocType,
        event: DocEvent,
        doc_id: &str,
        doc: serde_json::Value,
        audit_dir: &Path,
        actor: &str,
    ) -> Option<SyncOutcome> {
        // Fast path: if no hooks.toml exists at all, skip.
        let (global, project) = default_paths();
        if !global.exists() && !project.exists() {
            return None;
        }
        let registry = HookRegistry::load(&global, &project).ok()?;
        if !registry.has_handlers(entity, event) {
            return None;
        }
        let audit = AuditSink::new(audit_dir);
        let dispatcher = Dispatcher::new(registry, audit);
        let ctx = HandlerContext {
            actor: actor.to_string(),
            depth: 0,
            dry_run: false,
        };
        let params = SyncParams {
            entity,
            event,
            doc_id,
            doc,
            proposed: HookDecision::Allow,
            ctx: &ctx,
            seq: 0,
        };
        Some(dispatcher.run_sync(params))
    }

    /// Emit an observer (`after_*`/`on_*`) event.
    ///
    /// Observers run after the state change is durable. They cannot mutate
    /// the outcome. This is a fast no-op if no handlers are registered.
    pub fn emit_observer(
        entity: DocType,
        event: DocEvent,
        doc_id: &str,
        doc: serde_json::Value,
        audit_dir: &Path,
        actor: &str,
    ) {
        let (global, project) = default_paths();
        if !global.exists() && !project.exists() {
            return;
        }
        let registry = match HookRegistry::load(&global, &project) {
            Ok(r) => r,
            Err(_) => return,
        };
        if !registry.has_handlers(entity, event) {
            return;
        }
        let audit = AuditSink::new(audit_dir);
        let dispatcher = Dispatcher::new(registry, audit);
        let ctx = HandlerContext {
            actor: actor.to_string(),
            depth: 0,
            dry_run: false,
        };
        crate::runner::run_observers(&dispatcher, entity, event, doc_id, &doc, &ctx, 0);
    }
}
