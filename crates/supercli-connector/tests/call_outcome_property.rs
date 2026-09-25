//! Phase 9 H1 — exhaustive property test for the `CallOutcome` classifier
//! (`LinkError::call_outcome`).
//!
//! The safety property (Osman, Phase 9): **never `DefiniteFailed` when
//! bytes may have left the Host.** `DefiniteFailed` is reserved for two
//! cases only:
//! 1. explicit rejections — the far side answered "no" (MCP error,
//!    unknown tool, tool not declared, 400/401/403/404/409/422);
//! 2. provably-pre-call failures — spawn / handshake / missing URL, which
//!    by construction happen before any tool-call bytes exist.
//!
//! Everything else — 5xx, 408, 425, 429, other statuses, timeouts,
//! mid-call I/O death, unparseable replies, and any transport error once
//! `request_sent` is true — is `Ambiguous`: persisted, never auto-retried,
//! replaceable only by explicit human approval.
//!
//! This test enumerates every `LinkError` variant × `request_sent` and
//! pins the property, so a future arm added to the classifier cannot
//! silently reintroduce an unsafe `DefiniteFailed`.

use supercli_connector::link::{CallOutcome, LinkError};
use supercli_connector::manifest::ConnectorKind;
use supercli_connector::{ConnectorError, HttpConnectorError};

fn stdio_variants() -> Vec<ConnectorError> {
    vec![
        ConnectorError::Spawn("boom".into()),
        ConnectorError::UnsupportedKind(ConnectorKind::Builtin),
        ConnectorError::Handshake("bad hello".into()),
        ConnectorError::ToolNotDeclared("evil.tool".into()),
        ConnectorError::UnknownTool("nope".into()),
        ConnectorError::Mcp {
            code: -32601,
            message: "method not found".into(),
        },
        ConnectorError::Mcp {
            code: -32603,
            message: "".into(),
        },
        ConnectorError::Protocol("garbage".into()),
        ConnectorError::Io("eof".into()),
        ConnectorError::Timeout,
    ]
}

fn http_variants() -> Vec<HttpConnectorError> {
    let mut v = vec![
        HttpConnectorError::TransportSetup("dns".into()),
        HttpConnectorError::TransportMidCall("reset".into()),
        HttpConnectorError::Handshake("bad hello".into()),
        HttpConnectorError::ToolNotDeclared("evil.tool".into()),
        HttpConnectorError::UnknownTool("nope".into()),
        HttpConnectorError::Mcp {
            code: -32602,
            message: "invalid params".into(),
        },
        HttpConnectorError::Protocol("bad json".into()),
        HttpConnectorError::NoUrl,
    ];
    for code in [
        200u16, 301, 400, 401, 403, 404, 408, 409, 422, 425, 429, 500, 502, 503, 599,
    ] {
        v.push(HttpConnectorError::Status(code, "s".into()));
    }
    v
}

fn all_errors() -> Vec<(&'static str, LinkError)> {
    let mut out: Vec<(&'static str, LinkError)> = Vec::new();
    for e in stdio_variants() {
        let name: &'static str = match &e {
            ConnectorError::Spawn(_) => "Stdio::Spawn",
            ConnectorError::UnsupportedKind(_) => "Stdio::UnsupportedKind",
            ConnectorError::Handshake(_) => "Stdio::Handshake",
            ConnectorError::ToolNotDeclared(_) => "Stdio::ToolNotDeclared",
            ConnectorError::UnknownTool(_) => "Stdio::UnknownTool",
            ConnectorError::Mcp { .. } => "Stdio::Mcp",
            ConnectorError::Protocol(_) => "Stdio::Protocol",
            ConnectorError::Io(_) => "Stdio::Io",
            ConnectorError::Timeout => "Stdio::Timeout",
        };
        out.push((name, LinkError::Stdio(e)));
    }
    for e in http_variants() {
        let name: &'static str = match &e {
            HttpConnectorError::TransportSetup(_) => "Http::TransportSetup",
            HttpConnectorError::TransportMidCall(_) => "Http::TransportMidCall",
            HttpConnectorError::Status(c, _) => match c {
                400 | 401 | 403 | 404 | 409 | 422 => "Http::Status(reject)",
                _ => "Http::Status(other)",
            },
            HttpConnectorError::Handshake(_) => "Http::Handshake",
            HttpConnectorError::ToolNotDeclared(_) => "Http::ToolNotDeclared",
            HttpConnectorError::UnknownTool(_) => "Http::UnknownTool",
            HttpConnectorError::Mcp { .. } => "Http::Mcp",
            HttpConnectorError::Protocol(_) => "Http::Protocol",
            HttpConnectorError::NoUrl => "Http::NoUrl",
        };
        out.push((name, LinkError::Http(e)));
    }
    out.push(("LinkError::NoUrl", LinkError::NoUrl("c".into())));
    out.push(("LinkError::Io", LinkError::Io("eof".into())));
    out
}

