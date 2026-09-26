//! Scheduled autonomous-session policy.
//!
//! **Definition.** A *scheduled autonomous session* is a session an operator
//! arms to run an agent task on a schedule with no interactive Controller
//! attached (no human watching, no approval prompt answerable). This module
//! is the complete safety contract for such runs: what they may do, what
//! they may never do, and how every bound is enforced and audited.
//!
//! **Policy rules (safe defaults; adjustable per schedule within the
//! absolute ceilings):**
//!
//! 1. **Opt-in.** Nothing runs on a schedule unless a [`ScheduleSpec`]
//!    exists. Specs are created by the operator through an explicit
//!    management verb — never from inside a session, so a scheduled run
//!    can neither create nor modify schedules (no runaway forking).
//! 2. **No human ⇒ no Ask.** [`decide_autonomous`] maps `Ask` to
//!    [`DenyReason::NoHumanPresent`]. An unattended agent only ever runs
//!    `Allow` tools; anything that would need a human's judgment fails
//!    closed instead of hanging on an unanswerable prompt. The only way to
//!    widen an autonomous run's capabilities is the existing explicit
//!    mechanism: the operator sets the tool to `Allow` in the session's
//!    attachment record (auditable, per-session).
//! 3. **One session per schedule.** A schedule names exactly one session
//!    and cannot touch others.
//! 4. **Resource bounds.** Every run carries [`AutonomousPolicy`]:
//!    wall-clock cap (default 30 min), agent-step cap (default 200),
//!    output cap (default 4 MiB). Exceeding a cap kills the run.
//! 5. **Single-flight.** A schedule never overlaps itself: a trigger that
//!    fires while the previous run is active is *skipped* (not queued),
//!    via [`RunGuard`]. Skips are audited.
//! 6. **Bounded retries.** Automatic retries default to 0 and cap at 3
//!    with backoff; persistent failure notifies through the existing
//!    `notify_when_done` path instead of retrying forever.
//! 7. **Audit.** Every trigger appends a [`RunRecord`] to
//!    `<session-dir>/scheduled-runs.jsonl`: outcome, steps, every
//!    policy-denied tool, errors. Policy denials are distinguishable from
//!    explicit `Deny`s.
//! 8. **Kill switch.** Specs are pausable/deletable; pausing takes effect
//!    before the next trigger.
//!
//! **Status.** The full contract is implemented and tested: the decision
//! function, spec validation, [`ScheduledRunner`] (single-flight guard,
//! autonomous connector mode, duration/step/output caps, bounded retries
//! with backoff, one [`RunRecord`] appended per trigger), and the
//! tick-based [`Scheduler`] daemon that arms specs from
//! `<SUPERCLI_HOME>/schedules.json` and fires due triggers. Drive schedules
//! with `supercli schedule daemon` (or `supercli schedule run-once` for one
//! manual trigger). Do NOT drive schedules from system cron: single-flight
//! is enforced in-process by the daemon's [`RunGuard`], so a second driver
//! would break the no-overlap guarantee.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use supercli_connector::ApprovalPolicy;

use crate::durable_runs::{step_input_hash, RunState, RunsDb, StepKind};
use crate::schedule_leases::LeaseFence;

/// Minimum schedule interval: 60 s. Sub-minute schedules are rejected —
/// that is how a misconfigured schedule becomes a hot loop.
pub const MIN_INTERVAL_SECS: u64 = 60;
/// Hard ceiling for one run's wall clock: 24 h. Larger is a config error.
pub const MAX_DURATION_SECS: u64 = 24 * 3600;
/// Hard ceiling for agent steps in one run.
pub const MAX_STEPS: u32 = 10_000;
/// Hard ceiling for automatic retries of a failed run.
pub const MAX_RETRIES: u8 = 3;
/// Longest schedule id the validator accepts.
pub const MAX_ID_LEN: usize = 64;

/// Resource and retry bounds for one autonomous run. Defaults are the safe
/// starting point; the operator may adjust them per schedule, and every
/// value is validated against the absolute ceilings above, so even a
/// fully loosened schedule stays bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutonomousPolicy {
    /// Kill the run after this many seconds (default 1800 = 30 min).
    pub max_duration_secs: u64,
    /// Kill the run after this many agent steps (default 200).
    pub max_steps: u32,
    /// Kill the run after this many output bytes (default 4 MiB).
    pub max_output_bytes: u64,
    /// Automatic retries of a failed run, with backoff (default 0).
    pub max_retries: u8,
}

impl Default for AutonomousPolicy {
    fn default() -> Self {
        Self {
            max_duration_secs: 1800,
            max_steps: 200,
            max_output_bytes: 4 * 1024 * 1024,
            max_retries: 0,
        }
    }
}

/// Why a [`ScheduleSpec`] was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    BadId,
    EmptySessionId,
    IntervalTooShort,
    BadDuration,
    BadSteps,
    BadRetries,
    /// The task lists no tool calls — nothing to run.
    EmptyTask,
    /// A tool name is empty, too long, or contains illegal characters.
    BadToolName,
    /// A tool's arguments are neither a JSON object nor null.
    BadArguments,
    /// The task lists more steps than `max_steps` allows.
    TooManySteps,
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            PolicyError::BadId => "schedule id must be 1-64 chars of [A-Za-z0-9_-]",
            PolicyError::EmptySessionId => "schedule must name a session",
            PolicyError::IntervalTooShort => "schedule interval must be at least 60 seconds",
            PolicyError::BadDuration => "max_duration_secs must be 1..=86400",
            PolicyError::BadSteps => "max_steps must be 1..=10000",
            PolicyError::BadRetries => "max_retries must be 0..=3",
            PolicyError::EmptyTask => "schedule task must list at least one tool call",
            PolicyError::BadToolName => "tool names must be 1-128 chars of [A-Za-z0-9._-]",
            PolicyError::BadArguments => "tool arguments must be a JSON object",
            PolicyError::TooManySteps => "task lists more steps than max_steps allows",
        };
        write!(f, "{msg}")
    }
}

/// One connector tool call in a scheduled task: the exact tool and its
/// exact arguments. Scheduled tasks are explicit, never agentic: the
/// operator writes down precisely what the unattended run may do, and the
/// runner executes the list in order with no improvisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledToolCall {
    /// Connector tool name, e.g. `"allowy.echo"`.
    pub tool: String,
    /// Arguments passed verbatim to the tool. Null means "no arguments".
    #[serde(default)]
    pub arguments: serde_json::Value,
}

/// What a scheduled trigger executes. Today there is exactly one kind: an
/// explicit ordered list of connector tool calls. A prompt-driven agentic
/// task is deliberately NOT offered: an unattended agent loop cannot be
/// bounded the way an explicit tool list can.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledTask {
    ToolCalls(Vec<ScheduledToolCall>),
}

impl ScheduledTask {
    /// Number of tool calls the task executes: one agent step each.
    pub fn step_count(&self) -> usize {
        match self {
            ScheduledTask::ToolCalls(calls) => calls.len(),
        }
    }
}

/// Longest tool name the validator accepts.
pub const MAX_TOOL_NAME_LEN: usize = 128;

/// One armed schedule: which session, how often, under which bounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleSpec {
    /// Operator-chosen id, `[A-Za-z0-9_-]{1,64}`.
    pub id: String,
    /// The one session this schedule may drive.
    pub session_id: String,
    /// Seconds between triggers; at least [`MIN_INTERVAL_SECS`].
    pub interval_secs: u64,
    /// Bounds every run of this schedule inherits.
    pub policy: AutonomousPolicy,
    /// Paused specs never trigger.
    pub enabled: bool,
    /// What the trigger executes. Part of the validated spec: a schedule
    /// with an empty, malformed, or over-budget task is rejected at
    /// creation, never at 3 AM.
    pub task: ScheduledTask,
}

impl ScheduleSpec {
    /// Validate every field. Fails closed: any violation rejects the spec.
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.id.is_empty()
            || self.id.len() > MAX_ID_LEN
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(PolicyError::BadId);
        }
        if self.session_id.is_empty() {
            return Err(PolicyError::EmptySessionId);
        }
        if self.interval_secs < MIN_INTERVAL_SECS {
            return Err(PolicyError::IntervalTooShort);
        }
        if self.policy.max_duration_secs == 0 || self.policy.max_duration_secs > MAX_DURATION_SECS {
            return Err(PolicyError::BadDuration);
        }
        if self.policy.max_steps == 0 || self.policy.max_steps > MAX_STEPS {
            return Err(PolicyError::BadSteps);
        }
        if self.policy.max_retries > MAX_RETRIES {
            return Err(PolicyError::BadRetries);
        }
        let ScheduledTask::ToolCalls(calls) = &self.task;
        if calls.is_empty() {
            return Err(PolicyError::EmptyTask);
        }
        if calls.len() as u32 > self.policy.max_steps {
            return Err(PolicyError::TooManySteps);
        }
        for call in calls {
            if call.tool.is_empty()
                || call.tool.len() > MAX_TOOL_NAME_LEN
                || !call
                    .tool
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            {
                return Err(PolicyError::BadToolName);
            }
            // `call_tool` silently drops non-object arguments; the spec
            // rejects them instead so the operator sees what will run.
            if !call.arguments.is_object() && !call.arguments.is_null() {
                return Err(PolicyError::BadArguments);
            }
        }
        Ok(())
    }
}

/// Why an autonomous tool call was denied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    /// The tool's policy is `Ask` and no human is present to answer —
    /// fail closed. Distinct from [`DenyReason::ExplicitDeny`] in the audit
    /// trail so reviewers can see policy denials vs operator denials.
    NoHumanPresent,
    /// The tool's policy is `Ask` and the human answered the prompt with a
    /// decline. Only reachable in interactive mode; autonomous mode denies
    /// `Ask` tools as [`DenyReason::NoHumanPresent`] before any prompt.
    AskDeclined,
    /// The tool's policy is `Deny`.
    ExplicitDeny,
}

/// The one decision the policy makes at the tool gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomousDecision {
    Allow,
    Deny(DenyReason),
}

/// Map a session tool policy to the autonomous decision. `Ask` can never
/// be satisfied without a human, so it fails closed.
pub fn decide_autonomous(policy: ApprovalPolicy) -> AutonomousDecision {
    match policy {
        ApprovalPolicy::Allow => AutonomousDecision::Allow,
        ApprovalPolicy::Ask => AutonomousDecision::Deny(DenyReason::NoHumanPresent),
        ApprovalPolicy::Deny => AutonomousDecision::Deny(DenyReason::ExplicitDeny),
    }
}

/// Single-flight guard: a schedule never overlaps itself. The trigger that
/// finds its schedule already active must skip (and audit the skip), not
/// queue.
pub struct RunGuard {
    active: Mutex<HashSet<String>>,
}

/// Proof that a schedule's run slot is held; the runner must hand it back
/// to [`RunGuard::end`]. Not `Clone` — one token, one run.
pub struct RunToken {
    schedule_id: String,
}

impl RunGuard {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(HashSet::new()),
        }
    }

    /// Try to claim the run slot. `Some` = this trigger owns the run;
    /// `None` = a run is already active, skip this trigger.
    pub fn try_begin(&self, schedule_id: &str) -> Option<RunToken> {
        let mut active = self.active.lock().ok()?;
        if active.contains(schedule_id) {
            return None;
        }
        active.insert(schedule_id.to_string());
        Some(RunToken {
            schedule_id: schedule_id.to_string(),
        })
    }

    /// Release the slot. Always call, on every exit path of the run.
    pub fn end(&self, token: RunToken) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(&token.schedule_id);
        }
    }

    pub fn is_active(&self, schedule_id: &str) -> bool {
        self.active
            .lock()
            .map(|active| active.contains(schedule_id))
            .unwrap_or(false)
    }
}

impl Default for RunGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// How a scheduled run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Completed,
    Failed,
    /// Killed by a resource cap or the operator.
    Killed,
    /// Exceeded `max_duration_secs`.
    TimedOut,
    /// Trigger fired while the previous run was still active.
    SkippedOverlap,
    /// Takeover of a lapsed lease found the lapsed run's attempt log
    /// uncertain (an in-flight or ambiguous attempt): the schedule was
    /// NOT re-fired. A human must resolve the uncertain attempt through
    /// the explicit replacement path; the lease is held (not released) so
    /// no other worker re-fires behind the escalation.
    NeedsReview,
}

