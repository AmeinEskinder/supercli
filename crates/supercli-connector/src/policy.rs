//! Approval policy resolution for connector tool calls.
//!
//! The manifest declares ceilings; a session may only tighten them. The
//! ordering is [`ApprovalPolicy::Allow`] < `Ask` < `Deny`: a session
//! override is honored only when it is at least as restrictive as the
//! manifest default. A session that tries to loosen a default keeps the
//! default — this is enforced here, not left to callers.

use std::collections::HashMap;

use crate::manifest::{ApprovalPolicy, ConnectorManifest};

/// Resolve the effective policy for one tool: the session override wins
/// only if it tightens (or equals) the manifest default.
pub fn effective_policy(
    manifest: &ConnectorManifest,
    session_overrides: &HashMap<String, ApprovalPolicy>,
    tool: &str,
) -> ApprovalPolicy {
    let manifest_default = manifest.default_policy(tool);
    match session_overrides.get(tool) {
        Some(override_policy) if *override_policy >= manifest_default => *override_policy,
        _ => manifest_default,
    }
}

/// Whether a tool call with the given policy may proceed without asking.
/// `Ask` and `Deny` both require the harness's approval path; this helper
/// answers only the silent-allow case.
pub fn silently_allowed(policy: ApprovalPolicy) -> bool {
    policy == ApprovalPolicy::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::parse_manifest;

    const MANIFEST: &str = r#"
[connector]
name = "gmail"
version = "1.0.0"
display_name = "Gmail"
description = "Mail."
kind = "mcp-stdio"

[tools]
provides = ["mail.search", "mail.send"]

[policy]
"mail.search" = "allow"
"mail.send" = "ask"
"#;

    fn manifest() -> ConnectorManifest {
        parse_manifest(MANIFEST).unwrap()
    }

    #[test]
    fn session_can_tighten_never_loosen() {
        let manifest = manifest();
        let mut overrides = HashMap::new();

        // Tightening allow -> ask is honored.
        overrides.insert("mail.search".to_string(), ApprovalPolicy::Ask);
        assert_eq!(
            effective_policy(&manifest, &overrides, "mail.search"),
            ApprovalPolicy::Ask
        );

        // Loosening ask -> allow is ignored.
        overrides.insert("mail.send".to_string(), ApprovalPolicy::Allow);
        assert_eq!(
            effective_policy(&manifest, &overrides, "mail.send"),
            ApprovalPolicy::Ask
        );

        // Tightening ask -> deny is honored.
        overrides.insert("mail.send".to_string(), ApprovalPolicy::Deny);
        assert_eq!(
            effective_policy(&manifest, &overrides, "mail.send"),
            ApprovalPolicy::Deny
        );

        // No override: manifest default.
        assert_eq!(
            effective_policy(&manifest, &HashMap::new(), "mail.search"),
            ApprovalPolicy::Allow
        );
    }

    #[test]
    fn silently_allowed_only_for_allow() {
        assert!(silently_allowed(ApprovalPolicy::Allow));
        assert!(!silently_allowed(ApprovalPolicy::Ask));
        assert!(!silently_allowed(ApprovalPolicy::Deny));
    }
}
