//! Stored action reviews: every connector tool call is gated by a durable,
//! tamper-evident review record written *before* the tool executes.
//!
//! Each review is one JSON line in `<session-dir>/action-reviews.jsonl`:
//! who / what / when / decision / tool / arguments hash, plus a hash chain
//! (`prev_hash` links to the previous entry) so later tampering is
//! detectable via [`verify_review_chain`].
//!
//! Write-ahead rule: [`record_review`] appends the entry and fsyncs the
//! file before returning. Callers must record the review first and only
//! execute the tool on success; a write failure fails closed (the action
//! does not run) with [`ReviewError`].

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

/// The review log file inside a session directory.
pub const REVIEWS_FILE: &str = "action-reviews.jsonl";

/// Hash recorded as `prev_hash` for the first entry in a log.
const GENESIS_PREV_HASH: &str = "genesis";

/// Exclusive lock for the review log. Both [`record_review`] and
/// [`record_attempt_outcome`] hold this while they read the chain tail
/// and append, so concurrent writers (threads or processes) cannot
/// interleave lines or fork the SHA-256 hash chain.
///
/// Implemented as an OS advisory lock — `flock(LOCK_EX)` on
/// `action-reviews.jsonl.lock` — never as a create-exclusive lockfile:
/// the kernel releases an `flock` when the holding process exits for any
/// reason (normal exit, crash, SIGKILL), so a dead writer can never wedge
/// the log behind a stale lockfile and force every later write into the
/// 30-second fail-closed timeout. The lockfile itself is never deleted;
/// unlinking it would be racy (a new opener could create a fresh file
/// while another process still holds the old inode locked), and its mere
/// presence is meaningless — only the `flock` state excludes.
///
/// The 30-second acquisition timeout is kept as the fail-closed bound for
/// a genuinely contended lock; it no longer has to cover the stale-lock
/// case, because the kernel makes that case impossible.
struct LogLock {
    /// The open lockfile. Dropping it closes the fd, which releases the
    /// `flock` — no explicit unlock or unlink step exists to be skipped by
    /// a crash.
    #[cfg(unix)]
    _file: std::fs::File,
    /// Non-Unix fallback: `flock(2)` does not exist there, so exclusion
    /// still comes from atomic create-exclusive, released by deleting the
    /// file on drop. A crashed holder can wedge this path; Unix Hosts are
    /// the supported target.
    #[cfg(not(unix))]
    path: std::path::PathBuf,
}

impl LogLock {
    fn acquire(session_dir: &Path) -> Result<Self, ReviewError> {
        let lock_path = session_dir.join(format!("{REVIEWS_FILE}.lock"));
        #[cfg(unix)]
        return Self::acquire_flock(&lock_path);
        #[cfg(not(unix))]
        return Self::acquire_create_new(&lock_path);
    }

    /// Unix path: open (creating) the lockfile and take a non-blocking
    /// exclusive `flock`, polling so the 30s fail-closed bound still
    /// applies to genuine contention. A crashed holder needs no special
    /// handling — the kernel already released its `flock` on process exit.
    #[cfg(unix)]
    fn acquire_flock(lock_path: &Path) -> Result<Self, ReviewError> {
        use std::os::unix::io::AsRawFd;
        let fail = |msg: String| ReviewError::WriteFailed(msg);
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            // The lockfile carries no content — only the flock on its fd
            // matters — so never truncate an existing one.
            .truncate(false)
            .open(lock_path)
            .map_err(|e| fail(format!("cannot open lock {}: {e}", lock_path.display())))?;
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(30);
        loop {
            // SAFETY: `flock` on our own open fd; no pointer arguments.
            let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if rc == 0 {
                return Ok(Self { _file: file });
            }
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::WouldBlock {
                return Err(fail(format!("flock {}: {err}", lock_path.display())));
            }
            if start.elapsed() > timeout {
                return Err(fail(format!(
                    "timed out acquiring review-log lock {}",
                    lock_path.display()
                )));
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    /// Non-Unix fallback: the legacy create-exclusive lockfile.
    #[cfg(not(unix))]
    fn acquire_create_new(lock_path: &Path) -> Result<Self, ReviewError> {
        let fail = |msg: String| ReviewError::WriteFailed(msg);
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(30);
        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(lock_path)
            {
                Ok(_) => {
                    return Ok(Self {
                        path: lock_path.to_path_buf(),
                    })
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if start.elapsed() > timeout {
                        return Err(fail(format!(
                            "timed out acquiring review-log lock {}",
                            lock_path.display()
                        )));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Err(e) => {
                    return Err(fail(format!(
                        "cannot create lock {}: {e}",
                        lock_path.display()
                    )))
                }
            }
        }
    }
}

#[cfg(not(unix))]
impl Drop for LogLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Who authorized the action. Never empty: every path resolves to one of
/// these explicit forms.
#[derive(Debug, Clone)]
pub enum Actor {
    /// A human answered the approval prompt. `device_id` is the answering
    /// device when the approval channel captures it (e.g. the paired
    /// phone), otherwise an explicit channel label such as
    /// `local-prompt` — never blank.
    Human { device_id: String },
    /// A scheduled autonomous run. `trigger_id` is the schedule id.
    Scheduled { trigger_id: String },
    /// An allow-policy tool that needed no prompt.
    PolicyAllow,
}

impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Actor::Human { device_id } => write!(f, "human:{device_id}"),
            Actor::Scheduled { trigger_id } => write!(f, "scheduled:{trigger_id}"),
            Actor::PolicyAllow => write!(f, "policy:allow"),
        }
    }
}

impl Actor {
    /// Parse back the display form; unknown forms become an explicit
    /// human label rather than an empty actor.
    pub fn parse(s: &str) -> Self {
        if let Some(id) = s.strip_prefix("human:") {
            Actor::Human {
                device_id: if id.is_empty() {
                    "unidentified".to_string()
                } else {
                    id.to_string()
                },
            }
        } else if let Some(id) = s.strip_prefix("scheduled:") {
            Actor::Scheduled {
                trigger_id: if id.is_empty() {
                    "unknown-trigger".to_string()
                } else {
                    id.to_string()
                },
            }
        } else if s == "policy:allow" {
            Actor::PolicyAllow
        } else {
            Actor::Human {
                device_id: "unidentified".to_string(),
            }
        }
    }
}

/// The review's decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDecision {
    Approved,
    Denied,
}

