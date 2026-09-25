//! `supercli config check` — validate the workspace configuration.
//!
//! The config is the settings subset of `app-state.json`. Unknown keys are
//! reported as warnings (exit stays 0); invalid values are errors and the
//! command exits 2. The Host applies the same schema at startup and refuses
//! to serve an invalid config.

use supercli_core::{app_state, config};

pub const CONFIG_HELP: &str = "\
supercli config — inspect the workspace configuration

  supercli config check [--json]   validate settings in app-state.json
  supercli config reference        print the schema-generated config reference (Markdown)

Unknown keys print as warnings; invalid values print as errors and exit 2.\
";

fn usage_error(what: &str) -> i32 {
    eprintln!("supercli config: {what}\n{CONFIG_HELP}");
    2
}

/// `args` are the words after `config`; `json` selects machine output.
/// Returns the process exit code.
pub fn run(args: &[String], json: bool) -> i32 {
    let mut rest = args.iter().peekable();
    match rest.next().map(String::as_str) {
        Some("check") => {}
        Some("reference") => return print_reference(),
        Some("--help") | Some("-h") | Some("help") | None => {
            println!("{CONFIG_HELP}");
            return 0;
        }
        Some(other) => return usage_error(&format!("unknown subcommand {other:?}")),
    }
    if rest.any(|a| a != "--json" && a != "json") {
        return usage_error("`config check` takes no arguments besides --json");
    }

    let doc = app_state::load();
    check_loaded(&doc, json)
}

/// Validate an already-loaded document. Split from `run` so tests can
/// exercise the exit codes without mutating process-global `SUPERCLI_HOME`.
fn check_loaded(doc: &Result<serde_json::Value, String>, json: bool) -> i32 {
    let doc = match doc {
        Ok(doc) => doc,
        Err(err) => {
            let message = format!("invalid config:\n  error: cannot load app-state.json: {err}");
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "ok": false, "warnings": [], "errors": [message] })
                );
            } else {
                eprintln!("{message}");
            }
            return 2;
        }
    };
    let report = config::check_document(doc);

    if json {
        let warnings: Vec<String> = report
            .warnings
            .iter()
            .map(|w| format!("unknown key {w}"))
            .collect();
        let errors: Vec<String> = report.errors.iter().map(|e| e.to_string()).collect();
        println!(
            "{}",
            serde_json::json!({
                "ok": report.is_valid(),
                "warnings": warnings,
                "errors": errors,
            })
        );
    } else if report.is_valid() && report.warnings.is_empty() {
        println!("config OK");
    } else {
        // Same message shape the Host prints when it refuses to start.
        println!("{}", report.message());
    }

    if report.is_valid() {
        0
    } else {
        2
    }
}

/// Print the config reference as Markdown, generated from the P3 schema
/// (`supercli_core::config::SETTINGS`). Used by the mdBook docs build; not
/// handwritten.
fn print_reference() -> i32 {
    use supercli_core::config::{SettingType, SETTINGS};
    println!("# Config reference");
    println!();
    println!("Generated from the typed config schema (`supercli config reference`).");
    println!("Every setting has a documented default applied by its reader;");
    println!("missing keys are never an issue. Unknown keys produce warnings;");
    println!("invalid values produce errors (exit 2 from `supercli config check`).");
    println!();
    for def in SETTINGS {
        let ty_desc = match def.ty {
            SettingType::Bool => "boolean".to_string(),
            SettingType::Enum(vals) => format!("string, one of: {}", vals.join(", ")),
            SettingType::U64Enum(vals) => format!(
                "integer, one of: {}",
                vals.iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        println!("## `{}`", def.path);
        println!();
        println!("- **Type:** {ty_desc}");
        println!("- **Allowed:** {}", def.allowed);
        println!();
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(json: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(json)
    }

    #[test]
    fn unknown_key_warns_and_exits_zero() {
        let code = check_loaded(
            &doc(serde_json::json!({ "theme": "dark", "typo_key": 1 })),
            false,
        );
        assert_eq!(code, 0);
    }

    #[test]
    fn invalid_value_exits_two() {
        let code = check_loaded(&doc(serde_json::json!({ "theme": "neon" })), false);
        assert_eq!(code, 2);
    }

    #[test]
    fn valid_config_exits_zero() {
        let code = check_loaded(
            &doc(serde_json::json!({ "theme": "dark", "projects": [] })),
            false,
        );
        assert_eq!(code, 0);
    }

    #[test]
    fn unreadable_state_exits_two() {
        let code = check_loaded(&Err("boom".to_string()), false);
        assert_eq!(code, 2);
    }

    #[test]
    fn bad_subcommand_exits_two() {
        assert_eq!(run(&["bogus".to_string()], false), 2);
    }

    #[test]
    fn help_exits_zero() {
        assert_eq!(run(&[], false), 0);
        assert_eq!(run(&["--help".to_string()], false), 0);
    }
}
