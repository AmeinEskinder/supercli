//! Session-attached connector tools, consumed by the unified MCP server.
//!
//! `supercli connector enable <name> --session <id>` writes the attachment
//! record (`<session-dir>/connectors.json`); this module is the Host side
//! that consumes it. For every attached connector it opens a link — the
//! connector's MCP stdio process (one per MCP server process) or its
//! MCP-over-HTTP endpoint — injects the keychain token (env var
//! `SUPERCLI_CONNECTOR_TOKEN` for stdio, `Authorization: Bearer` for HTTP),
//! filters the advertised tools to the manifest's closed `tools.provides`
//! list, and enforces the effective approval policy (manifest default;
//! the session record may only tighten it) on every call. OAuth2 tokens
//! are refreshed before they expire. `Ask` tools prompt the user through
//! the Host approval hub (`/mcp/approve-connector`, with the grant
//! persisted per session+tool like the other approval kinds); every call —
//! allowed, approved, denied, or failed — is appended to
//! `<session-dir>/connectors-audit.jsonl`.
//!
//! Resolution is lazy and re-checked on every `tools/list` and
//! `tools/call`, so `disable` and `disconnect` take effect without
//! restarting the MCP server: a detached connector's link is dropped,
//! and a connector whose token was revoked fails closed on the next
//! refresh (the attachment record is rewritten by `disconnect` first, so
//! the drop is normally immediate; the token re-check is the backstop).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

