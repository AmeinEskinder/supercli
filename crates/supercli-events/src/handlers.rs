//! Phase 1 handler executors: shell commands and localhost webhooks.
//!
//! Both speak the same JSON contract: [`HandlerInput`] on stdin / POST
//! body, [`HandlerOutput`] (`{decision, patch, message}`) on stdout /
//! response body. Exit codes are ignored — the JSON decides.
//!
//! Fail-closed: a crash, timeout, invalid output, or transport error is
//! reported as [`ExecFailure`] and the dispatcher treats it as `reject`.

use crate::registry::HandlerTarget;
use crate::{HandlerInput, HandlerOutput, RunOutcome};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Why a handler invocation failed (fail-closed to `reject`).
#[derive(Debug, Clone)]
pub enum ExecFailure {
    Timeout,
    /// Spawn failed, non-UTF8 output, or output that is not the JSON
    /// contract.
    BadOutput(String),
    /// Webhook transport error (localhost only, so this is rare).
    Transport(String),
}

/// The result of running one handler.
pub struct ExecResult {
    pub output: Option<HandlerOutput>,
    pub failure: Option<ExecFailure>,
    pub elapsed_ms: u64,
    /// Captured stderr (shell) — goes to the trace log.
    pub stderr: String,
}

impl ExecResult {
    fn failed(failure: ExecFailure, elapsed_ms: u64, stderr: String) -> Self {
        ExecResult {
            output: None,
            failure: Some(failure),
            elapsed_ms,
            stderr,
        }
    }
}

/// Run one handler against `input`, enforcing `timeout_ms`. Synchronous;
/// the caller (dispatcher) runs sync handlers inline in the mutating path.
pub fn execute(target: &HandlerTarget, input: &HandlerInput, timeout_ms: u64) -> ExecResult {
    match target {
        HandlerTarget::Command(argv) => execute_command(argv, input, timeout_ms),
        HandlerTarget::Webhook(url) => execute_webhook(url, input, timeout_ms),
    }
}

fn execute_command(argv: &[String], input: &HandlerInput, timeout_ms: u64) -> ExecResult {
    let started = Instant::now();
    let body = match serde_json::to_string(input) {
        Ok(b) => b,
        Err(e) => {
            return ExecResult::failed(
                ExecFailure::BadOutput(format!("serialize input: {e}")),
                0,
                String::new(),
            )
        }
    };
    let mut child = match Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return ExecResult::failed(
                ExecFailure::BadOutput(format!("spawn {}: {e}", argv[0])),
                started.elapsed().as_millis() as u64,
                String::new(),
            )
        }
    };

    // Write stdin, then close it, before waiting: a handler that reads to
    // EOF must not deadlock against our wait.
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(body.as_bytes()).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return ExecResult::failed(
                ExecFailure::BadOutput("write stdin".to_string()),
                started.elapsed().as_millis() as u64,
                String::new(),
            );
        }
    }

    let timeout = Duration::from_millis(timeout_ms.max(1));
    let elapsed_ms = || started.elapsed().as_millis() as u64;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return ExecResult::failed(ExecFailure::Timeout, elapsed_ms(), String::new());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => {
                let _ = child.kill();
                return ExecResult::failed(
                    ExecFailure::BadOutput(format!("wait: {e}")),
                    elapsed_ms(),
                    String::new(),
                );
            }
        }
    }

    let out = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            return ExecResult::failed(
                ExecFailure::BadOutput(format!("read output: {e}")),
                elapsed_ms(),
                String::new(),
            )
        }
    };
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let stdout = match String::from_utf8(out.stdout) {
        Ok(s) => s,
        Err(_) => {
            return ExecResult::failed(
                ExecFailure::BadOutput("non-UTF8 stdout".to_string()),
                elapsed_ms(),
                stderr,
            )
        }
    };
    // Take the first non-empty line: handlers may print log noise, but
    // the contract line is JSON. (Strictly: the whole stdout must parse;
    // leading/trailing whitespace is tolerated.)
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return ExecResult::failed(
            ExecFailure::BadOutput("empty stdout".to_string()),
            elapsed_ms(),
            stderr,
        );
    }
    match serde_json::from_str::<HandlerOutput>(trimmed) {
        Ok(output) => ExecResult {
            output: Some(output),
            failure: None,
            elapsed_ms: elapsed_ms(),
            stderr,
        },
        Err(e) => ExecResult::failed(
            ExecFailure::BadOutput(format!("stdout is not the JSON contract: {e}")),
            elapsed_ms(),
            stderr,
        ),
    }
}

