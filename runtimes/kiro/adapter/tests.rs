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
fn kiro_hook_maps_v3_and_v2_lifecycle_events() {
    assert!(KIRO_HOOK_SCRIPT.contains("SessionStart|agentSpawn"));
    assert!(KIRO_HOOK_SCRIPT.contains("UserPromptSubmit|userPromptSubmit"));
    assert!(KIRO_HOOK_SCRIPT.contains("PreToolUse|preToolUse|PostToolUse|postToolUse"));
    assert!(KIRO_HOOK_SCRIPT.contains("messages.jsonl"));
    assert!(KIRO_HOOK_SCRIPT.contains("sessions/cli/$_provider_session_id.jsonl"));
    assert!(KIRO_HOOK_SCRIPT.contains("SUPERCLI_SESSION_ID"));
}

#[test]
fn kiro_mcp_config_explicitly_maps_session_identity() {
    let server = kiro_mcp_server_value(std::path::Path::new("/tmp/home/.supercli/bin/supercli-mcp"));
    assert_eq!(server["command"], "/tmp/home/.supercli/bin/supercli-mcp");
    assert_eq!(server["args"], json!([]));
    // Kiro v3 passes only the declared block to MCP children; the generic
    // hosted-shell variables carry identity, and grants come from the
    // Session manifest inside the gate — never from an env grant.
    assert_eq!(server["env"]["SUPERCLI_SESSION_ID"], "${SUPERCLI_SESSION_ID}");
    assert_eq!(server["env"]["SUPERCLI_HOME"], "${SUPERCLI_HOME}");
    assert_eq!(server["env"]["SUPERCLI_HOST_BIN"], "${SUPERCLI_HOST_BIN}");
    assert!(server["env"].get("SUPERCLI_SESSIONS_MCP_ENABLED").is_none());
}
