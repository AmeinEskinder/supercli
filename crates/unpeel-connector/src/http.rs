//! MCP over Streamable HTTP (`kind = "mcp-http"`).
//!
//! The connector's MCP server URL comes from the installed connector's
//! `config.json` (`{"mcp_url": "https://..."}`). The harness POSTs
//! JSON-RPC to the endpoint with `Accept: application/json,
//! text/event-stream` and an `Authorization: Bearer <token>` header when
//! the connector has a token. Responses are either plain JSON or a
//! server-sent-events stream; both are handled. A server-issued
//! `Mcp-Session-Id` is remembered and sent back on later requests.
//!
//! Like the stdio spawner, the advertised tool list is filtered against
//! the manifest's closed `tools.provides` — an HTTP connector that
//! starts offering new tools fails closed at connect time.

use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;

use crate::link::{now_ms, CallTelemetry};
use crate::manifest::ConnectorManifest;
use crate::process::McpTool;

/// The raw outcome of one HTTP POST: the status code, optional session id,
/// body text, and optional SSE event id — or the transport/MCP error — plus
/// the send telemetry for the attempt.
type PostOutcome = (
    Result<(u16, Option<String>, String, Option<String>), HttpConnectorError>,
    CallTelemetry,
);

#[derive(Debug, Error)]
pub enum HttpConnectorError {
    /// The transport failed before any request byte reached the wire:
    /// DNS resolution, TCP connect (including the TLS handshake), proxy
    /// setup, or a client-side request-construction error. The far side
    /// provably never saw the call.
    #[error("HTTP transport (before request sent): {0}")]
    TransportSetup(String),
    /// The transport failed after the request may have been sent:
    /// send/recv timeouts, mid-call I/O death, or a response body that
    /// died mid-read. The server may already have executed the call.
    #[error("HTTP transport (after request sent): {0}")]
    TransportMidCall(String),
    #[error("HTTP {0}: {1}")]
    Status(u16, String),
    #[error("MCP handshake failed: {0}")]
    Handshake(String),
    #[error("connector offers undeclared tool {0:?}: failing closed")]
    ToolNotDeclared(String),
    #[error("unknown tool {0:?}")]
    UnknownTool(String),
    #[error("MCP error {code}: {message}")]
    Mcp { code: i64, message: String },
    #[error("MCP protocol error: {0}")]
    Protocol(String),
    #[error("no mcp_url in the connector's config.json")]
    NoUrl,
}

/// A connected MCP-over-HTTP connector.
#[derive(Debug)]
pub struct ConnectorHttp {
    #[allow(dead_code)]
    manifest_name: String,
    url: String,
    token: String,
    session_id: Option<String>,
    provides: Vec<String>,
    tools: Vec<McpTool>,
    agent: ureq::Agent,
    next_id: u64,
}

