//! Typed Host → client session events (Phase 6 R4).
//!
//! Clients previously derived session state by polling and parsing terminal
//! output. This module gives the Host a structural, typed vocabulary for the
//! facts it already knows: turn lifecycle, tool-call approval flow, review
//! escalation, and lease/fence changes. Events are additive, capability-
//! advertised (`session.events.v1`), and carry a per-session monotonic
//! sequence so clients can resume without loss or duplication.
//!
//! The first slice implements turn + approval events. Tool execution,
//! ambiguity, and lease events are specified in
//! `docs/design/session-events.md` and land in later slices.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Maximum events retained per session (ring buffer).
pub const EVENT_BUFFER_CAPACITY: usize = 512;

/// Capability id advertising the event stream.
pub const SESSION_EVENTS_CAPABILITY: &str = "session.events.v1";

/// A typed Host → client session event.
///
/// The JSON wire shape uses `kind` as the discriminant plus `session_id`,
/// `seq`, and `at_ms` on every event. See `docs/design/session-events.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    TurnStarted {
        session_id: String,
        seq: u64,
        at_ms: u64,
        turn_id: String,
        trigger: String,
    },
    TurnFinished {
        session_id: String,
        seq: u64,
        at_ms: u64,
        turn_id: String,
        outcome: String,
    },
    ToolRequested {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        tool: String,
        summary: String,
    },
    ToolApproved {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        answered_by: Option<String>,
    },
    ToolDenied {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        answered_by: Option<String>,
    },
    NeedsReview {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        reason: String,
    },
    TurnCancelled {
        session_id: String,
        seq: u64,
        at_ms: u64,
        reason: String,
        ambiguous_attempts: Vec<String>,
    },
    ToolExecuted {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        success: bool,
    },
    ToolAmbiguous {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        reason: String,
    },
    ToolNeverRan {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        reason: String,
    },
    LeaseFenced {
        session_id: String,
        seq: u64,
        at_ms: u64,
        generation: u64,
        holder: String,
    },
    LeaseTakenOver {
        session_id: String,
        seq: u64,
        at_ms: u64,
        generation: u64,
        holder: String,
        previous_holder: String,
    },
}

