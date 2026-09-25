//! `unpeel migrate` — one-shot upgrade of on-disk state to the current schema.
//!
//! Dry-run by default: every store is scanned, a report is printed, and
//! nothing is changed. `--apply` backs each touched file up first
//! (`<name>.pre-migrate-<epoch>.bak` beside the original), then applies.
//! A second run is a no-op: migrated state classifies as current.
//!
//! What migrates:
//! * **Connector grants** (`app-state.json` → `mcp_connector_approvals`):
//!   legacy bare-string entries predate per-connector namespacing and fail
//!   closed at runtime. They are quarantined to
//!   `mcp_connector_approvals_quarantined` (never guessed into a connector)
//!   and the operator is told re-approval is required.
//! * **Review logs** (`app-sessions/*/action-reviews.jsonl`): pre-chain logs
//!   (entries without `prev_hash`/`entry_hash`) are deterministically
//!   re-chained in file order, preserving every other field. Logs that
//!   verify are untouched; logs with chain metadata that fail verification
//!   are reported as possible tampering and never rewritten.
//! * **Schedules** (`schedules.json` + `schedule-leases.db`): the registry
//!   is backed up and preserved as-is; the lease database gets the
//!   idempotent schema init/upgrade (adds a missing `lease_generation`
//!   column). Existing lease rows are never modified — stale ownership is
//!   not resurrected.
//! * **Config** (settings subset of `app-state.json`): values that fail the
//!   typed P3 schema are removed so the documented default applies (every
//!   setting has one; missing keys are never an issue). Unknown keys are
//!   warnings only and are left alone.
//!
//! Stop the Host before `--apply`: review-log and app-state writers take
//! locks, but `schedules.json` has none and a live Host could interleave
//! with the backup.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use unpeel_core::action_reviews::{
    classify_review_log, rechain_review_log, RechainNeed, REVIEWS_FILE,
};
use unpeel_core::schedule_leases::{leases_db_path, ScheduleLeases};
use unpeel_core::scheduled::schedules_path;
use unpeel_core::{app_paths, app_state};

pub const HELP: &str = "\
unpeel migrate — upgrade on-disk state to the current schema

  unpeel migrate [--apply] [--json]

Dry-run by default: scans connector grants, review logs, and schedule
state, prints what would change, and changes nothing. With --apply, every
touched file is backed up first (<name>.pre-migrate-<epoch>.bak beside the
original), then the migration runs. A second run is a no-op.

Migrates: legacy bare-string connector grants (quarantined, re-approval
required — the connector is never guessed), pre-chain review logs
(deterministically re-chained, entries preserved), and the schedule lease
database schema (initialized/upgraded; existing lease rows untouched).
Config: settings failing the typed schema are reset to their documented
defaults (unknown keys are warnings and are left alone).
Logs that fail verification with chain metadata present are reported and
left alone. Stop the Host before --apply.\
";

/// Copy `path` to `<name>.pre-migrate-<epoch>.bak` beside the original.
fn backup(path: &Path) -> Result<PathBuf, String> {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let stem = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("backup");
    let dest = path.with_file_name(format!("{stem}.pre-migrate-{epoch}.bak"));
    std::fs::copy(path, &dest).map_err(|e| format!("backup {}: {e}", path.display()))?;
    Ok(dest)
}

/// Backup, but only if no pre-migration backup exists yet. Returns the
/// backup path and whether it was created by this call. Used for files the
/// migration never modifies (schedules registry, lease DB): the first
/// backup is the pre-migration snapshot, and re-running must not spam
/// backups.
fn backup_once(path: &Path) -> Result<(PathBuf, bool), String> {
    if let Some(parent) = path.parent() {
        if let Ok(entries) = std::fs::read_dir(parent) {
            let stem = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let prefix = format!("{stem}.pre-migrate-");
            let mut existing: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".bak"))
                })
                .collect();
            existing.sort();
            if let Some(first) = existing.into_iter().next() {
                return Ok((first, false));
            }
        }
    }
    backup(path).map(|p| (p, true))
}

