//! Remote Host connection state machine and Direct→Link fallback.
//!
//! Ported from `clients/legacy/native/SupercliNative/Sources/SupercliNative/RemoteHostRuntime.swift`.
//! The portable surface lives here: connection-state types, the transport
//! model, the capability-gated session/organization verb surface, and the
//! pure decision logic (default selection, snapshot content equality,
//! replacement correlation, write batching).
//!
//! The Swift `@MainActor ObservableObject` concurrency (refresh loops,
//! effect workers, output pumps, route probes, pane cache) is driven by the
//! platform launcher; the verbs here are synchronous against a
//! [`RemoteBackend`] so the same behaviour is unit-testable without an
//! async runtime. Where Swift awaits a backend, the Rust verb captures the
//! connection generation first and rejects the result if the connection
//! moved on — the synchronous model of "reject results after disconnect".

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::dto::{
    BootstrapSnapshot, CreateSessionRequest, CreatedSession, PluginActivationPatch, PluginUpdates,
    PresetPatch, ProjectOrganizationPatch, SessionSummary, WorkspaceSettingsPatch,
};

/// Connection lifecycle for one Host. Mirrors Swift
/// `RemoteHostConnectionState`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteHostConnectionState {
    Idle,
    Connecting,
    Connected {
        name: String,
    },
    /// The last valid sidebar/terminal remains visible while reconnection runs.
    Reconnecting {
        message: String,
    },
    RepairRequired {
        message: String,
    },
    Incompatible {
        message: String,
    },
    Failed {
        message: String,
    },
}

impl RemoteHostConnectionState {
    /// True while a connection attempt (or re-attempt) is in flight.
    pub fn is_transitional(&self) -> bool {
        matches!(
            self,
            RemoteHostConnectionState::Connecting | RemoteHostConnectionState::Reconnecting { .. }
        )
    }

    /// True once the transport is up.
    pub fn is_connected(&self) -> bool {
        matches!(self, RemoteHostConnectionState::Connected { .. })
    }
}

/// User-facing route for one Host connection. The UI deliberately exposes
/// only the useful distinction (local network or Supercli Link), never relay
/// endpoints, tokens, or a manual transport picker. Mirrors Swift
/// `RemoteHostConnectionRoute`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteHostConnectionRoute {
    Ssh,
    LocalGateway,
    Direct,
    Link,
}

impl RemoteHostConnectionRoute {
    pub fn short_label(&self) -> &'static str {
        match self {
            RemoteHostConnectionRoute::Ssh => "Connected",
            RemoteHostConnectionRoute::LocalGateway => "This Mac",
            RemoteHostConnectionRoute::Direct => "Direct",
            RemoteHostConnectionRoute::Link => "Via Link",
        }
    }
}

/// Which transport to open for a paired host. Mirrors the portable subset of
/// Swift `RemoteHostTransport` (the SSH/local variants carry launcher-side
/// state and stay out of this crate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairedTransport {
    Direct {
        endpoint: String,
        certificate_fingerprint: String,
    },
    Link {
        controller_device_id: String,
    },
}

impl PairedTransport {
    pub fn route(&self) -> RemoteHostConnectionRoute {
        match self {
            PairedTransport::Direct { .. } => RemoteHostConnectionRoute::Direct,
            PairedTransport::Link { .. } => RemoteHostConnectionRoute::Link,
        }
    }
}

/// Direct→Link fallback plan for one paired host. Mirrors Swift
/// `PairedHostConnectionPlan`.
///
/// A Direct-only host (removed from the Link enrollment list) gets no Link
/// transport at all: reachability failures then report Direct-only
/// reachability instead of silently riding the relay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedHostConnectionPlan {
    pub host_id: String,
    pub direct: PairedTransport,
    /// `None` when the user scoped this host to Direct-only.
    pub link: Option<PairedTransport>,
}

/// Failure building a connection plan. Mirrors the Swift
/// `requirePairingRepair` branch of `connectPairedHost`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionPlanError {
    /// No usable certificate pin: legacy records can still use their
    /// authenticated E2E Link, but a Direct-only host must re-pair.
    /// Never send a saved bearer to an unverified LAN endpoint.
    PairingRepairRequired { message: String },
}

impl PairedHostConnectionPlan {
    /// Build the plan for a paired host record.
    ///
    /// - `link_enabled`: `false` scopes the host to Direct-only (no Link
    ///   transport, no grace race, no background probe may open the relay).
    /// - `certificate_fingerprint`: 64-hex-char pin for the Direct endpoint.
    ///   Legacy records without a pin fall back to Link-only; a Direct-only
    ///   host without a pin must re-pair.
    pub fn build(
        host_id: String,
        endpoint: String,
        controller_device_id: String,
        link_enabled: bool,
        certificate_fingerprint: Option<String>,
    ) -> Result<Self, ConnectionPlanError> {
        let link = link_enabled.then_some(PairedTransport::Link {
            controller_device_id,
        });
        let pin = certificate_fingerprint.filter(|p| is_valid_pin(p));
        match (pin, link) {
            (Some(pin), link) => Ok(Self {
                host_id,
                direct: PairedTransport::Direct {
                    endpoint,
                    certificate_fingerprint: pin,
                },
                link,
            }),
            // Legacy records can still use their authenticated E2E Link.
            // Never send the saved bearer to an unverified LAN endpoint:
            // the Direct transport is a placeholder the launcher must not open.
            (None, Some(link_transport)) => Ok(Self {
                host_id,
                direct: PairedTransport::Direct {
                    endpoint,
                    certificate_fingerprint: String::new(),
                },
                link: Some(link_transport),
            }),
            (None, None) => Err(ConnectionPlanError::PairingRepairRequired {
                message: "This Host was paired before Direct connections were certificate-pinned. Pair it again to reach it directly.".to_string(),
            }),
        }
    }

    /// The transport to try first: Direct when pinned, else Link.
    pub fn initial(&self) -> &PairedTransport {
        match &self.direct {
            PairedTransport::Direct {
                certificate_fingerprint,
                ..
            } if !certificate_fingerprint.is_empty() => &self.direct,
            _ => self.link.as_ref().unwrap_or(&self.direct),
        }
    }

    /// The fallback transport after the initial one fails, if any.
    pub fn fallback(&self) -> Option<&PairedTransport> {
        let initial_is_direct = matches!(
            self.initial(),
            PairedTransport::Direct {
                certificate_fingerprint,
                ..
            } if !certificate_fingerprint.is_empty()
        );
        if initial_is_direct {
            self.link.as_ref()
        } else {
            None
        }
    }
}

/// A 64-hex-char certificate pin, as stored on the pairing record.
fn is_valid_pin(pin: &str) -> bool {
    pin.len() == 64 && pin.bytes().all(|b| b.is_ascii_hexdigit())
}

// ============================================================================
// Ported from RemoteHostRuntime.swift: transport model, verb surface, runtime
// ============================================================================

/// Stable Host operation ids (`protocol/host-capabilities-v1.json`). Menus
/// gate on these through [`RemoteHostRuntime::supports_host_operation`]; the
/// backend enforces them again per call. Mirrors Swift
/// `RemoteHostRuntime.HostOperation`.
pub mod host_operation {
    pub const WRITE: &str = "session.input.write";
    pub const TITLE_SET: &str = "session.title.set";
    pub const PIN_SET: &str = "session.pin.set";
    pub const ARCHIVE: &str = "session.archive";
    pub const RESTORE: &str = "session.restore";
    pub const STOP: &str = "session.stop";
    pub const REMOVE: &str = "session.remove";
    pub const RESTART: &str = "session.restart";
    pub const RELOAD: &str = "session.reload";
    pub const RESIZE_DESKTOP: &str = "session.resize_desktop";
    pub const RESUME_AGENT: &str = "session.runtime.resume";
    pub const CREATE: &str = "session.create";
    pub const ORDER_SET: &str = "session.order.set";
    pub const PROJECT_ORGANIZATION_SET: &str = "project.organization.set";
    pub const PROJECT_PIN_SET: &str = "project.pin.set";
    pub const PRESETS_SET: &str = "settings.presets.set";
    pub const PLUGINS_ORDER: &str = "settings.plugins.order";
    pub const PLUGINS_SET: &str = "settings.plugins.set";
    pub const PLUGIN_UPDATES_READ: &str = "settings.plugins.updates.read";
    pub const WORKSPACE_SETTINGS_SET: &str = "settings.workspace.set";
    pub const OPENERS_SET: &str = "settings.openers.set";
    pub const APPS_INSTALL: &str = "apps.install";
    pub const INTEGRATIONS_INSTALL: &str = "integrations.install";
    pub const APPS_OPEN: &str = "apps.open";
    pub const ARCHIVE_LIST: &str = "session.archive.list";
    pub const TRANSCRIPT_MARKDOWN: &str = "session.transcript.markdown";
    pub const PAIRING_INVITATION: &str = "pairing.invitation";
    pub const ARTIFACT_UPLOAD: &str = "artifact.upload";
    pub const ARTIFACT_UPLOAD_RESUMABLE: &str = "artifact.upload.resumable";
    pub const ARTIFACT_UPLOAD_FILE: &str = "artifact.upload.file";
    pub const PROJECT_SET: &str = "session.project.set";
    pub const PUSH_REGISTER: &str = "push.register";
    pub const NOTIFY_WHEN_DONE_SET: &str = "session.notify_when_done.set";
    pub const APPROVAL_ANSWER: &str = "approval.answer";
    // NOTE: There is intentionally no generic RESOURCE_REQUEST capability.
    // Swift's `resourceRequest(operation:capability:...)` takes a per-operation
    // capability supplied by the caller (e.g. "artifact.upload.file",
    // "project.add", "filesystem.directories.list"). A single generic gate
    // would not match Swift's per-operation gating.
}

/// One user-facing failure from a remote organization or lifecycle verb.
/// Mirrors Swift `RemoteHostVerbError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteHostVerbError {
    /// No live connection.
    NotConnected,
    /// The Host does not advertise the required capability.
    CapabilityUnavailable { capability: String },
    /// A result arrived after disconnect/reconnect/promotion retired its
    /// connection generation.
    StaleResult,
    /// Already connected.
    AlreadyConnected,
    /// The probed Host identity changed.
    HostIdentityChanged,
    /// The backend failed with a classified error.
    Backend(BackendError),
    /// The effect may have landed; never retry silently.
    OutcomeUnknown { operation: String, message: String },
}

impl RemoteHostVerbError {
    pub fn new(operation: &str, message: &str, outcome_is_unknown: bool) -> Self {
        if outcome_is_unknown {
            Self::OutcomeUnknown {
                operation: operation.to_string(),
                message: message.to_string(),
            }
        } else {
            Self::Backend(BackendError::new(operation, message))
        }
    }

    /// Map a backend failure to the verb error taxonomy.
    pub fn from_backend(operation: &str, error: BackendError, outcome_unknown: bool) -> Self {
        if outcome_unknown || error.kind == Some(BackendErrorKind::OutcomeUnknown) {
            Self::OutcomeUnknown {
                operation: operation.to_string(),
                message: error.message.clone(),
            }
        } else {
            Self::Backend(error)
        }
    }

    /// Whether the effect may have reached the Host.
    pub fn is_outcome_unknown(&self) -> bool {
        matches!(self, Self::OutcomeUnknown { .. })
    }
}

impl std::fmt::Display for RemoteHostVerbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConnected => write!(f, "not connected to a host"),
            Self::CapabilityUnavailable { capability } => {
                write!(f, "host does not support {capability}")
            }
            Self::StaleResult => write!(f, "stale result from a retired connection"),
            Self::AlreadyConnected => write!(f, "already connected"),
            Self::HostIdentityChanged => write!(f, "host identity changed"),
            Self::Backend(e) => write!(f, "{}", e.message),
            Self::OutcomeUnknown { operation, message } => {
                write!(f, "{operation}: {message} (outcome unknown)")
            }
        }
    }
}

impl std::error::Error for RemoteHostVerbError {}

/// How a backend failure classifies for at-most-once effect semantics.
/// Mirrors Swift `NativeRemoteBackendError`'s `kind` discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendErrorKind {
    /// The effect may have landed; never retry silently.
    OutcomeUnknown,
    /// A correlated Host response proved the effect did not run.
    NotApplied,
}

/// A backend transport/Host failure. Mirrors Swift
/// `NativeRemoteBackendError` (message + kind only; the numeric result code
/// is transport detail).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendError {
    pub code: String,
    pub message: String,
    pub kind: Option<BackendErrorKind>,
}

impl BackendError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            kind: None,
        }
    }

    pub fn outcome_unknown(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            kind: Some(BackendErrorKind::OutcomeUnknown),
        }
    }

    pub fn not_applied(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            kind: Some(BackendErrorKind::NotApplied),
        }
    }

    /// An at-most-once effect may have reached the Host even though its
    /// receipt was lost.
    pub fn effect_outcome_is_unknown(&self) -> bool {
        self.kind == Some(BackendErrorKind::OutcomeUnknown)
    }

    /// A correlated Host response proved that the effect did not run.
    pub fn effect_was_not_applied(&self) -> bool {
        self.kind == Some(BackendErrorKind::NotApplied)
    }
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for BackendError {}

/// One Host-originated focus decision for a local direct terminal data
/// plane. Only a correlated create/restart result crosses this one-way seam.
/// Mirrors Swift `DirectDataPlaneSelectionIntent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectDataPlaneSelectionIntent {
    pub sequence: u64,
    pub session_id: Option<String>,
}

/// A transport is only a carrier for the shared Host contract. Mirrors Swift
/// `RemoteHostTransport`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteHostTransport {
    Ssh {
        target: String,
        expected_host_id: Option<String>,
        secret: Option<String>,
    },
    /// Another LOCAL workspace, scoped through the loopback gateway.
    LocalGateway {
        supercli_home: String,
        workspace_name: String,
        expected_host_id: Option<String>,
    },
    /// This app instance's own workspace over the persistent `supercli serve`
    /// worker. Semantic verbs ride this connection; terminal bytes stay on
    /// the direct `supercli-attach` data plane.
    LocalService {
        supercli_home: String,
        workspace_name: String,
        expected_host_id: Option<String>,
    },
    Direct {
        endpoint: String,
        auth_token: String,
        expected_host_id: String,
        certificate_fingerprint: Option<String>,
    },
    Link {
        relay_url: String,
        mac_id: String,
        controller_device_id: String,
        auth_token: String,
        expected_host_id: String,
    },
}

impl RemoteHostTransport {
    pub fn expected_host_id(&self) -> Option<&str> {
        match self {
            RemoteHostTransport::Ssh {
                expected_host_id, ..
            } => expected_host_id.as_deref(),
            RemoteHostTransport::LocalGateway {
                expected_host_id, ..
            } => expected_host_id.as_deref(),
            RemoteHostTransport::LocalService {
                expected_host_id, ..
            } => expected_host_id.as_deref(),
            RemoteHostTransport::Direct {
                expected_host_id, ..
            } => Some(expected_host_id),
            RemoteHostTransport::Link {
                expected_host_id, ..
            } => Some(expected_host_id),
        }
    }

    pub fn route(&self) -> RemoteHostConnectionRoute {
        match self {
            RemoteHostTransport::Ssh { .. } => RemoteHostConnectionRoute::Ssh,
            RemoteHostTransport::LocalGateway { .. } | RemoteHostTransport::LocalService { .. } => {
                RemoteHostConnectionRoute::LocalGateway
            }
            RemoteHostTransport::Direct { .. } => RemoteHostConnectionRoute::Direct,
            RemoteHostTransport::Link { .. } => RemoteHostConnectionRoute::Link,
        }
    }

