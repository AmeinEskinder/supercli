//! Approval gating for mutating git/file routes.
//!
//! The mutating `/mobile/git/*` routes (stage, unstage, commit, fetch, pull,
//! push) and `/mobile/files/write` are dangerous: a paired Controller could
//! use them for persistent code execution. They are gated through the Host's
//! [`ApprovalHub`], following the same pattern as `devices.rs` (HubGate ->
//! ApprovalHub):
//!
//! 1. `before_*` lifecycle event on the `supercli-events` bus (`FileWrite` /
//!    `GitOp` doctypes). Hooks can reject or escalate; fail-closed with 2s
//!    timeout. A rejection → 403, nothing executes.
//! 2. `ApprovalHub.request()` creates a `PendingApproval`, answered through
//!    the existing approval flow (desktop approval panel, mobile). Blocks
//!    until answered or timeout (120s). Deny/timeout/no-hub → 403, nothing
//!    executes.
//! 3. Only after an explicit Allow does the core route handler execute.
//! 4. `after_*` observer event emitted exactly once on success (audited via
//!    the events bus, visible in `supercli hooks trace`).
//!
//! This replaces the previous `{"approved": true}` request-body flag, which
//! was caller self-approval (zero protection): any paired Controller could
//! just send the flag. The ApprovalHub requires a human answer through the
//! established approval UI.

use std::sync::Arc;
use std::time::Duration;

use supercli_core::controller_api::ControllerRequest;

use crate::approvals::ApprovalHub;

/// Mutating routes that require human approval. (method, path) pairs.
const MUTATING_ROUTES: &[(&str, &str)] = &[
    ("POST", "/mobile/git/stage"),
    ("POST", "/mobile/git/unstage"),
    ("POST", "/mobile/git/commit"),
    ("POST", "/mobile/git/fetch"),
    ("POST", "/mobile/git/pull"),
    ("POST", "/mobile/git/push"),
    ("POST", "/mobile/files/write"),
];

/// Returns true if this request targets a mutating git/file route.
pub fn is_mutating_git_route(method: &str, path: &str) -> bool {
    MUTATING_ROUTES
        .iter()
        .any(|(m, p)| *m == method && *p == path)
}

/// Approval timeout for git/file operations (matches devices.rs).
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);

/// Check approval for a mutating git/file route.
///
/// Returns `Ok(())` if the operation may proceed (human allowed via
/// ApprovalHub, and no `before_*` hook rejected). Returns `Err((status,
/// body))` with 403 (denied/rejected/timeout) or 503 (hub unavailable).
///
/// This is called from `mobile.rs` BEFORE `controller_api::route_with_effects`.
/// On `Ok`, the caller proceeds to the core route handler; on success the
/// caller must invoke [`emit_after`] exactly once.
pub fn check_git_approval(
    request: &ControllerRequest,
    session_id: &str,
    hub: &Arc<ApprovalHub>,
) -> Result<(), (u16, serde_json::Value)> {
    let (doctype, op_name) = classify(request);

    // 1. before_* hook (fail-closed, 2s timeout via the events dispatcher).
    //    FileWrite for /mobile/files/write, GitOp for /mobile/git/*.
    let doc_id = format!("gitop-{}", request.id.as_deref().unwrap_or("unknown"));
    let doc = serde_json::json!({
        "op": op_name,
        "path": request.path,
        "body": request.body,
    });
    let audit_dir = supercli_core::app_paths::supercli_home();
    if let Some(outcome) = supercli_events::emit::emit_sync(
        doctype,
        supercli_events::DocEvent::BeforeValidate,
        &doc_id,
        doc,
        &audit_dir,
        "human:host",
    ) {
        if outcome.decision == supercli_events::HookDecision::Reject {
            let reason = outcome
                .reject_reason
                .unwrap_or_else(|| "hook rejected".to_string());
            return Err((
                403,
                serde_json::json!({ "error": format!("{op_name} rejected by hook: {reason}") }),
            ));
        }
    }

    // 2. Human approval via ApprovalHub (blocks until answered or timeout).
    //    Follows the devices.rs pattern: HubGate -> ApprovalHub.request().
    //    The hub is passed in from mobile.rs (not the global, which is
    //    for the device routes).
    let title = format!("{op_name} requested by paired controller");
    let body = format!(
        "Operation: {op_name}\nPath: {}\nSession: {session_id}",
        request.path
    );
    // request() blocks on the channel until a human answers via the desktop
    // approval panel or mobile, or until APPROVAL_TIMEOUT expires (fail-closed).
    let (approved, _answered_by) = hub.request(
        "git-file-op",
        title,
        body,
        session_id.to_string(),
        None,
        APPROVAL_TIMEOUT,
    );
    if !approved {
        return Err((
            403,
            serde_json::json!({ "error": format!("{op_name} denied: approval required (denied, timed out, or no human present)") }),
        ));
    }

    Ok(())
}

/// Emit the `after_*` observer event exactly once after a successful
/// mutating operation. Call only when the core handler returned 200.
pub fn emit_after(request: &ControllerRequest) {
    let (doctype, op_name) = classify(request);
    let doc_id = format!("gitop-{}", request.id.as_deref().unwrap_or("unknown"));
    let doc = serde_json::json!({
        "op": op_name,
        "path": request.path,
        "status": "ok",
    });
    let audit_dir = supercli_core::app_paths::supercli_home();
    supercli_events::emit::emit_observer(
        doctype,
        supercli_events::DocEvent::AfterInsert,
        &doc_id,
        doc,
        &audit_dir,
        "human:host",
    );
}

