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
fn amp_plugin_tracks_agent_lifecycle_and_prompt_hints() {
    assert!(AMP_PLUGIN_SCRIPT.contains("@i-know-the-amp-plugin-api-is-wip"));
    assert!(AMP_PLUGIN_SCRIPT.contains("amp.on(\"agent.start\""));
    assert!(AMP_PLUGIN_SCRIPT.contains("amp.on(\"agent.end\""));
    assert!(AMP_PLUGIN_SCRIPT.contains("tool_name: \"amp\""));
    assert!(AMP_PLUGIN_SCRIPT.contains("prompt_text"));
    assert!(AMP_PLUGIN_SCRIPT.contains("hook_event_name"));
    assert!(AMP_PLUGIN_SCRIPT.contains("threadIDFrom"));
    assert!(AMP_PLUGIN_SCRIPT.contains("session_id: threadIDFrom"));
}