impl SessionEvent {
    /// The wire `kind` discriminant.
    pub fn kind(&self) -> &'static str {
        match self {
            SessionEvent::TurnStarted { .. } => "turn.started",
            SessionEvent::TurnFinished { .. } => "turn.finished",
            SessionEvent::ToolRequested { .. } => "tool.requested",
            SessionEvent::ToolApproved { .. } => "tool.approved",
            SessionEvent::ToolDenied { .. } => "tool.denied",
            SessionEvent::NeedsReview { .. } => "needs_review",
            SessionEvent::TurnCancelled { .. } => "turn.cancelled",
            SessionEvent::ToolExecuted { .. } => "tool.executed",
            SessionEvent::ToolAmbiguous { .. } => "tool.ambiguous",
            SessionEvent::ToolNeverRan { .. } => "tool.never_ran",
            SessionEvent::LeaseFenced { .. } => "lease.fenced",
            SessionEvent::LeaseTakenOver { .. } => "lease.taken_over",
        }
    }

    pub fn session_id(&self) -> &str {
        match self {
            SessionEvent::TurnStarted { session_id, .. }
            | SessionEvent::TurnFinished { session_id, .. }
            | SessionEvent::ToolRequested { session_id, .. }
            | SessionEvent::ToolApproved { session_id, .. }
            | SessionEvent::ToolDenied { session_id, .. }
            | SessionEvent::NeedsReview { session_id, .. }
            | SessionEvent::TurnCancelled { session_id, .. }
            | SessionEvent::ToolExecuted { session_id, .. }
            | SessionEvent::ToolAmbiguous { session_id, .. }
            | SessionEvent::ToolNeverRan { session_id, .. }
            | SessionEvent::LeaseFenced { session_id, .. }
            | SessionEvent::LeaseTakenOver { session_id, .. } => session_id,
        }
    }

    pub fn seq(&self) -> u64 {
        match self {
            SessionEvent::TurnStarted { seq, .. }
            | SessionEvent::TurnFinished { seq, .. }
            | SessionEvent::ToolRequested { seq, .. }
            | SessionEvent::ToolApproved { seq, .. }
            | SessionEvent::ToolDenied { seq, .. }
            | SessionEvent::NeedsReview { seq, .. }
            | SessionEvent::TurnCancelled { seq, .. }
            | SessionEvent::ToolExecuted { seq, .. }
            | SessionEvent::ToolAmbiguous { seq, .. }
            | SessionEvent::ToolNeverRan { seq, .. }
            | SessionEvent::LeaseFenced { seq, .. }
            | SessionEvent::LeaseTakenOver { seq, .. } => *seq,
        }
    }

    /// Serialize to the wire JSON object.
    pub fn to_json(&self) -> serde_json::Value {
        let at_ms = match self {
            SessionEvent::TurnStarted { at_ms, .. }
            | SessionEvent::TurnFinished { at_ms, .. }
            | SessionEvent::ToolRequested { at_ms, .. }
            | SessionEvent::ToolApproved { at_ms, .. }
            | SessionEvent::ToolDenied { at_ms, .. }
            | SessionEvent::NeedsReview { at_ms, .. }
            | SessionEvent::TurnCancelled { at_ms, .. }
            | SessionEvent::ToolExecuted { at_ms, .. }
            | SessionEvent::ToolAmbiguous { at_ms, .. }
            | SessionEvent::ToolNeverRan { at_ms, .. }
            | SessionEvent::LeaseFenced { at_ms, .. }
            | SessionEvent::LeaseTakenOver { at_ms, .. } => *at_ms,
        };
        let mut obj = serde_json::json!({
            "kind": self.kind(),
            "session_id": self.session_id(),
            "seq": self.seq(),
            "at_ms": at_ms,
        });
        let map = obj.as_object_mut().expect("json object");
        match self {
            SessionEvent::TurnStarted {
                turn_id, trigger, ..
            } => {
                map.insert("turn_id".into(), turn_id.clone().into());
                map.insert("trigger".into(), trigger.clone().into());
            }
            SessionEvent::TurnFinished {
                turn_id, outcome, ..
            } => {
                map.insert("turn_id".into(), turn_id.clone().into());
                map.insert("outcome".into(), outcome.clone().into());
            }
            SessionEvent::ToolRequested {
                review_id,
                tool,
                summary,
                ..
            } => {
                map.insert("review_id".into(), review_id.clone().into());
                map.insert("tool".into(), tool.clone().into());
                map.insert("summary".into(), summary.clone().into());
            }
            SessionEvent::ToolApproved {
                review_id,
                answered_by,
                ..
            }
            | SessionEvent::ToolDenied {
                review_id,
                answered_by,
                ..
            } => {
                map.insert("review_id".into(), review_id.clone().into());
                if let Some(by) = answered_by {
                    map.insert("answered_by".into(), by.clone().into());
                }
            }
            SessionEvent::NeedsReview {
                review_id, reason, ..
            } => {
                map.insert("review_id".into(), review_id.clone().into());
                map.insert("reason".into(), reason.clone().into());
            }
            SessionEvent::TurnCancelled {
                reason,
                ambiguous_attempts,
                ..
            } => {
                map.insert("reason".into(), reason.clone().into());
                map.insert(
                    "ambiguous_attempts".into(),
                    ambiguous_attempts
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect::<Vec<_>>()
                        .into(),
                );
            }
            SessionEvent::ToolExecuted {
                review_id, success, ..
            } => {
                map.insert("review_id".into(), review_id.clone().into());
                map.insert(
                    "exit".into(),
                    (if *success { "ok" } else { "failed" }).into(),
                );
            }
            SessionEvent::ToolAmbiguous {
                review_id, reason, ..
            } => {
                map.insert("review_id".into(), review_id.clone().into());
                map.insert("reason".into(), reason.clone().into());
            }
            SessionEvent::ToolNeverRan {
                review_id, reason, ..
            } => {
                map.insert("review_id".into(), review_id.clone().into());
                map.insert("reason".into(), reason.clone().into());
            }
            SessionEvent::LeaseFenced {
                generation, holder, ..
            } => {
                map.insert("generation".into(), (*generation).into());
                map.insert("holder".into(), holder.clone().into());
            }
            SessionEvent::LeaseTakenOver {
                generation,
                holder,
                previous_holder,
                ..
            } => {
                map.insert("generation".into(), (*generation).into());
                map.insert("holder".into(), holder.clone().into());
                map.insert("previous_holder".into(), previous_holder.clone().into());
            }
        }
        obj
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Per-session ring buffer with a monotonic sequence counter.
#[derive(Debug)]
struct SessionLog {
    next_seq: u64,
    events: VecDeque<SessionEvent>,
}

impl SessionLog {
    fn new() -> Self {
        SessionLog {
            next_seq: 1,
            events: VecDeque::with_capacity(EVENT_BUFFER_CAPACITY),
        }
    }

    fn push(&mut self, build: impl FnOnce(u64, u64) -> SessionEvent) -> SessionEvent {
        let seq = self.next_seq;
        self.next_seq += 1;
        let event = build(seq, now_ms());
        if self.events.len() >= EVENT_BUFFER_CAPACITY {
            self.events.pop_front();
        }
        self.events.push_back(event.clone());
        event
    }

    /// Events with `seq > after`, oldest first. Returns `(events, resync)`
    /// where `resync` is true when the cursor predates the buffer.
    fn since(&self, after: u64, limit: usize) -> (Vec<SessionEvent>, bool) {
        let mut out = Vec::new();
        let mut resync = false;
        for event in &self.events {
            if event.seq() <= after {
                continue;
            }
            // If the first buffered event is newer than after+1, we dropped
            // events the client never saw.
            if out.is_empty() && !resync {
                if let Some(first) = self.events.front() {
                    if after < first.seq().saturating_sub(1) {
                        resync = true;
                    }
                }
            }
            out.push(event.clone());
            if out.len() >= limit {
                break;
            }
        }
        (out, resync)
    }
}

/// Shared event bus. Clone is cheap (`Arc<Mutex<…>>`); all mutation goes
/// through the single mutex so sequence assignment is atomic.
///
/// Approval events (`tool.approved` / `tool.denied`) are emitted only
/// after the Phase 5 write-ahead review record is durably written: the
/// emit path verifies the review id is present in the session's durable
/// `action-reviews.jsonl` before buffering, so the event stream can
/// never report an approval the audit log doesn't have. (`tool.requested`
/// is intentionally ungated: a request reports no decision, and at
/// request time no review exists yet.)
#[derive(Debug, Clone, Default)]
pub struct EventBus {
    inner: Arc<Inner>,
    /// Cross-process outcome reconciliation (F1): per-session count of
    /// `action-reviews.jsonl` lines already scanned for attempt-outcome
    /// records. Tool calls execute in other processes (the scheduled
    /// daemon, the MCP server), which record terminal outcomes durably but
    /// cannot reach this in-memory bus — so the bus reconciles new outcome
    /// records from the durable log (the cross-process authority) into
    /// `tool.executed` / `tool.ambiguous` events on demand.
    outcome_watermarks: Arc<Mutex<HashMap<String, usize>>>,
    /// `(session_id, review_id)` pairs whose outcome events were already
    /// emitted by an in-process path (e.g. the turn-cancel route), so
    /// reconciliation never re-announces them.
    announced_outcomes: Arc<Mutex<std::collections::HashSet<(String, String)>>>,
}

/// The mutex + condvar pair for the session logs. The condvar fires on
/// every emit so long-poll waiters (`poll_wait`) wake immediately when a
/// new in-process event lands. Cross-process outcomes (written durably by
/// the MCP server / scheduled daemon) have no in-memory signal; the
/// long-poll loop re-runs `reconcile_outcomes` on short slices instead.
#[derive(Debug, Default)]
struct Inner {
    mu: Mutex<HashMap<String, SessionLog>>,
    cv: Condvar,
}

/// True when `review_id` is present in the session's durable
/// action-review log.
///
/// The log is append-only and every append is fsync'd before
/// `record_review` returns, so presence means the review survived a
/// crash. The MCP server that writes the review and the Host process
/// that serves the event stream are different processes, so an
/// in-memory handshake cannot enforce the ordering — the durable log
/// itself is the authority.
fn review_log_has_entry(session_id: &str, review_id: &str) -> bool {
    let path = supercli_core::session_host::session_dir(session_id)
        .join(supercli_core::action_reviews::REVIEWS_FILE);
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return false;
    };
    // review_id is a UUID v4 (hex + dashes): the quoted pair is unambiguous.
    let needle = format!("\"review_id\":\"{review_id}\"");
    contents.lines().any(|line| line.contains(&needle))
}