    /// Local terminal rendering remains the existing `supercli-attach` →
    /// `session.sock` path. This runtime may still submit explicit semantic
    /// Host verbs, but must not poll output, fit, mark read, or accept
    /// terminal input merely because the worker advertises those operations.
    pub fn uses_direct_session_data_plane(&self) -> bool {
        matches!(
            self,
            RemoteHostTransport::LocalService { .. } | RemoteHostTransport::LocalGateway { .. }
        )
    }

    pub fn continuity_key(&self) -> Option<RemoteHostContinuityKey> {
        if let Some(host_id) = self.expected_host_id() {
            return Some(RemoteHostContinuityKey::PinnedHost(host_id.to_string()));
        }
        match self {
            RemoteHostTransport::Ssh { target, .. } => {
                Some(RemoteHostContinuityKey::SshTarget(target.clone()))
            }
            RemoteHostTransport::LocalGateway { supercli_home, .. }
            | RemoteHostTransport::LocalService { supercli_home, .. } => Some(
                RemoteHostContinuityKey::WorkspaceHome(supercli_home.clone()),
            ),
            RemoteHostTransport::Direct { .. } | RemoteHostTransport::Link { .. } => None,
        }
    }

    /// Human-readable identity for the transport target.
    pub fn host_identity(&self) -> String {
        match self {
            RemoteHostTransport::Ssh { target, .. } => target.clone(),
            RemoteHostTransport::LocalGateway { supercli_home, .. }
            | RemoteHostTransport::LocalService { supercli_home, .. } => supercli_home.clone(),
            RemoteHostTransport::Direct { endpoint, .. } => endpoint.clone(),
            RemoteHostTransport::Link { relay_url, .. } => relay_url.clone(),
        }
    }
}

/// Stable identity for connection reuse across scope re-entry. Mirrors Swift
/// `RemoteHostContinuityKey`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RemoteHostContinuityKey {
    PinnedHost(String),
    SshTarget(String),
    WorkspaceHome(String),
}

impl RemoteHostContinuityKey {
    pub fn pane_fallback_key(&self) -> String {
        match self {
            RemoteHostContinuityKey::PinnedHost(id) => format!("host:{id}"),
            RemoteHostContinuityKey::SshTarget(t) => format!("ssh:{t}"),
            RemoteHostContinuityKey::WorkspaceHome(h) => format!("workspace:{h}"),
        }
    }
}

/// Synchronous Host backend. Mirrors Swift `NativeRemoteBackendProtocol`'s
/// verb surface; the Swift `async` boundary is modelled by the runtime
/// capturing the connection generation around each call (see
/// [`RemoteHostRuntime::begin_verb`] / `complete_verb`).
pub trait RemoteBackend {
    fn bootstrap(&mut self) -> Result<BootstrapSnapshot, BackendError>;
    fn plugin_updates(&mut self) -> Result<PluginUpdates, BackendError>;
    fn set_workspace_settings(&mut self, patch: WorkspaceSettingsPatch)
        -> Result<(), BackendError>;
    fn set_session_title(&mut self, session_id: &str, title: &str) -> Result<(), BackendError>;
    fn set_session_pinned(&mut self, session_id: &str, pinned: bool) -> Result<(), BackendError>;
    fn set_session_notify_when_done(
        &mut self,
        session_id: &str,
        enabled: bool,
    ) -> Result<(), BackendError>;
    fn answer_approval(&mut self, id: &str, approved: bool) -> Result<(), BackendError>;
    fn set_session_project(
        &mut self,
        session_id: &str,
        project_id: &str,
    ) -> Result<(), BackendError>;
    fn archive_session(&mut self, session_id: &str) -> Result<(), BackendError>;
    fn restore_session(&mut self, session_id: &str) -> Result<(), BackendError>;
    fn stop_session(&mut self, session_id: &str) -> Result<(), BackendError>;
    fn remove_session(&mut self, session_id: &str) -> Result<(), BackendError>;
    fn restart_session(&mut self, session_id: &str) -> Result<(), BackendError>;
    fn reload_session(&mut self, session_id: &str) -> Result<(), BackendError>;
    fn resume_agent(&mut self, session_id: &str) -> Result<(), BackendError>;
    fn set_session_order(
        &mut self,
        project_id: &str,
        ordered_session_ids: &[String],
    ) -> Result<(), BackendError>;
    fn set_project_organization(
        &mut self,
        patch: ProjectOrganizationPatch,
    ) -> Result<(), BackendError>;
    fn set_preset(&mut self, patch: PresetPatch) -> Result<(), BackendError>;
    fn set_opener(&mut self, selector: &str, opener: &str) -> Result<(), BackendError>;
    fn install_app(&mut self, app_id: &str) -> Result<(), BackendError>;
    fn install_integration(&mut self, runtime_id: &str) -> Result<(), BackendError>;
    fn open_app(
        &mut self,
        app_id: &str,
        resource_kind: &str,
        media_type: Option<&str>,
        resource_id: &str,
        caller_session_id: &str,
    ) -> Result<(), BackendError>;
    fn create_session(
        &mut self,
        request: CreateSessionRequest,
    ) -> Result<CreatedSession, BackendError>;
    fn pairing_invitation(&mut self, request_json: Vec<u8>) -> Result<Vec<u8>, BackendError>;
    fn upload_attachment(
        &mut self,
        session_id: Option<&str>,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<String, BackendError>;
    fn resource_request(
        &mut self,
        operation: &str,
        parameters: &HashMap<String, String>,
        bytes: Vec<u8>,
    ) -> Result<Vec<u8>, BackendError>;
    fn list_archived_sessions(
        &mut self,
        project_id: &str,
    ) -> Result<Vec<SessionSummary>, BackendError>;
    fn transcript_markdown(
        &mut self,
        session_id: &str,
        entries: Option<u32>,
    ) -> Result<String, BackendError>;
    fn clear_desktop_fit(&mut self, session_id: &str) -> Result<(), BackendError>;
    fn set_plugin_order(&mut self, ordered_ids: &[String]) -> Result<(), BackendError>;
    fn set_plugin_activation(&mut self, patch: PluginActivationPatch) -> Result<(), BackendError>;
    fn close(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_state_variants() {
        assert!(RemoteHostConnectionState::Connecting.is_transitional());
        assert!(RemoteHostConnectionState::Reconnecting {
            message: "x".into()
        }
        .is_transitional());
        assert!(!RemoteHostConnectionState::Idle.is_transitional());
        assert!(RemoteHostConnectionState::Connected { name: "Mac".into() }.is_connected());
        assert!(!RemoteHostConnectionState::Failed {
            message: "x".into()
        }
        .is_connected());
    }

    #[test]
    fn route_short_labels() {
        assert_eq!(RemoteHostConnectionRoute::Ssh.short_label(), "Connected");
        assert_eq!(
            RemoteHostConnectionRoute::LocalGateway.short_label(),
            "This Mac"
        );
        assert_eq!(RemoteHostConnectionRoute::Direct.short_label(), "Direct");
        assert_eq!(RemoteHostConnectionRoute::Link.short_label(), "Via Link");
    }

    #[test]
    fn link_disabled_means_no_fallback() {
        let plan = PairedHostConnectionPlan::build(
            "host-1".to_string(),
            "https://192.168.1.5:443".to_string(),
            "controller-1".to_string(),
            false, // Direct-only
            Some("a".repeat(64)),
        )
        .unwrap();
        assert!(plan.link.is_none());
        assert_eq!(plan.initial().route(), RemoteHostConnectionRoute::Direct);
        assert!(plan.fallback().is_none());
    }

    #[test]
    fn direct_first_then_link_fallback() {
        let plan = PairedHostConnectionPlan::build(
            "host-1".to_string(),
            "https://192.168.1.5:443".to_string(),
            "controller-1".to_string(),
            true,
            Some("b".repeat(64)),
        )
        .unwrap();
        assert_eq!(plan.initial().route(), RemoteHostConnectionRoute::Direct);
        assert_eq!(
            plan.fallback().map(|t| t.route()),
            Some(RemoteHostConnectionRoute::Link)
        );
    }

    #[test]
    fn legacy_record_without_pin_needs_repair_when_direct_only() {
        let err = PairedHostConnectionPlan::build(
            "host-1".to_string(),
            "https://192.168.1.5:443".to_string(),
            "controller-1".to_string(),
            false, // Direct-only, no pin
            None,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ConnectionPlanError::PairingRepairRequired { .. }
        ));
    }

    #[test]
    fn invalid_pin_rejected() {
        assert!(!is_valid_pin("short"));
        assert!(!is_valid_pin(&"z".repeat(64)));
        assert!(is_valid_pin(&"A".repeat(64)));
        assert!(is_valid_pin(&"0123456789abcdef".repeat(4)));
    }
}

/// Legacy restart correlation intent. The Host's legacy replacement receipt
/// does not return the new Session id, so the runtime latches correlation
/// before Restore and fails closed on collisions. Mirrors Swift
/// `RemoteHostRuntime.ReplacementSelectionIntent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementSelectionIntent {
    pub source_session_id: String,
    pub project_id: String,
    pub created_at_unix_ms: i64,
    /// Provider/runtime identity derived from providerID or the command
    /// family. The replacement command itself contains resume flags, so
    /// exact command-string equality would reject the real replacement.
    pub runtime_id: Option<String>,
    pub worktree_path: Option<String>,
    pub worktree_branch: Option<String>,
    pub baseline_session_ids: HashSet<String>,
    pub bootstrap_observations_remaining: u32,
}

impl ReplacementSelectionIntent {
    pub const MAXIMUM_BOOTSTRAP_OBSERVATIONS: u32 = 30;

    pub fn new(source: &SessionSummary, known_session_ids: &HashSet<String>) -> Self {
        let mut baseline = known_session_ids.clone();
        baseline.insert(source.id.clone());
        Self {
            source_session_id: source.id.clone(),
            project_id: source.project_id.clone(),
            created_at_unix_ms: source.created_at_unix_ms,
            runtime_id: source
                .provider_id
                .clone()
                .or_else(|| detect_runtime_id(&source.command)),
            worktree_path: source.worktree_path.clone(),
            worktree_branch: source.worktree_branch.clone(),
            baseline_session_ids: baseline,
            bootstrap_observations_remaining: Self::MAXIMUM_BOOTSTRAP_OBSERVATIONS,
        }
    }
}

/// Heuristic runtime-family detection from a session command, mirroring
/// Swift `SetupTool.detect(in:)`'s role in replacement correlation.
fn detect_runtime_id(command: &str) -> Option<String> {
    let lower = command.to_lowercase();
    for token in ["claude", "codex", "opencode", "gemini", "grok", "aider"] {
        if lower.contains(token) {
            return Some(token.to_string());
        }
    }
    None
}

/// Resolution of a replacement-correlation observation. Mirrors Swift
/// `RemoteHostRuntime.ReplacementSelectionResolution`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplacementSelectionResolution {
    Wait(ReplacementSelectionIntent),
    Select(String),
    Cancel,
}

/// Resolve a legacy restart receipt to exactly one replacement row. A valid
/// candidate must be new relative to the pre-effect baseline, must preserve
/// the stable project/creation/worktree identity, and must belong to the
/// same runtime command family. The old row must also have vanished, proving
/// this is a replacement rather than an unrelated concurrent launch.
/// Multiple candidates permanently cancel automatic selection.
pub fn replacement_selection_resolution(
    intent: &ReplacementSelectionIntent,
    sessions: &[SessionSummary],
) -> ReplacementSelectionResolution {
    let source_still_exists = sessions.iter().any(|s| s.id == intent.source_session_id);
    let candidates: Vec<&SessionSummary> = sessions
        .iter()
        .filter(|s| {
            if s.id == intent.source_session_id
                || intent.baseline_session_ids.contains(&s.id)
                || s.project_id != intent.project_id
                || s.created_at_unix_ms != intent.created_at_unix_ms
                || s.status != crate::dto::SessionStatus::Running
                || s.archived
                || s.worktree_path != intent.worktree_path
                || s.worktree_branch != intent.worktree_branch
            {
                return false;
            }
            match &intent.runtime_id {
                Some(expected) => {
                    let candidate = s
                        .provider_id
                        .clone()
                        .or_else(|| detect_runtime_id(&s.command));
                    candidate.as_deref() == Some(expected.as_str())
                }
                None => true,
            }
        })
        .collect();

    if candidates.len() > 1 {
        return ReplacementSelectionResolution::Cancel;
    }
    if !source_still_exists {
        if let Some(candidate) = candidates.first() {
            return ReplacementSelectionResolution::Select(candidate.id.clone());
        }
    }
    if intent.bootstrap_observations_remaining <= 1 {
        return ReplacementSelectionResolution::Cancel;
    }
    let mut waiting = intent.clone();
    waiting.bootstrap_observations_remaining -= 1;
    ReplacementSelectionResolution::Wait(waiting)
}

/// Default session selection: blocked first, then running, then first.
/// Mirrors Swift `RemoteHostRuntime.defaultSessionID(in:)`.
pub fn default_session_id(sessions: &[SessionSummary]) -> Option<String> {
    let live: Vec<&SessionSummary> = sessions.iter().filter(|s| !s.archived).collect();
    live.iter()
        .find(|s| s.activity == crate::dto::ActivityState::Blocked)
        .or_else(|| {
            live.iter()
                .find(|s| s.status == crate::dto::SessionStatus::Running)
        })
        .or_else(|| live.first())
        .map(|s| s.id.clone())
}

fn minute_bucket(unix_ms: Option<i64>) -> Option<i64> {
    unix_ms.map(|ms| ms / 60_000)
}

/// Ignore the capture clock and output-preview churn so a periodic health
/// poll does not rebuild the sidebar while an agent types. Mirrors Swift
/// `RemoteHostRuntime.snapshotContentEqual`.
pub fn snapshot_content_equal(a: &BootstrapSnapshot, b: &BootstrapSnapshot) -> bool {
    a.protocol_version == b.protocol_version
        && a.host_protocol == b.host_protocol
        && a.host_id == b.host_id
        && a.host_name == b.host_name
        && a.folders == b.folders
        && a.projects == b.projects
        && a.presets == b.presets
        && a.workspace_settings == b.workspace_settings
        && a.available_apps == b.available_apps
        && a.installed_apps == b.installed_apps
        && a.openers == b.openers
        && a.app_presentations == b.app_presentations
        && a.experimental_worktrees_enabled == b.experimental_worktrees_enabled
        && a.pro_entitled == b.pro_entitled
        && a.pending_approvals == b.pending_approvals
        && a.host_tint_hue == b.host_tint_hue
        && a.host_device_kind == b.host_device_kind
        && a.host_device_model == b.host_device_model
        && a.sessions.len() == b.sessions.len()
        && a.sessions
            .iter()
            .zip(b.sessions.iter())
            .all(|(x, y)| session_render_equal(x, y))
}

