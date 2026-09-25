//! Connector process management: spawn a `mcp-stdio` connector executable,
//! run the MCP handshake over JSON-RPC 2.0 on stdio, and expose its tools.
//!
//! Security properties enforced here (see `docs/connectors.md`):
//! - the keychain token is injected as `SUPERCLI_CONNECTOR_TOKEN` env; the
//!   harness never passes it on the command line;
//! - the connector's advertised tool list is filtered against the
//!   manifest's closed `tools.provides` — a connector that starts
//!   offering new tools fails closed at spawn;
//! - every `call` re-checks the tool against `provides` (defense in
//!   depth; the session's approval policy is enforced by the caller).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::link::{now_ms, CallTelemetry};
use crate::manifest::{ConnectorKind, ConnectorManifest};

/// Env var carrying the connector's keychain token to its process.
pub const CONNECTOR_TOKEN_ENV: &str = "SUPERCLI_CONNECTOR_TOKEN";

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("cannot spawn connector: {0}")]
    Spawn(String),
    #[error("connector kind {0:?} is not runnable by this spawner")]
    UnsupportedKind(ConnectorKind),
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
    #[error("connector I/O error: {0}")]
    Io(String),
    #[error("connector timed out")]
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Option<Value>,
}

/// A running connector process with a completed MCP handshake.
#[derive(Debug)]
pub struct ConnectorProcess {
    #[allow(dead_code)]
    manifest_name: String,
    provides: Vec<String>,
    tools: Vec<McpTool>,
    child: Child,
    stdin: ChildStdin,
    responses: Receiver<String>,
    next_id: u64,
    timeout: Duration,
    stderr_log: std::sync::Arc<std::sync::Mutex<String>>,
}

impl ConnectorProcess {
    /// Spawn the connector executable, inject the keychain token, run the
    /// MCP handshake, and verify its tool list against the manifest.
    pub fn spawn(
        manifest: &ConnectorManifest,
        executable: &Path,
        token: &str,
        timeout: Duration,
    ) -> Result<Self, ConnectorError> {
        if manifest.kind != ConnectorKind::McpStdio {
            return Err(ConnectorError::UnsupportedKind(manifest.kind));
        }
        let mut child = Command::new(executable)
            .env(CONNECTOR_TOKEN_ENV, token)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ConnectorError::Spawn(e.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ConnectorError::Spawn("no stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ConnectorError::Spawn("no stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ConnectorError::Spawn("no stderr".to_string()))?;

        // Pump stdout lines into a channel so reads can time out.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        if tx.send(line.trim_end().to_string()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Drain stderr for `doctor` diagnostics; never blocks the protocol.
        let stderr_log = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let stderr_log_clone = std::sync::Arc::clone(&stderr_log);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        if let Ok(mut log) = stderr_log_clone.lock() {
                            log.push_str(&line);
                            let overflow = log.len().saturating_sub(16 * 1024);
                            if overflow > 0 {
                                log.drain(..overflow);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut process = Self {
            manifest_name: manifest.name.clone(),
            provides: manifest.provides.clone(),
            tools: Vec::new(),
            child,
            stdin,
            responses: rx,
            next_id: 1,
            timeout,
            stderr_log,
        };

        // MCP handshake.
        let (init_result, _) = process.request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "supercli-host", "version": env!("CARGO_PKG_VERSION") },
            }),
        );
        let init_result =
            init_result.map_err(|e| ConnectorError::Handshake(format!("initialize: {e}")))?;
        if init_result.get("protocolVersion").is_none() {
            return Err(ConnectorError::Handshake(
                "initialize response has no protocolVersion".to_string(),
            ));
        }
        process.notify("notifications/initialized", serde_json::json!({}))?;

