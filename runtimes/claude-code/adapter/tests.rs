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
    fn inventory_selects_native_install_or_plain_shell_update() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let dirs = [root.path().to_path_buf()];
        let inventory = || {
            crate::plugins::agents_wire_in_dirs(&dirs)
                .as_array().unwrap().iter()
                .find(|row| row["id"] == "com.anthropic.claude-code")
                .unwrap().clone()
        };
        let missing = inventory();
        assert_eq!(missing["installed"], false);
        assert_eq!(missing["installCommand"], "curl -fsSL https://claude.ai/install.sh | bash");

        let executable = root.path().join("claude");
        std::fs::write(&executable, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let installed = inventory();
        assert_eq!(installed["installed"], true);
        let update = installed["installCommand"].as_str().unwrap();
        // The updater must not resolve to a managed Claude runtime or run
        // npm over an existing native installation.
        assert!(crate::integrations::runtime_for_command(update).is_none());
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", update])
            .env("PATH", root.path())
            .env("SUPERCLI_HOME", root.path().join("state"))
            .output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "update\n");
    }

#[test]
fn claude_hook_script_records_last_hook_event() {
    let session_dir = temp_path("claude-record-session");
    let payload = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "provider-1"
    })
    .to_string();
    let output = run_hook_script_recording(
        crate::hook_assets::CLAUDE_HOOK_SCRIPT,
        "claude-record",
        &[],
        &payload,
        &session_dir,
    );
    assert!(output.status.success(), "claude hook failed: {output:?}");
    let event = read_last_hook_event(&session_dir);
    assert_eq!(
        event.get("hook_event_name").and_then(Value::as_str),
        Some("UserPromptSubmit")
    );
    assert!(event.get("tool_name").is_none());
}

