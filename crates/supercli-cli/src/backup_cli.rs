//! `unpeel backup` / `unpeel restore` — consistent, verifiable snapshots of
//! the workspace home.
//!
//! `backup` walks the home, snapshots each session's review log under its
//! log lock, snapshots `schedule-leases.db` through the SQLite
//! online-backup API, hashes every file (SHA-256), and writes one `.tar`
//! archive ending with `manifest.json`.
//!
//! `restore` verifies every file against the manifest and every staged
//! review hash chain before installing anything, refuses while a Host
//! holds the workspace serve lease, refuses to overwrite existing Unpeel
//! state without `--force`, and installs each file with tmp-file + rename.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use unpeel_core::{app_paths, backup};

pub const BACKUP_HELP: &str = "\
unpeel backup — snapshot this workspace home into a verifiable archive

  unpeel backup [--to <path>] [--json]

Writes a single .tar archive (default ./unpeel-backup-<epoch>.tar) with a
SHA-256 manifest of every file. Review logs are snapshotted under their
log lock and the lease database through the SQLite online-backup API, so
a backup is consistent even while the Host is writing.\
";

pub const RESTORE_HELP: &str = "\
unpeel restore — verify and reinstall a backup archive

  unpeel restore --from <path> [--force] [--json]

Verifies every file against the archive manifest and every staged review
hash chain before installing anything, refuses while a Host is serving
this workspace, and refuses to overwrite existing Unpeel state unless
--force is given. Each file is installed with tmp-file + rename.

Note: bare `unpeel restore <session>` (no --from) keeps its historical
meaning — restoring an archived session — and is handled by the session
commands, not here.\
";

fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Parse `--name value` / `--flag` args. Returns the value args and an
/// error string for anything unexpected.
type ParsedFlags<'a> = (Vec<(&'a str, &'a str)>, Vec<&'a str>);

fn parse_flags<'a>(
    args: &'a [String],
    value_flags: &[&str],
    bool_flags: &[&str],
) -> Result<ParsedFlags<'a>, String> {
    let mut values = Vec::new();
    let mut bools = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if value_flags.contains(&a) {
            let v = args
                .get(i + 1)
                .map(String::as_str)
                .ok_or_else(|| format!("{a} needs a value"))?;
            values.push((a, v));
            i += 2;
        } else if bool_flags.contains(&a) {
            bools.push(a);
            i += 1;
        } else {
            return Err(format!("unexpected {a:?}"));
        }
    }
    Ok((values, bools))
}

fn value<'a>(values: &[(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    values.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
}

pub fn backup_cmd(args: &[String]) -> i32 {
    if args
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        println!("{BACKUP_HELP}");
        return 0;
    }
    let (values, bools) = match parse_flags(args, &["--to"], &["--json"]) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("unpeel backup: {e}\n{BACKUP_HELP}");
            return 2;
        }
    };
    let json = bools.contains(&"--json");
    let dest: PathBuf = match value(&values, "--to") {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(format!("unpeel-backup-{}.tar", epoch_secs())),
    };
    let home = app_paths::unpeel_home();
    match backup::create_backup(&home, &dest) {
        Ok(report) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "ok": true,
                        "archive": report.archive,
                        "files": report.files,
                        "bytes": report.bytes,
                        "sessions": report.sessions,
                        "broken_chains": report.broken_chains,
                    })
                );
            } else {
                println!(
                    "backup {}: {} files, {} bytes, {} session(s)",
                    report.archive.display(),
                    report.files,
                    report.bytes,
                    report.sessions
                );
                for broken in &report.broken_chains {
                    println!("warning: broken review chain at backup time: {broken}");
                }
            }
            0
        }
        Err(e) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "ok": false, "error": e.to_string() })
                );
            } else {
                eprintln!("unpeel backup: {e}");
            }
            1
        }
    }
}

pub fn restore_cmd(args: &[String]) -> i32 {
    if args
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        println!("{RESTORE_HELP}");
        return 0;
    }
    let (values, bools) = match parse_flags(args, &["--from"], &["--json", "--force"]) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("unpeel restore: {e}\n{RESTORE_HELP}");
            return 2;
        }
    };
    let json = bools.contains(&"--json");
    let force = bools.contains(&"--force");
    let Some(from) = value(&values, "--from") else {
        eprintln!("unpeel restore: --from <path> is required\n{RESTORE_HELP}");
        return 2;
    };
    let home = app_paths::unpeel_home();
    match backup::restore_backup(Path::new(from), &home, force) {
        Ok(report) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "ok": true,
                        "home": report.home,
                        "files": report.files,
                        "sessions": report.sessions,
                        "chains_verified": report.chains_verified,
                    })
                );
            } else {
                println!(
                    "restored {} files, {} session(s), {} review chain(s) verified into {}",
                    report.files,
                    report.sessions,
                    report.chains_verified,
                    report.home.display()
                );
            }
            0
        }
        Err(e) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "ok": false, "error": e.to_string() })
                );
            } else {
                eprintln!("unpeel restore: {e}");
            }
            1
        }
    }
}
