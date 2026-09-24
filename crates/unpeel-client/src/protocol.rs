//! Host protocol versioning and capability identifiers.
//!
//! Mirrors `RemoteControlProtocol` in
//! `clients/shared/UnpeelShared/Sources/UnpeelShared/RemoteControlProtocol.swift`
//! and `protocol/host-capabilities-v1.json`. Major versions must match;
//! minor versions are additive and unknown capability ids are ignored.

/// Protocol major version. Must match the Host's.
pub const PROTOCOL_MAJOR: u32 = 1;
/// Latest known minor version (additive).
pub const PROTOCOL_MINOR: u32 = 22;

/// Well-known capability ids (a subset; the wire may carry more).
pub mod capabilities {
    pub const ARTIFACT_UPLOAD_RESUMABLE: &str = "artifact.upload.resumable";
    pub const ARTIFACT_UPLOAD_FILE: &str = "artifact.upload.file";
    pub const SESSION_ORDER_SET: &str = "session.order.set";
    pub const SESSION_RUNTIME_RESTART: &str = "session.runtime.restart";
    pub const SESSION_RUNTIME_RESUME: &str = "session.runtime.resume";
    pub const SESSION_RELOAD: &str = "session.reload";
    pub const PRESETS_SET: &str = "settings.presets.set";
    pub const PLUGINS_ORDER: &str = "settings.plugins.order";
    pub const PLUGINS_SET: &str = "settings.plugins.set";
    pub const PLUGIN_UPDATES_READ: &str = "settings.plugins.updates.read";
    pub const WORKSPACE_SETTINGS_SET: &str = "settings.workspace.set";
    pub const OPENERS_SET: &str = "settings.openers.set";
    pub const APPS_INSTALL: &str = "apps.install";
    pub const APPS_OPEN: &str = "apps.open";
    pub const INTEGRATIONS_INSTALL: &str = "integrations.install";
    pub const PROJECT_ADD: &str = "project.add";
    pub const HOST_MOBILE_TLS: &str = "host.mobile.tls";
    pub const FILESYSTEM_DIRECTORIES_LIST: &str = "filesystem.directories.list";
    pub const FILESYSTEM_DIRECTORIES_CREATE: &str = "filesystem.directories.create";
    pub const FILESYSTEM_FILE_READ: &str = "filesystem.file.read";
    // Route-ledger capability ids (from the v1 capability ledger).
    pub const APPROVAL_ANSWER: &str = "approval.answer";
    pub const APPROVAL_LIST: &str = "approval.list";
    pub const SESSION_SEND: &str = "session.send";
    pub const SESSION_OUTPUT: &str = "session.output";
    pub const TRANSCRIPT_READ: &str = "transcript.read";
    /// Session creation (`POST /mobile/sessions`).
    pub const SESSION_CREATE: &str = "session.create";
    /// Session → project move (`POST /mobile/session-organization#projectID`).
    pub const SESSION_PROJECT_SET: &str = "session.project.set";
    /// Typed session-event stream (`GET /mobile/events`, Phase 6 R4).
    pub const SESSION_EVENTS_V1: &str = "session.events.v1";
    /// Host-owned turn cancel (`POST /mobile/turn-cancel`, Phase 7 C2).
    /// When advertised, the composer Stop verb uses it; the raw Ctrl-C
    /// fallback is only for Hosts that do not advertise it.
    pub const SESSION_TURN_CANCEL: &str = "session.turn.cancel";
}

/// Whether this Host allows session creation, mirroring Swift's
/// `RemotePreviewStore.supportsSessionCreation`: a missing descriptor is a
/// shipped legacy Host whose launch route remains available; once a Host
/// advertises a descriptor it must be compatible and explicitly list the
/// `session.create` capability.
pub fn supports_session_creation(descriptor: Option<&HostProtocolDescriptor>) -> bool {
    match descriptor {
        None => true,
        Some(d) => d.is_compatible() && d.supports(capabilities::SESSION_CREATE),
    }
}

