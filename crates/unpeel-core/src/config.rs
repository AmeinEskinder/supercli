//! Typed configuration schema and validation.
//!
//! The workspace "config" is the settings subset of `app-state.json`: the
//! top-level setting keys plus the `experimental_features` object. This
//! module owns the schema so the CLI (`unpeel config check`) and the Host
//! (startup refusal) validate with exactly the same rules and messages.
//!
//! Rules:
//! - Every known setting path is type-checked when present. A wrong type or
//!   a value outside the allowed set is an **error** carrying the dotted
//!   path and the reason.
//! - A key the schema does not know is a **warning** carrying the dotted
//!   path. Unknown top-level keys are only warned about when they are not
//!   in [`KNOWN_STATE_KEYS`]: the rest of `app-state.json` is application
//!   state, not configuration, and must not trip the checker.
//! - Missing keys are never an issue; every setting has a documented
//!   default applied by its reader.

use serde_json::{Map, Value};
use std::fmt;

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

/// The value shape a setting accepts.
#[derive(Debug, Clone, Copy)]
enum SettingType {
    /// A real JSON boolean.
    Bool,
    /// A JSON string, one of the listed values (exact match).
    Enum(&'static [&'static str]),
    /// A JSON integer, one of the listed values.
    U64Enum(&'static [u64]),
}

struct SettingDef {
    /// Dotted path, e.g. `theme` or `experimental_features.sessions_mcp`.
    path: &'static str,
    ty: SettingType,
    /// Human description of the allowed values, used in messages.
    allowed: &'static str,
}

/// The schema. Mirrors `unpeel settings set`'s accepted keys and values;
/// `computer_use` stays parseable as a legacy alias (it no longer enables
/// the retired domain) and `computer_access` is the legacy read-fallback
/// spelling of `computer_default_access`.
static SETTINGS: &[SettingDef] = &[
    SettingDef {
        path: "experimental_features.sessions_mcp",
        ty: SettingType::Bool,
        allowed: "true or false",
    },
    SettingDef {
        path: "experimental_features.browser_mcp",
        ty: SettingType::Bool,
        allowed: "true or false",
    },
    SettingDef {
        path: "experimental_features.computer_use",
        ty: SettingType::Bool,
        allowed: "true or false",
    },
    SettingDef {
        path: "browser_default_access",
        ty: SettingType::Enum(&["on", "ask", "off"]),
        allowed: "on, ask, or off",
    },
    SettingDef {
        path: "mcp_nonchild_write_access",
        ty: SettingType::Enum(&["ask", "allow", "deny"]),
        allowed: "ask, allow, or deny",
    },
    SettingDef {
        path: "computer_default_access",
        ty: SettingType::Enum(&["ask", "allow", "off"]),
        allowed: "ask, allow, or off",
    },
    SettingDef {
        path: "computer_access",
        ty: SettingType::Enum(&["ask", "allow", "off"]),
        allowed: "ask, allow, or off",
    },
    SettingDef {
        path: "mcp_worktree_access",
        ty: SettingType::Bool,
        allowed: "true or false",
    },
    SettingDef {
        path: "mcp_auto_add_browser_screenshots",
        ty: SettingType::Bool,
        allowed: "true or false",
    },
    SettingDef {
        path: "auto_stop_archive_minutes",
        ty: SettingType::U64Enum(&[0, 30, 60, 120, 240, 480, 1440]),
        allowed: "0, 30, 60, 120, 240, 480, or 1440 (0 = off)",
    },
    SettingDef {
        path: "sidebar_stopped_limit",
        ty: SettingType::U64Enum(&[0, 3, 5, 10, 15, 25]),
        allowed: "0, 3, 5, 10, 15, or 25",
    },
    SettingDef {
        path: "theme",
        ty: SettingType::Enum(&["system", "light", "dark"]),
        allowed: "system, light, or dark",
    },
];

/// Top-level `app-state.json` keys that are application state, not config.
/// Best-effort: anything not listed here and not a known setting warns as
/// an unknown key. Warnings never block; the list only tunes noise.
/// Inventoried 2026-09-25 against the Rust writers (approvals.rs,
/// mcp_host.rs, session_ops.rs, migrate_cli.rs) plus the client-owned keys.
static KNOWN_STATE_KEYS: &[&str] = &[
    "projects",
    "active_project_id",
    "presets",
    "active_tabs",
    "pinned_sessions",
    "sessions",
    "worktrees",
    "appearance_settings",
    "transcript_settings",
    "file_openers",
    "openers",
    "session_sort_modes",
    "plugin_activation",
    "plugin_order",
    "experimental_features",
    // Approval grant stores (approvals.rs, migrate_cli.rs).
    "browser_approvals",
    "computer_approvals",
    "mcp_connector_approvals",
    "mcp_connector_approvals_quarantined",
    "mcp_write_approvals",
    "mcp_app_open_approvals",
    // Orchestrator grants (mcp_host.rs, session_ops.rs).
    "mcp_orchestrators",
];

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

