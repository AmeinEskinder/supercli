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
fn opencode_plugin_tracks_busy_idle_and_permission_events() {
    assert!(OPENCODE_PLUGIN_SCRIPT.contains("session.status"));
    assert!(OPENCODE_PLUGIN_SCRIPT.contains("permission.ask"));
    assert!(OPENCODE_PLUGIN_SCRIPT.contains("notify('Start', sessionID)"));
    assert!(OPENCODE_PLUGIN_SCRIPT.contains("notify(stopEvent, sessionID)"));
    assert!(OPENCODE_PLUGIN_SCRIPT.contains("notify('PermissionRequest', rootSessionID)"));
    assert!(OPENCODE_PLUGIN_SCRIPT.contains("session_id: sessionID"));
}

#[test]
#[ignore = "runs real OpenCode CLI against live auth"]
fn live_opencode_run_emits_stop() {
    if real_command_path("opencode").is_none() {
        return;
    }

    crate::hook_assets::install_opencode_plugin().expect("install opencode plugin");
    let capture = CaptureServer::start();

    let mut command = Command::new("opencode");
    command
        .current_dir(repo_root())
        .env("SUPERCLI_APP_PORT", capture.port.to_string())
        .env("SUPERCLI_SESSION_ID", "live-opencode")
        .env(
            "OPENCODE_CONFIG_DIR",
            crate::hook_assets::opencode_config_dir().to_string_lossy().to_string(),
        )
        .arg("run")
        .arg("--dir")
        .arg(repo_root())
        .arg("--format")
        .arg("json")
        .arg("Reply with exactly OK and nothing else.");

    let status =
        run_with_timeout(command, Duration::from_secs(180)).expect("opencode live smoke");
    assert!(status.success(), "opencode exited unsuccessfully: {status}");
    assert!(
        capture.wait_for_event("Start", Duration::from_secs(10)),
        "expected Start event, got {:?}",
        capture.event_names()
    );
    assert!(
        capture.wait_for_event("Stop", Duration::from_secs(10)),
        "expected Stop event, got {:?}",
        capture.event_names()
    );
}