fn is_bak_file(name: &str) -> bool {
    name.contains(".pre-migrate-") && name.ends_with(".bak")
}

// ---------------------------------------------------------------------------
// Step 1: connector grants
// ---------------------------------------------------------------------------

/// `(caller, tool)` pairs found as legacy bare strings.
pub struct GrantsScan {
    pub callers: usize,
    pub legacy: Vec<(String, String)>,
}

fn scan_grants(state_path: &Path) -> Result<GrantsScan, String> {
    let raw = match std::fs::read_to_string(state_path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(GrantsScan {
                callers: 0,
                legacy: Vec::new(),
            })
        }
        Err(e) => return Err(format!("read {}: {e}", state_path.display())),
    };
    let state: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", state_path.display()))?;
    let mut scan = GrantsScan {
        callers: 0,
        legacy: Vec::new(),
    };
    if let Some(map) = state
        .get("mcp_connector_approvals")
        .and_then(|m| m.as_object())
    {
        scan.callers = map.len();
        for (caller, list) in map {
            if let Some(items) = list.as_array() {
                for item in items {
                    if let Some(tool) = item.as_str() {
                        scan.legacy.push((caller.clone(), tool.to_string()));
                    }
                }
            }
        }
    }
    scan.legacy.sort();
    Ok(scan)
}

/// Quarantine legacy bare-string grants at an explicit state path.
/// Returns the number quarantined.
fn quarantine_grants_at(state_path: &Path) -> Result<usize, String> {
    app_state::edit_at(state_path, |root| {
        let approvals = root
            .entry("mcp_connector_approvals")
            .or_insert_with(|| serde_json::json!({}));
        let Some(map) = approvals.as_object_mut() else {
            return Ok(0);
        };
        let mut quarantined = 0usize;
        // Collect first: one pass per caller, no borrow games.
        let mut moves: Vec<(String, Vec<String>)> = Vec::new();
        for (caller, list) in map.iter_mut() {
            let Some(items) = list.as_array_mut() else {
                continue;
            };
            let bare: Vec<String> = items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            if bare.is_empty() {
                continue;
            }
            items.retain(|v| !v.is_string());
            moves.push((caller.clone(), bare));
        }
        if moves.is_empty() {
            return Ok(0);
        }
        let qmap = root
            .entry("mcp_connector_approvals_quarantined")
            .or_insert_with(|| serde_json::json!({}));
        let Some(qobj) = qmap.as_object_mut() else {
            return Err("mcp_connector_approvals_quarantined is not an object".to_string());
        };
        for (caller, bare) in moves {
            let entry = qobj.entry(caller).or_insert_with(|| serde_json::json!([]));
            if let Some(qlist) = entry.as_array_mut() {
                for tool in bare {
                    let v = serde_json::Value::String(tool);
                    if !qlist.contains(&v) {
                        qlist.push(v);
                        quarantined += 1;
                    }
                }
            }
        }
        Ok(quarantined)
    })
}

// ---------------------------------------------------------------------------
// Step 2: review logs
// ---------------------------------------------------------------------------

pub struct LogStatus {
    pub session: String,
    pub need: RechainNeed,
}