/// One audit line per trigger. Appended to
/// `<session-dir>/scheduled-runs.jsonl` — one JSON object per line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    pub schedule_id: String,
    pub session_id: String,
    pub triggered_at_ms: u64,
    pub outcome: RunOutcome,
    pub steps: u32,
    /// Every tool the policy denied during the run (Ask→deny included).
    pub denied_tools: Vec<String>,
    pub error: Option<String>,
}

impl RunRecord {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Append one [`RunRecord`] to `<session_dir>/scheduled-runs.jsonl`
/// (created if missing). Single `write_all` of one line — appends from
/// concurrent runs cannot interleave mid-line on POSIX.
pub fn append_run_record(session_dir: &Path, record: &RunRecord) -> io::Result<()> {
    let path = session_dir.join("scheduled-runs.jsonl");
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let mut line = record.to_json();
    line.push('\n');
    file.write_all(line.as_bytes())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Trigger runner
// ---------------------------------------------------------------------------

/// The tool-execution surface a scheduled run drives. Implemented by
/// [`crate::session_connectors::SessionConnectors`]; fakes implement it in
/// tests. Object-safe so the daemon can hold boxed executors.
pub trait ScheduledToolExecutor {
    /// See `SessionConnectors::set_autonomous`.
    fn set_autonomous(&mut self, autonomous: bool);
    /// See `SessionConnectors::is_autonomous`.
    fn is_autonomous(&self) -> bool;
    /// See `SessionConnectors::set_actor`. The runner sets
    /// `scheduled:<trigger-id>` so the review log names the trigger.
    fn set_actor(&mut self, actor: String);
    /// Install the fencing token for this run (`None` clears it). The
    /// scheduled runner sets it right after winning the lease claim; every
    /// side-effecting step then refuses to run when the fence is stale.
    fn set_lease_fence(&mut self, fence: Option<LeaseFence>);
    /// The fencing token installed by [`ScheduledToolExecutor::set_lease_fence`].
    fn lease_fence(&self) -> Option<&LeaseFence>;
    /// See `SessionConnectors::call_tool_detailed`.
    fn call_tool_detailed(
        &mut self,
        tool: &str,
        arguments: &serde_json::Value,
    ) -> Result<String, crate::session_connectors::ToolCallFailure>;
}

/// Time source for the runner. [`SystemClock`] in production; fakes in
/// tests, so timeouts and backoff are exercised without real sleeps.
pub trait RunClock {
    fn now_ms(&self) -> u64;
    fn sleep(&self, duration: Duration);
}

/// [`RunClock`] over the real system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl RunClock for SystemClock {
    fn now_ms(&self) -> u64 {
        now_ms()
    }
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Backoff between automatic retries: 5 s, 10 s, 20 s for the three
/// allowed retries, capped at 5 minutes (defense in depth if the ceiling
/// ever moves). Index 0 is the wait before the first retry.
pub fn retry_delay_for_attempt(retry_index: u32) -> Duration {
    let secs = 5u64.saturating_mul(1u64 << retry_index.min(6));
    Duration::from_secs(secs.min(300))
}

/// `run_trigger` computed the run's outcome but could not append its audit
/// record. The record is returned so the caller can surface it another
/// way; it must never be silently dropped.
#[derive(Debug)]
pub struct RunAuditError {
    pub record: RunRecord,
    pub source: io::Error,
}

impl std::fmt::Display for RunAuditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scheduled audit append failed: {}", self.source)
    }
}

impl std::error::Error for RunAuditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Releases the schedule's single-flight slot on drop: every exit path of
/// `run_trigger` — success, failure, even panic unwind — hands the token
/// back so the next trigger is not wedged shut.
struct RunRelease<'a> {
    guard: &'a RunGuard,
    token: Option<RunToken>,
}

impl Drop for RunRelease<'_> {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            self.guard.end(token);
        }
    }
}

/// Restores the executor's previous autonomous flag on drop, so a shared
/// `SessionConnectors` is never left in autonomous mode by a scheduled run.
struct AutonomousRestore<'a> {
    executor: &'a mut dyn ScheduledToolExecutor,
    previous: bool,
}

impl Drop for AutonomousRestore<'_> {
    fn drop(&mut self) {
        self.executor.set_autonomous(self.previous);
    }
}

/// The outcome of one attempt (before retries).
struct AttemptResult {
    outcome: RunOutcome,
    steps: u32,
    denied: Vec<String>,
    error: Option<String>,
    /// True when the attempt ended on an uncertain tool failure: the call
    /// may have executed, so the retry loop must not retry it even when
    /// retries remain. The attempt is already persisted as non-retryable
    /// in the connector audit log.
    uncertain: bool,
    /// True when the attempt ended on a stale lease fence: the worker
    /// lost the lease mid-run. Like `uncertain`, this never retries —
    /// retrying without the lease would just fail the fence again — and
    /// unlike `uncertain` the refused tool provably did NOT execute (the
    /// fence is checked before the call).
    stale_lease: bool,
}

/// Fires scheduled triggers: validates the spec, claims the single-flight
/// slot, executes the task with the executor in autonomous mode, enforces
/// the [`AutonomousPolicy`] caps, retries per policy, and appends exactly
/// one [`RunRecord`] per trigger — on every outcome, including validation
/// refusal and overlap skips.
pub struct ScheduledRunner<C: RunClock = SystemClock> {
    guard: RunGuard,
    clock: C,
    runs_db: Option<RunsDb>,
    /// Fail closed by default: if true and `runs_db` is None, `run_trigger`
    /// refuses to fire (audits a Failed record) instead of running
    /// unjournaled. Explicit opt-out via [`Self::allow_unjournaled`].
    durable_required: bool,
    /// Optional callback invoked with the durable run id immediately after
    /// the run is created (before any tool executes). Used by tests to
    /// synchronize on run creation (e.g., SIGKILL tests that must wait
    /// until the run exists before killing).
    on_run_created: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

impl<C: RunClock> ScheduledRunner<C> {
    pub fn new(clock: C) -> Self {
        Self {
            guard: RunGuard::new(),
            clock,
            runs_db: None,
            durable_required: true,
            on_run_created: None,
        }
    }

    /// Attach a durable-runs database. When present, triggers are journaled
    /// as durable runs: the run is created (or an orphaned run for the same
    /// schedule is resumed) before execution, every tool call is journaled
    /// write-ahead, and a crash mid-run is picked up by the next trigger
    /// invocation instead of starting over.
    pub fn with_durable_runs(mut self, db: RunsDb) -> Self {
        self.runs_db = Some(db);
        self
    }

    /// Set the durable-runs database on an existing runner.
    pub fn set_durable_runs(&mut self, db: RunsDb) {
        self.runs_db = Some(db);
    }

    /// Explicit opt-out of fail-closed durable runs: the runner will fire
    /// triggers without journaling. This is the only way to run unjournaled;
    /// the default is to refuse. The caller must have an explicit reason
    /// (e.g., `--no-durable` flag) to use this.
    pub fn allow_unjournaled(mut self) -> Self {
        self.durable_required = false;
        self
    }

    /// Set the unjournaled opt-out on an existing runner.
    pub fn set_allow_unjournaled(&mut self) {
        self.durable_required = false;
    }

    /// Set a callback invoked with the durable run id immediately after the
    /// run is created (before any tool executes). Used by tests to
    /// synchronize on run creation.
    pub fn on_run_created<F>(mut self, f: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.on_run_created = Some(Box::new(f));
        self
    }

    /// Fire one trigger for `spec` against `session_dir`, driving
    /// `executor`. Returns the appended audit record, or [`RunAuditError`]
    /// if the run completed but the record could not be appended.
    pub fn run_trigger(
        &self,
        spec: &ScheduleSpec,
        session_dir: &Path,
        executor: &mut dyn ScheduledToolExecutor,
    ) -> Result<RunRecord, RunAuditError> {
        let triggered_at_ms = self.clock.now_ms();

        // Validate before touching the guard or any tool: a bad spec never
        // runs, but the refusal is still audited.
        if let Err(error) = spec.validate() {
            return self.audit(
                session_dir,
                RunRecord {
                    schedule_id: spec.id.clone(),
                    session_id: spec.session_id.clone(),
                    triggered_at_ms,
                    outcome: RunOutcome::Failed,
                    steps: 0,
                    denied_tools: vec![],
                    error: Some(format!("invalid schedule: {error}")),
                },
            );
        }
        if !spec.enabled {
            return self.audit(
                session_dir,
                RunRecord {
                    schedule_id: spec.id.clone(),
                    session_id: spec.session_id.clone(),
                    triggered_at_ms,
                    outcome: RunOutcome::Failed,
                    steps: 0,
                    denied_tools: vec![],
                    error: Some("schedule is paused".to_string()),
                },
            );
        }
        // Fail closed: without a durable-runs database, a crash mid-run
        // would lose the journal and risk double-execution on resume.
        // Refuse to fire unless the operator explicitly opted out of
        // journaling (allow_unjournaled).
        if self.durable_required && self.runs_db.is_none() {
            return self.audit(
                session_dir,
                RunRecord {
                    schedule_id: spec.id.clone(),
                    session_id: spec.session_id.clone(),
                    triggered_at_ms,
                    outcome: RunOutcome::Failed,
                    steps: 0,
                    denied_tools: vec![],
                    error: Some(
                        "durable runs unavailable: refusing to fire without journal \
                         (explicit opt-out required: allow_unjournaled / --no-durable)"
                            .to_string(),
                    ),
                },
            );
        }
        let Some(token) = self.guard.try_begin(&spec.id) else {
            return self.audit(
                session_dir,
                RunRecord {
                    schedule_id: spec.id.clone(),
                    session_id: spec.session_id.clone(),
                    triggered_at_ms,
                    outcome: RunOutcome::SkippedOverlap,
                    steps: 0,
                    denied_tools: vec![],
                    error: None,
                },
            );
        };
        // From here on the slot is held; RAII returns it on every path.
        let _release = RunRelease {
            guard: &self.guard,
            token: Some(token),
        };

        // Autonomous mode for the whole run, restored afterwards even on
        // failure: Ask tools fail closed instead of hanging on a prompt no
        // human will ever answer.
        let previous = executor.is_autonomous();
        executor.set_autonomous(true);
        // The review log names the trigger as the actor for every call
        // this run makes.
        executor.set_actor(format!("scheduled:{}", spec.id));
        let autonomous = AutonomousRestore { executor, previous };

        let policy = spec.policy;
        let ScheduledTask::ToolCalls(calls) = &spec.task;

        // Durable runs: find an orphaned run for this schedule (crash
        // recovery) or create a fresh one. The run id links the journal to
        // this trigger; on resume, already-completed steps are skipped.
        let durable_ctx: Option<(String, u64)> = self.runs_db.as_ref().and_then(|db| {
            // Look for a non-terminal run whose plan names this schedule.
            let orphan = db.list_runs(None).ok()?.into_iter().find(|r| {
                !r.state.is_terminal()
                    && r.plan.contains(&format!("\"schedule_id\":\"{}\"", spec.id))
            });
            match orphan {
                Some(run) => {
                    // Resume: replay completed steps, continue at the first
                    // incomplete one. An ambiguous step forces NEEDS_REVIEW.
                    match db.resume_run(&run.id) {
                        Ok(plan) => {
                            if plan.needs_review_step_no.is_some() {
                                // Uncertain write: do not execute anything.
                                // The run is already marked NEEDS_REVIEW.
                                return Some((run.id, u64::MAX));
                            }
                            let start = plan.first_incomplete_step_no.unwrap_or(0);
                            // Claim the lease for this run so a concurrent
                            // trigger cannot double-execute.
                            let _ = db.claim_run(&run.id);
                            Some((run.id, start))
                        }
                        Err(_) => None,
                    }
                }
                None => {
                    // Fresh run: plan JSON carries the schedule identity.
                    let plan = format!(
                        "{{\"schedule_id\":\"{}\",\"session_id\":\"{}\",\"triggered_at_ms\":{}}}",
                        spec.id, spec.session_id, triggered_at_ms
                    );
                    let budgets = format!(
                        "{{\"max_steps\":{},\"max_duration_secs\":{}}}",
                        policy.max_steps, policy.max_duration_secs
                    );
                    match db.create_run(None, &plan, &budgets) {
                        Ok(id) => {
                            let _ = db.claim_run(&id);
                            // Transition QUEUED -> EXECUTING_TOOLS.
                            let _ = db.transition(&id, RunState::Queued, RunState::ExecutingTools);
                            // Notify the run-created callback (test synchronization).
                            if let Some(cb) = self.on_run_created.as_ref() {
                                cb(&id);
                            }
                            Some((id, 0))
                        }
                        Err(_) => None,
                    }
                }
            }
        });

        // Denied tools accumulate across retries (deduped below): a denial
        // is a security-relevant signal even if a later retry succeeds.
        let mut denied_tools: Vec<String> = Vec::new();
        let mut attempt_index: u32 = 0;
        let result = loop {
            let attempt = self.run_attempt(
                &policy,
                calls,
                autonomous.executor,
                durable_ctx
                    .as_ref()
                    .map(|(id, start)| (id.as_str(), *start)),
            );
            for tool in &attempt.denied {
                if !denied_tools.contains(tool) {
                    denied_tools.push(tool.clone());
                }
            }
            // Only transient failures retry. Killed covers resource-cap and
            // operator kills: retrying a run that blew its output cap (or
            // that someone deliberately killed) would just blow it again.
            // Uncertain tool failures never retry: the call may already
            // have executed, so a retry could double-apply a write. The
            // attempt is persisted as non-retryable; a human must issue
            // an explicit replacement. Stale-lease failures never retry
            // either: the lease is gone, so a retry would just fail the
            // fence again — and the refused tool provably did not run.
            let retryable = matches!(attempt.outcome, RunOutcome::Failed | RunOutcome::TimedOut)
                && !attempt.uncertain
                && !attempt.stale_lease;
            if !retryable || attempt_index >= policy.max_retries as u32 {
                break attempt;
            }
            self.clock.sleep(retry_delay_for_attempt(attempt_index));
            attempt_index += 1;
        };
        denied_tools.sort();
        denied_tools.dedup();

        // Mark the durable run terminal. A crash before this point leaves
        // the run in a non-terminal state, so the next trigger resumes it.
        if let Some((run_id, _)) = durable_ctx.as_ref() {
            if let Some(db) = self.runs_db.as_ref() {
                if result.outcome == RunOutcome::Completed {
                    // The run was in EXECUTING_TOOLS; transition to DONE.
                    // (If it was already terminal, the conditional update
                    // is a no-op.) Also try from other non-terminal states,
                    // in case the run was created but never transitioned.
                    let _ = db.transition(run_id, RunState::ExecutingTools, RunState::Done);
                    let _ = db.transition(run_id, RunState::Queued, RunState::Done);
                    let _ = db.transition(run_id, RunState::AwaitingModel, RunState::Done);
                    let _ = db.transition(run_id, RunState::AwaitingApproval, RunState::Done);
                    let _ = db.transition(run_id, RunState::Paused, RunState::Done);
                } else if result.uncertain {
                    let _ = db.mark_needs_review(run_id);
                } else {
                    let _ = db.transition(run_id, RunState::ExecutingTools, RunState::Failed);
                    let _ = db.transition(run_id, RunState::Queued, RunState::Failed);
                    let _ = db.transition(run_id, RunState::AwaitingModel, RunState::Failed);
                    let _ = db.transition(run_id, RunState::AwaitingApproval, RunState::Failed);
                    let _ = db.transition(run_id, RunState::Paused, RunState::Failed);
                }
                // Release the lease now that the run is terminal.
                let _ = db.release_run(run_id);
            }
        }

        // Run completion is a side-effecting step: a worker that lost its
        // lease mid-run must not write a completion record for a run
        // another worker may have taken over. The fence is checked before
        // the audit append; on stale the record is returned unaudited so
        // the in-memory report still reflects the failure, but no shared
        // state is written by the stale worker.
        if let Some(fence) = autonomous.executor.lease_fence() {
            if !fence.is_current() {
                return Ok(RunRecord {
                    schedule_id: spec.id.clone(),
                    session_id: spec.session_id.clone(),
                    triggered_at_ms,
                    outcome: RunOutcome::Failed,
                    steps: result.steps,
                    denied_tools,
                    error: Some(format!(
                        "stale lease: lost the lease mid-run; completion not recorded{}",
                        result
                            .error
                            .as_ref()
                            .map(|e| format!("; last attempt error: {e}"))
                            .unwrap_or_default()
                    )),
                });
            }
        }

        self.audit(
            session_dir,
            RunRecord {
                schedule_id: spec.id.clone(),
                session_id: spec.session_id.clone(),
                triggered_at_ms,
                outcome: result.outcome,
                // Steps executed in the final attempt. One record is
                // written per trigger, so retries don't inflate this.
                steps: result.steps,
                denied_tools,
                error: result.error,
            },
        )
    }

