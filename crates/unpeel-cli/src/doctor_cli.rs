//! `unpeel doctor` — R4 operability diagnostics.
//!
//! Checks:
//! - Home-dir permissions (UNPEEL_HOME exists, writable, not world-writable)
//! - Chain integrity (verify_review_chain for all sessions)
//! - Stale leases (expired but unreleased lease rows)
//! - Clock skew (system time vs file mtimes — detects major skew)

use std::path::PathBuf;

fn unpeel_home() -> PathBuf {
    if let Some(home) = std::env::var_os("UNPEEL_HOME") {
        return PathBuf::from(home);
    }
    // Fall back to ~/.unpeel (never used in tests; tests set UNPEEL_HOME).
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".unpeel");
    }
    PathBuf::from(".unpeel")
}

/// Run all doctor checks. Returns exit code (0 = all passed).
/// With `--json`, emits a JSON report.
pub fn run(args: &[String]) -> i32 {
    let json = args.iter().any(|a| a == "--json");
    let home = unpeel_home();

    let mut checks: Vec<(&str, bool, String)> = Vec::with_capacity(5);

    // 1. Home-dir permissions.
    checks.push(check_home_permissions(&home));

    // 2. Chain integrity for all sessions.
    checks.push(check_chain_integrity(&home));

    // 3. Stale leases.
    checks.push(check_stale_leases(&home));

    // 4. Clock skew.
    checks.push(check_clock_skew(&home));

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
        // Note: the canonical review log is reviews.jsonl (not action-reviews.jsonl).
        let review_log = session_dir.join("reviews.jsonl");
        if !review_log.exists() {
            continue;
        }
        checked += 1;
        // R4: use the real verify_review_chain from unpeel-core.
        // This validates the hash chain cryptographically, not just JSON syntax.
        match unpeel_core::action_reviews::verify_review_chain(&session_dir) {
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
    if !unpeel_core::schedule_leases::leases_db_path(home).exists() {
        return ("stale-leases", true, "no lease DB".to_string());
    }
    let db = match unpeel_core::schedule_leases::ScheduleLeases::open(home, "default") {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_json_output() {
        let dir = std::env::temp_dir().join(format!("doctor-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("UNPEEL_HOME", &dir);

        // Should pass with an empty home (no sessions, no leases).
        // run() returns exit code: 0 = success.
        let code = run(&["--json".to_string()]);
        assert_eq!(code, 0, "doctor should pass on empty home");

        std::env::remove_var("UNPEEL_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn doctor_detects_unwritable_home() {
        // Non-existent home fails the check.
        // Doctor is diagnostic: it must not create state under the home,
        // so remove the dir both before and after (robust, no pollution).
        let fake_home = std::path::Path::new("/nonexistent-doctor-test-12345");
        let _ = std::fs::remove_dir_all(fake_home);
        std::env::set_var("UNPEEL_HOME", "/nonexistent-doctor-test-12345");
        let code = run(&[]);
        std::env::remove_var("UNPEEL_HOME");
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
        std::env::set_var("UNPEEL_HOME", &dir);
        let code = run(&[]);
        std::env::remove_var("UNPEEL_HOME");
        assert_eq!(code, 0, "doctor should pass on empty home");
        assert!(
            !dir.join("schedule-leases.db").exists(),
            "doctor must not create the lease DB"
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
        let db = unpeel_core::schedule_leases::ScheduleLeases::open(&dir, "default")
            .unwrap()
            .with_ttl(0);
        db.claim("crashed-worker").unwrap().unwrap();
        let stale = db.list_stale().unwrap();
        assert_eq!(stale.len(), 1, "one stale row expected: {stale:?}");

        std::env::set_var("UNPEEL_HOME", &dir);
        let code = run(&[]);
        std::env::remove_var("UNPEEL_HOME");
        assert_ne!(code, 0, "doctor should fail with a stale lease");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
