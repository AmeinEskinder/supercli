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
fn cursor_hook_script_preserves_metadata_for_permission_events() {
    let capture = CaptureServer::start();
    let script = write_temp_hook_script("cursor-metadata", crate::hook_assets::CURSOR_HOOK_SCRIPT);
    let payload = json!({
        "session_id": "cursor-provider-session",
        "providerTranscriptPath": "/tmp/cursor-provider-session.jsonl",
        "tool_name": "terminal"
    })
    .to_string();

    let output = run_hook_script_with_stdin(
        &script,
        &["PermissionRequest"],
        &payload,
        &capture,
        "cursor-metadata",
    );

    assert!(output.status.success(), "cursor hook failed: {output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("\"continue\":true"),
        "cursor hook should auto-confirm permission request: {output:?}"
    );
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
        Some("cursor-provider-session")
    );
    assert_eq!(
        event.get("providerTranscriptPath").and_then(Value::as_str),
        Some("/tmp/cursor-provider-session.jsonl")
    );
    assert_eq!(
        event.get("tool_name").and_then(Value::as_str),
        Some("terminal")
    );
}

#[test]
fn cursor_hook_script_uses_cursor_conversation_id_for_start_events() {
    let capture = CaptureServer::start();
    let script = write_temp_hook_script("cursor-start-env", crate::hook_assets::CURSOR_HOOK_SCRIPT);

    let mut child = Command::new("bash")
        .arg(&script)
        .arg("Start")
        .env("HOME", hook_env_home("cursor-start-env"))
        .env("UNPEEL_APP_PORT", capture.port.to_string())
        .env("UNPEEL_SESSION_ID", "unpeel-route-session")
        .env("CURSOR_CONVERSATION_ID", "cursor-chat-123")
        .env(
            "UNPEEL_HOOK_TRACE_FILE",
            hook_trace_file("cursor-start-env"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cursor hook");
    child
        .stdin
        .take()
        .expect("cursor hook stdin")
        .write_all(b"{}")
        .expect("write cursor hook stdin");
    let output = child.wait_with_output().expect("wait for cursor hook");

    assert!(output.status.success(), "cursor hook failed: {output:?}");
    let event = capture
        .wait_for_event_payload("Start", Duration::from_secs(5))
        .unwrap_or_else(|| panic!("expected Start event, got {:?}", capture.events_snapshot()));
    assert_eq!(
        event.get("session_id").and_then(Value::as_str),
        Some("cursor-chat-123")
    );
}

#[test]
fn cursor_hook_script_ignores_all_grok_events() {
    let capture = CaptureServer::start();
    let script = write_temp_hook_script("cursor-ignore-grok", crate::hook_assets::CURSOR_HOOK_SCRIPT);
    let mut child = Command::new("bash")
        .arg(&script)
        .arg("Start")
        .env("HOME", hook_env_home("cursor-ignore-grok"))
        .env("UNPEEL_APP_PORT", capture.port.to_string())
        .env("UNPEEL_SESSION_ID", "unpeel-route-session")
        .env("GROK_SESSION_ID", "grok-provider-session")
        .env(
            "UNPEEL_HOOK_TRACE_FILE",
            hook_trace_file("cursor-ignore-grok"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cursor hook");
    child
        .stdin
        .take()
        .expect("cursor hook stdin")
        .write_all(br#"{}"#)
        .expect("write cursor hook stdin");
    let finished = child.wait_with_output().expect("wait for cursor hook");
    assert!(
        finished.status.success(),
        "cursor hook failed: {finished:?}"
    );
    assert!(
        String::from_utf8_lossy(&finished.stdout).contains(r#"{"continue":true}"#),
        "grok-ignored cursor hooks must fail-open, got {:?}",
        String::from_utf8_lossy(&finished.stdout)
    );
    thread::sleep(Duration::from_millis(200));
    assert!(
        capture.events_snapshot().is_empty(),
        "Grok must not inherit Cursor Start, got {:?}",
        capture.events_snapshot()
    );
}

#[test]
fn cursor_hook_script_auto_confirms_permission_requests() {
    assert!(CURSOR_HOOK_SCRIPT.contains("PermissionRequest"));
    assert!(CURSOR_HOOK_SCRIPT.contains("{\"continue\":true}"));
    assert!(CURSOR_HOOK_SCRIPT.contains("UNPEEL_APP_PORT"));
    assert!(CURSOR_HOOK_SCRIPT.contains("UNPEEL_PORT_REGISTRY_FILE"));
    assert!(CURSOR_HOOK_SCRIPT.contains("post_hook_event_to_current_ports"));
    assert!(CURSOR_HOOK_SCRIPT.contains("GROK_SESSION_ID"));
    assert!(CURSOR_HOOK_SCRIPT.contains("\"session_id\""));
    assert!(crate::hook_assets::GROK_HOOK_SCRIPT.contains("Attention"));
    let grok_hooks = crate::hook_assets::grok_hooks_json(&PathBuf::from("/tmp/grok-hook.sh"))
        .expect("serialize grok hooks");
    assert!(grok_hooks.contains("approval_required"));
    assert!(grok_hooks.contains("\"command\": \"/tmp/grok-hook.sh HookSeen\""));
    assert!(grok_hooks.contains("\"command\": \"/tmp/grok-hook.sh UserPromptSubmit\""));
    assert!(!grok_hooks.contains("\"command\": \"/tmp/grok-hook.sh Start\""));
    assert!(crate::hook_assets::GROK_HOOK_SCRIPT.contains("tool_name"));
}

#[test]
fn cursor_mcp_config_merges_and_prunes_unpeel_servers() {
    let home = hook_env_home("cursor-mcp-config");
    let mcp_path = home.join(".cursor").join("mcp.json");
    fs::create_dir_all(mcp_path.parent().unwrap()).expect("create cursor dir");
    // Seed with a user server plus every legacy Unpeel entry generation
    // (pre-rename unified `unpeel-mcp` and the per-domain pair): the merge
    // must adopt/replace with one `unpeel` entry and prune all legacy
    // names while leaving user servers alone.
    fs::write(
        &mcp_path,
        r#"{
  "mcpServers": {
"GmailCompany": { "url": "https://example.com/mcp" },
"unpeel-mcp": { "type": "stdio", "command": "/stale", "args": ["__mcp__"] },
"unpeel-sessions": { "type": "stdio", "command": "/stale", "args": ["__mcp__"] },
"unpeel-browser": { "type": "stdio", "command": "/stale", "args": ["__browser_mcp__"] }
  }
}
"#,
    )
    .expect("seed cursor mcp.json");

    crate::hook_assets::merge_cursor_mcp_servers_at(
        Some(&mcp_path),
        [
            (
                "unpeel",
                Some(json!({
                    "type": "stdio",
                    "command": "/Applications/Unpeel.app/Contents/MacOS/unpeel-host",
                    "args": ["__mcp__"],
                })),
            ),
            ("unpeel-mcp", None),
            ("unpeel-sessions", None),
            ("unpeel-browser", None),
        ],
    )
    .expect("merge cursor mcp.json");

    let merged: Value =
        serde_json::from_str(&fs::read_to_string(&mcp_path).expect("read mcp.json"))
            .expect("parse mcp.json");
    let servers = merged["mcpServers"].as_object().expect("mcpServers object");
    assert!(servers.contains_key("GmailCompany"));
    assert_eq!(servers["unpeel"]["args"][0], "__mcp__");
    assert!(!servers.contains_key("unpeel-mcp"));
    assert!(!servers.contains_key("unpeel-sessions"));
    assert!(!servers.contains_key("unpeel-browser"));
}