    /// Execute the task list once, enforcing the wall-clock, step, and
    /// output caps. The first denied or failed tool ends the attempt
    /// immediately (fail closed); completed steps before it still count.
    ///
    /// When `durable` is `Some((run_id, start_step))`, every tool call is
    /// journaled write-ahead to the durable run: a 'started' intent row
    /// (no outcome) is appended before the call, then a completion row
    /// after. Steps before `start_step` were completed by a previous
    /// (crashed) invocation and are skipped, not re-executed.
    fn run_attempt(
        &self,
        policy: &AutonomousPolicy,
        calls: &[ScheduledToolCall],
        executor: &mut dyn ScheduledToolExecutor,
        durable: Option<(&str, u64)>,
    ) -> AttemptResult {
        let started_ms = self.clock.now_ms();
        let deadline_ms = started_ms.saturating_add(policy.max_duration_secs.saturating_mul(1000));
        let mut steps: u32 = 0;
        let mut output_bytes: u64 = 0;
        let mut denied: Vec<String> = Vec::new();
        // Durable resume: skip steps already completed by a crashed
        // invocation. `step_no` is the journal index; `steps` counts
        // steps executed in this attempt.
        let start_step = durable.map(|(_, s)| s).unwrap_or(0);
        // A resumed run with an ambiguous step never reaches here: the
        // caller returns early with NEEDS_REVIEW (see run_trigger).
        if start_step == u64::MAX {
            return AttemptResult {
                outcome: RunOutcome::NeedsReview,
                steps: 0,
                denied,
                error: Some("durable run has an ambiguous step; human review required".to_string()),
                uncertain: true,
                stale_lease: false,
            };
        }
        for (idx, call) in calls.iter().enumerate() {
            let step_no = idx as u64;
            if step_no < start_step {
                // Already completed by a previous invocation: count it
                // toward the total but do not re-execute.
                steps += 1;
                continue;
            }
            // The wall-clock cap is enforced between tool calls. A single
            // call is itself bounded by the connector transport timeout,
            // so the deadline can be overshot by at most one call.
            if self.clock.now_ms() >= deadline_ms {
                return AttemptResult {
                    outcome: RunOutcome::TimedOut,
                    steps,
                    denied,
                    error: Some(format!(
                        "exceeded max_duration_secs ({})",
                        policy.max_duration_secs
                    )),
                    uncertain: false,
                    stale_lease: false,
                };
            }
            // Write-ahead: journal the intent BEFORE executing. A crash
            // after this row but before the call leaves a 'started' row
            // with no outcome; resume classifies it via reconcile_orphan
            // (Rerun/AlreadyComplete/NeedsReview).
            let input_hash = step_input_hash(&format!("{}:{}", call.tool, call.arguments));
            let intent = if let Some((run_id, _)) = durable {
                if let Some(db) = self.runs_db.as_ref() {
                    match db.begin_step(run_id, step_no, StepKind::Tool, &input_hash) {
                        Ok(intent) => Some(intent),
                        Err(_) => None,
                    }
                } else {
                    None
                }
            } else {
                None
            };
            match executor.call_tool_detailed(&call.tool, &call.arguments) {
                Ok(text) => {
                    steps += 1;
                    output_bytes = output_bytes.saturating_add(text.len() as u64);
                    // Journal the completion.
                    if let Some(intent) = intent {
                        if let Some(db) = self.runs_db.as_ref() {
                            let _ = db.complete_step(
                                &intent,
                                &crate::action_reviews::AttemptOutcome::Executed { success: true },
                                Some(&text),
                            );
                        }
                    }
                    if output_bytes > policy.max_output_bytes {
                        return AttemptResult {
                            outcome: RunOutcome::Killed,
                            steps,
                            denied,
                            error: Some(format!(
                                "exceeded max_output_bytes ({})",
                                policy.max_output_bytes
                            )),
                            uncertain: false,
                            stale_lease: false,
                        };
                    }
                    // Unreachable for ToolCalls tasks (validated against
                    // max_steps), kept as defense in depth for future task
                    // kinds.
                    if steps >= policy.max_steps {
                        return AttemptResult {
                            outcome: RunOutcome::Killed,
                            steps,
                            denied,
                            error: Some(format!("exceeded max_steps ({})", policy.max_steps)),
                            uncertain: false,
                            stale_lease: false,
                        };
                    }
                }
                Err(failure) => {
                    if failure.denied.is_some() && !denied.contains(&call.tool) {
                        denied.push(call.tool.clone());
                    }
                    // Journal the failure. An uncertain failure maps to
                    // Ambiguous (NEEDS_REVIEW, never replayed); a definite
                    // failure maps to Executed{success:false}.
                    if let Some(intent) = intent {
                        if let Some(db) = self.runs_db.as_ref() {
                            let outcome = if failure.uncertain {
                                crate::action_reviews::AttemptOutcome::Ambiguous {
                                    reason: failure.message.clone(),
                                }
                            } else {
                                crate::action_reviews::AttemptOutcome::Executed { success: false }
                            };
                            let _ = db.complete_step(&intent, &outcome, None);
                        }
                    }
                    return AttemptResult {
                        outcome: RunOutcome::Failed,
                        steps,
                        denied,
                        error: Some(failure.message),
                        uncertain: failure.uncertain,
                        stale_lease: failure.stale_lease,
                    };
                }
            }
        }
        AttemptResult {
            outcome: RunOutcome::Completed,
            steps,
            denied,
            error: None,
            uncertain: false,
            stale_lease: false,
        }
    }

    fn audit(&self, session_dir: &Path, record: RunRecord) -> Result<RunRecord, RunAuditError> {
        match append_run_record(session_dir, &record) {
            Ok(()) => Ok(record),
            Err(source) => Err(RunAuditError { record, source }),
        }
    }
}

// ---------------------------------------------------------------------------
// Schedule registry
// ---------------------------------------------------------------------------

/// Path of the schedule registry: `<home>/schedules.json`.
pub fn schedules_path(home: &Path) -> PathBuf {
    home.join("schedules.json")
}

/// Why the schedule registry could not be read or written.
#[derive(Debug)]
pub enum ScheduleStoreError {
    Io(io::Error),
    /// The registry file is corrupt. Never silently dropped: a torn
    /// registry must not silently disarm the operator's schedules.
    Json(String),
    /// A spec failed validation; the registry was left untouched.
    Invalid(PolicyError),
}

impl std::fmt::Display for ScheduleStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScheduleStoreError::Io(e) => write!(f, "schedule registry I/O: {e}"),
            ScheduleStoreError::Json(e) => write!(f, "schedule registry corrupt: {e}"),
            ScheduleStoreError::Invalid(e) => write!(f, "invalid schedule: {e}"),
        }
    }
}

impl std::error::Error for ScheduleStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ScheduleStoreError::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// Load all schedules. A missing file is an empty registry; a corrupt
/// file is an error.
pub fn load_schedules(home: &Path) -> Result<Vec<ScheduleSpec>, ScheduleStoreError> {
    let path = schedules_path(home);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(ScheduleStoreError::Io(e)),
    };
    serde_json::from_slice(&bytes)
        .map_err(|e| ScheduleStoreError::Json(format!("{}: {e}", path.display())))
}

/// Save schedules: every spec is validated first (fail closed at
/// management time), then the write goes through an exclusive flock plus
/// atomic rename — never a torn file, even if the process dies mid-write.
pub fn save_schedules(home: &Path, specs: &[ScheduleSpec]) -> Result<(), ScheduleStoreError> {
    for spec in specs {
        spec.validate().map_err(ScheduleStoreError::Invalid)?;
    }
    let path = schedules_path(home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(ScheduleStoreError::Io)?;
    }
    let _lock = lock_exclusive(&path).map_err(ScheduleStoreError::Io)?;
    let bytes =
        serde_json::to_vec_pretty(specs).map_err(|e| ScheduleStoreError::Json(e.to_string()))?;
    // Unique temp name per process so concurrent writers can't collide.
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, &bytes).map_err(ScheduleStoreError::Io)?;
    std::fs::rename(&tmp, &path).map_err(ScheduleStoreError::Io)?;
    Ok(())
}

