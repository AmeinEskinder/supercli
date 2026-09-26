//! Durable runs — crash-safe agent run journal.
//!
//! Design: `docs/durable-runs.md`.
//!
//! Every run (agent turn loop, subagent, long task plan) gets a journal in
//! `<SUPERCLI_HOME>/runs.db` (SQLite, WAL):
//!
//! - `runs` — run metadata: id, parent, state, plan, budgets, lease mirror
//! - `run_steps` — append-only step journal; a retry is a new row with
//!   `attempt+1`, never an UPDATE. The latest row per `step_no` wins.
//! - `run_events` — UI replay log (durable counterpart of the event bus).
//!
//! Leases reuse [`crate::schedule_leases`]: each run id is claimed as a
//! schedule id under tenant `"default"`. No new lease protocol.
//!
//! ## Write-ahead step protocol
//!
//! The hard crash case is *between* the side effect and the journal write:
//! a crash after the tool ran but before the outcome was journaled, or
//! after the intent but before the tool ran. A single post-hoc outcome
//! row cannot distinguish these, so steps use two journal rows:
//!
//! 1. **Fence first.** The worker verifies its lease fence is current
//!    *before* creating an intent. A worker refused by the fence never
//!    creates an intent at all — it simply stops.
//! 2. [`RunsDb::begin_step`] — appends an intent row (`outcome='started'`)
//!    and **fsyncs the database before returning**. The intent is durable
//!    before the side effect executes.
//! 3. Execute the side effect.
//! 4. [`RunsDb::complete_step`] — appends the outcome row (`attempt+1`).
//!
//! If the worker dies between 2 and 4, resume finds the orphaned intent
//! via [`RunsDb::find_orphaned_intents`] and classifies it with
//! [`classify_orphan`]. A worker that loses its fence *after* the intent
//! but *before* the call journals `never_ran` explicitly through
//! `complete_step` (or dies — which the orphan rule below covers as the
//! conservative superset).
//!
//! Safety invariants (non-negotiable):
//!
//! 1. Never re-execute a completed side effect. Completed = latest journal
//!    row has a decided outcome (anything but `'started'`). Replay the
//!    recorded output, don't re-call.
//! 2. `ambiguous` steps are never replayed: the run goes to NEEDS_REVIEW
//!    and waits for a human (same rule as Phase 5 attempt outcomes). An
//!    orphaned tool/subagent intent is `ambiguous` for the same reason:
//!    the side effect may have executed after the intent fsync.
//! 3. Fence before every side effect: a worker that lost its lease must
//!    journal `never_ran` and stop, never emit tool calls.
//! 4. State transitions are conditional single-row UPDATEs; a lost race
//!    returns 0 rows and the worker treats it as "someone else owns it".
//! 5. `begin_step` fails closed: if the intent cannot be fsync'd, the step
//!    must not execute.
//!
//! ## Reconciliation (before human review)
//!
//! A crash that orphans an intent does not automatically park the run.
//! [`reconcile_orphan`] runs first, per step kind:
//!
//! - `model` / `read`: re-run — no external side effect exists to
//!   duplicate.
//! - `file_write`: probe the target file; content hash equal to the
//!   intended hash (carried in the intent's reconcile hint) proves the
//!   write landed → journal as executed without re-running. Otherwise
//!   re-run (the rerun overwrites, so a torn write cannot survive).
//! - `idempotent_http`: re-run with the SAME idempotency key (the step id,
//!   carried in the hint); the remote side dedups, so a double landing is
//!   still a single effect.
//! - `opaque_write` (and legacy `tool`/`subagent`): nothing can prove the
//!   step's fate → NEEDS_REVIEW, never replayed.
//!
//! Only steps with no probe and no idempotency key ever reach review.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OptionalExtension};

use crate::action_reviews::AttemptOutcome;
use crate::browser_engine::sha256_hex;
use crate::schedule_leases::{LeaseError, LeaseFence, ScheduleLeases};

/// Database clock in milliseconds, evaluated inside SQL (same expression as
/// `schedule_leases::DB_NOW_MS`; duplicated as a SQL literal, not logic).
const DB_NOW_MS: &str = "(CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))";

/// Lease tenant used for run leases inside the shared lease table.
pub const RUN_LEASE_TENANT: &str = "default";

/// Outcome strings stored in `run_steps.outcome`. They mirror
/// [`AttemptOutcome`]'s classifier: `executed_ok`/`executed_failed` for
/// [`AttemptOutcome::Executed`], `ambiguous`, `never_ran`.
///
/// `started` is the write-ahead intent marker (see [`RunsDb::begin_step`]):
/// it is not a decided outcome. A `started` row that is still the latest
/// row for its `step_no` is an *orphaned intent* — the worker died after
/// the intent fsync and before journaling any outcome.
pub const OUTCOME_EXECUTED_OK: &str = "executed_ok";
pub const OUTCOME_EXECUTED_FAILED: &str = "executed_failed";
pub const OUTCOME_AMBIGUOUS: &str = "ambiguous";
pub const OUTCOME_NEVER_RAN: &str = "never_ran";
pub const OUTCOME_STARTED: &str = "started";

/// Map an [`AttemptOutcome`] to its stored outcome string.
pub fn outcome_str(outcome: &AttemptOutcome) -> &'static str {
    match outcome {
        AttemptOutcome::Executed { success: true } => OUTCOME_EXECUTED_OK,
        AttemptOutcome::Executed { success: false } => OUTCOME_EXECUTED_FAILED,
        AttemptOutcome::Ambiguous { .. } => OUTCOME_AMBIGUOUS,
        AttemptOutcome::NeverRan { .. } => OUTCOME_NEVER_RAN,
    }
}

/// Classify an orphaned intent (a `started` row with no outcome row) using
/// the [`AttemptOutcome`] vocabulary.
///
/// This is a thin projection of [`reconcile_orphan`] onto the
/// `AttemptOutcome` vocabulary: `Rerun` → `NeverRan`, `AlreadyComplete` →
/// `Executed`, `NeedsReview` → `Ambiguous`. Prefer [`reconcile_orphan`]
/// when the reconciliation output (e.g. the probe-match record) matters.
pub fn classify_orphan(intent: &StepIntent) -> AttemptOutcome {
    match reconcile_orphan(intent) {
        ReconcileVerdict::Rerun { reason } => AttemptOutcome::NeverRan { reason },
        ReconcileVerdict::AlreadyComplete { .. } => AttemptOutcome::Executed { success: true },
        ReconcileVerdict::NeedsReview { reason } => AttemptOutcome::Ambiguous { reason },
    }
}

/// Errors from the durable-runs store.
#[derive(Debug)]
pub enum RunsError {
    /// The database file could not be opened/created.
    Open(String),
    /// A SQL statement failed.
    Sql(String),
    /// The lease layer refused or failed.
    Lease(String),
    /// No run with this id exists.
    NotFound(String),
    /// A budget was exceeded; the run was transitioned to FAILED.
    BudgetExceeded { run_id: String, budget: String },
    /// The run is in a state that forbids the requested operation.
    InvalidState(String),
}

impl std::fmt::Display for RunsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunsError::Open(e) => write!(f, "durable_runs: open failed: {e}"),
            RunsError::Sql(e) => write!(f, "durable_runs: sql failed: {e}"),
            RunsError::Lease(e) => write!(f, "durable_runs: lease failed: {e}"),
            RunsError::NotFound(id) => write!(f, "durable_runs: run not found: {id}"),
            RunsError::BudgetExceeded { run_id, budget } => {
                write!(f, "durable_runs: run {run_id} exceeded budget {budget}")
            }
            RunsError::InvalidState(e) => write!(f, "durable_runs: invalid state: {e}"),
        }
    }
}

impl std::error::Error for RunsError {}

impl From<rusqlite::Error> for RunsError {
    fn from(e: rusqlite::Error) -> Self {
        RunsError::Sql(e.to_string())
    }
}

impl From<LeaseError> for RunsError {
    fn from(e: LeaseError) -> Self {
        RunsError::Lease(e.to_string())
    }
}

/// Run lifecycle states (docs/durable-runs.md §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Queued,
    AwaitingModel,
    ExecutingTools,
    AwaitingApproval,
    Paused,
    Done,
    Failed,
    NeedsReview,
}

impl RunState {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunState::Queued => "QUEUED",
            RunState::AwaitingModel => "AWAITING_MODEL",
            RunState::ExecutingTools => "EXECUTING_TOOLS",
            RunState::AwaitingApproval => "AWAITING_APPROVAL",
            RunState::Paused => "PAUSED",
            RunState::Done => "DONE",
            RunState::Failed => "FAILED",
            RunState::NeedsReview => "NEEDS_REVIEW",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "QUEUED" => Some(RunState::Queued),
            "AWAITING_MODEL" => Some(RunState::AwaitingModel),
            "EXECUTING_TOOLS" => Some(RunState::ExecutingTools),
            "AWAITING_APPROVAL" => Some(RunState::AwaitingApproval),
            "PAUSED" => Some(RunState::Paused),
            "DONE" => Some(RunState::Done),
            "FAILED" => Some(RunState::Failed),
            "NEEDS_REVIEW" => Some(RunState::NeedsReview),
            _ => None,
        }
    }

    /// Terminal: the run will never move again on its own.
    pub fn is_terminal(&self) -> bool {
        matches!(self, RunState::Done | RunState::Failed)
    }
}

/// Step kinds in the journal (docs/durable-runs.md §2.2).
///
/// Each kind declares how an orphaned intent (crash between the intent
/// fsync and the outcome journal) is reconciled on resume — see
/// [`reconcile_orphan`]:
///
/// - `Model` / `ReadOnly`: no external side effect (or a side-effect-free
///   read) — safe to re-run.
/// - `FileWrite`: the target file is probed — content hash compared to the
///   intended hash carried in the step's reconcile hint. Match → the write
///   landed before the crash (journal as executed, don't re-run);
///   mismatch/missing → it didn't (safe to re-run; the rerun overwrites).
/// - `IdempotentHttp`: replayed with the SAME idempotency key (the step
///   id, carried in the reconcile hint); the remote side dedups.
/// - `OpaqueWrite`: no probe and no idempotency key exists — the only kind
///   that goes to NEEDS_REVIEW.
/// - `Tool` / `Subagent`: legacy kinds from before reconciliation; treated
///   like `OpaqueWrite` (fail closed, never replay).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepKind {
    Model,
    ReadOnly,
    Tool,
    Subagent,
    FileWrite,
    IdempotentHttp,
    OpaqueWrite,
}

impl StepKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            StepKind::Model => "model",
            StepKind::ReadOnly => "read",
            StepKind::Tool => "tool",
            StepKind::Subagent => "subagent",
            StepKind::FileWrite => "file_write",
            StepKind::IdempotentHttp => "idempotent_http",
            StepKind::OpaqueWrite => "opaque_write",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "model" => Some(StepKind::Model),
            "read" => Some(StepKind::ReadOnly),
            "tool" => Some(StepKind::Tool),
            "subagent" => Some(StepKind::Subagent),
            "file_write" => Some(StepKind::FileWrite),
            "idempotent_http" => Some(StepKind::IdempotentHttp),
            "opaque_write" => Some(StepKind::OpaqueWrite),
            _ => None,
        }
    }

    /// True for kinds whose execution performs a tool-like side effect
    /// (used for `max_tool_calls` budget accounting).
    fn counts_as_tool_call(self) -> bool {
        matches!(
            self,
            StepKind::Tool | StepKind::FileWrite | StepKind::IdempotentHttp | StepKind::OpaqueWrite
        )
    }
}

