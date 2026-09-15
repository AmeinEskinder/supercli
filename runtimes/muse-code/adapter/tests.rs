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
fn muse_plugin_manifest_registers_each_event_with_a_distinct_source() {
    let manifest: Value =
        serde_json::from_str(&crate::hook_assets::muse_plugin_manifest_json().expect("manifest"))
            .expect("parse manifest");
    let hooks = manifest["capabilities"]["hooks"]
        .as_array()
        .expect("hooks array");
    let events: Vec<&str> = hooks
        .iter()
        .map(|hook| hook["event"].as_str().expect("event"))
        .collect();
    assert_eq!(
        events,
        [
            "SessionStart",
            "UserPromptSubmit",
            "Stop",
            "PermissionRequest"
        ]
    );
    // The muse plugin validator rejects two hooks sharing one source path.
    let sources: std::collections::HashSet<&str> = hooks
        .iter()
        .map(|hook| hook["command"][1].as_str().expect("source path"))
        .collect();
    assert_eq!(sources.len(), hooks.len());
    assert_eq!(manifest["name"], "unpeel");
    assert_eq!(manifest["compat"]["manifestDir"], ".muse-plugin");
}

#[test]
fn muse_plugin_manifest_registers_unified_mcp_server() {
    let manifest: Value =
        serde_json::from_str(&crate::hook_assets::muse_plugin_manifest_json().expect("manifest"))
            .expect("parse manifest");
    let servers = manifest["capabilities"]["mcpServers"]
        .as_array()
        .expect("mcpServers array");
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["id"], "unpeel");
    let command = servers[0]["command"]
        .as_array()
        .expect("command array");
    assert_eq!(command.len(), 1);
    assert!(crate::integrations::install::is_mcp_shim_command(
        command[0].as_str().expect("shim path")
    ));
}

#[test]
fn muse_hook_script_records_stop_but_not_session_start() {
    let session_dir = temp_path("muse-record-session");
    let stop_payload = json!({
        "hook_event_name": "Stop",
        "session_id": "muse-provider-1",
        "last_assistant_message": "done"
    })
    .to_string();
    let output = run_hook_script_recording(
        crate::hook_assets::MUSE_HOOK_SCRIPT,
        "muse-record",
        &[],
        &stop_payload,
        &session_dir,
    );
    assert!(output.status.success(), "muse hook failed: {output:?}");
    let event = read_last_hook_event(&session_dir);
    assert_eq!(
        event.get("hook_event_name").and_then(Value::as_str),
        Some("Stop")
    );

    // SessionStart only latches metadata; it must not overwrite the seed.
    let start_payload = json!({
        "hook_event_name": "SessionStart",
        "session_id": "muse-provider-1",
        "source": "startup"
    })
    .to_string();
    let output = run_hook_script_recording(
        crate::hook_assets::MUSE_HOOK_SCRIPT,
        "muse-record-start",
        &[],
        &start_payload,
        &session_dir,
    );
    assert!(output.status.success(), "muse hook failed: {output:?}");
    let event = read_last_hook_event(&session_dir);
    assert_eq!(
        event.get("hook_event_name").and_then(Value::as_str),
        Some("Stop")
    );
}

#[test]
fn muse_hook_script_drops_internal_reminder_permission_requests() {
    // Muse fires PermissionRequest for its internal reminder-decision
    // tool; the decision is auto-answered inside muse (no user prompt)
    // and can land after Stop, which used to latch attention forever.
    let session_dir = temp_path("muse-reminder-session");
    let stop_payload = json!({
        "hook_event_name": "Stop",
        "session_id": "muse-provider-2"
    })
    .to_string();
    let output = run_hook_script_recording(
        crate::hook_assets::MUSE_HOOK_SCRIPT,
        "muse-reminder-stop",
        &[],
        &stop_payload,
        &session_dir,
    );
    assert!(output.status.success(), "muse hook failed: {output:?}");

    let reminder_payload = json!({
        "hook_event_name": "PermissionRequest",
        "tool_name": "submit_reminder_decision",
        "session_id": "muse-provider-2"
    })
    .to_string();
    let output = run_hook_script_recording(
        crate::hook_assets::MUSE_HOOK_SCRIPT,
        "muse-reminder-perm",
        &[],
        &reminder_payload,
        &session_dir,
    );
    assert!(output.status.success(), "muse hook failed: {output:?}");
    let event = read_last_hook_event(&session_dir);
    assert_eq!(
        event.get("hook_event_name").and_then(Value::as_str),
        Some("Stop")
    );

    // A genuine tool approval must still record attention.
    let bash_payload = json!({
        "hook_event_name": "PermissionRequest",
        "tool_name": "bash",
        "session_id": "muse-provider-2"
    })
    .to_string();
    let output = run_hook_script_recording(
        crate::hook_assets::MUSE_HOOK_SCRIPT,
        "muse-reminder-bash",
        &[],
        &bash_payload,
        &session_dir,
    );
    assert!(output.status.success(), "muse hook failed: {output:?}");
    let event = read_last_hook_event(&session_dir);
    assert_eq!(
        event.get("hook_event_name").and_then(Value::as_str),
        Some("PermissionRequest")
    );
}

