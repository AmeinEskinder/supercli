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
fn cline_native_hook_forwards_root_session_id_and_busy_edge() {
    let capture = CaptureServer::start();
    let script = write_temp_hook_script("cline-native", CLINE_HOOK_SCRIPT);
    let payload = json!({
        "hookName": "agent_start",
        "taskId": "conv_ephemeral",
        "sessionContext": { "rootSessionId": "cline-persisted-session" },
        "workspaceRoots": ["/tmp/cline-project"]
    })
    .to_string();
    let output =
        run_hook_script_with_stdin(&script, &["TaskStart"], &payload, &capture, "cline-native");
    assert!(output.status.success(), "Cline hook failed: {output:?}");
    let event = capture
        .wait_for_event_payload("UserPromptSubmit", Duration::from_secs(3))
        .expect("Cline hook event");
    assert_eq!(
        event.get("session_id").and_then(Value::as_str),
        Some("cline-persisted-session")
    );
}

#[test]
fn cline_hook_installer_never_overwrites_a_user_hook_slot() {
    let hooks_dir = temp_path("cline-hook-slot");
    let user_hook = hooks_dir.join("TaskStart.bash");
    fs::write(&user_hook, "#!/bin/bash\necho user-owned\n").expect("write user hook");

    crate::hook_assets::write_cline_event_hook(
        &hooks_dir,
        "TaskStart",
        "#!/bin/bash\n# Managed by Unpeel.\necho managed\n",
    )
    .expect("install managed hook");
    assert_eq!(
        fs::read_to_string(&user_hook).expect("read user hook"),
        "#!/bin/bash\necho user-owned\n"
    );
    assert!(fs::read_to_string(hooks_dir.join("TaskStart.zsh"))
        .expect("read managed hook")
        .contains("# Managed by Unpeel."));
}