        // Tool list, filtered against the manifest's closed set.
        let (tools_result, _) = process.request("tools/list", serde_json::json!({}));
        let tools_result =
            tools_result.map_err(|e| ConnectorError::Handshake(format!("tools/list: {e}")))?;
        let advertised: Vec<McpTool> = tools_result
            .get("tools")
            .and_then(|t| serde_json::from_value(t.clone()).ok())
            .ok_or_else(|| ConnectorError::Handshake("tools/list has no tools".to_string()))?;
        for tool in &advertised {
            if !process.provides.contains(&tool.name) {
                return Err(ConnectorError::ToolNotDeclared(tool.name.clone()));
            }
        }
        process.tools = advertised;
        Ok(process)
    }

    /// Tools the harness exposes to sessions (manifest-filtered).
    pub fn tools(&self) -> &[McpTool] {
        &self.tools
    }

    /// Recent stderr output, for `connector doctor`.
    pub fn stderr_log(&self) -> String {
        self.stderr_log
            .lock()
            .map(|l| l.clone())
            .unwrap_or_default()
    }

    /// Call one of the connector's tools. The tool must be in the
    /// manifest's `provides`; approval policy is enforced by the caller.
    pub fn call(
        &mut self,
        tool: &str,
        arguments: HashMap<String, Value>,
    ) -> Result<Value, ConnectorError> {
        self.call_detailed(tool, arguments).0
    }

    /// [`call`](Self::call) plus per-attempt wire telemetry. For stdio,
    /// the request is "sent" once it is written to the child's stdin —
    /// bytes in the pipe may already be executing, so any failure after
    /// that is ambiguous (see
    /// [`CallOutcome`](crate::link::CallOutcome)).
    pub fn call_detailed(
        &mut self,
        tool: &str,
        arguments: HashMap<String, Value>,
    ) -> (Result<Value, ConnectorError>, CallTelemetry) {
        if !self.provides.contains(&tool.to_string()) {
            return (
                Err(ConnectorError::UnknownTool(tool.to_string())),
                CallTelemetry::not_sent(),
            );
        }
        self.request(
            "tools/call",
            serde_json::json!({ "name": tool, "arguments": arguments }),
        )
    }

    fn send(&mut self, message: &Value) -> Result<(), ConnectorError> {
        let mut line =
            serde_json::to_string(message).map_err(|e| ConnectorError::Protocol(e.to_string()))?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .map_err(|e| ConnectorError::Io(e.to_string()))?;
        self.stdin
            .flush()
            .map_err(|e| ConnectorError::Io(e.to_string()))?;
        Ok(())
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), ConnectorError> {
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
    ) -> (Result<Value, ConnectorError>, CallTelemetry) {
        let id = self.next_id;
        self.next_id += 1;
        if let Err(e) = self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        })) {
            // The write itself failed: some bytes may still have reached
            // the child (partial write), so the telemetry records
            // not-sent but the classifier still fails safe to Ambiguous
            // for I/O errors.
            return (Err(e), CallTelemetry::not_sent());
        }
        // The request bytes are in the child's stdin pipe: from here on
        // the child may already be executing, so every later failure is
        // ambiguous (see CallOutcome).
        let telemetry = CallTelemetry {
            request_sent_at: Some(now_ms()),
        };

        // Responses may interleave with notifications; skip the latter.
        let deadline_limit = 1000; // sanity bound on skipped notifications
        for _ in 0..deadline_limit {
            let line = match self.responses.recv_timeout(self.timeout) {
                Ok(line) => line,
                Err(_) => return (Err(ConnectorError::Timeout), telemetry),
            };
            let message: Value = match serde_json::from_str(&line) {
                Ok(message) => message,
                Err(e) => return (Err(ConnectorError::Protocol(e.to_string())), telemetry),
            };
            if message.get("id") != Some(&serde_json::Value::from(id)) {
                continue; // notification or a stale response
            }
            if let Some(error) = message.get("error") {
                return (
                    Err(ConnectorError::Mcp {
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
                Some(value) => return (Ok(value), telemetry),
                None => {
                    return (
                        Err(ConnectorError::Protocol(
                            "response has no result".to_string(),
                        )),
                        telemetry,
                    )
                }
            }
        }
        (
            Err(ConnectorError::Protocol(
                "too many interleaved notifications".to_string(),
            )),
            telemetry,
        )
    }
}

impl Drop for ConnectorProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::parse_manifest;
    use std::io::Write as IoWrite;

    /// Write a stub MCP server (Python) that serves a fixed tool list and
    /// echoes the token env var inside tool results.
    fn write_stub(dir: &Path, tools: &[&str]) -> std::path::PathBuf {
        let tools_json = serde_json::to_string(
            &tools
                .iter()
                .map(|name| {
                    serde_json::json!({
                        "name": name,
                        "description": format!("stub {name}"),
                        "inputSchema": { "type": "object" },
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let script = r#"import json, os, sys
TOOLS = json.loads(sys.argv[1])
TOKEN = os.environ.get("SUPERCLI_CONNECTOR_TOKEN", "")
def respond(id, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": id, "result": result}) + "\n")
    sys.stdout.flush()
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method = msg.get("method")
    mid = msg.get("id")
    if method == "initialize":
        respond(mid, {"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "stub", "version": "0"}})
    elif method == "tools/list":
        respond(mid, {"tools": TOOLS})
    elif method == "tools/call":
        name = msg["params"]["name"]
        respond(mid, {"content": [{"type": "text", "text": "called " + name + " token=" + TOKEN}]})
    elif method == "notifications/initialized":
        pass
"#;
        let path = dir.join("stub_connector.py");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(script.as_bytes()).unwrap();
        // Pass the tool list via argv: wrap python3 invocation in a shell script.
        let wrapper = dir.join("stub_connector");
        let mut wrapper_file = std::fs::File::create(&wrapper).unwrap();
        write!(
            wrapper_file,
            "#!/bin/sh\nexec python3 {} '{}'\n",
            path.display(),
            tools_json.replace('\'', "'\\''")
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        wrapper
    }

    const MANIFEST: &str = r#"
[connector]
name = "stub"
version = "0.1.0"
display_name = "Stub"
description = "Test stub."
kind = "mcp-stdio"

[tools]
provides = ["stub.echo"]
"#;

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("supercli-conn-proc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn spawn_handshake_and_call() {
        let dir = test_dir("ok");
        let exe = write_stub(&dir, &["stub.echo"]);
        let manifest = parse_manifest(MANIFEST).unwrap();
        let mut proc =
            ConnectorProcess::spawn(&manifest, &exe, "test-token-123", Duration::from_secs(10))
                .expect("spawn");
        assert_eq!(proc.tools().len(), 1);
        assert_eq!(proc.tools()[0].name, "stub.echo");

        let result = proc.call("stub.echo", HashMap::new()).expect("call");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("token=test-token-123"),
            "token injected via env: {text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fails_closed_on_undeclared_tool() {
        let dir = test_dir("undeclared");
        // Stub offers stub.echo (declared) plus stub.sneaky (not declared).
        let exe = write_stub(&dir, &["stub.echo", "stub.sneaky"]);
        let manifest = parse_manifest(MANIFEST).unwrap();
        let err = ConnectorProcess::spawn(&manifest, &exe, "t", Duration::from_secs(10))
            .expect_err("must fail closed");
        assert!(
            matches!(&err, ConnectorError::ToolNotDeclared(name) if name == "stub.sneaky"),
            "got: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_rejects_unknown_tool() {
        let dir = test_dir("unknown");
        let exe = write_stub(&dir, &["stub.echo"]);
        let manifest = parse_manifest(MANIFEST).unwrap();
        let mut proc =
            ConnectorProcess::spawn(&manifest, &exe, "t", Duration::from_secs(10)).expect("spawn");
        let err = proc
            .call("nope.missing", HashMap::new())
            .expect_err("unknown");
        assert!(matches!(err, ConnectorError::UnknownTool(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsupported_kind_is_an_error() {
        let manifest = parse_manifest(MANIFEST.replace("mcp-stdio", "mcp-http").as_str()).unwrap();
        let err = ConnectorProcess::spawn(
            &manifest,
            Path::new("/nonexistent"),
            "t",
            Duration::from_secs(1),
        )
        .expect_err("unsupported");
        assert!(matches!(err, ConnectorError::UnsupportedKind(_)));
    }

    /// Stub that handshakes normally but never answers `tools/call`.
    fn write_hanging_stub(dir: &Path) -> std::path::PathBuf {
        let script = r#"import json, sys, time
def respond(id, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": id, "result": result}) + "\n")
    sys.stdout.flush()
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method = msg.get("method")
    mid = msg.get("id")
    if method == "initialize":
        respond(mid, {"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "stub", "version": "0"}})
    elif method == "tools/list":
        respond(mid, {"tools": [{"name": "stub.echo", "description": "echo", "inputSchema": {"type": "object"}}]})
    elif method == "tools/call":
        time.sleep(30)
    elif method == "notifications/initialized":
        pass
"#;
        let path = dir.join("hanging_stub.py");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(script.as_bytes()).unwrap();
        let wrapper = dir.join("hanging_stub");
        let mut wrapper_file = std::fs::File::create(&wrapper).unwrap();
        write!(wrapper_file, "#!/bin/sh\nexec python3 {}\n", path.display()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        wrapper
    }

    #[test]
    fn call_timeout_is_ambiguous_with_sent_telemetry() {
        let dir = test_dir("hang");
        let exe = write_hanging_stub(&dir);
        let manifest = parse_manifest(MANIFEST).unwrap();
        let mut proc = ConnectorProcess::spawn(&manifest, &exe, "t", Duration::from_millis(500))
            .expect("spawn");
        let (result, telemetry) = proc.call_detailed("stub.echo", HashMap::new());
        let err = result.expect_err("call must time out");
        assert!(
            matches!(err, ConnectorError::Timeout),
            "unexpected error: {err}"
        );
        // The request bytes were written to the child's stdin pipe, so
        // the child may already be executing: ambiguous, and the attempt
        // records that the bytes were sent.
        assert!(
            telemetry.request_sent_at.is_some(),
            "request_sent_at must be recorded for stdio"
        );
        assert_eq!(
            crate::link::LinkError::Stdio(err).call_outcome(telemetry.request_sent_at.is_some()),
            crate::link::CallOutcome::Ambiguous
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
