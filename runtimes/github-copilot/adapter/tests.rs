use crate::hook_assets::test_support::*;
use crate::hook_assets::*;
use crate::hook_assets::{
    kiro_mcp_server_value, AMP_PLUGIN_SCRIPT, CLAUDE_HOOK_SCRIPT, CLINE_HOOK_SCRIPT,
    CODEX_NOTIFY_NORMALIZER_SCRIPT, COPILOT_HOOK_SCRIPT, CURSOR_HOOK_SCRIPT,
    GEMINI_HOOK_SCRIPT, KIMI_HOOK_SCRIPT, KIRO_HOOK_SCRIPT, NOTIFY_HOOK_SCRIPT,
    OPENCODE_PLUGIN_SCRIPT,
};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn copilot_hook_script_preserves_metadata_for_permission_events() {
    let capture = CaptureServer::start();
    let script = write_temp_hook_script("copilot-metadata", crate::hook_assets::COPILOT_HOOK_SCRIPT);
    let payload = json!({
        "sessionId": "copilot-provider-session",
        "conversationID": "copilot-conversation",
        "transcriptPath": "/tmp/copilot-provider-session.jsonl",
        "toolName": "shell"
    })
    .to_string();

    let output = run_hook_script_with_stdin(
        &script,
        &["preToolUse"],
        &payload,
        &capture,
        "copilot-metadata",
    );

    assert!(output.status.success(), "copilot hook failed: {output:?}");
    let event = capture
        .wait_for_event_payload("PermissionRequest", Duration::from_secs(5))
        .unwrap_or_else(|| {
            panic!(
                "expected PermissionRequest event, got {:?}",
                capture.events_snapshot()
            )
        });
    assert_eq!(
        event.get("session_id").and_then(Value::as_str),
        Some("copilot-provider-session")
    );
    assert_eq!(
        event.get("conversationID").and_then(Value::as_str),
        Some("copilot-conversation")
    );
    assert_eq!(
        event.get("transcriptPath").and_then(Value::as_str),
        Some("/tmp/copilot-provider-session.jsonl")
    );
    assert_eq!(
        event.get("tool_name").and_then(Value::as_str),
        Some("shell")
    );
}

#[test]
fn copilot_hook_script_maps_lifecycle_and_permission_events() {
    assert!(COPILOT_HOOK_SCRIPT.contains("sessionStart"));
    assert!(COPILOT_HOOK_SCRIPT.contains("sessionEnd"));
    assert!(COPILOT_HOOK_SCRIPT.contains("PermissionRequest"));
    assert!(COPILOT_HOOK_SCRIPT.contains("printf '{}\\n'"));
    assert!(COPILOT_HOOK_SCRIPT.contains("SUPERCLI_APP_PORT"));
}