/// Extract a human-readable message from a caught panic payload.
fn panic_message(panic: &Box<dyn std::any::Any + Send + 'static>) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// Test hook: if set, `execute_call` panics instead of calling the connector.
/// Tests use this to verify the mid-call panic → Ambiguous path (R2).
#[cfg(test)]
static EXECUTE_CALL_PANIC_HOOK: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
use sha2::{Digest, Sha256};
use supercli_connector::{
    default_roots, discover, effective_policy,
    link::{CallOutcome, ConnectorLink},
    open_connector_store, read_attachments, ApprovalPolicy, ConnectorManifest, CredentialStore,
    LinkError, OAuthError,
};

/// Handshake timeout for one connector process spawn.
const SPAWN_TIMEOUT: Duration = Duration::from_secs(10);
/// How often a live connector's token is re-read from the keychain. The
/// attachment record is the primary revocation path (`disconnect` rewrites
/// it, so the next refresh drops the process); this is the backstop for a
/// token deleted out of band.
const TOKEN_RECHECK_INTERVAL: Duration = Duration::from_secs(60);
/// Per-session audit log, append-only JSONL.
const AUDIT_FILE: &str = "connectors-audit.jsonl";
/// Approval hub route (served by `supercli serve`'s hook listener, like the
/// other `/mcp/approve-*` routes).
const APPROVE_ROUTE: &str = "/mcp/approve-connector";

fn trace(message: &str) {
    let path = crate::app_paths::supercli_home()
        .join("hooks")
        .join("trace.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(file, "[session-connectors] {message}");
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Scan roots: `SUPERCLI_CONNECTORS_DIR` (colon-separated, tests and dev)
/// wins over the default registrar sources — the same rule as the CLI.
fn roots() -> Vec<PathBuf> {
    if let Some(dirs) = std::env::var_os("SUPERCLI_CONNECTORS_DIR") {
        let roots: Vec<PathBuf> = std::env::split_paths(&dirs).collect();
        if !roots.is_empty() {
            return roots;
        }
    }
    default_roots()
}

/// One attached connector with a live link (stdio process or HTTP
/// session).
struct LiveConnector {
    manifest: ConnectorManifest,
    link: ConnectorLink,
    policy_overrides: HashMap<String, ApprovalPolicy>,
    /// The token the link was opened with; a change triggers a re-open.
    token: String,
    token_checked_at: Instant,
}

impl LiveConnector {
    fn policy_for(&self, tool: &str) -> ApprovalPolicy {
        effective_policy(&self.manifest, &self.policy_overrides, tool)
    }
}

/// Resolve the token to inject for one attached connector through the
/// shared [`supercli_connector::resolve_connector_token`] (OAuth2 tokens
/// are refreshed first when expiring). Failures become skip reasons so a
/// broken attachment produces a diagnosable error instead of a bare
/// "unknown tool".
fn resolve_token(
    manifest: &ConnectorManifest,
    dir: &Path,
    store: &dyn CredentialStore,
    name: &str,
) -> Result<String, String> {
    supercli_connector::resolve_connector_token(manifest, dir, store, name, SPAWN_TIMEOUT).map_err(
        |e| match e {
            OAuthError::NotConnected => {
                format!("{name} is not connected — run `supercli connector connect {name}`")
            }
            _ => format!(
                "{name}: token unavailable ({e}) — reconnect with `supercli connector connect {name}`"
            ),
        },
    )
}

/// The resolved connector set for one session, owned by one MCP server
/// process. `refresh` is called on every tools/list and tools/call.
/// Structured failure from [`SessionConnectors::call_tool_detailed`].
/// `denied` is `Some` exactly when the call was refused before the
/// connector ran — an explicit `Deny` policy, or an `Ask` tool with no
/// human present in autonomous mode. The scheduled autonomous-session
/// runner records denials in the run's audit trail.
/// `uncertain` is true when the call may have executed but no trustworthy
/// result was received (see [`CallOutcome`]): the attempt is persisted in
/// the audit log with `retryable: false`, the scheduled runner never
/// retries it, and only an explicit human-approved replacement call may
/// supersede it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallFailure {
    pub denied: Option<crate::scheduled::DenyReason>,
    pub uncertain: bool,
    /// The run lost its lease: the fencing token no longer matches the
    /// lease row. The tool provably did NOT execute — the fence is
    /// checked before the review write and again before the call — so
    /// unlike `uncertain` there is nothing to resolve, only a lease to
    /// re-win. Never retried.
    pub stale_lease: bool,
    pub message: String,
}

impl ToolCallFailure {
    fn denied(reason: crate::scheduled::DenyReason, message: String) -> Self {
        Self {
            denied: Some(reason),
            uncertain: false,
            stale_lease: false,
            message,
        }
    }

    fn failed(message: String) -> Self {
        Self {
            denied: None,
            uncertain: false,
            stale_lease: false,
            message,
        }
    }

    /// An ambiguous outcome: the connector call may already have executed.
    /// Never auto-retried; the message tells the human how to issue an
    /// explicit replacement.
    fn uncertain(message: String) -> Self {
        Self {
            denied: None,
            uncertain: true,
            stale_lease: false,
            message,
        }
    }

    /// The action review could not be durably recorded, so the tool was
    /// not executed: fail closed. Distinct from connector failures — the
    /// tool never ran.
    fn review_failed(message: String) -> Self {
        Self {
            denied: None,
            uncertain: false,
            stale_lease: false,
            message: format!("action review failed: {message}"),
        }
    }

    /// The run's fencing token no longer matches the lease row: another
    /// worker took over, or the lease lapsed. The tool did NOT execute —
    /// the fence is checked before the review write and again before the
    /// call — so this is a clean refusal, not an uncertain outcome. The
    /// scheduled runner never retries it.
    pub fn stale_lease(schedule_id: &str) -> Self {
        Self {
            denied: None,
            uncertain: false,
            stale_lease: true,
            message: format!(
                "stale lease for schedule {schedule_id:?}: another worker \
                 holds the lease now; refusing to execute"
            ),
        }
    }
}

impl std::fmt::Display for ToolCallFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ToolCallFailure {}

/// Listener notified after every terminal attempt outcome is durably
/// recorded. An in-process Host driver installs this to emit
/// `tool.executed` / `tool.ambiguous` session events; the listener only
/// fires after the durable record exists, so events are never emitted
/// first.
///
/// Tool calls usually execute in other processes (the scheduled daemon,
/// the MCP server), which cannot reach an in-process listener. For those,
/// supercli-serve's session-event bus reconciles new outcome records from
/// the durable `action-reviews.jsonl` into the same events on every
/// `/mobile/events` poll — the log is the cross-process authority, the
/// listener is the in-process fast path.
pub type OutcomeListener = Box<dyn Fn(&str, &crate::action_reviews::AttemptOutcome) + Send + Sync>;

/// Tighten-only decision from a ToolCall `before_execute` hook.
///
/// Mirrors `supercli_events::HookDecision` without depending on that crate
/// (which depends on `supercli_core` — the dependency would be circular).
/// `Escalate` means Allow → Ask (docs/events.md §4.2): the call is routed
/// into the normal approval flow with the attached reason, and runs only
/// if the user approves. Meaningful only when the tool's policy was
/// `Allow`; on `Ask`/`Deny` it is a no-op (cannot loosen).
#[derive(Debug, Clone)]
pub enum BeforeExecuteDecision {
    Allow,
    Escalate(String),
    Reject(String),
}

/// Owned context passed to a [`BeforeExecuteHook`].
#[derive(Debug, Clone)]
pub struct BeforeExecuteContext {
    pub tool: String,
    pub arguments: serde_json::Value,
    pub attempt_id: String,
    pub session_dir: PathBuf,
    pub actor: String,
}

/// Synchronous hook invoked after the write-ahead review is fsynced and
/// before any tool bytes are sent (the `ToolCall.before_execute` event).
/// Installed by the Host/CLI; `None` means no handlers (fast path).
pub type BeforeExecuteHook =
    Box<dyn Fn(&BeforeExecuteContext) -> BeforeExecuteDecision + Send + Sync>;

pub struct SessionConnectors {
    session_id: String,
    session_dir: PathBuf,
    live: HashMap<String, LiveConnector>,
    /// Attachments skipped on the last refresh, with the reason — kept so
    /// a broken attachment produces a diagnosable error instead of a bare
    /// "unknown tool".
    skipped: HashMap<String, String>,
    /// Autonomous mode: this call set is driven by a scheduled run with no
    /// human present. `Ask` tools are denied outright (see
    /// [`crate::scheduled::decide_autonomous`]) instead of blocking on an
    /// unanswerable approval prompt. Set by whatever drives the scheduled
    /// run; default false (interactive).
    autonomous: bool,
    /// Explicit actor for the review log when this call set runs without
    /// an interactive approval (e.g. `scheduled:<trigger-id>` set by the
    /// scheduled runner). When unset, the actor is derived per call:
    /// `policy:allow` for allow-policy tools, `human:<device>` for
    /// approvals answered at a prompt.
    actor: Option<String>,
    /// Fencing token for the scheduled run currently driving this call
    /// set, if any. Installed by the scheduled runner right after it wins
    /// the lease claim; `None` for interactive use, which is never
    /// fenced. Every side-effecting step of `execute_call` checks it and
    /// refuses with [`ToolCallFailure::stale_lease`] when the lease moved
    /// on — the tool then provably does not execute.
    lease_fence: Option<crate::schedule_leases::LeaseFence>,
    /// Optional listener notified after every terminal attempt outcome is
    /// durably recorded. The Host (supercli-serve) installs this to emit
    /// `tool.executed` / `tool.ambiguous` session events; the listener
    /// only fires after the durable record exists, so events are never
    /// emitted first.
    outcome_listener: Option<OutcomeListener>,
    /// Optional synchronous `ToolCall.before_execute` hook. Invoked after
    /// the write-ahead review is fsynced and before any tool bytes are
    /// sent. A `Reject` records `NeverRan { reason: "hook_rejected" }` and
    /// the tool does not run; an `Escalate` on an `Allow`-policy call
    /// re-enters the normal approval flow with the hook's reason attached
    /// (the tool runs only if the user approves). Installed by the
    /// Host/CLI via [`Self::set_before_execute_hook`]; `None` is the fast
    /// path (no hooks configured).
    before_execute_hook: Option<BeforeExecuteHook>,
}

impl SessionConnectors {
    /// Resolve the attachment record for a session directory.
    pub fn resolve(session_id: &str, session_dir: &Path) -> Self {
        let mut set = Self {
            session_id: session_id.to_string(),
            session_dir: session_dir.to_path_buf(),
            live: HashMap::new(),
            skipped: HashMap::new(),
            autonomous: false,
            actor: None,
            lease_fence: None,
            outcome_listener: None,
            before_execute_hook: None,
        };
        set.refresh();
        set
    }

    /// Mark this call set as driven by a scheduled autonomous run (no human
    /// present). The scheduled-session policy then applies: `Ask` tools are
    /// denied instead of prompting. The scheduled runner
    /// ([`crate::scheduled::ScheduledRunner`]) sets this for the duration of
    /// each run and restores the previous value afterwards.
    pub fn set_autonomous(&mut self, autonomous: bool) {
        self.autonomous = autonomous;
    }

    /// Whether this call set is currently driven with no human present.
    pub fn is_autonomous(&self) -> bool {
        self.autonomous
    }

    /// Set the explicit review-log actor for non-interactive runs (the
    /// scheduled runner sets `scheduled:<trigger-id>`). See the `actor`
    /// field.
    pub fn set_actor(&mut self, actor: String) {
        self.actor = Some(actor);
    }

    /// Install the outcome listener (F1). The Host calls this to emit
    /// `tool.executed` / `tool.ambiguous` session events; the listener
    /// fires only after the durable outcome record exists.
    pub fn set_outcome_listener(&mut self, listener: OutcomeListener) {
        self.outcome_listener = Some(listener);
    }

    /// Install the synchronous `ToolCall.before_execute` hook. The hook
    /// runs after the write-ahead review is fsynced and before any tool
    /// bytes are sent; see the `before_execute_hook` field for the
    /// tighten-only semantics. Typically installed once at startup by the
    /// Host/CLI from `supercli_events`.
    pub fn set_before_execute_hook(&mut self, hook: BeforeExecuteHook) {
        self.before_execute_hook = Some(hook);
    }

    /// Durably record a terminal attempt outcome and notify the listener.
    /// Every completion path of [`execute_call`](Self::execute_call) calls
    /// this: success, definite failure, ambiguity, stale-lease refusal,
    /// and cancellation (via the turn-cancel route). A write failure is
    /// logged but does not change the tool result — the tool already ran;
    /// the review stays without an outcome, which [`inflight_reviews`]
    /// conservatively treats as in-flight.
    fn record_outcome(
        &self,
        review_id: &str,
        outcome: crate::action_reviews::AttemptOutcome,
        actor: &str,
    ) {
        let parsed_actor = crate::action_reviews::Actor::parse(actor);
        match crate::action_reviews::record_attempt_outcome(
            &self.session_dir,
            review_id,
            outcome.clone(),
            parsed_actor,
        ) {
            Ok(_) => {
                if let Some(listener) = &self.outcome_listener {
                    listener(review_id, &outcome);
                }
            }
            Err(e) => {
                // The tool already executed; failing the result now would
                // lie. Log loudly — the missing outcome leaves the review
                // in-flight, which is the safe direction.
                eprintln!(
                    "session {}: failed to record outcome for review {review_id}: {e}",
                    self.session_id
                );
            }
        }
    }

    /// Refuse with [`ToolCallFailure::stale_lease`] unless this call
    /// set's fencing token still matches the lease row. Called before
    /// every side-effecting step of `execute_call`: the review write, the
    /// connector call itself, and each attempt write. A stale worker's
    /// tool call is refused before anything executes or is recorded.
    fn check_lease_fence(&self, schedule_id: &str) -> Result<(), ToolCallFailure> {
        if let Some(fence) = &self.lease_fence {
            if !fence.is_current() {
                return Err(ToolCallFailure::stale_lease(schedule_id));
            }
        }
        Ok(())
    }

    /// Re-read the attachment record; spawn newly attached connectors,
    /// drop detached ones, and fail closed on revoked tokens. Cheap when
    /// nothing changed: one small file read plus a directory scan.
    fn refresh(&mut self) {
        let attachments = match read_attachments(&self.session_dir) {
            Ok(a) => a,
            Err(e) => {
                // Fail closed: an unreadable record disables every
                // connector tool rather than serving a stale set.
                trace(&format!(
                    "session {}: cannot read attachment record: {e}; dropping all",
                    self.session_id
                ));
                self.live.clear();
                self.skipped.clear();
                self.skipped.insert(
                    "<record>".to_string(),
                    format!("attachment record unreadable: {e}"),
                );
                return;
            }
        };

        let roots = roots();
        let (found, discovery_errors) =
            discover(&roots.iter().map(PathBuf::as_path).collect::<Vec<_>>());
        let discovered: HashMap<&str, _> = found
            .iter()
            .map(|c| (c.manifest.name.as_str(), c))
            .collect();
        // A corrupt manifest in a scan root is diagnosable: map it by
        // directory name so the skip reason names the real problem.
        let corrupt: HashMap<String, String> = discovery_errors
            .iter()
            .filter_map(|e| {
                e.dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| (n.to_string(), e.error.to_string()))
            })
            .collect();
        let (store, _notice) = open_connector_store();

        // Drop what is no longer attached.
        self.live
            .retain(|name, _| attachments.connectors.contains_key(name));

        let mut skipped: HashMap<String, String> = HashMap::new();
        for (name, attachment) in &attachments.connectors {
            let Some(connector) = discovered.get(name.as_str()) else {
                let reason = corrupt
                    .get(name)
                    .map(|e| format!("manifest invalid: {e}"))
                    .unwrap_or_else(|| {
                        format!("attached but no connector named {name:?} is installed")
                    });
                skipped.insert(name.clone(), reason);
                self.live.remove(name);
                continue;
            };
            let cached_token = self.live.get(name).map(|live| live.token.clone());
            let recheck_due = self
                .live
                .get(name)
                .is_some_and(|live| live.token_checked_at.elapsed() > TOKEN_RECHECK_INTERVAL);
            // The token is re-read from the keychain on first open and on
            // the recheck cadence — the attachment record (re-read every
            // refresh) is the primary revocation path, this is the
            // backstop for a token deleted out of band.
            let token = if cached_token.is_none() || recheck_due {
                match resolve_token(&connector.manifest, &connector.dir, store.as_ref(), name) {
                    Ok(token) => token,
                    Err(reason) => {
                        skipped.insert(name.clone(), reason);
                        self.live.remove(name);
                        continue;
                    }
                }
            } else {
                cached_token.unwrap_or_default()
            };
            let needs_open = !self.live.contains_key(name);
            let token_changed = self.live.get(name).is_some_and(|live| live.token != token);
            if needs_open || token_changed {
                match ConnectorLink::open(
                    &connector.manifest,
                    &connector.dir,
                    &token,
                    SPAWN_TIMEOUT,
                ) {
                    Ok(link) => {
                        self.live.insert(
                            name.clone(),
                            LiveConnector {
                                manifest: connector.manifest.clone(),
                                link,
                                policy_overrides: attachment.policy.clone(),
                                token,
                                token_checked_at: Instant::now(),
                            },
                        );
                    }
                    Err(e) => {
                        trace(&format!(
                            "session {}: open {name} failed: {e}",
                            self.session_id
                        ));
                        skipped.insert(name.clone(), format!("could not start {name}: {e}"));
                        self.live.remove(name);
                    }
                }
            } else {
                let live = self.live.get_mut(name).expect("checked above");
                // Attachment still present: keep the link, take the latest
                // policy overrides (enable may have tightened them) and
                // reset the recheck clock.
                live.policy_overrides = attachment.policy.clone();
                if recheck_due {
                    live.token_checked_at = Instant::now();
                }
            }
        }
        self.skipped = skipped;
    }

    /// Tool definitions for `tools/list`: every advertised (Allow/Ask)
    /// tool of every live connector. Deny tools are not advertised; calls
    /// to them are refused anyway as defense in depth.
    pub fn tool_definitions(&mut self) -> Vec<Value> {
        self.refresh();
        let mut defs = Vec::new();
        let mut names: Vec<&String> = self.live.keys().collect();
        names.sort();
        for name in names {
            let live = &self.live[name];
            for tool in live.link.tools() {
                if live.policy_for(&tool.name) == ApprovalPolicy::Deny {
                    continue;
                }
                defs.push(json!({
                    "name": tool.name,
                    "description": format!(
                        "[connector {}] {}",
                        name,
                        tool.description.as_deref().unwrap_or("")
                    ),
                    "inputSchema": tool.input_schema.clone().unwrap_or(json!({"type": "object"})),
                }));
            }
        }
        defs
    }

    fn find_tool(&mut self, name: &str) -> Option<(String, ApprovalPolicy)> {
        self.refresh();
        let mut connectors: Vec<&String> = self.live.keys().collect();
        connectors.sort();
        for connector in connectors {
            let live = &self.live[connector];
            if live.link.tools().iter().any(|t| t.name == name) {
                return Some((connector.clone(), live.policy_for(name)));
            }
        }
        None
    }

    /// Dispatch one connector tool call: enforce the effective policy
    /// (Ask prompts the user), run it through the connector process, and
    /// audit the outcome. Returns the tool's text result.
    pub fn call_tool(&mut self, name: &str, arguments: &Value) -> Result<String, String> {
        self.call_tool_detailed(name, arguments)
            .map_err(|failure| failure.message)
    }

    /// Resolve the review-log actor for a non-interactive approval path.
    /// Never empty: an explicitly set actor (e.g. `scheduled:<trigger-id>`
    /// from the scheduled runner), otherwise the policy-derived form.
    fn auto_actor(&self, fallback: &str) -> String {
        self.actor.clone().unwrap_or_else(|| fallback.to_string())
    }

    /// Record a denied review and the matching denied attempt audit.
    /// Denials never execute, so a review-write failure still fails closed
    /// (nothing ran) but surfaces the distinct review error.
    #[allow(clippy::too_many_arguments)]
    fn record_denied(
        &mut self,
        actor: &str,
        connector: &str,
        tool: &str,
        policy: ApprovalPolicy,
        arguments: &Value,
        attempt_id: &str,
        args_hash: &str,
        replaces_attempt: Option<&str>,
        reason: crate::scheduled::DenyReason,
        err: String,
    ) -> ToolCallFailure {
        let review_id = match crate::action_reviews::record_review(
            &self.session_dir,
            crate::action_reviews::Actor::parse(actor),
            connector,
            tool,
            args_hash,
            crate::action_reviews::ReviewDecision::Denied,
            replaces_attempt,
        ) {
            Ok(entry) => Some(entry.review_id),
            Err(e) => return ToolCallFailure::review_failed(e.to_string()),
        };
        self.audit_attempt(&AttemptAudit {
            connector,
            tool,
            policy,
            approved: Some(false),
            arguments,
            attempt_id,
            args_hash,
            review_id: review_id.as_deref(),
            request_sent_at: None,
            outcome: "denied",
            retryable: false,
            replaces_attempt,
            error: Some(&err),
        });
        ToolCallFailure::denied(reason, err)
    }

    /// [`call_tool`](Self::call_tool) with structured failures: denials
    /// (explicit `Deny`, or `Ask` with no human present / declined) are
    /// distinguished from connector failures so the scheduled
    /// autonomous-session runner can record them in the run's audit trail.
    ///
    /// Every attempt is persisted to `connectors-audit.jsonl` with an
    /// attempt id, an arguments hash, the `request_sent_at` wire
    /// timestamp, and its [`CallOutcome`]. Ambiguous attempts (the call
    /// may have executed but no trustworthy result arrived) are recorded
    /// with `retryable: false` and returned as
    /// [`ToolCallFailure::uncertain`]: they are never auto-retried, and
    /// only an explicit human-approved
    /// [`call_tool_replacement`](Self::call_tool_replacement) may
    /// supersede them.
    ///
    /// Write-ahead: every execution is preceded by a durably recorded
    /// action review (`action-reviews.jsonl`, append + fsync) that the
    /// attempt references by review id; every denial records a denied
    /// review. If the review write fails, the action does not run.
    pub fn call_tool_detailed(
        &mut self,
        name: &str,
        arguments: &Value,
    ) -> Result<String, ToolCallFailure> {
        let (connector, policy) = match self.find_tool(name) {
            Some(found) => found,
            None => return Err(ToolCallFailure::failed(self.unknown_tool_error(name))),
        };
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let args_hash = args_hash(arguments);
        // The actor for the approved review, resolved per policy path.
        let actor = match policy {
            ApprovalPolicy::Deny => {
                let err = format!("tool {name:?} is denied by its approval policy");
                return Err(self.record_denied(
                    "policy:deny",
                    &connector,
                    name,
                    policy,
                    arguments,
                    &attempt_id,
                    &args_hash,
                    None,
                    crate::scheduled::DenyReason::ExplicitDeny,
                    err,
                ));
            }
            ApprovalPolicy::Ask => {
                // Scheduled autonomous runs have no human to answer the
                // prompt: fail closed per the scheduled-session policy
                // instead of blocking 130 s on an unanswerable approval.
                if self.autonomous {
                    let err = format!(
                        "tool {name:?} requires approval, which is unavailable in autonomous mode; \
                         denied by the scheduled-session policy"
                    );
                    let actor = self.auto_actor("scheduled:unknown-trigger");
                    return Err(self.record_denied(
                        &actor,
                        &connector,
                        name,
                        policy,
                        arguments,
                        &attempt_id,
                        &args_hash,
                        None,
                        crate::scheduled::DenyReason::NoHumanPresent,
                        err,
                    ));
                }
                match request_tool_approval(&self.session_id, &connector, name, None, None, None) {
                    Ok(outcome) => outcome.actor,
                    Err(rejection) => {
                        // A declined prompt is a denial decision by the
                        // human who answered; a prompt that never
                        // completed made no decision.
                        let actor = rejection
                            .actor
                            .unwrap_or_else(|| "human:unanswered-prompt".to_string());
                        let failure = self.record_denied(
                            &actor,
                            &connector,
                            name,
                            policy,
                            arguments,
                            &attempt_id,
                            &args_hash,
                            None,
                            crate::scheduled::DenyReason::AskDeclined,
                            rejection.message.clone(),
                        );
                        if rejection.declined {
                            return Err(failure);
                        }
                        // No decision was made: surface the prompt failure
                        // distinctly rather than as a denial.
                        return Err(ToolCallFailure::failed(rejection.message));
                    }
                }
            }
            ApprovalPolicy::Allow => self.auto_actor("policy:allow"),
        };
        self.execute_call(
            &connector,
            name,
            policy,
            Some(true),
            arguments,
            &attempt_id,
            &args_hash,
            None,
            &actor,
        )
    }

    /// Issue an explicit replacement for an ambiguous attempt. The
    /// replacement names the superseded attempt (`replaces_attempt`) and
    /// always requires fresh human approval — even when the tool's policy
    /// is `Allow` — and is refused outright with no human present
    /// (autonomous mode), so a scheduled run can never auto-fire it.
    pub fn call_tool_replacement(
        &mut self,
        name: &str,
        arguments: &Value,
        replaces_attempt: &str,
    ) -> Result<String, ToolCallFailure> {
        // No human present: refuse before touching tool resolution, so
        // this path can never auto-fire from a scheduled run.
        if self.autonomous {
            let err = format!(
                "replacement for ambiguous attempt {replaces_attempt} requires human approval, \
                 which is unavailable in autonomous mode; refused"
            );
            return Err(ToolCallFailure::denied(
                crate::scheduled::DenyReason::NoHumanPresent,
                err,
            ));
        }
        let (connector, policy) = match self.find_tool(name) {
            Some(found) => found,
            None => return Err(ToolCallFailure::failed(self.unknown_tool_error(name))),
        };
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let hash = args_hash(arguments);
        let actor = match request_tool_approval(
            &self.session_id,
            &connector,
            name,
            Some(replaces_attempt),
            Some(&hash),
            None,
        ) {
            Ok(outcome) => outcome.actor,
            Err(rejection) => {
                let actor = rejection
                    .actor
                    .unwrap_or_else(|| "human:unanswered-prompt".to_string());
                let failure = self.record_denied(
                    &actor,
                    &connector,
                    name,
                    policy,
                    arguments,
                    &attempt_id,
                    &hash,
                    Some(replaces_attempt),
                    crate::scheduled::DenyReason::AskDeclined,
                    rejection.message.clone(),
                );
                if rejection.declined {
                    return Err(failure);
                }
                return Err(ToolCallFailure::failed(rejection.message));
            }
        };
        self.execute_call(
            &connector,
            name,
            policy,
            Some(true),
            arguments,
            &attempt_id,
            &hash,
            Some(replaces_attempt),
            &actor,
        )
    }

    /// Run one approved connector call, persist the attempt, and classify
    /// the outcome. Ambiguous outcomes become
    /// [`ToolCallFailure::uncertain`] — recorded, never retried.
    ///
    /// Write-ahead: the approved review is durably recorded (append +
    /// fsync) *before* the tool executes, and the attempt references its
    /// review id. If the review write fails the tool does not run
    /// ([`ToolCallFailure::review_failed`]).
    #[allow(clippy::too_many_arguments)]
    fn execute_call(
        &mut self,
        connector: &str,
        name: &str,
        policy: ApprovalPolicy,
        approved: Option<bool>,
        arguments: &Value,
        attempt_id: &str,
        args_hash: &str,
        replaces_attempt: Option<&str>,
        actor: &str,
    ) -> Result<String, ToolCallFailure> {
        // R5: per-device rate limit on connector calls. Extract device_id
        // from the actor (human:<device_id>); scheduled/policy actors get
        // their own buckets.
        let device_id = if let Some(id) = actor.strip_prefix("human:") {
            id.to_string()
        } else {
            actor.to_string()
        };
        if !crate::rate_limit::global().check(&device_id, "connector") {
            return Err(ToolCallFailure::failed(format!(
                "rate limit exceeded for connector calls (device {device_id})"
            )));
        }

        // Fencing first: a worker that lost its lease refuses before the
        // review is written, so a stale run leaves no trace at all.
        let schedule_id = actor.strip_prefix("scheduled:").unwrap_or(actor);
        self.check_lease_fence(schedule_id)?;
        let review_id = match crate::action_reviews::record_review(
            &self.session_dir,
            crate::action_reviews::Actor::parse(actor),
            connector,
            name,
            args_hash,
            crate::action_reviews::ReviewDecision::Approved,
            replaces_attempt,
        ) {
            Ok(entry) => entry.review_id,
            Err(e) => return Err(ToolCallFailure::review_failed(e.to_string())),
        };
        // Doc event: ToolCall.before_execute (synchronous, tighten-only).
        // Fires after the write-ahead review is fsynced and before any
        // tool bytes are sent (docs/events.md §3.3). A rejection records
        // NeverRan (the tool provably never ran) and fails closed. An
        // escalation means Allow -> Ask (docs/events.md §4.2): the call
        // re-enters the normal approval flow with the hook's reason
        // attached, and runs only if the user approves. (The decision is
        // computed first so no borrow of `self` is held while the
        // approval prompt — a `&mut self` path — runs.)
        let hook_decision = self.before_execute_hook.as_ref().map(|hook| {
            let ctx = BeforeExecuteContext {
                tool: name.to_string(),
                arguments: arguments.clone(),
                attempt_id: attempt_id.to_string(),
                session_dir: self.session_dir.clone(),
                actor: actor.to_string(),
            };
            hook(&ctx)
        });
        // When a hook escalates an Allow-policy call and the user
        // approves, the human becomes the effective authorizer for the
        // rest of this attempt.
        let mut escalated_actor: Option<String> = None;
        if let Some(decision) = hook_decision {
            match decision {
                BeforeExecuteDecision::Allow => {}
                BeforeExecuteDecision::Escalate(hook_reason) => {
                    if policy == ApprovalPolicy::Allow {
                        if self.autonomous {
                            // No human present to answer the escalated
                            // prompt: fail closed, consistent with the Ask
                            // path in `call_tool_detailed`.
                            let reason = format!(
                                "hook escalated Allow->Ask ({hook_reason}) but no human can answer in autonomous mode"
                            );
                            self.record_outcome(
                                &review_id,
                                crate::action_reviews::AttemptOutcome::NeverRan {
                                    reason: reason.clone(),
                                },
                                actor,
                            );
                            return Err(ToolCallFailure::denied(
                                crate::scheduled::DenyReason::NoHumanPresent,
                                format!(
                                    "tool {name:?} escalated to approval by before_execute hook: {reason}"
                                ),
                            ));
                        }
                        match request_tool_approval(
                            &self.session_id,
                            connector,
                            name,
                            None,
                            None,
                            Some(&hook_reason),
                        ) {
                            Ok(outcome) => {
                                escalated_actor = Some(outcome.actor);
                            }
                            Err(rejection) => {
                                let decider = rejection.actor.unwrap_or_else(|| {
                                    "human:unanswered-prompt".to_string()
                                });
                                let completion = if rejection.declined {
                                    format!("declined by {decider}")
                                } else {
                                    "did not complete".to_string()
                                };
                                let reason = format!(
                                    "hook escalated Allow->Ask ({hook_reason}); approval {completion}"
                                );
                                // The write-ahead review already exists, so
                                // close it out terminally: the tool provably
                                // never ran.
                                self.record_outcome(
                                    &review_id,
                                    crate::action_reviews::AttemptOutcome::NeverRan {
                                        reason,
                                    },
                                    &decider,
                                );
                                self.audit_attempt(&AttemptAudit {
                                    connector,
                                    tool: name,
                                    policy,
                                    approved: Some(false),
                                    arguments,
                                    attempt_id,
                                    args_hash,
                                    review_id: Some(&review_id),
                                    request_sent_at: None,
                                    outcome: "denied",
                                    retryable: false,
                                    replaces_attempt,
                                    error: Some(&rejection.message),
                                });
                                if rejection.declined {
                                    return Err(ToolCallFailure::denied(
                                        crate::scheduled::DenyReason::AskDeclined,
                                        rejection.message,
                                    ));
                                }
                                // No decision was made: surface the prompt
                                // failure distinctly rather than as a denial.
                                return Err(ToolCallFailure::failed(rejection.message));
                            }
                        }
                    }
                    // Policy was already Ask (human approved) or Deny:
                    // escalate is a no-op, cannot loosen.
                }
                BeforeExecuteDecision::Reject(hook_reason) => {
                    self.record_outcome(
                        &review_id,
                        crate::action_reviews::AttemptOutcome::NeverRan {
                            reason: format!("hook_rejected: {hook_reason}"),
                        },
                        actor,
                    );
                    return Err(ToolCallFailure::failed(format!(
                        "tool {name:?} rejected by before_execute hook: {hook_reason}"
                    )));
                }
            }
        }
        let actor: &str = escalated_actor.as_deref().unwrap_or(actor);
        let args: HashMap<String, Value> = arguments
            .as_object()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        // Re-check before the call itself: the lease may have lapsed
        // while the review was being written. (Checked before borrowing
        // `live` so the borrow checker stays happy.)
        // The review exists now, so a stale lease must record a terminal
        // outcome — otherwise the takeover check sees this approved review
        // as in-flight and sends it to needs_review. The tool provably
        // never ran (no call bytes were sent), so this is NeverRan, not
        // Ambiguous: a later worker may safely re-fire.
        if let Err(e) = self.check_lease_fence(schedule_id) {
            self.record_outcome(
                &review_id,
                crate::action_reviews::AttemptOutcome::NeverRan {
                    reason: format!("stale lease before tool call: {e}"),
                },
                actor,
            );
            return Err(e);
        }
        let call_result = {
            let live = self.live.get_mut(connector).expect("find_tool checked");
            // R2: a panic in the connector call must not take down the Host
            // or leave the review in-flight. Catch it, record Ambiguous
            // (the external effect is unknown), and fail the call.
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Test hook: deterministic panic injection (R2). Recover from
                // poison so a panicking hook cannot wedge later calls.
                #[cfg(test)]
                if *EXECUTE_CALL_PANIC_HOOK
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                {
                    panic!("test-injected panic in connector call");
                }
                live.link.call_detailed(name, args)
            }))
        };
        let (result, telemetry) = match call_result {
            Ok(output) => output,
            Err(panic) => {
                let msg = panic_message(&panic);
                self.record_outcome(
                    &review_id,
                    crate::action_reviews::AttemptOutcome::Ambiguous {
                        reason: format!("tool call panicked: {msg}"),
                    },
                    actor,
                );
                self.audit_attempt(&AttemptAudit {
                    connector,
                    tool: name,
                    policy,
                    approved,
                    arguments,
                    attempt_id,
                    args_hash,
                    review_id: Some(&review_id),
                    request_sent_at: None,
                    outcome: "panic",
                    retryable: false,
                    replaces_attempt,
                    error: Some(&format!("tool call panicked: {msg}")),
                });
                return Err(ToolCallFailure::uncertain(format!(
                    "connector tool {name:?} panicked: {msg}"
                )));
            }
        };
        let request_sent_at = telemetry.request_sent_at;
        match result {
            Ok(value) => {
                let text = mcp_text_result(&value);
                // And again before the attempt write: a worker that went
                // stale mid-call must not record an outcome for a run it
                // no longer owns. The review-without-attempt left behind
                // is exactly what the takeover check treats as in-flight.
                // F1: if the fence is stale here the call may have
                // executed — record Ambiguous, never silently in-flight.
                if let Err(e) = self.check_lease_fence(schedule_id) {
                    self.record_outcome(
                        &review_id,
                        crate::action_reviews::AttemptOutcome::Ambiguous {
                            reason: format!("stale lease after tool call: {e}"),
                        },
                        actor,
                    );
                    return Err(e);
                }
                self.audit_attempt(&AttemptAudit {
                    connector,
                    tool: name,
                    policy,
                    approved,
                    arguments,
                    attempt_id,
                    args_hash,
                    review_id: Some(&review_id),
                    request_sent_at,
                    outcome: "ok",
                    retryable: false,
                    replaces_attempt,
                    error: None,
                });
                // F1: every normal completion writes its terminal outcome.
                // Without this the takeover check sees every approved run
                // as in-flight and tool.executed never fires.
                self.record_outcome(
                    &review_id,
                    crate::action_reviews::AttemptOutcome::Executed { success: true },
                    actor,
                );
                Ok(text)
            }
            Err(e) => {
                let err = e.to_string();
                match e.call_outcome(request_sent_at.is_some()) {
                    CallOutcome::DefiniteOk => {
                        unreachable!("call_outcome classifies failures only")
                    }
                    CallOutcome::DefiniteFailed => {
                        // Only pre-send transport failures are worth
                        // retrying: the far side provably never saw the
                        // call and the condition may be transient.
                        // Explicit rejections would fail identically.
                        let retryable = matches!(
                            e,
                            LinkError::Http(
                                supercli_connector::http::HttpConnectorError::TransportSetup(_)
                            )
                        );
                        let err = format!("connector tool {name:?} failed: {err}");
                        // F1: stale fence after a definite failure — the
                        // call provably did not execute, but the review
                        // exists, so record the terminal outcome anyway.
                        // DefiniteFailed means the far side never saw the
                        // call, so this is NeverRan, not Ambiguous.
                        if let Err(fence_err) = self.check_lease_fence(schedule_id) {
                            self.record_outcome(
                                &review_id,
                                crate::action_reviews::AttemptOutcome::NeverRan {
                                    reason: format!(
                                        "stale lease after definite failure: {fence_err}"
                                    ),
                                },
                                actor,
                            );
                            return Err(fence_err);
                        }
                        self.audit_attempt(&AttemptAudit {
                            connector,
                            tool: name,
                            policy,
                            approved,
                            arguments,
                            attempt_id,
                            args_hash,
                            review_id: Some(&review_id),
                            request_sent_at,
                            outcome: "definite_failed",
                            retryable,
                            replaces_attempt,
                            error: Some(&err),
                        });
                        // F1: definite failure is terminal — record it.
                        self.record_outcome(
                            &review_id,
                            crate::action_reviews::AttemptOutcome::Executed { success: false },
                            actor,
                        );
                        Err(ToolCallFailure::failed(err))
                    }
                    CallOutcome::Ambiguous => {
                        let err = format!(
                            "connector tool {name:?} may already have executed \
                             (attempt {attempt_id}): {err}. This attempt was recorded \
                             as ambiguous and will NOT be retried automatically. To \
                             supersede it, issue an explicit human-approved replacement \
                             call referencing attempt {attempt_id}."
                        );
                        // F1: stale fence on an ambiguous call — the
                        // uncertainty stands, so record Ambiguous with the
                        // fence reason appended.
                        if let Err(fence_err) = self.check_lease_fence(schedule_id) {
                            self.record_outcome(
                                &review_id,
                                crate::action_reviews::AttemptOutcome::Ambiguous {
                                    reason: format!(
                                        "ambiguous tool call, then stale lease: {fence_err}; {err}"
                                    ),
                                },
                                actor,
                            );
                            return Err(fence_err);
                        }
                        self.audit_attempt(&AttemptAudit {
                            connector,
                            tool: name,
                            policy,
                            approved,
                            arguments,
                            attempt_id,
                            args_hash,
                            review_id: Some(&review_id),
                            request_sent_at,
                            outcome: "ambiguous",
                            retryable: false,
                            replaces_attempt,
                            error: Some(&err),
                        });
                        // F1: ambiguity is terminal — record it durably so
                        // the takeover check and needs_review see the
                        // truth instead of a bare in-flight review.
                        self.record_outcome(
                            &review_id,
                            crate::action_reviews::AttemptOutcome::Ambiguous {
                                reason: err.clone(),
                            },
                            actor,
                        );
                        Err(ToolCallFailure::uncertain(err))
                    }
                }
            }
        }
    }

    fn unknown_tool_error(&self, name: &str) -> String {
        let mut msg = format!("Unknown tool: {name}");
        if !self.skipped.is_empty() {
            let mut skipped: Vec<(&String, &String)> = self.skipped.iter().collect();
            skipped.sort_by_key(|(k, _)| *k);
            let details: Vec<String> = skipped.iter().map(|(k, v)| format!("{k}: {v}")).collect();
            msg.push_str("; attached connectors currently unavailable: ");
            msg.push_str(&details.join(", "));
        }
        msg
    }

    /// Append one audit line. Append-only: a single `write_all` on an
    /// O_APPEND descriptor never interleaves with another writer's line,
    /// so no lock is needed (unlike read-modify-write shared state).
    /// Every attempt carries its id, arguments hash, `request_sent_at`
    /// wire timestamp, outcome, and retryability, so an ambiguous attempt
    /// can later be matched to its explicit human-approved replacement.
    fn audit_attempt(&self, attempt: &AttemptAudit<'_>) {
        let entry = json!({
            "ts": now_ms(),
            "session": self.session_id,
            "connector": attempt.connector,
            "tool": attempt.tool,
            "policy": format!("{:?}", attempt.policy).to_lowercase(),
            "approved": attempt.approved,
            "arguments": attempt.arguments,
            "attempt_id": attempt.attempt_id,
            "args_hash": attempt.args_hash,
            "review_id": attempt.review_id,
            "request_sent_at": attempt.request_sent_at,
            "outcome": attempt.outcome,
            "retryable": attempt.retryable,
            "replaces_attempt": attempt.replaces_attempt,
            "ok": attempt.outcome == "ok",
            "error": attempt.error,
        });
        let path = self.session_dir.join(AUDIT_FILE);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            use std::io::Write;
            let _ = writeln!(file, "{}", entry);
        } else {
            trace(&format!(
                "session {}: cannot append audit log {}",
                self.session_id,
                path.display()
            ));
        }
    }
}

