//! Config forms: render a connector's `config_schema` as an interactive
//! CLI questionnaire and validate `key=value` pairs against it.
//!
//! Schema field shape (from `connector.toml`):
//! ```toml
//! [config_schema]
//! account_hint = { type = "string", title = "Account label" }
//! client_id    = { type = "string", title = "OAuth client ID", description = "From the provider's developer console." }
//! port         = { type = "integer", title = "Port", default = 8080 }
//! verbose      = { type = "boolean", title = "Verbose logging", default = false }
//! ```
//! Supported `type`s: `string`, `integer`, `number`, `boolean`.
//! Optional `title`, `description`, `default`, and `enum` (allowed values).
//! Secrets never belong here — they go through `connect`, never `config.json`.

use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::Write;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FormError {
    #[error("I/O: {0}")]
    Io(String),
    #[error("config field {field:?}: {problem}")]
    Invalid { field: String, problem: String },
    #[error("unknown config field {0:?} (not in the connector's config_schema)")]
    UnknownField(String),
}

#[derive(Debug, Clone)]
struct Field {
    name: String,
    title: String,
    description: Option<String>,
    kind: String,
    default: Option<Value>,
    allowed: Option<Vec<Value>>,
}

fn parse_schema(schema: &HashMap<String, toml::Value>) -> Result<Vec<Field>, FormError> {
    let mut fields: Vec<Field> = Vec::new();
    for (name, raw) in schema {
        let table = raw.as_table().ok_or_else(|| FormError::Invalid {
            field: name.clone(),
            problem: "schema entry must be an inline table".to_string(),
        })?;
        let get_str = |key: &str| table.get(key).and_then(|v| v.as_str()).map(str::to_string);
        let kind = get_str("type").ok_or_else(|| FormError::Invalid {
            field: name.clone(),
            problem: "missing `type`".to_string(),
        })?;
        if !matches!(kind.as_str(), "string" | "integer" | "number" | "boolean") {
            return Err(FormError::Invalid {
                field: name.clone(),
                problem: format!("unsupported type {kind:?}"),
            });
        }
        let default = table.get("default").map(toml_to_json);
        let allowed = table
            .get("enum")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().map(toml_to_json).collect());
        fields.push(Field {
            title: get_str("title").unwrap_or_else(|| name.clone()),
            description: get_str("description"),
            name: name.clone(),
            kind,
            default,
            allowed,
        });
    }
    fields.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(fields)
}

