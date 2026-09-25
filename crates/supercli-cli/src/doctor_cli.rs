//! `unpeel doctor` — R4 operability diagnostics.
//!
//! Checks:
//! - Home-dir permissions (SUPERCLI_HOME exists, writable, not world-writable)
//! - Chain integrity (verify_review_chain for all sessions)
//! - Stale leases (expired but unreleased lease rows)
//! - Clock skew (system time vs file mtimes — detects major skew)

use std::path::PathBuf;

fn supercli_home() -> PathBuf {
    if let Some(home) = std::env::var_os("SUPERCLI_HOME") {
        return PathBuf::from(home);
    }
    // Fall back to ~/.supercli (never used in tests; tests set SUPERCLI_HOME).
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".supercli");
    }
    PathBuf::from(".supercli")
}

/// Run all doctor checks. Returns exit code (0 = all passed).
/// With `--json`, emits a JSON report.
/// Run the checks without printing. Shared by `run` and `unpeel init`
/// (which merges the doctor report into its own JSON output).
pub fn run_checks() -> (PathBuf, Vec<(&'static str, bool, String)>) {
    let home = supercli_home();

    // 1. Home-dir permissions. 2. Chain integrity. 3. Stale leases. 4. Clock skew.
    // 5. Grants file (S2: sharded from app-state.json).
    // 6. Grant audit (Phase 13 v2: grants ⊆ chain).
    let checks: Vec<(&str, bool, String)> = vec![
        check_home_permissions(&home),
        check_chain_integrity(&home),
        check_stale_leases(&home),
        check_clock_skew(&home),
        check_grants_file(&home),
        check_grant_audit(&home),
    ];

    (home, checks)
}

pub fn run(args: &[String]) -> i32 {
    let json = args.iter().any(|a| a == "--json");
    let bundle_idx = args.iter().position(|a| a == "--bundle");
    if let Some(idx) = bundle_idx {
        let output = args
            .get(idx + 1)
            .map(|s| s.as_str())
            .unwrap_or("unpeel-doctor-bundle.tar.gz");
        let output_path = std::path::PathBuf::from(output);
        let (home, _) = run_checks();
        match build_bundle(&home, &output_path) {
            Ok(()) => {
                println!("bundle written to {}", output_path.display());
                return 0;
            }
            Err(e) => {
                eprintln!("bundle failed: {e}");
                return 1;
            }
        }
    }
    let (home, checks) = run_checks();

    let passed = checks.iter().filter(|(_, ok, _)| *ok).count();
    let failed = checks.len() - passed;

    if json {
        let report = serde_json::json!({
            "home": home.to_string_lossy(),
            "passed": passed,
            "failed": failed,
            "checks": checks
                .iter()
                .map(|(name, ok, detail)| {
                    serde_json::json!({
                        "name": name,
                        "ok": ok,
                        "detail": detail,
                    })
                })
                .collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("unpeel doctor — home: {}", home.display());
        for (name, ok, detail) in &checks {
            let status = if *ok { "OK  " } else { "FAIL" };
            println!("  [{status}] {name}: {detail}");
        }
        println!("{passed} passed, {failed} failed");
    }

    if failed > 0 {
        eprintln!("{failed} check(s) failed");
        1
    } else {
        0
    }
}

fn check_home_permissions(home: &PathBuf) -> (&'static str, bool, String) {
    use std::os::unix::fs::PermissionsExt;

    if !home.exists() {
        return (
            "home-dir",
            false,
            format!("{} does not exist", home.display()),
        );
    }
    let Ok(metadata) = std::fs::metadata(home) else {
        return ("home-dir", false, "cannot read metadata".to_string());
    };
    if !metadata.is_dir() {
        return ("home-dir", false, "not a directory".to_string());
    }

    // Writable?
    let test_path = home.join(".doctor-write-test");
    let writable = std::fs::write(&test_path, b"test").is_ok();
    let _ = std::fs::remove_file(&test_path);
    if !writable {
        return ("home-dir", false, "not writable".to_string());
    }

    // Not world-writable? (security)
    let mode = metadata.permissions().mode();
    if mode & 0o002 != 0 {
        return (
            "home-dir",
            false,
            format!("world-writable (mode {:o})", mode & 0o777),
        );
    }

    (
        "home-dir",
        true,
        format!("writable, mode {:o}", mode & 0o777),
    )
}

fn check_chain_integrity(home: &std::path::Path) -> (&'static str, bool, String) {
    let sessions_dir = home.join("sessions");
    let Ok(entries) = std::fs::read_dir(&sessions_dir) else {
        return ("chain-integrity", true, "no sessions dir".to_string());
    };

    let mut checked = 0;
    let mut broken: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let session_dir = entry.path();
        if !session_dir.is_dir() {
            continue;
        }
        // Only check if there's a review log.
        let review_log = session_dir.join(supercli_core::action_reviews::REVIEWS_FILE);
        if !review_log.exists() {
            continue;
        }
        checked += 1;
        // R4: use the real verify_review_chain from unpeel-core.
        // This validates the hash chain cryptographically, not just JSON syntax.
        match supercli_core::action_reviews::verify_review_chain(&session_dir) {
            Ok(count) => {
                // Chain verifies; count is the number of entries.
                let _ = count;
            }
            Err(e) => {
                broken.push(format!(
                    "{}: chain verification failed: {e:?}",
                    entry.file_name().to_string_lossy()
                ));
            }
        }
    }

    if broken.is_empty() {
        (
            "chain-integrity",
            true,
            format!("{checked} session(s) checked"),
        )
    } else {
        ("chain-integrity", false, broken.join("; "))
    }
}

fn check_stale_leases(home: &std::path::Path) -> (&'static str, bool, String) {
    // R4: query the real lease DB for stale leases (expired but not released).
    // A stale lease is one where expires_at_ms <= now AND owner is not empty.
    // These indicate a crashed worker that didn't release its lease.
    //
    // Doctor is diagnostic: never create the DB as a side effect. If the
    // file doesn't exist, there are no leases at all.
    if !supercli_core::schedule_leases::leases_db_path(home).exists() {
        return ("stale-leases", true, "no lease DB".to_string());
    }
    let db = match supercli_core::schedule_leases::ScheduleLeases::open(home, "default") {
        Ok(db) => db,
        Err(e) => return ("stale-leases", false, format!("DB error: {e:?}")),
    };

    // Genuine stale detection: expired-but-owned rows, not just active holders.
    match db.list_stale() {
        Ok(stale) => {
            if stale.is_empty() {
                ("stale-leases", true, "no stale leases".to_string())
            } else {
                (
                    "stale-leases",
                    false,
                    format!(
                        "{} stale lease(s): {}",
                        stale.len(),
                        stale
                            .iter()
                            .map(|(id, owner, _)| format!("{id} (owner {owner})"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )
            }
        }
        Err(e) => ("stale-leases", false, format!("DB error: {e:?}")),
    }
}

fn check_clock_skew(home: &PathBuf) -> (&'static str, bool, String) {
    // Compare system time to the mtime of the home dir.
    // If the home dir was modified in the future, the clock may be skewed.
    let Ok(metadata) = std::fs::metadata(home) else {
        return ("clock-skew", true, "cannot read home mtime".to_string());
    };
    let Ok(mtime) = metadata.modified() else {
        return ("clock-skew", true, "cannot read mtime".to_string());
    };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return ("clock-skew", true, "cannot read system time".to_string());
    };
    let Ok(mtime_secs) = mtime.duration_since(std::time::UNIX_EPOCH) else {
        return ("clock-skew", true, "mtime before epoch".to_string());
    };

    // If mtime is more than 1 hour in the future, flag it.
    if mtime_secs.as_secs() > now.as_secs() + 3600 {
        return (
            "clock-skew",
            false,
            format!(
                "home mtime is {}s in the future (clock skew?)",
                mtime_secs.as_secs() - now.as_secs()
            ),
        );
    }

    ("clock-skew", true, "no significant skew".to_string())
}

/// S2: Check the sharded grants file (grants.json) is valid JSON.
/// A corrupt grants file fails closed (grants are re-requested).
fn check_grants_file(home: &PathBuf) -> (&'static str, bool, String) {
    let path = home.join("grants.json");
    if !path.exists() {
        return (
            "grants-file",
            true,
            "no grants file (no grants persisted yet)".to_string(),
        );
    }
    match std::fs::read(&path) {
        Ok(raw) => match serde_json::from_slice::<serde_json::Value>(&raw) {
            Ok(v) => {
                if v.is_object() {
                    ("grants-file", true, "valid".to_string())
                } else {
                    (
                        "grants-file",
                        false,
                        "grants.json is not a JSON object".to_string(),
                    )
                }
            }
            Err(e) => (
                "grants-file",
                false,
                format!("grants.json parse error: {e}"),
            ),
        },
        Err(e) => (
            "grants-file",
            false,
            format!("cannot read grants.json: {e}"),
        ),
    }
}

/// Phase 13 v2: Check grant audit chain (grants ⊆ chain).
/// Every grant in grants.json must have a corresponding grant_created entry
/// in the tamper-evident audit log. A grant without an audit entry is a
/// security violation (quarantined on startup).
fn check_grant_audit(_home: &PathBuf) -> (&'static str, bool, String) {
    // Verify the audit chain integrity.
    match supercli_core::grant_audit::verify_grant_audit() {
        Ok(count) => {
            // Check grants ⊆ chain.
            match supercli_core::grant_audit::doctor_check_grants_subset() {
                Ok(()) => (
                    "grant-audit",
                    true,
                    format!("{count} entries verified, grants ⊆ chain"),
                ),
                Err(e) => ("grant-audit", false, e),
            }
        }
        Err(e) => (
            "grant-audit",
            false,
            format!("audit chain verification failed: {e}"),
        ),
    }
}

/// Keys whose values are secrets. Matched case-insensitively against the
/// final path segment. Values for these keys are replaced with
/// "[REDACTED]" in the bundle.
const SECRET_KEYS: &[&str] = &[
    "token",
    "secret",
    "password",
    "api_key",
    "apikey",
    "private_key",
    "privatekey",
    "seed",
    "mnemonic",
    "pairing_code",
    "pairing_secret",
];

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    SECRET_KEYS.iter().any(|s| lower.contains(s))
}

/// Recursively redact secret values from a JSON document.
fn redact_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if is_secret_key(k) {
                    *v = serde_json::Value::String("[REDACTED]".to_string());
                } else {
                    redact_value(v);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                redact_value(v);
            }
        }
        _ => {}
    }
}

