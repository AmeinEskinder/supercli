//! `connector.toml` v1 manifest: parsing and validation.
//!
//! Implements the manifest section of `docs/connectors.md`. The manifest
//! is the trust boundary: `tools.provides` is a closed list the harness
//! enforces, and `policy` defaults are ceilings a session can only
//! tighten.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("invalid TOML: {0}")]
    InvalidToml(String),
    #[error("missing or invalid [connector] section: {0}")]
    BadConnectorSection(String),
    #[error("invalid connector name {0:?}: use lowercase letters, digits, dashes")]
    BadName(String),
    #[error("invalid version {0:?}: must be semver")]
    BadVersion(String),
    #[error("unknown connector kind {0:?}: mcp-stdio | mcp-http | builtin")]
    BadKind(String),
    #[error("unknown auth flow {0:?}: oauth2 | api-key | none")]
    BadAuthFlow(String),
    #[error("connector must provide at least one tool")]
    NoTools,
    #[error("invalid tool name {0:?}")]
    BadToolName(String),
    #[error("policy for undeclared tool {0:?}: policy keys must be in tools.provides")]
    PolicyForUnknownTool(String),
    #[error("unknown approval policy {0:?}: allow | ask | deny")]
    BadPolicy(String),
    #[error("missing required field {0:?}")]
    MissingField(String),
}

/// How the harness reaches the connector's MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorKind {
    /// Executable speaking MCP JSON-RPC over stdio.
    McpStdio,
    /// MCP server over HTTP (URL in `config.json`).
    McpHttp,
    /// Implemented inside the harness binary itself.
    Builtin,
}

/// How the connector authenticates to its external service. The harness
/// runs the flow once; the connector only ever sees the resulting token
/// via the `SUPERCLI_CONNECTOR_TOKEN` env var.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFlow {
    OAuth2,
    ApiKey,
    None,
}

/// Provider endpoints for the OAuth2 dance, from the manifest's
/// `[oauth]` table. Required for `connect` when `auth.flow = "oauth2"`;
/// the (public) `client_id` lives in the connector's `config.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct OAuthEndpoints {
    pub authorize_url: String,
    pub token_url: String,
}

/// Default approval policy per tool. Sessions may tighten these, never
/// loosen them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalPolicy {
    Allow,
    Ask,
    Deny,
}