/// Minimal localhost webhook POST. No external HTTP client: the contract
/// is JSON over a plain TCP socket with a short, hand-rolled HTTP/1.0
/// exchange. Localhost only (enforced at registration); no TLS, no
/// redirects, no chunked decoding beyond Content-Length framing.
fn execute_webhook(url: &str, input: &HandlerInput, timeout_ms: u64) -> ExecResult {
    let started = Instant::now();
    let elapsed_ms = || started.elapsed().as_millis() as u64;
    let fail = |f: ExecFailure| ExecResult::failed(f, elapsed_ms(), String::new());

    let (host, port, path) = match parse_local_url(url) {
        Ok(t) => t,
        Err(e) => return fail(ExecFailure::Transport(e)),
    };
    let body = match serde_json::to_string(input) {
        Ok(b) => b,
        Err(e) => return fail(ExecFailure::BadOutput(format!("serialize input: {e}"))),
    };
    let timeout = Duration::from_millis(timeout_ms.max(1));
    // `SocketAddr::from_str` does no DNS: map the one hostname we accept.
    // ("localhost" -> 127.0.0.1; "::1" stays bracketed and parses as IPv6.)
    let dial_host = if host == "localhost" {
        "127.0.0.1".to_string()
    } else {
        host.clone()
    };
    let addr = format!("{dial_host}:{port}");
    let sock_addr: std::net::SocketAddr = match addr.parse() {
        Ok(a) => a,
        Err(e) => return fail(ExecFailure::Transport(format!("parse addr {addr:?}: {e}"))),
    };
    let mut stream = match std::net::TcpStream::connect_timeout(&sock_addr, timeout) {
        Ok(s) => s,
        Err(e) => return fail(ExecFailure::Transport(format!("connect {addr}: {e}"))),
    };
    if stream
        .set_read_timeout(Some(timeout))
        .and_then(|()| stream.set_write_timeout(Some(timeout)))
        .is_err()
    {
        return fail(ExecFailure::Transport("set timeout".to_string()));
    }
    let event_id = input.event_id.clone();
    let req = format!(
        "POST {path} HTTP/1.0\r\nHost: {host}\r\nContent-Type: application/json\r\nX-Supercli-Event-Id: {event_id}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    if stream.write_all(req.as_bytes()).is_err() {
        return fail(ExecFailure::Transport("write request".to_string()));
    }
    let mut resp = Vec::new();
    {
        use std::io::Read;
        // Read until EOF or timeout; cap at 1 MiB.
        let mut buf = [0u8; 8192];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    resp.extend_from_slice(&buf[..n]);
                    if resp.len() > 1_048_576 {
                        return fail(ExecFailure::BadOutput("response too large".to_string()));
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    return fail(ExecFailure::Timeout)
                }
                Err(e) => return fail(ExecFailure::Transport(format!("read: {e}"))),
            }
            if started.elapsed() >= timeout {
                return fail(ExecFailure::Timeout);
            }
        }
    }
    let text = String::from_utf8_lossy(&resp);
    let body_start = text.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    let payload = text[body_start..].trim();
    if payload.is_empty() {
        return fail(ExecFailure::BadOutput("empty response body".to_string()));
    }
    match serde_json::from_str::<HandlerOutput>(payload) {
        Ok(output) => ExecResult {
            output: Some(output),
            failure: None,
            elapsed_ms: elapsed_ms(),
            stderr: String::new(),
        },
        Err(e) => fail(ExecFailure::BadOutput(format!(
            "response is not the JSON contract: {e}"
        ))),
    }
}