/// RAII exclusive flock on `<target>.lock`, released on drop (and by the
/// OS if the process dies mid-edit).
struct FileLock(#[allow(dead_code)] std::fs::File);

fn lock_exclusive(target: &Path) -> io::Result<FileLock> {
    use std::os::fd::AsRawFd;
    let lock_path = target.with_extension("lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)?;
    // `unsafe` is contained: flock on a file we just opened, no pointer.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(FileLock(file))
}

// ---------------------------------------------------------------------------
// Scheduler daemon
// ---------------------------------------------------------------------------

/// What one daemon tick did.
#[derive(Debug, Default)]
pub struct TickReport {
    /// One record per trigger fired this tick, in fire order.
    pub records: Vec<RunRecord>,
    /// Non-fatal problems: registry load failure, invalid specs, audit
    /// failures. The daemon logs these; a bad spec never stops the rest.
    pub errors: Vec<String>,
}

/// The scheduler daemon: arms specs from the on-disk registry and fires
/// due triggers through [`ScheduledRunner`]. Tick-based rather than a
/// thread, so tests drive it deterministically and the CLI host loop
/// sleeps between ticks.
///
/// `notify` is called at most once per trigger, only when the final
/// outcome — after retries are exhausted — is [`RunOutcome::Failed`],
/// [`RunOutcome::TimedOut`], or [`RunOutcome::Killed`]. That is the
/// persistent-failure notification; it replaces retrying forever.
pub struct Scheduler<C: RunClock = SystemClock> {
    runner: ScheduledRunner<C>,
    /// When each known schedule next fires (unix ms). A schedule is armed
    /// for one full interval after the daemon (re)starts — never fired
    /// immediately, so a restart can't thundering-herd the tools.
    next_due_ms: HashMap<String, u64>,
    /// SQL-lease store for cross-process single-flight. `None` keeps the
    /// historical behavior: only the in-process guard applies. Shared by
    /// `Arc<Mutex<..>>` with each run's [`LeaseFence`] so fence checks
    /// read the same database the claim wrote.
    leases: Option<Arc<Mutex<crate::schedule_leases::ScheduleLeases>>>,
}

impl<C: RunClock> Scheduler<C> {
    pub fn new(clock: C) -> Self {
        Self {
            runner: ScheduledRunner::new(clock),
            next_due_ms: HashMap::new(),
            leases: None,
        }
    }

    /// Coordinate with other scheduler workers through SQL leases: a due
    /// schedule fires only while this worker holds its lease. A crashed
    /// worker's lease expires and another worker takes over.
    pub fn with_lease_store(mut self, leases: crate::schedule_leases::ScheduleLeases) -> Self {
        self.leases = Some(Arc::new(Mutex::new(leases)));
        self
    }

    /// Attach a durable-runs database: triggers fired by this scheduler
    /// are journaled, and crashes are resumed instead of restarted.
    pub fn with_durable_runs(mut self, db: RunsDb) -> Self {
        self.runner.set_durable_runs(db);
        self
    }

    /// Explicit opt-out of fail-closed durable runs: the scheduler will
    /// fire triggers without journaling. Requires explicit operator intent.
    pub fn allow_unjournaled(mut self) -> Self {
        self.runner.set_allow_unjournaled();
        self
    }

    /// One pass: fire every due, enabled, valid spec. `session_dir_for`
    /// resolves a session id to its directory (production:
    /// [`crate::session_host::session_dir`); `make_executor` builds the
    /// tool surface for a session (production: a fresh `SessionConnectors`
    /// per trigger, put in autonomous mode by the runner).
    pub fn tick(
        &mut self,
        home: &Path,
        session_dir_for: &dyn Fn(&str) -> PathBuf,
        make_executor: &mut dyn FnMut(&str, &Path) -> Box<dyn ScheduledToolExecutor>,
        notify: &mut dyn FnMut(&ScheduleSpec, &RunRecord),
    ) -> TickReport {
        let mut report = TickReport::default();
        let now_ms = self.runner.clock.now_ms();
        let specs = match load_schedules(home) {
            Ok(specs) => specs,
            Err(error) => {
                report
                    .errors
                    .push(format!("cannot load schedule registry: {error}"));
                return report;
            }
        };
        let mut known: HashSet<String> = HashSet::new();
        for spec in &specs {
            known.insert(spec.id.clone());
            if !spec.enabled {
                self.next_due_ms.remove(&spec.id);
                continue;
            }
            if let Err(error) = spec.validate() {
                // Validated at save time; a hand-edited registry can still
                // rot. Skip loudly — never fire an invalid spec.
                report.errors.push(format!(
                    "schedule {:?} is invalid ({error}); not firing",
                    spec.id
                ));
                self.next_due_ms.remove(&spec.id);
                continue;
            }
            let due_ms = *self
                .next_due_ms
                .entry(spec.id.clone())
                .or_insert_with(|| now_ms.saturating_add(spec.interval_secs.saturating_mul(1000)));
            if now_ms < due_ms {
                continue;
            }
            // Cross-process single-flight: only the lease holder fires.
            // A lost claim is normal coordination, not an error — the
            // holder runs it. Fail-closed: a lease-database error means we
            // do not fire, never fire without a lease.
            //
            // The claim returns the fencing-token generation; the run
            // carries it as a LeaseFence, so a worker that stalls past
            // expiry cannot keep executing after another worker takes
            // over. On takeover the lapsed run's attempt log is checked
            // first (P5-1): an in-flight or ambiguous attempt escalates
            // to a human instead of re-firing.
            let session_dir = session_dir_for(&spec.session_id);
            let fence = if let Some(leases) = &self.leases {
                // A poisoned lease mutex fails closed: no claim, no fire.
                let claim = leases
                    .lock()
                    .map(|store| store.claim(&spec.id))
                    .unwrap_or_else(|_| {
                        Err(crate::schedule_leases::LeaseError::Open(
                            "lease store mutex poisoned".to_string(),
                        ))
                    });
                match claim {
                    Ok(None) => continue,
                    Ok(Some(info)) => {
                        if info.took_over.is_some() {
                            if let Some(escalation) = check_lapsed_run(&session_dir, spec, &info) {
                                let record = match self.runner.audit(&session_dir, escalation) {
                                    Ok(record) => record,
                                    Err(audit_error) => {
                                        report.errors.push(format!(
                                            "schedule {:?}: escalation audit failed: {}",
                                            spec.id, audit_error.source
                                        ));
                                        audit_error.record
                                    }
                                };
                                notify(spec, &record);
                                report.records.push(record);
                                // HOLD the lease: it is not released, so no
                                // other worker can re-fire behind the
                                // escalation. Expiry still covers a crashed
                                // holder; the next takeover re-checks.
                                continue;
                            }
                        }
                        Some(LeaseFence::new(
                            Arc::clone(leases),
                            spec.id.clone(),
                            info.generation,
                        ))
                    }
                    Err(error) => {
                        report.errors.push(format!(
                            "schedule {:?}: lease claim failed ({error}); not firing",
                            spec.id
                        ));
                        continue;
                    }
                }
            } else {
                None
            };
            let mut executor = make_executor(&spec.session_id, &session_dir);
            executor.set_lease_fence(fence);
            let record = match self
                .runner
                .run_trigger(spec, &session_dir, executor.as_mut())
            {
                Ok(record) => record,
                Err(audit_error) => {
                    report.errors.push(format!(
                        "schedule {:?}: ran but the audit append failed: {}",
                        spec.id, audit_error.source
                    ));
                    audit_error.record
                }
            };
            if matches!(
                record.outcome,
                RunOutcome::Failed
                    | RunOutcome::TimedOut
                    | RunOutcome::Killed
                    | RunOutcome::NeedsReview
            ) {
                notify(spec, &record);
            }
            report.records.push(record);
            // Release the lease on the clean path; expiry covers crashes.
            // A release error is non-fatal: the lease lapses on its own.
            if let Some(leases) = &self.leases {
                let released = leases
                    .lock()
                    .map(|store| store.release(&spec.id))
                    .unwrap_or(Ok(()));
                if let Err(error) = released {
                    report.errors.push(format!(
                        "schedule {:?}: lease release failed ({error}); \
                         lease will lapse",
                        spec.id
                    ));
                }
            }
            // The next firing is anchored to *this* firing, so a slow run
            // can't compress the interval (no catch-up storms).
            self.next_due_ms.insert(
                spec.id.clone(),
                now_ms.saturating_add(spec.interval_secs.saturating_mul(1000)),
            );
        }
        self.next_due_ms.retain(|id, _| known.contains(id));
        report
    }

    /// How long the host loop should sleep before the next tick: until the
    /// next known firing, clamped to [1 s, 60 s] so registry edits (add /
    /// pause / remove) are picked up promptly even with nothing armed.
    pub fn next_wake_in(&self) -> Duration {
        let now_ms = self.runner.clock.now_ms();
        let nearest = self
            .next_due_ms
            .values()
            .map(|due| due.saturating_sub(now_ms))
            .min()
            .unwrap_or(60_000);
        Duration::from_millis(nearest.clamp(1_000, 60_000))
    }
}

/// P5-1 takeover check: before a worker that just took over a lapsed
/// lease re-fires the schedule, inspect the lapsed run's attempt log.
///
/// Returns `Some(RunRecord)` with [`RunOutcome::NeedsReview`] when the
/// previous holder left uncertainty behind — a review with no recorded
/// attempt (the holder died between the write-ahead review and the
/// attempt write: in-flight) or an attempt with an `ambiguous` outcome —
/// that has not since been superseded by an explicit human-approved
/// replacement (a later attempt whose `replaces_attempt` names the
/// uncertain review or attempt id). A review with a `never_ran` outcome
/// is explicitly NOT uncertain: the tool provably never executed, so the
/// schedule may re-fire. Otherwise returns `None` and the schedule may
/// fire.
///
/// Only scheduled-run reviews (`actor == "scheduled:<schedule-id>"`) are
/// examined, so one schedule's uncertainty never blocks another. The
/// check is deliberately windowless: it covers every *unresolved*
/// uncertain item, which also closes the hole where an escalating worker
/// holds the lease, lapses, and the next takeover would otherwise see a
/// fresh window. Resolution is exactly the P5-1 human replacement path —
/// once the human supersedes the uncertain item, the next takeover fires
/// normally.
fn check_lapsed_run(
    session_dir: &Path,
    spec: &ScheduleSpec,
    info: &crate::schedule_leases::ClaimInfo,
) -> Option<RunRecord> {
    let actor = format!("scheduled:{}", spec.id);
    let audit_path = session_dir.join("connectors-audit.jsonl");
    let reviews_path = session_dir.join("action-reviews.jsonl");
    // Phase 1: collect every superseded id from both logs first, so a
    // replacement recorded *after* the uncertain item still resolves it
    // regardless of file order.
    let mut superseded: HashSet<String> = HashSet::new();
    for path in [&audit_path, &reviews_path] {
        if let Ok(text) = std::fs::read_to_string(path) {
            for line in text.lines() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(target) = v.get("replaces_attempt").and_then(|r| r.as_str()) {
                        superseded.insert(target.to_string());
                    }
                }
            }
        }
    }
    // Phase 2: this schedule's attempts, keyed by the review that
    // authorized them.
    let mut attempts: HashMap<String, (String, String)> = HashMap::new();
    if let Ok(text) = std::fs::read_to_string(&audit_path) {
        for line in text.lines() {
            let v: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let (Some(review_id), Some(attempt_id), Some(outcome)) = (
                v.get("review_id").and_then(|r| r.as_str()),
                v.get("attempt_id").and_then(|a| a.as_str()),
                v.get("outcome").and_then(|o| o.as_str()),
            ) else {
                continue;
            };
            attempts.insert(
                review_id.to_string(),
                (attempt_id.to_string(), outcome.to_string()),
            );
        }
    }
    // Phase 3: evaluate this schedule's reviews. Only scheduled-run
    // reviews (`actor == "scheduled:<schedule-id>"`) are examined, so
    // one schedule's uncertainty never blocks another. A review whose
    // attempt provably never ran (`never_ran` outcome) is NOT uncertain:
    // the tool definitely did not execute, so the next trigger may
    // safely re-fire the schedule.
    //
    // Two passes over the review log: the NeverRan resolutions are
    // collected first because a review line may precede its outcome line.
    let mut uncertain: Vec<String> = Vec::new();
    let mut never_ran: HashSet<String> = HashSet::new();
    let reviews_text = std::fs::read_to_string(&reviews_path).unwrap_or_default();
    for line in reviews_text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) == Some("attempt_outcome")
            && v.get("outcome").and_then(|o| o.as_str()) == Some("never_ran")
        {
            if let Some(rid) = v.get("review_id").and_then(|r| r.as_str()) {
                never_ran.insert(rid.to_string());
            }
        }
    }
    for line in reviews_text.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // Outcome records are not reviews.
        if v.get("type").and_then(|t| t.as_str()) == Some("attempt_outcome") {
            continue;
        }
        let (Some(review_id), Some(review_actor)) = (
            v.get("review_id").and_then(|r| r.as_str()),
            v.get("actor").and_then(|a| a.as_str()),
        ) else {
            continue;
        };
        if review_actor != actor || superseded.contains(review_id) || never_ran.contains(review_id)
        {
            continue;
        }
        match attempts.get(review_id) {
            None => uncertain.push(format!(
                "review {review_id} (in-flight: no attempt recorded)"
            )),
            Some((attempt_id, outcome)) => {
                if outcome == "ambiguous" && !superseded.contains(attempt_id) {
                    uncertain.push(format!("attempt {attempt_id} (ambiguous outcome)"));
                }
            }
        }
    }
    if uncertain.is_empty() {
        return None;
    }
    uncertain.sort();
    uncertain.dedup();
    let (prev_owner, prev_claimed_at_ms) = info
        .took_over
        .as_ref()
        .map(|(o, c)| (o.as_str(), *c))
        .unwrap_or(("<unknown>", 0));
    Some(RunRecord {
        schedule_id: spec.id.clone(),
        session_id: spec.session_id.clone(),
        triggered_at_ms: now_ms(),
        outcome: RunOutcome::NeedsReview,
        steps: 0,
        denied_tools: vec![],
        error: Some(format!(
            "takeover of lapsed lease (previous holder {prev_owner}, claimed {prev_claimed_at_ms}): \
             found {} uncertain attempt(s) from the lapsed run — {}; \
             NOT re-firing. Resolve each via an explicit human-approved replacement call \
             referencing the attempt (or review) id; the schedule stays held until then.",
            uncertain.len(),
            uncertain.join("; ")
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_spec() -> ScheduleSpec {
        ScheduleSpec {
            id: "nightly-triage".to_string(),
            session_id: "sess-1".to_string(),
            interval_secs: 3600,
            policy: AutonomousPolicy::default(),
            enabled: true,
            task: ScheduledTask::ToolCalls(vec![ScheduledToolCall {
                tool: "allowy.echo".to_string(),
                arguments: serde_json::json!({"text": "hi"}),
            }]),
        }
    }

    #[test]
    fn decide_maps_ask_to_no_human_present() {
        assert_eq!(
            decide_autonomous(ApprovalPolicy::Allow),
            AutonomousDecision::Allow
        );
        assert_eq!(
            decide_autonomous(ApprovalPolicy::Ask),
            AutonomousDecision::Deny(DenyReason::NoHumanPresent)
        );
        assert_eq!(
            decide_autonomous(ApprovalPolicy::Deny),
            AutonomousDecision::Deny(DenyReason::ExplicitDeny)
        );
    }

    #[test]
    fn valid_spec_passes() {
        assert_eq!(valid_spec().validate(), Ok(()));
    }

    #[test]
    fn spec_rejects_bad_ids() {
        for bad in ["", "has space", "has/slash", "has:colon", &"x".repeat(65)] {
            let mut spec = valid_spec();
            spec.id = bad.to_string();
            assert_eq!(spec.validate(), Err(PolicyError::BadId), "id {bad:?}");
        }
        let mut spec = valid_spec();
        spec.id = "ok_id-9".to_string();
        assert_eq!(spec.validate(), Ok(()));
    }

    #[test]
    fn spec_rejects_short_interval() {
        let mut spec = valid_spec();
        spec.interval_secs = 59;
        assert_eq!(spec.validate(), Err(PolicyError::IntervalTooShort));
        spec.interval_secs = 60;
        assert_eq!(spec.validate(), Ok(()));
    }

    #[test]
    fn spec_rejects_bad_bounds() {
        let mut spec = valid_spec();
        spec.policy.max_duration_secs = 0;
        assert_eq!(spec.validate(), Err(PolicyError::BadDuration));
        spec.policy.max_duration_secs = MAX_DURATION_SECS + 1;
        assert_eq!(spec.validate(), Err(PolicyError::BadDuration));

        let mut spec = valid_spec();
        spec.policy.max_steps = 0;
        assert_eq!(spec.validate(), Err(PolicyError::BadSteps));
        spec.policy.max_steps = MAX_STEPS + 1;
        assert_eq!(spec.validate(), Err(PolicyError::BadSteps));

        let mut spec = valid_spec();
        spec.policy.max_retries = MAX_RETRIES + 1;
        assert_eq!(spec.validate(), Err(PolicyError::BadRetries));

        let mut spec = valid_spec();
        spec.session_id.clear();
        assert_eq!(spec.validate(), Err(PolicyError::EmptySessionId));
    }

    #[test]
    fn spec_rejects_bad_tasks() {
        let mut spec = valid_spec();
        spec.task = ScheduledTask::ToolCalls(vec![]);
        assert_eq!(spec.validate(), Err(PolicyError::EmptyTask));

        for bad_tool in ["", "has space", "has/slash", &"x".repeat(129)] {
            let mut spec = valid_spec();
            spec.task = ScheduledTask::ToolCalls(vec![ScheduledToolCall {
                tool: bad_tool.to_string(),
                arguments: serde_json::Value::Null,
            }]);
            assert_eq!(
                spec.validate(),
                Err(PolicyError::BadToolName),
                "tool {bad_tool:?}"
            );
        }

        let mut spec = valid_spec();
        spec.task = ScheduledTask::ToolCalls(vec![ScheduledToolCall {
            tool: "ok.tool".to_string(),
            arguments: serde_json::json!("a string is not an object"),
        }]);
        assert_eq!(spec.validate(), Err(PolicyError::BadArguments));

        // Null arguments are fine (means "no arguments").
        let mut spec = valid_spec();
        spec.task = ScheduledTask::ToolCalls(vec![ScheduledToolCall {
            tool: "ok.tool".to_string(),
            arguments: serde_json::Value::Null,
        }]);
        assert_eq!(spec.validate(), Ok(()));

        // More steps than the policy allows is rejected at creation.
        let mut spec = valid_spec();
        spec.policy.max_steps = 1;
        spec.task = ScheduledTask::ToolCalls(vec![
            ScheduledToolCall {
                tool: "a.b".to_string(),
                arguments: serde_json::Value::Null,
            },
            ScheduledToolCall {
                tool: "c.d".to_string(),
                arguments: serde_json::Value::Null,
            },
        ]);
        assert_eq!(spec.validate(), Err(PolicyError::TooManySteps));
    }

    #[test]
    fn run_guard_is_single_flight() {
        let guard = RunGuard::new();
        let token = guard.try_begin("nightly").expect("first trigger runs");
        assert!(guard.is_active("nightly"));
        // Overlapping trigger skips — it must not queue behind the run.
        assert!(guard.try_begin("nightly").is_none());
        // A different schedule is unaffected.
        let other = guard.try_begin("hourly").expect("other schedule runs");
        guard.end(other);
        assert!(!guard.is_active("hourly"));
        // Releasing re-arms the schedule.
        guard.end(token);
        assert!(!guard.is_active("nightly"));
        assert!(guard.try_begin("nightly").is_some());
    }

    #[test]
    fn run_record_json_round_trips() {
        let record = RunRecord {
            schedule_id: "nightly".to_string(),
            session_id: "sess-1".to_string(),
            triggered_at_ms: 1_700_000_000_000,
            outcome: RunOutcome::Failed,
            steps: 42,
            denied_tools: vec!["asky.echo".to_string()],
            error: Some("boom".to_string()),
        };
        let json = record.to_json();
        let back: RunRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back, record);
        assert!(json.contains("\"outcome\":\"failed\""));
    }

    #[test]
    fn append_run_record_appends_lines() {
        let dir = std::env::temp_dir().join(format!("supercli-sched-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let record = RunRecord {
            schedule_id: "s".to_string(),
            session_id: "sess".to_string(),
            triggered_at_ms: 7,
            outcome: RunOutcome::SkippedOverlap,
            steps: 0,
            denied_tools: vec![],
            error: None,
        };
        append_run_record(&dir, &record).unwrap();
        append_run_record(&dir, &record).unwrap();
        let content = std::fs::read_to_string(dir.join("scheduled-runs.jsonl")).unwrap();
        assert_eq!(content.lines().count(), 2);
        for line in content.lines() {
            let back: RunRecord = serde_json::from_str(line).unwrap();
            assert_eq!(back.outcome, RunOutcome::SkippedOverlap);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- ScheduledRunner tests: fake clock + fake executor ---

    use crate::session_connectors::ToolCallFailure;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::rc::Rc;

    #[derive(Debug, Clone)]
    enum FakeBehavior {
        Ok(String),
        Deny(DenyReason),
        Fail(String),
        Uncertain(String),
    }

    #[derive(Debug, Default)]
    struct FakeClock {
        now_ms: Cell<u64>,
        sleeps: Cell<u64>,
    }

    impl FakeClock {
        fn new() -> Rc<Self> {
            Rc::new(Self {
                now_ms: Cell::new(1_000_000),
                sleeps: Cell::new(0),
            })
        }
        fn advance(&self, ms: u64) {
            self.now_ms.set(self.now_ms.get() + ms);
        }
    }

    impl RunClock for Rc<FakeClock> {
        fn now_ms(&self) -> u64 {
            self.now_ms.get()
        }
        fn sleep(&self, duration: Duration) {
            // No real waiting: the fake clock jumps forward, so timeouts
            // and backoff are exercised deterministically.
            self.sleeps.set(self.sleeps.get() + 1);
            self.advance(duration.as_millis() as u64);
        }
    }

    struct FakeExecutor {
        clock: Rc<FakeClock>,
        autonomous: bool,
        latency_ms: u64,
        behaviors: VecDeque<FakeBehavior>,
        calls: Vec<String>,
        autonomous_seen: Vec<bool>,
        calls_log: Option<Rc<RefCell<Vec<String>>>>,
        fence: Option<LeaseFence>,
    }

    impl FakeExecutor {
        fn new(clock: &Rc<FakeClock>) -> Self {
            Self {
                clock: Rc::clone(clock),
                autonomous: false,
                latency_ms: 0,
                behaviors: VecDeque::new(),
                calls: Vec::new(),
                autonomous_seen: Vec::new(),
                calls_log: None,
                fence: None,
            }
        }
        fn behavior(mut self, behavior: FakeBehavior) -> Self {
            self.behaviors.push_back(behavior);
            self
        }
    }

    impl ScheduledToolExecutor for FakeExecutor {
        fn set_autonomous(&mut self, autonomous: bool) {
            self.autonomous = autonomous;
        }
        fn is_autonomous(&self) -> bool {
            self.autonomous
        }
        fn set_actor(&mut self, _actor: String) {}
        fn set_lease_fence(&mut self, fence: Option<LeaseFence>) {
            self.fence = fence;
        }
        fn lease_fence(&self) -> Option<&LeaseFence> {
            self.fence.as_ref()
        }
        fn call_tool_detailed(
            &mut self,
            tool: &str,
            _arguments: &serde_json::Value,
        ) -> Result<String, ToolCallFailure> {
            self.calls.push(tool.to_string());
            self.autonomous_seen.push(self.autonomous);
            if let Some(log) = &self.calls_log {
                log.borrow_mut().push(tool.to_string());
            }
            self.clock.advance(self.latency_ms);
            match self
                .behaviors
                .pop_front()
                .unwrap_or(FakeBehavior::Ok(String::new()))
            {
                FakeBehavior::Ok(text) => Ok(text),
                FakeBehavior::Deny(reason) => Err(ToolCallFailure {
                    denied: Some(reason),
                    uncertain: false,
                    stale_lease: false,
                    message: format!("tool {tool:?} denied in autonomous mode"),
                }),
                FakeBehavior::Fail(message) => Err(ToolCallFailure {
                    denied: None,
                    uncertain: false,
                    stale_lease: false,
                    message,
                }),
                FakeBehavior::Uncertain(message) => Err(ToolCallFailure {
                    denied: None,
                    uncertain: true,
                    stale_lease: false,
                    message,
                }),
            }
        }
    }

    fn runner_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "supercli-sched-runner-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn read_records(dir: &Path) -> Vec<RunRecord> {
        let content = std::fs::read_to_string(dir.join("scheduled-runs.jsonl")).unwrap();
        content
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn two_call_spec() -> ScheduleSpec {
        ScheduleSpec {
            id: "nightly-triage".to_string(),
            session_id: "sess-1".to_string(),
            interval_secs: 3600,
            policy: AutonomousPolicy::default(),
            enabled: true,
            task: ScheduledTask::ToolCalls(vec![
                ScheduledToolCall {
                    tool: "t.one".to_string(),
                    arguments: serde_json::Value::Null,
                },
                ScheduledToolCall {
                    tool: "t.two".to_string(),
                    arguments: serde_json::Value::Null,
                },
            ]),
        }
    }

    #[test]
    fn runner_completes_task_in_autonomous_mode() {
        let clock = FakeClock::new();
        let runner = ScheduledRunner::new(Rc::clone(&clock)).allow_unjournaled(); // test: runner logic, not journal
        let dir = runner_test_dir("complete");
        let mut executor = FakeExecutor::new(&clock)
            .behavior(FakeBehavior::Ok("a".to_string()))
            .behavior(FakeBehavior::Ok("b".to_string()));

        let record = runner
            .run_trigger(&two_call_spec(), &dir, &mut executor)
            .expect("audit append works");
        assert_eq!(record.outcome, RunOutcome::Completed);
        assert_eq!(record.steps, 2);
        assert_eq!(record.error, None);
        assert!(record.denied_tools.is_empty());
        // Every tool call saw autonomous mode...
        assert_eq!(
            executor.calls,
            vec!["t.one".to_string(), "t.two".to_string()]
        );
        assert_eq!(executor.autonomous_seen, vec![true, true]);
        // ...and the previous flag was restored afterwards.
        assert!(!executor.is_autonomous());
        // Exactly one audit line, matching the returned record.
        let records = read_records(&dir);
        assert_eq!(records, vec![record]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runner_denies_ask_tool_and_fails_closed() {
        let clock = FakeClock::new();
        let runner = ScheduledRunner::new(Rc::clone(&clock)).allow_unjournaled(); // test: runner logic, not journal
        let dir = runner_test_dir("deny");
        let mut executor = FakeExecutor::new(&clock)
            .behavior(FakeBehavior::Ok("ok".to_string()))
            .behavior(FakeBehavior::Deny(DenyReason::NoHumanPresent))
            .behavior(FakeBehavior::Ok("must-not-run".to_string()));

        let record = runner
            .run_trigger(&two_call_spec(), &dir, &mut executor)
            .expect("audit append works");
        assert_eq!(record.outcome, RunOutcome::Failed);
        // The first call's step counts; the denied call ends the attempt.
        assert_eq!(record.steps, 1);
        assert_eq!(record.denied_tools, vec!["t.two".to_string()]);
        assert!(record.error.unwrap().contains("denied"));
        // Fail closed: the third tool never ran.
        assert_eq!(
            executor.calls,
            vec!["t.one".to_string(), "t.two".to_string()]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runner_enforces_output_cap() {
        let clock = FakeClock::new();
        let runner = ScheduledRunner::new(Rc::clone(&clock)).allow_unjournaled(); // test: runner logic, not journal
        let dir = runner_test_dir("output");
        let mut spec = two_call_spec();
        spec.policy.max_output_bytes = 10;
        // No retries: the kill must stand on its own.
        spec.policy.max_retries = 0;
        let mut executor =
            FakeExecutor::new(&clock).behavior(FakeBehavior::Ok("12345678901".to_string()));

        let record = runner
            .run_trigger(&spec, &dir, &mut executor)
            .expect("audit append works");
        assert_eq!(record.outcome, RunOutcome::Killed);
        assert_eq!(record.steps, 1);
        assert!(record.error.unwrap().contains("max_output_bytes"));
        // A killed run is never retried, even with retries configured.
        assert_eq!(executor.calls.len(), 1);
        assert_eq!(clock.sleeps.get(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runner_enforces_duration_cap() {
        let clock = FakeClock::new();
        let runner = ScheduledRunner::new(Rc::clone(&clock)).allow_unjournaled(); // test: runner logic, not journal
        let dir = runner_test_dir("duration");
        let mut spec = two_call_spec();
        spec.policy.max_duration_secs = 1;
        spec.policy.max_retries = 0;
        let mut executor = FakeExecutor::new(&clock);
        executor.latency_ms = 1500; // each call burns 1.5 fake seconds

        let record = runner
            .run_trigger(&spec, &dir, &mut executor)
            .expect("audit append works");
        assert_eq!(record.outcome, RunOutcome::TimedOut);
        assert_eq!(record.steps, 1);
        assert!(record.error.unwrap().contains("max_duration_secs"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runner_retries_failures_with_backoff_then_gives_up() {
        let clock = FakeClock::new();
        let runner = ScheduledRunner::new(Rc::clone(&clock)).allow_unjournaled(); // test: runner logic, not journal
        let dir = runner_test_dir("retries");
        let mut spec = two_call_spec();
        spec.policy.max_retries = 2;
        let mut executor = FakeExecutor::new(&clock)
            .behavior(FakeBehavior::Fail("boom 1".to_string()))
            .behavior(FakeBehavior::Fail("boom 2".to_string()))
            .behavior(FakeBehavior::Fail("boom 3".to_string()));

        let start = clock.now_ms();
        let record = runner
            .run_trigger(&spec, &dir, &mut executor)
            .expect("audit append works");
        assert_eq!(record.outcome, RunOutcome::Failed);
        assert_eq!(record.error.unwrap(), "boom 3");
        // Initial attempt + 2 retries, with the 5s/10s backoff between them.
        assert_eq!(executor.calls.len(), 3);
        assert_eq!(clock.sleeps.get(), 2);
        assert_eq!(clock.now_ms() - start, 5_000 + 10_000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runner_retry_success_counts_final_attempt() {
        let clock = FakeClock::new();
        let runner = ScheduledRunner::new(Rc::clone(&clock)).allow_unjournaled(); // test: runner logic, not journal
        let dir = runner_test_dir("retry-ok");
        let mut spec = two_call_spec();
        spec.policy.max_retries = 2;
        let mut executor = FakeExecutor::new(&clock)
            .behavior(FakeBehavior::Fail("transient".to_string()))
            .behavior(FakeBehavior::Ok("x".to_string()))
            .behavior(FakeBehavior::Ok("y".to_string()));

        let record = runner
            .run_trigger(&spec, &dir, &mut executor)
            .expect("audit append works");
        assert_eq!(record.outcome, RunOutcome::Completed);
        assert_eq!(record.steps, 2);
        assert_eq!(clock.sleeps.get(), 1);
        // One record per trigger: the retry is not a second audit line.
        assert_eq!(read_records(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runner_never_retries_uncertain_tool_failures() {
        // An uncertain failure (the call may already have executed) must
        // not be retried even when max_retries > 0: a retry could
        // double-apply a write. The attempt is terminal after one try.
        let clock = FakeClock::new();
        let runner = ScheduledRunner::new(Rc::clone(&clock)).allow_unjournaled(); // test: runner logic, not journal
        let dir = runner_test_dir("uncertain");
        let mut spec = two_call_spec();
        spec.policy.max_retries = 2;
        let mut executor = FakeExecutor::new(&clock)
            .behavior(FakeBehavior::Uncertain("may have executed".to_string()))
            .behavior(FakeBehavior::Ok("x".to_string()));

        let record = runner
            .run_trigger(&spec, &dir, &mut executor)
            .expect("audit append works");
        assert_eq!(record.outcome, RunOutcome::Failed);
        assert_eq!(record.error.unwrap(), "may have executed");
        // Exactly one attempt: no retry, no backoff sleep.
        assert_eq!(executor.calls.len(), 1);
        assert_eq!(clock.sleeps.get(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn retry_backoff_shape() {
        assert_eq!(retry_delay_for_attempt(0), Duration::from_secs(5));
        assert_eq!(retry_delay_for_attempt(1), Duration::from_secs(10));
        assert_eq!(retry_delay_for_attempt(2), Duration::from_secs(20));
        // Capped even for absurd indexes.
        assert_eq!(retry_delay_for_attempt(100), Duration::from_secs(300));
    }

    #[test]
    fn runner_skips_overlapping_trigger() {
        let clock = FakeClock::new();
        let runner = ScheduledRunner::new(Rc::clone(&clock)).allow_unjournaled(); // test: runner logic, not journal
        let dir = runner_test_dir("overlap");
        // Simulate a still-running trigger by holding its guard token.
        let token = runner.guard.try_begin("nightly-triage").unwrap();
        let mut executor = FakeExecutor::new(&clock);

        let record = runner
            .run_trigger(&two_call_spec(), &dir, &mut executor)
            .expect("audit append works");
        assert_eq!(record.outcome, RunOutcome::SkippedOverlap);
        assert_eq!(record.steps, 0);
        assert!(executor.calls.is_empty());
        assert_eq!(read_records(&dir).len(), 1);
        runner.guard.end(token);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runner_audits_invalid_and_paused_specs() {
        let clock = FakeClock::new();
        let runner = ScheduledRunner::new(Rc::clone(&clock)).allow_unjournaled(); // test: runner logic, not journal
        let dir = runner_test_dir("invalid");

        let mut bad = two_call_spec();
        bad.id = "not valid!!".to_string();
        let mut executor = FakeExecutor::new(&clock);
        let record = runner
            .run_trigger(&bad, &dir, &mut executor)
            .expect("audit append works");
        assert_eq!(record.outcome, RunOutcome::Failed);
        assert!(record.error.unwrap().contains("invalid schedule"));
        assert!(executor.calls.is_empty());

        let mut paused = two_call_spec();
        paused.enabled = false;
        let record = runner
            .run_trigger(&paused, &dir, &mut executor)
            .expect("audit append works");
        assert_eq!(record.outcome, RunOutcome::Failed);
        assert!(record.error.unwrap().contains("paused"));
        assert!(executor.calls.is_empty());
        assert_eq!(read_records(&dir).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn schedule_store_round_trips_and_refuses_bad_specs() {
        let home = runner_test_dir("store");
        assert_eq!(load_schedules(&home).unwrap(), vec![]);
        let specs = vec![two_call_spec()];
        save_schedules(&home, &specs).unwrap();
        assert_eq!(load_schedules(&home).unwrap(), specs);

        // Corrupt registry is an error, never an empty list.
        std::fs::write(schedules_path(&home), b"{not json").unwrap();
        assert!(matches!(
            load_schedules(&home),
            Err(ScheduleStoreError::Json(_))
        ));

        // Invalid specs are refused at save time; the file is untouched.
        let mut bad = two_call_spec();
        bad.interval_secs = 5;
        assert!(matches!(
            save_schedules(&home, std::slice::from_ref(&bad)),
            Err(ScheduleStoreError::Invalid(PolicyError::IntervalTooShort))
        ));
        assert!(matches!(
            load_schedules(&home),
            Err(ScheduleStoreError::Json(_))
        ));
        let _ = std::fs::remove_dir_all(&home);
    }

    // --- Scheduler daemon tests ---

    fn daemon_fixture(name: &str) -> (PathBuf, PathBuf, Rc<FakeClock>) {
        let home = runner_test_dir(name);
        let sessions = home.join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        let clock = FakeClock::new();
        (home, sessions, clock)
    }

    fn schedule_with_calls(id: &str, session: &str) -> ScheduleSpec {
        ScheduleSpec {
            id: id.to_string(),
            session_id: session.to_string(),
            interval_secs: 60,
            policy: AutonomousPolicy::default(),
            enabled: true,
            task: ScheduledTask::ToolCalls(vec![ScheduledToolCall {
                tool: "t.one".to_string(),
                arguments: serde_json::Value::Null,
            }]),
        }
    }

    #[test]
    fn scheduler_fires_due_triggers_and_notifies_on_persistent_failure() {
        let (home, sessions, clock) = daemon_fixture("daemon");
        save_schedules(&home, &[schedule_with_calls("hourly", "sess-1")]).unwrap();
        // Test: scheduler tick logic, not journal — explicit opt-out.
        let mut scheduler = Scheduler::new(Rc::clone(&clock)).allow_unjournaled();

        let queue: Rc<RefCell<VecDeque<FakeBehavior>>> = Rc::new(RefCell::new(VecDeque::new()));
        queue
            .borrow_mut()
            .push_back(FakeBehavior::Fail("persistent".to_string()));
        let calls: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let notifications: Rc<RefCell<Vec<(String, RunOutcome)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let clock2 = Rc::clone(&clock);
        let queue2 = Rc::clone(&queue);
        let calls2 = Rc::clone(&calls);
        let mut make_executor = move |_: &str, _: &Path| {
            let mut executor = FakeExecutor::new(&clock2);
            executor.behaviors = queue2.borrow_mut().drain(..).collect();
            executor.calls_log = Some(Rc::clone(&calls2));
            Box::new(executor) as Box<dyn ScheduledToolExecutor>
        };
        let notifications2 = Rc::clone(&notifications);
        let mut notify = |spec: &ScheduleSpec, record: &RunRecord| {
            notifications2
                .borrow_mut()
                .push((spec.id.clone(), record.outcome));
        };
        let session_dir_for = |id: &str| sessions.join(id);
        std::fs::create_dir_all(session_dir_for("sess-1")).unwrap();

        // Fresh daemon arms for one full interval: no immediate fire.
        let report = scheduler.tick(&home, &session_dir_for, &mut make_executor, &mut notify);
        assert!(report.records.is_empty());
        assert!(calls.borrow().is_empty());

        // After the interval passes the trigger fires; the failing tool is
        // retried zero times (default policy) and the persistent failure
        // notifies exactly once.
        clock.advance(61_000);
        let report = scheduler.tick(&home, &session_dir_for, &mut make_executor, &mut notify);
        assert_eq!(report.records.len(), 1);
        assert_eq!(report.records[0].outcome, RunOutcome::Failed);
        assert_eq!(calls.borrow().len(), 1);
        assert_eq!(
            notifications.borrow().as_slice(),
            &[("hourly".to_string(), RunOutcome::Failed)]
        );

        // A tick right after does not re-fire (anchored to the firing).
        let report = scheduler.tick(&home, &session_dir_for, &mut make_executor, &mut notify);
        assert!(report.records.is_empty());
        // ...but once another interval passes it fires again.
        clock.advance(61_000);
        queue
            .borrow_mut()
            .push_back(FakeBehavior::Ok("recovered".to_string()));
        let report = scheduler.tick(&home, &session_dir_for, &mut make_executor, &mut notify);
        assert_eq!(report.records.len(), 1);
        assert_eq!(report.records[0].outcome, RunOutcome::Completed);
        // Success does not notify.
        assert_eq!(notifications.borrow().len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn scheduler_lease_store_gives_single_flight_across_workers() {
        use crate::schedule_leases::{ScheduleLeases, DEFAULT_TENANT};
        let (home, sessions, clock) = daemon_fixture("daemon-leases");
        save_schedules(&home, &[schedule_with_calls("hourly", "sess-1")]).unwrap();
        let session_dir_for = |id: &str| sessions.join(id);
        std::fs::create_dir_all(session_dir_for("sess-1")).unwrap();

        // Two workers share one lease database: models two daemons.
        let leases_a = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        let leases_b = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        let mut sched_a = Scheduler::new(Rc::clone(&clock))
            .allow_unjournaled()
            .with_lease_store(leases_a);
        let mut sched_b = Scheduler::new(Rc::clone(&clock))
            .allow_unjournaled()
            .with_lease_store(leases_b);

        let mk_exec = || {
            let clock2 = Rc::clone(&clock);
            move |_: &str, _: &Path| {
                Box::new(FakeExecutor::new(&clock2)) as Box<dyn ScheduledToolExecutor>
            }
        };
        let mut notify = |_: &ScheduleSpec, _: &RunRecord| {};
        // First tick arms for one full interval (no immediate fire).
        let report = sched_a.tick(&home, &session_dir_for, &mut mk_exec(), &mut notify);
        assert!(report.records.is_empty());
        clock.advance(61_000);

        // Worker A fires and releases on the clean path, so worker B
        // would fire on the next due tick — but if A still held the
        // lease, B must skip.
        let report = sched_a.tick(&home, &session_dir_for, &mut mk_exec(), &mut notify);
        assert_eq!(report.records.len(), 1);

        // Simulate A crashing mid-run: claim the lease out-of-band and
        // never release it. (Lease expiry runs on the database clock,
        // not the fake clock, so this holds the lease in real time.)
        let leases_c = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        assert!(leases_c.claim("hourly").unwrap().is_some());
        clock.advance(61_000);
        let report = sched_b.tick(&home, &session_dir_for, &mut mk_exec(), &mut notify);
        assert!(
            report.records.is_empty(),
            "B must not fire while C holds the lease"
        );

        // After the lease lapses, B takes over. force_expire backdates
        // the row instead of waiting out the real 10-minute TTL; the
        // clock advance moves past B's arming point (B armed on its
        // first tick above) without affecting the database-clock lease.
        leases_c.force_expire("hourly").unwrap();
        clock.advance(61_000);
        let report = sched_b.tick(&home, &session_dir_for, &mut mk_exec(), &mut notify);
        assert_eq!(report.records.len(), 1, "B takes over after expiry");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// P5-1 takeover check: a lapsed run that left a write-ahead review
    /// with no attempt (the holder died between the review write and the
    /// attempt write — in-flight) must NOT be re-fired. The takeover
    /// escalates to a human instead.
    #[test]
    fn takeover_with_inflight_attempt_escalates_to_human() {
        use crate::schedule_leases::{ScheduleLeases, DEFAULT_TENANT};
        let (home, sessions, clock) = daemon_fixture("daemon-takeover-inflight");
        save_schedules(&home, &[schedule_with_calls("hourly", "sess-1")]).unwrap();
        let session_dir_for = |id: &str| sessions.join(id);
        let session_dir = session_dir_for("sess-1");
        std::fs::create_dir_all(&session_dir).unwrap();

        let leases = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        let mut scheduler = Scheduler::new(Rc::clone(&clock))
            .allow_unjournaled()
            .with_lease_store(leases);
        let mk_exec = || {
            let clock2 = Rc::clone(&clock);
            move |_: &str, _: &Path| {
                Box::new(FakeExecutor::new(&clock2)) as Box<dyn ScheduledToolExecutor>
            }
        };
        let notifications: Rc<RefCell<Vec<(String, RunOutcome)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let notifications2 = Rc::clone(&notifications);
        let mut notify = move |spec: &ScheduleSpec, record: &RunRecord| {
            notifications2
                .borrow_mut()
                .push((spec.id.clone(), record.outcome));
        };

        // Arm, then simulate the lapsed run: worker A claimed the lease
        // and died after the write-ahead review, before any attempt.
        let report = scheduler.tick(&home, &session_dir_for, &mut mk_exec(), &mut notify);
        assert!(report.records.is_empty());
        let review = crate::action_reviews::record_review(
            &session_dir,
            crate::action_reviews::Actor::Scheduled {
                trigger_id: "hourly".to_string(),
            },
            "allowy",
            "allowy.echo",
            "args-hash",
            crate::action_reviews::ReviewDecision::Approved,
            None,
        )
        .expect("review records");
        // A holds the lease out-of-band (same as a crashed daemon would)
        // and its lease lapses.
        let holder = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        holder.claim("hourly").unwrap().expect("A claims");
        holder.force_expire("hourly").unwrap();

        clock.advance(61_000);
        let report = scheduler.tick(&home, &session_dir_for, &mut mk_exec(), &mut notify);
        assert_eq!(report.records.len(), 1, "takeover produces one record");
        let record = &report.records[0];
        assert_eq!(record.outcome, RunOutcome::NeedsReview);
        let error = record.error.as_deref().unwrap_or("");
        assert!(
            error.contains(&review.review_id),
            "names the in-flight review: {error}"
        );
        assert!(error.contains("NOT re-firing"), "{error}");
        // The human is notified, and no tool ran.
        assert_eq!(notifications.borrow().len(), 1);
        assert_eq!(notifications.borrow()[0].1, RunOutcome::NeedsReview);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// P5-1 takeover check: an attempt the lapsed run recorded as
    /// `ambiguous` is uncertainty no machine may resolve — escalate, do
    /// not re-fire.
    #[test]
    fn takeover_with_ambiguous_attempt_escalates_to_human() {
        use crate::schedule_leases::{ScheduleLeases, DEFAULT_TENANT};
        let (home, sessions, clock) = daemon_fixture("daemon-takeover-ambiguous");
        save_schedules(&home, &[schedule_with_calls("hourly", "sess-1")]).unwrap();
        let session_dir_for = |id: &str| sessions.join(id);
        let session_dir = session_dir_for("sess-1");
        std::fs::create_dir_all(&session_dir).unwrap();

        let leases = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        let mut scheduler = Scheduler::new(Rc::clone(&clock))
            .allow_unjournaled()
            .with_lease_store(leases);
        let mk_exec = || {
            let clock2 = Rc::clone(&clock);
            move |_: &str, _: &Path| {
                Box::new(FakeExecutor::new(&clock2)) as Box<dyn ScheduledToolExecutor>
            }
        };
        let mut notify = |_: &ScheduleSpec, _: &RunRecord| {};
        let report = scheduler.tick(&home, &session_dir_for, &mut mk_exec(), &mut notify);
        assert!(report.records.is_empty());

        // Lapsed run: review + an attempt whose outcome is ambiguous.
        let review = crate::action_reviews::record_review(
            &session_dir,
            crate::action_reviews::Actor::Scheduled {
                trigger_id: "hourly".to_string(),
            },
            "allowy",
            "allowy.echo",
            "args-hash",
            crate::action_reviews::ReviewDecision::Approved,
            None,
        )
        .expect("review records");
        let audit_line = serde_json::json!({
            "attempt_id": "attempt-1",
            "review_id": review.review_id,
            "outcome": "ambiguous",
            "replaces_attempt": null,
        });
        std::fs::write(
            session_dir.join("connectors-audit.jsonl"),
            format!("{audit_line}\n"),
        )
        .unwrap();
        let holder = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        holder.claim("hourly").unwrap().expect("A claims");
        holder.force_expire("hourly").unwrap();

        clock.advance(61_000);
        let report = scheduler.tick(&home, &session_dir_for, &mut mk_exec(), &mut notify);
        assert_eq!(report.records.len(), 1);
        let record = &report.records[0];
        assert_eq!(record.outcome, RunOutcome::NeedsReview);
        assert!(
            record.error.as_deref().unwrap_or("").contains("attempt-1"),
            "names the ambiguous attempt: {:?}",
            record.error
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// S3: a lapsed run whose attempt provably never ran (`never_ran`
    /// outcome — the stale-lease fence refused before any tool-call bytes
    /// were sent) is NOT uncertainty. The takeover must re-fire the
    /// schedule normally, not escalate to `NeedsReview`.
    #[test]
    fn takeover_with_never_ran_attempt_fires_normally() {
        use crate::schedule_leases::{ScheduleLeases, DEFAULT_TENANT};
        let (home, sessions, clock) = daemon_fixture("daemon-takeover-neverran");
        save_schedules(&home, &[schedule_with_calls("hourly", "sess-1")]).unwrap();
        let session_dir_for = |id: &str| sessions.join(id);
        let session_dir = session_dir_for("sess-1");
        std::fs::create_dir_all(&session_dir).unwrap();

        let leases = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        let mut scheduler = Scheduler::new(Rc::clone(&clock))
            .allow_unjournaled()
            .with_lease_store(leases);
        let mk_exec = || {
            let clock2 = Rc::clone(&clock);
            move |_: &str, _: &Path| {
                Box::new(FakeExecutor::new(&clock2)) as Box<dyn ScheduledToolExecutor>
            }
        };
        let notifications: Rc<RefCell<Vec<(String, RunOutcome)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let notifications2 = Rc::clone(&notifications);
        let mut notify = move |spec: &ScheduleSpec, record: &RunRecord| {
            notifications2
                .borrow_mut()
                .push((spec.id.clone(), record.outcome));
        };
        let report = scheduler.tick(&home, &session_dir_for, &mut mk_exec(), &mut notify);
        assert!(report.records.is_empty());

        // Lapsed run: write-ahead review, then the fence went stale before
        // the call — durable `never_ran` outcome, no tool executed.
        let review = crate::action_reviews::record_review(
            &session_dir,
            crate::action_reviews::Actor::Scheduled {
                trigger_id: "hourly".to_string(),
            },
            "allowy",
            "allowy.echo",
            "args-hash",
            crate::action_reviews::ReviewDecision::Approved,
            None,
        )
        .expect("review records");
        crate::action_reviews::record_attempt_outcome(
            &session_dir,
            &review.review_id,
            crate::action_reviews::AttemptOutcome::NeverRan {
                reason: "stale lease before tool call".to_string(),
            },
            crate::action_reviews::Actor::Scheduled {
                trigger_id: "hourly".to_string(),
            },
        )
        .expect("never_ran outcome records");
        let holder = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        holder.claim("hourly").unwrap().expect("A claims");
        holder.force_expire("hourly").unwrap();

        clock.advance(61_000);
        let report = scheduler.tick(&home, &session_dir_for, &mut mk_exec(), &mut notify);
        // No NeedsReview: the schedule re-fires (FakeExecutor runs the
        // due trigger normally).
        for record in &report.records {
            assert_ne!(
                record.outcome,
                RunOutcome::NeedsReview,
                "never_ran must not escalate: {:?}",
                record
            );
        }
        assert!(
            !notifications
                .borrow()
                .iter()
                .any(|(_, o)| *o == RunOutcome::NeedsReview),
            "no needs-review notification for never_ran"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The P5-1 human replacement path resolves a takeover: once a human
    /// explicitly supersedes the uncertain attempt, the next takeover
    /// fires normally instead of escalating.
    #[test]
    fn takeover_after_human_replacement_fires_normally() {
        use crate::schedule_leases::{ScheduleLeases, DEFAULT_TENANT};
        let (home, sessions, clock) = daemon_fixture("daemon-takeover-resolved");
        save_schedules(&home, &[schedule_with_calls("hourly", "sess-1")]).unwrap();
        let session_dir_for = |id: &str| sessions.join(id);
        let session_dir = session_dir_for("sess-1");
        std::fs::create_dir_all(&session_dir).unwrap();

        let leases = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        let mut scheduler = Scheduler::new(Rc::clone(&clock))
            .allow_unjournaled()
            .with_lease_store(leases);
        let mk_exec = || {
            let clock2 = Rc::clone(&clock);
            move |_: &str, _: &Path| {
                Box::new(FakeExecutor::new(&clock2)) as Box<dyn ScheduledToolExecutor>
            }
        };
        let mut notify = |_: &ScheduleSpec, _: &RunRecord| {};
        let report = scheduler.tick(&home, &session_dir_for, &mut mk_exec(), &mut notify);
        assert!(report.records.is_empty());

        // Lapsed run left an ambiguous attempt...
        let review = crate::action_reviews::record_review(
            &session_dir,
            crate::action_reviews::Actor::Scheduled {
                trigger_id: "hourly".to_string(),
            },
            "allowy",
            "allowy.echo",
            "args-hash",
            crate::action_reviews::ReviewDecision::Approved,
            None,
        )
        .expect("review records");
        // ...which a human then explicitly superseded via the
        // replacement path (a later attempt naming the uncertain one).
        // Written before the uncertain attempt in the file on purpose:
        // resolution must not depend on log order.
        let audit_lines = serde_json::json!([
            {"attempt_id": "attempt-2", "review_id": review.review_id,
             "outcome": "ok", "replaces_attempt": "attempt-1"},
            {"attempt_id": "attempt-1", "review_id": review.review_id,
             "outcome": "ambiguous", "replaces_attempt": null},
        ]);
        let text: String = audit_lines
            .as_array()
            .unwrap()
            .iter()
            .map(|v| format!("{v}\n"))
            .collect();
        std::fs::write(session_dir.join("connectors-audit.jsonl"), text).unwrap();

        let holder = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        holder.claim("hourly").unwrap().expect("A claims");
        holder.force_expire("hourly").unwrap();

        clock.advance(61_000);
        let report = scheduler.tick(&home, &session_dir_for, &mut mk_exec(), &mut notify);
        assert_eq!(report.records.len(), 1, "takeover fires after resolution");
        assert_ne!(
            report.records[0].outcome,
            RunOutcome::NeedsReview,
            "resolved uncertainty does not escalate"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn scheduler_skips_paused_and_invalid_specs() {
        let (home, sessions, clock) = daemon_fixture("daemon-skip");
        let mut paused = schedule_with_calls("paused", "sess-1");
        paused.enabled = false;
        save_schedules(&home, &[paused]).unwrap();
        let mut scheduler = Scheduler::new(Rc::clone(&clock)).allow_unjournaled(); // test: scheduler logic, not journal
        let clock2 = Rc::clone(&clock);
        let mut make_executor = move |_: &str, _: &Path| {
            Box::new(FakeExecutor::new(&clock2)) as Box<dyn ScheduledToolExecutor>
        };
        let mut notify = |_: &ScheduleSpec, _: &RunRecord| panic!("must not notify");
        let session_dir_for = |id: &str| sessions.join(id);

        clock.advance(3_600_000);
        let report = scheduler.tick(&home, &session_dir_for, &mut make_executor, &mut notify);
        assert!(report.records.is_empty());
        assert!(report.errors.is_empty());

        // A hand-edited registry with an invalid spec is skipped loudly.
        let mut invalid = schedule_with_calls("broken", "sess-1");
        invalid.interval_secs = 5;
        let mut raw = serde_json::to_value(load_schedules(&home).unwrap()).unwrap();
        raw.as_array_mut()
            .unwrap()
            .push(serde_json::to_value(&invalid).unwrap());
        std::fs::write(
            schedules_path(&home),
            serde_json::to_vec_pretty(&raw).unwrap(),
        )
        .unwrap();
        let report = scheduler.tick(&home, &session_dir_for, &mut make_executor, &mut notify);
        assert!(report.records.is_empty());
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("broken"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn scheduler_next_wake_is_clamped() {
        let (_home, _sessions, clock) = daemon_fixture("daemon-wake");
        let scheduler = Scheduler::new(Rc::clone(&clock)).allow_unjournaled(); // test: scheduler logic, not journal
                                                                               // Nothing armed: wake in 60s to pick up registry edits.
        assert_eq!(scheduler.next_wake_in(), Duration::from_secs(60));
        let _ = std::fs::remove_dir_all(&_home);
    }

    #[test]
    fn durable_run_journals_scheduled_trigger_steps() {
        use crate::durable_runs::RunsDb;
        let dir = std::env::temp_dir().join(format!(
            "sched-durable-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db = RunsDb::open(&dir).unwrap();

        let clock = FakeClock::new();
        let runner = ScheduledRunner::new(Rc::clone(&clock)).with_durable_runs(db);
        let mut spec = valid_spec();
        spec.task = ScheduledTask::ToolCalls(vec![
            ScheduledToolCall {
                tool: "t1".to_string(),
                arguments: serde_json::json!({}),
            },
            ScheduledToolCall {
                tool: "t2".to_string(),
                arguments: serde_json::json!({}),
            },
        ]);
        let session_dir = dir.join("sess");
        std::fs::create_dir_all(&session_dir).unwrap();
        let mut executor = FakeExecutor::new(&clock);

        let record = runner
            .run_trigger(&spec, &session_dir, &mut executor)
            .unwrap();
        assert_eq!(record.outcome, RunOutcome::Completed);
        assert_eq!(record.steps, 2);
        // Both tools were actually called.
        assert_eq!(executor.calls, vec!["t1".to_string(), "t2".to_string()]);

        // The durable run was journaled: one run, DONE, with 4 step rows
        // (2 intents + 2 completions).
        let db2 = RunsDb::open(&dir).unwrap();
        let runs = db2.list_runs(Some("DONE")).unwrap();
        assert_eq!(runs.len(), 1, "expected one DONE run");
        let run = &runs[0];
        assert!(
            run.plan.contains("\"schedule_id\":\"nightly-triage\""),
            "plan should link the schedule: {}",
            run.plan
        );
        let plan = db2.resume_run(&run.id).unwrap();
        // All steps completed; resume would start at step 2 (no more work).
        assert_eq!(plan.completed.len(), 2);
        assert_eq!(plan.first_incomplete_step_no, Some(2));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn durable_run_resumes_after_crash() {
        use crate::durable_runs::{RunsDb, StepKind};
        let dir = std::env::temp_dir().join(format!(
            "sched-resume-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        // Simulate a crashed run: create the run, journal step 0 as
        // completed, leave step 1 as a 'started' intent with no outcome
        // (crash between intent and completion).
        let db = RunsDb::open(&dir).unwrap();
        let plan = "{\"schedule_id\":\"resume-test\",\"session_id\":\"sess-1\"}";
        let run_id = db.create_run(None, plan, "{}").unwrap();
        let h0 = crate::durable_runs::step_input_hash("t1:{}");
        db.append_step(&run_id, 0, StepKind::Tool, &h0, None, None)
            .unwrap();
        db.append_step(
            &run_id,
            0,
            StepKind::Tool,
            &h0,
            Some("out1"),
            Some(&crate::action_reviews::AttemptOutcome::Executed { success: true }),
        )
        .unwrap();
        let h1 = crate::durable_runs::step_input_hash("t2:{}");
        db.append_step(&run_id, 1, StepKind::Tool, &h1, None, None)
            .unwrap();
        // Crash: db dropped without completing step 1 or marking terminal.
        drop(db);

        // New runner invocation finds the orphan and resumes.
        let db2 = RunsDb::open(&dir).unwrap();
        let clock = FakeClock::new();
        let runner = ScheduledRunner::new(Rc::clone(&clock)).with_durable_runs(db2);
        let mut spec = valid_spec();
        spec.id = "resume-test".to_string();
        spec.task = ScheduledTask::ToolCalls(vec![
            ScheduledToolCall {
                tool: "t1".to_string(),
                arguments: serde_json::json!({}),
            },
            ScheduledToolCall {
                tool: "t2".to_string(),
                arguments: serde_json::json!({}),
            },
        ]);
        let session_dir = dir.join("sess");
        std::fs::create_dir_all(&session_dir).unwrap();
        let mut executor = FakeExecutor::new(&clock);

        let record = runner
            .run_trigger(&spec, &session_dir, &mut executor)
            .unwrap();
        assert_eq!(record.outcome, RunOutcome::Completed);
        // Step 0 was skipped (already done); only t2 executed.
        // `steps` counts skipped + executed.
        assert_eq!(record.steps, 2);
        assert_eq!(
            executor.calls,
            vec!["t2".to_string()],
            "t1 must not be re-executed after resume"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_trigger_refuses_without_journal_by_default() {
        // Fail closed: a runner without a RunsDb refuses to fire.
        let clock = FakeClock::new();
        let runner = ScheduledRunner::new(Rc::clone(&clock));
        let spec = valid_spec();
        let dir = std::env::temp_dir().join(format!(
            "sched-failclosed-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let session_dir = dir.join("sess");
        std::fs::create_dir_all(&session_dir).unwrap();
        let mut executor = FakeExecutor::new(&clock);

        let record = runner
            .run_trigger(&spec, &session_dir, &mut executor)
            .unwrap();
        assert_eq!(
            record.outcome,
            RunOutcome::Failed,
            "must refuse without journal"
        );
        assert!(
            record
                .error
                .as_ref()
                .unwrap()
                .contains("durable runs unavailable"),
            "error must explain the refusal: {:?}",
            record.error
        );
        // Nothing executed.
        assert_eq!(record.steps, 0);
        assert!(executor.calls.is_empty(), "no tool must run");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_trigger_fires_unjournaled_with_explicit_opt_out() {
        // Explicit opt-out: allow_unjournaled fires without a journal.
        let clock = FakeClock::new();
        let runner = ScheduledRunner::new(Rc::clone(&clock)).allow_unjournaled();
        let mut spec = valid_spec();
        spec.task = ScheduledTask::ToolCalls(vec![ScheduledToolCall {
            tool: "t1".to_string(),
            arguments: serde_json::json!({}),
        }]);
        let dir = std::env::temp_dir().join(format!(
            "sched-optout-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let session_dir = dir.join("sess");
        std::fs::create_dir_all(&session_dir).unwrap();
        let mut executor = FakeExecutor::new(&clock);

        let record = runner
            .run_trigger(&spec, &session_dir, &mut executor)
            .unwrap();
        assert_eq!(record.outcome, RunOutcome::Completed);
        assert_eq!(executor.calls, vec!["t1".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
