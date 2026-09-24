//! Blocking client for the Host's `/mobile` API.
//!
//! Authentication is a long-lived device Bearer <redacted> in the `Authorization`
//! header (see `mobile.rs`). Two transports:
//!
//! - plaintext HTTP (`HostClient::new`) for `http://` Direct endpoints, via
//!   ureq;
//! - pinned HTTPS (`HostClient::with_pinned_tls`) for `https://` endpoints,
//!   via [`crate::tls`] — the leaf certificate's SHA-256 must match the
//!   fingerprint from the sealed pairing response. There is deliberately no
//!   unpinned HTTPS: a self-signed Host certificate would fail WebPKI and
//!   skipping verification would send the auth token to anyone.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection};
use thiserror::Error;

use crate::dto::{BootstrapSnapshot, SessionSummary, TranscriptSnapshot};
use crate::tls::pinned_client_config;

/// Percent-encode one query parameter value (RFC 3986). Session IDs are
/// `[A-Za-z0-9_-]` and artifact names can carry anything the Host's
/// `safe_upload_filename` allows (`+`, `=`, `&`, …), so every value that
/// lands in a query string goes through this — an unescaped `&` or `=`
/// would otherwise split or rewrite the parameter list, and the Host's
/// `urldecode` turns a raw `+` into a space. Unreserved characters
/// (`A-Za-z0-9-_.~`) pass through untouched.
fn query_escape(value: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
    const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    utf8_percent_encode(value, UNRESERVED).to_string()
}

/// A fresh canonical lowercase UUIDv4, the only `upload_id` shape the
/// Host's resumable-upload route accepts (`invalid_upload_id` otherwise).
fn new_upload_id() -> Result<String, HostClientError> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| HostClientError::Transport(format!("rng for upload id: {e}")))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant 10xxxxxx
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-\
         {:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}

#[derive(Debug, Error)]
pub enum HostClientError {
    #[error("transport error: {0}")]
    Transport(String),
    /// TLS handshake failure on a pinned-`https://` Direct endpoint —
    /// including certificate-pin mismatch. This is a security boundary,
    /// never a reachability signal: the fallback policy must surface it
    /// as a hard error and must NOT route around it via the relay.
    #[error("TLS error: {0}")]
    Tls(String),
    #[error("host returned status {0}: {1}")]
    Status(u16, String),
    #[error("failed to decode response: {0}")]
    Decode(String),
    #[error("invalid endpoint: {0}")]
    InvalidEndpoint(String),
}

impl HostClientError {
    /// True only for failures that mean "the Direct route is unreachable"
    /// and the relay fallback may be attempted: network-level transport
    /// errors. TLS/pin failures, HTTP statuses, decode errors, and invalid
    /// endpoints are never reachability — falling back on them would
    /// silently route around a security decision or mask a real bug.
    pub fn is_reachability_failure(&self) -> bool {
        matches!(self, HostClientError::Transport(_))
    }
}

/// Why a Direct connect attempt failed, classified for the relay fallback
/// policy. Only [`DirectFailure::Unreachable`] may trigger the relay
/// fallback (and only when the Host record has Link enabled). Everything
/// else surfaces as a hard error.
#[derive(Debug)]
pub enum DirectFailure {
    /// The Direct route is unreachable (DNS, TCP connect, timeouts).
    /// Relay fallback is allowed when the record has Link enabled.
    Unreachable(HostClientError),
    /// Any other Host-side failure: TLS/pin mismatch, HTTP status,
    /// decode error, invalid endpoint. The relay must NOT be attempted —
    /// falling back here would silently route around a security decision.
    Hard(HostClientError),
    /// Local failure before any network I/O (credential store, client
    /// construction). The relay must NOT be attempted.
    Setup(String),
}

impl std::fmt::Display for DirectFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DirectFailure::Unreachable(e) => write!(f, "direct unreachable: {e}"),
            DirectFailure::Hard(e) => write!(f, "direct failed: {e}"),
            DirectFailure::Setup(e) => write!(f, "direct setup failed: {e}"),
        }
    }
}

impl std::error::Error for DirectFailure {}

impl DirectFailure {
    /// Whether the relay fallback may be attempted for this failure.
    /// True only for classified reachability failures.
    pub fn relay_eligible(&self) -> bool {
        matches!(self, DirectFailure::Unreachable(_))
    }

    fn classify(err: HostClientError) -> Self {
        if err.is_reachability_failure() {
            DirectFailure::Unreachable(err)
        } else {
            DirectFailure::Hard(err)
        }
    }
}

/// Attempt the Direct route to a paired Host (client construction +
/// bootstrap), classifying any failure for the relay fallback policy.
///
/// This is the single place the Direct→relay fallback decision is
/// grounded: callers must check [`DirectFailure::relay_eligible`] (and the
/// record's Link-enabled flag) before touching the relay. A TLS/pin
/// failure comes back as [`DirectFailure::Hard`] — never relay-eligible.
pub fn connect_direct_classified(
    record: &crate::pairing::PairedHostRecord,
    secrets: &crate::credentials::HostSecrets,
) -> Result<(HostClient, crate::dto::BootstrapSnapshot), DirectFailure> {
    let client =
        crate::pairing::client_for_paired_host(record, secrets).map_err(DirectFailure::classify)?;
    let snapshot = client.bootstrap().map_err(DirectFailure::classify)?;
    Ok((client, snapshot))
}

