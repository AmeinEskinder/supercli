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
fn gemini_hook_script_preserves_provider_metadata_for_lifecycle_events() {
    let capture = CaptureServer::start();
    let script = write_temp_hook_script("gemini-metadata", crate::hook_assets::GEMINI_HOOK_SCRIPT);
    let payload = json!({
        "hook_event_name": "BeforeAgent",
        "session_id": "gemini-provider-session",
        "conversationId": "gemini-conversation",
        "transcript_path": r#"/tmp/a "quoted" \ transcript.jsonl"#,
        "tool_name": "gemini"
    })
    .to_string();

    let output =
        run_hook_script_with_stdin(&script, &[], &payload, &capture, "gemini-metadata");

    assert!(output.status.success(), "gemini hook failed: {output:?}");
    let event = capture
        .wait_for_event_payload("UserPromptSubmit", Duration::from_secs(5))
        .unwrap_or_else(|| {
            panic!(
                "expected UserPromptSubmit event, got {:?}",
                capture.events_snapshot()
            )
        });
    assert_eq!(
        event.get("session_id").and_then(Value::as_str),
        Some("gemini-provider-session")
    );
    assert_eq!(
        event.get("conversationId").and_then(Value::as_str),
        Some("gemini-conversation")
    );
    assert_eq!(
        event.get("transcript_path").and_then(Value::as_str),
        Some(r#"/tmp/a "quoted" \ transcript.jsonl"#)
    );
    assert_eq!(
        event.get("tool_name").and_then(Value::as_str),
        Some("gemini")
    );
}

#[test]
fn gemini_hook_script_maps_agent_events() {
    assert!(GEMINI_HOOK_SCRIPT.contains("BeforeAgent"));
    assert!(GEMINI_HOOK_SCRIPT.contains("AfterAgent"));
    assert!(GEMINI_HOOK_SCRIPT.contains("AfterTool"));
    assert!(GEMINI_HOOK_SCRIPT.contains("Notification"));
    assert!(GEMINI_HOOK_SCRIPT.contains("EVENT_TYPE=\"UserPromptSubmit\""));
    assert!(GEMINI_HOOK_SCRIPT.contains("EVENT_TYPE=\"Stop\""));
    assert!(GEMINI_HOOK_SCRIPT.contains("EVENT_TYPE=\"PermissionRequest\""));
    assert!(GEMINI_HOOK_SCRIPT.contains("UNPEEL_APP_PORT"));
}