/// One auditable connector-call attempt, persisted to
/// `connectors-audit.jsonl`. `outcome` is one of `"ok"`,
/// `"definite_failed"`, `"ambiguous"`, or `"denied"`.
struct AttemptAudit<'a> {
    connector: &'a str,
    tool: &'a str,
    policy: ApprovalPolicy,
    approved: Option<bool>,
    arguments: &'a Value,
    attempt_id: &'a str,
    args_hash: &'a str,
    /// The write-ahead review authorizing this attempt, if one was recorded.
    review_id: Option<&'a str>,
    request_sent_at: Option<u64>,
    outcome: &'a str,
    retryable: bool,
    replaces_attempt: Option<&'a str>,
    error: Option<&'a str>,
}

/// Stable SHA-256 hash of the call arguments, recorded per attempt so a
/// later replacement or audit review can prove it carried identical (or
/// deliberately different) arguments.
fn args_hash(arguments: &Value) -> String {
    let canonical = serde_json::to_string(arguments).unwrap_or_default();
    let digest = Sha256::digest(canonical.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Extract the text result the same way `mcp_host::call_tool` does.
fn mcp_text_result(result: &Value) -> String {
    result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Ask the user — through the Host approval hub — to allow one connector
/// tool call. Blocks until answered or the hub times out; a grant is
/// persisted per session+tool, so later calls pass without asking again.
///
/// `replaces_attempt` names the ambiguous attempt this call supersedes
/// (always `Some` for [`SessionConnectors::call_tool_replacement`], which
/// forces a fresh prompt even for `Allow` tools); `args_hash` lets the
/// reviewer confirm the replacement's arguments. Both are additive
/// protocol fields.
/// Outcome of an interactive approval prompt.
struct ApprovalOutcome {
    /// Who answered, e.g. `human:paired-device` or `human:local-prompt`.
    /// Never empty.
    actor: String,
}

/// A rejected or uncompleted approval prompt.
struct ApprovalRejection {
    /// Who rejected, when a human actually answered (declined). `None`
    /// when the prompt never completed — no decision was made.
    actor: Option<String>,
    message: String,
    /// True when the human explicitly declined (a denial decision);
    /// false when the prompt never completed (no decision).
    declined: bool,
}

fn request_tool_approval(
    session_id: &str,
    connector: &str,
    tool: &str,
    replaces_attempt: Option<&str>,
    args_hash: Option<&str>,
    hook_reason: Option<&str>,
) -> Result<ApprovalOutcome, ApprovalRejection> {
    let response = match crate::mcp_host::app_request_with_timeout(
        APPROVE_ROUTE,
        &json!({
            "session_id": session_id,
            "connector": connector,
            "tool": tool,
            "replaces_attempt": replaces_attempt,
            "args_hash": args_hash,
            "hook_reason": hook_reason,
        }),
        Duration::from_secs(130),
    ) {
        Ok(response) => response,
        Err(error) => {
            return Err(ApprovalRejection {
                actor: None,
                declined: false,
                message: format!(
                    "Tool '{tool}' (connector '{connector}') needs your approval, but the approval \
                     prompt did not complete: {error}. If no Supercli frontend is running, start one; \
                     otherwise answer the prompt and retry."
                ),
            })
        }
    };
    // Who answered: the approval channel reports a device id when it
    // captures one (additive `answered_by`); otherwise the prompt was
    // answered at the local console. Never empty.
    let answered_by = response
        .get("answered_by")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("local-prompt");
    let actor = format!("human:{answered_by}");
    if response.get("approved").and_then(Value::as_bool) == Some(true) {
        return Ok(ApprovalOutcome { actor });
    }
    Err(ApprovalRejection {
        actor: Some(actor),
        declined: true,
        message: format!(
            "The user declined the '{tool}' call (connector '{connector}'). Do not retry on your \
             own — ask the user if they want to approve future calls."
        ),
    })
}

impl crate::scheduled::ScheduledToolExecutor for SessionConnectors {
    fn set_autonomous(&mut self, autonomous: bool) {
        SessionConnectors::set_autonomous(self, autonomous);
    }
    fn is_autonomous(&self) -> bool {
        SessionConnectors::is_autonomous(self)
    }
    fn set_actor(&mut self, actor: String) {
        SessionConnectors::set_actor(self, actor);
    }
    fn set_lease_fence(&mut self, fence: Option<crate::schedule_leases::LeaseFence>) {
        self.lease_fence = fence;
    }
    fn lease_fence(&self) -> Option<&crate::schedule_leases::LeaseFence> {
        self.lease_fence.as_ref()
    }
    fn call_tool_detailed(
        &mut self,
        tool: &str,
        arguments: &serde_json::Value,
    ) -> Result<String, ToolCallFailure> {
        SessionConnectors::call_tool_detailed(self, tool, arguments)
    }
}

// ---------------------------------------------------------------------------
// MCP server glue: one cached set per server process.
// ---------------------------------------------------------------------------

static CACHED: Mutex<Option<SessionConnectors>> = Mutex::new(None);

/// The connector set for this MCP server's session, if it runs inside a
/// session at all.
fn cached() -> Option<std::sync::MutexGuard<'static, Option<SessionConnectors>>> {
    let session_id = crate::mcp_host::self_session_id()?;
    let mut guard = CACHED.lock().ok()?;
    let same = guard
        .as_ref()
        .is_some_and(|set| set.session_id == session_id);
    if !same {
        let dir = crate::session_host::session_dir(&session_id);
        *guard = Some(SessionConnectors::resolve(&session_id, &dir));
    }
    Some(guard)
}

/// Tool definitions for `tools/list`. Empty outside a session.
pub(crate) fn connector_tool_definitions() -> Vec<Value> {
    cached()
        .map(|mut guard| {
            guard
                .as_mut()
                .map(|set| set.tool_definitions())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// Whether `name` is currently an attached connector tool. Refreshes the
/// set, so detach/disconnect take effect on the next call.
pub(crate) fn is_connector_tool(name: &str) -> bool {
    cached()
        .map(|mut guard| {
            guard
                .as_mut()
                .is_some_and(|set| set.find_tool(name).is_some())
        })
        .unwrap_or(false)
}

/// Dispatch a `tools/call` to the owning connector process. Returns the
/// tool's text result, like the domain dispatchers.
pub(crate) fn call_connector_tool(name: &str, arguments: &Value) -> Result<String, String> {
    let mut guard = cached().ok_or_else(|| format!("Unknown tool: {name}"))?;
    let set = guard
        .as_mut()
        .ok_or_else(|| format!("Unknown tool: {name}"))?;
    set.call_tool(name, arguments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use supercli_connector::CONNECTORS_KEYCHAIN_ENV;

    const MANIFEST_ASK: &str = r#"
[connector]
name = "asky"
version = "0.1.0"
display_name = "Asky"
description = "ask-policy test connector"
kind = "mcp-stdio"

[auth]
flow = "none"

[tools]
provides = ["asky.echo"]
"#;

    const MANIFEST_ALLOW: &str = r#"
[connector]
name = "allowy"
version = "0.1.0"
display_name = "Allowy"
description = "allow-policy test connector"
kind = "mcp-stdio"

[auth]
flow = "none"

[tools]
provides = ["allowy.echo"]

[policy]
"allowy.echo" = "allow"
"#;

    /// Stub MCP server: argv[1] is the tool name it serves.
    const STUB: &str = r#"import json, sys
TOOL = sys.argv[1]
TOOLS = [{"name": TOOL, "description": "echo it", "inputSchema": {"type": "object"}}]
def respond(mid, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": mid, "result": result}) + "\n")
    sys.stdout.flush()
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method, mid = msg.get("method"), msg.get("id")
    if method == "initialize":
        respond(mid, {"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "stub", "version": "0"}})
    elif method == "tools/list":
        respond(mid, {"tools": TOOLS})
    elif method == "tools/call":
        respond(mid, {"content": [{"type": "text", "text": "echo:" + json.dumps(msg["params"]["arguments"])}]})
"#;

    /// Counting stub: identical to STUB, but every `tools/call` appends one
    /// line to the file named by the `SUPERCLI_COUNT_FILE` env var. Lets a test
    /// prove the mock connector was never invoked (count exactly 0).
    const STUB_COUNTING: &str = r#"import json, sys, os
TOOL = sys.argv[1]
COUNT = os.environ.get("SUPERCLI_COUNT_FILE", "")
TOOLS = [{"name": TOOL, "description": "echo it", "inputSchema": {"type": "object"}}]
def respond(mid, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": mid, "result": result}) + "\n")
    sys.stdout.flush()
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method, mid = msg.get("method"), msg.get("id")
    if method == "initialize":
        respond(mid, {"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "stub", "version": "0"}})
    elif method == "tools/list":
        respond(mid, {"tools": TOOLS})
    elif method == "tools/call":
        if COUNT:
            with open(COUNT, "a") as f:
                f.write("called\n")
        respond(mid, {"content": [{"type": "text", "text": "echo:" + json.dumps(msg["params"]["arguments"])}]})
"#;

    const MANIFEST_COUNT_ALLOW: &str = r#"
[connector]
name = "county"
version = "0.1.0"
display_name = "County"
description = "counting allow-policy test connector"
kind = "mcp-stdio"

[auth]
flow = "none"

[tools]
provides = ["county.echo"]

[policy]
"county.echo" = "allow"
"#;

    struct Fixture {
        dir: PathBuf,
        session_dir: PathBuf,
        _env_guard: std::sync::MutexGuard<'static, ()>,
    }

    static FIXTURE_LOCK: Mutex<()> = Mutex::new(());
    static FIXTURE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    impl Fixture {
        fn write_connector(dir: &Path, name: &str, manifest: &str, tool: &str) {
            Self::write_connector_with_stub(dir, name, manifest, tool, STUB);
        }

        fn write_connector_with_stub(
            dir: &Path,
            name: &str,
            manifest: &str,
            tool: &str,
            stub: &str,
        ) {
            let conn = dir.join(name);
            std::fs::create_dir_all(&conn).unwrap();
            std::fs::write(conn.join("connector.toml"), manifest).unwrap();
            let script = conn.join("stub.py");
            std::fs::write(&script, stub).unwrap();
            let exe = conn.join("connector");
            // Quote the script path: fixture dirs may contain characters
            // (spaces, parens) that would otherwise break the shell.
            std::fs::write(
                &exe,
                format!("#!/bin/sh\nexec python3 \"{}\" {tool}\n", script.display()),
            )
            .unwrap();
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        fn new() -> Self {
            let guard = FIXTURE_LOCK.lock().unwrap();
            let n = FIXTURE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!(
                "supercli-core-conn-test-{}-{n}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self::write_connector(&dir, "asky", MANIFEST_ASK, "asky.echo");
            Self::write_connector(&dir, "allowy", MANIFEST_ALLOW, "allowy.echo");
            std::env::set_var("SUPERCLI_CONNECTORS_DIR", dir.as_os_str());
            std::env::set_var(CONNECTORS_KEYCHAIN_ENV, "memory");
            let session_dir = dir.join("session-1");
            std::fs::create_dir_all(&session_dir).unwrap();
            Self {
                dir,
                session_dir,
                _env_guard: guard,
            }
        }

        fn attach(&self, name: &str, policy: HashMap<String, ApprovalPolicy>) {
            supercli_connector::enable_attachment(&self.session_dir, name, policy).unwrap();
        }

        fn detach(&self, name: &str) {
            supercli_connector::disable_attachment(&self.session_dir, name).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
            std::env::remove_var("SUPERCLI_CONNECTORS_DIR");
            std::env::remove_var(CONNECTORS_KEYCHAIN_ENV);
        }
    }

    #[test]
    fn attached_connector_tools_are_listed() {
        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        fx.attach("asky", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        let defs = set.tool_definitions();
        let names: Vec<&str> = defs
            .iter()
            .filter_map(|d| d.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"allowy.echo"), "{names:?}");
        assert!(names.contains(&"asky.echo"), "{names:?}");
        let allowy = defs.iter().find(|d| d["name"] == "allowy.echo").unwrap();
        assert!(
            allowy["description"]
                .as_str()
                .unwrap()
                .contains("[connector allowy]"),
            "{allowy}"
        );
    }

    #[test]
    fn allow_tool_calls_through_and_audits() {
        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        let out = set
            .call_tool("allowy.echo", &json!({"msg": "hi"}))
            .expect("allow tool calls through");
        assert!(out.contains("echo:"), "{out}");
        assert!(out.contains("hi"), "{out}");
        // The audit log records the call with the connector name.
        let audit = std::fs::read_to_string(fx.session_dir.join(AUDIT_FILE)).unwrap();
        assert!(audit.contains("\"connector\":\"allowy\""), "{audit}");
        assert!(audit.contains("\"tool\":\"allowy.echo\""), "{audit}");
        assert!(audit.contains("\"ok\":true"), "{audit}");
        assert!(audit.contains("\"policy\":\"allow\""), "{audit}");
    }

    /// R2: a panic mid-tool-call must not take down the Host or leave the
    /// review in-flight. The panic is caught, the attempt is recorded as
    /// Ambiguous (external effect unknown), and the call fails.
    #[test]
    fn panic_mid_tool_call_records_ambiguous() {
        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);

        // Arm the hook: the connector call will panic.
        *EXECUTE_CALL_PANIC_HOOK.lock().unwrap() = true;
        let result = set.call_tool("allowy.echo", &json!({"msg": "hi"}));
        *EXECUTE_CALL_PANIC_HOOK.lock().unwrap() = false;

        // The call fails (does not crash the test process).
        let err = result.expect_err("panicking tool call must fail");
        assert!(err.contains("panicked"), "{err:?}");

        // The review is not left in-flight: an Ambiguous outcome was recorded.
        let inflight = crate::action_reviews::inflight_reviews(&fx.session_dir).expect("inflight");
        assert!(
            inflight.is_empty(),
            "panic must not leave in-flight: {inflight:?}"
        );

        // The chain verifies: review + Ambiguous outcome.
        let count =
            crate::action_reviews::verify_review_chain(&fx.session_dir).expect("chain verifies");
        assert_eq!(count, 2, "review + Ambiguous outcome");

        // The audit log marks the panic.
        let audit = std::fs::read_to_string(fx.session_dir.join(AUDIT_FILE)).unwrap();
        assert!(audit.contains("\"outcome\":\"panic\""), "{audit}");

        // R2 cascade: a NORMAL connector call on the same connector/session
        // afterwards must succeed. If any lock in the call path were
        // poisoned by the panic, this call would fail.
        let out = set
            .call_tool("allowy.echo", &json!({"msg": "after-panic"}))
            .expect("normal connector call after panic must succeed");
        assert!(out.contains("after-panic"), "{out}");
    }

    /// R5: end-to-end connector rate-limit test with a counting mock.
    ///
    /// Drives a real connector call through `call_tool_detailed` →
    /// `execute_call` (the production path) with a counting mock connector
    /// attached and the actor set to `human:<device>`. Exhausts the
    /// 120/min per-device connector bucket, then asserts:
    /// - the call fails with "rate limit exceeded" (never a tool result)
    /// - the mock connector was never invoked (call count exactly 0)
    /// - review-log bytes unchanged against a real canonical baseline
    /// - hash-chain length unchanged (`verify_review_chain`, unmasked)
    ///
    /// This proves the rate-limit check in `execute_call` fires before the
    /// review write and before the connector is invoked — and that the
    /// limiter is keyed to the authenticated device identity.
    #[test]
    fn connector_rate_limit_end_to_end_no_side_effects() {
        let fx = Fixture::new();
        // Counting mock: every tools/call appends one line to this file.
        let count_file = fx.dir.join("call-count.txt");
        std::env::set_var("SUPERCLI_COUNT_FILE", &count_file);
        Fixture::write_connector_with_stub(
            &fx.dir,
            "county",
            MANIFEST_COUNT_ALLOW,
            "county.echo",
            STUB_COUNTING,
        );
        fx.attach("county", HashMap::new());
        let mut set = SessionConnectors::resolve("session-conn-ratelimit", &fx.session_dir);
        let device = "e2e-conn-device-001";
        set.set_actor(format!("human:{device}"));

        // Real canonical baseline: one approved review through the
        // production writer, so the chain actually verifies.
        crate::action_reviews::record_review(
            &fx.session_dir,
            crate::action_reviews::Actor::Human {
                device_id: device.to_string(),
            },
            "county",
            "county.echo",
            "args-hash",
            crate::action_reviews::ReviewDecision::Approved,
            None,
        )
        .expect("baseline review");
        let review_log = fx.session_dir.join(crate::action_reviews::REVIEWS_FILE);
        let bytes_before = std::fs::read(&review_log).expect("read review log");
        let chain_before = crate::action_reviews::verify_review_chain(&fx.session_dir)
            .expect("baseline chain verifies");

        // Exhaust the 120/min connector bucket for this device.
        let limiter = crate::rate_limit::global();
        for _ in 0..120 {
            assert!(limiter.check(device, "connector"), "connector bucket fill");
        }

        // The real execution path: find_tool -> execute_call, whose FIRST
        // check is the per-device rate limit (before review write, before
        // the connector is invoked).
        let err = set
            .call_tool_detailed("county.echo", &json!({"msg": "hi"}))
            .expect_err("rate-limited connector call must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("rate limit exceeded"),
            "must fail closed on the rate limit: {msg}"
        );

        // The mock connector was never invoked: count is exactly 0.
        let calls = std::fs::read_to_string(&count_file)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        assert_eq!(calls, 0, "mock connector must never be invoked");

        // No side effects on the review log.
        let bytes_after = std::fs::read(&review_log).expect("read review log");
        assert_eq!(
            bytes_before, bytes_after,
            "review log bytes must be unchanged"
        );
        let chain_after = crate::action_reviews::verify_review_chain(&fx.session_dir)
            .expect("chain still verifies");
        assert_eq!(
            chain_before, chain_after,
            "hash chain length must be unchanged"
        );

        std::env::remove_var("SUPERCLI_COUNT_FILE");
    }

    #[test]
    fn autonomous_mode_denies_ask_tools_without_prompting() {
        let fx = Fixture::new();
        fx.attach("asky", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        set.set_autonomous(true);
        // Must fail fast: the interactive path would block up to 130 s on
        // the approval hub, which has no human in autonomous mode.
        let err = set
            .call_tool("asky.echo", &json!({"msg": "hi"}))
            .expect_err("ask tool denied in autonomous mode");
        assert!(err.contains("autonomous mode"), "{err}");
        assert!(err.contains("scheduled-session policy"), "{err}");
        // The denial is audited as policy=ask, approved=false so it is
        // distinguishable from an operator's explicit Deny.
        let audit = std::fs::read_to_string(fx.session_dir.join(AUDIT_FILE)).unwrap();
        assert!(audit.contains("\"tool\":\"asky.echo\""), "{audit}");
        assert!(audit.contains("\"policy\":\"ask\""), "{audit}");
        assert!(audit.contains("\"approved\":false"), "{audit}");
        assert!(audit.contains("\"ok\":false"), "{audit}");
    }

    #[test]
    fn autonomous_mode_still_allows_allow_tools() {
        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        set.set_autonomous(true);
        let out = set
            .call_tool("allowy.echo", &json!({"msg": "hi"}))
            .expect("allow tool still runs in autonomous mode");
        assert!(out.contains("hi"), "{out}");
    }

    #[test]
    fn replacement_is_refused_in_autonomous_mode() {
        // A replacement is a fresh external write: it must never fire
        // from a scheduled run, even for an allow-policy tool.
        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        set.set_autonomous(true);
        let err = set
            .call_tool_replacement("allowy.echo", &json!({"msg": "hi"}), "attempt-abc")
            .expect_err("replacement must be refused without a human");
        assert!(err.message.contains("attempt-abc"), "{err}");
        assert!(err.message.contains("autonomous"), "{err}");
    }

    #[test]
    fn replacement_routes_through_approval_and_audits_linkage() {
        // Even an allow-policy tool goes through the approval hub when it
        // is a replacement. With no frontend running the prompt fails
        // fast; the denial is audited with the original attempt id, the
        // arguments hash, and a non-retryable outcome.
        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        let err = set
            .call_tool_replacement("allowy.echo", &json!({"msg": "hi"}), "attempt-abc")
            .expect_err("no frontend: approval cannot complete");
        assert!(err.message.contains("approval"), "{err}");
        let audit = std::fs::read_to_string(fx.session_dir.join(AUDIT_FILE)).unwrap();
        let lines: Vec<&str> = audit.lines().collect();
        assert_eq!(lines.len(), 1, "{audit}");
        let entry: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(entry["tool"], "allowy.echo");
        assert_eq!(entry["replaces_attempt"], "attempt-abc");
        assert_eq!(entry["outcome"], "denied");
        assert_eq!(entry["retryable"], false);
        assert_eq!(entry["approved"], false);
        assert!(
            entry["args_hash"].as_str().is_some_and(|h| h.len() == 64),
            "{entry}"
        );
        assert!(
            entry["attempt_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty()),
            "{entry}"
        );
    }

    #[test]
    fn deny_policy_is_not_advertised_and_refuses() {
        let fx = Fixture::new();
        fx.attach(
            "allowy",
            HashMap::from([("allowy.echo".to_string(), ApprovalPolicy::Deny)]),
        );
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        let defs = set.tool_definitions();
        assert!(
            defs.iter().all(|d| d["name"] != "allowy.echo"),
            "deny tools are not advertised"
        );
        let err = set.call_tool("allowy.echo", &json!({})).unwrap_err();
        assert!(err.contains("denied"), "{err}");
        let audit = std::fs::read_to_string(fx.session_dir.join(AUDIT_FILE)).unwrap();
        assert!(audit.contains("\"ok\":false"), "{audit}");
    }

    #[test]
    fn detach_takes_effect_without_restart() {
        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        assert!(set.find_tool("allowy.echo").is_some());
        fx.detach("allowy");
        // Next call re-resolves: the tool is gone, the process dropped.
        assert!(set.find_tool("allowy.echo").is_none());
        assert!(set.live.is_empty());
        let err = set.call_tool("allowy.echo", &json!({})).unwrap_err();
        assert!(err.contains("Unknown tool"), "{err}");
    }

    #[test]
    fn corrupt_record_fails_closed() {
        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        std::fs::write(
            supercli_connector::attachments_path(&fx.session_dir),
            "{not json",
        )
        .unwrap();
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        assert!(set.tool_definitions().is_empty());
        let err = set.call_tool("allowy.echo", &json!({})).unwrap_err();
        assert!(err.contains("unreadable"), "{err}");
    }

    /// Stub MCP server that echoes the injected bearer token: argv[1] is
    /// the tool name it serves.
    const STUB_TOKEN_ECHO: &str = r#"import json, sys, os
TOOL = sys.argv[1]
TOKEN = os.environ.get("SUPERCLI_CONNECTOR_TOKEN", "")
TOOLS = [{"name": TOOL, "description": "echo it", "inputSchema": {"type": "object"}}]
def respond(mid, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": mid, "result": result}) + "\n")
    sys.stdout.flush()
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method, mid = msg.get("method"), msg.get("id")
    if method == "initialize":
        respond(mid, {"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "stub", "version": "0"}})
    elif method == "tools/list":
        respond(mid, {"tools": TOOLS})
    elif method == "tools/call":
        respond(mid, {"content": [{"type": "text", "text": "token=" + TOKEN}]})
"#;

    const MANIFEST_HTTP: &str = r#"
[connector]
name = "httpy"
version = "0.1.0"
display_name = "Httpy"
description = "mcp-http host test connector"
kind = "mcp-http"

[auth]
flow = "none"

[tools]
provides = ["httpy.echo"]

[policy]
"httpy.echo" = "allow"
"#;

    /// Minimal MCP-over-HTTP stub (plain JSON, no SSE). Serves exactly
    /// `n` requests; the test must make exactly `n` (a mismatch hangs
    /// the join, so the counts below are exact).
    fn serve_http_stub(n: usize) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming().take(n) {
                let mut stream = stream.unwrap();
                let mut raw = Vec::new();
                let mut buf = [0u8; 4096];
                let body: Value = loop {
                    let r = stream.read(&mut buf).unwrap_or(0);
                    if r == 0 {
                        break Value::Null;
                    }
                    raw.extend_from_slice(&buf[..r]);
                    if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&raw[..end]).to_string();
                        let len: usize = headers
                            .lines()
                            .find(|l| l.to_lowercase().starts_with("content-length:"))
                            .and_then(|l| l.split(':').nth(1))
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(0);
                        while raw.len() < end + 4 + len {
                            let r = stream.read(&mut buf).unwrap_or(0);
                            if r == 0 {
                                break;
                            }
                            raw.extend_from_slice(&buf[..r]);
                        }
                        break serde_json::from_slice(&raw[end + 4..]).unwrap_or(Value::Null);
                    }
                };
                let id = body.get("id").cloned().unwrap_or(Value::Null);
                let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("");
                let result = match method {
                    "initialize" => {
                        json!({"protocolVersion": "2024-11-05", "capabilities": {}})
                    }
                    "tools/list" => json!({"tools": [
                        {"name": "httpy.echo", "description": "echo",
                         "inputSchema": {"type": "object"}}
                    ]}),
                    "tools/call" => {
                        json!({"content": [{"type": "text", "text": "http-echo-ok"}]})
                    }
                    _ => json!({}),
                };
                // Notifications carry no id and get an empty body back.
                let payload = if body.get("id").is_some() {
                    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
                } else {
                    String::new()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (url, handle)
    }

    #[test]
    fn http_connector_lists_calls_and_detaches() {
        let fx = Fixture::new();
        // open: initialize + notify + tools/list = 3; one call = 1.
        let (url, handle) = serve_http_stub(4);
        let dir = fx.dir.join("httpy");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("connector.toml"), MANIFEST_HTTP).unwrap();
        std::fs::write(
            dir.join("config.json"),
            format!(r#"{{"mcp_url": {url:?}}}"#),
        )
        .unwrap();

        fx.attach("httpy", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        assert!(set.find_tool("httpy.echo").is_some());
        let out = set
            .call_tool("httpy.echo", &json!({}))
            .expect("http tool calls through");
        assert!(out.contains("http-echo-ok"), "{out}");

        // Detach takes effect without restarting the host: the next
        // re-resolve drops the tool and the HTTP link.
        fx.detach("httpy");
        assert!(set.find_tool("httpy.echo").is_none());
        assert!(set.live.is_empty());
        handle.join().unwrap();
    }

    /// Token-endpoint stub for the OAuth refresh test: answers one
    /// `grant_type=refresh_token` POST with a fresh access token.
    fn serve_token_stub() -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let r = stream.read(&mut buf).unwrap_or(0);
                if r == 0 {
                    break;
                }
                raw.extend_from_slice(&buf[..r]);
                if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&raw[..end]).to_string();
                    let len: usize = headers
                        .lines()
                        .find(|l| l.to_lowercase().starts_with("content-length:"))
                        .and_then(|l| l.split(':').nth(1))
                        .and_then(|v| v.trim().parse().ok())
                        .unwrap_or(0);
                    while raw.len() < end + 4 + len {
                        let r = stream.read(&mut buf).unwrap_or(0);
                        if r == 0 {
                            break;
                        }
                        raw.extend_from_slice(&buf[..r]);
                    }
                    break;
                }
            }
            let body = String::from_utf8_lossy(&raw).into_owned();
            assert!(
                body.contains("grant_type=refresh_token"),
                "expected a refresh grant, got: {body}"
            );
            let payload = r#"{"access_token":"fresh-access","refresh_token":"fresh-refresh","expires_in":3600}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(response.as_bytes());
        });
        (url, handle)
    }

    #[test]
    fn oauth_refresh_rotates_keychain_token() {
        let fx = Fixture::new();
        let (token_url, token_handle) = serve_token_stub();
        let manifest = format!(
            r#"
[connector]
name = "oauthy"
version = "0.1.0"
display_name = "Oauthy"
description = "oauth2 host test connector"
kind = "mcp-stdio"

[auth]
flow = "oauth2"

[oauth]
authorize_url = "http://127.0.0.1:1/unused"
token_url = "{token_url}"

[tools]
provides = ["oauthy.echo"]

[policy]
"oauthy.echo" = "allow"
"#
        );
        let dir = fx.dir.join("oauthy");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("connector.toml"), &manifest).unwrap();
        let script = dir.join("stub.py");
        std::fs::write(&script, STUB_TOKEN_ECHO).unwrap();
        let exe = dir.join("connector");
        std::fs::write(
            &exe,
            format!(
                "#!/bin/sh\nexec python3 \"{}\" oauthy.echo\n",
                script.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dir.join("config.json"), r#"{"client_id": "test-client"}"#).unwrap();

        // Store an already-expired token set: the host must refresh it
        // through token_url before the first call.
        let stale = supercli_connector::TokenSet {
            access_token: "stale-access".to_string(),
            refresh_token: Some("stale-refresh".to_string()),
            expires_at_unix: Some(supercli_connector::now_unix() - 60),
            token_url: Some(token_url.clone()),
            obtained_at_unix: supercli_connector::now_unix() - 7200,
        };
        let (store, _notice) = supercli_connector::open_connector_store();
        supercli_connector::store_connector_token(store.as_ref(), "oauthy", &stale.to_stored())
            .unwrap();

        fx.attach("oauthy", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        let out = set
            .call_tool("oauthy.echo", &json!({}))
            .expect("oauth tool calls through after refresh");
        // The refreshed token — not the stale one — reached the process.
        assert!(out.contains("token=fresh-access"), "{out}");
        assert!(!out.contains("stale-access"), "{out}");
        // And the rotated set was persisted back to the keychain.
        let stored = supercli_connector::load_connector_token(store.as_ref(), "oauthy")
            .unwrap()
            .unwrap();
        let rotated = supercli_connector::TokenSet::from_stored(&stored).unwrap();
        assert_eq!(rotated.access_token, "fresh-access");
        assert_eq!(rotated.refresh_token.as_deref(), Some("fresh-refresh"));
        token_handle.join().unwrap();
    }

    #[test]
    fn ask_tool_without_approver_errors_honestly() {
        let fx = Fixture::new();
        fx.attach("asky", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        // No app/serve running in tests: the approval prompt cannot
        // complete, and the error must say so instead of hanging.
        let err = set.call_tool("asky.echo", &json!({})).unwrap_err();
        assert!(err.contains("approval"), "{err}");
        let audit = std::fs::read_to_string(fx.session_dir.join(AUDIT_FILE)).unwrap();
        assert!(audit.contains("\"policy\":\"ask\""), "{audit}");
        assert!(audit.contains("\"approved\":false"), "{audit}");
    }

    // ---- P5-2 stored action reviews ----

    fn read_reviews(fx: &Fixture) -> Vec<Value> {
        let content =
            std::fs::read_to_string(fx.session_dir.join(crate::action_reviews::REVIEWS_FILE))
                .unwrap();
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .collect()
    }

    fn read_attempts(fx: &Fixture) -> Vec<Value> {
        let content = std::fs::read_to_string(fx.session_dir.join(AUDIT_FILE)).unwrap();
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .collect()
    }

    #[test]
    fn review_is_persisted_before_execution_and_attempt_references_it() {
        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        set.call_tool("allowy.echo", &json!({"msg": "hi"}))
            .expect("allow tool calls through");

        let reviews = read_reviews(&fx);
        // The durable log holds both the review and its terminal outcome
        // entry; the review itself appears exactly once.
        let review_entries: Vec<&Value> = reviews
            .iter()
            .filter(|v| v.get("type").and_then(Value::as_str) != Some("attempt_outcome"))
            .collect();
        assert_eq!(review_entries.len(), 1, "{reviews:?}");
        let review = review_entries[0];
        assert_eq!(review["decision"], "approved");
        assert_eq!(review["tool"], "allowy.echo");
        assert_eq!(review["connector"], "allowy");
        assert_eq!(review["actor"], "policy:allow");
        assert!(review["entry_hash"].as_str().unwrap().len() == 64);
        let review_id = review["review_id"].as_str().unwrap().to_string();

        // The outcome entry in the same log references the review.
        let outcome_entries: Vec<&Value> = reviews
            .iter()
            .filter(|v| v.get("type").and_then(Value::as_str) == Some("attempt_outcome"))
            .collect();
        assert_eq!(outcome_entries.len(), 1, "{reviews:?}");
        assert_eq!(outcome_entries[0]["review_id"], review_id);
        assert_eq!(outcome_entries[0]["outcome"], "executed");

        // The attempt references the review that authorized it.
        let attempts = read_attempts(&fx);
        assert_eq!(attempts.len(), 1, "{attempts:?}");
        assert_eq!(attempts[0]["review_id"], review_id);
        assert_eq!(attempts[0]["outcome"], "ok");

        // The chain verifies over both entries (review + its outcome).
        assert_eq!(
            crate::action_reviews::verify_review_chain(&fx.session_dir).unwrap(),
            2
        );
    }

    #[test]
    fn review_write_failure_fails_closed_and_never_executes() {
        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        // Make the review log unopenable: a directory at the log path.
        std::fs::create_dir(fx.session_dir.join(crate::action_reviews::REVIEWS_FILE)).unwrap();
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        let err = set
            .call_tool("allowy.echo", &json!({"msg": "hi"}))
            .expect_err("review write must fail closed");
        // Distinct error: the tool never ran.
        assert!(err.contains("action review failed"), "{err}");
        assert!(err.contains("failing closed"), "{err}");
        // No attempt was recorded or executed: the audit log was never created.
        assert!(
            !fx.session_dir.join(AUDIT_FILE).exists(),
            "no attempt may exist without its review"
        );
    }

    #[test]
    fn deny_decisions_record_denied_reviews_with_explicit_actor() {
        let fx = Fixture::new();
        fx.attach("asky", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        set.set_autonomous(true);
        set.set_actor("scheduled:nightly-backup".to_string());
        let err = set
            .call_tool("asky.echo", &json!({}))
            .expect_err("ask denied in autonomous mode");
        assert!(err.contains("autonomous mode"), "{err}");

        let reviews = read_reviews(&fx);
        assert_eq!(reviews.len(), 1, "{reviews:?}");
        assert_eq!(reviews[0]["decision"], "denied");
        assert_eq!(reviews[0]["actor"], "scheduled:nightly-backup");
        assert_eq!(reviews[0]["tool"], "asky.echo");
        // The denied attempt references the denied review.
        let attempts = read_attempts(&fx);
        assert_eq!(attempts.len(), 1, "{attempts:?}");
        assert_eq!(attempts[0]["review_id"], reviews[0]["review_id"]);
        assert_eq!(attempts[0]["outcome"], "denied");
    }

    #[test]
    fn autonomous_denial_without_actor_is_still_explicit() {
        let fx = Fixture::new();
        fx.attach("asky", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        set.set_autonomous(true);
        // No set_actor: the actor must still be explicit, never empty.
        set.call_tool("asky.echo", &json!({})).unwrap_err();
        let reviews = read_reviews(&fx);
        assert_eq!(reviews.len(), 1);
        let actor = reviews[0]["actor"].as_str().unwrap();
        assert!(!actor.is_empty(), "actor must never be empty");
        assert_eq!(actor, "scheduled:unknown-trigger");
    }

    #[test]
    fn kill_between_review_and_call_leaves_review_without_attempt() {
        // Simulates a crash after the write-ahead review but before the
        // tool executes: the review exists durably, no attempt references
        // it. The reverse (attempt without review) is impossible by
        // construction — execute_call records the review before
        // link.call_detailed and fails closed on any write error.
        let fx = Fixture::new();
        let entry = crate::action_reviews::record_review(
            &fx.session_dir,
            crate::action_reviews::Actor::PolicyAllow,
            "allowy",
            "allowy.echo",
            "deadbeef",
            crate::action_reviews::ReviewDecision::Approved,
            None,
        )
        .expect("review records");
        // "Crash" here: no call, no attempt.
        assert!(!fx.session_dir.join(AUDIT_FILE).exists());
        let reviews = read_reviews(&fx);
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0]["review_id"], entry.review_id);
        assert_eq!(
            crate::action_reviews::verify_review_chain(&fx.session_dir).unwrap(),
            1
        );
    }

    #[test]
    fn stale_lease_fence_refuses_before_any_side_effect() {
        // The required fencing scenario: A claims gen 1 and stalls past
        // expiry; B takes over at gen 2. When A wakes, every one of its
        // tool calls is refused with the distinct stale-lease error
        // BEFORE the review write — so no review is recorded, no attempt
        // is audited, and the connector is never called.
        use crate::schedule_leases::{LeaseFence, ScheduleLeases, DEFAULT_TENANT};
        use crate::scheduled::ScheduledToolExecutor;
        use std::sync::{Arc, Mutex};

        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        let lease_home =
            std::env::temp_dir().join(format!("supercli-fence-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&lease_home);
        std::fs::create_dir_all(&lease_home).unwrap();

        // A claims gen 1 and starts a run with its fence installed.
        let leases = ScheduleLeases::open(&lease_home, DEFAULT_TENANT).unwrap();
        let info_a = leases.claim("sched").unwrap().expect("A claims gen 1");
        assert_eq!(info_a.generation, 1);
        let shared = Arc::new(Mutex::new(leases));
        let fence_a = LeaseFence::new(Arc::clone(&shared), "sched".to_string(), 1);
        assert!(fence_a.is_current(), "A's fence is current before expiry");

        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        set.set_autonomous(true);
        set.set_actor("scheduled:sched".to_string());
        set.set_lease_fence(Some(fence_a));

        // B takes over the lapsed lease at gen 2 (same process, distinct
        // worker identity — owner ids are per store, not per pid).
        shared.lock().unwrap().force_expire("sched").unwrap();
        let info_b = shared
            .lock()
            .unwrap()
            .claim("sched")
            .unwrap()
            .expect("B takes over");
        assert_eq!(info_b.generation, 2);
        assert_ne!(info_a.generation, info_b.generation);

        // A's call is refused before the write-ahead review: the tool
        // provably did not execute, so this is a clean refusal, not an
        // uncertain outcome.
        let err = set
            .call_tool_detailed("allowy.echo", &json!({"msg": "hi"}))
            .expect_err("stale worker must be refused");
        assert!(err.stale_lease, "distinct stale-lease error: {err:?}");
        assert!(!err.uncertain, "not an uncertain outcome: {err:?}");

        // No side effects whatsoever: no review, no attempt, and the
        // stub connector never ran (it would have appended to the audit
        // log on any outcome).
        assert!(
            !fx.session_dir
                .join(crate::action_reviews::REVIEWS_FILE)
                .exists(),
            "no review may be written by a stale worker"
        );
        assert!(
            !fx.session_dir.join(AUDIT_FILE).exists(),
            "no attempt may be audited by a stale worker"
        );
        let _ = std::fs::remove_dir_all(&lease_home);
    }

    /// S3: the fence is current when the call starts (passes the
    /// pre-review check) but goes stale after the write-ahead review is
    /// durably recorded and before any tool-call bytes are sent. The
    /// recorded outcome must be `NeverRan` — the tool provably never ran —
    /// never `Ambiguous`.
    ///
    /// Deterministic interleaving: a helper thread holds an exclusive
    /// `flock` on the review-log lockfile (the same lock `LogLock`
    /// takes), so the tool call blocks inside `record_review` after
    /// passing the pre-review fence check. The main thread then
    /// force-expires the lease and releases the flock; the review writes
    /// and the post-review fence check observes the stale fence.
    #[test]
    fn stale_lease_between_review_and_call_records_never_ran() {
        use crate::schedule_leases::{LeaseFence, ScheduleLeases, DEFAULT_TENANT};
        use crate::scheduled::ScheduledToolExecutor;
        use std::sync::{mpsc, Arc, Mutex};

        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        let lease_home =
            std::env::temp_dir().join(format!("supercli-neverran-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&lease_home);
        std::fs::create_dir_all(&lease_home).unwrap();

        let leases = ScheduleLeases::open(&lease_home, DEFAULT_TENANT).unwrap();
        let info = leases.claim("sched").unwrap().expect("claim gen 1");
        let shared = Arc::new(Mutex::new(leases));
        let fence = LeaseFence::new(Arc::clone(&shared), "sched".to_string(), info.generation);
        assert!(fence.is_current(), "fence current before the call");

        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);
        set.set_autonomous(true);
        set.set_actor("scheduled:sched".to_string());
        set.set_lease_fence(Some(fence));

        // Helper thread: hold the review-log flock until told to release.
        let lock_path = fx
            .session_dir
            .join(format!("{}.lock", crate::action_reviews::REVIEWS_FILE));
        let (locked_tx, locked_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let holder = std::thread::spawn(move || {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&lock_path)
                .expect("open lockfile");
            use std::os::unix::io::AsRawFd;
            // SAFETY: `flock` on our own open fd; no pointer arguments.
            let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            assert_eq!(rc, 0, "helper takes the log flock");
            locked_tx.send(()).expect("signal locked");
            release_rx.recv().expect("wait for release");
            // `file` drops here, releasing the flock.
        });
        locked_rx.recv().expect("helper holds the flock");

        // Run the tool call in another thread: it passes the pre-review
        // fence check (fence still current) then blocks in
        // `record_review`'s `LogLock` acquisition.
        let (done_tx, done_rx) = mpsc::channel();
        let caller = std::thread::spawn(move || {
            let err = set
                .call_tool_detailed("allowy.echo", &json!({"msg": "hi"}))
                .expect_err("stale worker must be refused");
            done_tx.send(err).expect("report result");
        });
        // Let the caller reach the log lock. Everything before the lock
        // is a fast in-memory fence check; 1s is a generous bound, and a
        // miss fails loudly below (no review written) rather than
        // silently passing.
        std::thread::sleep(std::time::Duration::from_secs(1));
        // Expire the lease, then release the flock: the review writes,
        // and the post-review fence check observes the stale fence.
        shared.lock().unwrap().force_expire("sched").unwrap();
        release_tx.send(()).expect("release the flock");

        let err: ToolCallFailure = done_rx
            .recv_timeout(std::time::Duration::from_secs(35))
            .expect("tool call completes");
        caller.join().expect("caller thread joins");
        holder.join().expect("holder thread joins");

        assert!(err.stale_lease, "distinct stale-lease error: {err:?}");
        assert!(!err.uncertain, "not an uncertain outcome: {err:?}");

        // The review was written (pre-review check passed), and its
        // outcome is `never_ran` — the tool provably never ran — not
        // `ambiguous`.
        let entries = read_reviews(&fx);
        assert_eq!(entries.len(), 2, "review + outcome: {entries:?}");
        assert!(
            entries[0].get("review_id").is_some() && entries[0].get("type").is_none(),
            "first line is the review: {}",
            entries[0]
        );
        assert_eq!(entries[1]["type"], "attempt_outcome");
        assert_eq!(entries[1]["outcome"], "never_ran");
        assert!(
            entries[1].get("reason").and_then(|r| r.as_str()).is_some(),
            "NeverRan carries a reason: {}",
            entries[1]
        );
        // The stub connector never ran: it would have appended to the
        // audit log on any outcome.
        assert!(
            !fx.session_dir.join(AUDIT_FILE).exists(),
            "no attempt may be audited for a call that never ran"
        );
        // Hash chain intact over review + outcome.
        assert_eq!(
            crate::action_reviews::verify_review_chain(&fx.session_dir).unwrap(),
            2
        );
        let _ = std::fs::remove_dir_all(&lease_home);
    }

    /// F1: a normal successful tool call writes its terminal `Executed`
    /// outcome record linked to the review id, fires the outcome listener
    /// (which the Host uses to emit `tool.executed`), and leaves no
    /// in-flight review behind — so a later takeover proceeds without
    /// `needs_review`.
    #[test]
    fn normal_success_records_executed_outcome_and_clears_inflight() {
        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        let mut set = SessionConnectors::resolve("session-1", &fx.session_dir);

        // Capture listener notifications: (review_id, outcome).
        let notified = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let notified2 = notified.clone();
        set.set_outcome_listener(Box::new(move |review_id, outcome| {
            notified2
                .lock()
                .unwrap()
                .push((review_id.to_string(), outcome.clone()));
        }));

        let text = set
            .call_tool("allowy.echo", &json!({"msg": "hi"}))
            .expect("allow-policy tool succeeds");
        assert!(text.contains("echo:"), "{text}");

        // The durable outcome record exists, linked to the review.
        let log_path = fx.session_dir.join(crate::action_reviews::REVIEWS_FILE);
        let contents = std::fs::read_to_string(&log_path).expect("review log exists");
        let mut review_id: Option<String> = None;
        let mut saw_executed = false;
        for line in contents.lines().filter(|l| !l.trim().is_empty()) {
            let v: serde_json::Value = serde_json::from_str(line).expect("valid JSON");
            if v.get("type").and_then(serde_json::Value::as_str) == Some("attempt_outcome") {
                assert_eq!(
                    v.get("outcome").and_then(serde_json::Value::as_str),
                    Some("executed")
                );
                assert_eq!(
                    v.get("success").and_then(serde_json::Value::as_bool),
                    Some(true)
                );
                saw_executed = true;
                review_id = v
                    .get("review_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
            }
        }
        assert!(saw_executed, "an Executed outcome record must exist");
        let review_id = review_id.expect("outcome carries the review id");

        // The listener fired exactly once, with the same review id and
        // outcome — this is what the Host turns into `tool.executed`.
        let calls = notified.lock().unwrap();
        assert_eq!(calls.len(), 1, "listener fires once per terminal outcome");
        assert_eq!(calls[0].0, review_id);
        assert!(
            matches!(
                calls[0].1,
                crate::action_reviews::AttemptOutcome::Executed { success: true }
            ),
            "listener sees Executed(success)"
        );
        drop(calls);

        // Takeover check: no in-flight reviews remain, so a new worker
        // takes over without needs_review.
        let inflight =
            crate::action_reviews::inflight_reviews(&fx.session_dir).expect("inflight scan works");
        assert!(
            inflight.is_empty(),
            "a normally completed run must not look in-flight: {inflight:?}"
        );

        // The hash chain still verifies with the outcome appended.
        crate::action_reviews::verify_review_chain(&fx.session_dir)
            .expect("chain verifies after outcome");
    }

    /// Read the review log JSONL and return the outcome strings recorded.
    fn review_outcomes(session_dir: &Path) -> Vec<String> {
        let log = std::fs::read_to_string(session_dir.join(crate::action_reviews::REVIEWS_FILE))
            .expect("review log readable");
        log.lines()
            .filter_map(|line| {
                let v: serde_json::Value = serde_json::from_str(line).ok()?;
                if v.get("type").and_then(|t| t.as_str()) != Some("attempt_outcome") {
                    return None;
                }
                v.get("outcome")
                    .and_then(|o| o.as_str())
                    .map(|s| s.to_string())
            })
            .collect()
    }

    #[test]
    fn before_execute_hook_fires_with_tool_context() {
        let fx = Fixture::new();
        fx.attach("allowy", HashMap::new());
        let mut set = SessionConnectors::resolve("session-hook-ctx", &fx.session_dir);
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, String)>::new()));
        let seen_clone = seen.clone();
        set.set_before_execute_hook(Box::new(move |ctx: &BeforeExecuteContext| {
            seen_clone
                .lock()
                .unwrap()
                .push((ctx.tool.clone(), ctx.actor.clone()));
            BeforeExecuteDecision::Allow
        }));
        set.set_actor("human:test-device".to_string());
        let out = set
            .call_tool("allowy.echo", &json!({"msg": "hi"}))
            .expect("allow hook lets the tool run");
        assert!(out.contains("echo:"), "{out}");
        let calls = seen.lock().unwrap();
        assert_eq!(calls.len(), 1, "hook fires exactly once per tool call");
        assert_eq!(calls[0].0, "allowy.echo");
        assert_eq!(calls[0].1, "human:test-device");
    }

    #[test]
    fn before_execute_reject_blocks_execution_and_records_never_ran() {
        let fx = Fixture::new();
        // Counting mock: proves the connector is never invoked.
        let count_file = fx.dir.join("hook-reject-count.txt");
        std::env::set_var("SUPERCLI_COUNT_FILE", &count_file);
        Fixture::write_connector_with_stub(
            &fx.dir,
            "county",
            MANIFEST_COUNT_ALLOW,
            "county.echo",
            STUB_COUNTING,
        );
        fx.attach("county", HashMap::new());
        let mut set = SessionConnectors::resolve("session-hook-reject", &fx.session_dir);
        set.set_before_execute_hook(Box::new(|_ctx: &BeforeExecuteContext| {
            BeforeExecuteDecision::Reject("test policy says no".to_string())
        }));
        let err = set
            .call_tool_detailed("county.echo", &json!({"msg": "hi"}))
            .expect_err("rejected tool call must fail");
        assert!(
            err.message.contains("rejected by before_execute hook"),
            "error names the hook: {}",
            err.message
        );
        assert!(
            err.message.contains("test policy says no"),
            "error carries the hook reason: {}",
            err.message
        );
        // The mock connector was never invoked: zero tool bytes sent.
        let calls = std::fs::read_to_string(&count_file)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        assert_eq!(calls, 0, "rejected tool must never execute");
        // The outcome is NeverRan (terminal), not in-flight.
        let outcomes = review_outcomes(&fx.session_dir);
        assert!(
            outcomes.iter().any(|o| o == "never_ran"),
            "rejection records never_ran: {outcomes:?}"
        );
        let inflight =
            crate::action_reviews::inflight_reviews(&fx.session_dir).expect("inflight scan works");
        assert!(
            inflight.is_empty(),
            "rejected call must not look in-flight: {inflight:?}"
        );
        std::env::remove_var("SUPERCLI_COUNT_FILE");
    }

    /// One-shot loopback stub for the approval bridge
    /// (`/mcp/approve-connector`). Answers the next approval POST with
    /// `body`, then closes. Returns the port to advertise via
    /// `SUPERCLI_APP_PORT`.
    fn approval_stub(body: &'static str) -> u16 {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("local addr").port();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            // Drain the request headers so the client sees a clean reply.
            let mut request = [0u8; 8192];
            let mut seen = 0;
            while seen < request.len() {
                let n = stream.read(&mut request[seen..]).unwrap_or(0);
                if n == 0 {
                    break;
                }
                seen += n;
                if request[..seen].windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        });
        port
    }

    /// Point the approval bridge and the supercli home at the fixture, so
    /// the escalated prompt hits [`approval_stub`] and no real frontend.
    /// Env vars are removed by the caller at the end of the test; the
    /// fixture lock serializes all of this.
    fn use_approval_stub(fx: &Fixture, port: u16) {
        std::env::set_var("SUPERCLI_APP_PORT", port.to_string());
        std::env::set_var("SUPERCLI_HOME", &fx.dir);
    }

    fn clear_approval_stub() {
        std::env::remove_var("SUPERCLI_APP_PORT");
        std::env::remove_var("SUPERCLI_HOME");
    }

    /// Read the actor recorded on the attempt_outcome entries.
    fn review_outcome_actors(session_dir: &std::path::Path) -> Vec<String> {
        let log = std::fs::read_to_string(session_dir.join(crate::action_reviews::REVIEWS_FILE))
            .expect("review log readable");
        log.lines()
            .filter_map(|line| {
                let v: serde_json::Value = serde_json::from_str(line).ok()?;
                if v.get("type").and_then(|t| t.as_str()) != Some("attempt_outcome") {
                    return None;
                }
                v.get("actor").and_then(|a| a.as_str()).map(|s| s.to_string())
            })
            .collect()
    }

    #[test]
    fn before_execute_escalate_on_allow_routes_to_approval_and_runs_once_on_approve() {
        let fx = Fixture::new();
        // The user approves the escalated prompt.
        let port = approval_stub(r#"{"approved": true, "answered_by": "test-device"}"#);
        use_approval_stub(&fx, port);
        let count_file = fx.dir.join("hook-escalate-approve-count.txt");
        std::env::set_var("SUPERCLI_COUNT_FILE", &count_file);
        Fixture::write_connector_with_stub(
            &fx.dir,
            "county",
            MANIFEST_COUNT_ALLOW,
            "county.echo",
            STUB_COUNTING,
        );
        fx.attach("county", HashMap::new());
        let mut set = SessionConnectors::resolve("session-hook-escalate-approve", &fx.session_dir);
        set.set_before_execute_hook(Box::new(|_ctx: &BeforeExecuteContext| {
            BeforeExecuteDecision::Escalate("test wants a human to look".to_string())
        }));
        // Escalate means Allow -> Ask: the approval card appears, the user
        // approves, and the tool runs exactly once.
        let out = set
            .call_tool_detailed("county.echo", &json!({"msg": "hi"}))
            .expect("approved escalated call must run");
        assert!(out.contains("echo:"), "{out}");
        let calls = std::fs::read_to_string(&count_file)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        assert_eq!(calls, 1, "approved escalated tool must run exactly once");
        let outcomes = review_outcomes(&fx.session_dir);
        assert!(
            outcomes.iter().any(|o| o == "executed"),
            "approved escalation records executed: {outcomes:?}"
        );
        let actors = review_outcome_actors(&fx.session_dir);
        assert!(
            actors.iter().any(|a| a == "human:test-device"),
            "the human approver is the effective authorizer: {actors:?}"
        );
        let inflight =
            crate::action_reviews::inflight_reviews(&fx.session_dir).expect("inflight scan works");
        assert!(inflight.is_empty(), "no in-flight reviews remain: {inflight:?}");
        std::env::remove_var("SUPERCLI_COUNT_FILE");
        clear_approval_stub();
    }

    #[test]
    fn before_execute_escalate_on_allow_denied_by_user_does_not_run() {
        let fx = Fixture::new();
        // The user declines the escalated prompt.
        let port = approval_stub(r#"{"approved": false, "answered_by": "test-device"}"#);
        use_approval_stub(&fx, port);
        let count_file = fx.dir.join("hook-escalate-deny-count.txt");
        std::env::set_var("SUPERCLI_COUNT_FILE", &count_file);
        Fixture::write_connector_with_stub(
            &fx.dir,
            "county",
            MANIFEST_COUNT_ALLOW,
            "county.echo",
            STUB_COUNTING,
        );
        fx.attach("county", HashMap::new());
        let mut set = SessionConnectors::resolve("session-hook-escalate-deny", &fx.session_dir);
        set.set_before_execute_hook(Box::new(|_ctx: &BeforeExecuteContext| {
            BeforeExecuteDecision::Escalate("test wants a human to look".to_string())
        }));
        let err = set
            .call_tool_detailed("county.echo", &json!({"msg": "hi"}))
            .expect_err("declined escalated call must not run");
        assert!(
            err.message.contains("declined"),
            "error reports the decline: {}",
            err.message
        );
        let calls = std::fs::read_to_string(&count_file)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        assert_eq!(calls, 0, "declined escalated tool must never execute");
        let outcomes = review_outcomes(&fx.session_dir);
        assert!(
            outcomes.iter().any(|o| o == "never_ran"),
            "declined escalation records never_ran: {outcomes:?}"
        );
        let actors = review_outcome_actors(&fx.session_dir);
        assert!(
            actors.iter().any(|a| a == "human:test-device"),
            "the decliner is recorded: {actors:?}"
        );
        let inflight =
            crate::action_reviews::inflight_reviews(&fx.session_dir).expect("inflight scan works");
        assert!(
            inflight.is_empty(),
            "declined call must not look in-flight: {inflight:?}"
        );
        std::env::remove_var("SUPERCLI_COUNT_FILE");
        clear_approval_stub();
    }
}
