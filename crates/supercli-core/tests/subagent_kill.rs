//! Real-runtime parent/child subagent reattachment test.
//!
//! Starts a real `subagent_kill_helper` child process (a parent run that
//! spawns one child subagent run executing 3 FileWrite steps), SIGKILLs it
//! mid-child-task, restarts it, and verifies:
//! - the SAME parent run id is reattached (not a new parent),
//! - the SAME child run id is reattached (no duplicate child spawned),
//! - each child step's side effect appears exactly once in the external
//!   side-effects log (zero duplicates, verified from the external log,
//!   not the journal),
//! - both runs reach DONE.

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
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("subagent-kill-{}-{}-{}", name, pid, nanos));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn helper_bin() -> PathBuf {
    // target/debug/deps/subagent_kill-<hash> -> target/debug/examples/subagent_kill_helper
    let mut p = std::env::current_exe().unwrap();
    p.pop(); // deps
    p.pop(); // debug
    p.join("examples").join("subagent_kill_helper")
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
        .expect("spawn subagent_kill_helper")
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

/// Query the DB via rusqlite (bundled). Returns None if the DB is missing
/// or the query fails. Output mimics the sqlite3 CLI default format:
/// `|`-separated columns, newline-separated rows.
fn db_query(home: &PathBuf, sql: &str) -> Option<String> {
    let db = home.join("runs.db");
    if !db.exists() {
        return None;
    }
    let conn = rusqlite::Connection::open(&db).ok()?;
    let mut stmt = conn.prepare(sql).ok()?;
    let col_count = stmt.column_count();
    let rows = stmt
        .query_map([], |row| {
            let mut cols = Vec::with_capacity(col_count);
            for i in 0..col_count {
                let v: rusqlite::types::Value = row.get(i)?;
                cols.push(match v {
                    rusqlite::types::Value::Null => String::new(),
                    rusqlite::types::Value::Integer(n) => n.to_string(),
                    rusqlite::types::Value::Real(f) => f.to_string(),
                    rusqlite::types::Value::Text(s) => s,
                    rusqlite::types::Value::Blob(b) => String::from_utf8_lossy(&b).into_owned(),
                });
            }
            Ok(cols.join("|"))
        })
        .ok()?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.ok()?);
    }
    Some(out.join("\n"))
}

#[test]
fn parent_child_subagent_sigkill_reattaches_same_run() {
    parent_child_sigkill_case("pre-write", Duration::from_millis(800));
}

/// Kill lands between the side effect and the outcome journal: the resume
/// probe must classify the orphan as AlreadyComplete (no re-execution),
/// so the side effect still appears exactly once.
#[test]
fn parent_child_subagent_sigkill_post_write_reattaches() {
    parent_child_sigkill_case("post-write", Duration::from_millis(1100));
}

fn parent_child_sigkill_case(name: &str, kill_after: Duration) {
    let dir = test_dir(name);
    let home = dir.join("home");
    let side_effects = dir.join("side-effects.log");
    fs::create_dir_all(&home).unwrap();

    // Run 1: start the helper (creates parent + spawns child), kill -9 it
    // mid-child-task. Each child step is ~600ms (400ms pre-sleep + write +
    // 200ms post-sleep); the kill delay selects which window is hit.
    let mut child1 = spawn_helper(&home, &side_effects);
    std::thread::sleep(kill_after);
    child1.kill().expect("kill -9 helper");
    let _ = child1.wait();
    println!("run 1: killed mid-child-task");

    // The parent run id is pinned by the helper's pointer file.
    let parent_id_1 = fs::read_to_string(home.join("parent_run_id"))
        .expect("parent_run_id pointer must exist")
        .trim()
        .to_string();
    println!("run 1 parent id: {}", parent_id_1);

    // The child run id, from the DB.
    let child_id_1 = db_query(
        &home,
        &format!("SELECT id FROM runs WHERE parent_run='{}';", parent_id_1),
    )
    .expect("db query");
    println!("run 1 child id: {}", child_id_1);
    assert!(
        !child_id_1.is_empty(),
        "run 1 must have spawned exactly one child"
    );

    // Let the killed process's lease TTL (500ms) expire so the restart can
    // take over the claims.
    std::thread::sleep(Duration::from_secs(1));

    // Run 2: restart the helper; it must reattach, not respawn.
    let mut child2 = spawn_helper(&home, &side_effects);
    let exited = wait_for_exit(&mut child2, Duration::from_secs(30));
    assert!(exited, "helper run 2 must exit within 30s");
    let output = child2.wait_with_output().expect("wait run 2");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("run 2 stdout:\n{}", stdout);
    println!("run 2 stderr:\n{}", stderr);
    assert!(output.status.success(), "helper run 2 must exit 0");

    // SAME parent id: the pointer file is only written on the fresh path.
    let parent_id_2 = fs::read_to_string(home.join("parent_run_id"))
        .expect("parent_run_id pointer must exist")
        .trim()
        .to_string();
    assert_eq!(
        parent_id_1, parent_id_2,
        "restart must reattach the same parent run, not create a new one"
    );

    // SAME child id: exactly one child row for the parent, unchanged.
    let child_rows = db_query(
        &home,
        &format!(
            "SELECT COUNT(*), GROUP_CONCAT(id) FROM runs WHERE parent_run='{}';",
            parent_id_1
        ),
    )
    .expect("db query");
    println!("child rows after restart: {}", child_rows);
    let parts: Vec<&str> = child_rows.split('|').collect();
    assert_eq!(
        parts[0], "1",
        "exactly one child run must exist (no respawn)"
    );
    assert_eq!(
        parts[1], child_id_1,
        "the child run id must be unchanged after restart"
    );

    // No duplicate parents either.
    let parent_count =
        db_query(&home, "SELECT COUNT(*) FROM runs WHERE parent_run IS NULL;").expect("db query");
    assert_eq!(parent_count, "1", "exactly one parent run must exist");

    // Zero duplicates: each child step appears exactly once in the external
    // log (the ground truth, not the journal).
    let log = fs::read_to_string(&side_effects).expect("read side-effects log");
    println!("side-effects log:\n{}", log);
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for line in log.lines() {
        if let Some(step) = line.strip_prefix("executed ") {
            *counts.entry(step).or_insert(0) += 1;
        }
    }
    println!("side-effect counts: {:?}", counts);
    for n in 0..3 {
        let step = format!("child-step-{}", n);
        assert_eq!(
            counts.get(step.as_str()),
            Some(&1),
            "step {} must have executed exactly once: {:?}",
            step,
            counts
        );
    }

    // Both runs reached DONE.
    let states = db_query(
        &home,
        &format!(
            "SELECT state FROM runs WHERE id IN ('{}','{}') ORDER BY id;",
            parent_id_1, child_id_1
        ),
    )
    .expect("db query");
    println!("run states: {}", states);
    for state in states.lines() {
        assert_eq!(state, "DONE", "every run must be DONE, got {}", state);
    }

    let _ = fs::remove_dir_all(&dir);
}