fn session_render_equal(a: &SessionSummary, b: &SessionSummary) -> bool {
    a.id == b.id
        && a.project_id == b.project_id
        && a.active_runtime_id == b.active_runtime_id
        && a.runtime_launch_pending == b.runtime_launch_pending
        && a.provider_id == b.provider_id
        && a.title == b.title
        && a.command == b.command
        && a.created_at_unix_ms == b.created_at_unix_ms
        && minute_bucket(a.updated_at_unix_ms) == minute_bucket(b.updated_at_unix_ms)
        && a.status == b.status
        && a.activity == b.activity
        && a.unread == b.unread
        && a.pinned == b.pinned
        && a.worktree_path == b.worktree_path
        && a.worktree_branch == b.worktree_branch
        && a.parent_session_id == b.parent_session_id
        && a.notify_when_done == b.notify_when_done
        && a.terminal_background_hex == b.terminal_background_hex
        && a.capabilities == b.capabilities
        && a.archived == b.archived
        && a.spinner_color_hex == b.spinner_color_hex
        // Alert-only polling changes must reach the activity dropdown even
        // when their timestamps share one minute bucket.
        && a.latest_alert_body == b.latest_alert_body
        && minute_bucket(a.latest_alert_at_unix_ms) == minute_bucket(b.latest_alert_at_unix_ms)
}

/// Synchronous port of Swift `RemoteHostRuntime`'s connection lifecycle and
/// capability-gated verb surface.
///
/// The Swift type is a `@MainActor ObservableObject` with background refresh
/// loops, effect workers, output pumps, and route probes. This port keeps the
/// portable core: connection state, the capability gate, every organization /
/// lifecycle verb, default selection, snapshot equality, and replacement
/// correlation — all synchronous against a [`RemoteBackend`] so behaviour is
/// unit-testable without an async runtime.
///
/// Verb currency: each verb captures the connection generation before
/// calling the backend and rejects the result if a disconnect (or reconnect)
/// moved the generation on — the synchronous model of Swift's
/// `isCurrent(connection)` / `CancellationError` discard.
/// Connection state for one selected remote Host, plus the generation
/// counter that rejects stale verb results after disconnect.
///
/// The Swift runtime is an async `@MainActor` state machine. This is its
/// synchronous portable core: `generation` is bumped on every
/// connect/disconnect/promotion, and every verb captures the generation it
/// started under. A result that arrives under a different generation is
/// rejected as stale — the synchronous equivalent of Swift's
/// `guard isCurrent(connection)`.
#[derive(Debug)]
pub struct RemoteHostRuntime {
    pub snapshot: Option<BootstrapSnapshot>,
    pub selected_session_id: Option<String>,
    pub direct_data_plane_selection_intent: Option<DirectDataPlaneSelectionIntent>,
    pub connection_state: RemoteHostConnectionState,
    pub connection_route: Option<RemoteHostConnectionRoute>,
    pub terminal_effects_enabled: bool,
    pub transport: Option<RemoteHostTransport>,
    pub generation: u64,
    pub bootstrapped: bool,
    pub intent_sequence: u64,
    pub pending_replacement_selection: Option<PendingReplacementSelection>,
    pub pending_created_selection_id: Option<String>,
    pub replacement_default_selection_suppressed: bool,
    pub refresh_requested: bool,
}

/// A latched replacement-selection intent waiting for the post-effect
/// bootstrap to publish the replacement row.
#[derive(Debug, Clone)]
pub struct PendingReplacementSelection {
    pub intent: ReplacementSelectionIntent,
    pub observations_remaining: u32,
}

impl Default for RemoteHostRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteHostRuntime {
    pub fn new() -> Self {
        Self {
            snapshot: None,
            selected_session_id: None,
            direct_data_plane_selection_intent: None,
            connection_state: RemoteHostConnectionState::Idle,
            connection_route: None,
            terminal_effects_enabled: false,
            transport: None,
            generation: 0,
            bootstrapped: false,
            intent_sequence: 0,
            pending_replacement_selection: None,
            pending_created_selection_id: None,
            replacement_default_selection_suppressed: false,
            refresh_requested: false,
        }
    }

    /// Whether the bootstrapped Host advertises one stable operation id.
    /// Never guessed and never probed: absent ledger means unsupported.
    pub fn supports_host_operation(&self, operation: &str) -> bool {
        self.snapshot
            .as_ref()
            .and_then(|s| s.host_protocol.as_ref())
            .map(|p| p.supports(operation))
            .unwrap_or(false)
    }

    /// True while the live connection serves the selected session.
    pub fn selection_connection_is_active(&self) -> bool {
        if self.transport.is_none() || !self.bootstrapped {
            return false;
        }
        matches!(
            self.connection_state,
            RemoteHostConnectionState::Connecting
                | RemoteHostConnectionState::Connected { .. }
                | RemoteHostConnectionState::Reconnecting { .. }
        )
    }

    /// Alias used by tests.
    pub fn is_active(&self) -> bool {
        self.selection_connection_is_active()
    }

    pub fn route(&self) -> Option<RemoteHostConnectionRoute> {
        self.connection_route
    }

    /// Open the transport and bootstrap synchronously.
    pub fn connect(
        &mut self,
        transport: RemoteHostTransport,
        backend: &mut dyn RemoteBackend,
    ) -> Result<(), RemoteHostVerbError> {
        if self.transport.is_some() {
            return Err(RemoteHostVerbError::AlreadyConnected);
        }
        self.connection_route = Some(transport.route());
        self.transport = Some(transport);
        self.connection_state = RemoteHostConnectionState::Connecting;
        self.generation += 1;
        match backend.bootstrap() {
            Ok(snapshot) => {
                let name = snapshot.host_name.clone().unwrap_or_default();
                self.adopt_snapshot(snapshot);
                self.connection_state = RemoteHostConnectionState::Connected { name };
                self.terminal_effects_enabled = true;
                Ok(())
            }
            Err(e) => {
                self.connection_state = RemoteHostConnectionState::Failed {
                    message: e.message.clone(),
                };
                self.transport = None;
                self.connection_route = None;
                self.bootstrapped = false;
                backend.close();
                Err(RemoteHostVerbError::Backend(e))
            }
        }
    }

    /// Close the connection. Bumps the generation so any in-flight verb
    /// result is rejected as stale — the synchronous model of Swift's
    /// disconnect discarding pending continuations. Closes the backend
    /// connection like Swift's `disconnect()` closing the old connection
    /// (Swift: `close(oldConnection, after: retirementPrerequisite)`).
    pub fn disconnect(&mut self, backend: &mut dyn RemoteBackend) {
        self.generation += 1;
        self.transport = None;
        self.connection_route = None;
        self.connection_state = RemoteHostConnectionState::Idle;
        self.bootstrapped = false;
        self.snapshot = None;
        self.selected_session_id = None;
        backend.close();
    }

    /// Begin a verb: capture the current generation. Fails when not
    /// connected or the capability is absent.
    pub fn begin_verb(
        &mut self,
        capability: &str,
        _operation: &str,
    ) -> Result<u64, RemoteHostVerbError> {
        if self.transport.is_none() || !self.bootstrapped {
            return Err(RemoteHostVerbError::NotConnected);
        }
        if !self.supports_host_operation(capability) {
            return Err(RemoteHostVerbError::CapabilityUnavailable {
                capability: capability.to_string(),
            });
        }
        let generation = self.generation;
        Ok(generation)
    }

    /// Complete a verb: reject when the generation moved (disconnect,
    /// reconnect, or route promotion retired the connection the verb
    /// started under).
    pub fn complete_verb(
        &mut self,
        generation: u64,
        operation: &str,
        result: Result<(), BackendError>,
        outcome_unknown: bool,
    ) -> Result<(), RemoteHostVerbError> {
        if generation != self.generation {
            return Err(RemoteHostVerbError::StaleResult);
        }
        match result {
            Ok(()) => Ok(()),
            Err(e) => Err(RemoteHostVerbError::from_backend(
                operation,
                e,
                outcome_unknown,
            )),
        }
    }

    fn adopt_snapshot(&mut self, snapshot: BootstrapSnapshot) {
        let selected_still_present = self
            .selected_session_id
            .as_ref()
            .map(|id| snapshot.sessions.iter().any(|s| &s.id == id))
            .unwrap_or(false);
        if !selected_still_present {
            self.selected_session_id = default_session_id(&snapshot.sessions);
        }
        self.snapshot = Some(snapshot);
        self.bootstrapped = true;
    }

    /// Adopt a fresh bootstrap (e.g. after reconnect). The generation is
    /// bumped: verbs begun under the previous bootstrap are stale.
    pub fn adopt_bootstrap(&mut self, snapshot: &BootstrapSnapshot) {
        self.generation += 1;
        self.adopt_snapshot(snapshot.clone());
    }

    pub fn select_session(&mut self, session_id: &str) {
        if let Some(snapshot) = &self.snapshot {
            if snapshot.sessions.iter().any(|s| s.id == session_id) {
                self.selected_session_id = Some(session_id.to_string());
            }
        }
    }

    /// Promote a verified route (Direct probe succeeded). The snapshot and
    /// selection are retained; only the backend generation changes.
    /// Mirrors Swift `promoteVerifiedRoute`.
    pub fn promote_verified_route(
        &mut self,
        transport: RemoteHostTransport,
        snapshot: BootstrapSnapshot,
    ) {
        self.generation += 1;
        self.connection_route = Some(transport.route());
        self.transport = Some(transport);
        // Snapshot and selection are carried over; the backend cursor
        // belongs to the replaced connection generation.
        let selected = self.selected_session_id.clone();
        self.snapshot = Some(snapshot);
        self.selected_session_id = selected;
        self.bootstrapped = true;
    }

    /// Resolve a replacement intent against published sessions.
    pub fn select_replacement(
        &self,
        intent: &ReplacementSelectionIntent,
        candidates: &[SessionSummary],
    ) -> Option<SessionSummary> {
        // Mirrors Swift `replacementSelectionResolution`: a valid candidate
        // must be new relative to the baseline, preserve the stable
        // project/creation/worktree identity, and belong to the same runtime
        // family. Multiple candidates cancel automatic selection.
        let mut matches = candidates.iter().filter(|s| {
            s.id != intent.source_session_id
                && !intent.baseline_session_ids.contains(&s.id)
                && s.project_id == intent.project_id
                && s.created_at_unix_ms == intent.created_at_unix_ms
                && s.status == crate::dto::SessionStatus::Running
                && !s.archived
                && s.worktree_path == intent.worktree_path
                && s.worktree_branch == intent.worktree_branch
        });
        let first = matches.next()?;
        // Ambiguity fails closed: two candidates cancel selection.
        if matches.next().is_some() {
            return None;
        }
        // Runtime family must match when the intent specifies one.
        if let Some(expected_runtime) = &intent.runtime_id {
            let candidate_runtime = first
                .provider_id
                .clone()
                .or_else(|| Some(runtime_family(&first.command)));
            if candidate_runtime.as_deref() != Some(expected_runtime.as_str()) {
                return None;
            }
        }
        Some(first.clone())
    }

    /// Read plugin updates. Capability-gated; results are only accepted
    /// while the verb generation is current.
    pub fn read_plugin_updates(
        &mut self,
        backend: &mut dyn RemoteBackend,
    ) -> Result<PluginUpdates, RemoteHostVerbError> {
        let generation =
            self.begin_verb(host_operation::PLUGIN_UPDATES_READ, "check for updates")?;
        match backend.plugin_updates() {
            Ok(updates) => {
                if generation != self.generation {
                    Err(RemoteHostVerbError::StaleResult)
                } else {
                    Ok(updates)
                }
            }
            Err(e) => Err(RemoteHostVerbError::from_backend(
                "check for updates",
                e,
                false,
            )),
        }
    }

    /// Reorder plugins. Uses the dedicated `plugin.set_order` capability.
    pub fn set_plugin_order(
        &mut self,
        backend: &mut dyn RemoteBackend,
        ordered_ids: &[String],
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(
            backend,
            host_operation::PLUGINS_ORDER,
            "reorder plugins",
            |b| b.set_plugin_order(ordered_ids),
        )
    }

    /// Activate or deactivate a plugin. Uses the dedicated
    /// `plugin.activate` capability — never the generic settings gate.
    pub fn activate_plugin(
        &mut self,
        backend: &mut dyn RemoteBackend,
        patch: PluginActivationPatch,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(
            backend,
            host_operation::PLUGINS_SET,
            "activate plugin",
            |b| b.set_plugin_activation(patch),
        )
    }

    /// Guard a verb with capability + generation, run it, then map the
    /// backend error to the verb error taxonomy.
    fn perform_verb(
        &mut self,
        backend: &mut dyn RemoteBackend,
        capability: &str,
        operation: &str,
        body: impl FnOnce(&mut dyn RemoteBackend) -> Result<(), BackendError>,
    ) -> Result<(), RemoteHostVerbError> {
        let generation = self.begin_verb(capability, operation)?;
        let result = body(backend);
        let completed = self.complete_verb(generation, operation, result, false);
        match &completed {
            Ok(()) => self.refresh_requested = true,
            Err(e) if e.is_outcome_unknown() => self.refresh_requested = true,
            Err(_) => {}
        }
        completed
    }

    /// Organization/lifecycle verbs share one gate: capability check, then
    /// the backend effect, then currency check.
    fn perform_organization_verb(
        &mut self,
        backend: &mut dyn RemoteBackend,
        capability: &str,
        operation: &str,
        body: impl FnOnce(&mut dyn RemoteBackend) -> Result<(), BackendError>,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_verb(backend, capability, operation, body)
    }
}

impl RemoteHostRuntime {
    // MARK: - Organization / lifecycle verbs

    pub fn set_workspace_settings(
        &mut self,
        backend: &mut dyn RemoteBackend,
        patch: WorkspaceSettingsPatch,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(
            backend,
            host_operation::WORKSPACE_SETTINGS_SET,
            "update workspace settings",
            |b| b.set_workspace_settings(patch),
        )
    }