impl EventBus {
    pub fn new() -> Self {
        EventBus {
            inner: Arc::new(Inner::default()),
            outcome_watermarks: Arc::new(Mutex::new(HashMap::new())),
            announced_outcomes: Arc::new(Mutex::new(std::collections::HashSet::new())),
        }
    }

    /// R4: metrics for the /metrics endpoint.
    /// Returns (session_count, total_ring_buffer_depth).
    pub fn metrics(&self) -> (usize, usize) {
        let Ok(guard) = self.inner.mu.lock() else {
            return (0, 0);
        };
        let sessions = guard.len();
        let depth: usize = guard.values().map(|log| log.events.len()).sum();
        (sessions, depth)
    }

    fn emit(&self, session_id: &str, build: impl FnOnce(u64, u64) -> SessionEvent) {
        let Ok(mut guard) = self.inner.mu.lock() else {
            return;
        };
        let log = guard
            .entry(session_id.to_owned())
            .or_insert_with(SessionLog::new);
        log.push(build);
        // Wake long-poll waiters: a new event is available.
        self.inner.cv.notify_all();
    }

    pub fn emit_turn_started(&self, session_id: &str, turn_id: &str, trigger: &str) {
        let turn_id = turn_id.to_owned();
        let trigger = trigger.to_owned();
        self.emit(session_id, |seq, at_ms| SessionEvent::TurnStarted {
            session_id: session_id.to_owned(),
            seq,
            at_ms,
            turn_id,
            trigger,
        });
    }

    pub fn emit_turn_finished(&self, session_id: &str, turn_id: &str, outcome: &str) {
        let turn_id = turn_id.to_owned();
        let outcome = outcome.to_owned();
        self.emit(session_id, |seq, at_ms| SessionEvent::TurnFinished {
            session_id: session_id.to_owned(),
            seq,
            at_ms,
            turn_id,
            outcome,
        });
    }

    pub fn emit_tool_requested(
        &self,
        session_id: &str,
        review_id: &str,
        tool: &str,
        summary: &str,
    ) {
        let review_id = review_id.to_owned();
        let tool = tool.to_owned();
        let summary = summary.to_owned();
        self.emit(session_id, |seq, at_ms| SessionEvent::ToolRequested {
            session_id: session_id.to_owned(),
            seq,
            at_ms,
            review_id,
            tool,
            summary,
        });
    }

    /// Buffer a `tool.approved` / `tool.denied` event for an answered
    /// approval — but only when the write-ahead review record is already
    /// durable. An answer for a review id absent from the session's
    /// `action-reviews.jsonl` is dropped instead of buffered, so the
    /// stream can never report an approval the audit log doesn't have.
    pub fn emit_tool_answered(
        &self,
        session_id: &str,
        review_id: &str,
        approved: bool,
        answered_by: Option<&str>,
    ) {
        if !review_log_has_entry(session_id, review_id) {
            return;
        }
        let review_id = review_id.to_owned();
        let answered_by = answered_by.map(str::to_owned);
        self.emit(session_id, move |seq, at_ms| {
            if approved {
                SessionEvent::ToolApproved {
                    session_id: session_id.to_owned(),
                    seq,
                    at_ms,
                    review_id,
                    answered_by,
                }
            } else {
                SessionEvent::ToolDenied {
                    session_id: session_id.to_owned(),
                    seq,
                    at_ms,
                    review_id,
                    answered_by,
                }
            }
        });
    }

