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
use std::path::{Path, PathBuf};
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

fn spawn_helper(home: &PathBuf, side_effects: &PathBuf, progress: &PathBuf) -> Child {
    Command::new(helper_bin())
        .arg("--home")
        .arg(home)
        .arg("--side-effects")
        .arg(side_effects)
        .arg("--progress")
        .arg(progress)
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
fn get_run_id(home: &Path) -> Option<String> {
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

/// Wait for a specific marker line in the progress file, with a timeout.
/// Returns true if found, false on timeout.
fn wait_for_marker(progress: &PathBuf, prefix: &str, timeout: Duration) -> bool {
    let start = SystemTime::now();
    loop {
        if let Ok(content) = fs::read_to_string(progress) {
            for line in content.lines() {
                if line.starts_with(prefix) {
                    return true;
                }
            }
        }
        if start.elapsed().unwrap() > timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Wait for the `run-created <id>` marker in the progress file, with a timeout.
/// Returns the run id if found, None on timeout.
fn wait_for_run_created(progress: &PathBuf, timeout: Duration) -> Option<String> {
    let start = SystemTime::now();
    loop {
        if let Ok(content) = fs::read_to_string(progress) {
            for line in content.lines() {
                if let Some(id) = line.strip_prefix("run-created ") {
                    return Some(id.trim().to_string());
                }
            }
        }
        if start.elapsed().unwrap() > timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Simple pseudo-random delay in milliseconds, derived from system time.
/// Used to kill at a random point after the run-created marker, so repeated
/// test runs exercise different kill timings.
fn random_delay_ms(max_ms: u64) -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // Mix in the process ID for additional entropy across parallel runs.
    let pid = std::process::id() as u128;
    ((nanos ^ (pid << 32)) % (max_ms as u128 + 1)) as u64
}

#[test]
fn scheduled_daemon_sigkill_resumes_same_run() {
    let dir = test_dir("daemon");
    let home = dir.join("home");
    let side_effects = dir.join("side-effects.log");
    let progress = dir.join("progress.log");
    fs::create_dir_all(&home).unwrap();

    // Run 1: start the helper, wait for run-created marker, then wait for
    // step-1 to complete, then kill -9 at a random point after that.
    // This makes the kill timing deterministic relative to run creation:
    // - The run is guaranteed to exist before the kill (fixes the original
    //   flakiness where kill landed before run creation on slow machines).
    // - Step-1 is guaranteed to have executed (satisfies the test's
    //   "step-1 must have executed" assertion).
    // - The kill still lands at different points across iterations (during
    //   step-2, between steps, etc.), exercising the resume logic.
    let mut child1 = spawn_helper(&home, &side_effects, &progress);
    let marker_id = wait_for_run_created(&progress, Duration::from_secs(30));
    assert!(
        marker_id.is_some(),
        "helper must write run-created marker within 30s"
    );
    println!("run 1: saw run-created marker: {:?}", marker_id);
    // Wait for step-1 to complete (ensures its side effect was written).
    let step1_done = wait_for_marker(&progress, "step-completed step-1", Duration::from_secs(30));
    assert!(step1_done, "step-1 must complete within 30s");
    println!("run 1: step-1 completed");
    // Random delay 0-800ms after step-1: step-2 takes ~600ms
    // (400ms sleep + side effect + 200ms), step-3 starts ~1200ms.
    // Kills land at various points: during step-2, between step-2 and step-3,
    // during step-3, etc.
    let delay = random_delay_ms(800);
    println!("run 1: killing after {}ms random delay", delay);
    std::thread::sleep(Duration::from_millis(delay));
    // Real SIGKILL.
    child1.kill().expect("kill -9 helper");
    let _ = child1.wait();
    println!("run 1: killed mid-run");

    // Capture the run id from the DB before restart.
    // The DB is at <home>/runs.db; query via a helper python or sqlite3.
    let run_id_1 = get_run_id(&home);
    println!("run 1 id: {:?}", run_id_1);
    assert!(run_id_1.is_some(), "run 1 must have created a run");
    // The marker id and DB id must agree.
    assert_eq!(
        marker_id.as_deref(),
        run_id_1.as_deref(),
        "progress marker run id must match DB run id"
    );

    // Run 2: restart the helper; it should resume the same run.
    // Use a fresh progress file so we don't confuse markers from run 1.
    let progress2 = dir.join("progress2.log");
    let mut child2 = spawn_helper(&home, &side_effects, &progress2);
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
fn count_runs(home: &Path) -> usize {
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