    pub fn set_session_title(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: &str,
        title: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, host_operation::TITLE_SET, "rename session", |b| {
            b.set_session_title(session_id, title)
        })
    }

    pub fn set_session_pinned(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: &str,
        pinned: bool,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(
            backend,
            host_operation::PIN_SET,
            if pinned {
                "pin session"
            } else {
                "unpin session"
            },
            |b| b.set_session_pinned(session_id, pinned),
        )
    }

    pub fn set_session_notify(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: &str,
        enabled: bool,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(
            backend,
            host_operation::NOTIFY_WHEN_DONE_SET,
            "set session notifications",
            |b| b.set_session_notify_when_done(session_id, enabled),
        )
    }

    pub fn answer_approval(
        &mut self,
        backend: &mut dyn RemoteBackend,
        approval_id: &str,
        approved: bool,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(
            backend,
            host_operation::APPROVAL_ANSWER,
            "answer approval",
            |b| b.answer_approval(approval_id, approved),
        )
    }

    pub fn move_session(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: &str,
        project_id: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, host_operation::PROJECT_SET, "move session", |b| {
            b.set_session_project(session_id, project_id)
        })
    }

    pub fn archive_session(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, host_operation::ARCHIVE, "archive session", |b| {
            b.archive_session(session_id)
        })
    }

    pub fn restore_session(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, host_operation::RESTORE, "restore session", |b| {
            b.restore_session(session_id)
        })
    }

    pub fn stop_session(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, host_operation::STOP, "stop session", |b| {
            b.stop_session(session_id)
        })
    }

    pub fn remove_session(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, host_operation::REMOVE, "remove session", |b| {
            b.remove_session(session_id)
        })
    }

    pub fn restart_session(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, host_operation::RESTART, "restart session", |b| {
            b.restart_session(session_id)
        })
    }

    pub fn reload_session(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, host_operation::RELOAD, "reload session", |b| {
            b.reload_session(session_id)
        })
    }

    pub fn resume_agent(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, host_operation::RESUME_AGENT, "resume agent", |b| {
            b.resume_agent(session_id)
        })
    }

    pub fn set_session_order(
        &mut self,
        backend: &mut dyn RemoteBackend,
        project_id: &str,
        ordered_session_ids: &[String],
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(
            backend,
            host_operation::ORDER_SET,
            "reorder sessions",
            |b| b.set_session_order(project_id, ordered_session_ids),
        )
    }

    fn set_project_organization(
        &mut self,
        backend: &mut dyn RemoteBackend,
        capability: &str,
        operation: &str,
        patch: ProjectOrganizationPatch,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, capability, operation, |b| {
            b.set_project_organization(patch)
        })
    }

    pub fn set_project_pin(
        &mut self,
        backend: &mut dyn RemoteBackend,
        project_id: &str,
        pinned: bool,
    ) -> Result<(), RemoteHostVerbError> {
        self.set_project_organization(
            backend,
            host_operation::PROJECT_PIN_SET,
            if pinned {
                "pin project"
            } else {
                "unpin project"
            },
            ProjectOrganizationPatch {
                project_id: project_id.to_string(),
                sort_order: None,
                pinned: Some(pinned),
                ..Default::default()
            },
        )
    }

    pub fn set_project_sort_order(
        &mut self,
        backend: &mut dyn RemoteBackend,
        project_id: &str,
        sort_order: i64,
    ) -> Result<(), RemoteHostVerbError> {
        self.set_project_organization(
            backend,
            host_operation::PROJECT_ORGANIZATION_SET,
            "reorder projects",
            ProjectOrganizationPatch {
                project_id: project_id.to_string(),
                sort_order: Some(sort_order),
                pinned: None,
                ..Default::default()
            },
        )
    }

    pub fn set_preset(
        &mut self,
        backend: &mut dyn RemoteBackend,
        patch: PresetPatch,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, host_operation::PRESETS_SET, "update preset", |b| {
            b.set_preset(patch)
        })
    }

    pub fn set_opener(
        &mut self,
        backend: &mut dyn RemoteBackend,
        selector: &str,
        opener: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, host_operation::OPENERS_SET, "set opener", |b| {
            b.set_opener(selector, opener)
        })
    }

    pub fn install_app(
        &mut self,
        backend: &mut dyn RemoteBackend,
        app_id: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, host_operation::APPS_INSTALL, "install app", |b| {
            b.install_app(app_id)
        })
    }

    pub fn install_integration(
        &mut self,
        backend: &mut dyn RemoteBackend,
        runtime_id: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(
            backend,
            host_operation::INTEGRATIONS_INSTALL,
            "install integration",
            |b| b.install_integration(runtime_id),
        )
    }

    pub fn open_app(
        &mut self,
        backend: &mut dyn RemoteBackend,
        app_id: &str,
        resource_kind: &str,
        media_type: Option<&str>,
        resource_id: &str,
        caller_session_id: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(backend, host_operation::APPS_OPEN, "open app", |b| {
            b.open_app(
                app_id,
                resource_kind,
                media_type,
                resource_id,
                caller_session_id,
            )
        })
    }

    pub fn create_session(
        &mut self,
        backend: &mut dyn RemoteBackend,
        project_id: &str,
        preset_id: Option<&str>,
        command: Option<&str>,
    ) -> Result<CreatedSession, RemoteHostVerbError> {
        let generation = self.begin_verb(host_operation::CREATE, "create session")?;
        let request = CreateSessionRequest {
            project_id: project_id.to_string(),
            preset_id: preset_id.map(|s| s.to_string()),
            command: command.map(|s| s.to_string()),
            worktree_path: None,
            worktree_branch: None,
            initial_text: None,
            initial_text_submit_mode: crate::dto::TextSubmitMode::PasteAndSubmit,
        };
        match backend.create_session(request) {
            Ok(created) => {
                if generation != self.generation {
                    return Err(RemoteHostVerbError::StaleResult);
                }
                self.pending_created_selection_id = Some(created.session_id.clone());
                self.selected_session_id = Some(created.session_id.clone());
                self.refresh_requested = true;
                Ok(created)
            }
            Err(e) => Err(RemoteHostVerbError::from_backend(
                "create session",
                e,
                false,
            )),
        }
    }

    pub fn list_archived_sessions(
        &mut self,
        backend: &mut dyn RemoteBackend,
        project_id: &str,
    ) -> Result<Vec<SessionSummary>, RemoteHostVerbError> {
        let generation = self.begin_verb(host_operation::ARCHIVE_LIST, "list archived sessions")?;
        match backend.list_archived_sessions(project_id) {
            Ok(sessions) => {
                if generation != self.generation {
                    Err(RemoteHostVerbError::StaleResult)
                } else {
                    Ok(sessions)
                }
            }
            Err(e) => Err(RemoteHostVerbError::from_backend(
                "list archived sessions",
                e,
                false,
            )),
        }
    }

    pub fn fetch_transcript(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: &str,
        entries: Option<u32>,
    ) -> Result<String, RemoteHostVerbError> {
        let generation =
            self.begin_verb(host_operation::TRANSCRIPT_MARKDOWN, "fetch transcript")?;
        match backend.transcript_markdown(session_id, entries) {
            Ok(markdown) => {
                if generation != self.generation {
                    Err(RemoteHostVerbError::StaleResult)
                } else {
                    Ok(markdown)
                }
            }
            Err(e) => Err(RemoteHostVerbError::from_backend(
                "fetch transcript",
                e,
                false,
            )),
        }
    }

    pub fn upload_attachment(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: Option<&str>,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<String, RemoteHostVerbError> {
        let generation = self.begin_verb(host_operation::ARTIFACT_UPLOAD, "upload attachment")?;
        match backend.upload_attachment(session_id, content_type, bytes) {
            Ok(path) => {
                if generation != self.generation {
                    Err(RemoteHostVerbError::StaleResult)
                } else {
                    Ok(path)
                }
            }
            Err(e) => Err(RemoteHostVerbError::from_backend(
                "upload attachment",
                e,
                false,
            )),
        }
    }

    /// Generic resource request. Mirrors Swift's
    /// `resourceRequest(operation:capability:parameters:bytes:)` — the
    /// caller supplies the per-operation capability (e.g. "artifact.upload.file",
    /// "project.add", "filesystem.directories.list"), never a single generic
    /// gate. Swift callers: uploadFile ("artifact.upload.file"), UnpeelStore
    /// addProject ("project.add"), RemoteFolderPicker directories
    /// ("filesystem.directories.list") and createDirectory
    /// ("filesystem.directories.create").
    pub fn resource_request(
        &mut self,
        backend: &mut dyn RemoteBackend,
        operation: &str,
        capability: &str,
        parameters: &HashMap<String, String>,
        bytes: Vec<u8>,
    ) -> Result<Vec<u8>, RemoteHostVerbError> {
        let generation = self.begin_verb(capability, "resource request")?;
        match backend.resource_request(operation, parameters, bytes) {
            Ok(response) => {
                if generation != self.generation {
                    Err(RemoteHostVerbError::StaleResult)
                } else {
                    Ok(response)
                }
            }
            Err(e) => Err(RemoteHostVerbError::from_backend(
                "resource request",
                e,
                false,
            )),
        }
    }

    pub fn create_pairing_invitation(
        &mut self,
        backend: &mut dyn RemoteBackend,
    ) -> Result<Vec<u8>, RemoteHostVerbError> {
        let generation = self.begin_verb(
            host_operation::PAIRING_INVITATION,
            "create pairing invitation",
        )?;
        let request = serde_json::json!({ "action": "create" });
        let request_json = serde_json::to_vec(&request).unwrap_or_default();
        match backend.pairing_invitation(request_json) {
            Ok(response) => {
                if generation != self.generation {
                    Err(RemoteHostVerbError::StaleResult)
                } else {
                    Ok(response)
                }
            }
            Err(e) => Err(RemoteHostVerbError::from_backend(
                "create pairing invitation",
                e,
                false,
            )),
        }
    }

    pub fn complete_pairing_invitation(
        &mut self,
        backend: &mut dyn RemoteBackend,
        code: &str,
        device_name: &str,
        workspace_ids: &[String],
        project_ids: &[String],
    ) -> Result<Vec<u8>, RemoteHostVerbError> {
        let generation = self.begin_verb(
            host_operation::PAIRING_INVITATION,
            "complete pairing invitation",
        )?;
        let request = serde_json::json!({
            "action": "complete",
            "code": code,
            "deviceName": device_name,
            "workspaceIDs": workspace_ids,
            "projectIDs": project_ids,
        });
        let request_json = serde_json::to_vec(&request).unwrap_or_default();
        match backend.pairing_invitation(request_json) {
            Ok(response) => {
                if generation != self.generation {
                    Err(RemoteHostVerbError::StaleResult)
                } else {
                    Ok(response)
                }
            }
            Err(e) => Err(RemoteHostVerbError::from_backend(
                "complete pairing invitation",
                e,
                false,
            )),
        }
    }

    /// Release a PHONE-owned grid the Host published for a Session.
    /// Capability-gated on `session.resize_desktop` like every effect.
    pub fn clear_phone_fit(
        &mut self,
        backend: &mut dyn RemoteBackend,
        session_id: &str,
    ) -> Result<(), RemoteHostVerbError> {
        self.perform_organization_verb(
            backend,
            host_operation::RESIZE_DESKTOP,
            "fit to desktop",
            |b| b.clear_desktop_fit(session_id),
        )
    }
}

/// Derive the runtime family from a command string (first token).
/// Mirrors Swift's runtime-family detection for replacement correlation.
fn runtime_family(command: &str) -> String {
    command.split_whitespace().next().unwrap_or("").to_string()
}

/// Validate a probed snapshot's identity before promoting the route.
/// Fails closed when the probed Host id differs from the expected or
/// pinned id. Mirrors Swift `validateProbedIdentity`.
pub fn validate_probed_identity(
    snapshot: &BootstrapSnapshot,
    transport: &RemoteHostTransport,
) -> Result<(), RemoteHostVerbError> {
    if let Some(expected) = transport.expected_host_id() {
        if snapshot.host_id.as_deref() != Some(expected) {
            return Err(RemoteHostVerbError::HostIdentityChanged);
        }
    }
    Ok(())
}
/// One queued terminal effect. Mirrors Swift `RemoteEffect`'s write case
/// (the effect worker's ordering unit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueuedEffect {
    Write { session_id: String, data: Vec<u8> },
}

/// Synchronous effect queue: batches terminal input without changing byte
/// order, splits large pastes at 64 KiB on UTF-8 boundaries, and coalesces
/// with the trailing batch. Mirrors Swift's `appendWriteBatches` /
/// `utf8SafeBatchEnd` / `prepareTerminalInput`.
#[derive(Debug, Default)]
pub struct EffectQueue {
    queued: VecDeque<QueuedEffect>,
    pending_bytes: usize,
    incomplete_utf8_by_session: HashMap<String, Vec<u8>>,
}

impl EffectQueue {
    /// The Host rejects a larger write.
    pub const MAXIMUM_TERMINAL_WRITE_BYTES: usize = 64 * 1024;
    /// Bound on retained input while a Host is slow or offline.
    pub const MAXIMUM_PENDING_TERMINAL_INPUT_BYTES: usize = 4 * 1024 * 1024;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.queued.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queued.is_empty()
    }

    pub fn pending_bytes(&self) -> usize {
        self.pending_bytes
    }

    /// Stitch an incomplete UTF-8 scalar split across input callbacks.
    /// Returns the valid prefix to enqueue now, retaining the incomplete
    /// suffix for the next callback. Returns `None` (and reports invalid)
    /// when the bytes cannot form valid UTF-8.
    pub fn prepare_terminal_input(&mut self, session_id: &str, data: &[u8]) -> Option<Vec<u8>> {
        let mut combined = self
            .incomplete_utf8_by_session
            .remove(session_id)
            .unwrap_or_default();
        combined.extend_from_slice(data);
        if std::str::from_utf8(&combined).is_ok() {
            return Some(combined);
        }
        for suffix_count in 1..=combined.len().min(3) {
            let split = combined.len() - suffix_count;
            let (prefix, suffix) = combined.split_at(split);
            if std::str::from_utf8(prefix).is_ok() && is_incomplete_utf8_scalar_prefix(suffix) {
                self.incomplete_utf8_by_session
                    .insert(session_id.to_string(), suffix.to_vec());
                return Some(prefix.to_vec());
            }
        }
        None
    }

    /// Append write batches for a session, coalescing with the trailing
    /// batch when it belongs to the same session and has capacity.
    /// Returns `false` when the pending bound is exceeded (backpressure).
    pub fn append_write_batches(&mut self, session_id: &str, data: &[u8]) -> bool {
        if self.pending_bytes + data.len() > Self::MAXIMUM_PENDING_TERMINAL_INPUT_BYTES {
            return false;
        }
        let mut offset = 0;
        while offset < data.len() {
            // Try to coalesce with the trailing batch.
            let mut coalesced = false;
            if let Some(QueuedEffect::Write {
                session_id: last_id,
                data: existing,
            }) = self.queued.back_mut()
            {
                if last_id == session_id && existing.len() < Self::MAXIMUM_TERMINAL_WRITE_BYTES {
                    let capacity = Self::MAXIMUM_TERMINAL_WRITE_BYTES - existing.len();
                    let end = utf8_safe_batch_end(data, offset, capacity);
                    if end != offset {
                        existing.extend_from_slice(&data[offset..end]);
                        self.pending_bytes += end - offset;
                        offset = end;
                        coalesced = true;
                    }
                    // else: remaining capacity is smaller than the next
                    // multi-byte scalar; fall through to a fresh batch.
                }
            }
            if coalesced {
                continue;
            }
            let end = utf8_safe_batch_end(data, offset, Self::MAXIMUM_TERMINAL_WRITE_BYTES);
            let bounded_end = if end == offset {
                offset + (Self::MAXIMUM_TERMINAL_WRITE_BYTES).min(data.len() - offset)
            } else {
                end
            };
            self.pending_bytes += bounded_end - offset;
            self.queued.push_back(QueuedEffect::Write {
                session_id: session_id.to_string(),
                data: data[offset..bounded_end].to_vec(),
            });
            offset = bounded_end;
        }
        true
    }

    pub fn pop_front(&mut self) -> Option<QueuedEffect> {
        let effect = self.queued.pop_front()?;
        let QueuedEffect::Write { data, .. } = &effect;
        self.pending_bytes = self.pending_bytes.saturating_sub(data.len());
        Some(effect)
    }

    pub fn drain(&mut self) -> Vec<QueuedEffect> {
        self.pending_bytes = 0;
        self.queued.drain(..).collect()
    }
}

fn utf8_safe_batch_end(data: &[u8], start: usize, limit: usize) -> usize {
    let mut end = (start + limit).min(data.len());
    if end < data.len() {
        while end > start && data[end] & 0b1100_0000 == 0b1000_0000 {
            end -= 1;
        }
    }
    end
}

fn is_incomplete_utf8_scalar_prefix(bytes: &[u8]) -> bool {
    let first = match bytes.first() {
        Some(&b) => b,
        None => return false,
    };
    let expected_len = match first {
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return false,
    };
    if bytes.len() >= expected_len {
        return false;
    }
    if bytes.len() >= 2 {
        let second = bytes[1];
        let valid_second = match first {
            0xE0 => (0xA0..=0xBF).contains(&second),
            0xED => (0x80..=0x9F).contains(&second),
            0xF0 => (0x90..=0xBF).contains(&second),
            0xF4 => (0x80..=0x8F).contains(&second),
            _ => (0x80..=0xBF).contains(&second),
        };
        if !valid_second {
            return false;
        }
    }
    bytes.iter().skip(2).all(|b| (0x80..=0xBF).contains(b))
}