/// Reconciliation parameters for an orphaned intent, stored as JSON in
/// `run_steps.reconcile_hint`. Only the fields relevant to the step's kind
/// are populated.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ReconcileHint {
    /// FileWrite: absolute path of the probe target.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub path: Option<String>,
    /// FileWrite: expected SHA-256 hex of the intended file content.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub expected_hash: Option<String>,
    /// IdempotentHttp: the idempotency key to replay with (the step id).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub idempotency_key: Option<String>,
}

impl ReconcileHint {
    pub fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }

    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }
}

/// The verdict of reconciling one orphaned intent on resume.
#[derive(Debug, Clone)]
pub enum ReconcileVerdict {
    /// Safe to re-execute: the step has no external side effect
    /// (`Model`, `ReadOnly`), the probe proved the side effect did NOT
    /// land (`FileWrite` with missing/mismatched content), or the replay
    /// carries the same idempotency key and the remote dedups
    /// (`IdempotentHttp`).
    Rerun { reason: String },
    /// The probe proved the side effect already landed before the crash
    /// (`FileWrite` whose content hash matches). `output` describes the
    /// reconciliation and is journaled as the step's outcome — the step is
    /// NOT re-executed.
    AlreadyComplete { reason: String, output: String },
    /// Nothing can prove whether the side effect landed (`OpaqueWrite`,
    /// legacy `Tool`/`Subagent`, or a `FileWrite`/`IdempotentHttp` whose
    /// hint is missing or malformed). The run is parked in NEEDS_REVIEW
    /// and the step is never replayed.
    NeedsReview { reason: String },
}

/// Result of probing a `FileWrite` target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileProbe {
    /// The file exists and its SHA-256 matches the intended hash: the
    /// write landed before the crash.
    Completed,
    /// The file is missing or its content differs: the write did not land
    /// (or was torn mid-write — a rerun overwrites it wholesale).
    Incomplete,
    /// No usable hint (missing/malformed): cannot probe at all.
    NoProbe,
}

/// Probe the `FileWrite` target: read the file and compare its SHA-256 to
/// the intended hash from the reconcile hint.
///
/// Soundness: the hint (path + expected hash) is journaled in the same
/// fsync'd intent row as the step itself, so a surviving orphan always
/// carries its own probe parameters. The probe only *reads*; it never
/// mutates. A torn write (crash mid-write) yields a hash mismatch, which
/// correctly classifies as `Incomplete` — the rerun overwrites the file
/// completely, so no partial content survives.
fn probe_file_write(hint_json: Option<&str>) -> FileProbe {
    let hint = hint_json.and_then(ReconcileHint::from_json);
    let (path, expected) = match hint {
        Some(h) => match (h.path, h.expected_hash) {
            (Some(p), Some(e)) => (p, e),
            _ => return FileProbe::NoProbe,
        },
        None => return FileProbe::NoProbe,
    };
    match std::fs::read(&path) {
        Ok(bytes) if sha256_hex(&bytes) == expected => FileProbe::Completed,
        _ => FileProbe::Incomplete,
    }
}

/// Reconcile an orphaned intent (a `started` row with no outcome row).
///
/// This runs *before* falling back to human review, so that a crash only
/// parks the run when nothing can prove the step's fate:
///
/// - `Model` / `ReadOnly`: re-running spends tokens / re-reads, but cannot
///   duplicate a mutation → `Rerun`.
/// - `FileWrite`: probe the target. Hash matches → `AlreadyComplete`
///   (journal as executed, never re-run the write). Otherwise → `Rerun`
///   (the rerun overwrites, so a torn partial write cannot survive).
/// - `IdempotentHttp`: → `Rerun`; the worker replays with the SAME
///   idempotency key from the hint, and the remote side dedups — a replay
///   that lands twice is still a single effect.
/// - `OpaqueWrite` / legacy `Tool` / `Subagent`: no probe and no key can
///   prove anything → `NeedsReview`. The step is never replayed.
///
/// Fail-closed: a `FileWrite` or `IdempotentHttp` whose hint is missing or
/// malformed degrades to `NeedsReview`, never to `Rerun`.
pub fn reconcile_orphan(intent: &StepIntent) -> ReconcileVerdict {
    match intent.kind {
        StepKind::Model => ReconcileVerdict::Rerun {
            reason: "orphaned model intent: model calls have no external \
                     side effect; safe to re-run"
                .to_string(),
        },
        StepKind::ReadOnly => ReconcileVerdict::Rerun {
            reason: "orphaned read-only intent: reads have no side effect; \
                     safe to re-run"
                .to_string(),
        },
        StepKind::FileWrite => match probe_file_write(intent.reconcile_hint.as_deref()) {
            FileProbe::Completed => {
                let hash = intent
                    .reconcile_hint
                    .as_deref()
                    .and_then(ReconcileHint::from_json)
                    .and_then(|h| h.expected_hash)
                    .unwrap_or_default();
                ReconcileVerdict::AlreadyComplete {
                    reason: "file probe: target content hash matches the \
                             intended hash; the write landed before the crash"
                        .to_string(),
                    output: format!("{{\"reconciled\":\"probe_match\",\"hash\":\"{hash}\"}}"),
                }
            }
            FileProbe::Incomplete => ReconcileVerdict::Rerun {
                reason: "file probe: target missing or content differs; the \
                         write did not land — safe to re-run (rerun overwrites)"
                    .to_string(),
            },
            FileProbe::NoProbe => ReconcileVerdict::NeedsReview {
                reason: "file_write intent has no usable reconcile hint; \
                         cannot probe — fail closed to review"
                    .to_string(),
            },
        },
        StepKind::IdempotentHttp => {
            match intent
                .reconcile_hint
                .as_deref()
                .and_then(ReconcileHint::from_json)
                .and_then(|h| h.idempotency_key)
            {
                Some(key) => ReconcileVerdict::Rerun {
                    reason: format!(
                        "idempotent_http intent: replay with the same \
                         idempotency key ({key}); the remote side dedups"
                    ),
                },
                None => ReconcileVerdict::NeedsReview {
                    reason: "idempotent_http intent has no idempotency key; \
                             cannot replay safely — fail closed to review"
                        .to_string(),
                },
            }
        }
        StepKind::OpaqueWrite | StepKind::Tool | StepKind::Subagent => {
            ReconcileVerdict::NeedsReview {
                reason: "orphaned intent with no outcome row and no \
                         reconciliation strategy: the side effect may have \
                         executed after the intent fsync; never replay"
                    .to_string(),
            }
        }
    }
}

/// One run row.
#[derive(Debug, Clone)]
pub struct Run {
    pub id: String,
    pub parent_run: Option<String>,
    pub state: RunState,
    pub plan: String,
    pub budgets: String,
    pub lease_owner: Option<String>,
    pub lease_generation: u64,
    pub lease_expires_at: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    /// Completed steps (latest row per step_no with non-NULL outcome).
    pub steps_done: u64,
    /// Highest step_no seen + 1 (0 when the journal is empty).
    pub steps_total: u64,
}

/// One effective step: the latest journal row for a `step_no`.
#[derive(Debug, Clone)]
pub struct Step {
    pub run_id: String,
    pub step_no: u64,
    pub kind: StepKind,
    pub input_hash: String,
    pub output: Option<String>,
    pub outcome: Option<String>,
    pub attempt: u64,
    pub review_id: Option<String>,
    pub ts_ms: u64,
    /// JSON [`ReconcileHint`] from the latest journal row, if any.
    pub reconcile_hint: Option<String>,
}

impl Step {
    /// True when the journal proves this step finished with a decided
    /// outcome. A write-ahead intent (`outcome = 'started'`) is NOT
    /// complete: the side effect may or may not have run, and the step
    /// must go through [`classify_orphan`], not the replay path.
    pub fn is_complete(&self) -> bool {
        matches!(self.outcome.as_deref(), Some(s) if s != OUTCOME_STARTED)
    }
}

/// A write-ahead intent returned by [`RunsDb::begin_step`]: proof that the
/// worker durably recorded "I am about to execute this step" (fsync'd)
/// before the side effect ran. The matching outcome arrives later via
/// [`RunsDb::complete_step`] as a new row with `attempt + 1`; until then
/// the intent is *orphaned* and [`classify_orphan`] decides its fate.
#[derive(Debug, Clone)]
pub struct StepIntent {
    pub run_id: String,
    pub step_no: u64,
    pub attempt: u64,
    pub kind: StepKind,
    pub input_hash: String,
    /// JSON [`ReconcileHint`] (file probe params, idempotency key), if the
    /// worker supplied one at [`RunsDb::begin_step`] time.
    pub reconcile_hint: Option<String>,
}

/// One UI replay event.
#[derive(Debug, Clone)]
pub struct RunEvent {
    pub run_id: String,
    pub seq: u64,
    pub event_type: String,
    pub payload: String,
    pub created_at_ms: u64,
}

/// What a worker learns from [`RunsDb::resume_run`]: the completed steps to
/// replay (no re-execution) and where to continue.
#[derive(Debug, Clone)]
pub struct ResumePlan {
    pub run_id: String,
    /// Completed steps in order; feed `output` back into the agent loop.
    pub completed: Vec<Step>,
    /// First step_no to execute, if any.
    pub first_incomplete_step_no: Option<u64>,
    /// Set when the blocking step is `ambiguous`: the run was moved to
    /// NEEDS_REVIEW and the step must NOT be replayed.
    pub needs_review_step_no: Option<u64>,
}

/// What [`RunsDb::claim_run`] won: the fencing token and a live fence.
pub struct RunClaim {
    pub generation: u64,
    pub took_over: bool,
    pub fence: LeaseFence,
}

/// Budget limits parsed from the run's `budgets` JSON.
#[derive(Debug, Clone, Default)]
pub struct Budgets {
    pub max_steps: Option<u64>,
    pub max_tool_calls: Option<u64>,
    pub max_ms: Option<u64>,
    pub max_cost_cents: Option<u64>,
}

impl Budgets {
    fn parse(json: &str) -> Self {
        let v: serde_json::Value = serde_json::from_str(json).unwrap_or(serde_json::Value::Null);
        let get = |k: &str| v.get(k).and_then(|x| x.as_u64());
        Budgets {
            max_steps: get("max_steps"),
            max_tool_calls: get("max_tool_calls"),
            max_ms: get("max_ms"),
            max_cost_cents: get("max_cost_cents"),
        }
    }
}

/// Path to `runs.db` under a home directory.
pub fn runs_db_path(home: &Path) -> PathBuf {
    home.join("runs.db")
}

/// The durable-runs store: one SQLite (WAL) database per home plus a
/// shared lease handle. `Connection` is not `Sync`; share `RunsDb`
/// behind `Arc<Mutex<_>>` across threads like `ScheduleLeases`.
pub struct RunsDb {
    conn: Connection,
    leases: Arc<Mutex<ScheduleLeases>>,
    #[allow(dead_code)]
    home: PathBuf,
}