/// Progress reply of one resumable-upload chunk.
#[derive(Debug, Clone)]
pub struct UploadProgress {
    /// Offset the next chunk must start at.
    pub next_offset: u64,
    /// True when the file is fully received and published.
    pub complete: bool,
    /// Present on the completing chunk: the Host-side absolute path of the
    /// published artifact — the value clients paste into the agent's
    /// composer (Swift `uploadImage` parity).
    pub path: Option<String>,
    /// Present on the completing chunk: the published file name.
    pub name: Option<String>,
}

/// Parameters for one resumable-upload chunk (kept as a struct so the
/// call stays under the argument-count lint).
#[derive(Debug, Clone)]
pub struct UploadChunkParams<'a> {
    pub session_id: &'a str,
    pub upload_id: &'a str,
    pub offset: u64,
    pub total_size: u64,
    pub sha256_hex: &'a str,
    pub content_type: &'a str,
    pub bytes: &'a [u8],
}

/// One row of the session browser gallery: see [`crate::types::ArtifactMeta`].
pub use crate::types::ArtifactMeta;

/// The `GET /mobile/output` JSON body, parsed strictly: every field the
/// protocol specifies must be present with the right type. Unknown fields
/// are ignored (future-tolerant), but a missing `nextOffset` or a
/// mistyped `truncated` is a decode error, never a silent default.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutputChunkResponse {
    offset: u64,
    next_offset: u64,
    data_base64: String,
    truncated: bool,
}

/// One chunk of a session's raw PTY output, as returned by
/// `GET /mobile/output`.
#[derive(Debug, Clone)]
pub struct OutputChunk {
    /// Byte offset the returned data starts at.
    pub offset: u64,
    /// Byte offset to pass as `offset` on the next call.
    pub next_offset: u64,
    /// True when the Host could not honor the requested offset (log
    /// rotated/truncated) — the consumer should reset its parser.
    pub truncated: bool,
    /// Raw PTY bytes (base64-decoded), safe to feed to a VT parser.
    pub data: Vec<u8>,
}

/// One HTTP request/response round trip. Implementations must never send
/// the request anywhere but the configured base URL.
pub(crate) trait DirectTransport: Send + Sync {
    fn roundtrip(
        &self,
        method: &str,
        url: &str,
        auth: &str,
        body: Option<(&str, &[u8])>,
    ) -> Result<(u16, String), HostClientError>;
}

/// Which path a [`HostClient`] talks to its Host over: see [`crate::types::TransportKind`].
pub use crate::types::TransportKind;

/// A client bound to one Host's `/mobile` endpoint.
#[derive(Clone)]
pub struct HostClient {
    transport: Arc<dyn DirectTransport>,
    base_url: String,
    token: String,
    kind: TransportKind,
}