impl ReviewDecision {
    fn as_str(self) -> &'static str {
        match self {
            ReviewDecision::Approved => "approved",
            ReviewDecision::Denied => "denied",
        }
    }
}

/// A review entry as written to the log.
#[derive(Debug, Clone)]
pub struct ReviewEntry {
    pub review_id: String,
    pub ts_ms: u64,
    pub actor: Actor,
    pub connector: String,
    pub tool: String,
    pub args_hash: String,
    pub decision: ReviewDecision,
    /// Attempt id this review authorizes a replacement for, if any.
    pub replaces_attempt: Option<String>,
    pub prev_hash: String,
    pub entry_hash: String,
}

/// What went wrong recording a review. Any of these means the action must
/// not run: fail closed.
#[derive(Debug)]
pub enum ReviewError {
    /// The review could not be durably written (I/O, fsync failure, ...).
    /// Distinct from tool failures: the tool never executed.
    WriteFailed(String),
}

impl fmt::Display for ReviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReviewError::WriteFailed(e) => write!(
                f,
                "action review could not be durably recorded ({e}); failing closed: the tool was not executed"
            ),
        }
    }
}

impl std::error::Error for ReviewError {}

/// What went wrong verifying a review chain.
#[derive(Debug)]
pub enum ChainError {
    Io(String),
    CorruptLine { line: usize, detail: String },
    HashMismatch { line: usize },
    BrokenLink { line: usize },
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChainError::Io(e) => write!(f, "cannot read review log: {e}"),
            ChainError::CorruptLine { line, detail } => {
                write!(f, "review log line {line} is corrupt: {detail}")
            }
            ChainError::HashMismatch { line } => {
                write!(
                    f,
                    "review log line {line} failed hash verification (tampered?)"
                )
            }
            ChainError::BrokenLink { line } => {
                write!(
                    f,
                    "review log line {line} does not link to the previous entry (tampered?)"
                )
            }
        }
    }
}

impl std::error::Error for ChainError {}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    // Local hex encoding: avoids a new dependency for one call.
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        out.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    out
}

/// Canonical bytes of an entry for hashing: fixed field order, no
/// `entry_hash` (it is what we are computing).
fn canonical_bytes(entry: &ReviewEntry) -> Vec<u8> {
    let v = json!({
        "review_id": entry.review_id,
        "ts_ms": entry.ts_ms,
        "actor": entry.actor.to_string(),
        "connector": entry.connector,
        "tool": entry.tool,
        "args_hash": entry.args_hash,
        "decision": entry.decision.as_str(),
        "replaces_attempt": entry.replaces_attempt,
        "prev_hash": entry.prev_hash,
    });
    // Canonical form: serde_json is built WITHOUT the preserve_order feature,
    // so Map is a BTreeMap and keys serialize in ALPHABETICAL order, not
    // insertion order. The field order in the json! macro above is irrelevant;
    // what matters is the sorted-key bytes. See docs/hash-chain-canonical-form.md
    // for the full specification and golden vectors.
    v.to_string().into_bytes()
}

fn entry_line(entry: &ReviewEntry) -> String {
    json!({
        "review_id": entry.review_id,
        "ts_ms": entry.ts_ms,
        "actor": entry.actor.to_string(),
        "connector": entry.connector,
        "tool": entry.tool,
        "args_hash": entry.args_hash,
        "decision": entry.decision.as_str(),
        "replaces_attempt": entry.replaces_attempt,
        "prev_hash": entry.prev_hash,
        "entry_hash": entry.entry_hash,
    })
    .to_string()
}

