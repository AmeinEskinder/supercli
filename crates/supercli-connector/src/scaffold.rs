//! `supercli connector new`: scaffold a new connector directory.
//!
//! Generates a valid `connector.toml` (validated through the real
//! manifest parser before it is returned) plus a stub MCP server for
//! `mcp-stdio` connectors — a Python script answering
//! `initialize`/`tools/list`/`tools/call` for the declared tools, ready
//! to replace with a real implementation. For `mcp-http` the scaffold
//! writes a `config.json` with a placeholder URL instead of an
//! executable. `builtin` has nothing to scaffold and is refused.

use std::fmt::Write as FmtWrite;

use crate::manifest::{parse_manifest, AuthFlow, ConnectorKind, ConnectorManifest, ManifestError};

#[derive(Debug, Clone, Default)]
pub struct ScaffoldOptions {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub kind: Option<ConnectorKind>,
    pub auth_flow: Option<AuthFlow>,
    pub auth_scopes: Vec<String>,
    /// Defaults to `["<name>.echo"]`.
    pub tools: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScaffoldError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("builtin connectors are implemented inside the harness — nothing to scaffold")]
    Builtin,
    #[error("invalid tool name {0:?}")]
    BadTool(String),
}

/// The generated files: the caller writes them into `<dir>/<name>/`.
#[derive(Debug, Clone)]
pub struct ScaffoldedConnector {
    /// Validated by parsing it back with the real manifest parser.
    pub manifest_toml: String,
    pub manifest: ConnectorManifest,
    /// `Some` for `mcp-stdio`: the stub `connector` executable source.
    pub stub_source: Option<String>,
    /// `Some` for `mcp-http`: placeholder `config.json`.
    pub config_json: Option<String>,
}

fn kind_str(kind: ConnectorKind) -> &'static str {
    match kind {
        ConnectorKind::McpStdio => "mcp-stdio",
        ConnectorKind::McpHttp => "mcp-http",
        ConnectorKind::Builtin => "builtin",
    }
}

fn auth_str(flow: AuthFlow) -> &'static str {
    match flow {
        AuthFlow::OAuth2 => "oauth2",
        AuthFlow::ApiKey => "api-key",
        AuthFlow::None => "none",
    }
}