#[test]
fn claude_hook_script_ignores_grok_compat_session_start() {
    let capture = CaptureServer::start();
    let script = write_temp_hook_script("claude-ignore-grok", crate::hook_assets::CLAUDE_HOOK_SCRIPT);
    let output = Command::new("bash")
        .arg(&script)
        .env("HOME", hook_env_home("claude-ignore-grok"))
        .env("SUPERCLI_APP_PORT", capture.port.to_string())
        .env("SUPERCLI_SESSION_ID", "unpeel-route-session")
        .env("GROK_SESSION_ID", "grok-provider-session")
        .env(
            "SUPERCLI_HOOK_TRACE_FILE",
            hook_trace_file("claude-ignore-grok"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn claude hook");
    let mut child = output;
    child
        .stdin
        .take()
        .expect("claude hook stdin")
        .write_all(br#"{"hookEventName":"session_start","sessionId":"grok-provider-session"}"#)
        .expect("write claude hook stdin");
    let finished = child.wait_with_output().expect("wait for claude hook");
    assert!(
        finished.status.success(),
        "claude hook failed: {finished:?}"
    );
    thread::sleep(Duration::from_millis(200));
    assert!(
        capture.events_snapshot().is_empty(),
        "Grok-compat SessionStart must not be posted as a Claude hook event, got {:?}",
        capture.events_snapshot()
    );
}

#[test]
fn stale_tmp_claude_hooks_are_pruned() {
    let current = "/Users/me/.unpeel/hooks/claude-hooks.sh";
    assert!(!crate::hook_assets::is_stale_supercli_claude_hook(current, current));
    assert!(crate::hook_assets::is_stale_supercli_claude_hook(
        "/tmp/ur-c8eem28y/hooks/claude-hooks.sh",
        current
    ));
    assert!(!crate::hook_assets::is_stale_supercli_claude_hook(
        "/usr/bin/echo hello",
        current
    ));

    let mut array = vec![
        crate::hook_assets::build_hook_entry("SessionStart", "/tmp/old/hooks/claude-hooks.sh"),
        crate::hook_assets::build_hook_entry("SessionStart", current),
        json!({"hooks":[{"type":"command","command":"echo keep-me"}]}),
    ];
    assert!(crate::hook_assets::prune_stale_supercli_claude_hooks(&mut array, current));
    assert_eq!(array.len(), 2);
    assert_eq!(array[0]["hooks"][0]["command"].as_str(), Some(current));
    assert_eq!(
        array[1]["hooks"][0]["command"].as_str(),
        Some("echo keep-me")
    );
}

#[test]
fn claude_hook_script_forwards_session_start_as_hook_seen() {
    // The settings hook subscribes SessionStart so an in-tool /resume or
    // /clear re-links the session to the new conversation id. The script
    // must forward it as HookSeen (metadata-only latch) and must not let
    // it overwrite the durable busy/idle seed.
    assert!(crate::hook_assets::HOOK_EVENTS.contains(&"SessionStart"));
    assert!(crate::hook_assets::CLAUDE_HOOK_SCRIPT.contains(r#""hook_event_name":"HookSeen""#));

    let session_dir = temp_path("claude-record-start");
    let stop_payload = json!({
        "hook_event_name": "Stop",
        "session_id": "provider-1"
    })
    .to_string();
    let output = run_hook_script_recording(
        crate::hook_assets::CLAUDE_HOOK_SCRIPT,
        "claude-record-stop-seed",
        &[],
        &stop_payload,
        &session_dir,
    );
    assert!(output.status.success(), "claude hook failed: {output:?}");

    let start_payload = json!({
        "hook_event_name": "SessionStart",
        "session_id": "provider-2-resumed",
        "source": "resume"
    })
    .to_string();
    let output = run_hook_script_recording(
        crate::hook_assets::CLAUDE_HOOK_SCRIPT,
        "claude-record-start",
        &[],
        &start_payload,
        &session_dir,
    );
    assert!(output.status.success(), "claude hook failed: {output:?}");
    let event = read_last_hook_event(&session_dir);
    assert_eq!(
        event.get("hook_event_name").and_then(Value::as_str),
        Some("Stop")
    );
}

#[test]
fn claude_hook_script_records_permission_tool_name() {
    let session_dir = temp_path("claude-record-perm");
    let payload = json!({
        "hook_event_name": "PermissionRequest",
        "tool_name": "AskUserQuestion"
    })
    .to_string();
    let output = run_hook_script_recording(
        crate::hook_assets::CLAUDE_HOOK_SCRIPT,
        "claude-record-perm",
        &[],
        &payload,
        &session_dir,
    );
    assert!(output.status.success(), "claude hook failed: {output:?}");
    let event = read_last_hook_event(&session_dir);
    assert_eq!(
        event.get("hook_event_name").and_then(Value::as_str),
        Some("PermissionRequest")
    );
    assert_eq!(
        event.get("tool_name").and_then(Value::as_str),
        Some("AskUserQuestion")
    );
}

#[test]
fn claude_hook_script_posts_to_registered_app_ports() {
    assert!(CLAUDE_HOOK_SCRIPT.contains("SUPERCLI_PORT_REGISTRY_FILE"));
    assert!(CLAUDE_HOOK_SCRIPT.contains("current_supercli_ports"));
    assert!(CLAUDE_HOOK_SCRIPT.contains("post_hook_payload_to_current_ports"));
    assert!(crate::hook_assets::HOOK_EVENTS.contains(&"StopFailure"));
}

#[test]
#[ignore = "runs real Claude CLI against live auth"]
fn live_claude_print_emits_start_and_stop() {
    if real_command_path("claude").is_none() {
        return;
    }

    let capture = CaptureServer::start();
    let script_path = crate::hook_assets::claude_hook_script_path();
    crate::hook_assets::write_executable_script(
        &script_path,
        crate::hook_assets::CLAUDE_HOOK_SCRIPT,
        "Claude hook script",
    )
    .expect("write claude hook script");

    let settings_dir = temp_path("claude-settings");
    let settings_path = settings_dir.join("settings.json");
    let command = script_path.to_string_lossy().to_string();
    let settings = json!({
        "hooks": {
            "UserPromptSubmit": [crate::hook_assets::build_hook_entry("UserPromptSubmit", &command)],
            "Stop": [crate::hook_assets::build_hook_entry("Stop", &command)],
            "PermissionRequest": [crate::hook_assets::build_hook_entry("PermissionRequest", &command)]
        }
    });
    fs::write(
        &settings_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&settings).expect("serialize settings")
        ),
    )
    .expect("write claude settings");

    let mut command = Command::new("claude");
    command
        .current_dir(repo_root())
        .env("SUPERCLI_APP_PORT", capture.port.to_string())
        .env("SUPERCLI_SESSION_ID", "live-claude")
        .arg("-p")
        .arg("--settings")
        .arg(&settings_path)
        .arg("--permission-mode")
        .arg("acceptEdits")
        .arg("Reply with exactly OK and nothing else.");

    let status =
        run_with_timeout(command, Duration::from_secs(120)).expect("claude live smoke");
    assert!(status.success(), "claude exited unsuccessfully: {status}");
    assert!(
        capture.wait_for_event("UserPromptSubmit", Duration::from_secs(10)),
        "expected UserPromptSubmit event, got {:?}",
        capture.event_names()
    );
    assert!(
        capture.wait_for_event("Stop", Duration::from_secs(10)),
        "expected Stop event, got {:?}",
        capture.event_names()
    );
}