impl ConnectorHttp {
    /// Connect, run the MCP handshake, and verify the tool list against
    /// the manifest.
    pub fn connect(
        manifest: &ConnectorManifest,
        url: &str,
        token: &str,
        timeout: Duration,
    ) -> Result<Self, HttpConnectorError> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build()
            .into();
        let mut link = Self {
            manifest_name: manifest.name.clone(),
            url: url.to_string(),
            token: token.to_string(),
            session_id: None,
            provides: manifest.provides.clone(),
            tools: Vec::new(),
            agent,
            next_id: 1,
        };
        let (init_result, _) = link.request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "unpeel-host", "version": env!("CARGO_PKG_VERSION") },
            }),
        );
        let init_result =
            init_result.map_err(|e| HttpConnectorError::Handshake(format!("initialize: {e}")))?;
        if init_result.get("protocolVersion").is_none() {
            return Err(HttpConnectorError::Handshake(
                "initialize response has no protocolVersion".to_string(),
            ));
        }
        link.notify("notifications/initialized", serde_json::json!({}))
            .map_err(|e| HttpConnectorError::Handshake(format!("initialized: {e}")))?;

        let (tools_result, _) = link.request("tools/list", serde_json::json!({}));
        let tools_result =
            tools_result.map_err(|e| HttpConnectorError::Handshake(format!("tools/list: {e}")))?;
        let advertised: Vec<McpTool> = tools_result
            .get("tools")
            .and_then(|t| serde_json::from_value(t.clone()).ok())
            .ok_or_else(|| HttpConnectorError::Handshake("tools/list has no tools".to_string()))?;
        for tool in &advertised {
            if !link.provides.contains(&tool.name) {
                return Err(HttpConnectorError::ToolNotDeclared(tool.name.clone()));
            }
        }
        link.tools = advertised;
        Ok(link)
    }

    /// Tools the harness exposes to sessions (manifest-filtered).
    pub fn tools(&self) -> &[McpTool] {
        &self.tools
    }

    /// Call one of the connector's tools. The tool must be in the
    /// manifest's `provides`; approval policy is enforced by the caller.
    pub fn call(
        &mut self,
        tool: &str,
        arguments: HashMap<String, Value>,
    ) -> Result<Value, HttpConnectorError> {
        self.call_detailed(tool, arguments).0
    }

    /// [`call`](Self::call) plus per-attempt wire telemetry: whether the
    /// request bytes reached the wire, which is what separates a definite
    /// failure from an ambiguous one (see
    /// [`CallOutcome`](crate::link::CallOutcome)). The telemetry is
    /// returned on every path — including failures — so the attempt can
    /// always be audited.
    pub fn call_detailed(
        &mut self,
        tool: &str,
        arguments: HashMap<String, Value>,
    ) -> (Result<Value, HttpConnectorError>, CallTelemetry) {
        if !self.provides.contains(&tool.to_string()) {
            return (
                Err(HttpConnectorError::UnknownTool(tool.to_string())),
                CallTelemetry::not_sent(),
            );
        }
        self.request(
            "tools/call",
            serde_json::json!({ "name": tool, "arguments": arguments }),
        )
    }

    fn post(&mut self, body: &Value) -> PostOutcome {
        let mut req = self
            .agent
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");
        if !self.token.is_empty() {
            req = req.header("Authorization", &format!("Bearer {}", self.token));
        }
        if let Some(session) = &self.session_id {
            req = req.header("Mcp-Session-Id", session.as_str());
        }
        let mut response = match req.send_json(body) {
            Ok(response) => response,
            Err(e) => return (Err(map_send_error(e)), CallTelemetry::not_sent()),
        };
        // Response headers arrived: the request bytes reached the wire
        // (and the server). Everything from here on is mid-call, even if
        // the body then dies mid-read.
        let telemetry = CallTelemetry {
            request_sent_at: Some(now_ms()),
        };
        let status = response.status().as_u16();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let session = header("Mcp-Session-Id");
        let content_type = header("Content-Type");
        let text = match response.body_mut().read_to_string() {
            Ok(text) => text,
            Err(e) => {
                return (
                    Err(HttpConnectorError::TransportMidCall(format!(
                        "response body died mid-read: {e}"
                    ))),
                    telemetry,
                )
            }
        };
        (Ok((status, session, text, content_type)), telemetry)
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), HttpConnectorError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let (post_result, _) = self.post(&body);
        let (status, session, _, _) = post_result?;
        if let Some(s) = session {
            self.session_id = Some(s);
        }
        // Notifications return 202 Accepted with no body on a conforming
        // server; tolerate anything 2xx.
        if !(200..300).contains(&status) {
            return Err(HttpConnectorError::Status(status, method.to_string()));
        }
        Ok(())
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
    ) -> (Result<Value, HttpConnectorError>, CallTelemetry) {
        let id = self.next_id;
        self.next_id += 1;
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let (post_result, telemetry) = self.post(&body);
        let (status, session, text, content_type) = match post_result {
            Ok(parts) => parts,
            Err(e) => return (Err(e), telemetry),
        };
        if let Some(s) = session {
            self.session_id = Some(s);
        }
        if !(200..300).contains(&status) {
            return (
                Err(HttpConnectorError::Status(
                    status,
                    text.chars().take(200).collect(),
                )),
                telemetry,
            );
        }
        let message: Value = if content_type
            .as_deref()
            .unwrap_or("")
            .contains("text/event-stream")
        {
            match sse_last_data(&text) {
                Some(data) => data,
                None => {
                    return (
                        Err(HttpConnectorError::Protocol(
                            "SSE stream has no data".to_string(),
                        )),
                        telemetry,
                    )
                }
            }
        } else {
            match serde_json::from_str(&text) {
                Ok(message) => message,
                Err(e) => {
                    return (
                        Err(HttpConnectorError::Protocol(format!(
                            "bad JSON response: {e}"
                        ))),
                        telemetry,
                    )
                }
            }
        };
        if let Some(error) = message.get("error") {
            return (
                Err(HttpConnectorError::Mcp {
                    code: error.get("code").and_then(|c| c.as_i64()).unwrap_or(0),
                    message: error
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                }),
                telemetry,
            );
        }
        match message.get("result").cloned() {
            Some(value) => (Ok(value), telemetry),
            None => (
                Err(HttpConnectorError::Protocol(
                    "response has no result".to_string(),
                )),
                telemetry,
            ),
        }
    }
}