/// Build the diagnostics bundle. Returns the path to the created archive.
fn build_bundle(home: &std::path::Path, output: &std::path::Path) -> Result<(), String> {
    let staging = std::env::temp_dir().join(format!("unpeel-bundle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| format!("create staging dir: {e}"))?;

    // versions.json
    let versions = serde_json::json!({
        "supercli": env!("CARGO_PKG_VERSION"),
        "rustc": rustc_version(),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    });
    std::fs::write(
        staging.join("versions.json"),
        serde_json::to_string_pretty(&versions).unwrap(),
    )
    .map_err(|e| format!("write versions.json: {e}"))?;

    // config.redacted.json
    let state_path = home.join("app-state.json");
    let mut config = if state_path.exists() {
        serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(&state_path)
                .map_err(|e| format!("read app-state.json: {e}"))?,
        )
        .map_err(|e| format!("parse app-state.json: {e}"))?
    } else {
        serde_json::json!({})
    };
    redact_value(&mut config);
    std::fs::write(
        staging.join("config.redacted.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .map_err(|e| format!("write config.redacted.json: {e}"))?;

    // doctor.json
    let (_, checks) = run_checks();
    let passed = checks.iter().filter(|(_, ok, _)| *ok).count();
    let doctor_report = serde_json::json!({
        "passed": passed,
        "failed": checks.len() - passed,
        "checks": checks
            .iter()
            .map(|(name, ok, detail)| {
                serde_json::json!({"name": name, "ok": ok, "detail": detail})
            })
            .collect::<Vec<_>>(),
    });
    std::fs::write(
        staging.join("doctor.json"),
        serde_json::to_string_pretty(&doctor_report).unwrap(),
    )
    .map_err(|e| format!("write doctor.json: {e}"))?;

    // stats.json: lease/chain statistics (no payload contents)
    let stats = collect_stats(home)?;
    std::fs::write(
        staging.join("stats.json"),
        serde_json::to_string_pretty(&stats).unwrap(),
    )
    .map_err(|e| format!("write stats.json: {e}"))?;

    // logs.jsonl: recent JSON log lines, redacted
    collect_logs(home, &staging.join("logs.jsonl"))?;

    // Create the tar.gz archive
    create_tar_gz(&staging, output)?;

    let _ = std::fs::remove_dir_all(&staging);
    Ok(())
}

fn rustc_version() -> String {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".to_string())
        .trim()
        .to_string()
}

fn collect_stats(home: &std::path::Path) -> Result<serde_json::Value, String> {
    let mut sessions = 0;
    let mut total_reviews = 0;
    let mut chains_ok = 0;
    let mut chains_failed = 0;

    let sessions_dir = home.join("app-sessions");
    if sessions_dir.exists() {
        for entry in
            std::fs::read_dir(&sessions_dir).map_err(|e| format!("read app-sessions: {e}"))?
        {
            let entry = entry.map_err(|e| format!("read dir entry: {e}"))?;
            if !entry
                .file_type()
                .map_err(|e| format!("file type: {e}"))?
                .is_dir()
            {
                continue;
            }
            sessions += 1;
            let log_path = entry.path().join("action-reviews.jsonl");
            if log_path.exists() {
                let content = std::fs::read_to_string(&log_path)
                    .map_err(|e| format!("read review log: {e}"))?;
                let count = content.lines().filter(|l| !l.trim().is_empty()).count();
                total_reviews += count;
                // Verify chain (metadata only, no payload)
                match supercli_core::action_reviews::verify_review_bytes(content.as_bytes(), "bundle")
                {
                    Ok(_) => chains_ok += 1,
                    Err(_) => chains_failed += 1,
                }
            }
        }
    }

    // Lease statistics
    let (leases_total, leases_stale) = collect_lease_stats(home);

    Ok(serde_json::json!({
        "sessions": sessions,
        "total_reviews": total_reviews,
        "chains_verified": chains_ok,
        "chains_failed": chains_failed,
        "leases": {
            "total": leases_total,
            "stale": leases_stale,
        },
    }))
}

fn collect_lease_stats(home: &std::path::Path) -> (usize, usize) {
    let db_path = home.join("schedule-leases.db");
    if !db_path.exists() {
        return (0, 0);
    }
    // Use the ScheduleLeases API if available; fall back to 0 on error.
    match supercli_core::schedule_leases::ScheduleLeases::open(home, "default") {
        Ok(db) => {
            let total = db.list_holders().map(|v| v.len()).unwrap_or(0);
            let stale = db.list_stale().map(|v| v.len()).unwrap_or(0);
            (total, stale)
        }
        Err(_) => (0, 0),
    }
}

fn collect_logs(home: &std::path::Path, output: &std::path::Path) -> Result<(), String> {
    // Collect recent JSON log lines from known log locations, redacted.
    // We take the last 100 lines from each log file found.
    //
    // SECURITY: action-reviews.jsonl and grant-audit.jsonl are EXCLUDED.
    // They contain review payloads and command text which must not appear
    // in diagnostic bundles. Chain statistics are in stats.json instead.
    const EXCLUDED: &[&str] = &["action-reviews.jsonl", "grant-audit.jsonl"];
    let log_dirs = [home.join("logs"), home.to_path_buf()];
    let mut out = std::fs::File::create(output).map_err(|e| format!("create logs.jsonl: {e}"))?;
    use std::io::Write;

    for dir in &log_dirs {
        if !dir.exists() {
            continue;
        }
        let entries = std::fs::read_dir(dir).map_err(|e| format!("read log dir: {e}"))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("log dir entry: {e}"))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            // Skip security-sensitive logs.
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if EXCLUDED.contains(&name) {
                    continue;
                }
            }
            if let Ok(content) = std::fs::read(&path) {
                // Take last 100 lines
                let lines: Vec<&[u8]> = content
                    .split(|&b| b == b'\n')
                    .filter(|l| !l.is_empty())
                    .collect();
                let start = lines.len().saturating_sub(100);
                for line in &lines[start..] {
                    if let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(line) {
                        redact_value(&mut value);
                        let redacted = serde_json::to_string(&value).unwrap();
                        writeln!(out, "{redacted}").map_err(|e| format!("write logs: {e}"))?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn create_tar_gz(staging: &std::path::Path, output: &std::path::Path) -> Result<(), String> {
    // Use the `tar` command for simplicity (available on all Unix).
    let status = std::process::Command::new("tar")
        .arg("-czf")
        .arg(output)
        .arg("-C")
        .arg(staging)
        .arg(".")
        .status()
        .map_err(|e| format!("run tar: {e}"))?;
    if !status.success() {
        return Err("tar failed".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_json_output() {
        let dir = std::env::temp_dir().join(format!("doctor-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("SUPERCLI_HOME", &dir);

        // Should pass with an empty home (no sessions, no leases).
        // run() returns exit code: 0 = success.
        let code = run(&["--json".to_string()]);
        assert_eq!(code, 0, "doctor should pass on empty home");

        std::env::remove_var("SUPERCLI_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn doctor_detects_unwritable_home() {
        // Non-existent home fails the check.
        // Doctor is diagnostic: it must not create state under the home,
        // so remove the dir both before and after (robust, no pollution).
        let fake_home = std::path::Path::new("/nonexistent-doctor-test-12345");
        let _ = std::fs::remove_dir_all(fake_home);
        std::env::set_var("SUPERCLI_HOME", "/nonexistent-doctor-test-12345");
        let code = run(&[]);
        std::env::remove_var("SUPERCLI_HOME");
        let _ = std::fs::remove_dir_all(fake_home);
        assert_ne!(code, 0, "doctor should fail on missing home");
    }

    #[test]
    fn doctor_does_not_create_lease_db() {
        // R4: doctor is read-only diagnostics. Running it on an empty home
        // must not create the lease DB as a side effect.
        let dir = std::env::temp_dir().join(format!("doctor-nodb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("SUPERCLI_HOME", &dir);
        let code = run(&[]);
        std::env::remove_var("SUPERCLI_HOME");
        assert_eq!(code, 0, "doctor should pass on empty home");
        assert!(
            !dir.join("schedule-leases.db").exists(),
            "doctor must not create the lease DB"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bundle_excludes_review_payloads_and_command_text() {
        // Planted assertion: doctor --bundle must not include review payloads
        // or command text from action-reviews.jsonl or grant-audit.jsonl.
        let dir = std::env::temp_dir().join(format!("doctor-bundle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Plant sensitive content in the excluded files.
        let secret_cmd = "PLANTED_SECRET_COMMAND_rm_-rf_/";
        let review_line = serde_json::json!({
            "review_id": "r1",
            "tool": "shell",
            "command": secret_cmd,
            "payload": {"secret": "data"},
        });
        std::fs::write(
            dir.join("action-reviews.jsonl"),
            serde_json::to_string(&review_line).unwrap(),
        )
        .unwrap();
        let audit_line = serde_json::json!({
            "grant_key": "write:PLANTED_AUDIT_SECRET",
            "actor": "human:test",
        });
        std::fs::write(
            dir.join("grant-audit.jsonl"),
            serde_json::to_string(&audit_line).unwrap(),
        )
        .unwrap();
        // A benign log that SHOULD be included.
        std::fs::write(dir.join("other.jsonl"), r#"{"msg":"hello"}"#).unwrap();

        let out_path = dir.join("logs.jsonl");
        collect_logs(&dir, &out_path).unwrap();
        let content = std::fs::read_to_string(&out_path).unwrap();

        assert!(
            !content.contains("PLANTED_SECRET_COMMAND"),
            "bundle logs must not contain review command text"
        );
        assert!(
            !content.contains("PLANTED_AUDIT_SECRET"),
            "bundle logs must not contain audit grant keys"
        );
        assert!(
            content.contains("hello"),
            "bundle logs should include benign logs"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn doctor_reports_stale_lease() {
        // R4: an expired-but-owned lease row is genuinely detected and fails
        // the stale-leases check. with_ttl(0) makes the claim expire at once.
        let dir = std::env::temp_dir().join(format!("doctor-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = supercli_core::schedule_leases::ScheduleLeases::open(&dir, "default")
            .unwrap()
            .with_ttl(0);
        db.claim("crashed-worker").unwrap().unwrap();
        let stale = db.list_stale().unwrap();
        assert_eq!(stale.len(), 1, "one stale row expected: {stale:?}");

        std::env::set_var("SUPERCLI_HOME", &dir);
        let code = run(&[]);
        std::env::remove_var("SUPERCLI_HOME");
        assert_ne!(code, 0, "doctor should fail with a stale lease");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