fn stub_source(name: &str, tools: &[String]) -> String {
    let entries: Vec<String> = tools
        .iter()
        .map(|tool| {
            format!(
                "    {{\"name\": {tool:?}, \"description\": \"Stub implementation - replace me.\", \"inputSchema\": {{\"type\": \"object\"}}}}"
            )
        })
        .collect();
    let tools_json = format!("[\n{}\n]", entries.join(",\n"));
    format!(
        r#"#!/usr/bin/env python3
"""Stub MCP server for the {name} connector.

Speaks MCP JSON-RPC over stdio for the tools declared in connector.toml.
Replace the tools/call branch with a real implementation; keep the
tool names exactly as declared — the harness filters everything else.
"""
import json
import sys

TOOLS = json.loads({tools_json:?})

def respond(mid, result):
    sys.stdout.write(json.dumps({{"jsonrpc": "2.0", "id": mid, "result": result}}) + "\n")
    sys.stdout.flush()

def respond_error(mid, message):
    sys.stdout.write(json.dumps({{"jsonrpc": "2.0", "id": mid, "error": {{"code": -32601, "message": message}}}}) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method, mid = msg.get("method"), msg.get("id")
    if method is None or method.startswith("notifications/"):
        continue
    if method == "initialize":
        respond(mid, {{"protocolVersion": "2024-11-05", "capabilities": {{}}, "serverInfo": {{"name": {name:?}, "version": "0.1.0"}}}})
    elif method == "tools/list":
        respond(mid, {{"tools": TOOLS}})
    elif method == "tools/call":
        tool = msg["params"]["name"]
        args = msg["params"].get("arguments", {{}})
        respond(mid, {{"content": [{{"type": "text", "text": tool + ":" + json.dumps(args)}}]}})
    else:
        respond_error(mid, "unknown method: " + str(method))
"#
    )
}

/// Scaffold a new connector. The generated `connector.toml` is parsed
/// back through the real manifest parser, so a successful return is a
/// proof the scaffold is valid.
pub fn scaffold(
    name: &str,
    options: ScaffoldOptions,
) -> Result<ScaffoldedConnector, ScaffoldError> {
    let kind = options.kind.unwrap_or(ConnectorKind::McpStdio);
    if kind == ConnectorKind::Builtin {
        return Err(ScaffoldError::Builtin);
    }
    let auth_flow = options.auth_flow.unwrap_or(AuthFlow::None);
    let display_name = options.display_name.unwrap_or_else(|| name.to_string());
    let description = options.description.unwrap_or_default();
    let tools = if options.tools.is_empty() {
        vec![format!("{name}.echo")]
    } else {
        options.tools
    };

    let mut manifest_toml = format!(
        r#"[connector]
name = {name:?}
version = "0.1.0"
display_name = {display_name:?}
description = {description:?}
kind = "{kind}"

[auth]
flow = "{auth}"
"#,
        kind = kind_str(kind),
        auth = auth_str(auth_flow),
    );
    if !options.auth_scopes.is_empty() {
        let _ = writeln!(manifest_toml, "scopes = [");
        for scope in &options.auth_scopes {
            let _ = writeln!(manifest_toml, "  {scope:?},");
        }
        manifest_toml.push_str("]\n");
    }
    manifest_toml.push_str("\n[tools]\nprovides = [\n");
    for tool in &tools {
        let _ = writeln!(manifest_toml, "  {tool:?},");
    }
    manifest_toml.push_str("]\n");
    manifest_toml.push_str(
        r#"
# Default approval policy per tool (allow | ask | deny). Tools not listed
# default to "ask". A session may tighten these, never loosen them.
# [policy]
# "example.tool" = "ask"
"#,
    );

    // The real parser is the validator: name rules, tool-name rules, and
    // section shapes are all enforced here, not reimplemented.
    let manifest = parse_manifest(&manifest_toml)?;
    // parse_manifest already validated every tool name; double-check the
    // count matches so a dropped tool can never slip through silently.
    if manifest.provides.len() != tools.len() {
        return Err(ScaffoldError::BadTool(
            "tool list mismatch after parse".to_string(),
        ));
    }

    let (stub_source, config_json) = match kind {
        ConnectorKind::McpStdio => (Some(stub_source(name, &tools)), None),
        ConnectorKind::McpHttp => (
            None,
            Some(
                serde_json::to_string_pretty(&serde_json::json!({
                    "url": "http://127.0.0.1:8080/mcp",
                    "_comment": "Replace with the connector's MCP HTTP endpoint.",
                }))
                .expect("config JSON"),
            ),
        ),
        ConnectorKind::Builtin => unreachable!("refused above"),
    };

    Ok(ScaffoldedConnector {
        manifest_toml,
        manifest,
        stub_source,
        config_json,
    })
}

/// Parse a `--kind` flag value for `new`. `builtin` is accepted by the
/// string table but refused by [`scaffold`] — nothing to generate.
pub fn connector_kind_from_str(s: &str) -> Option<ConnectorKind> {
    match s {
        "mcp-stdio" => Some(ConnectorKind::McpStdio),
        "mcp-http" => Some(ConnectorKind::McpHttp),
        "builtin" => Some(ConnectorKind::Builtin),
        _ => None,
    }
}

/// Parse an `--auth` flag value for `new`.
pub fn auth_flow_from_str(s: &str) -> Option<AuthFlow> {
    match s {
        "oauth2" => Some(AuthFlow::OAuth2),
        "api-key" => Some(AuthFlow::ApiKey),
        "none" => Some(AuthFlow::None),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_defaults_are_valid() {
        let s = scaffold("demo", ScaffoldOptions::default()).unwrap();
        assert_eq!(s.manifest.name, "demo");
        assert_eq!(s.manifest.provides, vec!["demo.echo".to_string()]);
        assert_eq!(s.manifest.auth_flow, AuthFlow::None);
        assert!(s.stub_source.is_some());
        assert!(s.config_json.is_none());
        // The stub serves exactly the declared tools.
        assert!(s.stub_source.unwrap().contains("demo.echo"));
    }

    #[test]
    fn scaffold_bad_name_and_tool_are_refused() {
        assert!(matches!(
            scaffold("Bad Name", ScaffoldOptions::default()),
            Err(ScaffoldError::Manifest(ManifestError::BadName(_)))
        ));
        assert!(matches!(
            scaffold(
                "demo",
                ScaffoldOptions {
                    tools: vec!["BAD TOOL".to_string()],
                    ..Default::default()
                }
            ),
            Err(ScaffoldError::Manifest(ManifestError::BadToolName(_)))
        ));
    }

    #[test]
    fn scaffold_builtin_is_refused_and_http_gets_config() {
        assert!(matches!(
            scaffold(
                "demo",
                ScaffoldOptions {
                    kind: Some(ConnectorKind::Builtin),
                    ..Default::default()
                }
            ),
            Err(ScaffoldError::Builtin)
        ));
        let s = scaffold(
            "demo",
            ScaffoldOptions {
                kind: Some(ConnectorKind::McpHttp),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(s.stub_source.is_none());
        assert!(s.config_json.unwrap().contains("http://127.0.0.1:8080/mcp"));
    }

    #[test]
    fn scaffolded_stub_answers_mcp() {
        let s = scaffold("demo", ScaffoldOptions::default()).unwrap();
        let dir =
            std::env::temp_dir().join(format!("supercli-conn-scaffold-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("connector");
        std::fs::write(&exe, s.stub_source.unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let manifest = s.manifest;
        let mut proc = crate::process::ConnectorProcess::spawn(
            &manifest,
            &exe,
            "",
            std::time::Duration::from_secs(10),
        )
        .expect("spawn scaffolded stub");
        let tools = proc.tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "demo.echo");
        let mut args = std::collections::HashMap::new();
        args.insert(
            "msg".to_string(),
            serde_json::Value::String("hi".to_string()),
        );
        let out = proc.call("demo.echo", args).expect("call");
        let text = serde_json::to_string(&out).unwrap();
        assert!(text.contains("demo.echo"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
