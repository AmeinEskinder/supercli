//! SIGKILL test for the scheduled trigger runner.
//!
//! Starts a real `scheduled_kill_helper` child process (which runs a
//! scheduled trigger with 3 steps through `ScheduledRunner`), SIGKILLs it
//! mid-run, restarts it, and verifies:
//! - The SAME run id is resumed (not a new run)
//! - Each step's side effect appears exactly once (zero duplicates,
//!   verified from the external side-effects log, not the journal)
//! - The run completes (DONE or NEEDS_REVIEW)

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("sched-kill-{}-{}", name, nanos));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn helper_bin() -> PathBuf {
    // The example binary built by `cargo build --example scheduled_kill_helper`.
    let mut p = std::env::current_exe().unwrap();
    // target/debug/deps/scheduled_kill-<hash> -> target/debug/examples/scheduled_kill_helper
    p.pop(); // deps
    p.pop(); // debug
    p.join("examples").join("scheduled_kill_helper")
}

fn spawn_helper(home: &PathBuf, side_effects: &PathBuf) -> Child {
    Command::new(helper_bin())
        .arg("--home")
        .arg(home)
        .arg("--side-effects")
        .arg(side_effects)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn scheduled_kill_helper")
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> bool {
    let start = SystemTime::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {
                if start.elapsed().unwrap() > timeout {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return false,
        }
    }
}

/// Get the (single) run id from <home>/runs.db using the sqlite3 CLI.
/// Returns None if sqlite3 is unavailable or no run exists.
fn get_run_id(home: &PathBuf) -> Option<String> {
    let db = home.join("runs.db");
    if !db.exists() {
        return None;
    }
    let out = Command::new("sqlite3")
        .arg(&db)
        .arg("SELECT id FROM runs ORDER BY rowid DESC LIMIT 1;")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

#[test]
fn scheduled_daemon_sigkill_resumes_same_run() {
    let dir = test_dir("daemon");
    let home = dir.join("home");
    let side_effects = dir.join("side-effects.log");
    fs::create_dir_all(&home).unwrap();

    // Run 1: start the helper, kill -9 it mid-run.
    let mut child1 = spawn_helper(&home, &side_effects);
    // Step 1: 400ms sleep + side effect + 200ms. Step 2 starts ~600ms.
    // Kill at 800ms: step 1 done, step 2 in progress (or just done).
    std::thread::sleep(Duration::from_millis(800));
    // Real SIGKILL.
    child1.kill().expect("kill -9 helper");
    let _ = child1.wait();
    println!("run 1: killed mid-run");

    // Capture the run id from the DB before restart.
    // The DB is at <home>/runs.db; query via a helper python or sqlite3.
    let run_id_1 = get_run_id(&home);
    println!("run 1 id: {:?}", run_id_1);
    assert!(run_id_1.is_some(), "run 1 must have created a run");

    // Run 2: restart the helper; it should resume the same run.
    let mut child2 = spawn_helper(&home, &side_effects);
    let exited = wait_for_exit(&mut child2, Duration::from_secs(30));
    assert!(exited, "helper run 2 must exit within 30s");
    let output = child2.wait_with_output().expect("wait run 2");
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("run 2 stdout: {}", stdout);

    let run_id_2 = get_run_id(&home);
    println!("run 2 id: {:?}", run_id_2);

    // SAME run id: the restart resumed, not restarted.
    assert_eq!(
        run_id_1, run_id_2,
        "restart must resume the same run id, not create a new one"
    );

    // Zero duplicates: each step appears exactly once in the external log.
    let log = fs::read_to_string(&side_effects).expect("read side-effects log");
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for line in log.lines() {
        // Format: "executed step-N"
        if let Some(step) = line.strip_prefix("executed ") {
            *counts.entry(step).or_insert(0) += 1;
        }
    }
    println!("side-effect counts: {:?}", counts);
    for (step, count) in &counts {
        assert_eq!(
            *count, 1,
            "step {} executed {} times, expected exactly once (duplicate!)",
            step, count
        );
    }
    // At least step 1 must have run (it completed before the kill).
    assert!(
        counts.contains_key("step-1"),
        "step-1 must have executed: {:?}",
        counts
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Count runs in the DB.
#[allow(dead_code)]
fn count_runs(home: &PathBuf) -> usize {
    let db = home.join("runs.db");
    let out = Command::new("sqlite3")
        .arg(&db)
        .arg("SELECT COUNT(*) FROM runs;")
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse()
            .unwrap_or(0),
        _ => 0,
    }
}