fn toml_to_json(v: &toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::json!(*i),
        toml::Value::Float(f) => serde_json::json!(*f),
        toml::Value::Boolean(b) => Value::Bool(*b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(a) => Value::Array(a.iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => {
            let mut m = Map::new();
            for (k, v) in t {
                m.insert(k.clone(), toml_to_json(v));
            }
            Value::Object(m)
        }
    }
}

/// Coerce a raw string into the field's type (`"8080"` → 8080 for
/// integer, `"true"` → true for boolean, …).
fn coerce(field: &Field, raw: &str) -> Result<Value, FormError> {
    let invalid = |problem: String| FormError::Invalid {
        field: field.name.clone(),
        problem,
    };
    let value = match field.kind.as_str() {
        "string" => Value::String(raw.to_string()),
        "integer" => raw
            .trim()
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| invalid(format!("{raw:?} is not an integer")))?,
        "number" => raw
            .trim()
            .parse::<f64>()
            .map(|f| serde_json::json!(f))
            .map_err(|_| invalid(format!("{raw:?} is not a number")))?,
        "boolean" => match raw.trim().to_lowercase().as_str() {
            "true" | "yes" | "1" => Value::Bool(true),
            "false" | "no" | "0" => Value::Bool(false),
            _ => return Err(invalid(format!("{raw:?} is not a boolean (true/false)"))),
        },
        _ => unreachable!("schema validation rejects other types"),
    };
    if let Some(allowed) = &field.allowed {
        if !allowed.contains(&value) {
            return Err(invalid(format!(
                "{raw:?} is not one of {}",
                allowed
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }
    Ok(value)
}

/// Validate `key=value` pairs against the schema (unknown fields and
/// type mismatches are errors). Missing fields fall back to schema
/// defaults when the caller merges.
pub fn validate_pairs(
    schema: &HashMap<String, toml::Value>,
    pairs: &[(String, String)],
) -> Result<Map<String, Value>, FormError> {
    let fields = parse_schema(schema)?;
    let by_name: HashMap<&str, &Field> = fields.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut out = Map::new();
    for (key, raw) in pairs {
        let field = by_name
            .get(key.as_str())
            .ok_or_else(|| FormError::UnknownField(key.clone()))?;
        out.insert(key.clone(), coerce(field, raw)?);
    }
    Ok(out)
}

/// Interactive questionnaire over stdin/stdout. Empty input accepts the
/// schema default (when one exists); fields without a default may be
/// left empty to skip them. Returns the answered values.
pub fn run_form(schema: &HashMap<String, toml::Value>) -> Result<Map<String, Value>, FormError> {
    let fields = parse_schema(schema)?;
    let mut out = Map::new();
    for field in &fields {
        if let Some(desc) = &field.description {
            println!("  {}", desc.trim());
        }
        let default_hint = field
            .default
            .as_ref()
            .map(|d| format!(" [{}]", render_default(d)))
            .unwrap_or_default();
        print!("{} ({}){default_hint}: ", field.title, field.kind);
        std::io::stdout()
            .flush()
            .map_err(|e| FormError::Io(e.to_string()))?;
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| FormError::Io(e.to_string()))?;
        let line = line.trim();
        if line.is_empty() {
            if let Some(default) = &field.default {
                out.insert(field.name.clone(), default.clone());
            }
            continue;
        }
        match coerce(field, line) {
            Ok(value) => {
                out.insert(field.name.clone(), value);
            }
            Err(e) => {
                eprintln!("  {e} — skipped (re-run the form to set it).");
            }
        }
    }
    Ok(out)
}

fn render_default(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Merge answered values over an existing config object (answers win).
pub fn merge_config(
    existing: &Map<String, Value>,
    answers: Map<String, Value>,
) -> Map<String, Value> {
    let mut merged = existing.clone();
    for (k, v) in answers {
        merged.insert(k, v);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> HashMap<String, toml::Value> {
        let toml_str = r#"
account_hint = { type = "string", title = "Account label" }
port = { type = "integer", title = "Port", default = 8080 }
verbose = { type = "boolean", title = "Verbose" }
mode = { type = "string", title = "Mode", enum = ["a", "b"] }
"#;
        toml::from_str(toml_str).unwrap()
    }

    #[test]
    fn validates_and_coerces_pairs() {
        let out = validate_pairs(
            &schema(),
            &[
                ("port".to_string(), "9090".to_string()),
                ("verbose".to_string(), "yes".to_string()),
                ("mode".to_string(), "a".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(out["port"], serde_json::json!(9090));
        assert_eq!(out["verbose"], serde_json::json!(true));
        // Bad integer.
        assert!(validate_pairs(&schema(), &[("port".into(), "nope".into())]).is_err());
        // Unknown field.
        assert!(validate_pairs(&schema(), &[("nope".into(), "1".into())]).is_err());
        // Enum violation.
        assert!(validate_pairs(&schema(), &[("mode".into(), "c".into())]).is_err());
    }

    #[test]
    fn rejects_bad_schema() {
        let bad: HashMap<String, toml::Value> =
            toml::from_str(r#"x = { type = "fancy", title = "X" }"#).unwrap();
        assert!(validate_pairs(&bad, &[]).is_err());
    }

    #[test]
    fn merge_prefers_answers() {
        let mut existing = Map::new();
        existing.insert("port".to_string(), serde_json::json!(1));
        let mut answers = Map::new();
        answers.insert("port".to_string(), serde_json::json!(2));
        let merged = merge_config(&existing, answers);
        assert_eq!(merged["port"], serde_json::json!(2));
    }
}