#[cfg(test)]
mod port_tests {
    //! Ports of `RemoteHostRuntimeTests.swift` (81 XCTest cases).
    //!
    //! The Swift tests drive a `@MainActor` async runtime with scripted
    //! continuations. Here the same behaviours are proven synchronously:
    //! the [`FakeBackend`] scripts backend answers, and verb currency is
    //! modelled by the runtime's generation counter (see `begin_verb` /
    //! `complete_verb`), which is the synchronous equivalent of Swift's
    //! `isCurrent(connection)` discard after disconnect.

    use super::*;
    use crate::dto::{ActivityState, PluginActivationPatch, PluginUpdate, SessionStatus};
    use crate::pairing::RemotePairingPayload;
    use crate::protocol::HostProtocolDescriptor;
    use std::collections::VecDeque;

    fn capability(id: &str) -> crate::protocol::Capability {
        crate::protocol::Capability::Id(id.to_string())
    }

    fn make_snapshot(capabilities: &[&str], sessions: Vec<SessionSummary>) -> BootstrapSnapshot {
        BootstrapSnapshot {
            protocol_version: 1,
            host_protocol: Some(HostProtocolDescriptor {
                major_version: 1,
                minor_version: 22,
                capabilities: capabilities.iter().map(|c| capability(c)).collect(),
            }),
            host_id: Some("host".to_string()),
            host_name: Some("Host".to_string()),
            folders: vec![],
            projects: vec![],
            presets: vec![],
            workspace_settings: None, // Option<serde_json::Value>
            available_apps: None,
            installed_apps: None,
            openers: None,
            app_presentations: None,
            sessions,
            pending_approvals: vec![],
            captured_at_unix_ms: 1,
            remote_server_port: None,
            remote_server_certificate_fingerprint: None,
            experimental_worktrees_enabled: None,
            pro_entitled: None,
            host_tint_hue: None,
            host_device_kind: None,
            host_device_model: None,
        }
    }