/// Read the `entry_hash` of the last line in the log, if any.
fn last_entry_hash(session_dir: &Path) -> Result<Option<String>, String> {
    let path = session_dir.join(REVIEWS_FILE);
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot open review log: {e}")),
    };
    let reader = BufReader::new(file);
    let mut last: Option<String> = None;
    for line in reader.lines() {
        let line = line.map_err(|e| format!("cannot read review log: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        last = Some(line);
    }
    match last {
        None => Ok(None),
        Some(line) => {
            let v: Value =
                serde_json::from_str(&line).map_err(|e| format!("corrupt review log: {e}"))?;
            Ok(v.get("entry_hash")
                .and_then(Value::as_str)
                .map(str::to_string))
        }
    }
}

/// Durably record a review *before* the tool executes.
///
/// Appends one JSON line to `action-reviews.jsonl` and fsyncs the file, so
/// a crash between the review and the call leaves a review with no attempt
/// — never an attempt with no review. On any write failure returns
/// [`ReviewError::WriteFailed`]; the caller must fail closed.
pub fn record_review(
    session_dir: &Path,
    actor: Actor,
    connector: &str,
    tool: &str,
    args_hash: &str,
    decision: ReviewDecision,
    replaces_attempt: Option<&str>,
) -> Result<ReviewEntry, ReviewError> {
    // Exclusive lock: the tail-hash read and the append are one atomic
    // step. Without this, two concurrent writers can read the same tail
    // and fork the chain.
    let _lock = LogLock::acquire(session_dir)?;
    let prev_hash = last_entry_hash(session_dir)
        .map_err(ReviewError::WriteFailed)?
        .unwrap_or_else(|| GENESIS_PREV_HASH.to_string());
    let mut entry = ReviewEntry {
        review_id: uuid::Uuid::new_v4().to_string(),
        ts_ms: now_ms(),
        actor,
        connector: connector.to_string(),
        tool: tool.to_string(),
        args_hash: args_hash.to_string(),
        decision,
        replaces_attempt: replaces_attempt.map(str::to_string),
        prev_hash,
        entry_hash: String::new(),
    };
    entry.entry_hash = sha256_hex(&canonical_bytes(&entry));
    let mut line = entry_line(&entry);
    line.push('\n');

    let path = session_dir.join(REVIEWS_FILE);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| ReviewError::WriteFailed(format!("open {}: {e}", path.display())))?;
    file.write_all(line.as_bytes())
        .map_err(|e| ReviewError::WriteFailed(format!("write {}: {e}", path.display())))?;
    file.flush()
        .map_err(|e| ReviewError::WriteFailed(format!("flush {}: {e}", path.display())))?;
    // Durable before we return: the review must survive a crash here.
    file.sync_all()
        .map_err(|e| ReviewError::WriteFailed(format!("fsync {}: {e}", path.display())))?;
    Ok(entry)
}

/// The recorded outcome of one authorized tool attempt.
///
/// `Ambiguous` is the Phase 5 uncertain-write marker: the Host could not
/// determine whether the external write took effect (e.g. a turn cancel
/// landed mid-call). An ambiguous attempt is never auto-retried and never
/// reclassified as failed — only an explicit human-approved replacement
/// (via `replaces_attempt`) may supersede it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptOutcome {
    Executed {
        success: bool,
    },
    Ambiguous {
        reason: String,
    },
    /// The tool provably never ran: the attempt was refused before any
    /// tool-call bytes were sent (stale lease fence between the write-ahead
    /// review and the send; stale lease after a definite transport failure
    /// that provably never reached the far side). Unlike `Ambiguous` this
    /// is certain — a later worker may safely re-fire the schedule — so it
    /// never escalates to `needs_review` and never blocks takeover.
    NeverRan {
        reason: String,
    },
}

impl AttemptOutcome {
    fn kind_str(&self) -> &'static str {
        match self {
            AttemptOutcome::Executed { .. } => "executed",
            AttemptOutcome::Ambiguous { .. } => "ambiguous",
            AttemptOutcome::NeverRan { .. } => "never_ran",
        }
    }
}

/// One attempt-outcome entry in the same hash-chained log as the reviews.
/// Appended after the tool runs (or when the Host marks the attempt
/// ambiguous); the outcome extends the review's chain link, so a client
/// can correlate `review_id` across the whole attempt lifecycle.
#[derive(Debug, Clone)]
pub struct OutcomeEntry {
    pub review_id: String,
    pub ts_ms: u64,
    pub actor: Actor,
    pub outcome: AttemptOutcome,
    pub prev_hash: String,
    pub entry_hash: String,
}