    pub fn emit_needs_review(&self, session_id: &str, review_id: &str, reason: &str) {
        let review_id = review_id.to_owned();
        let reason = reason.to_owned();
        self.emit(session_id, |seq, at_ms| SessionEvent::NeedsReview {
            session_id: session_id.to_owned(),
            seq,
            at_ms,
            review_id,
            reason,
        });
    }

    /// Buffer a `turn.cancelled` event. The cancel itself is a Host-side
    /// fact (the verb ran); `ambiguous_attempts` lists the review ids that
    /// were marked ambiguous by this cancel, if any.
    pub fn emit_turn_cancelled(
        &self,
        session_id: &str,
        reason: &str,
        ambiguous_attempts: Vec<String>,
    ) {
        let reason = reason.to_owned();
        self.emit(session_id, |seq, at_ms| SessionEvent::TurnCancelled {
            session_id: session_id.to_owned(),
            seq,
            at_ms,
            reason,
            ambiguous_attempts,
        });
    }

    /// True when the session's durable review log holds an attempt-outcome
    /// entry for `review_id`. `tool.executed` / `tool.ambiguous` are
    /// derived from the durable Phase 5 records and must never get ahead
    /// of them, so emission is gated on this check.
    fn outcome_log_has_entry(session_id: &str, review_id: &str) -> bool {
        let path = supercli_core::session_host::session_dir(session_id)
            .join(supercli_core::action_reviews::REVIEWS_FILE);
        let Ok(contents) = std::fs::read_to_string(&path) else {
            return false;
        };
        // review_id is a UUID v4: the quoted pair is unambiguous, and the
        // outcome entry carries "type":"attempt_outcome".
        let needle = format!("\"review_id\":\"{review_id}\"");
        contents
            .lines()
            .any(|line| line.contains(&needle) && line.contains("\"type\":\"attempt_outcome\""))
    }

    /// Remember that this outcome's events were already emitted in-process,
    /// so cross-process reconciliation skips it. Called by the direct emit
    /// paths after they buffer their events.
    fn mark_outcome_announced(&self, session_id: &str, review_id: &str) {
        if let Ok(mut announced) = self.announced_outcomes.lock() {
            announced.insert((session_id.to_owned(), review_id.to_owned()));
        }
    }

    /// Buffer a `tool.executed` event — but only when the attempt outcome
    /// is already durably recorded. The stream can never report an
    /// execution the audit log doesn't have.
    pub fn emit_tool_executed(&self, session_id: &str, review_id: &str, success: bool) {
        if !Self::outcome_log_has_entry(session_id, review_id) {
            return;
        }
        let review_id = review_id.to_owned();
        self.emit(session_id, |seq, at_ms| SessionEvent::ToolExecuted {
            session_id: session_id.to_owned(),
            seq,
            at_ms,
            review_id: review_id.clone(),
            success,
        });
        self.mark_outcome_announced(session_id, &review_id);
    }

    /// Buffer a `tool.ambiguous` event — gated on the durable outcome
    /// record exactly like `tool.executed`.
    pub fn emit_tool_ambiguous(&self, session_id: &str, review_id: &str, reason: &str) {
        if !Self::outcome_log_has_entry(session_id, review_id) {
            return;
        }
        let review_id = review_id.to_owned();
        let reason = reason.to_owned();
        self.emit(session_id, |seq, at_ms| SessionEvent::ToolAmbiguous {
            session_id: session_id.to_owned(),
            seq,
            at_ms,
            review_id: review_id.clone(),
            reason: reason.clone(),
        });
        self.mark_outcome_announced(session_id, &review_id);
    }

    /// Buffer a `tool.never_ran` event — gated on the durable outcome
    /// record exactly like `tool.executed`. Unlike `tool.ambiguous` this
    /// carries no `needs_review`: the tool provably never ran, so there
    /// is nothing uncertain for a human to resolve.
    pub fn emit_tool_never_ran(&self, session_id: &str, review_id: &str, reason: &str) {
        if !Self::outcome_log_has_entry(session_id, review_id) {
            return;
        }
        let review_id = review_id.to_owned();
        let reason = reason.to_owned();
        self.emit(session_id, |seq, at_ms| SessionEvent::ToolNeverRan {
            session_id: session_id.to_owned(),
            seq,
            at_ms,
            review_id: review_id.clone(),
            reason: reason.clone(),
        });
        self.mark_outcome_announced(session_id, &review_id);
    }