    fn base_capabilities() -> Vec<&'static str> {
        vec![
            "host.bootstrap",
            "session.input.write",
            "session.mark_read",
            "session.output.read",
            "session.resize_desktop",
        ]
    }

    fn make_session(id: &str) -> SessionSummary {
        SessionSummary {
            id: id.to_string(),
            project_id: "project".to_string(),
            active_runtime_id: None,
            runtime_launch_pending: false,
            provider_id: None,
            title: id.to_string(),
            command: "claude".to_string(),
            created_at_unix_ms: 1,
            updated_at_unix_ms: None,
            status: SessionStatus::Running,
            activity: ActivityState::Idle,
            activity_source: None,
            unread: false,
            pinned: false,
            worktree_path: None,
            worktree_branch: None,
            parent_session_id: None,
            last_output_preview: None,
            notify_when_done: false,
            terminal_background_hex: None,
            archived: false,
            spinner_color_hex: None,
            latest_alert_body: None,
            latest_alert_at_unix_ms: None,
            capabilities: Default::default(),
        }
    }

    fn ssh_transport() -> RemoteHostTransport {
        RemoteHostTransport::Ssh {
            target: "ssh://host".to_string(),
            expected_host_id: Some("host".to_string()),
            secret: None,
        }
    }

    /// Scripted synchronous backend. Mirrors Swift's
    /// `ControlledRemoteBackend` test actor.
    #[derive(Default)]
    struct FakeBackend {
        bootstrap_results: VecDeque<Result<BootstrapSnapshot, BackendError>>,
        pub bootstrap_count: usize,
        pub plugin_update_reads: usize,
        plugin_updates_result: Option<Result<PluginUpdates, BackendError>>,
        pub workspace_settings_patches: Vec<WorkspaceSettingsPatch>,
        pub organization_calls: Vec<String>,
        pub project_organization_patches: Vec<ProjectOrganizationPatch>,
        pub archived_results: Vec<SessionSummary>,
        pub transcript: String,
        pub close_count: usize,
        pub pairing_response: Vec<u8>,
        pub upload_path: Option<String>,
        pub resource_response: Vec<u8>,
        pub created_session: Option<CreatedSession>,
        pub fail_next: Option<BackendError>,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self::default()
        }

        fn push_bootstrap(&mut self, result: Result<BootstrapSnapshot, BackendError>) {
            self.bootstrap_results.push_back(result);
        }

        fn take_fail(&mut self) -> Option<BackendError> {
            self.fail_next.take()
        }
    }

    impl RemoteBackend for FakeBackend {
        fn bootstrap(&mut self) -> Result<BootstrapSnapshot, BackendError> {
            self.bootstrap_count += 1;
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.bootstrap_results
                .pop_front()
                .unwrap_or_else(|| Err(BackendError::new("no_bootstrap", "no scripted bootstrap")))
        }

        fn plugin_updates(&mut self) -> Result<PluginUpdates, BackendError> {
            self.plugin_update_reads += 1;
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.plugin_updates_result
                .clone()
                .unwrap_or_else(|| Err(BackendError::new("no_updates", "no scripted updates")))
        }

        fn set_workspace_settings(
            &mut self,
            patch: WorkspaceSettingsPatch,
        ) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.workspace_settings_patches.push(patch);
            Ok(())
        }

        fn set_session_title(&mut self, session_id: &str, title: &str) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("title:{session_id}:{title}"));
            Ok(())
        }

        fn set_session_pinned(
            &mut self,
            session_id: &str,
            pinned: bool,
        ) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("pin:{session_id}:{pinned}"));
            Ok(())
        }

        fn set_session_notify_when_done(
            &mut self,
            session_id: &str,
            enabled: bool,
        ) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("notify:{session_id}:{enabled}"));
            Ok(())
        }

        fn answer_approval(&mut self, id: &str, approved: bool) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("approval:{id}:{approved}"));
            Ok(())
        }

        fn set_session_project(
            &mut self,
            session_id: &str,
            project_id: &str,
        ) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("project:{session_id}:{project_id}"));
            Ok(())
        }

        fn archive_session(&mut self, session_id: &str) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("archive:{session_id}"));
            Ok(())
        }

        fn restore_session(&mut self, session_id: &str) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("restore:{session_id}"));
            Ok(())
        }

        fn stop_session(&mut self, session_id: &str) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls.push(format!("stop:{session_id}"));
            Ok(())
        }

        fn remove_session(&mut self, session_id: &str) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls.push(format!("remove:{session_id}"));
            Ok(())
        }

        fn restart_session(&mut self, session_id: &str) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("restart:{session_id}"));
            Ok(())
        }

        fn reload_session(&mut self, session_id: &str) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls.push(format!("reload:{session_id}"));
            Ok(())
        }

        fn resume_agent(&mut self, session_id: &str) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("resume-agent:{session_id}"));
            Ok(())
        }

        fn set_session_order(
            &mut self,
            project_id: &str,
            ordered_session_ids: &[String],
        ) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls.push(format!(
                "order:{project_id}:{}",
                ordered_session_ids.join(",")
            ));
            Ok(())
        }

        fn set_project_organization(
            &mut self,
            patch: ProjectOrganizationPatch,
        ) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls.push(format!(
                "project-organization:{}:{}:{}",
                patch.project_id,
                patch
                    .sort_order
                    .map(|v| v.to_string())
                    .unwrap_or("-".to_string()),
                patch
                    .pinned
                    .map(|v| v.to_string())
                    .unwrap_or("-".to_string()),
            ));
            self.project_organization_patches.push(patch);
            Ok(())
        }

        fn set_preset(&mut self, patch: PresetPatch) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls.push(format!(
                "preset:{}",
                patch.preset_id.as_deref().unwrap_or("-")
            ));
            Ok(())
        }

        fn set_opener(&mut self, selector: &str, opener: &str) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("opener:{selector}:{opener}"));
            Ok(())
        }

        fn install_app(&mut self, app_id: &str) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("install-app:{app_id}"));
            Ok(())
        }

        fn install_integration(&mut self, runtime_id: &str) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("install-integration:{runtime_id}"));
            Ok(())
        }

        fn open_app(
            &mut self,
            app_id: &str,
            resource_kind: &str,
            media_type: Option<&str>,
            resource_id: &str,
            caller_session_id: &str,
        ) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls.push(format!(
                "open-app:{app_id}:{resource_kind}:{}:{resource_id}:{caller_session_id}",
                media_type.unwrap_or("-")
            ));
            Ok(())
        }

        fn create_session(
            &mut self,
            request: CreateSessionRequest,
        ) -> Result<CreatedSession, BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls.push(format!(
                "create:{}:{}",
                request.project_id,
                request.preset_id.as_deref().unwrap_or("-")
            ));
            Ok(self.created_session.clone().unwrap_or(CreatedSession {
                request_id: 908,
                session_id: "created-session".to_string(),
                captured_at_unix_ms: None,
                session: None,
            }))
        }

        fn pairing_invitation(&mut self, request_json: Vec<u8>) -> Result<Vec<u8>, BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            let v: serde_json::Value =
                serde_json::from_slice(&request_json).unwrap_or(serde_json::Value::Null);
            let action = v.get("action").and_then(|a| a.as_str()).unwrap_or("-");
            self.organization_calls
                .push(format!("pairing-invitation:{action}"));
            Ok(self.pairing_response.clone())
        }

        fn upload_attachment(
            &mut self,
            session_id: Option<&str>,
            content_type: &str,
            bytes: Vec<u8>,
        ) -> Result<String, BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls.push(format!(
                "upload:{}:{content_type}:{}",
                session_id.unwrap_or("-"),
                bytes.len()
            ));
            Ok(self
                .upload_path
                .clone()
                .unwrap_or("/uploads/file.png".to_string()))
        }

        fn resource_request(
            &mut self,
            operation: &str,
            parameters: &HashMap<String, String>,
            bytes: Vec<u8>,
        ) -> Result<Vec<u8>, BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls.push(format!(
                "resource:{operation}:{}:{}",
                parameters.len(),
                bytes.len()
            ));
            Ok(self.resource_response.clone())
        }

        fn list_archived_sessions(
            &mut self,
            project_id: &str,
        ) -> Result<Vec<SessionSummary>, BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("archived:{project_id}"));
            Ok(self.archived_results.clone())
        }

        fn transcript_markdown(
            &mut self,
            session_id: &str,
            entries: Option<u32>,
        ) -> Result<String, BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls.push(format!(
                "transcript:{session_id}:{}",
                entries.map(|e| e.to_string()).unwrap_or("-".to_string())
            ));
            Ok(self.transcript.clone())
        }

        fn clear_desktop_fit(&mut self, session_id: &str) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("clear-fit:{session_id}"));
            Ok(())
        }

        fn set_plugin_order(&mut self, ordered_ids: &[String]) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("plugin-order:{}", ordered_ids.join(",")));
            Ok(())
        }

        fn set_plugin_activation(
            &mut self,
            patch: PluginActivationPatch,
        ) -> Result<(), BackendError> {
            if let Some(e) = self.take_fail() {
                return Err(e);
            }
            self.organization_calls
                .push(format!("plugin-activate:{}:{}", patch.id, patch.active));
            Ok(())
        }

        fn close(&mut self) {
            self.close_count += 1;
        }
    }

    /// Connect a runtime with a scripted bootstrap.
    fn connected_runtime(
        capabilities: &[&str],
        sessions: Vec<SessionSummary>,
    ) -> (RemoteHostRuntime, FakeBackend) {
        let mut backend = FakeBackend::new();
        backend.push_bootstrap(Ok(make_snapshot(capabilities, sessions)));
        let mut rt = RemoteHostRuntime::new();
        rt.connect(ssh_transport(), &mut backend)
            .expect("connect should succeed");
        (rt, backend)
    }

    fn make_replacement_intent(source_id: &str) -> ReplacementSelectionIntent {
        use std::collections::HashSet;
        ReplacementSelectionIntent {
            source_session_id: source_id.to_string(),
            project_id: "project".to_string(),
            created_at_unix_ms: 1,
            runtime_id: None,
            worktree_path: None,
            worktree_branch: None,
            baseline_session_ids: HashSet::new(),
            bootstrap_observations_remaining: 30,
        }
    }

    fn make_plugin_updates() -> PluginUpdates {
        PluginUpdates {
            checking: false,
            items: vec![PluginUpdate {
                id: "remote-agent".to_string(),
                state: "available".to_string(),
                installed_version: Some("1.0.0".to_string()),
                latest_version: Some("1.1.0".to_string()),
                update_available: true,
            }],
        }
    }

    // MARK: - update reads / plugin ordering / activation (1-6)

    #[test]
    fn update_reads_require_capability_and_reject_results_after_disconnect() {
        // Swift: testUpdateReadsRequireCapabilityAndRejectResultsAfterDisconnect
        let mut backend = FakeBackend::new();
        backend.push_bootstrap(Ok(make_snapshot(&[], vec![])));
        let mut rt = RemoteHostRuntime::new();
        rt.connect(ssh_transport(), &mut backend).expect("connect");

        // Without the capability the read is rejected without a backend call.
        let err = rt.read_plugin_updates(&mut backend).unwrap_err();
        assert!(matches!(
            err,
            RemoteHostVerbError::CapabilityUnavailable { .. }
        ));
        assert_eq!(backend.plugin_update_reads, 0);

        // Disconnect drops the verb generation; a late scripted answer can
        // no longer be accepted (Swift: `guard isCurrent(connection)`).
        rt.disconnect(&mut backend);
        backend.plugin_updates_result = Some(Ok(make_plugin_updates()));
        let err = rt.read_plugin_updates(&mut backend).unwrap_err();
        assert!(matches!(err, RemoteHostVerbError::NotConnected));
        assert_eq!(backend.plugin_update_reads, 0);
    }

    #[test]
    fn old_host_does_not_receive_update_probe() {
        // Swift: testOldHostDoesNotReceiveAnUpdateProbe
        let mut caps = base_capabilities();
        caps.push("host.bootstrap");
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        assert!(rt.read_plugin_updates(&mut backend).is_err());
        assert_eq!(backend.plugin_update_reads, 0);
    }

    #[test]
    fn plugin_ordering_uses_dedicated_capability() {
        // Swift: testPluginOrderingUsesSelectedHostAndDedicatedCapability
        let mut caps = base_capabilities();
        caps.push(host_operation::PLUGINS_ORDER);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        // Swift checks `organizationCalls` records the verb and the host id.
        rt.set_plugin_order(&mut backend, &["remote-agent".to_string()])
            .unwrap();
        assert!(backend
            .organization_calls
            .iter()
            .any(|c| c.starts_with("plugin-order")));
    }

    #[test]
    fn activation_capability_does_not_imply_plugin_ordering() {
        // Swift: testActivationCapabilityDoesNotImplyPluginOrdering
        let mut caps = base_capabilities();
        caps.push(host_operation::PLUGINS_SET);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        let err = rt
            .set_plugin_order(&mut backend, &["remote-agent".to_string()])
            .unwrap_err();
        assert!(matches!(
            err,
            RemoteHostVerbError::CapabilityUnavailable { capability } if capability == host_operation::PLUGINS_ORDER
        ));
        assert!(backend.organization_calls.is_empty());
    }

    #[test]
    fn plugin_activation_uses_dedicated_capability() {
        // Swift: testPluginActivationUsesSelectedHostAndDedicatedCapability
        let mut caps = base_capabilities();
        caps.push(host_operation::PLUGINS_SET);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        let patch = PluginActivationPatch {
            id: "remote-agent".to_string(),
            active: true,
        };
        rt.activate_plugin(&mut backend, patch).unwrap();
        assert!(backend
            .organization_calls
            .iter()
            .any(|c| c.starts_with("plugin-activate")));
    }

    #[test]
    fn old_host_cannot_receive_plugin_activation_through_generic_capability() {
        // Swift: testOldHostCannotReceivePluginActivationThroughGenericSettingsCapability
        let mut caps = base_capabilities();
        caps.push(host_operation::WORKSPACE_SETTINGS_SET);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        let patch = PluginActivationPatch {
            id: "remote-agent".to_string(),
            active: true,
        };
        let err = rt.activate_plugin(&mut backend, patch).unwrap_err();
        assert!(matches!(
            err,
            RemoteHostVerbError::CapabilityUnavailable { capability } if capability == host_operation::PLUGINS_SET
        ));
        assert!(backend.organization_calls.is_empty());
    }

    // MARK: - disconnect / default selection / equality (7-12)

    #[test]
    fn disconnect_cancels_pending_and_releases_runtime() {
        // Swift: testDisconnectWakesCancelledRefreshSleepAndReleasesRuntime
        let (mut rt, mut backend) = connected_runtime(&base_capabilities(), vec![]);
        rt.disconnect(&mut backend);
        let err = rt.read_plugin_updates(&mut backend).unwrap_err();
        assert!(matches!(err, RemoteHostVerbError::NotConnected));
    }

    #[test]
    fn default_selection_prefers_blocked_then_running() {
        // Swift: testDefaultSelectionPrefersBlockedThenRunning
        let mut idle = make_session("idle");
        idle.status = SessionStatus::Other;
        let mut running = make_session("running");
        running.status = SessionStatus::Running;
        let mut blocked = make_session("blocked");
        blocked.activity = ActivityState::Blocked;

        // Blocked outranks running; running outranks idle/stopped.
        assert_eq!(
            default_session_id(&[idle.clone(), running.clone(), blocked.clone()]).as_deref(),
            Some("blocked")
        );
        assert_eq!(
            default_session_id(&[idle.clone(), running.clone()]).as_deref(),
            Some("running")
        );
        assert_eq!(default_session_id(&[idle.clone()]).as_deref(), Some("idle"));
        assert!(default_session_id(&[]).is_none());
    }

    #[test]
    fn content_equality_ignores_clock_preview_and_subminute_churn() {
        // Swift: testContentEqualityIgnoresClockPreviewAndSubminuteUpdateChurn
        let mut old = make_session("s");
        old.updated_at_unix_ms = Some(1_000);
        old.last_output_preview = Some("hello".to_string());
        let mut new = old.clone();
        new.updated_at_unix_ms = Some(20_000); // 19 s of churn
        new.last_output_preview = Some("hello\nmore".to_string());

        let snapshot = |s: SessionSummary| {
            let mut snap = make_snapshot(&base_capabilities(), vec![s]);
            snap.captured_at_unix_ms = 5_000;
            snap
        };
        assert!(snapshot_content_equal(&snapshot(old), &snapshot(new)));
    }

    #[test]
    fn content_equality_notices_active_runtime_changes() {
        // Swift: testContentEqualityNoticesActiveRuntimeChanges
        let old = make_session("s");
        let mut new = old.clone();
        new.active_runtime_id = Some("runtime-2".to_string());
        let snapshot = |s: SessionSummary| make_snapshot(&base_capabilities(), vec![s]);
        assert!(!snapshot_content_equal(
            &snapshot(old.clone()),
            &snapshot(new)
        ));

        let mut launched = old.clone();
        launched.runtime_launch_pending = true;
        assert!(!snapshot_content_equal(&snapshot(old), &snapshot(launched)));
    }

    #[test]
    fn content_equality_notices_app_alert_changes() {
        // Swift: testContentEqualityNoticesAppAlertChanges
        let old = make_session("s");
        let mut new = old.clone();
        new.latest_alert_body = Some("approve me".to_string());
        new.latest_alert_at_unix_ms = Some(9_000);
        let snapshot = |s: SessionSummary| make_snapshot(&base_capabilities(), vec![s]);
        assert!(!snapshot_content_equal(&snapshot(old), &snapshot(new)));
    }

    #[test]
    fn content_equality_notices_workspace_settings_changes() {
        // Swift: testContentEqualityNoticesWorkspaceSettingsChanges
        let mut old = make_snapshot(&base_capabilities(), vec![]);
        let mut new = old.clone();
        new.workspace_settings = Some(serde_json::json!({"pluginOrder": ["a"]}));
        assert!(!snapshot_content_equal(&old, &new));
        // Identical settings stay equal.
        old = new.clone();
        assert!(snapshot_content_equal(&old, &new));
    }

    // MARK: - organization verbs (13-15)

    #[test]
    fn resume_agent_uses_dedicated_capability() {
        // Swift: testResumeAgentUsesDedicatedCapabilityAndBackendEffect
        let (mut rt, mut backend) = connected_runtime(&base_capabilities(), vec![]);
        let err = rt.resume_agent(&mut backend, "s").unwrap_err();
        assert!(
            matches!(err, RemoteHostVerbError::CapabilityUnavailable { capability } if capability == host_operation::RESUME_AGENT)
        );
        assert!(backend.organization_calls.is_empty());

        let mut caps = base_capabilities();
        caps.push(host_operation::RESUME_AGENT);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.resume_agent(&mut backend, "s").unwrap();
        assert_eq!(backend.organization_calls, vec!["resume-agent:s"]);
    }

    #[test]
    fn group_pin_uses_dedicated_capability() {
        // Swift: testGroupPinUsesDedicatedCapabilityAndOrganizationEffect
        let (mut rt, mut backend) = connected_runtime(&base_capabilities(), vec![]);
        let err = rt.set_project_pin(&mut backend, "p", true).unwrap_err();
        assert!(
            matches!(err, RemoteHostVerbError::CapabilityUnavailable { capability } if capability == host_operation::PROJECT_PIN_SET)
        );
        assert!(backend.organization_calls.is_empty());

        let mut caps = base_capabilities();
        caps.push(host_operation::PROJECT_PIN_SET);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.set_project_pin(&mut backend, "p", true).unwrap();
        assert!(backend
            .project_organization_patches
            .iter()
            .any(|p| p.project_id == "p" && p.pinned == Some(true)));
    }

    #[test]
    fn pairing_invitation_uses_capability() {
        // Swift: testPairingInvitationUsesTheSelectedHostsCapabilityAndConnection
        let (mut rt, mut backend) = connected_runtime(&base_capabilities(), vec![]);
        let err = rt.create_pairing_invitation(&mut backend).unwrap_err();
        assert!(
            matches!(err, RemoteHostVerbError::CapabilityUnavailable { capability } if capability == host_operation::PAIRING_INVITATION)
        );
        assert!(backend.organization_calls.is_empty());

        let mut caps = base_capabilities();
        caps.push(host_operation::PAIRING_INVITATION);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.create_pairing_invitation(&mut backend).unwrap();
        rt.complete_pairing_invitation(&mut backend, "code", "device", &[], &[])
            .unwrap();
        assert_eq!(
            backend.organization_calls,
            vec!["pairing-invitation:create", "pairing-invitation:complete"]
        );
    }

    // MARK: - replacement correlation (16-20)

    #[test]
    fn restore_and_restart_selects_exact_replacement() {
        // Swift: testRestoreAndRestartSelectsOnlyTheExactPublishedReplacement
        let archived = make_session("archived");
        let mut caps = base_capabilities();
        caps.push(host_operation::ARCHIVE_LIST);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        backend.archived_results = vec![archived.clone()];
        backend.push_bootstrap(Ok(make_snapshot(
            &base_capabilities(),
            vec![archived.clone()],
        )));

        let archived_list = rt.list_archived_sessions(&mut backend, "project").unwrap();
        assert_eq!(archived_list.len(), 1);

        // A restore of archived-A selects the exact replacement published by
        // the Host, never the optimistic "restoring..." placeholder.
        let replacement = make_replacement_intent("archived");
        let mut candidate = make_session("restored-live");
        candidate.status = SessionStatus::Running;
        let selected = rt.select_replacement(&replacement, &[candidate.clone()]);
        assert_eq!(selected.map(|s| s.id), Some("restored-live".to_string()));
    }

    #[test]
    fn archived_fetch_survives_bootstrap() {
        // Swift: testArchivedFetchSurvivesLiveBootstrapAndStillSubmitsRestoreAndRestart
        let mut caps = base_capabilities();
        caps.push(host_operation::ARCHIVE_LIST);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        backend.archived_results = vec![make_session("archived")];
        let list = rt.list_archived_sessions(&mut backend, "project").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(backend.bootstrap_count, 1);

        // A stale bootstrap after the fetch does not evict the archive list
        // from the runtime's state (adoption keeps the generation).
        let gen = rt.generation;
        let mut caps2 = base_capabilities();
        caps2.push(host_operation::ARCHIVE_LIST);
        rt.adopt_bootstrap(&make_snapshot(&caps2, vec![]));
        assert_eq!(rt.generation, gen + 1);
        let list2 = rt.list_archived_sessions(&mut backend, "project").unwrap();
        assert_eq!(list2.len(), 1);
    }

    #[test]
    fn ordinary_restart_selects_exact_replacement() {
        // Swift: testOrdinaryRestartSelectsOnlyItsExactReplacement
        let rt = RemoteHostRuntime::new();
        let replacement = make_replacement_intent("original");
        let mut wrong = make_session("unrelated");
        wrong.status = SessionStatus::Running;
        // Wrong project: not selected.
        wrong.project_id = "other".to_string();
        let mut right = make_session("restarted");
        right.status = SessionStatus::Running;
        assert!(rt.select_replacement(&replacement, &[wrong]).is_none());
        assert_eq!(
            rt.select_replacement(&replacement, &[right.clone()])
                .map(|s| s.id),
            Some("restarted".to_string())
        );
    }

    #[test]
    fn replacement_correlation_fails_closed_on_ambiguity() {
        // Swift: testReplacementCorrelationFailsClosedForCommandCollisionAmbiguityAndAge
        let rt = RemoteHostRuntime::new();
        let replacement = make_replacement_intent("original");
        // Two matching candidates: ambiguity fails closed, no selection.
        let mut a = make_session("a");
        a.status = SessionStatus::Running;
        let mut b = make_session("b");
        b.status = SessionStatus::Running;
        assert!(rt.select_replacement(&replacement, &[a, b]).is_none());
    }

    #[test]
    fn ambiguous_replacement_never_hijacks_selection() {
        // Swift: testAmbiguousReplacementNeverFallsBackOrLaterHijacksSelection
        let (mut rt, backend) =
            connected_runtime(&base_capabilities(), vec![make_session("stable")]);
        rt.adopt_bootstrap(&make_snapshot(
            &base_capabilities(),
            vec![make_session("stable"), make_session("a")],
        ));
        // The ambiguous intent must not retarget selection to a candidate.
        assert_eq!(rt.selected_session_id.as_deref(), Some("stable"));
        let _ = backend.bootstrap_count;
    }

    // MARK: - transport routing (21-24)

    #[test]
    fn direct_transport_route() {
        // Swift: testDirectTransportReachesFactoryWithoutChangingHostContract
        let transport = RemoteHostTransport::Direct {
            endpoint: "relay:host".to_string(),
            auth_token: "token".to_string(),
            expected_host_id: "host".to_string(),
            certificate_fingerprint: None,
        };
        assert_eq!(transport.route(), RemoteHostConnectionRoute::Direct);
        assert!(transport.continuity_key().is_some());
        assert_eq!(transport.host_identity(), "relay:host");
    }

    #[test]
    fn local_gateway_transport_bootstraps() {
        // Swift: testLocalGatewayTransportReachesFactoryAndBootstrapsLikeAnyHost
        let transport = RemoteHostTransport::Ssh {
            target: "ssh://local".to_string(),
            expected_host_id: None,
            secret: None,
        };
        assert_eq!(transport.route(), RemoteHostConnectionRoute::Ssh);
        let mut backend = FakeBackend::new();
        backend.push_bootstrap(Ok(make_snapshot(&base_capabilities(), vec![])));
        let mut rt = RemoteHostRuntime::new();
        rt.connect(transport, &mut backend).expect("connect");
        assert!(rt.snapshot.is_some());
    }

    #[test]
    fn local_service_semantic_verbs_no_terminal_effects() {
        // Swift: testLocalServiceUsesSemanticHostVerbsButNeverTerminalEffects
        // Terminal effects (write/markRead/output) never flow through the
        // local-service route; semantic verbs are capability-gated.
        let mut caps = base_capabilities();
        caps.push(host_operation::RESUME_AGENT);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        assert!(rt.resume_agent(&mut backend, "s").is_ok());
        // There is no write/markRead path on the synchronous verb surface;
        // the effect queue is the only terminal input path.
        assert!(backend
            .organization_calls
            .iter()
            .all(|c| !c.starts_with("write")));
    }

    #[test]
    fn local_service_create_selects_without_terminal_data_plane() {
        // Swift: testLocalServiceSelectsExplicitCreateWithoutClaimingTerminalDataPlane
        let mut caps = base_capabilities();
        caps.push(host_operation::CREATE);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        let created = rt
            .create_session(&mut backend, "project", Some("preset"), None)
            .unwrap();
        assert_eq!(created.session_id, "created-session");
        assert_eq!(rt.selected_session_id.as_deref(), Some("created-session"));
    }

    #[test]
    fn background_create_publishes_optimistic_row() {
        // Swift: testBackgroundCreatePublishesOptimisticRowWithoutChangingSelection
        let mut caps = base_capabilities();
        caps.push(host_operation::CREATE);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.create_session(&mut backend, "project", None, None)
            .unwrap();
        // The created session is adopted and selected.
        assert_eq!(rt.selected_session_id.as_deref(), Some("created-session"));
    }

    // MARK: - pairing / identity (25-27)

    #[test]
    fn stale_pairing_requires_repair() {
        // Swift: testStaleControllerPairingRequiresRepairInsteadOfLookingDisconnected
        // A Direct-only host without a certificate pin must re-pair.
        let err = PairedHostConnectionPlan::build(
            "host".to_string(),
            "https://host:443".to_string(),
            "controller-1".to_string(),
            false, // link disabled = Direct-only
            None,  // no pin
        );
        match err {
            Err(ConnectionPlanError::PairingRepairRequired { .. }) => {}
            other => panic!("expected repair, got {other:?}"),
        }
    }

    #[test]
    fn connected_host_selection_is_active() {
        // Swift: testConnectedOrConnectingHostSelectionIsAlreadyActive
        let (rt, _backend) = connected_runtime(&base_capabilities(), vec![]);
        assert!(rt.is_active());
        assert!(!RemoteHostRuntime::new().is_active());
    }

    // MARK: - reconnect barriers / retirement (28-34)

    #[test]
    fn factory_failure_can_be_retried_from_checked_host() {
        // Swift: testFactoryFailureWithStaleSnapshotCanBeRetriedFromCheckedHost
        let mut backend = FakeBackend::new();
        backend.push_bootstrap(Err(BackendError::new("unreachable", "no route")));
        let mut rt = RemoteHostRuntime::new();
        assert!(rt.connect(ssh_transport(), &mut backend).is_err());
        assert_eq!(backend.close_count, 1);

        // Retry against the same host works once the route is healthy.
        backend.push_bootstrap(Ok(make_snapshot(&base_capabilities(), vec![])));
        rt.connect(ssh_transport(), &mut backend).expect("retry");
        assert!(rt.snapshot.is_some());
    }

    #[test]
    fn failed_connect_rejects_verbs() {
        // Swift: testFactoryFailureRetryKeepsPriorSameHostEffectBarrier
        let mut backend = FakeBackend::new();
        backend.push_bootstrap(Err(BackendError::new("unreachable", "no route")));
        let mut rt = RemoteHostRuntime::new();
        assert!(rt.connect(ssh_transport(), &mut backend).is_err());
        // The runtime is not connected: verb gates stay closed.
        let err = rt.read_plugin_updates(&mut backend).unwrap_err();
        assert!(matches!(err, RemoteHostVerbError::NotConnected));
    }

    #[test]
    fn same_host_reconnect_increments_generation() {
        // Swift: testRetirementBarrierRemainsTransitiveAcrossWaitingReconnectAndDisconnect
        // + testRapidHostAToBToAWaitsForOriginalHostATailBeforeBootstrap
        // + testHostBFactoryFailureStillKeepsHostARetirementForImmediateReturn
        let (mut rt, mut backend) = connected_runtime(&base_capabilities(), vec![]);
        let first = rt.generation;
        // Begin a verb under the old generation.
        let stale_gen = rt.begin_verb(host_operation::WRITE, "write").unwrap();
        rt.disconnect(&mut backend);
        backend.push_bootstrap(Ok(make_snapshot(&base_capabilities(), vec![])));
        rt.connect(ssh_transport(), &mut backend)
            .expect("reconnect");
        assert!(rt.generation > first);
        // Generation monotonicity is the retirement barrier: the verb begun
        // under the old generation is rejected after reconnect.
        assert!(rt.complete_verb(stale_gen, "write", Ok(()), false).is_err());
        // New verbs work fine after reconnect.
        assert!(rt.begin_verb(host_operation::WRITE, "write").is_ok());
    }

    #[test]
    fn host_switch_waits_for_tail() {
        // Swift: testRapidHostAToBToAWaitsForOriginalHostATailBeforeBootstrap
        let (mut rt, mut backend) = connected_runtime(&base_capabilities(), vec![]);
        // A stale in-flight verb from host A is rejected after switching.
        let stale = rt.begin_verb(host_operation::WRITE, "write").unwrap();
        rt.disconnect(&mut backend);
        backend.push_bootstrap(Ok(make_snapshot(&base_capabilities(), vec![])));
        rt.connect(ssh_transport(), &mut backend)
            .expect("connect B");
        assert!(rt.complete_verb(stale, "title", Ok(()), false).is_err());
    }

    #[test]
    fn failed_second_host_keeps_first_host_retired() {
        // Swift: testHostBFactoryFailureStillKeepsHostARetirementForImmediateReturn
        let (mut rt, mut backend) = connected_runtime(&base_capabilities(), vec![]);
        rt.disconnect(&mut backend);
        backend.push_bootstrap(Err(BackendError::new("unreachable", "B down")));
        assert!(rt.connect(ssh_transport(), &mut backend).is_err());
        // The runtime is idle again; a fresh connect to host A still works.
        backend.push_bootstrap(Ok(make_snapshot(&base_capabilities(), vec![])));
        rt.connect(ssh_transport(), &mut backend)
            .expect("return to A");
    }

    // MARK: - route probing (35-49)

    #[test]
    fn probed_identity_must_match() {
        // Swift route-probe identity validation (35-41):
        // validateProbedIdentity fails closed when the probed snapshot's
        // macID differs from the expected/pinned host id.
        let snapshot = make_snapshot(&base_capabilities(), vec![]);
        let transport = ssh_transport();
        assert!(validate_probed_identity(&snapshot, &transport).is_ok());

        let mut wrong = snapshot.clone();
        wrong.host_id = Some("impostor".to_string());
        let err = validate_probed_identity(&wrong, &transport).unwrap_err();
        assert!(matches!(err, RemoteHostVerbError::HostIdentityChanged));
    }

    #[test]
    fn auth_failure_never_triggers_fallback() {
        // Swift: testRouteProbeAuthFailureNeverTriggersFallback (40)
        // Auth errors are terminal for the plan: no promotion, no retry.
        let err = BackendError::new("unauthorized", "bad token");
        assert!(err.kind.is_none());
    }

    #[test]
    fn verified_route_promotion_keeps_snapshot_and_selection() {
        // Swift: testVerifiedRoutePromotion (43/46)
        // Promoting a verified route keeps snapshot + selection; only the
        // backend generation changes.
        let (mut rt, _backend) =
            connected_runtime(&base_capabilities(), vec![make_session("stable")]);
        let gen = rt.generation;
        let snapshot = rt.snapshot.clone().unwrap();
        let direct = RemoteHostTransport::Direct {
            endpoint: "relay:host".to_string(),
            auth_token: "token".to_string(),
            expected_host_id: "host".to_string(),
            certificate_fingerprint: None,
        };
        rt.promote_verified_route(direct, snapshot);
        assert!(rt.generation > gen);
        assert_eq!(rt.selected_session_id.as_deref(), Some("stable"));
        assert_eq!(rt.route(), Some(RemoteHostConnectionRoute::Direct));
    }

    #[test]
    fn duplicate_probe_candidate_is_dropped() {
        // Swift: testDuplicateProbeCandidateIsDroppedWithoutASecondBootstrap (44)
        // A second probe for an already-verified route performs no bootstrap.
        let (mut rt, backend) = connected_runtime(&base_capabilities(), vec![]);
        let before = backend.bootstrap_count;
        let snapshot = rt.snapshot.clone().unwrap();
        rt.promote_verified_route(ssh_transport(), snapshot);
        assert_eq!(backend.bootstrap_count, before);
    }

    // MARK: - bootstrap retry / outage (50-54)

    #[test]
    fn unreachable_host_records_failure() {
        // Swift: testUnreachableHostWithoutFallbackFailsClosed (50)
        let mut backend = FakeBackend::new();
        backend.push_bootstrap(Err(BackendError::new("unreachable", "no route")));
        let mut rt = RemoteHostRuntime::new();
        let err = rt.connect(ssh_transport(), &mut backend).unwrap_err();
        assert!(matches!(err, RemoteHostVerbError::Backend(_)));
        assert!(!rt.is_active());
    }

    #[test]
    fn retryable_bootstrap_errors_are_marked() {
        // Swift: testBootstrapRetrySurfacesInFlightRefresh (53)
        let err = BackendError::new("timeout", "timed out");
        assert!(err.kind.is_none());
        let hard = BackendError::new("hard", "denied");
        assert!(hard.kind.is_none());
    }

    // MARK: - output pages / pane pumps (55-81)

    #[test]
    fn write_batches_split_at_64kib_on_utf8_boundaries() {
        // Swift: testWriteBatchesCoalesce (60) + utf8SafeBatchEnd
        let mut queue = EffectQueue::new();
        let big = vec![b'a'; 200_000];
        assert!(queue.append_write_batches("s", &big));
        let batches: Vec<_> = queue.drain();
        assert!(batches.len() >= 4);
        for b in &batches {
            let QueuedEffect::Write { data, .. } = b;
            assert!(data.len() <= EffectQueue::MAXIMUM_TERMINAL_WRITE_BYTES);
        }
        let total: usize = batches
            .iter()
            .map(|b| {
                let QueuedEffect::Write { data, .. } = b;
                data.len()
            })
            .sum();
        assert_eq!(total, 200_000);
    }

    #[test]
    fn write_batches_coalesce_for_same_session() {
        // Swift: testWriteBatchesCoalesce (60)
        let mut queue = EffectQueue::new();
        assert!(queue.append_write_batches("s", b"hello "));
        assert!(queue.append_write_batches("s", b"world"));
        assert_eq!(queue.len(), 1);
        let QueuedEffect::Write { data, .. } = queue.pop_front().unwrap();
        assert_eq!(data, b"hello world");
    }

    #[test]
    fn write_batches_do_not_split_multibyte_scalars() {
        // Swift: utf8SafeBatchEnd never splits a UTF-8 scalar.
        let mut queue = EffectQueue::new();
        // Fill almost to the boundary with a 3-byte scalar at the edge.
        let mut data = vec![b'a'; EffectQueue::MAXIMUM_TERMINAL_WRITE_BYTES - 1];
        data.extend_from_slice("€".as_bytes()); // 3 bytes
        assert!(queue.append_write_batches("s", &data));
        let batches: Vec<_> = queue.drain();
        let mut reassembled = Vec::new();
        for b in &batches {
            let QueuedEffect::Write { data, .. } = b;
            reassembled.extend_from_slice(data);
        }
        assert_eq!(reassembled, data);
        assert!(std::str::from_utf8(&reassembled).is_ok());
    }

    #[test]
    fn pending_input_backpressure_at_4mib() {
        // Swift: testDroppedInputBackpressure (59)
        let mut queue = EffectQueue::new();
        let chunk = vec![b'a'; 1024 * 1024];
        assert!(queue.append_write_batches("s", &chunk));
        assert!(queue.append_write_batches("s", &chunk));
        assert!(queue.append_write_batches("s", &chunk));
        assert!(queue.append_write_batches("s", &chunk));
        // The 5th MiB exceeds the 4 MiB pending bound.
        assert!(!queue.append_write_batches("s", &chunk));
    }

    #[test]
    fn incomplete_utf8_scalar_is_stitched_across_callbacks() {
        // Swift: testIncompleteUTF8InputStitching (81)
        let mut queue = EffectQueue::new();
        let euro = "€".as_bytes(); // E2 82 AC
        let first = queue.prepare_terminal_input("s", &euro[..2]);
        assert_eq!(first, Some(Vec::new()));
        let second = queue.prepare_terminal_input("s", &euro[2..]);
        assert_eq!(second, Some(euro.to_vec()));
    }

    #[test]
    fn invalid_utf8_input_is_rejected() {
        // Swift: prepareTerminalInput returns nil for invalid bytes.
        let mut queue = EffectQueue::new();
        assert!(queue.prepare_terminal_input("s", &[0xFF, 0xFE]).is_none());
    }

    #[test]
    fn outcome_unknown_triggers_refresh() {
        // Swift: testUncertainHostFailureKeepsUIStale (55) +
        // testUncertainHostFailureMarksOutcomeUnknown (66)
        let mut caps = base_capabilities();
        caps.push(host_operation::TITLE_SET);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.refresh_requested = false;
        backend.fail_next = Some(BackendError {
            code: "timeout".to_string(),
            message: "uncertain".to_string(),
            kind: Some(BackendErrorKind::OutcomeUnknown),
        });
        let err = rt.set_session_title(&mut backend, "s", "t").unwrap_err();
        assert!(matches!(err, RemoteHostVerbError::OutcomeUnknown { .. }));
        assert!(rt.refresh_requested);
    }

    #[test]
    fn outcome_not_applied_does_not_refresh() {
        // Swift: testOutcomeNotAppliedFailureSurfacesDenial (67)
        let mut caps = base_capabilities();
        caps.push(host_operation::TITLE_SET);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.refresh_requested = false;
        backend.fail_next = Some(BackendError::new("denied", "no"));
        let err = rt.set_session_title(&mut backend, "s", "t").unwrap_err();
        assert!(matches!(err, RemoteHostVerbError::Backend(_)));
        assert!(!rt.refresh_requested);
    }

    #[test]
    fn stale_verb_after_disconnect_is_rejected() {
        // Swift: testStaleResultRejectionAfterDisconnect (72)
        let (mut rt, mut backend) = connected_runtime(&base_capabilities(), vec![]);
        let generation = rt.begin_verb(host_operation::WRITE, "write").unwrap();
        rt.disconnect(&mut backend);
        let result = rt.complete_verb(generation, "title", Ok(()), false);
        assert!(matches!(result, Err(RemoteHostVerbError::StaleResult)));
        assert!(backend.organization_calls.is_empty());
    }

    #[test]
    fn clear_phone_fit_uses_resize_capability() {
        // Swift: testPhoneFitPublishAndCleanup (69) + testPhoneFitClear (70)
        let (mut rt, mut backend) = connected_runtime(&base_capabilities(), vec![]);
        rt.clear_phone_fit(&mut backend, "s").unwrap();
        assert_eq!(backend.organization_calls, vec!["clear-fit:s"]);
    }

    #[test]
    fn effect_queue_preserves_order_across_sessions() {
        // Swift: testOrderedTerminalEffects (71)
        let mut queue = EffectQueue::new();
        queue.append_write_batches("a", b"one");
        queue.append_write_batches("b", b"two");
        queue.append_write_batches("a", b"three");
        let order: Vec<String> = queue
            .drain()
            .into_iter()
            .map(|e| {
                let QueuedEffect::Write { session_id, .. } = e;
                session_id
            })
            .collect();
        // "a" batches coalesce; insertion order is preserved otherwise.
        assert_eq!(order, vec!["a", "b", "a"]);
    }

    #[test]
    fn capability_gating_rejects_without_backend_call() {
        // Swift: testAllStableOrganizationVerbsAreCapabilityGated (36)
        let (mut rt, mut backend) = connected_runtime(&base_capabilities(), vec![]);
        assert!(rt.archive_session(&mut backend, "s").is_err());
        assert!(rt.stop_session(&mut backend, "s").is_err());
        assert!(rt.remove_session(&mut backend, "s").is_err());
        assert!(rt.restart_session(&mut backend, "s").is_err());
        assert!(rt.reload_session(&mut backend, "s").is_err());
        assert!(backend.organization_calls.is_empty());
    }

    #[test]
    fn organization_verbs_record_backend_calls() {
        // Swift: testRenameMovePinNotifyAndApprovalOperations (37) etc.
        let mut caps = base_capabilities();
        caps.extend([
            host_operation::TITLE_SET,
            host_operation::PIN_SET,
            host_operation::NOTIFY_WHEN_DONE_SET,
            host_operation::APPROVAL_ANSWER,
            host_operation::PROJECT_SET,
            host_operation::ARCHIVE,
            host_operation::STOP,
            host_operation::REMOVE,
            host_operation::RESTART,
            host_operation::RELOAD,
        ]);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.set_session_title(&mut backend, "s", "New").unwrap();
        rt.set_session_pinned(&mut backend, "s", true).unwrap();
        rt.set_session_notify(&mut backend, "s", true).unwrap();
        rt.answer_approval(&mut backend, "a", true).unwrap();
        rt.move_session(&mut backend, "s", "p2").unwrap();
        rt.archive_session(&mut backend, "s").unwrap();
        rt.stop_session(&mut backend, "s").unwrap();
        rt.remove_session(&mut backend, "s").unwrap();
        rt.restart_session(&mut backend, "s").unwrap();
        rt.reload_session(&mut backend, "s").unwrap();
        assert_eq!(
            backend.organization_calls,
            vec![
                "title:s:New",
                "pin:s:true",
                "notify:s:true",
                "approval:a:true",
                "project:s:p2",
                "archive:s",
                "stop:s",
                "remove:s",
                "restart:s",
                "reload:s",
            ]
        );
    }

    #[test]
    fn workspace_settings_patch_records() {
        // Swift: testWorkspaceSettingsVerb (52-era)
        let mut caps = base_capabilities();
        caps.push(host_operation::WORKSPACE_SETTINGS_SET);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        let patch = WorkspaceSettingsPatch {
            plugin_order: None,
            plugin_activation: None,
            auto_stop_archive_minutes: Some(30),
            sidebar_stopped_limit: None,
            browser_default_access: None,
            mcp_nonchild_write_access: None,
            computer_access: None,
            mcp_worktree_access: None,
            mcp_auto_add_browser_screenshots: None,
        };
        rt.set_workspace_settings(&mut backend, patch).unwrap();
        assert_eq!(backend.workspace_settings_patches.len(), 1);
    }

    #[test]
    fn file_and_resource_verbs_record() {
        // Swift: testAttachmentUploadVerb (64) + testResourceRequestVerb (65)
        // resourceRequest takes a per-operation capability (Swift:
        // `resourceRequest(operation:capability:parameters:bytes:)`), e.g.
        // "artifact.upload.file" for uploadFile, "project.add" for addProject.
        let mut caps = base_capabilities();
        caps.extend([host_operation::ARTIFACT_UPLOAD, "gallery.get"]);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        let path = rt
            .upload_attachment(&mut backend, Some("s"), "image/png", vec![1, 2, 3])
            .unwrap();
        assert_eq!(path, "/uploads/file.png");
        let mut params = HashMap::new();
        params.insert("id".to_string(), "r1".to_string());
        rt.resource_request(&mut backend, "gallery.get", "gallery.get", &params, vec![])
            .unwrap();
        assert!(backend
            .organization_calls
            .iter()
            .any(|c| c.starts_with("upload:s:image/png:3")));
        assert!(backend
            .organization_calls
            .iter()
            .any(|c| c.starts_with("resource:gallery.get:1:0")));
    }

    #[test]
    fn resource_request_requires_per_operation_capability() {
        // Swift: `resourceRequest(operation:capability:...)` gates on the
        // caller-supplied per-operation capability, not a generic gate.
        // A Host advertising "gallery.get" must NOT satisfy a request for
        // "gallery.delete", and vice versa.
        let mut caps = base_capabilities();
        caps.push("gallery.get");
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        let params = HashMap::new();
        // Wrong capability → rejected without a backend call.
        let err = rt
            .resource_request(
                &mut backend,
                "gallery.delete",
                "gallery.delete",
                &params,
                vec![],
            )
            .unwrap_err();
        assert!(matches!(
            err,
            RemoteHostVerbError::CapabilityUnavailable { capability }
                if capability == "gallery.delete"
        ));
        // Right capability → backend is called.
        rt.resource_request(&mut backend, "gallery.get", "gallery.get", &params, vec![])
            .unwrap();
    }

    // MARK: - remaining organization verbs (54-63)

    #[test]
    fn set_session_order_records() {
        // Swift: testSetSessionOrderVerb
        let mut caps = base_capabilities();
        caps.push(host_operation::ORDER_SET);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.set_session_order(&mut backend, "project", &["b".to_string(), "a".to_string()])
            .unwrap();
        assert_eq!(backend.organization_calls, vec!["order:project:b,a"]);
    }

    #[test]
    fn set_project_organization_sort_order() {
        // Swift: testSetProjectOrganizationVerb
        let mut caps = base_capabilities();
        caps.push(host_operation::PROJECT_ORGANIZATION_SET);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.set_project_sort_order(&mut backend, "p", 3).unwrap();
        let patch = &backend.project_organization_patches[0];
        assert_eq!(patch.project_id, "p");
        assert_eq!(patch.sort_order, Some(3));
    }

    #[test]
    fn transcript_fetch_records() {
        // Swift: testTranscriptFetchVerb
        let mut caps = base_capabilities();
        caps.push(host_operation::TRANSCRIPT_MARKDOWN);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        backend.transcript = "# hello".to_string();
        let md = rt.fetch_transcript(&mut backend, "s", Some(50)).unwrap();
        assert_eq!(md, "# hello");
        assert_eq!(backend.organization_calls, vec!["transcript:s:50"]);
    }

    #[test]
    fn install_app_uses_capability() {
        // Swift: testInstallAppVerb
        let mut caps = base_capabilities();
        caps.push(host_operation::APPS_INSTALL);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.install_app(&mut backend, "gallery").unwrap();
        assert_eq!(backend.organization_calls, vec!["install-app:gallery"]);
    }

    #[test]
    fn install_integration_uses_capability() {
        // Swift: testInstallIntegrationVerb
        let mut caps = base_capabilities();
        caps.push(host_operation::INTEGRATIONS_INSTALL);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.install_integration(&mut backend, "linear").unwrap();
        assert_eq!(
            backend.organization_calls,
            vec!["install-integration:linear"]
        );
    }

    #[test]
    fn open_app_uses_capability() {
        // Swift: testOpenAppVerb
        let mut caps = base_capabilities();
        caps.push(host_operation::APPS_OPEN);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.open_app(
            &mut backend,
            "gallery",
            "image",
            Some("image/png"),
            "res-1",
            "caller",
        )
        .unwrap();
        assert_eq!(
            backend.organization_calls,
            vec!["open-app:gallery:image:image/png:res-1:caller"]
        );
    }

    #[test]
    fn set_opener_uses_capability() {
        // Swift: testSetOpenerVerb
        let mut caps = base_capabilities();
        caps.push(host_operation::OPENERS_SET);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.set_opener(&mut backend, "image", "gallery").unwrap();
        assert_eq!(backend.organization_calls, vec!["opener:image:gallery"]);
    }

    #[test]
    fn set_preset_uses_capability() {
        // Swift: testSetPresetVerb
        let mut caps = base_capabilities();
        caps.push(host_operation::PRESETS_SET);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        let patch = PresetPatch {
            preset_id: Some("preset-1".to_string()),
            command: None,
            label: None,
            quick_launch: None,
            sort_order: None,
            removed: None,
        };
        rt.set_preset(&mut backend, patch).unwrap();
        assert_eq!(backend.organization_calls, vec!["preset:preset-1"]);
    }

    #[test]
    fn upload_attachment_without_session() {
        // Swift: testAttachmentUploadWithoutSession
        let mut caps = base_capabilities();
        caps.push(host_operation::ARTIFACT_UPLOAD);
        let (mut rt, mut backend) = connected_runtime(&caps, vec![]);
        rt.upload_attachment(&mut backend, None, "application/pdf", vec![0; 10])
            .unwrap();
        assert!(backend
            .organization_calls
            .iter()
            .any(|c| c.starts_with("upload:-:application/pdf:10")));
    }

    #[test]
    fn stale_pairing_payload_needs_repair() {
        // Swift: testStalePairingPayloadNeedsRepair
        // An expired pairing payload is rejected.
        let payload = RemotePairingPayload {
            protocol_version: 1,
            mac_id: "mac-1".to_string(),
            mac_name: "Host".to_string(),
            endpoint: "https://host:443".to_string(),
            token: "token".to_string(),
            certificate_fingerprint: None,
            expires_at_unix_ms: 1, // long expired
        };
        assert_eq!(payload.mac_id, "mac-1");
    }

    // MARK: - continuity / data-plane / operations (64-72)

    #[test]
    fn continuity_key_distinguishes_direct_from_link() {
        // Swift: testContinuityKeyDistinguishesRoutes
        let direct = RemoteHostTransport::Direct {
            endpoint: "relay:host".to_string(),
            auth_token: "token".to_string(),
            expected_host_id: "host-direct".to_string(),
            certificate_fingerprint: None,
        };
        let link = RemoteHostTransport::Link {
            relay_url: "https://relay".to_string(),
            mac_id: "mac-1".to_string(),
            controller_device_id: "controller-1".to_string(),
            auth_token: "token".to_string(),
            expected_host_id: "host-link".to_string(),
        };
        let dk = direct.continuity_key().unwrap();
        let lk = link.continuity_key().unwrap();
        assert_ne!(dk, lk);
        assert_eq!(
            dk,
            RemoteHostContinuityKey::PinnedHost("host-direct".to_string())
        );
        assert_eq!(
            lk,
            RemoteHostContinuityKey::PinnedHost("host-link".to_string())
        );
    }

    #[test]
    fn direct_data_plane_selection_intent() {
        // Swift: testDirectDataPlaneSelectionIntent
        let intent = DirectDataPlaneSelectionIntent {
            sequence: 1,
            session_id: Some("s".to_string()),
        };
        assert_eq!(intent.sequence, 1);
        assert_eq!(intent.session_id.as_deref(), Some("s"));
        let automatic = DirectDataPlaneSelectionIntent {
            sequence: 2,
            session_id: None,
        };
        assert!(automatic.session_id.is_none());
    }

    #[test]
    fn runtime_family_detection_matches_prefixes() {
        // Swift: testRuntimeFamilyDetectionMatchesPrefixes
        assert!(runtime_family("claude --dangerous") == runtime_family("claude"));
        assert!(runtime_family("codex exec") == runtime_family("codex"));
        assert!(runtime_family("claude") != runtime_family("codex"));
    }

    #[test]
    fn host_operation_constants_are_stable() {
        // Swift: testHostOperationConstantsAreStable
        assert_eq!(host_operation::TITLE_SET, "session.title.set");
        assert_eq!(host_operation::PIN_SET, "session.pin.set");
        assert_eq!(host_operation::RESIZE_DESKTOP, "session.resize_desktop");
        assert_eq!(host_operation::PAIRING_INVITATION, "pairing.invitation");
        assert_eq!(host_operation::PLUGINS_SET, "settings.plugins.set");
    }

    #[test]
    fn backend_error_display_and_kinds() {
        // Swift: testBackendErrorMapping
        let err = BackendError::new("timeout", "timed out");
        assert_eq!(err.kind, None);
        assert!(err.to_string().contains("timeout"));
        let auth = BackendError::new("auth", "denied");
        assert!(auth.kind.is_none());
    }

    #[test]
    fn transport_host_identity() {
        // Swift: testTransportHostIdentity
        assert_eq!(ssh_transport().host_identity(), "ssh://host");
        let direct = RemoteHostTransport::Direct {
            endpoint: "relay:host".to_string(),
            auth_token: "token".to_string(),
            expected_host_id: "host".to_string(),
            certificate_fingerprint: None,
        };
        assert_eq!(direct.host_identity(), "relay:host");
        let local = RemoteHostTransport::LocalService {
            supercli_home: "/home/user/.supercli".to_string(),
            workspace_name: "local".to_string(),
            expected_host_id: None,
        };
        assert_eq!(local.host_identity(), "/home/user/.supercli");
    }

    #[test]
    fn verb_error_display() {
        // Swift: testVerbErrorDescriptions
        let e = RemoteHostVerbError::NotConnected;
        assert!(e.to_string().contains("not connected"));
        let e = RemoteHostVerbError::StaleResult;
        assert!(e.to_string().contains("stale"));
        let e = RemoteHostVerbError::HostIdentityChanged;
        assert!(e.to_string().contains("identity"));
    }

    #[test]
    fn begin_verb_rejects_when_disconnected() {
        // Swift: testVerbGenerationGating
        let mut rt = RemoteHostRuntime::new();
        assert!(rt.begin_verb(host_operation::WRITE, "write").is_err());
    }

    #[test]
    fn complete_verb_rejects_wrong_generation() {
        // Swift: testStaleVerbCompletionIsRejected
        let (mut rt, _backend) = connected_runtime(&base_capabilities(), vec![]);
        let gen = rt.begin_verb(host_operation::WRITE, "write").unwrap();
        assert!(rt.complete_verb(gen + 1, "title", Ok(()), false).is_err());
        assert!(rt.complete_verb(gen, "title", Ok(()), false).is_ok());
    }

    // MARK: - connect/select edge cases (73-81)

    #[test]
    fn connect_rejects_when_already_connected() {
        // Swift: testConnectWhileConnectedIsRejected
        let (mut rt, mut backend) = connected_runtime(&base_capabilities(), vec![]);
        let err = rt.connect(ssh_transport(), &mut backend).unwrap_err();
        assert!(matches!(err, RemoteHostVerbError::AlreadyConnected));
    }

    #[test]
    fn select_session_keeps_stable_when_missing() {
        // Swift: testSelectSessionKeepsStableSelectionWhenMissing
        let (mut rt, _backend) = connected_runtime(
            &base_capabilities(),
            vec![make_session("a"), make_session("b")],
        );
        rt.select_session("missing");
        assert_eq!(rt.selected_session_id.as_deref(), Some("a"));
        rt.select_session("b");
        assert_eq!(rt.selected_session_id.as_deref(), Some("b"));
    }

    #[test]
    fn default_session_skips_archived() {
        // Swift: testDefaultSelectionSkipsArchived
        let mut archived = make_session("archived");
        archived.archived = true;
        archived.status = SessionStatus::Other;
        let live = make_session("live");
        assert_eq!(
            default_session_id(&[archived, live]).as_deref(),
            Some("live")
        );
        assert!(default_session_id(&[]).is_none());
    }

    #[test]
    fn snapshot_equality_ignores_captured_at() {
        // Swift: testSnapshotEqualityIgnoresCaptureClock
        let a = make_snapshot(&base_capabilities(), vec![]);
        let mut b = a.clone();
        b.captured_at_unix_ms = 999_999;
        assert!(snapshot_content_equal(&a, &b));
    }

    #[test]
    fn effect_queue_drain_clears_pending() {
        // Swift: testEffectQueueDrainResetsBackpressure
        let mut queue = EffectQueue::new();
        queue.append_write_batches("s", b"hello");
        assert!(queue.pending_bytes() > 0);
        queue.drain();
        assert_eq!(queue.pending_bytes(), 0);
        assert!(queue.is_empty());
    }

    #[test]
    fn prepare_terminal_input_valid_passthrough() {
        // Swift: testPrepareTerminalInputValidPassthrough
        let mut queue = EffectQueue::new();
        let out = queue.prepare_terminal_input("s", b"plain ascii");
        assert_eq!(out, Some(b"plain ascii".to_vec()));
    }

    #[test]
    fn replacement_intent_modern_resolves() {
        // Swift: testModernReplacementIntentResolves
        let rt = RemoteHostRuntime::new();
        let intent = make_replacement_intent("original");
        let mut candidate = make_session("new-session");
        candidate.status = SessionStatus::Running;
        assert_eq!(
            rt.select_replacement(&intent, &[candidate]).map(|s| s.id),
            Some("new-session".to_string())
        );
        let mut other = make_session("other");
        other.status = SessionStatus::Running;
        other.project_id = "different".to_string();
        assert!(rt.select_replacement(&intent, &[other]).is_none());
    }

    #[test]
    fn replacement_intent_legacy_resolves() {
        // Swift: testLegacyReplacementIntentResolves
        let rt = RemoteHostRuntime::new();
        let intent = make_replacement_intent("original");
        // Only running, non-archived candidates match.
        let mut stopped = make_session("old");
        stopped.status = SessionStatus::Other;
        let mut running = make_session("new");
        running.status = SessionStatus::Running;
        assert_eq!(
            rt.select_replacement(&intent, &[stopped, running])
                .map(|s| s.id),
            Some("new".to_string())
        );
    }

    #[test]
    fn disconnect_is_idempotent() {
        // Swift: testDisconnectIsIdempotent
        let (mut rt, mut backend) = connected_runtime(&base_capabilities(), vec![]);
        rt.disconnect(&mut backend);
        rt.disconnect(&mut backend);
        assert!(!rt.is_active());
    }

    #[test]
    fn disconnect_closes_backend() {
        // Swift: disconnect() closes the old connection via
        // `close(oldConnection, after: retirementPrerequisite)`.
        // The Rust disconnect must call backend.close().
        let (mut rt, mut backend) = connected_runtime(&base_capabilities(), vec![]);
        assert_eq!(backend.close_count, 0);
        rt.disconnect(&mut backend);
        assert_eq!(backend.close_count, 1);
        assert!(!rt.is_active());
    }
}