#[test]
fn muse_hook_script_drops_yolo_permission_requests() {
    // --yolo disables approval entirely, so a PermissionRequest from a
    // yolo launch can never be a user-facing prompt; forwarding it would
    // latch attention with nothing to clear it. The parent command is
    // overridden here through the script's test seam instead of ps.
    let session_dir = temp_path("muse-yolo-session");
    let stop_payload = json!({
        "hook_event_name": "Stop",
        "session_id": "muse-provider-3"
    })
    .to_string();
    let output = run_hook_script_recording(
        crate::hook_assets::MUSE_HOOK_SCRIPT,
        "muse-yolo-stop",
        &[],
        &stop_payload,
        &session_dir,
    );
    assert!(output.status.success(), "muse hook failed: {output:?}");

    let perm_payload = json!({
        "hook_event_name": "PermissionRequest",
        "tool_name": "bash",
        "session_id": "muse-provider-3"
    })
    .to_string();
    let run_with_parent = |label: &str, parent_command: &str, port: u16| {
        let script = write_temp_hook_script(label, crate::hook_assets::MUSE_HOOK_SCRIPT);
        let mut command = Command::new("bash");
        command
            .arg(&script)
            .env("HOME", hook_env_home(label))
            .env("UNPEEL_APP_PORT", port.to_string())
            .env("UNPEEL_SESSION_ID", "unpeel-record-session")
            .env("UNPEEL_SESSION_DIR", &session_dir)
            .env("UNPEEL_RUNTIME_GENERATION", "7")
            .env("UNPEEL_MUSE_PARENT_COMMAND_OVERRIDE", parent_command)
            .env("UNPEEL_HOOK_TRACE_FILE", hook_trace_file(label))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("spawn muse hook");
        child
            .stdin
            .take()
            .expect("hook stdin")
            .write_all(perm_payload.as_bytes())
            .expect("write hook stdin");
        child.wait_with_output().expect("wait for muse hook")
    };

    // Yolo launch: dropped — the Stop seed is untouched and the owner
    // receives nothing (delivery is synchronous, so an empty snapshot
    // after exit is conclusive, not a race).
    let capture = CaptureServer::start();
    let output = run_with_parent("muse-yolo-perm", "muse --yolo", capture.port);
    assert!(output.status.success(), "muse hook failed: {output:?}");
    assert!(
        capture.events_snapshot().is_empty(),
        "yolo PermissionRequest must not be posted, got {:?}",
        capture.events_snapshot()
    );
    let event = read_last_hook_event(&session_dir);
    assert_eq!(
        event.get("hook_event_name").and_then(Value::as_str),
        Some("Stop")
    );

    // Ordinary launch: forwarded — the owner receives the event and the
    // seed records the attention transition with its tool.
    let capture = CaptureServer::start();
    let output = run_with_parent("muse-non-yolo-perm", "muse resume abc", capture.port);
    assert!(output.status.success(), "muse hook failed: {output:?}");
    let posted = capture
        .wait_for_event_payload("PermissionRequest", Duration::from_secs(5))
        .unwrap_or_else(|| {
            panic!(
                "expected PermissionRequest event, got {:?}",
                capture.events_snapshot()
            )
        });
    assert_eq!(
        posted.get("tool_name").and_then(Value::as_str),
        Some("bash")
    );
    let event = read_last_hook_event(&session_dir);
    assert_eq!(
        event.get("hook_event_name").and_then(Value::as_str),
        Some("PermissionRequest")
    );
    assert_eq!(event.get("tool_name").and_then(Value::as_str), Some("bash"));
}