fn scan_review_logs(home: &Path) -> Vec<LogStatus> {
    let mut out = Vec::new();
    let root = home.join("app-sessions");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return out;
    };
    let mut sessions: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| !e.file_name().to_str().is_some_and(is_bak_file))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    sessions.sort();
    for session in sessions {
        let dir = root.join(&session);
        if !dir.join(REVIEWS_FILE).exists() {
            continue;
        }
        out.push(LogStatus {
            session,
            need: classify_review_log(&dir),
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Step 3: schedules
// ---------------------------------------------------------------------------

pub struct SchedulesScan {
    pub registry_present: bool,
    pub registry_schedules: Option<usize>,
    pub registry_parse_error: Option<String>,
    pub leases_db_present: bool,
}

fn scan_schedules(home: &Path) -> SchedulesScan {
    let reg = schedules_path(home);
    let mut scan = SchedulesScan {
        registry_present: reg.exists(),
        registry_schedules: None,
        registry_parse_error: None,
        leases_db_present: leases_db_path(home).exists(),
    };
    if scan.registry_present {
        match std::fs::read_to_string(&reg) {
            Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(v) => {
                    scan.registry_schedules = v.as_array().map(|a| a.len());
                }
                Err(e) => scan.registry_parse_error = Some(e.to_string()),
            },
            Err(e) => scan.registry_parse_error = Some(e.to_string()),
        }
    }
    scan
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

struct StepOutcome {
    title: &'static str,
    lines: Vec<String>,
    changed: bool,
    error: Option<String>,
}

fn step_grants(apply: bool) -> StepOutcome {
    let path = app_paths::app_state_path();
    step_grants_at(&path, apply)
}

fn step_grants_at(state_path: &Path, apply: bool) -> StepOutcome {
    let mut step = StepOutcome {
        title: "connector grants",
        lines: Vec::new(),
        changed: false,
        error: None,
    };
    match scan_grants(state_path) {
        Ok(scan) => {
            step.lines.push(format!(
                "scanned {} caller(s) in mcp_connector_approvals",
                scan.callers
            ));
            if scan.legacy.is_empty() {
                step.lines.push("no legacy bare-string grants".to_string());
                return step;
            }
            for (caller, tool) in &scan.legacy {
                step.lines
                    .push(format!("legacy grant: caller {caller:?}, tool {tool:?}"));
            }
            if !apply {
                step.lines.push(format!(
                    "would quarantine {} legacy grant(s) \
                     (re-approval required; connector is never guessed)",
                    scan.legacy.len()
                ));
                return step;
            }
            match backup(state_path).and_then(|b| quarantine_grants_at(state_path).map(|n| (b, n)))
            {
                Ok((bak, n)) => {
                    step.changed = true;
                    step.lines.push(format!(
                        "backed up {} to {}",
                        state_path.display(),
                        bak.display()
                    ));
                    step.lines.push(format!(
                        "quarantined {n} legacy grant(s) to \
                         mcp_connector_approvals_quarantined; re-approval required"
                    ));
                }
                Err(e) => step.error = Some(e),
            }
        }
        Err(e) => step.error = Some(e),
    }
    step
}

fn step_review_logs(home: &Path, apply: bool) -> StepOutcome {
    let mut step = StepOutcome {
        title: "review logs",
        lines: Vec::new(),
        changed: false,
        error: None,
    };
    let statuses = scan_review_logs(home);
    if statuses.is_empty() {
        step.lines
            .push("no action-reviews.jsonl files found".to_string());
        return step;
    }
    for status in statuses {
        let dir = home.join("app-sessions").join(&status.session);
        match &status.need {
            RechainNeed::Current(n) => {
                step.lines
                    .push(format!("{}: current ({n} entries)", status.session));
            }
            RechainNeed::PreChain(n) => {
                if !apply {
                    step.lines.push(format!(
                        "{}: pre-chain ({n} entries) — would back up and re-chain",
                        status.session
                    ));
                    continue;
                }
                if *n == 0 {
                    step.lines
                        .push(format!("{}: empty log, nothing to do", status.session));
                    continue;
                }
                let log_path = dir.join(REVIEWS_FILE);
                match backup(&log_path).and_then(|bak| {
                    rechain_review_log(&dir)
                        .map(|count| (bak, count))
                        .map_err(|e| format!("{e:?}"))
                }) {
                    Ok((bak, count)) => {
                        step.changed = true;
                        step.lines.push(format!(
                            "{}: backed up to {}, re-chained {count} entries (verified)",
                            status.session,
                            bak.file_name().and_then(|s| s.to_str()).unwrap_or("backup")
                        ));
                    }
                    Err(e) => {
                        step.error = Some(format!("{}: rechain failed: {e}", status.session));
                    }
                }
            }
            RechainNeed::Broken(e) => {
                step.lines.push(format!(
                    "{}: BROKEN CHAIN ({e:?}) — possible tampering; left untouched",
                    status.session
                ));
            }
        }
    }
    step
}

fn step_schedules(home: &Path, apply: bool) -> StepOutcome {
    let mut step = StepOutcome {
        title: "schedules",
        lines: Vec::new(),
        changed: false,
        error: None,
    };
    let scan = scan_schedules(home);
    let reg = schedules_path(home);
    if !scan.registry_present {
        step.lines.push("no schedules.json".to_string());
    } else {
        match (scan.registry_schedules, scan.registry_parse_error) {
            (Some(n), _) => step.lines.push(format!("schedules.json: {n} schedule(s)")),
            (_, Some(e)) => {
                step.error = Some(format!("schedules.json does not parse: {e}"));
                return step;
            }
            _ => step
                .lines
                .push("schedules.json: present (unrecognized shape)".to_string()),
        }
        if apply {
            match backup_once(&reg) {
                Ok((bak, created)) => {
                    let name = bak.file_name().and_then(|s| s.to_str()).unwrap_or("backup");
                    if created {
                        step.changed = true;
                        step.lines
                            .push(format!("backed up schedules.json to {name}"));
                    } else {
                        step.lines
                            .push(format!("pre-migration backup already present ({name})"));
                    }
                }
                Err(e) => {
                    step.error = Some(e);
                    return step;
                }
            }
        } else {
            step.lines
                .push("would back up schedules.json (registry preserved as-is)".to_string());
        }
    }
    let db = leases_db_path(home);
    if !scan.leases_db_present {
        step.lines.push("no schedule-leases.db".to_string());
        if apply {
            step.lines
                .push("initializing empty lease database (current schema)".to_string());
        } else {
            step.lines
                .push("would initialize lease database schema".to_string());
            return step;
        }
    } else if !apply {
        step.lines
            .push("would back up schedule-leases.db and run idempotent schema upgrade".to_string());
        return step;
    } else {
        match backup_once(&db) {
            Ok((bak, created)) => {
                let name = bak.file_name().and_then(|s| s.to_str()).unwrap_or("backup");
                if created {
                    step.changed = true;
                    step.lines
                        .push(format!("backed up schedule-leases.db to {name}"));
                } else {
                    step.lines
                        .push(format!("pre-migration backup already present ({name})"));
                }
            }
            Err(e) => {
                step.error = Some(e);
                return step;
            }
        }
    }
    // Idempotent schema init/upgrade. Existing rows are never modified:
    // CREATE TABLE IF NOT EXISTS plus a conditional ADD COLUMN; no
    // ownership is resurrected.
    match ScheduleLeases::open(home, "migrate") {
        Ok(store) => {
            drop(store);
            step.lines
                .push("lease schema ensured (idempotent init/upgrade)".to_string());
        }
        Err(e) => step.error = Some(format!("lease schema init failed: {e:?}")),
    }
    step
}

fn print_human(apply: bool, steps: &[StepOutcome]) {
    if apply {
        println!("unpeel migrate --apply");
    } else {
        println!("unpeel migrate (dry-run; pass --apply to change anything)\n");
    }
    for step in steps {
        println!("[{}]", step.title);
        for line in &step.lines {
            println!("  {line}");
        }
        if let Some(e) = &step.error {
            println!("  ERROR: {e}");
        }
    }
    let changed = steps.iter().any(|s| s.changed);
    let failed = steps.iter().any(|s| s.error.is_some());
    println!();
    if failed {
        println!("migration finished with errors");
    } else if apply && changed {
        println!("migration applied");
    } else if apply {
        println!("nothing to migrate");
    } else {
        println!("dry-run complete; nothing was changed");
    }
}

fn print_json(apply: bool, steps: &[StepOutcome]) {
    let steps_json: Vec<serde_json::Value> = steps
        .iter()
        .map(|s| {
            serde_json::json!({
                "title": s.title,
                "lines": s.lines,
                "changed": s.changed,
                "error": s.error,
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::json!({
            "dry_run": !apply,
            "steps": steps_json,
        })
    );
}

/// Remove a dotted path (e.g. `theme` or `experimental_features.foo`) from a
/// JSON document. Returns true when something was removed.
fn remove_dotted_path(doc: &mut serde_json::Value, path: &str) -> bool {
    let mut parts: Vec<&str> = path.split('.').collect();
    let last = match parts.pop() {
        Some(l) => l,
        None => return false,
    };
    let mut cur = doc;
    for part in parts {
        match cur.get_mut(part) {
            Some(serde_json::Value::Object(_)) => {
                cur = cur.get_mut(part).unwrap();
            }
            _ => return false,
        }
    }
    match cur {
        serde_json::Value::Object(map) => map.remove(last).is_some(),
        _ => false,
    }
}

fn step_config(apply: bool) -> StepOutcome {
    let mut step = StepOutcome {
        title: "config",
        lines: Vec::new(),
        changed: false,
        error: None,
    };
    let path = app_paths::app_state_path();
    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            step.lines.push("no app-state.json; nothing to check".to_string());
            return step;
        }
        Err(e) => {
            step.error = Some(format!("read {}: {e}", path.display()));
            return step;
        }
    };
    let mut doc: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(d) => d,
        Err(e) => {
            step.error = Some(format!("parse {}: {e}", path.display()));
            return step;
        }
    };
    let report = unpeel_core::config::check_document(&doc);
    for w in &report.warnings {
        step.lines
            .push(format!("warning: {}: {}", w.path, w.message));
    }
    if report.errors.is_empty() {
        step.lines.push("config valid".to_string());
        return step;
    }
    for e in &report.errors {
        step.lines.push(format!("invalid: {}: {}", e.path, e.message));
    }
    if !apply {
        step.lines.push(format!(
            "would reset {} invalid setting(s) to their defaults",
            report.errors.len()
        ));
        return step;
    }
    match backup(&path) {
        Ok(bak) => {
            step.lines.push(format!(
                "backed up {} to {}",
                path.display(),
                bak.display()
            ));
        }
        Err(e) => {
            step.error = Some(e);
            return step;
        }
    }
    let mut removed = 0;
    for e in &report.errors {
        if remove_dotted_path(&mut doc, &e.path) {
            removed += 1;
        }
    }
    // Re-check: the document must be valid after the reset.
    let after = unpeel_core::config::check_document(&doc);
    if !after.errors.is_empty() {
        step.error = Some(format!(
            "config still invalid after reset: {}",
            after
                .errors
                .iter()
                .map(|e| format!("{}: {}", e.path, e.message))
                .collect::<Vec<_>>()
                .join("; ")
        ));
        return step;
    }
    match std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()) {
        Ok(()) => {
            step.changed = true;
            step.lines.push(format!(
                "reset {removed} invalid setting(s) to defaults"
            ));
        }
        Err(e) => step.error = Some(format!("write {}: {e}", path.display())),
    }
    step
}

pub fn run(args: &[String]) -> i32 {
    if args
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        println!("{HELP}");
        return 0;
    }
    let apply = args.iter().any(|a| a == "--apply");
    let json = args.iter().any(|a| a == "--json");
    for a in args {
        if a != "--apply" && a != "--json" {
            eprintln!("unknown migrate flag {a:?}\n{HELP}");
            return 2;
        }
    }
    let home = app_paths::unpeel_home();
    let steps = vec![
        step_grants(apply),
        step_review_logs(&home, apply),
        step_schedules(&home, apply),
        step_config(apply),
    ];
    if json {
        print_json(apply, &steps);
    } else {
        print_human(apply, &steps);
    }
    if steps.iter().any(|s| s.error.is_some()) {
        1
    } else {
        0
    }
}

#[cfg(test)]
#[path = "migrate_cli_tests.rs"]
mod tests;