/// Whether this Host supports moving a session between projects, mirroring
/// Swift's capability-gated organize verbs: a missing descriptor is a
/// shipped legacy Host — the `session-organization` route already accepts
/// `projectID`, so the move verb stays available; once a Host advertises a
/// descriptor it must be compatible and explicitly list
/// `session.project.set`.
pub fn supports_session_project_move(descriptor: Option<&HostProtocolDescriptor>) -> bool {
    match descriptor {
        None => true,
        Some(d) => d.is_compatible() && d.supports(capabilities::SESSION_PROJECT_SET),
    }
}

/// Whether this Host advertises the Host-owned turn-cancel verb (Phase 7
/// C2). A missing descriptor is a shipped legacy Host: the cancel verb did
/// not exist there, so the raw Ctrl-C fallback stays in use. Once a Host
/// advertises a descriptor it must be compatible and explicitly list
/// `session.turn.cancel` before a Controller uses the verb.
pub fn supports_turn_cancel(descriptor: Option<&HostProtocolDescriptor>) -> bool {
    match descriptor {
        None => false,
        Some(d) => d.is_compatible() && d.supports(capabilities::SESSION_TURN_CANCEL),
    }
}
/// A missing descriptor is a shipped legacy Host without the stream; once a
/// Host advertises a descriptor it must be compatible and explicitly list
/// the `session.events.v1` capability.
pub fn has_session_events(descriptor: Option<&HostProtocolDescriptor>) -> bool {
    match descriptor {
        None => false,
        Some(d) => d.is_compatible() && d.supports(capabilities::SESSION_EVENTS_V1),
    }
}

/// One entry of the Host's capability advertisement.
///
/// Older Hosts advertise a bare string list; newer Hosts advertise the
/// structured route ledger. Both shapes are accepted.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Capability {
    Id(String),
    Ledger(LedgerCapability),
}

/// A structured capability-ledger entry: capability id bound to a route.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LedgerCapability {
    pub id: String,
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub native: bool,
    #[serde(default)]
    pub tui: bool,
}

/// Additive Host-level capability contract carried by bootstrap.
///
/// Missing on older Hosts means "legacy capabilities unknown". Major
/// versions must match; minor versions and unknown capability ids are
/// additive — never branch on Host kind or probe routes with 404s.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HostProtocolDescriptor {
    #[serde(rename = "majorVersion", default)]
    pub major_version: u32,
    #[serde(rename = "minorVersion", default)]
    pub minor_version: u32,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
}

impl HostProtocolDescriptor {
    /// Major versions must match; the controller refuses to speak otherwise.
    pub fn is_compatible(&self) -> bool {
        self.major_version == PROTOCOL_MAJOR
    }

    /// Whether the Host advertises a capability, in either ledger shape.
    pub fn supports(&self, id: &str) -> bool {
        self.capabilities.iter().any(|c| match c {
            Capability::Id(s) => s == id,
            Capability::Ledger(l) => l.id == id,
        })
    }