/// Classify a request into (doctype, op_name).
fn classify(request: &ControllerRequest) -> (supercli_events::DocType, &'static str) {
    match request.path.as_str() {
        "/mobile/files/write" => (supercli_events::DocType::FileWrite, "file write"),
        "/mobile/git/stage" => (supercli_events::DocType::GitOp, "git stage"),
        "/mobile/git/unstage" => (supercli_events::DocType::GitOp, "git unstage"),
        "/mobile/git/commit" => (supercli_events::DocType::GitOp, "git commit"),
        "/mobile/git/fetch" => (supercli_events::DocType::GitOp, "git fetch"),
        "/mobile/git/pull" => (supercli_events::DocType::GitOp, "git pull"),
        "/mobile/git/push" => (supercli_events::DocType::GitOp, "git push"),
        _ => (supercli_events::DocType::GitOp, "git op"),
    }
}

// Re-export for tests.
#[cfg(test)]
pub(crate) fn test_classify(
    request: &ControllerRequest,
) -> (supercli_events::DocType, &'static str) {
    classify(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_request(path: &str) -> ControllerRequest {
        ControllerRequest {
            id: Some("test-123".to_string()),
            method: "POST".to_string(),
            path: path.to_string(),
            query: std::collections::HashMap::new(),
            body: serde_json::json!({}),
            content_type: None,
            body_base64: None,
            principal: supercli_core::controller_api::ControllerPrincipal::PairedDevice {
                device_id: "test-device".to_string(),
                name: "test".to_string(),
                principal_id: None,
            },
        }
    }

    #[test]
    fn mutating_routes_are_recognized() {
        // All seven mutating routes must be gated.
        for path in [
            "/mobile/git/stage",
            "/mobile/git/unstage",
            "/mobile/git/commit",
            "/mobile/git/fetch",
            "/mobile/git/pull",
            "/mobile/git/push",
            "/mobile/files/write",
        ] {
            assert!(
                is_mutating_git_route("POST", path),
                "{path} should require approval"
            );
        }
        // Non-mutating routes must NOT be gated.
        assert!(!is_mutating_git_route("GET", "/mobile/git/status"));
        assert!(!is_mutating_git_route("POST", "/mobile/git/status"));
        assert!(!is_mutating_git_route("POST", "/mobile/files/read"));
        assert!(!is_mutating_git_route("GET", "/mobile/bootstrap"));
    }

    #[test]
    fn classify_assigns_correct_doctypes() {
        let req = test_request("/mobile/files/write");
        let (dt, op) = test_classify(&req);
        assert_eq!(dt, supercli_events::DocType::FileWrite);
        assert_eq!(op, "file write");

        let req = test_request("/mobile/git/push");
        let (dt, op) = test_classify(&req);
        assert_eq!(dt, supercli_events::DocType::GitOp);
        assert_eq!(op, "git push");
    }

    /// Push blocks until the ApprovalHub is answered (no auto-approve).
    /// Deny -> 403. This proves the human gate is real, not caller-supplied.
    #[test]
    fn push_blocks_until_answered_deny_gives_403() {
        let hub = Arc::new(ApprovalHub::default());
        let req = test_request("/mobile/git/push");

        // Spawn the check in a thread (it blocks on hub.request()).
        let hub_clone = hub.clone();
        let handle =
            std::thread::spawn(move || check_git_approval(&req, "test-session", &hub_clone));

        // Wait for the approval to be queued.
        let approval_id = {
            let mut id = None;
            for _ in 0..50 {
                let pending = hub.list_json();
                if let Some(first) = pending.first() {
                    if let Some(s) = first.get("id").and_then(|v| v.as_str()) {
                        id = Some(s.to_string());
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            id.expect("approval should be queued")
        };

        // The check must still be blocked (not auto-approved).
        assert!(
            !handle.is_finished(),
            "check_git_approval must block until a human answers"
        );

        // Deny from the "human" side.
        let outcome = hub.answer(
            &approval_id,
            false,
            Some("test-human".to_string()),
            "nonce-1",
        );
        assert!(
            matches!(
                outcome,
                crate::approvals::AnswerOutcome::Applied(_)
                    | crate::approvals::AnswerOutcome::AlreadyResolved(_)
            ),
            "answer should apply, got {outcome:?}"
        );

        // The blocked check must now return 403.
        let result = handle.join().expect("thread panicked");
        match result {
            Err((403, body)) => {
                let err = body.get("error").and_then(|v| v.as_str()).unwrap_or("");
                assert!(
                    err.contains("denied") || err.contains("approval"),
                    "403 body should mention denial, got: {err}"
                );
            }
            other => panic!("expected 403 on deny, got {other:?}"),
        }
    }

    /// Allow -> Ok(()). The operation may proceed only after explicit Allow.
    #[test]
    fn push_allow_permits_execution() {
        let hub = Arc::new(ApprovalHub::default());
        let req = test_request("/mobile/git/push");

        let hub_clone = hub.clone();
        let handle =
            std::thread::spawn(move || check_git_approval(&req, "test-session", &hub_clone));

        let approval_id = {
            let mut id = None;
            for _ in 0..50 {
                let pending = hub.list_json();
                if let Some(first) = pending.first() {
                    if let Some(s) = first.get("id").and_then(|v| v.as_str()) {
                        id = Some(s.to_string());
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            id.expect("approval should be queued")
        };

        // Allow from the "human" side.
        let outcome = hub.answer(
            &approval_id,
            true,
            Some("test-human".to_string()),
            "nonce-2",
        );
        assert!(
            matches!(
                outcome,
                crate::approvals::AnswerOutcome::Applied(_)
                    | crate::approvals::AnswerOutcome::AlreadyResolved(_)
            ),
            "answer should apply, got {outcome:?}"
        );

        let result = handle.join().expect("thread panicked");
        assert!(
            result.is_ok(),
            "explicit Allow should permit, got {result:?}"
        );
    }
}