impl std::fmt::Debug for HostClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostClient")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl HostClient {
    /// `base_url` is the Host's Direct `/mobile` endpoint, e.g.
    /// `http://192.168.1.10:8321/mobile`. Only `http://` URLs are accepted
    /// here — use [`HostClient::with_pinned_tls`] for `https://`. Anything
    /// else is a recoverable [`HostClientError::InvalidEndpoint`], not a
    /// panic, so API misuse can't crash the caller.
    pub fn new(
        base_url: impl Into<String>,
        token: impl Into<String>,
    ) -> Result<Self, HostClientError> {
        let base_url = base_url.into().trim_end_matches('/').to_owned();
        if !base_url.starts_with("http://") {
            return Err(HostClientError::InvalidEndpoint(format!(
                "HostClient::new is for plaintext http:// endpoints, got: {base_url}"
            )));
        }
        Ok(Self {
            transport: Arc::new(UreqTransport {
                // Bounded like the pinned transport below: a blackholed
                // Direct route must fail instead of hanging the
                // Direct→relay fallback probe.
                agent: ureq::Agent::new_with_config(
                    ureq::config::Config::builder()
                        .timeout_global(Some(Duration::from_secs(30)))
                        // ureq's default turns every non-2xx into
                        // `Err(StatusCode)` before we see the body. That
                        // would collapse HTTP statuses into
                        // `HostClientError::Transport`, which the fallback
                        // policy reads as *reachability* — a 401 from the
                        // Host must surface as `Status` (hard failure, no
                        // relay fallback), never as `Unreachable`. Same
                        // fix as `pair()`'s, at the transport level.
                        .http_status_as_error(false)
                        .build(),
                ),
            }),
            base_url,
            token: token.into(),
            kind: TransportKind::Direct,
        })
    }

    /// Pinned-HTTPS client for `https://` Direct endpoints. The `fingerprint`
    /// is the lowercase hex SHA-256 of the Host's leaf certificate, taken
    /// from the sealed pairing response (`remoteServerCertificateFingerprint`
    /// / `certificateFingerprint`). Fails when the URL is not `https://` or
    /// the fingerprint is malformed.
    pub fn with_pinned_tls(
        base_url: impl Into<String>,
        token: impl Into<String>,
        fingerprint: &str,
    ) -> Result<Self, HostClientError> {
        let base_url = base_url.into().trim_end_matches('/').to_owned();
        if !base_url.starts_with("https://") {
            return Err(HostClientError::InvalidEndpoint(
                "with_pinned_tls requires an https:// endpoint".to_string(),
            ));
        }
        let config = pinned_client_config(fingerprint).ok_or_else(|| {
            HostClientError::InvalidEndpoint("malformed certificate fingerprint".to_string())
        })?;
        Ok(Self {
            transport: Arc::new(PinnedHttpsTransport {
                config,
                timeout: Duration::from_secs(30),
            }),
            base_url,
            token: token.into(),
            kind: TransportKind::Direct,
        })
    }

    /// Relay (`Link`) client: every `/mobile` request is tunneled through
    /// an E2E-sealed relay connection instead of a Direct LAN route. The
    /// `conn` must already be connected (see
    /// [`RelayConnection::connect`](crate::relay_conn::RelayConnection::connect));
    /// the bearer `token` is the same pairing auth token, carried inside
    /// the sealed tunnel — never on the relay wire itself.
    ///
    /// The fallback policy (Direct first, relay only on Direct reachability
    /// failure, probe back to Direct) lives with the caller; this
    /// constructor just builds the relay-backed client.
    pub fn via_relay(conn: crate::relay_conn::RelayConnection, token: impl Into<String>) -> Self {
        Self {
            transport: Arc::new(crate::relay_transport::RelayTransport::new(
                conn,
                Duration::from_secs(30),
            )),
            // Not a routable URL: the relay routes by the connection's
            // mac id, and RelayTransport only needs the `/mobile/...`
            // path suffix of whatever URL the client methods build.
            base_url: "relay://link/mobile".to_string(),
            token: token.into(),
            kind: TransportKind::Relay,
        }
    }

    /// Which path this client talks to its Host over: Direct or Via Link.
    pub fn transport_kind(&self) -> TransportKind {
        self.kind
    }

    fn get_text(&self, path: &str) -> Result<String, HostClientError> {
        let url = format!("{}{}", self.base_url, path);
        let auth = format!("Bearer {}", self.token);
        let (status, text) = self.transport.roundtrip("GET", &url, &auth, None)?;
        if !(200..300).contains(&status) {
            return Err(HostClientError::Status(status, text));
        }
        Ok(text)
    }

    fn post_json(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, HostClientError> {
        let url = format!("{}{}", self.base_url, path);
        let auth = format!("Bearer {}", self.token);
        let body_bytes =
            serde_json::to_vec(&body).map_err(|e| HostClientError::Decode(e.to_string()))?;
        let (status, text) = self.transport.roundtrip(
            "POST",
            &url,
            &auth,
            Some(("application/json", &body_bytes)),
        )?;
        if !(200..300).contains(&status) {
            return Err(HostClientError::Status(status, text));
        }
        serde_json::from_str(&text).map_err(|e| HostClientError::Decode(e.to_string()))
    }

    /// `GET /mobile/bootstrap` — the full Controller snapshot.
    pub fn bootstrap(&self) -> Result<BootstrapSnapshot, HostClientError> {
        let text = self.get_text("/bootstrap")?;
        serde_json::from_str(&text).map_err(|e| HostClientError::Decode(e.to_string()))
    }

    /// `GET /mobile/events` — poll the typed session-event stream (Phase 6 R4).
    ///
    /// `after_seq` is the cursor from the previous poll (0 = everything
    /// buffered); `limit` is clamped by the Host to 1024. Returns the raw
    /// [`crate::events::EventsResponse`]; use [`crate::events::ClientEventCursor`]
    /// to track per-session cursors across polls.
    pub fn events(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u64,
    ) -> Result<crate::events::EventsResponse, HostClientError> {
        // Query params are part of the path for this simple GET helper;
        // session ids are validated by the Host (`safe_session_id`).
        let path = format!(
            "/events?session_id={}&after_seq={}&limit={}",
            session_id, after_seq, limit
        );
        let text = self.get_text(&path)?;
        serde_json::from_str(&text).map_err(|e| HostClientError::Decode(e.to_string()))
    }

    /// One chunk of a session's raw PTY output.
    ///
    /// `GET /mobile/output?session_id=…&offset=…&limit=…&wait_ms=…` returns
    /// JSON: `{ offset, nextOffset, dataBase64, truncated }`. The Host
    /// truncates chunks at a safe boundary (never splitting a UTF-8 scalar
    /// or an escape sequence), and `wait_ms` long-polls for new bytes —
    /// the primitive a live terminal view loops on.
    pub fn output_chunk(
        &self,
        session_id: &str,
        offset: Option<u64>,
        limit: usize,
        wait_ms: u64,
    ) -> Result<OutputChunk, HostClientError> {
        let mut path = format!("/output?session_id={session_id}&limit={limit}&wait_ms={wait_ms}");
        if let Some(offset) = offset {
            path.push_str(&format!("&offset={offset}"));
        }
        let text = self.get_text(&path)?;
        let raw: OutputChunkResponse = serde_json::from_str(&text)
            .map_err(|e| HostClientError::Decode(format!("output chunk: {e}")))?;
        let data =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &raw.data_base64)
                .map_err(|e| HostClientError::Decode(format!("output chunk base64: {e}")))?;
        Ok(OutputChunk {
            offset: raw.offset,
            next_offset: raw.next_offset,
            truncated: raw.truncated,
            data,
        })
    }

    /// Compatibility wrapper over [`HostClient::output_chunk`]: one chunk
    /// from the start of the log, no waiting. Kept so callers written
    /// against the pre-chunk API keep compiling; new code should use
    /// `output_chunk` with explicit offset tracking.
    pub fn output(&self, session_id: &str) -> Result<OutputChunk, HostClientError> {
        self.output_chunk(session_id, None, 65536, 0)
    }

    /// `GET /mobile/transcript-markdown?session_id=…` — transcript as markdown.
    pub fn transcript_markdown(&self, session_id: &str) -> Result<String, HostClientError> {
        self.get_text(&format!("/transcript-markdown?session_id={session_id}"))
    }

    /// `POST /mobile/write` — send input to a session's PTY.
    pub fn write(
        &self,
        session_id: &str,
        data: &str,
    ) -> Result<serde_json::Value, HostClientError> {
        self.post_json(
            "/write",
            serde_json::json!({"sessionID": session_id, "data": data}),
        )
    }

    /// `POST /mobile/turn-cancel` — the Host-owned cancel verb (capability
    /// `session.turn.cancel`, Phase 7 C2). Call this only when the Host
    /// advertises the capability (see
    /// [`crate::protocol::supports_turn_cancel`]); Hosts that don't
    /// advertise it keep the raw Ctrl-C fallback via [`HostClient::write`].
    ///
    /// The Host marks in-flight tool attempts Ambiguous (never failed,
    /// never auto-retried), interrupts the PTY best-effort, and emits
    /// `turn.cancelled`. Returns the Host's JSON body on success.
    pub fn cancel_turn(
        &self,
        session_id: &str,
        reason: &str,
    ) -> Result<serde_json::Value, HostClientError> {
        self.post_json(
            "/turn-cancel",
            serde_json::json!({"sessionID": session_id, "reason": reason}),
        )
    }

    /// `POST /mobile/sessions` — create a session from a preset/command.
    pub fn create_session(
        &self,
        command: &str,
        project_id: Option<&str>,
    ) -> Result<serde_json::Value, HostClientError> {
        let mut body = serde_json::json!({"command": command});
        if let Some(p) = project_id {
            body["projectID"] = serde_json::Value::String(p.to_owned());
        }
        self.post_json("/sessions", body)
    }

    /// `POST /mobile/sessions` — create a session from a preset id, the
    /// shape Swift's preset drawer sends (`RemoteCreateSessionRequest` with
    /// `projectID` + `presetID`). The Host 400s without a project id, so
    /// callers must resolve one first.
    pub fn create_session_with_preset(
        &self,
        project_id: &str,
        preset_id: &str,
    ) -> Result<serde_json::Value, HostClientError> {
        self.post_json(
            "/sessions",
            serde_json::json!({"projectID": project_id, "presetID": preset_id}),
        )
    }

    /// Extract the new session's id from a `POST /mobile/sessions`
    /// response (`{"sessionID": …}`).
    pub fn created_session_id(response: &serde_json::Value) -> Option<String> {
        response
            .get("sessionID")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    }

    /// `POST /mobile/session-action` — restart / remove / restart-agent /
    /// resume-agent.
    pub fn session_action(
        &self,
        action: &str,
        session_id: &str,
    ) -> Result<serde_json::Value, HostClientError> {
        self.post_json(
            "/session-action",
            serde_json::json!({"action": action, "sessionID": session_id}),
        )
    }

    /// `POST /mobile/session-organization` — patch a session's organization
    /// fields (title, pinned, archived). Archive/restore ride this patch,
    /// not the session-action endpoint — mirroring the native clients.
    /// `None` fields are left untouched on the Host.
    pub fn update_session_organization(
        &self,
        session_id: &str,
        title: Option<&str>,
        pinned: Option<bool>,
        archived: Option<bool>,
    ) -> Result<serde_json::Value, HostClientError> {
        self.update_session_organization_full(session_id, title, pinned, archived, None)
    }

    /// Full `POST /mobile/session-organization` patch, including the
    /// notify-when-done toggle the iOS organize sheet offers. `None`
    /// fields are left untouched on the Host.
    pub fn update_session_organization_full(
        &self,
        session_id: &str,
        title: Option<&str>,
        pinned: Option<bool>,
        archived: Option<bool>,
        notify_when_done: Option<bool>,
    ) -> Result<serde_json::Value, HostClientError> {
        let mut body = serde_json::json!({"sessionID": session_id});
        if let Some(t) = title {
            body["title"] = serde_json::Value::String(t.to_owned());
        }
        if let Some(p) = pinned {
            body["pinned"] = serde_json::Value::Bool(p);
        }
        if let Some(a) = archived {
            body["archived"] = serde_json::Value::Bool(a);
        }
        if let Some(n) = notify_when_done {
            body["notifyWhenDone"] = serde_json::Value::Bool(n);
        }
        self.post_json("/session-organization", body)
    }

    /// Move a session to a different project/group. This is the Controller
    /// verb for the "Move to ▸" project destinations: it posts only
    /// `projectID` to `/session-organization`. The Host accepts `projectID`
    /// additively (no wire break); gate the UI on
    /// [`crate::protocol::supports_session_project_move`] so older Hosts
    /// never get the verb.
    pub fn move_session_to_project(
        &self,
        session_id: &str,
        project_id: &str,
    ) -> Result<serde_json::Value, HostClientError> {
        let body = serde_json::json!({"sessionID": session_id, "projectID": project_id});
        self.post_json("/session-organization", body)
    }

    /// `POST /mobile/project-organization` — patch a project/group's
    /// organization fields, mirroring the iOS project organize sheet:
    /// rename groups (`display_name`, groups only), set a main project's
    /// folder color (`color_id`, one of sky/blue/violet/rose/amber/moss/
    /// teal/graphite or empty to clear), flip a group's session sort
    /// (`date_sorted`), pin groups (`pinned`), and move the project to
    /// `sort_order` among its same-parent siblings. `None` fields are left
    /// untouched on the Host.
    pub fn update_project_organization(
        &self,
        project_id: &str,
        display_name: Option<&str>,
        color_id: Option<&str>,
        date_sorted: Option<bool>,
        pinned: Option<bool>,
        sort_order: Option<usize>,
    ) -> Result<serde_json::Value, HostClientError> {
        let mut body = serde_json::json!({"projectID": project_id});
        if let Some(n) = display_name {
            body["displayName"] = serde_json::Value::String(n.to_owned());
        }
        if let Some(c) = color_id {
            body["colorID"] = serde_json::Value::String(c.to_owned());
        }
        if let Some(d) = date_sorted {
            body["dateSorted"] = serde_json::Value::Bool(d);
        }
        if let Some(p) = pinned {
            body["pinned"] = serde_json::Value::Bool(p);
        }
        if let Some(s) = sort_order {
            body["sortOrder"] = serde_json::Value::from(s);
        }
        self.post_json("/project-organization", body)
    }

    /// `GET /mobile/archive?project_id=…` — the project's archive library
    /// (Mac-resolved summaries, newest first), as shown by the iOS archived
    /// sessions sheet.
    pub fn archived_sessions(
        &self,
        project_id: &str,
    ) -> Result<Vec<SessionSummary>, HostClientError> {
        let text = self.get_text(&format!("/archive?project_id={}", query_escape(project_id)))?;
        let body: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| HostClientError::Decode(e.to_string()))?;
        let sessions = body
            .get("sessions")
            .cloned()
            .unwrap_or(serde_json::Value::Array(Vec::new()));
        serde_json::from_value(sessions).map_err(|e| HostClientError::Decode(e.to_string()))
    }

    /// `POST /mobile/approvals/answer` — answer a pending approval.
    pub fn answer_approval(
        &self,
        approval_id: &str,
        approved: bool,
    ) -> Result<serde_json::Value, HostClientError> {
        self.post_json(
            "/approvals/answer",
            serde_json::json!({"id": approval_id, "approved": approved}),
        )
    }

    /// `POST /mobile/mark-read` — clear a session's unread flag.
    pub fn mark_read(&self, session_id: &str) -> Result<serde_json::Value, HostClientError> {
        self.post_json("/mark-read", serde_json::json!({"sessionID": session_id}))
    }

    /// `POST /mobile/push-token` — register this device's push token so the
    /// Host can push "needs input" / "finished" notifications while the app
    /// is closed. `environment` is "sandbox" or "production" (APNs).
    pub fn register_push_token(
        &self,
        token: &str,
        environment: &str,
    ) -> Result<serde_json::Value, HostClientError> {
        self.post_json(
            "/push-token",
            serde_json::json!({"apnsToken": token, "environment": environment}),
        )
    }

    /// One row of the session browser gallery.
    ///
    /// `GET /mobile/artifacts?session_id=…` returns
    /// `{ sessionID, artifacts: [{kind, name, size, modified_at_unix_ms}], capturedAtUnixMs }`.
    pub fn browser_artifacts(
        &self,
        session_id: &str,
    ) -> Result<Vec<ArtifactMeta>, HostClientError> {
        #[derive(serde::Deserialize)]
        struct ListResponse {
            artifacts: Vec<ArtifactMeta>,
        }
        let text = self.get_text(&format!(
            "/artifacts?session_id={}",
            query_escape(session_id)
        ))?;
        let raw: ListResponse = serde_json::from_str(&text)
            .map_err(|e| HostClientError::Decode(format!("artifacts: {e}")))?;
        Ok(raw.artifacts)
    }

    /// Full artifact bytes, fetched through the bounded chunk route.
    ///
    /// `GET /mobile/artifact?session_id=…&kind=…&name=…&offset=…&limit=…`
    /// returns `{ contentType, offset, nextOffset, totalSize, dataBase64 }`;
    /// this follows `nextOffset` until the whole file is assembled.
    ///
    /// The loop is defended against a misbehaving Host: a `nextOffset`
    /// that does not advance is a stall (without this the loop would spin
    /// forever on non-empty chunks), and the assembled length must equal
    /// the advertised `totalSize`.
    pub fn artifact_bytes(
        &self,
        session_id: &str,
        kind: &str,
        name: &str,
    ) -> Result<(String, Vec<u8>), HostClientError> {
        self.artifact_bytes_query(session_id, kind, name, None)
    }

    /// Grid-sized thumbnail bytes for an image artifact. `max_dim` is the
    /// longest side in pixels (Swift parity: the iOS client asks for 512).
    /// The Host honors `max_dim` through its platform thumbnail adapter and
    /// falls back to the original bytes when it can't — either way the
    /// result is decodable image bytes, so callers need no fallback logic.
    pub fn artifact_thumbnail_bytes(
        &self,
        session_id: &str,
        kind: &str,
        name: &str,
        max_dim: u32,
    ) -> Result<(String, Vec<u8>), HostClientError> {
        self.artifact_bytes_query(session_id, kind, name, Some(max_dim.max(1)))
    }

    fn artifact_bytes_query(
        &self,
        session_id: &str,
        kind: &str,
        name: &str,
        max_dim: Option<u32>,
    ) -> Result<(String, Vec<u8>), HostClientError> {
        let extra = max_dim.map(|d| format!("&max_dim={d}")).unwrap_or_default();
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ChunkResponse {
            content_type: String,
            next_offset: u64,
            total_size: u64,
            data_base64: String,
        }
        let mut bytes = Vec::new();
        let mut offset: u64 = 0;
        let mut total_size: Option<u64> = None;
        let mut content_type: Option<String> = None;
        loop {
            let text = self.get_text(&format!(
                "/artifact?session_id={}&kind={}&name={}&offset={offset}&limit=262144{extra}",
                query_escape(session_id),
                query_escape(kind),
                query_escape(name),
            ))?;
            let raw: ChunkResponse = serde_json::from_str(&text)
                .map_err(|e| HostClientError::Decode(format!("artifact chunk: {e}")))?;
            let chunk = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                &raw.data_base64,
            )
            .map_err(|e| HostClientError::Decode(format!("artifact base64: {e}")))?;
            if content_type.is_none() {
                content_type = Some(raw.content_type.clone());
            }
            match total_size {
                None => total_size = Some(raw.total_size),
                Some(first) if first != raw.total_size => {
                    return Err(HostClientError::Decode(
                        "artifact changed size mid-download".into(),
                    ));
                }
                Some(_) => {}
            }
            bytes.extend_from_slice(&chunk);
            if raw.next_offset <= offset && offset < raw.total_size {
                return Err(HostClientError::Decode(
                    "artifact read stalled: next offset did not advance".into(),
                ));
            }
            offset = raw.next_offset;
            if offset >= raw.total_size {
                break;
            }
        }
        let total_size = total_size.unwrap_or(0);
        if bytes.len() as u64 != total_size {
            return Err(HostClientError::Decode(format!(
                "artifact size mismatch: got {} bytes, host advertised {total_size}",
                bytes.len()
            )));
        }
        Ok((content_type.unwrap_or_default(), bytes))
    }

    /// `POST /mobile/artifact-delete?session_id=…&kind=…&name=…` — delete one
    /// artifact (query-param contract, not a body).
    pub fn delete_artifact(
        &self,
        session_id: &str,
        kind: &str,
        name: &str,
    ) -> Result<serde_json::Value, HostClientError> {
        self.post_json(
            &format!(
                "/artifact-delete?session_id={}&kind={}&name={}",
                query_escape(session_id),
                query_escape(kind),
                query_escape(name),
            ),
            serde_json::json!({}),
        )
    }

    /// `POST /mobile/request-screenshot` with `{sessionID}` — asks the Host
    /// to capture a screenshot into the session's gallery (the new artifact
    /// then appears in [`HostClient::browser_artifacts`]).
    pub fn request_screenshot(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value, HostClientError> {
        self.post_json(
            "/request-screenshot",
            serde_json::json!({"sessionID": session_id}),
        )
    }

    /// POST raw bytes (non-JSON body) and parse the JSON reply.
    fn post_bytes(
        &self,
        path: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<serde_json::Value, HostClientError> {
        let url = format!("{}{}", self.base_url, path);
        let auth = format!("Bearer {}", self.token);
        let (status, text) =
            self.transport
                .roundtrip("POST", &url, &auth, Some((content_type, bytes)))?;
        if !(200..300).contains(&status) {
            return Err(HostClientError::Status(status, text));
        }
        serde_json::from_str(&text).map_err(|e| HostClientError::Decode(e.to_string()))
    }

    /// One chunk of a resumable gallery upload.
    ///
    /// `POST /mobile/upload-chunk?session_id=…&upload_id=…&offset=…&total_size=…&sha256=…`
    /// with the raw chunk bytes as the body. The caller drives the loop:
    /// `next_offset` from the reply is the next chunk's offset; `complete`
    /// ends it. `sha256_hex` is the hex digest of the *whole* file.
    ///
    /// The Host only accepts a canonical lowercase UUIDv4 `upload_id`
    /// (`invalid_upload_id` otherwise); [`HostClient::upload_artifact`]
    /// mints one for you.
    pub fn upload_artifact_chunk(
        &self,
        params: &UploadChunkParams<'_>,
    ) -> Result<UploadProgress, HostClientError> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct UploadResponse {
            next_offset: u64,
            complete: bool,
            path: Option<String>,
            name: Option<String>,
        }
        let path = format!(
            "/upload-chunk?session_id={}&upload_id={}&offset={}&total_size={}&sha256={}",
            query_escape(params.session_id),
            query_escape(params.upload_id),
            params.offset,
            params.total_size,
            query_escape(params.sha256_hex),
        );
        let raw: UploadResponse =
            serde_json::from_value(self.post_bytes(&path, params.content_type, params.bytes)?)
                .map_err(|e| HostClientError::Decode(format!("upload chunk: {e}")))?;
        Ok(UploadProgress {
            next_offset: raw.next_offset,
            complete: raw.complete,
            path: raw.path,
            name: raw.name,
        })
    }

    /// Upload a whole file to the session gallery through the resumable
    /// chunk route (256 KiB chunks). The `upload_id` is minted here as a
    /// UUIDv4 — the only shape the Host accepts — and the sha256 the Host
    /// verifies covers the whole file.
    ///
    /// Returns the Host-side absolute path of the published artifact (the
    /// completion response's `path`, e.g.
    /// `<sessions>/…/artifacts/uploads/<name>`): the value to quote into the
    /// agent's composer, mirroring Swift's `uploadImage`.
    ///
    /// Empty uploads are refused locally: the Host would reject a
    /// zero-byte file at signature validation, so claiming success
    /// without contacting it would be a lie.
    pub fn upload_artifact(
        &self,
        session_id: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<String, HostClientError> {
        if bytes.is_empty() {
            return Err(HostClientError::Decode(
                "upload_artifact: refusing empty upload".into(),
            ));
        }
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(bytes);
        let sha256_hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let upload_id = new_upload_id()?;
        let total_size = bytes.len() as u64;
        let mut offset = 0u64;
        let mut completed_path: Option<String> = None;
        while offset < total_size {
            let end = (offset + 262_144).min(total_size) as usize;
            let progress = self.upload_artifact_chunk(&UploadChunkParams {
                session_id,
                upload_id: &upload_id,
                offset,
                total_size,
                sha256_hex: &sha256_hex,
                content_type,
                bytes: &bytes[offset as usize..end],
            })?;
            if progress.complete {
                completed_path = progress.path;
                break;
            }
            if progress.next_offset <= offset {
                return Err(HostClientError::Decode(
                    "upload stalled: next offset did not advance".into(),
                ));
            }
            offset = progress.next_offset;
        }
        match completed_path {
            Some(path) if !path.is_empty() => Ok(path),
            _ => Err(HostClientError::Decode(
                "upload ended without the host's completion path".into(),
            )),
        }
    }

    /// `POST /mobile/resize` — resize a session's PTY.
    pub fn resize(
        &self,
        session_id: &str,
        columns: i64,
        rows: i64,
    ) -> Result<serde_json::Value, HostClientError> {
        self.post_json(
            "/resize",
            serde_json::json!({"sessionID": session_id, "columns": columns, "rows": rows}),
        )
    }

    pub fn _transcript_snapshot_hint(&self) -> Option<TranscriptSnapshot> {
        // Reserved: structured transcript reads arrive with the `chat.*`
        // verbs (Track C). Raw markdown + output cover v1.
        None
    }
}

// ---------------------------------------------------------------------------
// Transports
// ---------------------------------------------------------------------------

/// Plaintext HTTP via ureq.
struct UreqTransport {
    agent: ureq::Agent,
}

impl DirectTransport for UreqTransport {
    fn roundtrip(
        &self,
        method: &str,
        url: &str,
        auth: &str,
        body: Option<(&str, &[u8])>,
    ) -> Result<(u16, String), HostClientError> {
        let mut response = if method == "GET" {
            self.agent
                .get(url)
                .header("Authorization", auth)
                .call()
                .map_err(|e| HostClientError::Transport(e.to_string()))?
        } else if method == "POST" {
            let (content_type, bytes) =
                body.ok_or_else(|| HostClientError::Transport("POST requires a body".to_string()))?;
            self.agent
                .post(url)
                .header("Authorization", auth)
                .header("Content-Type", content_type)
                .send(bytes)
                .map_err(|e| HostClientError::Transport(e.to_string()))?
        } else {
            return Err(HostClientError::Transport(format!(
                "unsupported method {method}"
            )));
        };
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| HostClientError::Decode(e.to_string()))?;
        Ok((status, text))
    }
}