/// Canonical bytes of an outcome entry for hashing: keys sorted alphabetically
/// (serde_json without preserve_order), no `entry_hash` (it is what we are
/// computing). See docs/hash-chain-canonical-form.md for the specification.
fn canonical_outcome_bytes(entry: &OutcomeEntry) -> Vec<u8> {
    let (success, reason) = match &entry.outcome {
        AttemptOutcome::Executed { success } => (Some(*success), None),
        AttemptOutcome::Ambiguous { reason } | AttemptOutcome::NeverRan { reason } => {
            (None, Some(reason.as_str()))
        }
    };
    let v = json!({
        "type": "attempt_outcome",
        "review_id": entry.review_id,
        "ts_ms": entry.ts_ms,
        "actor": entry.actor.to_string(),
        "outcome": entry.outcome.kind_str(),
        "success": success,
        "reason": reason,
        "prev_hash": entry.prev_hash,
    });
    v.to_string().into_bytes()
}

fn outcome_entry_line(entry: &OutcomeEntry) -> String {
    let (success, reason) = match &entry.outcome {
        AttemptOutcome::Executed { success } => (Some(*success), None),
        AttemptOutcome::Ambiguous { reason } | AttemptOutcome::NeverRan { reason } => {
            (None, Some(reason.as_str()))
        }
    };
    json!({
        "type": "attempt_outcome",
        "review_id": entry.review_id,
        "ts_ms": entry.ts_ms,
        "actor": entry.actor.to_string(),
        "outcome": entry.outcome.kind_str(),
        "success": success,
        "reason": reason,
        "prev_hash": entry.prev_hash,
        "entry_hash": entry.entry_hash,
    })
    .to_string()
}

/// Durably record the outcome of one authorized tool attempt.
///
/// Appends one JSON line to `action-reviews.jsonl` (same file, same hash
/// chain, same fsync-before-return durability as [`record_review`]). Fails
/// closed: an outcome for an unknown `review_id`, or any write failure,
/// returns [`ReviewError::WriteFailed`] and nothing is appended. Recording
/// a second outcome for a review that already has one is also refused —
/// attempt history is append-only, never rewritten.
pub fn record_attempt_outcome(
    session_dir: &Path,
    review_id: &str,
    outcome: AttemptOutcome,
    actor: Actor,
) -> Result<OutcomeEntry, ReviewError> {
    let fail = |msg: String| ReviewError::WriteFailed(msg);
    // Same exclusive lock as `record_review`: the review-existence scan,
    // the duplicate-outcome check, the tail-hash read, and the append are
    // one atomic step.
    let _lock = LogLock::acquire(session_dir)?;
    // The outcome must attach to a real review, and a review gets at most
    // one outcome: re-read the log (it is append-only; a short scan is the
    // honest check, not a cache).
    let path = session_dir.join(REVIEWS_FILE);
    let mut have_review = false;
    let mut have_outcome = false;
    if path.exists() {
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| fail(format!("read {}: {e}", path.display())))?;
        for line in contents.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let v: Value =
                serde_json::from_str(line).map_err(|e| fail(format!("corrupt review log: {e}")))?;
            if v.get("review_id").and_then(Value::as_str) != Some(review_id) {
                continue;
            }
            match v.get("type").and_then(Value::as_str) {
                Some("attempt_outcome") => have_outcome = true,
                _ => have_review = true,
            }
        }
    }
    if !have_review {
        return Err(fail(format!(
            "unknown review_id {review_id:?}: refusing to record an orphan outcome"
        )));
    }
    if have_outcome {
        return Err(fail(format!(
            "review {review_id:?} already has a recorded outcome: refusing to rewrite history"
        )));
    }

    let prev_hash = last_entry_hash(session_dir)
        .map_err(fail)?
        .unwrap_or_else(|| GENESIS_PREV_HASH.to_string());
    let mut entry = OutcomeEntry {
        review_id: review_id.to_string(),
        ts_ms: now_ms(),
        actor,
        outcome,
        prev_hash,
        entry_hash: String::new(),
    };
    entry.entry_hash = sha256_hex(&canonical_outcome_bytes(&entry));
    let mut line = outcome_entry_line(&entry);
    line.push('\n');

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| fail(format!("open {}: {e}", path.display())))?;
    file.write_all(line.as_bytes())
        .map_err(|e| fail(format!("write {}: {e}", path.display())))?;
    file.flush()
        .map_err(|e| fail(format!("flush {}: {e}", path.display())))?;
    file.sync_all()
        .map_err(|e| fail(format!("fsync {}: {e}", path.display())))?;
    Ok(entry)
}

/// Review ids that authorized an execution (`Approved`) but have no
/// recorded attempt outcome yet — i.e. tool calls that may still be in
/// flight. This is the set a turn cancel must mark ambiguous.
pub fn inflight_reviews(session_dir: &Path) -> Result<Vec<String>, ReviewError> {
    let fail = |msg: String| ReviewError::WriteFailed(msg);
    let path = session_dir.join(REVIEWS_FILE);
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(fail(format!("read {}: {e}", path.display()))),
    };
    let mut approved: Vec<String> = Vec::new();
    let mut resolved: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in contents.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value =
            serde_json::from_str(line).map_err(|e| fail(format!("corrupt review log: {e}")))?;
        let Some(review_id) = v.get("review_id").and_then(Value::as_str) else {
            continue;
        };
        match v.get("type").and_then(Value::as_str) {
            Some("attempt_outcome") => {
                resolved.insert(review_id.to_string());
            }
            _ => {
                if v.get("decision").and_then(Value::as_str) == Some("approved") {
                    approved.push(review_id.to_string());
                }
            }
        }
    }
    approved.retain(|id| !resolved.contains(id));
    Ok(approved)
}