/// Variants allowed to classify as `DefiniteFailed`: explicit rejections
/// (the far side answered "no"), provably-pre-call failures (spawn,
/// handshake, missing URL — before any call bytes can exist), and a
/// transport-setup failure observed *before* any byte was sent.
fn may_be_definite_failed(name: &str, request_sent: bool) -> bool {
    matches!(
        name,
        "Stdio::Mcp"
            | "Http::Mcp"
            | "Stdio::UnknownTool"
            | "Http::UnknownTool"
            | "Stdio::ToolNotDeclared"
            | "Http::ToolNotDeclared"
            | "Http::Status(reject)"
            | "Stdio::Spawn"
            | "Stdio::UnsupportedKind"
            | "Stdio::Handshake"
            | "Http::Handshake"
            | "Http::NoUrl"
            | "LinkError::NoUrl"
    ) || (name == "Http::TransportSetup" && !request_sent)
}

#[test]
fn call_outcome_never_definite_failed_when_bytes_may_have_left() {
    let mut checked = 0;
    for (name, err) in all_errors() {
        for request_sent in [false, true] {
            let outcome = err.call_outcome(request_sent);
            // The classifier only ever classifies failures.
            assert_ne!(
                outcome,
                CallOutcome::DefiniteOk,
                "{name} sent={request_sent}: failure classified DefiniteOk"
            );
            if outcome == CallOutcome::DefiniteFailed {
                assert!(
                    may_be_definite_failed(name, request_sent),
                    "{name} sent={request_sent}: DefiniteFailed outside the \
                     explicit-rejection / provably-pre-call set — bytes may have left"
                );
            }
            // The sharp edge: once bytes were sent, transport-class and
            // non-rejection errors must fail safe to Ambiguous.
            if request_sent
                && matches!(
                    name,
                    "Http::TransportSetup"
                        | "Http::TransportMidCall"
                        | "Http::Status(other)"
                        | "Stdio::Protocol"
                        | "Http::Protocol"
                        | "Stdio::Io"
                        | "LinkError::Io"
                        | "Stdio::Timeout"
                )
            {
                assert_eq!(
                    outcome,
                    CallOutcome::Ambiguous,
                    "{name} sent=true: must be Ambiguous, got {outcome:?}"
                );
            }
            checked += 1;
        }
    }
    eprintln!("call_outcome property: {checked} variant x sent combinations checked");
}

#[test]
fn call_outcome_pre_send_transport_failure_is_definite() {
    // The fail-safe direction: a DNS/connect/TLS failure with no bytes
    // sent provably never ran.
    let err = LinkError::Http(HttpConnectorError::TransportSetup("dns".into()));
    assert_eq!(
        err.call_outcome(false),
        CallOutcome::DefiniteFailed,
        "pre-send transport failure must stay DefiniteFailed"
    );
    // ...but the same error class after bytes were sent fails safe.
    assert_eq!(
        err.call_outcome(true),
        CallOutcome::Ambiguous,
        "post-send transport error must be Ambiguous"
    );
}