/// Pinned HTTPS: raw TCP + rustls with [`crate::tls::PinningVerifier`],
/// speaking minimal HTTP/1.1. ureq cannot take a custom rustls verifier,
/// so this transport owns the whole stack. Responses are limited to 8 MiB.
struct PinnedHttpsTransport {
    config: Arc<ClientConfig>,
    timeout: Duration,
}

const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

impl DirectTransport for PinnedHttpsTransport {
    fn roundtrip(
        &self,
        method: &str,
        url: &str,
        auth: &str,
        body: Option<(&str, &[u8])>,
    ) -> Result<(u16, String), HostClientError> {
        let (host, port, path) = parse_https_url(url)?;
        let server_name = ServerName::try_from(host.clone())
            .map(|name| name.to_owned())
            .map_err(|_| HostClientError::InvalidEndpoint("bad https host".to_string()))?;
        let addr = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|e| HostClientError::Transport(e.to_string()))?
            .next()
            .ok_or_else(|| HostClientError::Transport("DNS resolution failed".to_string()))?;
        let mut tcp = TcpStream::connect_timeout(&addr, self.timeout)
            .map_err(|e| HostClientError::Transport(e.to_string()))?;
        tcp.set_read_timeout(Some(self.timeout))
            .map_err(|e| HostClientError::Transport(e.to_string()))?;
        tcp.set_write_timeout(Some(self.timeout))
            .map_err(|e| HostClientError::Transport(e.to_string()))?;
        let conn = ClientConnection::new(Arc::clone(&self.config), server_name)
            .map_err(|e| HostClientError::Tls(format!("TLS setup: {e}")))?;
        let mut tls_conn = conn;
        // Explicit handshake BEFORE any HTTP bytes: pin mismatch and other
        // TLS failures surface here as HostClientError::Tls — distinctly
        // from the TCP reachability failures above. The fallback policy
        // treats Tls as a hard error, never a relay trigger.
        tls_conn
            .complete_io(&mut tcp)
            .map_err(|e| HostClientError::Tls(format!("TLS handshake: {e}")))?;
        let mut tls = rustls::StreamOwned::new(tls_conn, tcp);

        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: {auth}\r\nConnection: close\r\n"
        );
        if let Some((content_type, bytes)) = body {
            use std::fmt::Write;
            write!(
                request,
                "Content-Type: {content_type}\r\nContent-Length: {}\r\n",
                bytes.len()
            )
            .expect("writing to String cannot fail");
        }
        request.push_str("\r\n");
        tls.write_all(request.as_bytes())
            .map_err(|e| HostClientError::Transport(e.to_string()))?;
        if let Some((_, bytes)) = body {
            tls.write_all(bytes)
                .map_err(|e| HostClientError::Transport(e.to_string()))?;
        }
        tls.flush()
            .map_err(|e| HostClientError::Transport(e.to_string()))?;

        let (status, headers, body_bytes) = read_http_response(&mut tls)?;
        // The pinning decision already happened in the explicit handshake
        // above; a failure there is HostClientError::Tls, never Transport,
        // so the relay fallback policy can't mistake it for reachability.
        let _ = headers;
        let text =
            String::from_utf8(body_bytes).map_err(|e| HostClientError::Decode(e.to_string()))?;
        Ok((status, text))
    }
}