    /// Route (method, path) for a ledger capability, if the Host binds one.
    pub fn route_for(&self, id: &str) -> Option<(&str, &str)> {
        self.capabilities.iter().find_map(|c| match c {
            Capability::Ledger(l) if l.id == id => Some((l.method.as_str(), l.path.as_str())),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_both_capability_shapes() {
        let json = serde_json::json!({
            "majorVersion": 1,
            "minorVersion": 21,
            "capabilities": [
                "integrations.install",
                {"id": "approval.answer", "method": "POST", "path": "/mobile/approvals/answer", "native": true, "tui": true}
            ]
        });
        let d: HostProtocolDescriptor = serde_json::from_value(json).unwrap();
        assert!(d.is_compatible());
        assert!(d.supports(capabilities::INTEGRATIONS_INSTALL));
        assert!(d.supports(capabilities::APPROVAL_ANSWER));
        assert!(!d.supports("nope.not.real"));
        assert_eq!(
            d.route_for(capabilities::APPROVAL_ANSWER),
            Some(("POST", "/mobile/approvals/answer"))
        );
    }

    #[test]
    fn major_mismatch_is_incompatible() {
        let d = HostProtocolDescriptor {
            major_version: 2,
            minor_version: 0,
            capabilities: vec![],
        };
        assert!(!d.is_compatible());
    }

    #[test]
    fn supports_session_creation_rules() {
        // Missing descriptor = legacy Host: creation allowed.
        assert!(supports_session_creation(None));
        // Compatible descriptor without the capability: hidden.
        let d = HostProtocolDescriptor {
            major_version: PROTOCOL_MAJOR,
            minor_version: 0,
            capabilities: vec![],
        };
        assert!(!supports_session_creation(Some(&d)));
        // Compatible descriptor advertising session.create: allowed.
        let d = HostProtocolDescriptor {
            major_version: PROTOCOL_MAJOR,
            minor_version: 0,
            capabilities: vec![Capability::Id(capabilities::SESSION_CREATE.to_string())],
        };
        assert!(supports_session_creation(Some(&d)));
        // Incompatible major: refused even with the capability.
        let d = HostProtocolDescriptor {
            major_version: PROTOCOL_MAJOR + 1,
            minor_version: 0,
            capabilities: vec![Capability::Id(capabilities::SESSION_CREATE.to_string())],
        };
        assert!(!supports_session_creation(Some(&d)));
    }

    #[test]
    fn supports_session_project_move_rules() {
        // Missing descriptor = legacy Host: the session-organization route
        // already accepts projectID, so the move verb stays available.
        assert!(supports_session_project_move(None));
        // Compatible descriptor without the capability: hidden.
        let d = HostProtocolDescriptor {
            major_version: PROTOCOL_MAJOR,
            minor_version: 0,
            capabilities: vec![],
        };
        assert!(!supports_session_project_move(Some(&d)));
        // Compatible descriptor advertising session.project.set: allowed.
        let d = HostProtocolDescriptor {
            major_version: PROTOCOL_MAJOR,
            minor_version: 0,
            capabilities: vec![Capability::Id(
                capabilities::SESSION_PROJECT_SET.to_string(),
            )],
        };
        assert!(supports_session_project_move(Some(&d)));
        // Incompatible major: refused even with the capability.
        let d = HostProtocolDescriptor {
            major_version: PROTOCOL_MAJOR + 1,
            minor_version: 0,
            capabilities: vec![Capability::Id(
                capabilities::SESSION_PROJECT_SET.to_string(),
            )],
        };
        assert!(!supports_session_project_move(Some(&d)));
    }

    /// F3: the composer Stop button uses the protocol cancel verb only
    /// when `session.turn.cancel` is advertised; otherwise it falls back
    /// to writing Ctrl-C (`\x03`) to the PTY. A missing descriptor (legacy
    /// Host) or an incompatible major version also means fallback.
    #[test]
    fn turn_cancel_capability_gates_protocol_cancel() {
        // Missing descriptor = legacy Host without the verb: fallback.
        assert!(!supports_turn_cancel(None));

        // Compatible descriptor advertising the capability: protocol cancel.
        let d = HostProtocolDescriptor {
            major_version: PROTOCOL_MAJOR,
            minor_version: 22,
            capabilities: vec![Capability::Id(
                capabilities::SESSION_TURN_CANCEL.to_string(),
            )],
        };
        assert!(supports_turn_cancel(Some(&d)));

        // Compatible descriptor without the capability: fallback to Ctrl-C.
        let d = HostProtocolDescriptor {
            major_version: PROTOCOL_MAJOR,
            minor_version: 22,
            capabilities: vec![Capability::Id("session.stop".to_string())],
        };
        assert!(!supports_turn_cancel(Some(&d)));

        // Incompatible major: fallback even with the capability listed.
        let d = HostProtocolDescriptor {
            major_version: PROTOCOL_MAJOR + 1,
            minor_version: 0,
            capabilities: vec![Capability::Id(
                capabilities::SESSION_TURN_CANCEL.to_string(),
            )],
        };
        assert!(!supports_turn_cancel(Some(&d)));
    }
}
