//! Structured JSON logging for the Host (R4).
//!
//! Emits one JSON object per line to stderr (or a file), with:
//! - `ts`: RFC3339 timestamp
//! - `level`: DEBUG, INFO, WARN, ERROR
//! - `msg`: the message
//! - `fields`: optional structured data
//!
//! Level is controlled by `SUPERCLI_LOG_LEVEL` env var (default INFO).
//! Set to DEBUG for verbose output.

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    fn as_str(&self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }

    /// Parse a level name (case-insensitive). Used by the `--log-level` flag.
    /// Returns `None` for unrecognized names.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "DEBUG" => Some(Level::Debug),
            "INFO" => Some(Level::Info),
            "WARN" | "WARNING" => Some(Level::Warn),
            "ERROR" => Some(Level::Error),
            _ => None,
        }
    }

    fn from_env() -> Self {
        std::env::var("SUPERCLI_LOG_LEVEL")
            .ok()
            .and_then(|s| Level::parse(&s))
            .unwrap_or(Level::Info)
    }
}

static MIN_LEVEL: OnceLock<Level> = OnceLock::new();

fn min_level() -> Level {
    *MIN_LEVEL.get_or_init(Level::from_env)
}

/// Override the minimum log level programmatically (e.g., from a `--log-level`
/// CLI flag). Takes precedence over the `SUPERCLI_LOG_LEVEL` env var. Must be
/// called before any log output; subsequent calls are ignored (OnceLock).
pub fn set_level(level: Level) {
    let _ = MIN_LEVEL.set(level);
}

/// Format a log event as a JSON string (without printing).
/// Used by `log()` and by tests to verify schema.
fn format_log(level: Level, msg: &str, fields: Option<serde_json::Value>) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut obj = serde_json::json!({
        "ts": ts,
        "level": level.as_str(),
        "msg": msg,
    });
    if let Some(f) = fields {
        obj["fields"] = f;
    }
    obj.to_string()
}

/// Log a structured JSON event.
///
/// Example output:
/// `{"ts":1727227620,"level":"INFO","msg":"handler panicked","fields":{"path":"/mobile/events"}}`
///
/// `ts` is Unix seconds (no chrono dependency).
pub fn log(level: Level, msg: &str, fields: Option<serde_json::Value>) {
    if level < min_level() {
        return;
    }
    eprintln!("{}", format_log(level, msg, fields));
}

/// Convenience wrappers.
pub fn debug(msg: &str) {
    log(Level::Debug, msg, None);
}

pub fn info(msg: &str) {
    log(Level::Info, msg, None);
}

pub fn warn(msg: &str) {
    log(Level::Warn, msg, None);
}

pub fn error(msg: &str) {
    log(Level::Error, msg, None);
}

pub fn info_fields(msg: &str, fields: serde_json::Value) {
    log(Level::Info, msg, Some(fields));
}

pub fn error_fields(msg: &str, fields: serde_json::Value) {
    log(Level::Error, msg, Some(fields));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_ordering() {
        assert!(Level::Debug < Level::Info);
        assert!(Level::Info < Level::Warn);
        assert!(Level::Warn < Level::Error);
    }

    #[test]
    fn level_as_str() {
        assert_eq!(Level::Debug.as_str(), "DEBUG");
        assert_eq!(Level::Error.as_str(), "ERROR");
    }

    #[test]
    fn level_parse() {
        assert_eq!(Level::parse("debug"), Some(Level::Debug));
        assert_eq!(Level::parse("DEBUG"), Some(Level::Debug));
        assert_eq!(Level::parse("info"), Some(Level::Info));
        assert_eq!(Level::parse("warn"), Some(Level::Warn));
        assert_eq!(Level::parse("warning"), Some(Level::Warn));
        assert_eq!(Level::parse("error"), Some(Level::Error));
        assert_eq!(Level::parse("invalid"), None);
        assert_eq!(Level::parse(""), None);
    }

    /// R4: JSON log schema — verify the output has the required fields
    /// with correct types.
    #[test]
    fn log_schema_has_required_fields() {
        let json_str = format_log(Level::Info, "test message", None);
        let v: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

        // Required fields.
        assert!(v.get("ts").is_some(), "must have ts field");
        assert!(v.get("level").is_some(), "must have level field");
        assert!(v.get("msg").is_some(), "must have msg field");

        // Types.
        assert!(v["ts"].is_u64(), "ts must be u64 (unix seconds)");
        assert_eq!(v["level"], "INFO", "level must be string");
        assert_eq!(v["msg"], "test message", "msg must match");

        // No fields key when None.
        assert!(v.get("fields").is_none(), "fields absent when None");
    }

    #[test]
    fn log_schema_with_fields() {
        let fields = serde_json::json!({"path": "/mobile/events", "status": 500});
        let json_str = format_log(Level::Error, "handler panicked", Some(fields));
        let v: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

        assert_eq!(v["level"], "ERROR");
        assert_eq!(v["msg"], "handler panicked");
        assert!(v.get("fields").is_some(), "fields present when Some");
        assert_eq!(v["fields"]["path"], "/mobile/events");
        assert_eq!(v["fields"]["status"], 500);
    }

    #[test]
    fn log_schema_all_levels() {
        for (level, expected) in [
            (Level::Debug, "DEBUG"),
            (Level::Info, "INFO"),
            (Level::Warn, "WARN"),
            (Level::Error, "ERROR"),
        ] {
            let json_str = format_log(level, "msg", None);
            let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
            assert_eq!(
                v["level"], expected,
                "level {expected} serializes correctly"
            );
        }
    }
}