fn parse_https_url(url: &str) -> Result<(String, u16, String), HostClientError> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| HostClientError::InvalidEndpoint("not an https:// URL".to_string()))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    // Split host/port on the last ':'; bracketed IPv6 is not supported for
    // Direct endpoints (the QR form cannot encode it either).
    let (host, port) = match authority.rfind(':') {
        Some(i) => {
            let port: u16 = authority[i + 1..]
                .parse()
                .map_err(|_| HostClientError::InvalidEndpoint("bad https port".to_string()))?;
            (authority[..i].to_string(), port)
        }
        None => (authority.to_string(), 443),
    };
    if host.is_empty() || host.contains('[') {
        return Err(HostClientError::InvalidEndpoint(
            "bad https host".to_string(),
        ));
    }
    Ok((host, port, path.to_string()))
}

/// Minimal HTTP/1.1 response reader: status line, headers, then body via
/// Content-Length, chunked transfer-encoding, or connection close.
type HttpResponse = (u16, Vec<(String, String)>, Vec<u8>);

fn read_http_response(stream: &mut dyn Read) -> Result<HttpResponse, HostClientError> {
    let mut head = Vec::new();
    let mut buf = [0u8; 1];
    // Read until the blank line ending the header block.
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 64 * 1024 {
            return Err(HostClientError::Decode(
                "response head too large".to_string(),
            ));
        }
        let n = stream
            .read(&mut buf)
            .map_err(|e| HostClientError::Transport(e.to_string()))?;
        if n == 0 {
            return Err(HostClientError::Decode(
                "truncated response head".to_string(),
            ));
        }
        head.push(buf[0]);
    }
    let head_text = String::from_utf8(head).map_err(|e| HostClientError::Decode(e.to_string()))?;
    let mut lines = head_text.lines();
    let status_line = lines
        .next()
        .ok_or_else(|| HostClientError::Decode("empty response".to_string()))?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| HostClientError::Decode("bad status line".to_string()))?;
    let mut headers = Vec::new();
    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| HostClientError::Decode("bad header".to_string()))?;
        let name_lower = name.trim().to_lowercase();
        let value = value.trim().to_string();
        if name_lower == "content-length" {
            content_length = Some(
                value
                    .parse()
                    .map_err(|_| HostClientError::Decode("bad content-length".to_string()))?,
            );
        }
        if name_lower == "transfer-encoding" && value.to_lowercase().contains("chunked") {
            chunked = true;
        }
        headers.push((name_lower, value));
    }

    let mut body = Vec::new();
    if chunked {
        body = read_chunked_body(stream)?;
    } else if let Some(len) = content_length {
        if len > MAX_RESPONSE_BYTES {
            return Err(HostClientError::Decode("response too large".to_string()));
        }
        body.resize(len, 0);
        stream
            .read_exact(&mut body)
            .map_err(|e| HostClientError::Transport(e.to_string()))?;
    } else {
        // Connection-close delimited.
        let mut chunk = [0u8; 8192];
        loop {
            if body.len() > MAX_RESPONSE_BYTES {
                return Err(HostClientError::Decode("response too large".to_string()));
            }
            let n = stream
                .read(&mut chunk)
                .map_err(|e| HostClientError::Transport(e.to_string()))?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);
        }
    }
    Ok((status, headers, body))
}

