//! One handle for both connector transports: MCP over stdio
//! ([`ConnectorProcess`]) and MCP over Streamable HTTP
//! ([`ConnectorHttp`]). Callers (the CLI's `run`/`doctor`, the Host's
//! session connector layer) open a link without caring which transport
//! the manifest declares.

use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

use crate::http::{ConnectorHttp, HttpConnectorError};
use crate::manifest::{ConnectorKind, ConnectorManifest};
use crate::process::{ConnectorError, ConnectorProcess, McpTool};

/// Whether a connector call provably ran, provably did not, or sits in
/// between. "Did bytes reach the wire" (`request_sent_at`) is what
/// separates the classes; when the classifier is unsure it returns
/// [`CallOutcome::Ambiguous`] — fail safe, not convenient. Ambiguous
/// attempts are persisted, never auto-retried, and can only be replaced
/// by an explicit human-approved replacement call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallOutcome {
    DefiniteOk,
    DefiniteFailed,
    Ambiguous,
}

/// Per-attempt wire telemetry. `request_sent_at` is `Some(ms since epoch)`
/// once the request bytes were handed to the transport, `None` when the
/// call failed before anything was sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallTelemetry {
    pub request_sent_at: Option<u64>,
}

impl CallTelemetry {
    pub fn not_sent() -> Self {
        Self {
            request_sent_at: None,
        }
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Error)]
pub enum LinkError {
    #[error(transparent)]
    Stdio(#[from] ConnectorError),
    #[error(transparent)]
    Http(#[from] HttpConnectorError),
    #[error("no mcp_url in {0}'s config.json (mcp-http needs one)")]
    NoUrl(String),
    #[error("I/O: {0}")]
    Io(String),
}

impl LinkError {
    /// Classify a failed call. `request_sent` is whether the attempt's
    /// telemetry recorded `request_sent_at` (bytes reached the wire).
    ///
    /// DefiniteFailed is only for calls that provably never ran or were
    /// explicitly rejected:
    /// - connect/DNS/TLS failure before the request was sent
    ///   ([`HttpConnectorError::TransportSetup`] with no bytes sent),
    /// - 4xx validation/auth rejections (400/401/403/404/409/422),
    /// - unknown tool / tool not declared / spawn / handshake failures,
    /// - MCP error responses (the server explicitly answered with an error).
    ///
    /// Everything else is Ambiguous: 5xx, 408, 425, 429, any other status,
    /// timeouts, mid-call I/O death, mid-body-read failures, and
    /// unparseable replies — in all of these the server may already have
    /// executed the write. When in doubt the classifier returns
    /// Ambiguous; a pre-send transport error observed *after* bytes were
    /// (also) recorded as sent likewise fails safe to Ambiguous.
    pub fn call_outcome(&self, request_sent: bool) -> CallOutcome {
        use CallOutcome::{Ambiguous, DefiniteFailed};
        match self {
            // Explicit rejections: the far side answered "no".
            LinkError::Stdio(ConnectorError::Mcp { .. })
            | LinkError::Http(HttpConnectorError::Mcp { .. })
            | LinkError::Stdio(ConnectorError::UnknownTool(_))
            | LinkError::Http(HttpConnectorError::UnknownTool(_))
            | LinkError::Stdio(ConnectorError::ToolNotDeclared(_))
            | LinkError::Http(HttpConnectorError::ToolNotDeclared(_)) => DefiniteFailed,
            LinkError::Http(HttpConnectorError::Status(400 | 401 | 403 | 404 | 409 | 422, _)) => {
                DefiniteFailed
            }
            // 408, 425, 429, 5xx, and any other status: the server may
            // have executed the write before responding.
            LinkError::Http(HttpConnectorError::Status(..)) => Ambiguous,
            // Never ran: spawn, handshake, and gating failures happen
            // before any tool call bytes exist.
            LinkError::Stdio(ConnectorError::Spawn(_))
            | LinkError::Stdio(ConnectorError::UnsupportedKind(_))
            | LinkError::Stdio(ConnectorError::Handshake(_))
            | LinkError::Http(HttpConnectorError::Handshake(_))
            | LinkError::Http(HttpConnectorError::NoUrl)
            | LinkError::NoUrl(_) => DefiniteFailed,
            // Transport: "did bytes reach the wire" separates the
            // classes. A pre-send failure (DNS/connect) with no bytes
            // sent provably never ran. Anything else — including a
            // pre-send-class error recorded after bytes were sent, which
            // should not happen — fails safe to Ambiguous.
            LinkError::Http(HttpConnectorError::TransportSetup(_)) if !request_sent => {
                DefiniteFailed
            }
            _ => Ambiguous,
        }
    }
}

/// A live connector, either transport.
pub enum ConnectorLink {
    Stdio(ConnectorProcess),
    Http(ConnectorHttp),
}

impl ConnectorLink {
    /// Open the connector installed at `dir`: spawn the `connector`
    /// executable for `mcp-stdio`, or connect to the `mcp_url` in
    /// `config.json` for `mcp-http`. `builtin` connectors live in the
    /// harness binary and are not opened through this path.
    pub fn open(
        manifest: &ConnectorManifest,
        dir: &Path,
        token: &str,
        timeout: Duration,
    ) -> Result<Self, LinkError> {
        match manifest.kind {
            ConnectorKind::McpStdio => {
                let exe = dir.join("connector");
                Ok(ConnectorLink::Stdio(ConnectorProcess::spawn(
                    manifest, &exe, token, timeout,
                )?))
            }
            ConnectorKind::McpHttp => {
                let url = mcp_url_from_config(dir)
                    .ok_or_else(|| LinkError::NoUrl(dir.display().to_string()))?;
                Ok(ConnectorLink::Http(ConnectorHttp::connect(
                    manifest, &url, token, timeout,
                )?))
            }
            ConnectorKind::Builtin => Err(LinkError::Io(format!(
                "connector {:?} is builtin: implemented inside the harness binary",
                manifest.name
            ))),
        }
    }

    /// Tools the harness exposes to sessions (manifest-filtered).
    pub fn tools(&self) -> &[McpTool] {
        match self {
            ConnectorLink::Stdio(p) => p.tools(),
            ConnectorLink::Http(h) => h.tools(),
        }
    }

    /// Call one of the connector's tools. The tool must be in the
    /// manifest's `provides`; approval policy is enforced by the caller.
    pub fn call(
        &mut self,
        tool: &str,
        arguments: HashMap<String, Value>,
    ) -> Result<Value, LinkError> {
        self.call_detailed(tool, arguments).0
    }

    /// [`call`](Self::call) plus per-attempt wire telemetry. The
    /// telemetry's `request_sent_at` feeds
    /// [`LinkError::call_outcome`]: it records whether the request bytes
    /// reached the wire, which is what separates a definite failure from
    /// an ambiguous one. Telemetry is returned on every path so the
    /// attempt can always be audited.
    pub fn call_detailed(
        &mut self,
        tool: &str,
        arguments: HashMap<String, Value>,
    ) -> (Result<Value, LinkError>, CallTelemetry) {
        match self {
            ConnectorLink::Stdio(p) => {
                let (result, telemetry) = p.call_detailed(tool, arguments);
                (result.map_err(LinkError::from), telemetry)
            }
            ConnectorLink::Http(h) => {
                let (result, telemetry) = h.call_detailed(tool, arguments);
                (result.map_err(LinkError::from), telemetry)
            }
        }
    }
}

/// Read one string value from an installed connector's `config.json`.
pub fn config_string(dir: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("config.json")).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

/// Read `mcp_url` from an installed connector's `config.json`.
pub fn mcp_url_from_config(dir: &Path) -> Option<String> {
    config_string(dir, "mcp_url")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConnectorError;
    use crate::process::ConnectorError;

    fn http(e: HttpConnectorError) -> LinkError {
        LinkError::Http(e)
    }

    fn stdio(e: ConnectorError) -> LinkError {
        LinkError::Stdio(e)
    }

    // ---- Ambiguous: the server may already have executed the write ----

    #[test]
    fn stdio_timeout_is_ambiguous() {
        assert_eq!(
            stdio(ConnectorError::Timeout).call_outcome(true),
            CallOutcome::Ambiguous
        );
    }

    #[test]
    fn stdio_io_is_ambiguous() {
        // The child died mid-call; bytes were already in its stdin pipe.
        assert_eq!(
            stdio(ConnectorError::Io("broken pipe".to_string())).call_outcome(true),
            CallOutcome::Ambiguous
        );
    }

    #[test]
    fn stdio_unparseable_reply_is_ambiguous() {
        assert_eq!(
            stdio(ConnectorError::Protocol("bad JSON".to_string())).call_outcome(true),
            CallOutcome::Ambiguous
        );
    }

    #[test]
    fn http_mid_call_transport_is_ambiguous() {
        assert_eq!(
            http(HttpConnectorError::TransportMidCall("timeout".to_string())).call_outcome(true),
            CallOutcome::Ambiguous
        );
    }

    #[test]
    fn http_mid_body_read_failure_is_ambiguous() {
        // Headers arrived (request_sent_at set), body died mid-read.
        assert_eq!(
            http(HttpConnectorError::TransportMidCall(
                "response body died mid-read: truncated".to_string()
            ))
            .call_outcome(true),
            CallOutcome::Ambiguous
        );
    }

    #[test]
    fn http_protocol_error_is_ambiguous() {
        assert_eq!(
            http(HttpConnectorError::Protocol(
                "bad JSON response".to_string()
            ))
            .call_outcome(true),
            CallOutcome::Ambiguous
        );
    }

    #[test]
    fn http_5xx_is_ambiguous() {
        for code in [500, 502, 503, 504] {
            assert_eq!(
                http(HttpConnectorError::Status(code, "boom".to_string())).call_outcome(true),
                CallOutcome::Ambiguous,
                "HTTP {code} must be ambiguous"
            );
        }
    }

    #[test]
    fn http_408_425_429_are_ambiguous() {
        for code in [408, 425, 429] {
            assert_eq!(
                http(HttpConnectorError::Status(code, "slow down".to_string())).call_outcome(true),
                CallOutcome::Ambiguous,
                "HTTP {code} must be ambiguous"
            );
        }
    }

    #[test]
    fn http_unknown_status_is_ambiguous_fail_safe() {
        // Any status not on the explicit definite list fails safe.
        for code in [402, 405, 410, 418, 451, 599] {
            assert_eq!(
                http(HttpConnectorError::Status(code, "hmm".to_string())).call_outcome(true),
                CallOutcome::Ambiguous,
                "HTTP {code} must be ambiguous"
            );
        }
    }

    // ---- DefiniteFailed: provably never ran or explicitly rejected ----

    #[test]
    fn http_pre_send_transport_is_definite() {
        assert_eq!(
            http(HttpConnectorError::TransportSetup(
                "DNS resolution failed".to_string()
            ))
            .call_outcome(false),
            CallOutcome::DefiniteFailed
        );
        assert_eq!(
            http(HttpConnectorError::TransportSetup(
                "TCP connect failed".to_string()
            ))
            .call_outcome(false),
            CallOutcome::DefiniteFailed
        );
    }

    #[test]
    fn http_definite_4xx_are_definite() {
        for code in [400, 401, 403, 404, 409, 422] {
            assert_eq!(
                http(HttpConnectorError::Status(code, "rejected".to_string())).call_outcome(true),
                CallOutcome::DefiniteFailed,
                "HTTP {code} must be definite"
            );
        }
    }

    #[test]
    fn mcp_error_responses_are_definite() {
        assert_eq!(
            http(HttpConnectorError::Mcp {
                code: -32602,
                message: "invalid params".to_string()
            })
            .call_outcome(true),
            CallOutcome::DefiniteFailed
        );
        assert_eq!(
            stdio(ConnectorError::Mcp {
                code: -32602,
                message: "invalid params".to_string()
            })
            .call_outcome(true),
            CallOutcome::DefiniteFailed
        );
    }

    #[test]
    fn gating_failures_are_definite() {
        assert_eq!(
            stdio(ConnectorError::UnknownTool("x".to_string())).call_outcome(false),
            CallOutcome::DefiniteFailed
        );
        assert_eq!(
            http(HttpConnectorError::UnknownTool("x".to_string())).call_outcome(false),
            CallOutcome::DefiniteFailed
        );
        assert_eq!(
            stdio(ConnectorError::ToolNotDeclared("x".to_string())).call_outcome(false),
            CallOutcome::DefiniteFailed
        );
        assert_eq!(
            stdio(ConnectorError::Spawn("nope".to_string())).call_outcome(false),
            CallOutcome::DefiniteFailed
        );
        assert_eq!(
            stdio(ConnectorError::Handshake("bad".to_string())).call_outcome(false),
            CallOutcome::DefiniteFailed
        );
        assert_eq!(
            http(HttpConnectorError::Handshake("bad".to_string())).call_outcome(false),
            CallOutcome::DefiniteFailed
        );
        assert_eq!(
            http(HttpConnectorError::NoUrl).call_outcome(false),
            CallOutcome::DefiniteFailed
        );
        assert_eq!(
            LinkError::NoUrl("dir".to_string()).call_outcome(false),
            CallOutcome::DefiniteFailed
        );
    }

    // ---- Fail-safe overrides ----

    #[test]
    fn pre_send_error_with_bytes_sent_fails_safe_to_ambiguous() {
        // Should not happen (TransportSetup implies nothing was sent), but
        // if the telemetry ever disagrees, ambiguity wins.
        assert_eq!(
            http(HttpConnectorError::TransportSetup("DNS?".to_string())).call_outcome(true),
            CallOutcome::Ambiguous
        );
    }

    #[test]
    fn link_io_is_ambiguous() {
        assert_eq!(
            LinkError::Io("?".to_string()).call_outcome(false),
            CallOutcome::Ambiguous
        );
    }

    #[test]
    fn telemetry_not_sent_by_default() {
        assert_eq!(CallTelemetry::not_sent().request_sent_at, None);
    }
}