/// One schema finding, always carrying the dotted config path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIssue {
    /// Dotted path, e.g. `theme` or `experimental_features.sessions_mcp`.
    pub path: String,
    /// Human-readable reason, e.g. `expected system, light, or dark; got "neon"`.
    pub message: String,
}

impl ConfigIssue {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "'{}': {}", self.path, self.message)
    }
}

/// The result of checking a config document.
#[derive(Debug, Default, Clone)]
pub struct ConfigReport {
    /// Unknown keys. Advisory; never fail a check on these alone.
    pub warnings: Vec<ConfigIssue>,
    /// Invalid values. `unpeel config check` exits 2; the Host refuses to start.
    pub errors: Vec<ConfigIssue>,
}

impl ConfigReport {
    /// True when no setting holds an invalid value.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// The shared message format used by `unpeel config check` and the
    /// Host's startup refusal, so both surfaces say the same thing.
    pub fn message(&self) -> String {
        let mut out = String::from("invalid config:");
        for error in &self.errors {
            out.push_str(&format!("\n  error: {error}"));
        }
        for warning in &self.warnings {
            out.push_str(&format!("\n  warning: unknown key {warning}"));
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Checking
// ---------------------------------------------------------------------------

fn lookup<'a>(state: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let mut current = state;
    let mut segments = path.split('.').peekable();
    loop {
        let segment = segments.next()?;
        let value = current.get(segment)?;
        match segments.peek() {
            None => return Some(value),
            Some(_) => current = value.as_object()?,
        }
    }
}

fn describe(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("{s:?}"),
        Value::Array(_) => "an array".into(),
        Value::Object(_) => "an object".into(),
    }
}

fn check_value(def: &SettingDef, value: &Value) -> Option<ConfigIssue> {
    let ok = match def.ty {
        SettingType::Bool => value.is_boolean(),
        SettingType::Enum(allowed) => value.as_str().is_some_and(|s| allowed.contains(&s)),
        SettingType::U64Enum(allowed) => value.as_u64().is_some_and(|n| allowed.contains(&n)),
    };
    if ok {
        None
    } else {
        Some(ConfigIssue::new(
            def.path,
            format!("expected {}; got {}", def.allowed, describe(value)),
        ))
    }
}

/// Validate the settings subset of an `app-state.json` document.
pub fn check_config(state: &Map<String, Value>) -> ConfigReport {
    let mut report = ConfigReport::default();

    for def in SETTINGS {
        if let Some(value) = lookup(state, def.path) {
            if let Some(issue) = check_value(def, value) {
                report.errors.push(issue);
            }
        }
    }

    // Unknown keys inside the feature-flags object: that object is purely
    // config, so anything unrecognized is definitionally a config key.
    match state.get("experimental_features") {
        Some(Value::Object(features)) => {
            for key in features.keys() {
                let path = format!("experimental_features.{key}");
                if !SETTINGS.iter().any(|def| def.path == path) {
                    report.warnings.push(ConfigIssue::new(
                        path.clone(),
                        "unknown setting; did you mean one of: ".to_string()
                            + &SETTINGS
                                .iter()
                                .filter_map(|def| {
                                    def.path
                                        .strip_prefix("experimental_features.")
                                        .map(str::to_string)
                                })
                                .collect::<Vec<_>>()
                                .join(", "),
                    ));
                }
            }
        }
        Some(other) => report.errors.push(ConfigIssue::new(
            "experimental_features",
            format!("expected an object; got {}", describe(other)),
        )),
        None => {}
    }

    // Unknown top-level keys, minus application state.
    for key in state.keys() {
        if KNOWN_STATE_KEYS.contains(&key.as_str()) {
            continue;
        }
        if SETTINGS.iter().any(|def| def.path == key) {
            continue;
        }
        report.warnings.push(ConfigIssue::new(
            key.clone(),
            "unknown key; see `unpeel settings list` for the supported settings".to_string(),
        ));
    }

    report
}

/// Check a whole loaded document; a non-object root is itself an error.
pub fn check_document(doc: &Value) -> ConfigReport {
    match doc.as_object() {
        Some(state) => check_config(state),
        None => ConfigReport {
            warnings: Vec::new(),
            errors: vec![ConfigIssue::new(
                "(root)",
                format!("expected a JSON object; got {}", describe(doc)),
            )],
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn state(doc: Value) -> Map<String, Value> {
        doc.as_object().cloned().expect("test doc is an object")
    }

    #[test]
    fn valid_config_has_no_findings() {
        let doc = json!({
            "experimental_features": { "sessions_mcp": true, "browser_mcp": false },
            "browser_default_access": "ask",
            "mcp_nonchild_write_access": "deny",
            "computer_default_access": "off",
            "mcp_worktree_access": true,
            "mcp_auto_add_browser_screenshots": false,
            "auto_stop_archive_minutes": 60,
            "sidebar_stopped_limit": 10,
            "theme": "dark",
            "presets": [],
            "projects": [],
        });
        let report = check_config(&state(doc));
        assert!(
            report.warnings.is_empty(),
            "warnings: {:?}",
            report.warnings
        );
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
        assert!(report.is_valid());
    }

    #[test]
    fn empty_config_is_valid() {
        let report = check_config(&state(json!({})));
        assert!(report.is_valid());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn unknown_keys_warn_with_path() {
        let doc = json!({
            "experimental_features": { "sessions_mcp_typo": true },
            "browser_defualt_access": "ask",
        });
        let report = check_config(&state(doc));
        assert!(report.is_valid(), "unknown keys must not fail");
        assert_eq!(report.warnings.len(), 2);
        let paths: Vec<&str> = report.warnings.iter().map(|w| w.path.as_str()).collect();
        assert!(
            paths.contains(&"experimental_features.sessions_mcp_typo"),
            "{paths:?}"
        );
        assert!(paths.contains(&"browser_defualt_access"), "{paths:?}");
    }

    #[test]
    fn invalid_values_error_with_path_and_reason() {
        let doc = json!({
            "theme": "neon",
            "mcp_worktree_access": "yes",
            "auto_stop_archive_minutes": 45,
            "sidebar_stopped_limit": -1,
            "experimental_features": { "sessions_mcp": "true" },
        });
        let report = check_config(&state(doc));
        assert!(!report.is_valid());
        assert_eq!(report.errors.len(), 5);
        for error in &report.errors {
            assert!(!error.path.is_empty() && !error.message.is_empty());
        }
        let by_path = |p: &str| {
            report
                .errors
                .iter()
                .find(|e| e.path == p)
                .unwrap_or_else(|| panic!("no error for {p}"))
                .message
                .clone()
        };
        assert!(
            by_path("theme").contains("system, light, or dark"),
            "{}",
            by_path("theme")
        );
        assert!(by_path("mcp_worktree_access").contains("true or false"));
        assert!(by_path("auto_stop_archive_minutes").contains("0, 30, 60"));
        assert!(by_path("sidebar_stopped_limit").contains("0, 3, 5, 10, 15, or 25"));
        assert!(by_path("experimental_features.sessions_mcp").contains("true or false"));
    }

    #[test]
    fn non_object_feature_flags_is_an_error() {
        let doc = json!({ "experimental_features": ["sessions_mcp"] });
        let report = check_config(&state(doc));
        assert!(!report.is_valid());
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].path, "experimental_features");
    }

    #[test]
    fn non_object_root_is_an_error() {
        let report = check_document(&json!([1, 2, 3]));
        assert!(!report.is_valid());
        assert_eq!(report.errors[0].path, "(root)");
    }

    #[test]
    fn legacy_spellings_validate() {
        let doc = json!({
            "computer_access": "ask",
            "experimental_features": { "computer_use": false },
        });
        let report = check_config(&state(doc));
        assert!(report.is_valid(), "{:?}", report.errors);
    }

    #[test]
    fn production_state_keys_do_not_warn() {
        // Every top-level key the app itself writes must be either a
        // known setting or a known state key — otherwise real homes
        // would trip the unknown-key warning on every check.
        let doc = json!({
            "projects": [],
            "active_project_id": null,
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {},
            "browser_approvals": ["dev-1"],
            "computer_approvals": [],
            "mcp_connector_approvals": {},
            "mcp_connector_approvals_quarantined": {},
            "mcp_write_approvals": {},
            "mcp_app_open_approvals": {},
            "mcp_orchestrators": {},
            "theme": "dark",
        });
        let report = check_config(&state(doc));
        assert!(
            report.warnings.is_empty(),
            "warnings: {:?}",
            report.warnings
        );
        assert!(report.is_valid());
    }

    #[test]
    fn message_format_carries_paths() {
        let doc = json!({ "theme": "neon", "typo_key": 1 });
        let report = check_config(&state(doc));
        let message = report.message();
        assert!(message.contains("invalid config:"), "{message}");
        assert!(message.contains("error: 'theme':"), "{message}");
        assert!(
            message.contains("warning: unknown key 'typo_key':"),
            "{message}"
        );
    }
}
