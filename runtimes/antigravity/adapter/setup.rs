use crate::hook_assets::{read_mergeable_json_object, write_file_atomic};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

/// Antigravity CLI reads global MCP servers from this file (its `/mcp`
/// overlay manages the same file; a workspace may add `.agents/mcp_config.json`
/// beside a project). Documented at antigravity.google/docs/cli/mcp, 2026-09.
pub(crate) fn antigravity_mcp_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        home.join(".gemini")
            .join("config")
            .join("mcp_config.json")
    })
}

/// The managed `unpeel` stdio entry. No `env` block: the shim resolves the
/// calling Session from the inherited environment, or from process ancestry
/// when the launcher strips it, and serves no tools outside a hosted Session.
pub(crate) fn antigravity_mcp_server_value(shim: &str) -> Value {
    json!({
        "command": shim,
        "args": [],
    })
}

pub fn install() -> Result<(), String> {
    let Some(path) = antigravity_mcp_config_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create Antigravity config dir: {error}"))?;
    }
    let Some(mut config) = read_mergeable_json_object(&path, "Antigravity mcp_config.json")?
    else {
        // Never replace malformed user configuration.
        return Ok(());
    };
    let root = config.as_object_mut().unwrap();
    let servers = root.entry("mcpServers").or_insert_with(|| json!({}));
    if !servers.is_object() {
        return Ok(());
    }
    let shim = crate::integrations::install::write_mcp_shim()?;
    let desired = antigravity_mcp_server_value(&shim.to_string_lossy());
    let servers = servers.as_object_mut().unwrap();
    if servers.get("unpeel") == Some(&desired) {
        return Ok(());
    }
    servers.insert("unpeel".into(), desired);
    let serialized = serde_json::to_string_pretty(&config)
        .map_err(|error| format!("Failed to serialize Antigravity mcp_config.json: {error}"))?;
    write_file_atomic(
        &path,
        &format!("{serialized}\n"),
        "Antigravity mcp_config.json",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_entry_is_a_plain_stdio_server_with_inherited_environment() {
        let entry = antigravity_mcp_server_value("/home/me/.unpeel/bin/unpeel-mcp");
        assert_eq!(entry["command"], "/home/me/.unpeel/bin/unpeel-mcp");
        assert_eq!(entry["args"], json!([]));
        assert!(entry.get("env").is_none());
    }
}