impl std::str::FromStr for ApprovalPolicy {
    type Err = ManifestError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "allow" => Ok(ApprovalPolicy::Allow),
            "ask" => Ok(ApprovalPolicy::Ask),
            "deny" => Ok(ApprovalPolicy::Deny),
            other => Err(ManifestError::BadPolicy(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectorManifest {
    pub name: String,
    pub version: semver::Version,
    pub display_name: String,
    pub description: String,
    pub kind: ConnectorKind,
    pub auth_flow: AuthFlow,
    pub auth_scopes: Vec<String>,
    /// Closed list of tool names the harness exposes to sessions.
    pub provides: Vec<String>,
    /// Default policy per tool; tools not listed default to [`ApprovalPolicy::Ask`].
    pub policy: HashMap<String, ApprovalPolicy>,
    /// Raw `config_schema` table (JSON-Schema-ish); rendered as a form at
    /// install time. Kept uninterpreted — the UI layer validates values.
    pub config_schema: HashMap<String, toml::Value>,
    /// OAuth2 provider endpoints, from the `[oauth]` table. `None` when
    /// the manifest has no `[oauth]` section.
    pub oauth: Option<OAuthEndpoints>,
}

#[derive(Debug, Deserialize)]
struct RawManifest {
    connector: Option<RawConnector>,
    auth: Option<RawAuth>,
    tools: Option<RawTools>,
    policy: Option<HashMap<String, String>>,
    config_schema: Option<HashMap<String, toml::Value>>,
    oauth: Option<RawOAuth>,
}

#[derive(Debug, Deserialize)]
struct RawOAuth {
    authorize_url: Option<String>,
    token_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawConnector {
    name: Option<String>,
    version: Option<String>,
    display_name: Option<String>,
    description: Option<String>,
    kind: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawAuth {
    flow: Option<String>,
    scopes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct RawTools {
    provides: Option<Vec<String>>,
}

fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

fn is_valid_tool_name(name: &str) -> bool {
    // Dotted names like `mail.search`; each segment lowercase alnum/dash.
    !name.is_empty()
        && name.split('.').all(|seg| {
            !seg.is_empty()
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

fn required<T>(value: Option<T>, field: &str) -> Result<T, ManifestError> {
    value.ok_or_else(|| ManifestError::MissingField(field.to_string()))
}

/// Parse and validate a `connector.toml` v1 manifest.
pub fn parse_manifest(toml_str: &str) -> Result<ConnectorManifest, ManifestError> {
    let raw: RawManifest =
        toml::from_str(toml_str).map_err(|e| ManifestError::InvalidToml(e.to_string()))?;
    let connector = raw
        .connector
        .ok_or_else(|| ManifestError::BadConnectorSection("missing [connector]".to_string()))?;

    let name = required(connector.name, "connector.name")?;
    if !is_valid_name(&name) {
        return Err(ManifestError::BadName(name));
    }
    let version_str = required(connector.version, "connector.version")?;
    let version = semver::Version::parse(&version_str)
        .map_err(|_| ManifestError::BadVersion(version_str.clone()))?;
    let kind = match required(connector.kind, "connector.kind")?.as_str() {
        "mcp-stdio" => ConnectorKind::McpStdio,
        "mcp-http" => ConnectorKind::McpHttp,
        "builtin" => ConnectorKind::Builtin,
        other => return Err(ManifestError::BadKind(other.to_string())),
    };

    let auth = raw.auth.unwrap_or(RawAuth {
        flow: None,
        scopes: None,
    });
    let auth_flow = match auth.flow.as_deref().unwrap_or("none") {
        "oauth2" => AuthFlow::OAuth2,
        "api-key" => AuthFlow::ApiKey,
        "none" => AuthFlow::None,
        other => return Err(ManifestError::BadAuthFlow(other.to_string())),
    };

    let tools = raw.tools.unwrap_or(RawTools { provides: None });
    let provides = required(tools.provides, "tools.provides")?;
    if provides.is_empty() {
        return Err(ManifestError::NoTools);
    }
    for tool in &provides {
        if !is_valid_tool_name(tool) {
            return Err(ManifestError::BadToolName(tool.clone()));
        }
    }

    let mut policy = HashMap::new();
    for (tool, policy_str) in raw.policy.unwrap_or_default() {
        if !provides.contains(&tool) {
            return Err(ManifestError::PolicyForUnknownTool(tool));
        }
        let approval: ApprovalPolicy = policy_str.parse()?;
        policy.insert(tool, approval);
    }

    let oauth = match raw.oauth {
        None => None,
        Some(o) => {
            let authorize_url = o
                .authorize_url
                .filter(|u| !u.trim().is_empty())
                .ok_or_else(|| ManifestError::MissingField("oauth.authorize_url".to_string()))?;
            let token_url = o
                .token_url
                .filter(|u| !u.trim().is_empty())
                .ok_or_else(|| ManifestError::MissingField("oauth.token_url".to_string()))?;
            Some(OAuthEndpoints {
                authorize_url,
                token_url,
            })
        }
    };

    Ok(ConnectorManifest {
        name,
        version,
        display_name: required(connector.display_name, "connector.display_name")?,
        description: required(connector.description, "connector.description")?,
        kind,
        auth_flow,
        auth_scopes: auth.scopes.unwrap_or_default(),
        provides,
        policy,
        config_schema: raw.config_schema.unwrap_or_default(),
        oauth,
    })
}

impl ConnectorManifest {
    /// The manifest's default policy for a tool; unlisted tools default to
    /// `Ask` (fail towards human review, never towards silent execution).
    pub fn default_policy(&self, tool: &str) -> ApprovalPolicy {
        self.policy
            .get(tool)
            .copied()
            .unwrap_or(ApprovalPolicy::Ask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
[connector]
name = "gmail"
version = "1.2.0"
display_name = "Gmail"
description = "Read, search, draft and send mail."
kind = "mcp-stdio"

[auth]
flow = "oauth2"
scopes = ["https://www.googleapis.com/auth/gmail.modify"]

[tools]
provides = ["mail.search", "mail.read", "mail.draft", "mail.send"]

[policy]
"mail.search" = "allow"
"mail.read" = "allow"
"mail.draft" = "ask"
"mail.send" = "ask"

[config_schema]
account_hint = { type = "string", title = "Account label" }
"#;

    #[test]
    fn parses_valid_manifest() {
        let m = parse_manifest(VALID).expect("valid");
        assert_eq!(m.name, "gmail");
        assert_eq!(m.version, semver::Version::new(1, 2, 0));
        assert_eq!(m.kind, ConnectorKind::McpStdio);
        assert_eq!(m.auth_flow, AuthFlow::OAuth2);
        assert_eq!(m.provides.len(), 4);
        assert_eq!(m.default_policy("mail.search"), ApprovalPolicy::Allow);
        assert_eq!(m.default_policy("mail.draft"), ApprovalPolicy::Ask);
        assert_eq!(m.default_policy("mail.read"), ApprovalPolicy::Allow);
        assert_eq!(m.config_schema.len(), 1);
    }

    #[test]
    fn rejects_bad_manifests() {
        // Not TOML at all.
        assert!(matches!(
            parse_manifest("[[["),
            Err(ManifestError::InvalidToml(_))
        ));
        // Missing [connector].
        assert!(matches!(
            parse_manifest("[tools]\nprovides = [\"a.b\"]\n"),
            Err(ManifestError::BadConnectorSection(_))
        ));
        // Bad name.
        let bad_name = VALID.replace("name = \"gmail\"", "name = \"Gmail!\"");
        assert!(matches!(
            parse_manifest(&bad_name),
            Err(ManifestError::BadName(_))
        ));
        // Bad version.
        let bad_version = VALID.replace("version = \"1.2.0\"", "version = \"yesterday\"");
        assert!(matches!(
            parse_manifest(&bad_version),
            Err(ManifestError::BadVersion(_))
        ));
        // Unknown kind.
        let bad_kind = VALID.replace("kind = \"mcp-stdio\"", "kind = \"carrier-pigeon\"");
        assert!(matches!(
            parse_manifest(&bad_kind),
            Err(ManifestError::BadKind(_))
        ));
        // No tools.
        let no_tools = VALID.replace(
            "provides = [\"mail.search\", \"mail.read\", \"mail.draft\", \"mail.send\"]",
            "provides = []",
        );
        assert!(matches!(
            parse_manifest(&no_tools),
            Err(ManifestError::NoTools)
        ));
        // Policy for a tool the connector doesn't provide.
        let bad_policy = VALID.replace(
            "\"mail.send\" = \"ask\"",
            "\"mail.send\" = \"ask\"\n\"mail.delete\" = \"deny\"",
        );
        assert!(matches!(
            parse_manifest(&bad_policy),
            Err(ManifestError::PolicyForUnknownTool(_))
        ));
        // Unknown policy value.
        let bad_policy_value =
            VALID.replace("\"mail.send\" = \"ask\"", "\"mail.send\" = \"sometimes\"");
        assert!(matches!(
            parse_manifest(&bad_policy_value),
            Err(ManifestError::BadPolicy(_))
        ));
    }

    #[test]
    fn minimal_manifest_defaults() {
        let m = parse_manifest(
            r#"
[connector]
name = "notes"
version = "0.1.0"
display_name = "Notes"
description = "Local notes."
kind = "builtin"

[tools]
provides = ["notes.list"]
"#,
        )
        .expect("minimal");
        assert_eq!(m.auth_flow, AuthFlow::None);
        assert!(m.auth_scopes.is_empty());
        assert_eq!(m.default_policy("notes.list"), ApprovalPolicy::Ask);
    }
}