/// Split a validated localhost URL into (host, port, path). Registration
/// already enforced the localhost rule; this is framing only.
fn parse_local_url(url: &str) -> Result<(String, u16, String), String> {
    let lower = url.to_ascii_lowercase();
    let (default_port, rest) = if let Some(r) = lower.strip_prefix("http://") {
        (80u16, r)
    } else if let Some(r) = lower.strip_prefix("https://") {
        // https to localhost in Phase 1 is unusual but harmless; plain TCP
        // below would fail the TLS handshake and surface as a transport
        // error (fail-closed). Kept for URL-shape completeness.
        (443u16, r)
    } else {
        return Err("URL must start with http:// or https://".to_string());
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let (host, port) = if let Some(stripped) = authority.strip_prefix('[') {
        // [::1]:port
        let end = stripped.find(']').ok_or("bad IPv6 literal")?;
        let h = format!("[{}]", &stripped[..end]);
        let p = stripped[end + 1..]
            .strip_prefix(':')
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(default_port);
        (h, p)
    } else if let Some(colon) = authority.rfind(':') {
        let p = authority[colon + 1..]
            .parse::<u16>()
            .unwrap_or(default_port);
        (authority[..colon].to_string(), p)
    } else {
        (authority.to_string(), default_port)
    };
    let host = if host == "localhost" {
        "127.0.0.1".to_string()
    } else {
        host
    };
    Ok((host, port, path))
}

/// Map an [`ExecResult`] to the audit `outcome` string plus the effective
/// decision contribution. Failures fail closed to `reject`.
pub fn classify_exec(
    result: &ExecResult,
) -> (RunOutcome, crate::HookDecision, Option<&'static str>) {
    if let Some(f) = &result.failure {
        let outcome = match f {
            ExecFailure::Timeout => RunOutcome::Timeout,
            _ => RunOutcome::Crash,
        };
        let reason = match f {
            ExecFailure::Timeout => "hook_timeout",
            ExecFailure::BadOutput(_) => "hook_bad_output",
            ExecFailure::Transport(_) => "hook_crash",
        };
        return (outcome, crate::HookDecision::Reject, Some(reason));
    }
    let output = result
        .output
        .as_ref()
        .expect("ExecResult has neither output nor failure");
    let (decision, bad) = output.decision_or_reject();
    if bad.is_some() {
        (RunOutcome::Crash, crate::HookDecision::Reject, bad)
    } else {
        (RunOutcome::Ok, decision, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::HandlerTarget;
    use crate::{DocEvent, DocType, HandlerContext};
    use std::io::{Read, Write};

    fn input() -> HandlerInput {
        HandlerInput {
            event_id: "ToolCall:tc-1:before_execute:1".to_string(),
            entity: DocType::ToolCall.as_str().to_string(),
            event: DocEvent::BeforeExecute.as_str().to_string(),
            doc: serde_json::json!({"id": "tc-1", "tool": "write_file"}),
            context: HandlerContext {
                actor: "human:dev-1".to_string(),
                depth: 0,
                dry_run: true,
            },
        }
    }

    /// A shell one-liner that echoes its stdin wrapped as a decision.
    /// Uses `sh` (always on PATH on unix test hosts).
    fn echo_cmd(decision_json: &str) -> HandlerTarget {
        HandlerTarget::Command(vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("cat > /dev/null; printf '%s' '{decision_json}'"),
        ])
    }

    #[test]
    fn webhook_localhost_round_trip() {
        // A real TCP server on 127.0.0.1: proves the localhost-only
        // webhook path delivers and parses the JSON contract.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            // Read the request (headers + body); one read is enough for
            // a small test payload.
            let mut buf = vec![0u8; 65536];
            let n = stream.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            assert!(req.starts_with("POST /hook HTTP/1.0"));
            assert!(req.contains("X-Supercli-Event-Id: ToolCall:tc-1:before_execute:1"));
            let body = r#"{"decision":"escalate","message":"webhook says ask"}"#;
            let resp = format!(
                "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(resp.as_bytes()).unwrap();
        });
        let target = HandlerTarget::Webhook(format!("http://localhost:{port}/hook"));
        let r = execute(&target, &input(), 5000);
        server.join().unwrap();
        assert!(r.failure.is_none(), "failure: {:?}", r.failure);
        let (outcome, decision, _) = classify_exec(&r);
        assert_eq!(outcome, RunOutcome::Ok);
        assert_eq!(decision, crate::HookDecision::Escalate);
        assert_eq!(r.output.unwrap().message, "webhook says ask");
    }

    #[test]
    fn webhook_non_localhost_is_transport_error() {
        // Registry would have rejected this; the executor fails closed too.
        let target = HandlerTarget::Webhook("http://example.com:9/hook".to_string());
        let r = execute(&target, &input(), 1000);
        let (outcome, decision, reason) = classify_exec(&r);
        assert_eq!(outcome, RunOutcome::Crash);
        assert_eq!(decision, crate::HookDecision::Reject);
        assert!(reason.unwrap().contains("hook_crash"));
    }

    #[test]
    fn command_allow_round_trip() {
        let r = execute(
            &echo_cmd(r#"{"decision":"allow","patch":{"note":"x"},"message":""}"#),
            &input(),
            2000,
        );
        assert!(r.failure.is_none());
        let out = r.output.unwrap();
        assert_eq!(out.decision, "allow");
        assert_eq!(out.patch["note"], "x");
    }

    #[test]
    fn command_unknown_decision_fails_closed() {
        let r = execute(&echo_cmd(r#"{"decision":"maybe"}"#), &input(), 2000);
        assert!(r.failure.is_none()); // transport fine; classification rejects
        let (outcome, decision, reason) = classify_exec(&r);
        assert_eq!(outcome, RunOutcome::Crash);
        assert_eq!(decision, crate::HookDecision::Reject);
        assert_eq!(reason, Some("hook_bad_output"));
    }

    #[test]
    fn command_bad_json_fails_closed() {
        let r = execute(&echo_cmd("not json at all"), &input(), 2000);
        assert!(matches!(r.failure, Some(ExecFailure::BadOutput(_))));
        let (outcome, decision, reason) = classify_exec(&r);
        assert_eq!(outcome, RunOutcome::Crash);
        assert_eq!(decision, crate::HookDecision::Reject);
        assert_eq!(reason, Some("hook_bad_output"));
    }

    #[test]
    fn command_timeout_fails_closed() {
        let target = HandlerTarget::Command(vec![
            "sh".to_string(),
            "-c".to_string(),
            "sleep 5; echo '{\"decision\":\"allow\"}'".to_string(),
        ]);
        let r = execute(&target, &input(), 200);
        assert!(matches!(r.failure, Some(ExecFailure::Timeout)));
        let (outcome, decision, reason) = classify_exec(&r);
        assert_eq!(outcome, RunOutcome::Timeout);
        assert_eq!(decision, crate::HookDecision::Reject);
        assert_eq!(reason, Some("hook_timeout"));
    }

    #[test]
    fn command_missing_binary_fails_closed() {
        let target = HandlerTarget::Command(vec!["/nonexistent-binary-xyz".to_string()]);
        let r = execute(&target, &input(), 2000);
        assert!(matches!(r.failure, Some(ExecFailure::BadOutput(_))));
        let (_, decision, _) = classify_exec(&r);
        assert_eq!(decision, crate::HookDecision::Reject);
    }
}
