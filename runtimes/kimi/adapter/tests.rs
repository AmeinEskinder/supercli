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
fn kimi_config_reconciliation_preserves_user_hooks_and_replaces_managed_hooks() {
    let raw = r#"
theme = "light"

[[hooks]]
event = "PostToolUse"
matcher = "WriteFile"
command = "prettier"

[[hooks]]
event = "Stop"
command = "\"${SUPERCLI_HOME:-$HOME/.unpeel}/hooks/kimi-hook.sh\" old"
timeout = 5
"#;
    let updated = crate::hook_assets::reconcile_kimi_config(raw, true).expect("reconcile Kimi Code config");
    let parsed = updated.parse::<toml::Value>().expect("valid TOML");
    assert_eq!(
        parsed.get("theme").and_then(toml::Value::as_str),
        Some("light")
    );
    assert!(updated.contains("command = \"prettier\""));
    assert!(!updated.contains("kimi-hook.sh\\\" old"));
    assert_eq!(updated.matches("[[hooks]]").count(), 10);
    assert_eq!(
        updated
            .matches("${SUPERCLI_HOME:-$HOME/.unpeel}/hooks/kimi-hook.sh")
            .count(),
        9
    );
    assert!(updated.contains("event = \"SessionStart\""));
    assert!(updated.contains("event = \"PermissionRequest\""));
    assert!(updated.contains("event = \"Interrupt\""));
    assert!(updated.contains("matcher = \"permission_prompt\""));
    assert!(updated.contains("matcher = \"ask_user_question|AskUserQuestion\""));
}

#[test]
fn legacy_kimi_config_omits_kimi_code_only_hook_events() {
    let updated = crate::hook_assets::reconcile_kimi_config("", false).expect("reconcile legacy Kimi");
    assert_eq!(updated.matches("[[hooks]]").count(), 7);
    assert!(!updated.contains("event = \"PermissionRequest\""));
    assert!(!updated.contains("event = \"Interrupt\""));
}

#[test]
fn kimi_code_mcp_entries_preserve_user_name_collisions() {
    let mut servers = serde_json::Map::new();
    servers.insert(
        "unpeel-sessions".to_string(),
        json!({"command":"user-owned-server"}),
    );
    crate::hook_assets::upsert_kimi_code_managed_mcp(
        &mut servers,
        "unpeel-sessions",
        crate::mcp_gate::SESSIONS_KIND,
        json!({"command":"/tmp/home/.unpeel/bin/unpeel-mcp", "args":[]}),
    );
    assert_eq!(
        servers["unpeel-sessions"]["command"],
        Value::String("user-owned-server".to_string())
    );
    assert_eq!(
        servers["unpeel-sessions-unpeel"]["command"],
        "/tmp/home/.unpeel/bin/unpeel-mcp"
    );
    // Legacy gate entries written by older builds still count as ours.
    servers.insert(
        "unpeel".to_string(),
        json!({"command":"/tmp/unpeel-host",
               "args":[crate::mcp_gate::MCP_GATE_ARG, crate::mcp_gate::UNIFIED_KIND]}),
    );
    crate::hook_assets::upsert_kimi_code_managed_mcp(
        &mut servers,
        "unpeel",
        crate::mcp_gate::UNIFIED_KIND,
        json!({"command":"/tmp/home/.unpeel/bin/unpeel-mcp", "args":[]}),
    );
    assert_eq!(servers["unpeel"]["command"], "/tmp/home/.unpeel/bin/unpeel-mcp");
}

#[test]
fn kimi_hook_forwards_provider_session_and_attention_metadata() {
    let capture = CaptureServer::start();
    let script = write_temp_hook_script("kimi-metadata", KIMI_HOOK_SCRIPT);
    let payload = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "kimi-provider-session",
        "cwd": "/tmp/kimi-project",
        "tool_name": "AskUserQuestion"
    })
    .to_string();
    let output = run_hook_script_with_stdin(
        &script,
        &["Attention"],
        &payload,
        &capture,
        "kimi-metadata",
    );
    assert!(output.status.success(), "Kimi hook failed: {output:?}");
    let event = capture
        .wait_for_event_payload("PermissionRequest", Duration::from_secs(3))
        .expect("Kimi hook event");
    assert_eq!(
        event.get("session_id").and_then(Value::as_str),
        Some("kimi-provider-session")
    );
    assert_eq!(
        event.get("tool_name").and_then(Value::as_str),
        Some("AskUserQuestion")
    );
}
