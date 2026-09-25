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
fn grok_hook_script_carries_provider_session_and_tool_for_attention_events() {
    let capture = CaptureServer::start();
    let script = write_temp_hook_script("grok-attention", crate::hook_assets::GROK_HOOK_SCRIPT);

    let mut child = Command::new("bash")
        .arg(&script)
        .arg("Attention")
        .env("HOME", hook_env_home("grok-attention"))
        .env("SUPERCLI_APP_PORT", capture.port.to_string())
        .env("SUPERCLI_SESSION_ID", "supercli-route-session")
        .env("GROK_SESSION_ID", "grok-provider-session")
        .env("SUPERCLI_HOOK_TRACE_FILE", hook_trace_file("grok-attention"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn grok hook");
    child
        .stdin
        .take()
        .expect("grok hook stdin")
        .write_all(br#"{"toolName":"shell","notificationType":"approval_required"}"#)
        .expect("write grok hook stdin");
    let output = child.wait_with_output().expect("wait for grok hook");

    assert!(output.status.success(), "grok hook failed: {output:?}");
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
        Some("grok-provider-session")
    );
    assert_eq!(
        event.get("tool_name").and_then(Value::as_str),
        Some("shell")
    );
}

#[test]
fn grok_hook_script_forwards_user_prompt_submit_as_busy_event() {
    let capture = CaptureServer::start();
    let script = write_temp_hook_script("grok-user-prompt-submit", crate::hook_assets::GROK_HOOK_SCRIPT);

    let output = run_hook_script_with_stdin(
        &script,
        &["UserPromptSubmit"],
        "{}",
        &capture,
        "grok-user-prompt-submit",
    );

    assert!(output.status.success(), "grok hook failed: {output:?}");
    capture
        .wait_for_event_payload("UserPromptSubmit", Duration::from_secs(5))
        .unwrap_or_else(|| {
            panic!(
                "expected UserPromptSubmit event, got {:?}",
                capture.events_snapshot()
            )
        });
}

#[test]
fn grok_hook_script_posts_through_port_registry_without_app_port() {
    let capture = CaptureServer::start();
    let script = write_temp_hook_script("grok-registry-only", crate::hook_assets::GROK_HOOK_SCRIPT);
    let registry_dir = temp_path("grok-registry-only-ports");
    let registry = registry_dir.join("app-ports");
    fs::write(&registry, format!("{}\n", capture.port)).expect("write port registry");

    let mut command = Command::new("bash");
    command
        .arg(&script)
        .arg("UserPromptSubmit")
        .env("HOME", hook_env_home("grok-registry-only"))
        .env_remove("SUPERCLI_APP_PORT")
        .env("SUPERCLI_SESSION_ID", "supercli-route-session")
        .env("SUPERCLI_APP_PORT_REGISTRY_FILE", &registry)
        .env(
            "SUPERCLI_HOOK_TRACE_FILE",
            hook_trace_file("grok-registry-only"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn grok hook");
    child
        .stdin
        .take()
        .expect("grok hook stdin")
        .write_all(b"{}")
        .expect("write grok hook stdin");
    let output = child.wait_with_output().expect("wait for grok hook");

    assert!(output.status.success(), "grok hook failed: {output:?}");
    capture
        .wait_for_event_payload("UserPromptSubmit", Duration::from_secs(5))
        .unwrap_or_else(|| {
            panic!(
                "expected UserPromptSubmit via app-ports, got {:?}",
                capture.events_snapshot()
            )
        });
}

#[test]
fn grok_native_cancel_and_failure_keep_their_event_identity() {
    let script = write_temp_hook_script("grok-turn-end", crate::hook_assets::GROK_HOOK_SCRIPT);
    let hooks: Value = serde_json::from_str(&crate::hook_assets::grok_hooks_json(&script).unwrap()).unwrap();
    for event in ["StopCancelled", "StopFailure"] {
        let command = hooks["hooks"][event][0]["hooks"][0]["command"]
            .as_str()
            .expect("native turn-end registration");
        let argument = command
            .strip_prefix(&format!("{} ", script.display()))
            .unwrap();
        let session = temp_path(&format!("grok-{event}"));
        let output = run_hook_path_recording(&script, event, &[argument], "{}", &session);
        assert!(output.status.success(), "{output:?}");
        let recorded = read_last_hook_event(&session);
        assert_eq!(recorded["hook_event_name"], event);
        assert_eq!(recorded["supercli_runtime_generation"], 7);
    }
}