    /// Reconcile attempt-outcome records written by other processes into
    /// session events (F1).
    ///
    /// Tool calls execute outside this process (the scheduled daemon, the
    /// MCP server): they record terminal outcomes durably in the session's
    /// `action-reviews.jsonl` but cannot touch this in-memory bus. This
    /// scans the log past the per-session watermark and buffers one
    /// `tool.executed` (or `tool.ambiguous` + `needs_review`, or
    /// `tool.never_ran`) event per new outcome record. Outcomes already announced in-process (e.g. by the
    /// turn-cancel route) are skipped via the announced set.
    ///
    /// The watermark advances only over successfully parsed lines: a torn
    /// tail line (a concurrent writer mid-append) is retried on the next
    /// call, and the announced set makes re-scans idempotent. At-least-once
    /// across a Host restart is accepted — clients dedupe on `review_id`,
    /// which every one of these events carries.
    pub fn reconcile_outcomes(&self, session_id: &str) {
        let watermark = self
            .outcome_watermarks
            .lock()
            .map(|w| w.get(session_id).copied().unwrap_or(0))
            .unwrap_or(0);
        let path = supercli_core::session_host::session_dir(session_id)
            .join(supercli_core::action_reviews::REVIEWS_FILE);
        let Ok(contents) = std::fs::read_to_string(&path) else {
            return;
        };
        let lines: Vec<&str> = contents.lines().collect();
        // The log is append-only; a shorter file than the watermark means
        // the world changed under us — rescan from the start (the announced
        // set keeps it duplicate-free).
        let start = if lines.len() < watermark {
            0
        } else {
            watermark
        };
        let mut scanned = start;
        for line in &lines[start..] {
            let line = line.trim();
            if line.is_empty() {
                scanned += 1;
                continue;
            }
            // Fast path: only outcome records matter here.
            if !line.contains("\"type\":\"attempt_outcome\"") {
                scanned += 1;
                continue;
            }
            let parsed: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => break, // torn tail line: retry next poll.
            };
            scanned += 1;
            let review_id = parsed
                .get("review_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            if review_id.is_empty() {
                continue;
            }
            let already = self
                .announced_outcomes
                .lock()
                .map(|a| a.contains(&(session_id.to_owned(), review_id.to_owned())))
                .unwrap_or(false);
            if already {
                continue;
            }
            match parsed.get("outcome").and_then(serde_json::Value::as_str) {
                Some("executed") => {
                    let success = parsed
                        .get("success")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    // The record is in the log we just read: the durability
                    // gate inside emit_tool_executed passes by construction.
                    self.emit_tool_executed(session_id, review_id, success);
                }
                Some("ambiguous") => {
                    let reason = parsed
                        .get("reason")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("attempt outcome recorded as ambiguous");
                    self.emit_tool_ambiguous(session_id, review_id, reason);
                    // An ambiguous attempt's external effect is unverified
                    // no matter which process recorded it: escalate for
                    // human review, mirroring the turn-cancel path.
                    self.emit_needs_review(
                        session_id,
                        review_id,
                        &format!("ambiguous outcome recorded: {reason}"),
                    );
                }
                Some("never_ran") => {
                    let reason = parsed
                        .get("reason")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("attempt outcome recorded as never_ran");
                    // No needs_review: the tool provably never ran, so
                    // there is nothing uncertain to escalate.
                    self.emit_tool_never_ran(session_id, review_id, reason);
                }
                _ => {}
            }
        }
        if let Ok(mut w) = self.outcome_watermarks.lock() {
            w.insert(session_id.to_owned(), scanned);
        }
    }

    /// Buffer a `lease.fenced` event: the fence manager fenced the session
    /// at `generation` for `holder`. The monotonic generation (not wall
    /// clock) is what clients use to decide whether they hold the fence.
    pub fn emit_lease_fenced(&self, session_id: &str, generation: u64, holder: &str) {
        let holder = holder.to_owned();
        self.emit(session_id, |seq, at_ms| SessionEvent::LeaseFenced {
            session_id: session_id.to_owned(),
            seq,
            at_ms,
            generation,
            holder,
        });
    }

    /// Buffer a `lease.taken_over` event: `holder` took the lease at
    /// `generation` from `previous_holder`.
    pub fn emit_lease_taken_over(
        &self,
        session_id: &str,
        generation: u64,
        holder: &str,
        previous_holder: &str,
    ) {
        let holder = holder.to_owned();
        let previous_holder = previous_holder.to_owned();
        self.emit(session_id, |seq, at_ms| SessionEvent::LeaseTakenOver {
            session_id: session_id.to_owned(),
            seq,
            at_ms,
            generation,
            holder,
            previous_holder,
        });
    }

    /// Poll the buffer: events with `seq > after_seq`, oldest first.
    pub fn poll(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: usize,
    ) -> (Vec<SessionEvent>, u64, bool) {
        let Ok(guard) = self.inner.mu.lock() else {
            return (Vec::new(), after_seq, false);
        };
        let Some(log) = guard.get(session_id) else {
            return (Vec::new(), after_seq, false);
        };
        let (events, resync) = log.since(after_seq, limit);
        let next_seq = events.last().map(|e| e.seq()).unwrap_or(after_seq);
        (events, next_seq, resync)
    }

    /// Long-poll: block until an event with `seq > after_seq` is available
    /// or `timeout` elapses. Returns `(events, next_seq, resync)` like
    /// `poll`; on timeout `events` is empty and `next_seq == after_seq`
    /// (the client retries with the same cursor).
    ///
    /// The condvar wakes on every in-process emit. Cross-process outcomes
    /// (durable log writes by the MCP server / scheduled daemon) have no
    /// in-memory signal, so callers should re-run `reconcile_outcomes`
    /// between slices — or use the `wait_slice` pattern in `handle_events`.
    pub fn poll_wait(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: usize,
        timeout: Duration,
    ) -> (Vec<SessionEvent>, u64, bool) {
        let deadline = Instant::now() + timeout;
        let Ok(mut guard) = self.inner.mu.lock() else {
            return (Vec::new(), after_seq, false);
        };
        loop {
            if let Some(log) = guard.get(session_id) {
                let (events, resync) = log.since(after_seq, limit);
                if !events.is_empty() {
                    let next_seq = events.last().map(|e| e.seq()).unwrap_or(after_seq);
                    return (events, next_seq, resync);
                }
            }
            let now = Instant::now();
            if now >= deadline {
                return (Vec::new(), after_seq, false);
            }
            guard = match self.inner.cv.wait_timeout(guard, deadline - now) {
                Ok((g, _)) => g,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }

    /// Highest assigned sequence for a session (0 when no events).
    pub fn latest_seq(&self, session_id: &str) -> u64 {
        let Ok(guard) = self.inner.mu.lock() else {
            return 0;
        };
        guard
            .get(session_id)
            .map(|log| log.next_seq.saturating_sub(1))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seq_is_monotonic_per_session() {
        let bus = EventBus::new();
        bus.emit_turn_started("s1", "t1", "user");
        bus.emit_turn_finished("s1", "t1", "completed");
        bus.emit_turn_started("s2", "t9", "user");
        let (events, next, _) = bus.poll("s1", 0, 128);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].seq(), 1);
        assert_eq!(events[1].seq(), 2);
        assert_eq!(next, 2);
        // s2 has its own sequence space.
        let (other, _, _) = bus.poll("s2", 0, 128);
        assert_eq!(other.len(), 1);
        assert_eq!(other[0].seq(), 1);
    }

    #[test]
    fn poll_respects_cursor_and_limit() {
        let bus = EventBus::new();
        for i in 0..5 {
            bus.emit_turn_started("s1", &format!("t{i}"), "user");
        }
        let (events, next, _) = bus.poll("s1", 2, 2);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].seq(), 3);
        assert_eq!(next, 4);
        let (rest, next2, _) = bus.poll("s1", next, 128);
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].seq(), 5);
        assert_eq!(next2, 5);
    }

    #[test]
    fn poll_wait_returns_immediately_when_events_available() {
        let bus = EventBus::new();
        bus.emit_turn_started("s1", "t1", "user");
        let t0 = Instant::now();
        let (events, next, _) = bus.poll_wait("s1", 0, 128, Duration::from_secs(5));
        assert!(t0.elapsed() < Duration::from_secs(2));
        assert_eq!(events.len(), 1);
        assert_eq!(next, 1);
    }

    #[test]
    fn poll_wait_times_out_with_empty_events() {
        let bus = EventBus::new();
        let t0 = Instant::now();
        let (events, next, _) = bus.poll_wait("s1", 0, 128, Duration::from_millis(200));
        let elapsed = t0.elapsed();
        assert!(events.is_empty());
        assert_eq!(next, 0);
        assert!(elapsed >= Duration::from_millis(150), "elapsed={elapsed:?}");
        assert!(elapsed < Duration::from_secs(5), "elapsed={elapsed:?}");
    }

    #[test]
    fn poll_wait_wakes_on_emit() {
        let bus = EventBus::new();
        let bus2 = bus.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            bus2.emit_turn_started("s1", "t1", "user");
        });
        let t0 = Instant::now();
        let (events, next, _) = bus.poll_wait("s1", 0, 128, Duration::from_secs(10));
        let elapsed = t0.elapsed();
        handle.join().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(next, 1);
        // Woke on the emit, well before the 10s timeout.
        assert!(elapsed < Duration::from_secs(5), "elapsed={elapsed:?}");
    }

    #[test]
    fn wire_shape_has_kind_session_seq() {
        let bus = EventBus::new();
        bus.emit_tool_requested("s1", "r-1", "bash.exec", "cargo test");
        let (events, _, _) = bus.poll("s1", 0, 128);
        let json = events[0].to_json();
        assert_eq!(json["kind"], "tool.requested");
        assert_eq!(json["session_id"], "s1");
        assert_eq!(json["seq"], 1);
        assert_eq!(json["review_id"], "r-1");
        assert!(json.get("at_ms").is_some());
    }

    /// Run `f` with SUPERCLI_HOME pointed at a fresh temp dir, serialized
    /// on the shared home lock so no two SUPERCLI_HOME-mutating tests
    /// observe each other's home.
    fn with_temp_home(tag: &str, f: impl FnOnce()) {
        let _guard = crate::approvals::APP_STATE_LOCK.lock().unwrap();
        let dir =
            std::env::temp_dir().join(format!("supercli-events-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let prev = std::env::var_os("SUPERCLI_HOME");
        std::env::set_var("SUPERCLI_HOME", &dir);
        f();
        match &prev {
            Some(p) => std::env::set_var("SUPERCLI_HOME", p),
            None => std::env::remove_var("SUPERCLI_HOME"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Durably record a review in the temp home's session dir, returning
    /// its review id. Uses the real `record_review` (append + fsync).
    fn write_review(
        session_id: &str,
        decision: supercli_core::action_reviews::ReviewDecision,
    ) -> String {
        let dir = supercli_core::session_host::session_dir(session_id);
        std::fs::create_dir_all(&dir).unwrap();
        supercli_core::action_reviews::record_review(
            &dir,
            supercli_core::action_reviews::Actor::PolicyAllow,
            "shell",
            "bash.exec",
            "args-hash",
            decision,
            None,
        )
        .unwrap()
        .review_id
    }

    #[test]
    fn approval_answer_emits_approved_or_denied() {
        with_temp_home("answered", || {
            let bus = EventBus::new();
            let approved_id =
                write_review("s1", supercli_core::action_reviews::ReviewDecision::Approved);
            let denied_id = write_review("s1", supercli_core::action_reviews::ReviewDecision::Denied);
            bus.emit_tool_answered("s1", &approved_id, true, Some("device:phone-1"));
            bus.emit_tool_answered("s1", &denied_id, false, None);
            let (events, _, _) = bus.poll("s1", 0, 128);
            assert_eq!(events.len(), 2);
            assert_eq!(events[0].kind(), "tool.approved");
            assert_eq!(events[1].kind(), "tool.denied");
            assert_eq!(events[0].to_json()["answered_by"], "device:phone-1");
        });
    }

    #[test]
    fn approval_answer_without_durable_review_emits_nothing() {
        with_temp_home("gated", || {
            let bus = EventBus::new();
            // No review ever written: the answer must not reach the
            // stream, so clients can never see an approval the audit
            // log doesn't have.
            bus.emit_tool_answered("s-no-review", "r-missing", true, None);
            let (events, _, _) = bus.poll("s-no-review", 0, 128);
            assert!(events.is_empty());

            // A review id from another session's log doesn't count either.
            let other_id = write_review(
                "s-other",
                supercli_core::action_reviews::ReviewDecision::Approved,
            );
            bus.emit_tool_answered("s-no-review", &other_id, true, None);
            let (events, _, _) = bus.poll("s-no-review", 0, 128);
            assert!(events.is_empty());
        });
    }

    #[test]
    fn turn_cancelled_always_emits() {
        with_temp_home("cancelled", || {
            let bus = EventBus::new();
            // Idle cancel: no reviews at all, still emits turn.cancelled.
            bus.emit_turn_cancelled("s-idle", "user pressed stop", vec![]);
            let (events, _, _) = bus.poll("s-idle", 0, 128);
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].kind(), "turn.cancelled");
            let body = events[0].to_json();
            assert_eq!(body["reason"], serde_json::json!("user pressed stop"));
            assert_eq!(body["ambiguous_attempts"], serde_json::json!([]));

            // Mid-tool cancel carries the ambiguous review ids.
            bus.emit_turn_cancelled(
                "s-busy",
                "cancel",
                vec!["r-1".to_string(), "r-2".to_string()],
            );
            let (events, _, _) = bus.poll("s-busy", 0, 128);
            assert_eq!(events[0].kind(), "turn.cancelled");
            assert_eq!(
                events[0].to_json()["ambiguous_attempts"],
                serde_json::json!(["r-1", "r-2"])
            );
        });
    }

    #[test]
    fn tool_outcome_events_gate_on_durable_outcome() {
        use supercli_core::action_reviews::{record_attempt_outcome, Actor, AttemptOutcome};
        with_temp_home("outcome-gated", || {
            let bus = EventBus::new();
            let review_id = write_review(
                "s-outcome",
                supercli_core::action_reviews::ReviewDecision::Approved,
            );
            // Review exists but no outcome recorded: nothing emits.
            bus.emit_tool_executed("s-outcome", &review_id, true);
            bus.emit_tool_ambiguous("s-outcome", &review_id, "x");
            let (events, _, _) = bus.poll("s-outcome", 0, 128);
            assert!(events.is_empty());

            // Record the outcome durably, then the events flow.
            let dir = supercli_core::session_host::session_dir("s-outcome");
            record_attempt_outcome(
                &dir,
                &review_id,
                AttemptOutcome::Ambiguous {
                    reason: "cancelled mid-flight".into(),
                },
                Actor::PolicyAllow,
            )
            .expect("outcome");
            bus.emit_tool_ambiguous("s-outcome", &review_id, "cancelled mid-flight");
            let (events, _, _) = bus.poll("s-outcome", 0, 128);
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].kind(), "tool.ambiguous");
            assert_eq!(
                events[0].to_json()["review_id"],
                serde_json::json!(review_id)
            );
        });
    }

    #[test]
    fn lease_events_carry_generation() {
        with_temp_home("lease", || {
            let bus = EventBus::new();
            bus.emit_lease_fenced("s-lease", 7, "host-a");
            bus.emit_lease_taken_over("s-lease", 8, "host-b", "host-a");
            let (events, _, _) = bus.poll("s-lease", 0, 128);
            assert_eq!(events.len(), 2);
            assert_eq!(events[0].kind(), "lease.fenced");
            assert_eq!(events[0].to_json()["generation"], serde_json::json!(7));
            assert_eq!(events[1].kind(), "lease.taken_over");
            assert_eq!(events[1].to_json()["generation"], serde_json::json!(8));
            assert_eq!(
                events[1].to_json()["previous_holder"],
                serde_json::json!("host-a")
            );
        });
    }

    /// Durably record an attempt outcome in the temp home's session dir,
    /// as the scheduled daemon / MCP server would after the tool ran.
    fn write_outcome(
        session_id: &str,
        review_id: &str,
        outcome: supercli_core::action_reviews::AttemptOutcome,
    ) {
        let dir = supercli_core::session_host::session_dir(session_id);
        supercli_core::action_reviews::record_attempt_outcome(
            &dir,
            review_id,
            outcome,
            supercli_core::action_reviews::Actor::PolicyAllow,
        )
        .unwrap();
    }

    /// F1: a normal success recorded durably by another process reconciles
    /// into exactly one `tool.executed` event — the production path for
    /// the event the in-process OutcomeListener seam covers for in-process
    /// drivers.
    #[test]
    fn reconcile_outcomes_emits_tool_executed_for_durable_success() {
        with_temp_home("reconcile-exec", || {
            let bus = EventBus::new();
            let review_id = write_review(
                "s-exec",
                supercli_core::action_reviews::ReviewDecision::Approved,
            );
            write_outcome(
                "s-exec",
                &review_id,
                supercli_core::action_reviews::AttemptOutcome::Executed { success: true },
            );
            bus.reconcile_outcomes("s-exec");
            let (events, _, _) = bus.poll("s-exec", 0, 128);
            assert_eq!(events.len(), 1, "expected exactly one event: {events:?}");
            assert_eq!(events[0].kind(), "tool.executed");
            let json = events[0].to_json();
            assert_eq!(json["review_id"], serde_json::json!(review_id));
            assert_eq!(json["exit"], serde_json::json!("ok"));

            // Idempotent: a second reconciliation emits nothing new.
            bus.reconcile_outcomes("s-exec");
            let (events, _, _) = bus.poll("s-exec", 1, 128);
            assert!(
                events.is_empty(),
                "reconciliation must be idempotent: {events:?}"
            );
        });
    }

    /// F1: an ambiguous outcome recorded by another process reconciles
    /// into `tool.ambiguous` plus a `needs_review` escalation — the
    /// external effect is unverified no matter which process recorded it.
    #[test]
    fn reconcile_outcomes_escalates_ambiguous_with_needs_review() {
        with_temp_home("reconcile-amb", || {
            let bus = EventBus::new();
            let review_id = write_review(
                "s-amb",
                supercli_core::action_reviews::ReviewDecision::Approved,
            );
            write_outcome(
                "s-amb",
                &review_id,
                supercli_core::action_reviews::AttemptOutcome::Ambiguous {
                    reason: "transport dropped after send".into(),
                },
            );
            bus.reconcile_outcomes("s-amb");
            let (events, _, _) = bus.poll("s-amb", 0, 128);
            assert_eq!(
                events.len(),
                2,
                "expected tool.ambiguous + needs_review: {events:?}"
            );
            assert_eq!(events[0].kind(), "tool.ambiguous");
            assert_eq!(
                events[0].to_json()["review_id"],
                serde_json::json!(review_id)
            );
            assert_eq!(events[1].kind(), "needs_review");
            assert_eq!(
                events[1].to_json()["review_id"],
                serde_json::json!(review_id)
            );
        });
    }

    /// S3: a durable `never_ran` outcome reconciles to exactly one
    /// `tool.never_ran` event — and never to `needs_review`, since the
    /// tool provably never ran and there is nothing uncertain.
    #[test]
    fn reconcile_outcomes_emits_tool_never_ran_without_escalation() {
        with_temp_home("reconcile-neverran", || {
            let bus = EventBus::new();
            let review_id = write_review(
                "s-nr",
                supercli_core::action_reviews::ReviewDecision::Approved,
            );
            write_outcome(
                "s-nr",
                &review_id,
                supercli_core::action_reviews::AttemptOutcome::NeverRan {
                    reason: "stale lease before tool call".into(),
                },
            );
            bus.reconcile_outcomes("s-nr");
            let (events, _, _) = bus.poll("s-nr", 0, 128);
            assert_eq!(
                events.len(),
                1,
                "expected exactly one tool.never_ran, no needs_review: {events:?}"
            );
            assert_eq!(events[0].kind(), "tool.never_ran");
            assert_eq!(
                events[0].to_json()["review_id"],
                serde_json::json!(review_id)
            );
            assert_eq!(
                events[0].to_json()["reason"],
                serde_json::json!("stale lease before tool call")
            );
            // Idempotent: a second reconcile emits nothing new.
            bus.reconcile_outcomes("s-nr");
            let (events2, _, _) = bus.poll("s-nr", 1, 128);
            assert!(events2.is_empty(), "reconcile is idempotent: {events2:?}");
        });
    }

    /// F1: outcomes already announced in-process (the turn-cancel route's
    /// direct emits) are not re-announced by reconciliation.
    #[test]
    fn reconcile_outcomes_skips_already_announced() {
        with_temp_home("reconcile-skip", || {
            let bus = EventBus::new();
            let review_id = write_review(
                "s-skip",
                supercli_core::action_reviews::ReviewDecision::Approved,
            );
            write_outcome(
                "s-skip",
                &review_id,
                supercli_core::action_reviews::AttemptOutcome::Ambiguous {
                    reason: "cancelled mid-flight".into(),
                },
            );
            // The in-process path (turn-cancel): durable record exists, so
            // the direct emit lands on the stream and marks the outcome
            // announced.
            bus.emit_tool_ambiguous("s-skip", &review_id, "cancelled mid-flight");
            bus.reconcile_outcomes("s-skip");
            let (events, _, _) = bus.poll("s-skip", 0, 128);
            assert_eq!(events.len(), 1, "no duplicate announcement: {events:?}");
            assert_eq!(events[0].kind(), "tool.ambiguous");
        });
    }
}
