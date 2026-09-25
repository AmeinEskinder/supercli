//! `unpeel integrations` — install Unpeel's integration with each agent CLI.
//!
//! A preset launches its command in your login shell exactly as typed; the
//! integration is what makes that agent report busy/idle/attention through
//! hooks and reach the unified `unpeel` MCP server. It is installed once per
//! Host, explicitly, into the provider's own global configuration, and kept
//! current by the Host after upgrades.

use supercli_core::integrations::install;

pub const HELP: &str = "\
unpeel integrations — Unpeel's integration with each agent CLI

  unpeel integrations [list] [--json]
  unpeel integrations install <runtime> [--project DIR] [--json]
  unpeel integrations install --all [--json]

An integration registers the runtime's lifecycle hooks and the unified
`unpeel` MCP server in that CLI's own global configuration (for example
~/.claude/settings.json and ~/.claude.json for Claude, ~/.codex/hooks.json and
~/.codex/config.toml for Codex). Launching a preset never installs anything:
a command runs in your login shell as typed. Once installed, the Host keeps
the integration current after upgrades.

<runtime> is the runtime's short name (claude, codex, gemini, …), its catalog
id, or its command. --all installs every installable runtime whose CLI is on
this Host's PATH. Amp and GitHub Copilot read hooks per project; pass
--project DIR to add the project file there.";

pub fn run(args: &[String], json: bool) -> Result<i32, String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut project: Option<&str> = None;
    let mut all = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => {}
            "--all" => all = true,
            "--project" => {
                project = Some(
                    iter.next()
                        .map(String::as_str)
                        .ok_or("--project needs a directory")?,
                );
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown integrations option {flag:?}\n\n{HELP}"));
            }
            value => positional.push(value),
        }
    }
    match positional.as_slice() {
        [] | ["list"] => {
            list(json);
            Ok(0)
        }
        ["help"] | ["--help"] | ["-h"] => {
            println!("{HELP}");
            Ok(0)
        }
        ["install"] if all => install_all(json),
        ["install", runtime] => install_one(runtime, project, json),
        _ => Err(HELP.to_string()),
    }
}

fn list(json: bool) {
    let rows = install::list();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).unwrap_or_default()
        );
        return;
    }
    for row in rows {
        let state = if !row.installable {
            "detection only"
        } else if !row.installed {
            "not installed"
        } else if row.current {
            "installed"
        } else {
            "installed (refreshing)"
        };
        let provides = match (row.lifecycle_hooks, row.mcp) {
            (true, true) => "hooks, mcp",
            (true, false) => "hooks",
            (false, true) => "mcp",
            (false, false) => "",
        };
        println!("{:<14} {:<24} {}", row.runtime, state, provides);
    }
}

fn install_one(runtime: &str, project: Option<&str>, json: bool) -> Result<i32, String> {
    let status = install::install(runtime)?;
    if let Some(dir) = project {
        install_project_file(&status.runtime, dir)?;
    }
    finish(vec![status], json);
    Ok(0)
}

fn install_all(json: bool) -> Result<i32, String> {
    let inventory = supercli_core::plugins::agents_wire();
    let present: std::collections::HashSet<String> = inventory
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["installed"] == true)
        .filter_map(|row| row["id"].as_str().map(str::to_string))
        .collect();
    let mut installed = Vec::new();
    let mut failures = Vec::new();
    for row in install::list() {
        if !row.installable || !present.contains(&row.id) {
            continue;
        }
        match install::install(&row.runtime) {
            Ok(status) => installed.push(status),
            Err(error) => failures.push(format!("{}: {error}", row.runtime)),
        }
    }
    finish(installed, json);
    if failures.is_empty() {
        Ok(0)
    } else {
        Err(failures.join("\n"))
    }
}

fn install_project_file(runtime: &str, dir: &str) -> Result<(), String> {
    let dir = std::fs::canonicalize(dir)
        .map_err(|error| format!("--project {dir}: {error}"))?
        .to_string_lossy()
        .to_string();
    match runtime {
        "amp" => supercli_core::hook_assets::prepare_amp_project_plugin(&dir),
        "copilot" => supercli_core::hook_assets::prepare_copilot_project_hooks(&dir),
        other => Err(format!(
            "{other} reads its hooks globally; --project applies to amp and github-copilot only"
        )),
    }
}

fn finish(statuses: Vec<install::IntegrationStatus>, json: bool) {
    // Bootstrap publishes integration state to every Controller; ping the
    // worker so an open Settings window refreshes without waiting for a poll.
    supercli_core::state_bus::announce(supercli_core::state_bus::Change::AppState, None);
    supercli_core::state_bus::flush();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&statuses).unwrap_or_default()
        );
        return;
    }
    for status in statuses {
        println!("installed the {} integration", status.label);
    }
}