/// Classify a `send_json` failure by whether any request byte could have
/// reached the server. DNS resolution and TCP-connect failures (the TLS
/// handshake rides inside the connect phase) happen before the wire, so
/// the far side provably never saw the call. An `Io` error is genuinely
/// ambiguous — ureq does not distinguish a TLS handshake failure
/// (pre-send) from a mid-request reset (post-send) — so it fails safe to
/// mid-call. Anything not provably pre-send is mid-call.
fn map_send_error(e: ureq::Error) -> HttpConnectorError {
    match &e {
        ureq::Error::HostNotFound => {
            HttpConnectorError::TransportSetup(format!("DNS resolution failed: {e}"))
        }
        ureq::Error::ConnectionFailed => {
            HttpConnectorError::TransportSetup(format!("TCP connect failed: {e}"))
        }
        ureq::Error::Timeout(t) => match t {
            ureq::Timeout::Resolve | ureq::Timeout::Connect => HttpConnectorError::TransportSetup(
                format!("timeout during connection setup: {t:?}"),
            ),
            // Send/recv/global timeouts: the request may be sitting in
            // the server's queue right now.
            _ => HttpConnectorError::TransportMidCall(format!("timeout mid-call: {t:?}")),
        },
        ureq::Error::Io(io_err) => {
            // A refused TCP connect provably sent nothing: the SYN got a
            // RST during connection establishment (a mid-request reset
            // surfaces as ConnectionReset instead). Any other I/O error
            // is fail-safe ambiguous — it may be a TLS handshake failure
            // (pre-send) or a mid-request reset (post-send), and ureq
            // does not distinguish.
            if io_err.kind() == std::io::ErrorKind::ConnectionRefused {
                HttpConnectorError::TransportSetup(format!("TCP connect refused: {io_err}"))
            } else {
                HttpConnectorError::TransportMidCall(format!("I/O error mid-call: {io_err}"))
            }
        }
        // The server answered our POST with a redirect we refuse to
        // follow: the request reached it, so it may have executed.
        ureq::Error::RedirectFailed => HttpConnectorError::TransportMidCall(format!(
            "POST redirect refused after the request was sent: {e}"
        )),
        // Malformed HTTP bytes coming back: the server sent something,
        // so our request reached it.
        ureq::Error::Protocol(_) => HttpConnectorError::Protocol(format!("bad HTTP framing: {e}")),
        // Unreachable with http_status_as_error(false), but if it ever
        // fires the status classifier in link.rs is the single grounding
        // point for status codes — never TransportSetup.
        ureq::Error::StatusCode(code) => {
            HttpConnectorError::Status(*code, format!("HTTP status as error: {e}"))
        }
        // Client-side construction failures (bad URI, proxy config):
        // nothing left the process.
        _ => HttpConnectorError::TransportSetup(format!("request setup failed: {e}")),
    }
}