/// One journal row for a step: the full attempt history of
/// `(run_id, step_no)`, oldest first. Used by crash-recovery verification
/// (and `runs show`) to see how an orphan was reconciled: a second
/// `started` row means the step was re-begun after a crash, an
/// `executed_ok` row whose output carries `"reconciled":"probe_match"`
/// means the file probe proved the write landed.
#[derive(Debug, Clone)]
pub struct StepAttempt {
    pub attempt: u64,
    pub kind: StepKind,
    pub input_hash: String,
    pub output: Option<String>,
    pub outcome: Option<String>,
}

impl RunsDb {
    /// All journal rows for `(run_id, step_no)`, ordered by attempt.
    pub fn step_attempts(&self, run_id: &str, step_no: u64) -> Result<Vec<StepAttempt>, RunsError> {
        let mut stmt = self.conn.prepare(
            "SELECT attempt, kind, input_hash, output, outcome
             FROM run_steps
             WHERE run_id = ?1 AND step_no = ?2
             ORDER BY attempt",
        )?;
        let attempts = stmt
            .query_map(rusqlite::params![run_id, step_no as i64], |row| {
                Ok(StepAttempt {
                    attempt: row.get::<_, i64>(0).unwrap_or(0).max(0) as u64,
                    kind: row
                        .get::<_, String>(1)
                        .ok()
                        .and_then(|s| StepKind::parse(&s))
                        .unwrap_or(StepKind::Tool),
                    input_hash: row.get(2)?,
                    output: row.get(3)?,
                    outcome: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(attempts)
    }
}

impl RunsDb {
    /// Open (creating) `runs.db` under `home`, with the default lease TTL.
    pub fn open(home: &Path) -> Result<Self, RunsError> {
        Self::open_inner(home, None)
    }

    /// Open with a custom lease TTL (tests use short leases to simulate
    /// expiry and takeover without sleeping for minutes).
    pub fn open_with_ttl(home: &Path, ttl_ms: u64) -> Result<Self, RunsError> {
        Self::open_inner(home, Some(ttl_ms))
    }

    fn open_inner(home: &Path, ttl_ms: Option<u64>) -> Result<Self, RunsError> {
        let path = runs_db_path(home);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| RunsError::Open(format!("{}: {e}", path.display())))?;
        }
        let conn = Connection::open(&path)
            .map_err(|e| RunsError::Open(format!("{}: {e}", path.display())))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS runs (
                 id TEXT PRIMARY KEY,
                 parent_run TEXT,
                 state TEXT NOT NULL DEFAULT 'QUEUED',
                 plan TEXT NOT NULL DEFAULT '{}',
                 budgets TEXT NOT NULL DEFAULT '{}',
                 lease_owner TEXT,
                 lease_generation INTEGER NOT NULL DEFAULT 0,
                 lease_expires_at INTEGER NOT NULL DEFAULT 0,
                 created_at_ms INTEGER NOT NULL,
                 updated_at_ms INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS run_steps (
                 run_id TEXT NOT NULL,
                 step_no INTEGER NOT NULL,
                 attempt INTEGER NOT NULL DEFAULT 0,
                 kind TEXT NOT NULL,
                 input_hash TEXT NOT NULL,
                 output TEXT,
                 outcome TEXT,
                 review_id TEXT,
                 ts_ms INTEGER NOT NULL,
                 PRIMARY KEY (run_id, step_no, attempt)
             );
             CREATE INDEX IF NOT EXISTS idx_run_steps_run
                 ON run_steps(run_id, step_no, attempt);
             CREATE TABLE IF NOT EXISTS run_events (
                 run_id TEXT NOT NULL,
                 seq INTEGER NOT NULL,
                 event_type TEXT NOT NULL,
                 payload TEXT NOT NULL DEFAULT '{}',
                 created_at_ms INTEGER NOT NULL,
                 PRIMARY KEY (run_id, seq)
             );",
        )?;
        // Schema evolution: `run_steps.reconcile_hint` carries the
        // reconciliation parameters for an orphaned intent (file probe
        // path+hash, idempotency key) as JSON. Databases created before
        // this column existed get it via ALTER TABLE.
        let has_hint: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('run_steps')
             WHERE name = 'reconcile_hint'",
            [],
            |row| row.get(0),
        )?;
        if has_hint == 0 {
            conn.execute("ALTER TABLE run_steps ADD COLUMN reconcile_hint TEXT", [])?;
        };
        let mut leases = ScheduleLeases::open(home, RUN_LEASE_TENANT)?;
        if let Some(ttl) = ttl_ms {
            leases = leases.with_ttl(ttl);
        }
        Ok(RunsDb {
            conn,
            leases: Arc::new(Mutex::new(leases)),
            home: home.to_path_buf(),
        })
    }

    /// Create a run in QUEUED state. Returns the run id (`run_<uuid>`).
    pub fn create_run(
        &self,
        parent: Option<&str>,
        plan: &str,
        budgets: &str,
    ) -> Result<String, RunsError> {
        let id = format!("run_{}", uuid::Uuid::new_v4().simple());
        let changed = self.conn.execute(
            &format!(
                "INSERT INTO runs
                 (id, parent_run, state, plan, budgets, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, 'QUEUED', ?3, ?4, {DB_NOW_MS}, {DB_NOW_MS})"
            ),
            rusqlite::params![id, parent, plan, budgets],
        )?;
        debug_assert_eq!(changed, 1);
        Ok(id)
    }

    /// Append a step journal row. Retries are new rows: `attempt` is
    /// `max(attempt)+1` for the `(run_id, step_no)` pair. `output` may be
    /// None for a started-but-unfinished step; `outcome` None until decided.
    ///
    /// Budget enforcement happens *before* the append for completed tool
    /// steps: exceeding `max_tool_calls`/`max_steps` transitions the run to
    /// FAILED and returns `BudgetExceeded` without journaling the step.
    pub fn append_step(
        &self,
        run_id: &str,
        step_no: u64,
        kind: StepKind,
        input_hash: &str,
        output: Option<&str>,
        outcome: Option<&AttemptOutcome>,
    ) -> Result<(), RunsError> {
        let outcome_str = outcome.map(outcome_str);
        if let Some(outcome) = outcome_str {
            self.check_budgets(run_id, kind, outcome)?;
        }
        let attempt: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(attempt), -1) + 1 FROM run_steps
                 WHERE run_id = ?1 AND step_no = ?2",
                rusqlite::params![run_id, step_no as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);
        self.conn.execute(
            &format!(
                "INSERT INTO run_steps
                 (run_id, step_no, attempt, kind, input_hash, output, outcome, ts_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, {DB_NOW_MS})"
            ),
            rusqlite::params![
                run_id,
                step_no as i64,
                attempt,
                kind.as_str(),
                input_hash,
                output,
                outcome_str
            ],
        )?;
        self.conn.execute(
            &format!("UPDATE runs SET updated_at_ms = {DB_NOW_MS} WHERE id = ?1"),
            rusqlite::params![run_id],
        )?;
        Ok(())
    }