fn read_chunked_body(stream: &mut dyn Read) -> Result<Vec<u8>, HostClientError> {
    let mut body = Vec::new();
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    let mut read_line =
        |line: &mut Vec<u8>, stream: &mut dyn Read| -> Result<(), HostClientError> {
            line.clear();
            loop {
                let n = stream
                    .read(&mut byte)
                    .map_err(|e| HostClientError::Transport(e.to_string()))?;
                if n == 0 {
                    return Err(HostClientError::Decode("truncated chunk".to_string()));
                }
                line.push(byte[0]);
                if line.ends_with(b"\r\n") {
                    line.truncate(line.len() - 2);
                    return Ok(());
                }
                if line.len() > 1024 {
                    return Err(HostClientError::Decode("chunk line too long".to_string()));
                }
            }
        };
    loop {
        read_line(&mut line, stream)?;
        let line_text =
            String::from_utf8(line.clone()).map_err(|e| HostClientError::Decode(e.to_string()))?;
        let size = usize::from_str_radix(line_text.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| HostClientError::Decode("bad chunk size".to_string()))?;
        if size == 0 {
            // Consume trailers.
            loop {
                read_line(&mut line, stream)?;
                if line.is_empty() {
                    break;
                }
            }
            break;
        }
        if body.len() + size > MAX_RESPONSE_BYTES {
            return Err(HostClientError::Decode("response too large".to_string()));
        }
        let start = body.len();
        body.resize(start + size, 0);
        stream
            .read_exact(&mut body[start..])
            .map_err(|e| HostClientError::Transport(e.to_string()))?;
        let mut crlf = [0u8; 2];
        stream
            .read_exact(&mut crlf)
            .map_err(|e| HostClientError::Transport(e.to_string()))?;
        if crlf != *b"\r\n" {
            return Err(HostClientError::Decode("bad chunk framing".to_string()));
        }
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_escape_keeps_unreserved_and_encodes_reserved() {
        assert_eq!(query_escape("s-1"), "s-1");
        assert_eq!(query_escape("a.png"), "a.png");
        assert_eq!(query_escape("a&b=c?.png"), "a%26b%3Dc%3F.png");
        // The Host's urldecode turns a raw `+` into a space: it must be
        // encoded, and a real space too.
        assert_eq!(query_escape("x y+z"), "x%20y%2Bz");
        assert_eq!(query_escape("100%"), "100%25");
    }

    #[test]
    fn new_upload_id_is_canonical_uuid_v4() {
        let id = new_upload_id().expect("rng works");
        assert_eq!(id.len(), 36);
        assert_eq!(&id[8..9], "-");
        assert_eq!(&id[14..15], "4", "version nibble: {id}");
        assert!(
            matches!(&id[19..20], "8" | "9" | "a" | "b"),
            "variant: {id}"
        );
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'));
        assert_ne!(new_upload_id().expect("rng"), id, "unique per call");
    }
}