/// Verify the hash chain of a session's review log.
///
/// Returns the number of verified entries. Fails on the first corrupt
/// line, hash mismatch, or broken link.
pub fn verify_review_chain(session_dir: &Path) -> Result<usize, ChainError> {
    let path = session_dir.join(REVIEWS_FILE);
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(ChainError::Io(format!("open {}: {e}", path.display()))),
    };
    let reader = BufReader::new(file);
    let mut prev_hash = GENESIS_PREV_HASH.to_string();
    let mut count = 0usize;
    for (idx, line) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let line = line.map_err(|e| ChainError::Io(format!("read {}: {e}", path.display())))?;
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(&line).map_err(|e| ChainError::CorruptLine {
            line: line_no,
            detail: e.to_string(),
        })?;
        let get = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .ok_or_else(|| ChainError::CorruptLine {
                    line: line_no,
                    detail: format!("missing/invalid field {k:?}"),
                })
        };
        // Outcome entries share the review log's hash chain; verify them
        // with their own canonical form.
        if v.get("type").and_then(Value::as_str) == Some("attempt_outcome") {
            let outcome = match get("outcome")? {
                "executed" => AttemptOutcome::Executed {
                    success: v.get("success").and_then(Value::as_bool).ok_or_else(|| {
                        ChainError::CorruptLine {
                            line: line_no,
                            detail: "missing/invalid field \"success\"".to_string(),
                        }
                    })?,
                },
                "ambiguous" => AttemptOutcome::Ambiguous {
                    reason: get("reason")?.to_string(),
                },
                "never_ran" => AttemptOutcome::NeverRan {
                    reason: get("reason")?.to_string(),
                },
                other => {
                    return Err(ChainError::CorruptLine {
                        line: line_no,
                        detail: format!("unknown outcome {other:?}"),
                    });
                }
            };
            let entry = OutcomeEntry {
                review_id: get("review_id")?.to_string(),
                ts_ms: v.get("ts_ms").and_then(Value::as_u64).ok_or_else(|| {
                    ChainError::CorruptLine {
                        line: line_no,
                        detail: "missing/invalid field \"ts_ms\"".to_string(),
                    }
                })?,
                actor: Actor::parse(get("actor")?),
                outcome,
                prev_hash: get("prev_hash")?.to_string(),
                entry_hash: get("entry_hash")?.to_string(),
            };
            if entry.prev_hash != prev_hash {
                return Err(ChainError::BrokenLink { line: line_no });
            }
            if sha256_hex(&canonical_outcome_bytes(&entry)) != entry.entry_hash {
                return Err(ChainError::HashMismatch { line: line_no });
            }
            prev_hash = entry.entry_hash.clone();
            count += 1;
            continue;
        }
        let entry = ReviewEntry {
            review_id: get("review_id")?.to_string(),
            ts_ms: v.get("ts_ms").and_then(Value::as_u64).ok_or_else(|| {
                ChainError::CorruptLine {
                    line: line_no,
                    detail: "missing/invalid field \"ts_ms\"".to_string(),
                }
            })?,
            actor: Actor::parse(get("actor")?),
            connector: get("connector")?.to_string(),
            tool: get("tool")?.to_string(),
            args_hash: get("args_hash")?.to_string(),
            decision: match get("decision")? {
                "approved" => ReviewDecision::Approved,
                "denied" => ReviewDecision::Denied,
                other => {
                    return Err(ChainError::CorruptLine {
                        line: line_no,
                        detail: format!("unknown decision {other:?}"),
                    })
                }
            },
            replaces_attempt: v
                .get("replaces_attempt")
                .and_then(Value::as_str)
                .map(str::to_string),
            prev_hash: get("prev_hash")?.to_string(),
            entry_hash: get("entry_hash")?.to_string(),
        };
        if entry.prev_hash != prev_hash {
            return Err(ChainError::BrokenLink { line: line_no });
        }
        if sha256_hex(&canonical_bytes(&entry)) != entry.entry_hash {
            return Err(ChainError::HashMismatch { line: line_no });
        }
        prev_hash = entry.entry_hash.clone();
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("unpeel-core-reviews-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn actor_display_is_never_empty() {
        assert_eq!(
            Actor::Human {
                device_id: "phone-1".into()
            }
            .to_string(),
            "human:phone-1"
        );
        assert_eq!(
            Actor::Scheduled {
                trigger_id: "nightly".into()
            }
            .to_string(),
            "scheduled:nightly"
        );
        assert_eq!(Actor::PolicyAllow.to_string(), "policy:allow");
        // Parse never yields an empty actor either.
        assert_eq!(Actor::parse("human:").to_string(), "human:unidentified");
        assert_eq!(
            Actor::parse("scheduled:").to_string(),
            "scheduled:unknown-trigger"
        );
        assert_eq!(Actor::parse("bogus").to_string(), "human:unidentified");
    }

    #[test]
    fn chain_verifies_and_detects_tampering() {
        let dir = test_dir("chain");
        for i in 0..3 {
            record_review(
                &dir,
                Actor::Human {
                    device_id: "phone-1".into(),
                },
                "c",
                &format!("tool-{i}"),
                "ab12",
                ReviewDecision::Approved,
                None,
            )
            .expect("record");
        }
        assert_eq!(verify_review_chain(&dir).expect("verify"), 3);

        // Flip one byte in the second entry's tool name.
        let path = dir.join(REVIEWS_FILE);
        let content = std::fs::read_to_string(&path).unwrap();
        let tampered = content.replacen("tool-1", "tool-X", 1);
        assert_ne!(content, tampered);
        std::fs::write(&path, tampered).unwrap();
        let err = verify_review_chain(&dir).expect_err("tamper must be detected");
        assert!(
            matches!(
                err,
                ChainError::HashMismatch { line: 2 } | ChainError::BrokenLink { line: _ }
            ),
            "unexpected: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_log_verifies_as_empty() {
        let dir = test_dir("empty");
        assert_eq!(verify_review_chain(&dir).expect("verify"), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn review_write_failure_is_fail_closed() {
        // A session dir that is a file, not a directory: the review log
        // cannot be created.
        let dir = test_dir("notadir");
        let file_path = dir.join("blocker");
        std::fs::write(&file_path, b"x").unwrap();
        let err = record_review(
            &file_path,
            Actor::PolicyAllow,
            "c",
            "t",
            "ab12",
            ReviewDecision::Approved,
            None,
        )
        .expect_err("write must fail");
        assert!(matches!(err, ReviewError::WriteFailed(_)));
        assert!(err.to_string().contains("failing closed"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn outcome_record_attaches_to_review_and_extends_chain() {
        let dir = test_dir("outcome");
        let actor = Actor::Human {
            device_id: "phone-1".into(),
        };
        let review = record_review(
            &dir,
            actor.clone(),
            "connector-x",
            "write_file",
            "aa55",
            ReviewDecision::Approved,
            None,
        )
        .expect("review");
        // In-flight before the outcome is recorded.
        let inflight = inflight_reviews(&dir).expect("inflight");
        assert_eq!(inflight, vec![review.review_id.clone()]);

        let outcome = record_attempt_outcome(
            &dir,
            &review.review_id,
            AttemptOutcome::Ambiguous {
                reason: "turn cancelled mid-flight".into(),
            },
            actor,
        )
        .expect("outcome");
        assert_eq!(outcome.review_id, review.review_id);
        assert!(matches!(outcome.outcome, AttemptOutcome::Ambiguous { .. }));
        // The outcome extends the same hash chain: prev links to the review.
        assert_eq!(outcome.prev_hash, review.entry_hash);

        // No longer in-flight.
        let inflight = inflight_reviews(&dir).expect("inflight");
        assert!(inflight.is_empty());

        // The mixed chain verifies: 1 review + 1 outcome.
        assert_eq!(verify_review_chain(&dir).expect("verify"), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// S3: `NeverRan` round-trips through the durable log: it records,
    /// serializes as `never_ran`, extends the hash chain, resolves the
    /// review out of the in-flight set, and verifies.
    #[test]
    fn never_ran_outcome_records_and_verifies() {
        let dir = test_dir("outcome-neverran");
        let actor = Actor::Scheduled {
            trigger_id: "hourly".into(),
        };
        let review = record_review(
            &dir,
            actor.clone(),
            "connector-x",
            "write_file",
            "aa55",
            ReviewDecision::Approved,
            None,
        )
        .expect("review");
        assert_eq!(
            inflight_reviews(&dir).expect("inflight"),
            vec![review.review_id.clone()]
        );

        let outcome = record_attempt_outcome(
            &dir,
            &review.review_id,
            AttemptOutcome::NeverRan {
                reason: "stale lease before tool call".into(),
            },
            actor,
        )
        .expect("outcome");
        assert_eq!(outcome.review_id, review.review_id);
        assert!(matches!(
            outcome.outcome,
            AttemptOutcome::NeverRan { ref reason } if reason == "stale lease before tool call"
        ));
        assert_eq!(outcome.prev_hash, review.entry_hash);

        // Resolved out of the in-flight set, and the chain verifies.
        assert!(inflight_reviews(&dir).expect("inflight").is_empty());
        assert_eq!(verify_review_chain(&dir).expect("verify"), 2);

        // The serialized line carries `never_ran` with a reason and a null
        // success — the exact bytes other implementations must match.
        let line = outcome_entry_line(&outcome);
        let v: serde_json::Value = serde_json::from_str(&line).expect("valid json");
        assert_eq!(v["type"], "attempt_outcome");
        assert_eq!(v["outcome"], "never_ran");
        assert_eq!(v["reason"], "stale lease before tool call");
        assert!(v["success"].is_null());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn outcome_refuses_unknown_review_and_double_outcome() {
        let dir = test_dir("outcome-refuse");
        let actor = Actor::PolicyAllow;
        // Unknown review id: fail closed, no orphan outcome.
        let err = record_attempt_outcome(
            &dir,
            "no-such-review",
            AttemptOutcome::Executed { success: true },
            actor.clone(),
        )
        .expect_err("must refuse unknown review");
        assert!(matches!(err, ReviewError::WriteFailed(_)));

        let review = record_review(
            &dir,
            actor.clone(),
            "c",
            "t",
            "ab12",
            ReviewDecision::Approved,
            None,
        )
        .expect("review");
        record_attempt_outcome(
            &dir,
            &review.review_id,
            AttemptOutcome::Executed { success: true },
            actor.clone(),
        )
        .expect("first outcome");
        // A second outcome for the same review rewrites history: refused.
        let err = record_attempt_outcome(
            &dir,
            &review.review_id,
            AttemptOutcome::Executed { success: false },
            actor,
        )
        .expect_err("must refuse second outcome");
        assert!(matches!(err, ReviewError::WriteFailed(_)));
        // Chain still verifies with exactly the two entries.
        assert_eq!(verify_review_chain(&dir).expect("verify"), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn outcome_tamper_is_detected() {
        let dir = test_dir("outcome-tamper");
        let actor = Actor::PolicyAllow;
        let review = record_review(
            &dir,
            actor.clone(),
            "c",
            "t",
            "ab12",
            ReviewDecision::Approved,
            None,
        )
        .expect("review");
        record_attempt_outcome(
            &dir,
            &review.review_id,
            AttemptOutcome::Executed { success: true },
            actor,
        )
        .expect("outcome");
        // Flip one byte in the outcome line's reason/success field.
        let path = dir.join(REVIEWS_FILE);
        let contents = std::fs::read_to_string(&path).unwrap();
        let tampered = contents.replacen("\"success\":true", "\"success\":false", 1);
        assert_ne!(tampered, contents);
        std::fs::write(&path, tampered).unwrap();
        assert!(matches!(
            verify_review_chain(&dir),
            Err(ChainError::HashMismatch { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F2: N threads appending reviews + outcomes concurrently must not
    /// interleave lines, lose records, duplicate outcomes, or fork the
    /// hash chain. The exclusive [`LogLock`] serializes the
    /// read-tail/append step for both writers.
    #[test]
    fn concurrent_writers_keep_chain_intact() {
        let dir = test_dir("concurrent-writers");
        const WRITERS: usize = 8;
        const PER_WRITER: usize = 5;

        let mut handles = Vec::new();
        for w in 0..WRITERS {
            let dir = dir.clone();
            handles.push(std::thread::spawn(move || {
                let actor = Actor::PolicyAllow;
                for i in 0..PER_WRITER {
                    let review = record_review(
                        &dir,
                        actor.clone(),
                        "c",
                        &format!("tool-{w}-{i}"),
                        "ab12",
                        ReviewDecision::Approved,
                        None,
                    )
                    .expect("review append must succeed under contention");
                    record_attempt_outcome(
                        &dir,
                        &review.review_id,
                        AttemptOutcome::Executed { success: true },
                        actor.clone(),
                    )
                    .expect("outcome append must succeed under contention");
                }
            }));
        }
        for h in handles {
            h.join().expect("writer thread panicked");
        }

        // No lost records: every review has exactly one outcome.
        let path = dir.join(REVIEWS_FILE);
        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(
            lines.len(),
            WRITERS * PER_WRITER * 2,
            "every review and its outcome must be present"
        );

        // No duplicated outcomes: each review_id appears exactly twice
        // (once as a review, once as its outcome).
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for line in &lines {
            let v: Value = serde_json::from_str(line).expect("valid JSON line");
            let id = v
                .get("review_id")
                .and_then(Value::as_str)
                .expect("review_id on every line")
                .to_string();
            *counts.entry(id).or_insert(0) += 1;
        }
        assert_eq!(counts.len(), WRITERS * PER_WRITER);
        assert!(
            counts.values().all(|&c| c == 2),
            "no review may gain a second outcome under contention"
        );

        // Chain intact: the serialized appends form one valid hash chain.
        verify_review_chain(&dir).expect("hash chain must verify after concurrent writes");

        // The lock is always released: re-acquiring must be immediate, not
        // the 30s fail-closed timeout. (The lockfile itself now persists on
        // disk by design — only the flock state excludes — so absence is
        // no longer the check.)
        let start = std::time::Instant::now();
        let _lock = LogLock::acquire(&dir).expect("lock must be free after writers finish");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "lock re-acquire took {:?}: a stale hold would wedge for 30s",
            start.elapsed()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Child entry point for [`killed_holder_releases_lock`]: when the
    /// `UNPEEL_TEST_HOLD_REVIEW_LOCK` env var names a session dir, record
    /// one review, take the lock, signal readiness via a `holder-ready`
    /// file, then hold the lock until killed. Without the env var this is
    /// a no-op (it also runs as an ordinary test in the parent suite).
    #[test]
    fn lock_holder_child() {
        let Some(dir) = std::env::var_os("UNPEEL_TEST_HOLD_REVIEW_LOCK") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        // One review first, so the parent can prove the chain survived the kill.
        let review = record_review(
            &dir,
            Actor::PolicyAllow,
            "c",
            "child-tool",
            "ab12",
            ReviewDecision::Approved,
            None,
        )
        .expect("child must record its review");
        let _lock = LogLock::acquire(&dir).expect("child must acquire the lock");
        std::fs::write(dir.join("holder-ready"), review.review_id.as_bytes())
            .expect("child must signal readiness");
        // Hold until SIGKILL. `park` in a loop: nothing unparks us, and
        // SIGKILL cannot be caught, so the kernel releases the flock on
        // process exit with no Drop running and no cleanup.
        loop {
            std::thread::park();
        }
    }

    /// A lock holder killed with SIGKILL must not wedge the log: the
    /// kernel releases its `flock` on process exit, so the parent
    /// re-acquires promptly — not after the 30s fail-closed timeout the
    /// old create-exclusive lockfile imposed — and the hash chain still
    /// verifies across the kill.
    #[test]
    fn killed_holder_releases_lock() {
        let dir = test_dir("killed-holder");
        let exe = std::env::current_exe().expect("test binary path");
        let mut child = std::process::Command::new(exe)
            .env("UNPEEL_TEST_HOLD_REVIEW_LOCK", &dir)
            .arg("--exact")
            .arg("action_reviews::tests::lock_holder_child")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn lock-holder child");

        // Wait for the child to hold the lock (bounded; fail loudly if it
        // never gets there instead of hanging the suite).
        let ready = dir.join("holder-ready");
        let start = std::time::Instant::now();
        while !ready.exists() {
            if start.elapsed() > std::time::Duration::from_secs(30) {
                let _ = child.kill();
                panic!("lock-holder child never became ready");
            }
            // Bail if the child died on its own: waiting on the ready
            // file forever would mask the real failure.
            if let Ok(Some(status)) = child.try_wait() {
                panic!("lock-holder child exited early: {status}");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // SIGKILL: uncatchable, no Drop runs, no cleanup — exactly the
        // wedge case the old create-exclusive lockfile could not survive.
        child.kill().expect("SIGKILL the holder");
        let status = child.wait().expect("reap the holder");
        assert!(!status.success(), "killed child must not exit 0");

        // The parent must acquire promptly — well under the 30s
        // fail-closed timeout a stale lockfile would impose.
        let start = std::time::Instant::now();
        let review = record_review(
            &dir,
            Actor::PolicyAllow,
            "c",
            "parent-tool",
            "cd34",
            ReviewDecision::Approved,
            None,
        )
        .expect("parent must acquire the lock promptly after the holder is killed");
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "lock acquisition took {elapsed:?} after SIGKILL: stale-lock wedge"
        );

        record_attempt_outcome(
            &dir,
            &review.review_id,
            AttemptOutcome::Executed { success: true },
            Actor::PolicyAllow,
        )
        .expect("parent outcome");

        // The chain verifies across the kill: the child's review plus the
        // parent's review and outcome, one unbroken hash chain.
        assert_eq!(verify_review_chain(&dir).expect("chain must verify"), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Golden vector: pins the exact canonical bytes and SHA-256 hash of a
    /// fixed review record. Any implementation (including the D1 control
    /// plane) must reproduce these bytes exactly. See
    /// docs/hash-chain-canonical-form.md for the specification.
    #[test]
    fn golden_vector_canonical_bytes() {
        let entry = ReviewEntry {
            review_id: "golden-review-1".into(),
            ts_ms: 1_700_000_000_000,
            actor: Actor::Human {
                device_id: "golden-device".into(),
            },
            connector: "golden-connector".into(),
            tool: "golden.tool".into(),
            args_hash: "abc123".into(),
            decision: ReviewDecision::Approved,
            replaces_attempt: None,
            prev_hash: "GENESIS".into(),
            entry_hash: String::new(),
        };
        let bytes = canonical_bytes(&entry);
        // Keys in alphabetical order, no whitespace, no entry_hash.
        assert_eq!(
            String::from_utf8_lossy(&bytes),
            "{\"actor\":\"human:golden-device\",\"args_hash\":\"abc123\",\
             \"connector\":\"golden-connector\",\"decision\":\"approved\",\
             \"prev_hash\":\"GENESIS\",\"replaces_attempt\":null,\
             \"review_id\":\"golden-review-1\",\"tool\":\"golden.tool\",\
             \"ts_ms\":1700000000000}"
        );
        assert_eq!(
            sha256_hex(&bytes),
            "227f6b6f340c76cde8d040caed6a30438371d4c39bf6c53400e8b8dfccbb24d1"
        );
    }
}