    /// Force the database (including WAL frames) to stable storage.
    ///
    /// `PRAGMA wal_checkpoint(TRUNCATE)` checkpoints every WAL frame back
    /// into the main database file, fsyncs it, and truncates the WAL.
    /// After this returns, neither `kill -9` nor a power loss can lose the
    /// rows written before it. Fails closed: a blocked checkpoint is an
    /// error, and [`RunsDb::begin_step`] treats that as "the intent is not
    /// trusted — the step must not execute".
    fn fsync_db(&self) -> Result<(), RunsError> {
        let (busy, _log_frames, _checkpointed): (i64, i64, i64) =
            self.conn
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
        if busy != 0 {
            return Err(RunsError::Sql(
                "wal_checkpoint(TRUNCATE) blocked by another connection; intent not durable"
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// Write-ahead intent: journal "I am about to execute step `step_no`"
    /// and fsync the database *before* returning, so the intent is durable
    /// before the side effect executes.
    ///
    /// Caller contract (the lease discipline that makes orphan
    /// classification sound — see [`classify_orphan`]):
    /// 1. Hold the run lease ([`RunsDb::claim_run`]) across the whole
    ///    begin → execute → complete sequence.
    /// 2. Verify the lease fence is current *before* calling `begin_step`.
    ///    A worker refused by the fence must never create an intent.
    /// 3. Execute the side effect, then [`RunsDb::complete_step`].
    ///
    /// The intent row carries `outcome = 'started'` and `output = NULL`;
    /// the later outcome row uses `attempt + 1`, so the intent stays in
    /// history. Intents do not consume budgets (budgets count decided
    /// outcomes only).
    ///
    /// Fails closed: if the fsync fails, returns `Err` and the step must
    /// not execute.
    pub fn begin_step(
        &self,
        run_id: &str,
        step_no: u64,
        kind: StepKind,
        input_hash: &str,
    ) -> Result<StepIntent, RunsError> {
        self.begin_step_with_hint(run_id, step_no, kind, input_hash, None)
    }

    /// Write-ahead intent with a reconciliation hint: `hint` is JSON
    /// ([`ReconcileHint`]) carrying the parameters [`reconcile_orphan`]
    /// needs if this intent is orphaned by a crash (file probe path+hash,
    /// idempotency key). The hint is fsync'd in the same intent row as the
    /// step itself, so a surviving orphan always carries its own probe
    /// parameters.
    pub fn begin_step_with_hint(
        &self,
        run_id: &str,
        step_no: u64,
        kind: StepKind,
        input_hash: &str,
        hint: Option<&str>,
    ) -> Result<StepIntent, RunsError> {
        let attempt: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(attempt), -1) + 1 FROM run_steps
                 WHERE run_id = ?1 AND step_no = ?2",
                rusqlite::params![run_id, step_no as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);
        self.conn.execute(
            &format!(
                "INSERT INTO run_steps
                 (run_id, step_no, attempt, kind, input_hash, output, outcome,
                  reconcile_hint, ts_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, '{OUTCOME_STARTED}', ?6, {DB_NOW_MS})"
            ),
            rusqlite::params![
                run_id,
                step_no as i64,
                attempt,
                kind.as_str(),
                input_hash,
                hint
            ],
        )?;
        // Durability BEFORE the side effect: a crash after this point
        // leaves a classifiable orphan instead of silence.
        self.fsync_db()?;
        self.conn.execute(
            &format!("UPDATE runs SET updated_at_ms = {DB_NOW_MS} WHERE id = ?1"),
            rusqlite::params![run_id],
        )?;
        Ok(StepIntent {
            run_id: run_id.to_string(),
            step_no,
            attempt: attempt.max(0) as u64,
            kind,
            input_hash: input_hash.to_string(),
            reconcile_hint: hint.map(|s| s.to_string()),
        })
    }

    /// Journal the outcome of a step previously begun with
    /// [`RunsDb::begin_step`]. Appends a new row with
    /// `attempt = intent.attempt + 1` (never an UPDATE), so the intent row
    /// stays in history and the latest-row-wins rule picks the outcome.
    /// Budget enforcement runs before the append, same as
    /// [`RunsDb::append_step`]. The outcome row is fsync'd as well, so a
    /// crash right after `complete_step` returns cannot resurrect the
    /// intent as an orphan.
    pub fn complete_step(
        &self,
        intent: &StepIntent,
        outcome: &AttemptOutcome,
        output: Option<&str>,
    ) -> Result<(), RunsError> {
        let outcome_s = outcome_str(outcome);
        self.check_budgets(&intent.run_id, intent.kind, outcome_s)?;
        let attempt = intent.attempt as i64 + 1;
        self.conn.execute(
            &format!(
                "INSERT INTO run_steps
                 (run_id, step_no, attempt, kind, input_hash, output, outcome,
                  reconcile_hint, ts_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, {DB_NOW_MS})"
            ),
            rusqlite::params![
                intent.run_id,
                intent.step_no as i64,
                attempt,
                intent.kind.as_str(),
                intent.input_hash,
                output,
                outcome_s,
                intent.reconcile_hint,
            ],
        )?;
        self.fsync_db()?;
        self.conn.execute(
            &format!("UPDATE runs SET updated_at_ms = {DB_NOW_MS} WHERE id = ?1"),
            rusqlite::params![intent.run_id],
        )?;
        Ok(())
    }

    /// Intents (`outcome = 'started'`) that are still the latest row for
    /// their `step_no`: the worker died after the intent fsync but before
    /// journaling any outcome. Ordered by `step_no`. Empty means every
    /// begun step reached a decided outcome.
    pub fn find_orphaned_intents(&self, run_id: &str) -> Result<Vec<StepIntent>, RunsError> {
        let mut stmt = self.conn.prepare(
            "SELECT s.run_id, s.step_no, s.attempt, s.kind, s.input_hash,
                    s.reconcile_hint
             FROM run_steps s
             JOIN (SELECT step_no, MAX(attempt) AS a FROM run_steps
                   WHERE run_id = ?1 GROUP BY step_no) latest
               ON s.step_no = latest.step_no AND s.attempt = latest.a
             WHERE s.run_id = ?1 AND s.outcome = 'started'
             ORDER BY s.step_no",
        )?;
        let intents = stmt
            .query_map(rusqlite::params![run_id], |row| {
                Ok(StepIntent {
                    run_id: row.get(0)?,
                    step_no: row.get::<_, i64>(1).unwrap_or(0).max(0) as u64,
                    attempt: row.get::<_, i64>(2).unwrap_or(0).max(0) as u64,
                    kind: row
                        .get::<_, String>(3)
                        .ok()
                        .and_then(|s| StepKind::parse(&s))
                        .unwrap_or(StepKind::Tool),
                    input_hash: row.get(4)?,
                    reconcile_hint: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(intents)
    }

    /// Record the action-review id for a step's latest row (links the
    /// journal to the hash-chained audit log).
    pub fn link_review(
        &self,
        run_id: &str,
        step_no: u64,
        review_id: &str,
    ) -> Result<(), RunsError> {
        self.conn.execute(
            "UPDATE run_steps SET review_id = ?3
             WHERE run_id = ?1 AND step_no = ?2
               AND attempt = (SELECT MAX(attempt) FROM run_steps
                              WHERE run_id = ?1 AND step_no = ?2)",
            rusqlite::params![run_id, step_no as i64, review_id],
        )?;
        Ok(())
    }

    /// Append a UI replay event (per-run sequence).
    pub fn append_event(
        &self,
        run_id: &str,
        event_type: &str,
        payload: &str,
    ) -> Result<(), RunsError> {
        let seq: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(seq), -1) + 1 FROM run_events WHERE run_id = ?1",
                rusqlite::params![run_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        self.conn.execute(
            &format!(
                "INSERT INTO run_events (run_id, seq, event_type, payload, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, {DB_NOW_MS})"
            ),
            rusqlite::params![run_id, seq, event_type, payload],
        )?;
        Ok(())
    }

    /// Read one run, with step progress.
    pub fn get_run(&self, run_id: &str) -> Result<Run, RunsError> {
        self.conn
            .query_row(
                "SELECT id, parent_run, state, plan, budgets,
                        lease_owner, lease_generation, lease_expires_at,
                        created_at_ms, updated_at_ms
                 FROM runs WHERE id = ?1",
                rusqlite::params![run_id],
                |row| {
                    Ok(Run {
                        id: row.get(0)?,
                        parent_run: row.get(1)?,
                        state: row
                            .get::<_, String>(2)
                            .ok()
                            .and_then(|s| RunState::parse(&s))
                            .unwrap_or(RunState::Queued),
                        plan: row.get(3)?,
                        budgets: row.get(4)?,
                        lease_owner: row.get(5)?,
                        lease_generation: row.get::<_, i64>(6).unwrap_or(0).max(0) as u64,
                        lease_expires_at: row.get::<_, i64>(7).unwrap_or(0).max(0) as u64,
                        created_at_ms: row.get::<_, i64>(8).unwrap_or(0).max(0) as u64,
                        updated_at_ms: row.get::<_, i64>(9).unwrap_or(0).max(0) as u64,
                        steps_done: 0,
                        steps_total: 0,
                    })
                },
            )
            .optional()?
            .map(|mut run| {
                let (done, total) = self.step_progress(&run.id).unwrap_or((0, 0));
                run.steps_done = done;
                run.steps_total = total;
                run
            })
            .ok_or_else(|| RunsError::NotFound(run_id.to_string()))
    }

    fn step_progress(&self, run_id: &str) -> Result<(u64, u64), RunsError> {
        // done = distinct step_no with a decided outcome on the latest
        // attempt; a write-ahead intent ('started') is not done. total =
        // max(step_no)+1 over all rows.
        let done: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM (
                 SELECT step_no, MAX(attempt) AS a FROM run_steps
                 WHERE run_id = ?1 GROUP BY step_no
             ) latest
             JOIN run_steps s ON s.run_id = ?1
               AND s.step_no = latest.step_no AND s.attempt = latest.a
             WHERE s.outcome IS NOT NULL AND s.outcome != 'started'",
            rusqlite::params![run_id],
            |row| row.get(0),
        )?;
        let total: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(step_no), -1) + 1 FROM run_steps WHERE run_id = ?1",
                rusqlite::params![run_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok((done.max(0) as u64, total.max(0) as u64))
    }

    /// List runs, optionally filtered by state string. Newest first.
    pub fn list_runs(&self, state_filter: Option<&str>) -> Result<Vec<Run>, RunsError> {
        let mut stmt = if state_filter.is_some() {
            self.conn.prepare(
                "SELECT id, parent_run, state, plan, budgets,
                        lease_owner, lease_generation, lease_expires_at,
                        created_at_ms, updated_at_ms
                 FROM runs WHERE state = ?1 ORDER BY created_at_ms DESC",
            )?
        } else {
            self.conn.prepare(
                "SELECT id, parent_run, state, plan, budgets,
                        lease_owner, lease_generation, lease_expires_at,
                        created_at_ms, updated_at_ms
                 FROM runs ORDER BY created_at_ms DESC",
            )?
        };
        let rows: Vec<Run> = if let Some(filter) = state_filter {
            stmt.query_map(rusqlite::params![filter], row_to_run)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map([], row_to_run)?
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(rows
            .into_iter()
            .map(|mut run| {
                let (done, total) = self.step_progress(&run.id).unwrap_or((0, 0));
                run.steps_done = done;
                run.steps_total = total;
                run
            })
            .collect())
    }

    /// Child runs of a parent (subagents re-attach to these on resume
    /// instead of spawning new ones).
    pub fn list_children(&self, parent_id: &str) -> Result<Vec<Run>, RunsError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent_run, state, plan, budgets,
                    lease_owner, lease_generation, lease_expires_at,
                    created_at_ms, updated_at_ms
             FROM runs WHERE parent_run = ?1 ORDER BY created_at_ms",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![parent_id], row_to_run)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The latest journal row per step_no, ordered by step_no.
    fn latest_steps(&self, run_id: &str) -> Result<Vec<Step>, RunsError> {
        let mut stmt = self.conn.prepare(
            "SELECT s.run_id, s.step_no, s.kind, s.input_hash, s.output,
                    s.outcome, s.attempt, s.review_id, s.ts_ms,
                    s.reconcile_hint
             FROM run_steps s
             JOIN (SELECT step_no, MAX(attempt) AS a FROM run_steps
                   WHERE run_id = ?1 GROUP BY step_no) latest
               ON s.step_no = latest.step_no AND s.attempt = latest.a
             WHERE s.run_id = ?1
             ORDER BY s.step_no",
        )?;
        let steps = stmt
            .query_map(rusqlite::params![run_id], |row| {
                Ok(Step {
                    run_id: row.get(0)?,
                    step_no: row.get::<_, i64>(1).unwrap_or(0).max(0) as u64,
                    kind: row
                        .get::<_, String>(2)
                        .ok()
                        .and_then(|s| StepKind::parse(&s))
                        .unwrap_or(StepKind::Tool),
                    input_hash: row.get(3)?,
                    output: row.get(4)?,
                    outcome: row.get(5)?,
                    attempt: row.get::<_, i64>(6).unwrap_or(0).max(0) as u64,
                    review_id: row.get(7)?,
                    ts_ms: row.get::<_, i64>(8).unwrap_or(0).max(0) as u64,
                    reconcile_hint: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(steps)
    }

    /// Conditional state transition: `UPDATE ... WHERE state = <from>`.
    /// Returns true when this worker won the race (1 row changed).
    pub fn transition(
        &self,
        run_id: &str,
        from: RunState,
        to: RunState,
    ) -> Result<bool, RunsError> {
        let changed = self.conn.execute(
            &format!(
                "UPDATE runs SET state = ?3, updated_at_ms = {DB_NOW_MS}
                 WHERE id = ?1 AND state = ?2"
            ),
            rusqlite::params![run_id, from.as_str(), to.as_str()],
        )?;
        Ok(changed == 1)
    }

    /// Move a run to NEEDS_REVIEW from any non-terminal state (used when
    /// resume hits an `ambiguous` step). Returns true when transitioned.
    pub fn mark_needs_review(&self, run_id: &str) -> Result<bool, RunsError> {
        let changed = self.conn.execute(
            &format!(
                "UPDATE runs SET state = 'NEEDS_REVIEW', updated_at_ms = {DB_NOW_MS}
                 WHERE id = ?1 AND state NOT IN ('DONE', 'FAILED', 'NEEDS_REVIEW')"
            ),
            rusqlite::params![run_id],
        )?;
        Ok(changed == 1)
    }

    /// Pause from any active state. Records `paused_from` in the plan JSON.
    pub fn pause_run(&self, run_id: &str) -> Result<bool, RunsError> {
        let run = self.get_run(run_id)?;
        let from = run.state;
        if matches!(from, RunState::Paused | RunState::Done | RunState::Failed) {
            return Ok(false);
        }
        if self.transition(run_id, from, RunState::Paused)? {
            self.patch_plan(run_id, "paused_from", from.as_str())?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Cancel from any non-terminal state (FAILED + `cancelled: true`).
    pub fn cancel_run(&self, run_id: &str) -> Result<bool, RunsError> {
        let run = self.get_run(run_id)?;
        let from = run.state;
        if from.is_terminal() {
            return Ok(false);
        }
        if self.transition(run_id, from, RunState::Failed)? {
            self.patch_plan(run_id, "cancelled", "true")?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Retry from FAILED or NEEDS_REVIEW: back to QUEUED, journal kept
    /// (the next append_step for a failed step_no uses attempt+1).
    /// From NEEDS_REVIEW the human must have resolved the ambiguous step
    /// first (its outcome row is updated by the operator tooling, not here).
    pub fn retry_run(&self, run_id: &str) -> Result<bool, RunsError> {
        let run = self.get_run(run_id)?;
        match run.state {
            RunState::Failed | RunState::NeedsReview => {
                self.transition(run_id, run.state, RunState::Queued)
            }
            _ => Ok(false),
        }
    }

    /// Set a string key in the plan JSON (best-effort; leaves non-JSON
    /// plans untouched).
    fn patch_plan(&self, run_id: &str, key: &str, value: &str) -> Result<(), RunsError> {
        let run = self.get_run(run_id)?;
        let mut v: serde_json::Value =
            serde_json::from_str(&run.plan).unwrap_or(serde_json::Value::Null);
        if let serde_json::Value::Object(ref mut map) = v {
            map.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
            let plan = serde_json::to_string(&v).unwrap_or(run.plan);
            self.conn.execute(
                "UPDATE runs SET plan = ?2 WHERE id = ?1",
                rusqlite::params![run_id, plan],
            )?;
        }
        Ok(())
    }

    /// Claim the run lease (takeover when lapsed). Mirrors the lease
    /// columns into the runs row for debuggability. Returns None when
    /// another worker holds a live lease.
    pub fn claim_run(&self, run_id: &str) -> Result<Option<RunClaim>, RunsError> {
        // Fail fast on unknown runs: claiming a lease for a nonexistent
        // run would create an orphan lease row.
        self.get_run(run_id)?;
        let (info, expires_at) = {
            let leases = self
                .leases
                .lock()
                .map_err(|_| RunsError::Lease("lease lock poisoned".to_string()))?;
            let info = match leases.claim(run_id)? {
                Some(info) => info,
                None => return Ok(None),
            };
            let expires_at = leases
                .lease_row(run_id)?
                .map(|r| r.expires_at_ms)
                .unwrap_or(0);
            (info, expires_at)
        };
        self.conn.execute(
            &format!(
                "UPDATE runs SET lease_owner = ?2, lease_generation = ?3,
                                lease_expires_at = ?4, updated_at_ms = {DB_NOW_MS}
                 WHERE id = ?1"
            ),
            rusqlite::params![
                run_id,
                self.worker_id(),
                info.generation as i64,
                expires_at as i64
            ],
        )?;
        let fence = LeaseFence::new(
            Arc::clone(&self.leases),
            run_id.to_string(),
            info.generation,
        );
        Ok(Some(RunClaim {
            generation: info.generation,
            took_over: info.took_over.is_some(),
            fence,
        }))
    }

    /// This worker's lease owner id (for diagnostics).
    pub fn worker_id(&self) -> String {
        self.leases
            .lock()
            .map(|g| g.worker_id().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    }

    /// Extend the run lease. False when the lease is gone or belongs to
    /// someone else.
    pub fn renew_run(&self, run_id: &str) -> Result<bool, RunsError> {
        let leases = self
            .leases
            .lock()
            .map_err(|_| RunsError::Lease("lease lock poisoned".to_string()))?;
        Ok(leases.renew(run_id)?)
    }

    /// Release the run lease on the clean path (DONE/FAILED/PAUSED decided).
    pub fn release_run(&self, run_id: &str) -> Result<(), RunsError> {
        let leases = self
            .leases
            .lock()
            .map_err(|_| RunsError::Lease("lease lock poisoned".to_string()))?;
        leases.release(run_id)?;
        Ok(())
    }
}

fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<Run> {
    Ok(Run {
        id: row.get(0)?,
        parent_run: row.get(1)?,
        state: row
            .get::<_, String>(2)
            .ok()
            .and_then(|s| RunState::parse(&s))
            .unwrap_or(RunState::Queued),
        plan: row.get(3)?,
        budgets: row.get(4)?,
        lease_owner: row.get(5)?,
        lease_generation: row.get::<_, i64>(6).unwrap_or(0).max(0) as u64,
        lease_expires_at: row.get::<_, i64>(7).unwrap_or(0).max(0) as u64,
        created_at_ms: row.get::<_, i64>(8).unwrap_or(0).max(0) as u64,
        updated_at_ms: row.get::<_, i64>(9).unwrap_or(0).max(0) as u64,
        steps_done: 0,
        steps_total: 0,
    })
}

impl RunsDb {
    /// Budget enforcement, checked *before* journaling a completed step.
    /// Exceeding a budget transitions the run to FAILED and returns
    /// `BudgetExceeded` — the step is NOT journaled (the 3rd tool call
    /// never lands in the journal when `max_tool_calls` is 2).
    fn check_budgets(&self, run_id: &str, kind: StepKind, outcome: &str) -> Result<(), RunsError> {
        let run = self.get_run(run_id)?;
        let budgets = Budgets::parse(&run.budgets);
        // Only steps that actually did work count. `never_ran` provably
        // did nothing; in-progress (NULL) rows aren't counted here because
        // this path only runs for decided outcomes. Side-effecting step
        // kinds (tool, file_write, idempotent_http, opaque_write) count;
        // model/read-only steps don't.
        let counts_as_call = kind.counts_as_tool_call()
            && matches!(outcome, "executed_ok" | "executed_failed" | "ambiguous");
        if counts_as_call {
            if let Some(max) = budgets.max_tool_calls {
                let used: i64 = self.conn.query_row(
                    "SELECT COUNT(*) FROM (
                       SELECT step_no, MAX(attempt) AS a FROM run_steps
                       WHERE run_id = ?1
                         AND kind IN ('tool','file_write','idempotent_http','opaque_write')
                       GROUP BY step_no
                     ) latest
                     JOIN run_steps s ON s.run_id = ?1
                       AND s.step_no = latest.step_no AND s.attempt = latest.a
                     WHERE s.outcome IN ('executed_ok','executed_failed','ambiguous')",
                    rusqlite::params![run_id],
                    |row| row.get(0),
                )?;
                if used.max(0) as u64 + 1 > max {
                    let _ = self.transition(run_id, run.state, RunState::Failed);
                    return Err(RunsError::BudgetExceeded {
                        run_id: run_id.to_string(),
                        budget: "max_tool_calls".to_string(),
                    });
                }
            }
        }
        if let Some(max) = budgets.max_steps {
            // Decided outcomes only: a write-ahead intent ('started') is
            // not a completed step and does not consume the budget.
            let used: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM (
                   SELECT step_no, MAX(attempt) AS a FROM run_steps
                   WHERE run_id = ?1 GROUP BY step_no
                 ) latest
                 JOIN run_steps s ON s.run_id = ?1
                   AND s.step_no = latest.step_no AND s.attempt = latest.a
                 WHERE s.outcome IS NOT NULL AND s.outcome != 'started'",
                rusqlite::params![run_id],
                |row| row.get(0),
            )?;
            if used.max(0) as u64 + 1 > max {
                let _ = self.transition(run_id, run.state, RunState::Failed);
                return Err(RunsError::BudgetExceeded {
                    run_id: run_id.to_string(),
                    budget: "max_steps".to_string(),
                });
            }
        }
        if let Some(max_ms) = budgets.max_ms {
            let now_ms: i64 = self
                .conn
                .query_row(&format!("SELECT {DB_NOW_MS}"), [], |row| row.get(0))?;
            let elapsed = (now_ms - run.created_at_ms as i64).max(0) as u64;
            if elapsed > max_ms {
                let _ = self.transition(run_id, run.state, RunState::Failed);
                return Err(RunsError::BudgetExceeded {
                    run_id: run_id.to_string(),
                    budget: "max_ms".to_string(),
                });
            }
        }
        Ok(())
    }

    /// Resume analysis for a run: replay completed steps (no re-execution),
    /// continue at the first incomplete step.
    ///
    /// - No row / NULL outcome / `never_ran` → safe to execute.
    /// - `ambiguous` → NEVER replayed: the run is moved to NEEDS_REVIEW
    ///   and the step is reported via `needs_review_step_no`.
    /// - `started` (orphaned write-ahead intent) → reconciled by
    ///   [`reconcile_orphan`] *before* falling back to review:
    ///   `Rerun` → the step is safe to execute (first incomplete step);
    ///   `AlreadyComplete` → the probe proved the side effect landed, so
    ///   the outcome is journaled as executed and the scan continues
    ///   (never re-executed); `NeedsReview` → the run is moved to
    ///   NEEDS_REVIEW and the step is never replayed.
    /// - `executed_ok` / `executed_failed` → complete; output is replayed.
    ///
    /// The caller is expected to hold the run lease (see [`RunsDb::claim_run`]).
    pub fn resume_run(&self, run_id: &str) -> Result<ResumePlan, RunsError> {
        let steps = self.latest_steps(run_id)?;
        let mut completed = Vec::new();
        let mut first_incomplete_step_no: Option<u64> = None;
        let mut needs_review_step_no: Option<u64> = None;
        let mut expected: u64 = 0;
        for step in steps {
            if step.step_no != expected {
                // Gap in the journal (shouldn't happen, but a crash can't
                // create future rows): treat the gap as incomplete.
                first_incomplete_step_no = Some(expected);
                break;
            }
            match step.outcome.as_deref() {
                Some("executed_ok") | Some("executed_failed") => {
                    completed.push(step);
                    expected += 1;
                }
                Some("ambiguous") => {
                    needs_review_step_no = Some(expected);
                    break;
                }
                // Write-ahead orphan: the intent was fsync'd but no outcome
                // was ever journaled. Reconcile it — review is the last
                // resort, not the default.
                Some("started") => {
                    let intent = StepIntent {
                        run_id: step.run_id.clone(),
                        step_no: step.step_no,
                        attempt: step.attempt,
                        kind: step.kind,
                        input_hash: step.input_hash.clone(),
                        reconcile_hint: step.reconcile_hint.clone(),
                    };
                    match reconcile_orphan(&intent) {
                        ReconcileVerdict::Rerun { .. } => {
                            first_incomplete_step_no = Some(expected);
                            break;
                        }
                        ReconcileVerdict::AlreadyComplete { output, .. } => {
                            // The probe proved the side effect landed
                            // before the crash: journal the outcome so the
                            // orphan is closed, and keep scanning — later
                            // steps may still need work. The step is NOT
                            // re-executed.
                            self.complete_step(
                                &intent,
                                &AttemptOutcome::Executed { success: true },
                                Some(&output),
                            )?;
                            let mut done_step = step;
                            done_step.outcome = Some(OUTCOME_EXECUTED_OK.to_string());
                            done_step.output = Some(output);
                            done_step.attempt += 1;
                            completed.push(done_step);
                            expected += 1;
                        }
                        ReconcileVerdict::NeedsReview { .. } => {
                            needs_review_step_no = Some(expected);
                            break;
                        }
                    }
                }
                // None (legacy started-but-unfinished), never_ran (provably
                // did nothing), or an unknown string: safe to (re-)execute.
                _ => {
                    first_incomplete_step_no = Some(expected);
                    break;
                }
            }
        }
        if first_incomplete_step_no.is_none() && needs_review_step_no.is_none() {
            first_incomplete_step_no = Some(expected);
        }
        if needs_review_step_no.is_some() {
            self.mark_needs_review(run_id)?;
        }
        Ok(ResumePlan {
            run_id: run_id.to_string(),
            completed,
            first_incomplete_step_no,
            needs_review_step_no,
        })
    }

    /// Read the UI replay log for a run, in order.
    pub fn read_events(&self, run_id: &str) -> Result<Vec<RunEvent>, RunsError> {
        let mut stmt = self.conn.prepare(
            "SELECT run_id, seq, event_type, payload, created_at_ms
             FROM run_events WHERE run_id = ?1 ORDER BY seq",
        )?;
        let events = stmt
            .query_map(rusqlite::params![run_id], |row| {
                Ok(RunEvent {
                    run_id: row.get(0)?,
                    seq: row.get::<_, i64>(1).unwrap_or(0).max(0) as u64,
                    event_type: row.get(2)?,
                    payload: row.get(3)?,
                    created_at_ms: row.get::<_, i64>(4).unwrap_or(0).max(0) as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(events)
    }
}

/// Compute the SHA-256 idempotency key for a step's canonical input.
/// Workers hash the canonical (sorted-key) JSON of the step input so a
/// retried step with identical input is recognizable without replaying.
pub fn step_input_hash(canonical_input: &str) -> String {
    sha256_hex(canonical_input.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_reviews::AttemptOutcome;

    fn test_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "supercli-durable-runs-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ok_outcome() -> AttemptOutcome {
        AttemptOutcome::Executed { success: true }
    }

    fn step_hash(run_tag: &str, step_no: u64) -> String {
        step_input_hash(&format!("{run_tag}:step:{step_no}:write_file:/tmp/x"))
    }

    #[test]
    fn create_get_list_roundtrip() {
        let home = test_home("crud");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, r#"{"steps":[]}"#, "{}").unwrap();
        assert!(id.starts_with("run_"));
        let run = db.get_run(&id).unwrap();
        assert_eq!(run.state, RunState::Queued);
        assert_eq!(run.parent_run, None);
        let child = db.create_run(Some(&id), "{}", "{}").unwrap();
        let runs = db.list_runs(None).unwrap();
        assert_eq!(runs.len(), 2);
        let queued = db.list_runs(Some("QUEUED")).unwrap();
        assert_eq!(queued.len(), 2);
        let kids = db.list_children(&id).unwrap();
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].id, child);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn transitions_are_conditional() {
        let home = test_home("trans");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        assert!(db
            .transition(&id, RunState::Queued, RunState::AwaitingModel)
            .unwrap());
        // Lost race: wrong `from` state transitions nothing.
        assert!(!db
            .transition(&id, RunState::Queued, RunState::Done)
            .unwrap());
        assert_eq!(db.get_run(&id).unwrap().state, RunState::AwaitingModel);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn pause_cancel_retry() {
        let home = test_home("pcr");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, r#"{"steps":[]}"#, "{}").unwrap();
        assert!(db
            .transition(&id, RunState::Queued, RunState::ExecutingTools)
            .unwrap());
        assert!(db.pause_run(&id).unwrap());
        assert_eq!(db.get_run(&id).unwrap().state, RunState::Paused);
        assert!(!db.pause_run(&id).unwrap()); // already paused
        assert!(!db.retry_run(&id).unwrap()); // PAUSED is not retryable
        let id2 = db.create_run(None, "{}", "{}").unwrap();
        assert!(db.cancel_run(&id2).unwrap());
        assert_eq!(db.get_run(&id2).unwrap().state, RunState::Failed);
        assert!(db.retry_run(&id2).unwrap());
        assert_eq!(db.get_run(&id2).unwrap().state, RunState::Queued);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn retry_keeps_journal_and_bumps_attempt() {
        let home = test_home("retry");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        db.append_step(
            &id,
            0,
            StepKind::Tool,
            &step_hash("r", 0),
            Some(r#"{"error":"boom"}"#),
            Some(&AttemptOutcome::Executed { success: false }),
        )
        .unwrap();
        assert!(db
            .transition(&id, RunState::Queued, RunState::Failed)
            .unwrap());
        assert!(db.retry_run(&id).unwrap());
        db.append_step(
            &id,
            0,
            StepKind::Tool,
            &step_hash("r", 0),
            Some(r#"{"ok":true}"#),
            Some(&ok_outcome()),
        )
        .unwrap();
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.completed.len(), 1);
        assert_eq!(plan.completed[0].attempt, 1);
        assert_eq!(plan.completed[0].outcome.as_deref(), Some("executed_ok"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn step_outcome_mapping() {
        assert_eq!(outcome_str(&ok_outcome()), "executed_ok");
        assert_eq!(
            outcome_str(&AttemptOutcome::Executed { success: false }),
            "executed_failed"
        );
        assert_eq!(
            outcome_str(&AttemptOutcome::Ambiguous { reason: "x".into() }),
            "ambiguous"
        );
        assert_eq!(
            outcome_str(&AttemptOutcome::NeverRan { reason: "x".into() }),
            "never_ran"
        );
    }

    #[test]
    fn events_roundtrip() {
        let home = test_home("events");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        db.append_event(&id, "run.started", r#"{"by":"test"}"#)
            .unwrap();
        db.append_event(&id, "step.done", r#"{"step":0}"#).unwrap();
        let events = db.read_events(&id).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].event_type, "step.done");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn uncertain_write_goes_to_needs_review() {
        let home = test_home("needsreview");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        db.append_step(
            &id,
            0,
            StepKind::Tool,
            &step_hash("u", 0),
            Some(r#"{"ok":true}"#),
            Some(&ok_outcome()),
        )
        .unwrap();
        // Transport dropped post-send: the step may have executed.
        db.append_step(
            &id,
            1,
            StepKind::Tool,
            &step_hash("u", 1),
            None,
            Some(&AttemptOutcome::Ambiguous {
                reason: "transport dropped after send".into(),
            }),
        )
        .unwrap();
        let plan = db.resume_run(&id).unwrap();
        // The ambiguous step is NOT in completed (never replayed) ...
        assert_eq!(plan.completed.len(), 1);
        assert_eq!(plan.completed[0].step_no, 0);
        // ... and the run is parked in NEEDS_REVIEW.
        assert_eq!(plan.needs_review_step_no, Some(1));
        assert_eq!(db.get_run(&id).unwrap().state, RunState::NeedsReview);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn never_ran_step_is_safe_to_execute() {
        let home = test_home("neverran");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        db.append_step(
            &id,
            0,
            StepKind::Tool,
            &step_hash("n", 0),
            None,
            Some(&AttemptOutcome::NeverRan {
                reason: "stale fence before send".into(),
            }),
        )
        .unwrap();
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.needs_review_step_no, None);
        assert_eq!(plan.first_incomplete_step_no, Some(0));
        assert!(plan.completed.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn lease_fence_takeover() {
        let home = test_home("fence");
        // Worker A claims with a short TTL.
        let db_a = RunsDb::open_with_ttl(&home, 80).unwrap();
        let id = db_a.create_run(None, "{}", "{}").unwrap();
        let claim_a = db_a.claim_run(&id).unwrap().expect("A claims");
        assert_eq!(claim_a.generation, 1);
        assert!(!claim_a.took_over);
        assert!(claim_a.fence.is_current());
        // A's lease lapses (simulated crash: no renew, no release).
        std::thread::sleep(std::time::Duration::from_millis(150));
        // Worker B takes over: generation bumps, A's fence goes stale.
        let db_b = RunsDb::open_with_ttl(&home, 80).unwrap();
        let claim_b = db_b.claim_run(&id).unwrap().expect("B takes over");
        assert_eq!(claim_b.generation, 2);
        assert!(claim_b.took_over);
        assert!(!claim_a.fence.is_current());
        assert!(claim_b.fence.is_current());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn budget_enforcement() {
        let home = test_home("budget");
        let db = RunsDb::open(&home).unwrap();
        let id = db
            .create_run(None, "{}", r#"{"max_tool_calls":2}"#)
            .unwrap();
        for step_no in 0..2u64 {
            db.append_step(
                &id,
                step_no,
                StepKind::Tool,
                &step_hash("b", step_no),
                Some(r#"{"ok":true}"#),
                Some(&ok_outcome()),
            )
            .unwrap();
        }
        // The 3rd tool call exceeds the budget: FAILED, nothing journaled.
        let err = db
            .append_step(
                &id,
                2,
                StepKind::Tool,
                &step_hash("b", 2),
                Some(r#"{"ok":true}"#),
                Some(&ok_outcome()),
            )
            .unwrap_err();
        assert!(matches!(err, RunsError::BudgetExceeded { .. }));
        assert_eq!(db.get_run(&id).unwrap().state, RunState::Failed);
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.completed.len(), 2);

        // max_steps variant.
        let id2 = db.create_run(None, "{}", r#"{"max_steps":1}"#).unwrap();
        db.append_step(
            &id2,
            0,
            StepKind::Model,
            &step_hash("m", 0),
            Some("{}"),
            Some(&ok_outcome()),
        )
        .unwrap();
        let err2 = db
            .append_step(
                &id2,
                1,
                StepKind::Model,
                &step_hash("m", 1),
                Some("{}"),
                Some(&ok_outcome()),
            )
            .unwrap_err();
        assert!(matches!(err2, RunsError::BudgetExceeded { .. }));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Kill -9 at every step boundary, 50 chaos iterations. Each iteration
    /// kills the "worker" (drops the DB handle) at a different boundary,
    /// reopens, takes over the lease, and resumes. Asserts the run reaches
    /// DONE with zero duplicated side effects: every step's `input_hash`
    /// appears exactly once with an `executed` outcome in the journal.
    #[test]
    fn durable_run_survives_kill_at_step_boundary() {
        const STEPS: u64 = 5;
        for iter in 0..50u64 {
            let tag = format!("chaos{iter}");
            let home = test_home(&tag);
            let boundary = iter % (STEPS + 1); // kill after `boundary` steps
            let mut side_effects: Vec<String> = Vec::new();

            // Worker 1: run `boundary` steps, then die (kill -9).
            let run_id = {
                let db = RunsDb::open_with_ttl(&home, 100).unwrap();
                let id = db.create_run(None, r#"{"steps":5}"#, "{}").unwrap();
                let claim = db.claim_run(&id).unwrap().expect("claim");
                assert!(claim.fence.is_current());
                assert!(db
                    .transition(&id, RunState::Queued, RunState::ExecutingTools)
                    .unwrap());
                for step_no in 0..boundary {
                    let h = step_hash(&tag, step_no);
                    db.append_step(
                        &id,
                        step_no,
                        StepKind::Tool,
                        &h,
                        Some(&format!(r#"{{"step":{step_no}}}"#)),
                        Some(&ok_outcome()),
                    )
                    .unwrap();
                    side_effects.push(h); // the "real world" side effect
                }
                // kill -9: drop the handle without release/renew.
                drop(db);
                id
            };
            std::thread::sleep(std::time::Duration::from_millis(160));

            // Worker 2: take over and resume.
            {
                let db = RunsDb::open_with_ttl(&home, 100).unwrap();
                let claim = db.claim_run(&run_id).unwrap().expect("takeover");
                assert!(claim.took_over || boundary == 0 || claim.generation >= 1);
                let plan = db.resume_run(&run_id).unwrap();
                assert_eq!(
                    plan.completed.len() as u64,
                    boundary,
                    "iter {iter}: resume must replay exactly the pre-kill steps"
                );
                assert_eq!(plan.first_incomplete_step_no, Some(boundary));
                assert_eq!(plan.needs_review_step_no, None);
                // Continue at the first incomplete step — never re-execute.
                for step_no in boundary..STEPS {
                    let h = step_hash(&tag, step_no);
                    // Guard: this step must not already be in the journal.
                    assert!(
                        !side_effects.contains(&h),
                        "iter {iter}: step {step_no} would be a duplicate side effect"
                    );
                    db.append_step(
                        &run_id,
                        step_no,
                        StepKind::Tool,
                        &h,
                        Some(&format!(r#"{{"step":{step_no}}}"#)),
                        Some(&ok_outcome()),
                    )
                    .unwrap();
                    side_effects.push(h);
                }
                assert!(db
                    .transition(&run_id, RunState::ExecutingTools, RunState::Done)
                    .unwrap());
                db.release_run(&run_id).unwrap();

                // Audit: every input_hash appears exactly once as executed.
                let plan = db.resume_run(&run_id).unwrap();
                assert_eq!(plan.completed.len() as u64, STEPS);
                let mut seen = std::collections::HashSet::new();
                for step in &plan.completed {
                    assert_eq!(step.outcome.as_deref(), Some("executed_ok"));
                    assert!(
                        seen.insert(step.input_hash.clone()),
                        "duplicate journal row"
                    );
                }
                assert_eq!(seen.len() as u64, STEPS);
                assert_eq!(db.get_run(&run_id).unwrap().state, RunState::Done);
            }
            assert_eq!(side_effects.len() as u64, STEPS, "iter {iter}");
            let _ = std::fs::remove_dir_all(&home);
        }
    }

    /// A parent with 3 subagent children is killed mid-run. On resume the
    /// parent re-attaches to the existing children (no respawn): every
    /// child completes exactly once.
    #[test]
    fn parent_with_3_subagents_killed_mid_run() {
        let home = test_home("subagents");
        let (parent_id, child_ids) = {
            let db = RunsDb::open_with_ttl(&home, 100).unwrap();
            let parent = db.create_run(None, r#"{"steps":["a","b"]}"#, "{}").unwrap();
            db.claim_run(&parent).unwrap().expect("claim parent");
            let mut children = Vec::new();
            for i in 0..3u64 {
                let child = db
                    .create_run(Some(&parent), &format!(r#"{{"task":"child{i}"}}"#), "{}")
                    .unwrap();
                db.claim_run(&child).unwrap().expect("claim child");
                // Each child completes its first step before the kill.
                db.append_step(
                    &child,
                    0,
                    StepKind::Tool,
                    &step_hash(&format!("child{i}"), 0),
                    Some(r#"{"partial":true}"#),
                    Some(&ok_outcome()),
                )
                .unwrap();
                children.push(child);
            }
            // Parent does one step, then kill -9.
            db.append_step(
                &parent,
                0,
                StepKind::Model,
                &step_hash("parent", 0),
                Some(r#"{"turn":1}"#),
                Some(&ok_outcome()),
            )
            .unwrap();
            drop(db);
            (parent, children)
        };
        std::thread::sleep(std::time::Duration::from_millis(160));

        {
            let db = RunsDb::open_with_ttl(&home, 100).unwrap();
            // Re-attach: the parent finds its children, no new runs.
            let kids = db.list_children(&parent_id).unwrap();
            assert_eq!(kids.len(), 3);
            for (i, kid) in kids.iter().enumerate() {
                assert!(child_ids.contains(&kid.id), "child {i} is a respawn");
                db.claim_run(&kid.id).unwrap().expect("claim child");
                let plan = db.resume_run(&kid.id).unwrap();
                assert_eq!(plan.completed.len(), 1);
                assert_eq!(plan.first_incomplete_step_no, Some(1));
                // Finish the child exactly once.
                db.append_step(
                    &kid.id,
                    1,
                    StepKind::Tool,
                    &step_hash(&format!("child{i}"), 1),
                    Some(r#"{"done":true}"#),
                    Some(&ok_outcome()),
                )
                .unwrap();
                assert!(db
                    .transition(&kid.id, RunState::Queued, RunState::Done)
                    .unwrap());
            }
            // No child was respawned: still exactly 3.
            assert_eq!(db.list_children(&parent_id).unwrap().len(), 3);
            // Each child's journal has exactly 2 executed steps.
            for kid in db.list_children(&parent_id).unwrap() {
                let plan = db.resume_run(&kid.id).unwrap();
                assert_eq!(plan.completed.len(), 2);
            }
            // Parent resumes and finishes.
            db.claim_run(&parent_id).unwrap().expect("claim parent");
            let plan = db.resume_run(&parent_id).unwrap();
            assert_eq!(plan.completed.len(), 1);
            db.append_step(
                &parent_id,
                1,
                StepKind::Model,
                &step_hash("parent", 1),
                Some(r#"{"turn":2}"#),
                Some(&ok_outcome()),
            )
            .unwrap();
            assert!(db
                .transition(&parent_id, RunState::Queued, RunState::Done)
                .unwrap());
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Write-ahead: the intent is fsync'd before the side effect, so a
    /// crash (drop the DB handle, reopen) still finds the orphaned intent.
    #[test]
    fn write_ahead_intent_survives_crash() {
        let home = test_home("intentcrash");
        let id = {
            let db = RunsDb::open(&home).unwrap();
            let id = db.create_run(None, "{}", "{}").unwrap();
            let intent = db
                .begin_step(&id, 0, StepKind::Tool, &step_hash("w", 0))
                .unwrap();
            assert_eq!(intent.step_no, 0);
            assert_eq!(intent.attempt, 0);
            assert_eq!(intent.kind, StepKind::Tool);
            // kill -9: drop the handle without completing the step.
            drop(db);
            id
        };
        // Reopen: the fsync'd intent must be there.
        let db = RunsDb::open(&home).unwrap();
        let orphans = db.find_orphaned_intents(&id).unwrap();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].step_no, 0);
        assert_eq!(orphans[0].kind, StepKind::Tool);
        assert_eq!(orphans[0].input_hash, step_hash("w", 0));
        // An intent is not a completed step.
        let run = db.get_run(&id).unwrap();
        assert_eq!(run.steps_done, 0);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Orphaned model intent: model calls have no external side effect, so
    /// the orphan classifies as NeverRan and the step is safe to re-run.
    #[test]
    fn orphan_classified_never_ran_for_model_steps() {
        let home = test_home("orphanmodel");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        let intent = db
            .begin_step(&id, 0, StepKind::Model, &step_hash("m", 0))
            .unwrap();
        assert!(matches!(
            classify_orphan(&intent),
            AttemptOutcome::NeverRan { .. }
        ));
        // Resume treats it as safe to (re-)run: first incomplete, and the
        // run is NOT parked in NEEDS_REVIEW.
        let plan = db.resume_run(&id).unwrap();
        assert!(plan.completed.is_empty());
        assert_eq!(plan.first_incomplete_step_no, Some(0));
        assert_eq!(plan.needs_review_step_no, None);
        assert_eq!(db.get_run(&id).unwrap().state, RunState::Queued);
        // Re-beginning the step bumps the attempt; the old orphan is
        // superseded, not duplicated.
        let intent2 = db
            .begin_step(&id, 0, StepKind::Model, &step_hash("m", 0))
            .unwrap();
        assert_eq!(intent2.attempt, 1);
        db.complete_step(&intent2, &ok_outcome(), Some("{}"))
            .unwrap();
        assert!(db.find_orphaned_intents(&id).unwrap().is_empty());
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.completed.len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Orphaned tool intent: the side effect may have executed after the
    /// intent fsync, so the orphan classifies as Ambiguous — the run goes
    /// to NEEDS_REVIEW and the step is never replayed.
    #[test]
    fn orphan_classified_ambiguous_for_tool_steps() {
        let home = test_home("orphantool");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        db.transition(&id, RunState::Queued, RunState::ExecutingTools)
            .unwrap();
        let intent = db
            .begin_step(&id, 0, StepKind::Tool, &step_hash("t", 0))
            .unwrap();
        assert!(matches!(
            classify_orphan(&intent),
            AttemptOutcome::Ambiguous { .. }
        ));
        // Simulate the crash: drop and reopen, then resume.
        drop(db);
        let db = RunsDb::open(&home).unwrap();
        let plan = db.resume_run(&id).unwrap();
        assert!(plan.completed.is_empty());
        assert_eq!(plan.needs_review_step_no, Some(0));
        assert_eq!(plan.first_incomplete_step_no, None);
        assert_eq!(db.get_run(&id).unwrap().state, RunState::NeedsReview);
        // The orphan is still listed — nothing was silently dropped or
        // replayed.
        assert_eq!(db.find_orphaned_intents(&id).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Orphaned subagent intent: same rule as tools — Ambiguous, never
    /// replayed, run parks in NEEDS_REVIEW.
    #[test]
    fn orphan_classified_ambiguous_for_subagent_steps() {
        let home = test_home("orphansub");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        let intent = db
            .begin_step(&id, 0, StepKind::Subagent, &step_hash("s", 0))
            .unwrap();
        assert!(matches!(
            classify_orphan(&intent),
            AttemptOutcome::Ambiguous { .. }
        ));
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.needs_review_step_no, Some(0));
        assert_eq!(db.get_run(&id).unwrap().state, RunState::NeedsReview);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The live-worker path for "fence failed before the call": the worker
    /// journals `never_ran` explicitly through `complete_step`, so no
    /// orphan is left behind and the step is safe to run on resume.
    #[test]
    fn fenced_out_worker_journals_never_ran_no_orphan() {
        let home = test_home("fencedout");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        let intent = db
            .begin_step(&id, 0, StepKind::Tool, &step_hash("f", 0))
            .unwrap();
        // Fence check after the intent: stale -> refuse the call, journal
        // never_ran explicitly.
        db.complete_step(
            &intent,
            &AttemptOutcome::NeverRan {
                reason: "fence lost after intent, before send".into(),
            },
            None,
        )
        .unwrap();
        assert!(db.find_orphaned_intents(&id).unwrap().is_empty());
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.first_incomplete_step_no, Some(0));
        assert_eq!(plan.needs_review_step_no, None);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Normal flow: begin -> execute -> complete. The outcome row (attempt
    /// + 1) supersedes the intent; no orphans remain; resume replays the
    /// completed step.
    #[test]
    fn complete_step_after_intent() {
        let home = test_home("complete");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        let intent = db
            .begin_step(&id, 0, StepKind::Tool, &step_hash("c", 0))
            .unwrap();
        // The intent alone is not a completed step.
        assert_eq!(db.get_run(&id).unwrap().steps_done, 0);
        db.complete_step(&intent, &ok_outcome(), Some(r#"{"ok":true}"#))
            .unwrap();
        assert!(db.find_orphaned_intents(&id).unwrap().is_empty());
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.completed.len(), 1);
        assert_eq!(plan.completed[0].outcome.as_deref(), Some("executed_ok"));
        assert_eq!(plan.completed[0].attempt, 1);
        assert_eq!(plan.first_incomplete_step_no, Some(1));
        assert_eq!(db.get_run(&id).unwrap().steps_done, 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Intents do not consume the step budget; only decided outcomes do.
    #[test]
    fn intents_do_not_consume_budget() {
        let home = test_home("intentbudget");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", r#"{"max_steps":1}"#).unwrap();
        // An open intent must not trip max_steps=1 ...
        let intent = db
            .begin_step(&id, 0, StepKind::Tool, &step_hash("ib", 0))
            .unwrap();
        // ... and completing the first step is still allowed ...
        db.complete_step(&intent, &ok_outcome(), Some("{}"))
            .unwrap();
        // ... but a second begun+completed step exceeds the budget.
        let intent2 = db
            .begin_step(&id, 1, StepKind::Tool, &step_hash("ib", 1))
            .unwrap();
        let err = db
            .complete_step(&intent2, &ok_outcome(), Some("{}"))
            .unwrap_err();
        assert!(matches!(err, RunsError::BudgetExceeded { .. }));
        let _ = std::fs::remove_dir_all(&home);
    }

    // ---- Reconciliation (Amein's review: review is the last resort) ----

    fn file_hint(home: &std::path::Path, name: &str, content: &str) -> (String, String) {
        let path = home.join(name);
        std::fs::write(&path, content).unwrap();
        let hint = ReconcileHint {
            path: Some(path.to_string_lossy().to_string()),
            expected_hash: Some(sha256_hex(content.as_bytes())),
            idempotency_key: None,
        };
        (path.to_string_lossy().to_string(), hint.to_json().unwrap())
    }

    /// FileWrite orphan where the target already has the intended content:
    /// the probe proves the write landed → journaled as executed, NOT
    /// re-run, run NOT parked.
    #[test]
    fn reconcile_file_write_completed_probe() {
        let home = test_home("reconfiledone");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        db.transition(&id, RunState::Queued, RunState::ExecutingTools)
            .unwrap();
        let content = "chaos-file:complete\n";
        let (path, hint_json) = file_hint(&home, "target.txt", content);
        // Simulate: intent fsync'd, write landed, crash before outcome.
        let intent = db
            .begin_step_with_hint(
                &id,
                0,
                StepKind::FileWrite,
                &step_hash("rf", 0),
                Some(&hint_json),
            )
            .unwrap();
        assert!(matches!(
            reconcile_orphan(&intent),
            ReconcileVerdict::AlreadyComplete { .. }
        ));
        drop(db);
        let db = RunsDb::open(&home).unwrap();
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.needs_review_step_no, None);
        assert_eq!(plan.completed.len(), 1);
        assert_eq!(plan.completed[0].outcome.as_deref(), Some("executed_ok"));
        assert!(plan.completed[0]
            .output
            .as_deref()
            .unwrap_or("")
            .contains("probe_match"));
        assert_eq!(plan.first_incomplete_step_no, Some(1));
        // No orphan left behind; the file was NOT rewritten by resume.
        assert!(db.find_orphaned_intents(&id).unwrap().is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// FileWrite orphan where the target is missing: the write did not
    /// land → safe to re-run.
    #[test]
    fn reconcile_file_write_missing_reruns() {
        let home = test_home("reconfilemissing");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        let missing = home.join("not_there.txt");
        let hint = ReconcileHint {
            path: Some(missing.to_string_lossy().to_string()),
            expected_hash: Some(sha256_hex(b"intended")),
            idempotency_key: None,
        };
        // Crash after the intent but before the write.
        let intent = db
            .begin_step_with_hint(
                &id,
                0,
                StepKind::FileWrite,
                &step_hash("rf", 1),
                Some(&hint.to_json().unwrap()),
            )
            .unwrap();
        assert!(matches!(
            reconcile_orphan(&intent),
            ReconcileVerdict::Rerun { .. }
        ));
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.needs_review_step_no, None);
        assert_eq!(plan.first_incomplete_step_no, Some(0));
        // Re-executing writes the file; a later resume probes it complete.
        std::fs::write(&missing, b"intended").unwrap();
        let intent2 = db
            .begin_step_with_hint(
                &id,
                0,
                StepKind::FileWrite,
                &step_hash("rf", 1),
                Some(&hint.to_json().unwrap()),
            )
            .unwrap();
        assert_eq!(intent2.attempt, 1);
        db.complete_step(&intent2, &ok_outcome(), Some("{}"))
            .unwrap();
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.completed.len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// FileWrite orphan where the target has DIFFERENT content (torn
    /// write): hash mismatch → re-run, and the rerun overwrites wholesale.
    #[test]
    fn reconcile_file_write_hash_mismatch_reruns() {
        let home = test_home("reconfiletorn");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        let path = home.join("torn.txt");
        std::fs::write(&path, b"partial-write-").unwrap(); // torn content
        let hint = ReconcileHint {
            path: Some(path.to_string_lossy().to_string()),
            expected_hash: Some(sha256_hex(b"full intended content")),
            idempotency_key: None,
        };
        let intent = db
            .begin_step_with_hint(
                &id,
                0,
                StepKind::FileWrite,
                &step_hash("rf", 2),
                Some(&hint.to_json().unwrap()),
            )
            .unwrap();
        assert!(matches!(
            reconcile_orphan(&intent),
            ReconcileVerdict::Rerun { .. }
        ));
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.first_incomplete_step_no, Some(0));
        assert_eq!(plan.needs_review_step_no, None);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// FileWrite orphan with no usable hint: cannot probe → fail closed to
    /// review, never re-run blind.
    #[test]
    fn reconcile_file_write_no_hint_needs_review() {
        let home = test_home("reconfilenohint");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        let intent = db
            .begin_step(&id, 0, StepKind::FileWrite, &step_hash("rf", 3))
            .unwrap();
        assert!(intent.reconcile_hint.is_none());
        assert!(matches!(
            reconcile_orphan(&intent),
            ReconcileVerdict::NeedsReview { .. }
        ));
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.needs_review_step_no, Some(0));
        assert_eq!(db.get_run(&id).unwrap().state, RunState::NeedsReview);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// IdempotentHttp orphan with a key: re-run with the SAME key; the
    /// verdict surfaces the key so the worker replays it exactly.
    #[test]
    fn reconcile_idempotent_http_reruns_with_same_key() {
        let home = test_home("reconidem");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        let hint = ReconcileHint {
            path: None,
            expected_hash: None,
            idempotency_key: Some("idem:run:3".to_string()),
        };
        let intent = db
            .begin_step_with_hint(
                &id,
                3,
                StepKind::IdempotentHttp,
                &step_hash("ri", 3),
                Some(&hint.to_json().unwrap()),
            )
            .unwrap();
        match reconcile_orphan(&intent) {
            ReconcileVerdict::Rerun { reason } => {
                assert!(reason.contains("idem:run:3"), "reason: {reason}")
            }
            v => panic!("expected Rerun, got {v:?}"),
        }
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.needs_review_step_no, None);
        // Steps before the orphan are untouched; resume continues AT it.
        assert_eq!(plan.first_incomplete_step_no, Some(0));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// IdempotentHttp orphan without a key: cannot replay safely → review.
    #[test]
    fn reconcile_idempotent_http_no_key_needs_review() {
        let home = test_home("reconidemkeyless");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        let intent = db
            .begin_step(&id, 0, StepKind::IdempotentHttp, &step_hash("ri", 0))
            .unwrap();
        assert!(matches!(
            reconcile_orphan(&intent),
            ReconcileVerdict::NeedsReview { .. }
        ));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// OpaqueWrite orphan: no probe, no key → NEEDS_REVIEW, never replayed.
    #[test]
    fn reconcile_opaque_write_needs_review() {
        let home = test_home("reconopaque");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        db.transition(&id, RunState::Queued, RunState::ExecutingTools)
            .unwrap();
        let intent = db
            .begin_step(&id, 0, StepKind::OpaqueWrite, &step_hash("ro", 0))
            .unwrap();
        assert!(matches!(
            reconcile_orphan(&intent),
            ReconcileVerdict::NeedsReview { .. }
        ));
        drop(db);
        let db = RunsDb::open(&home).unwrap();
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.needs_review_step_no, Some(0));
        assert!(plan.completed.is_empty());
        assert_eq!(db.get_run(&id).unwrap().state, RunState::NeedsReview);
        // Still listed as an orphan — nothing was silently replayed.
        assert_eq!(db.find_orphaned_intents(&id).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// ReadOnly orphan: reads have no side effect → safe to re-run.
    #[test]
    fn reconcile_readonly_reruns() {
        let home = test_home("reconread");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        let intent = db
            .begin_step(&id, 0, StepKind::ReadOnly, &step_hash("rr", 0))
            .unwrap();
        assert!(matches!(
            reconcile_orphan(&intent),
            ReconcileVerdict::Rerun { .. }
        ));
        let plan = db.resume_run(&id).unwrap();
        assert_eq!(plan.first_incomplete_step_no, Some(0));
        assert_eq!(plan.needs_review_step_no, None);
        assert_eq!(db.get_run(&id).unwrap().state, RunState::Queued);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The reconcile hint survives the crash: written in the fsync'd
    /// intent row, readable from the orphan after reopen.
    #[test]
    fn reconcile_hint_survives_crash() {
        let home = test_home("reconhintcrash");
        let id = {
            let db = RunsDb::open(&home).unwrap();
            let id = db.create_run(None, "{}", "{}").unwrap();
            let hint = ReconcileHint {
                path: None,
                expected_hash: None,
                idempotency_key: Some("idem:crash:1".to_string()),
            };
            db.begin_step_with_hint(
                &id,
                0,
                StepKind::IdempotentHttp,
                &step_hash("rh", 0),
                Some(&hint.to_json().unwrap()),
            )
            .unwrap();
            drop(db); // kill -9
            id
        };
        let db = RunsDb::open(&home).unwrap();
        let orphans = db.find_orphaned_intents(&id).unwrap();
        assert_eq!(orphans.len(), 1);
        let hint = orphans[0]
            .reconcile_hint
            .as_deref()
            .and_then(ReconcileHint::from_json)
            .expect("hint parses");
        assert_eq!(hint.idempotency_key.as_deref(), Some("idem:crash:1"));
        // And the verdict still resolves to Rerun with that key.
        assert!(matches!(
            reconcile_orphan(&orphans[0]),
            ReconcileVerdict::Rerun { .. }
        ));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// step_attempts exposes the attempt history for verification.
    #[test]
    fn step_attempts_history() {
        let home = test_home("attempts");
        let db = RunsDb::open(&home).unwrap();
        let id = db.create_run(None, "{}", "{}").unwrap();
        let i1 = db
            .begin_step(&id, 0, StepKind::Model, &step_hash("a", 0))
            .unwrap();
        drop(db);
        let db = RunsDb::open(&home).unwrap();
        let i2 = db
            .begin_step(&id, 0, StepKind::Model, &step_hash("a", 0))
            .unwrap();
        assert_eq!(i1.attempt, 0);
        assert_eq!(i2.attempt, 1);
        db.complete_step(&i2, &ok_outcome(), Some("{}")).unwrap();
        let attempts = db.step_attempts(&id, 0).unwrap();
        assert_eq!(attempts.len(), 3);
        assert_eq!(attempts[0].outcome.as_deref(), Some("started"));
        assert_eq!(attempts[1].outcome.as_deref(), Some("started"));
        assert_eq!(attempts[2].outcome.as_deref(), Some("executed_ok"));
        let _ = std::fs::remove_dir_all(&home);
    }
}