/// Take the last `data:` payload of a server-sent-events body and parse
/// it as JSON (MCP streams one JSON-RPC message per event).
fn sse_last_data(text: &str) -> Option<Value> {
    let mut last: Option<&str> = None;
    for line in text.lines() {
        let line = line.strip_prefix("data:").unwrap_or(line).trim();
        if line.is_empty() || line == "[DONE]" {
            continue;
        }
        last = Some(line);
    }
    last.and_then(|l| serde_json::from_str(l).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::LinkError;
    use crate::manifest::parse_manifest;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    const MANIFEST: &str = r#"
[connector]
name = "httpstub"
version = "1.0.0"
display_name = "HTTP Stub"
description = "HTTP test stub."
kind = "mcp-http"

[tools]
provides = ["httpstub.echo"]
"#;

    /// Minimal MCP-over-HTTP stub: plain-JSON responses, echoes the bearer
    /// token inside tool results.
    fn serve_stub() -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming().take(4) {
                let mut stream = stream.unwrap();
                let req = read_request(&mut stream);
                let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
                let body: Value = serde_json::from_str(&req[body_start..]).unwrap_or(Value::Null);
                let id = body.get("id").cloned().unwrap_or(Value::Null);
                let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("");
                let auth = req
                    .lines()
                    .find(|l| l.to_lowercase().starts_with("authorization:"))
                    .unwrap_or("")
                    .to_string();
                let result = match method {
                    "initialize" => {
                        serde_json::json!({"protocolVersion": "2024-11-05", "capabilities": {}})
                    }
                    "tools/list" => {
                        serde_json::json!({"tools": [{"name": "httpstub.echo", "description": "echo", "inputSchema": {"type": "object"}}]})
                    }
                    "tools/call" => {
                        serde_json::json!({"content": [{"type": "text", "text": format!("echo auth={auth}")}]})
                    }
                    _ => serde_json::json!({}),
                };
                let payload =
                    serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string();
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

    /// Read a full HTTP request: headers, then exactly Content-Length
    /// body bytes (a single read() may return before the body arrives).
    fn read_request(stream: &mut std::net::TcpStream) -> String {
        let mut raw = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = stream.read(&mut buf).unwrap_or(0);
            if n == 0 {
                break;
            }
            raw.extend_from_slice(&buf[..n]);
            if let Some(end) = find_header_end(&raw) {
                let headers = String::from_utf8_lossy(&raw[..end]).to_string();
                let content_length = headers
                    .lines()
                    .find(|l| l.to_lowercase().starts_with("content-length:"))
                    .and_then(|l| l.split(':').nth(1))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                let want = end + 4 + content_length;
                while raw.len() < want {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    raw.extend_from_slice(&buf[..n]);
                }
                break;
            }
        }
        String::from_utf8_lossy(&raw).into_owned()
    }

    fn find_header_end(raw: &[u8]) -> Option<usize> {
        raw.windows(4).position(|w| w == b"\r\n\r\n")
    }

    #[test]
    fn http_handshake_and_call() {
        let (url, handle) = serve_stub();
        let manifest = parse_manifest(MANIFEST).unwrap();
        let mut link = ConnectorHttp::connect(&manifest, &url, "tok-1", Duration::from_secs(10))
            .expect("connect");
        assert_eq!(link.tools().len(), 1);
        assert_eq!(link.tools()[0].name, "httpstub.echo");
        let result = link.call("httpstub.echo", HashMap::new()).expect("call");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Bearer tok-1"), "bearer token sent: {text}");
        handle.join().unwrap();
    }

    #[test]
    fn call_rejects_unknown_tool() {
        let (url, handle) = serve_stub();
        let manifest = parse_manifest(MANIFEST).unwrap();
        let mut link =
            ConnectorHttp::connect(&manifest, &url, "", Duration::from_secs(10)).expect("connect");
        let err = link
            .call("nope.missing", HashMap::new())
            .expect_err("unknown");
        assert!(matches!(err, HttpConnectorError::UnknownTool(_)));
        // The stub only serves the handshake here; detach the server
        // thread rather than joining a listener that would block.
        std::mem::forget(handle);
    }

    /// How the stub misbehaves on `tools/call` (handshake always succeeds).
    #[derive(Clone, Copy)]
    enum CallMisbehavior {
        /// Headers promise 500 body bytes, then the connection dies.
        TruncateBody,
        /// Never responds (client timeout fires).
        Hang,
        /// Responds with the given HTTP status.
        Status(u16),
    }

    /// Stub whose handshake is healthy but whose `tools/call` misbehaves.
    fn serve_misbehaving(misbehavior: CallMisbehavior) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming().take(4) {
                let mut stream = stream.unwrap();
                let req = read_request(&mut stream);
                let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
                let body: Value = serde_json::from_str(&req[body_start..]).unwrap_or(Value::Null);
                let id = body.get("id").cloned().unwrap_or(Value::Null);
                let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("");
                let payload =
                    serde_json::json!({"jsonrpc": "2.0", "id": id, "result": {}}).to_string();
                match method {
                    "initialize" => {
                        let result = serde_json::json!({"protocolVersion": "2024-11-05", "capabilities": {}});
                        let payload =
                            serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result})
                                .to_string();
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            payload.len(),
                            payload
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                    "tools/list" => {
                        let result = serde_json::json!({"tools": [{"name": "httpstub.echo", "description": "echo", "inputSchema": {"type": "object"}}]});
                        let payload =
                            serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result})
                                .to_string();
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            payload.len(),
                            payload
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                    "tools/call" => match misbehavior {
                        CallMisbehavior::TruncateBody => {
                            // Promise a long body, deliver a fragment, die.
                            let head = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 500\r\nConnection: close\r\n\r\n{\"jsonrpc\":\"2.0\",";
                            let _ = stream.write_all(head.as_bytes());
                            // Dropping the stream mid-body: the client's
                            // read_to_string fails.
                        }
                        CallMisbehavior::Hang => {
                            std::thread::sleep(Duration::from_secs(30));
                        }
                        CallMisbehavior::Status(code) => {
                            let reason = match code {
                                400 => "Bad Request",
                                500 => "Internal Server Error",
                                _ => "Error",
                            };
                            let response = format!(
                                "HTTP/1.1 {code} {reason}\r\nContent-Length: 2\r\nConnection: close\r\n\r\nxx"
                            );
                            let _ = stream.write_all(response.as_bytes());
                        }
                    },
                    _ => {
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            payload.len(),
                            payload
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                }
            }
        });
        (url, handle)
    }

    fn connect_misbehaving(
        misbehavior: CallMisbehavior,
        timeout: Duration,
    ) -> (ConnectorHttp, std::thread::JoinHandle<()>) {
        let (url, handle) = serve_misbehaving(misbehavior);
        let manifest = parse_manifest(MANIFEST).unwrap();
        let link = ConnectorHttp::connect(&manifest, &url, "", timeout).expect("connect");
        (link, handle)
    }

    #[test]
    fn mid_body_read_failure_is_ambiguous_with_sent_telemetry() {
        let (mut link, handle) =
            connect_misbehaving(CallMisbehavior::TruncateBody, Duration::from_secs(10));
        let (result, telemetry) = link.call_detailed("httpstub.echo", HashMap::new());
        let err = result.expect_err("body must die mid-read");
        assert!(
            matches!(err, HttpConnectorError::TransportMidCall(_)),
            "unexpected error: {err}"
        );
        // Headers arrived, so bytes reached the wire.
        assert!(
            telemetry.request_sent_at.is_some(),
            "request_sent_at must be recorded"
        );
        assert_eq!(
            LinkError::Http(err).call_outcome(telemetry.request_sent_at.is_some()),
            crate::link::CallOutcome::Ambiguous
        );
        handle.join().unwrap();
    }

    #[test]
    fn call_timeout_is_ambiguous() {
        let (mut link, handle) =
            connect_misbehaving(CallMisbehavior::Hang, Duration::from_millis(500));
        let (result, telemetry) = link.call_detailed("httpstub.echo", HashMap::new());
        let err = result.expect_err("call must time out");
        assert!(
            matches!(err, HttpConnectorError::TransportMidCall(_)),
            "unexpected error: {err}"
        );
        assert!(telemetry.request_sent_at.is_none());
        // A global timeout can fire before the headers come back, so no
        // sent timestamp is recorded — but the variant already encodes
        // "not provably pre-send", so the outcome is still Ambiguous.
        assert_eq!(
            LinkError::Http(err).call_outcome(telemetry.request_sent_at.is_some()),
            crate::link::CallOutcome::Ambiguous
        );
        // The hanging server thread would block join(); detach it.
        std::mem::forget(handle);
    }

    #[test]
    fn http_500_call_is_ambiguous() {
        let (mut link, handle) =
            connect_misbehaving(CallMisbehavior::Status(500), Duration::from_secs(10));
        let (result, telemetry) = link.call_detailed("httpstub.echo", HashMap::new());
        let err = result.expect_err("500 must fail");
        assert!(
            matches!(err, HttpConnectorError::Status(500, _)),
            "unexpected error: {err}"
        );
        assert!(telemetry.request_sent_at.is_some());
        assert_eq!(
            LinkError::Http(err).call_outcome(true),
            crate::link::CallOutcome::Ambiguous
        );
        handle.join().unwrap();
    }

    #[test]
    fn http_400_call_is_definite() {
        let (mut link, handle) =
            connect_misbehaving(CallMisbehavior::Status(400), Duration::from_secs(10));
        let (result, telemetry) = link.call_detailed("httpstub.echo", HashMap::new());
        let err = result.expect_err("400 must fail");
        assert!(
            matches!(err, HttpConnectorError::Status(400, _)),
            "unexpected error: {err}"
        );
        assert!(telemetry.request_sent_at.is_some());
        assert_eq!(
            LinkError::Http(err).call_outcome(true),
            crate::link::CallOutcome::DefiniteFailed
        );
        handle.join().unwrap();
    }

    #[test]
    fn send_error_mapping() {
        // No network: classify constructed ureq errors directly.
        assert!(matches!(
            map_send_error(ureq::Error::HostNotFound),
            HttpConnectorError::TransportSetup(_)
        ));
        assert!(matches!(
            map_send_error(ureq::Error::ConnectionFailed),
            HttpConnectorError::TransportSetup(_)
        ));
        assert!(matches!(
            map_send_error(ureq::Error::Timeout(ureq::Timeout::Connect)),
            HttpConnectorError::TransportSetup(_)
        ));
        assert!(matches!(
            map_send_error(ureq::Error::Timeout(ureq::Timeout::Resolve)),
            HttpConnectorError::TransportSetup(_)
        ));
        // Anything at/after send is mid-call, including kinds we cannot
        // attribute (I/O covers both TLS-handshake and mid-request
        // failures: fail safe).
        assert!(matches!(
            map_send_error(ureq::Error::Timeout(ureq::Timeout::SendBody)),
            HttpConnectorError::TransportMidCall(_)
        ));
        assert!(matches!(
            map_send_error(ureq::Error::Timeout(ureq::Timeout::RecvBody)),
            HttpConnectorError::TransportMidCall(_)
        ));
        assert!(matches!(
            map_send_error(ureq::Error::Timeout(ureq::Timeout::Global)),
            HttpConnectorError::TransportMidCall(_)
        ));
        assert!(matches!(
            map_send_error(ureq::Error::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "rst"
            ))),
            HttpConnectorError::TransportMidCall(_)
        ));
        // A refused connect provably sent nothing: pre-send, definite.
        assert!(matches!(
            map_send_error(ureq::Error::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "rst"
            ))),
            HttpConnectorError::TransportSetup(_)
        ));
        assert!(matches!(
            map_send_error(ureq::Error::RedirectFailed),
            HttpConnectorError::TransportMidCall(_)
        ));
    }

    #[test]
    fn connect_refused_is_definite() {
        // Bind a port then close it: TCP connect is refused before any
        // byte is sent.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let manifest = parse_manifest(MANIFEST).unwrap();
        let err = ConnectorHttp::connect(
            &manifest,
            &format!("http://127.0.0.1:{port}"),
            "",
            Duration::from_secs(2),
        )
        .expect_err("connect must be refused");
        let link_err = LinkError::Http(err);
        assert!(
            link_err.to_string().contains("refused"),
            "unexpected error: {link_err}"
        );
        assert_eq!(
            link_err.call_outcome(false),
            crate::link::CallOutcome::DefiniteFailed
        );
    }
}
