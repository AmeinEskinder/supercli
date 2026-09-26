//! Helper binary for the parent/child subagent SIGKILL test.
//!
//! Simulates a parent process that spawns one child subagent run. The child
//! executes 3 `FileWrite` steps, each appending a line to an external
//! append-only log (the ground truth). Each step journals a write-ahead
//! intent carrying a [`ReconcileHint`] (log path + expected SHA-256 of the
//! post-append content), so a SIGKILL at ANY point — before the intent,
//! between intent and write, between write and outcome journal — still
//! yields exactly one log line per step on resume (probe match ->
//! AlreadyComplete, probe mismatch -> clean rerun that overwrites).
//!
//! The test SIGKILLs this process mid-child-task, restarts it, and verifies:
//! - the SAME parent run id and SAME child run id are reattached (no
//!   duplicate child spawned),
//! - each child step's side effect appears exactly once in the external log,
//! - both runs reach DONE.
//!
//! Usage: subagent_kill_helper --home <dir> --side-effects <path>

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use supercli_core::action_reviews::AttemptOutcome;
use supercli_core::browser_engine::sha256_hex;
use supercli_core::durable_runs::{
    ReconcileHint, RunState, RunsDb, StepKind, step_input_hash,
};

const CHILD_STEPS: u64 = 3;
const LEASE_TTL_MS: u64 = 500;

fn record_side_effect(log_path: &Path, line: &str, expected_after: &[u8]) {
    // Overwrite the whole log with the expected post-append content and
    // fsync. A torn write is safe: the resume probe hashes the file, a
    // mismatch means clean rerun, and the rerun overwrites wholesale.
    let f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_path)
        .expect("open side-effects log");
    let mut f = f;
    f.write_all(expected_after).expect("write side effect");
    f.sync_all().expect("fsync side-effects log");
    eprintln!("helper: side effect landed: {}", line.trim());
}

/// Execute one child step with the write-ahead + probe pattern.
fn execute_child_step(db: &RunsDb, child_id: &str, step_no: u64, log_path: &Path) {
    let line = format!("executed child-step-{}\n", step_no);
    let current = fs::read(log_path).unwrap_or_default();
    let mut expected_after = current.clone();
    expected_after.extend_from_slice(line.as_bytes());
    let expected_hash = sha256_hex(&expected_after);

    let hint = ReconcileHint {
        path: Some(log_path.to_string_lossy().to_string()),
        expected_hash: Some(expected_hash),
        idempotency_key: None,
    };
    let input_hash = step_input_hash(&format!("child-step-{}", step_no));
    let intent = db
        .begin_step_with_hint(
            child_id,
            step_no,
            StepKind::FileWrite,
            &input_hash,
            hint.to_json().as_deref(),
        )
        .expect("begin child step");

    // Widen the kill window: sleep before the side effect.
    std::thread::sleep(Duration::from_millis(400));
    record_side_effect(log_path, &line, &expected_after);
    // Sleep after too, so a kill can land between the side effect and the
    // outcome journal write (the probe must catch this -> AlreadyComplete).
    std::thread::sleep(Duration::from_millis(200));

    db.complete_step(
        &intent,
        &AttemptOutcome::Executed { success: true },
        Some(&format!("{{\"step\":{}}}", step_no)),
    )
    .expect("complete child step");
}

/// Claim a run, retrying while the previous (killed) owner's lease is still
/// valid. Returns the claim's took_over flag.
fn claim_with_retry(db: &RunsDb, run_id: &str) -> bool {
    for _ in 0..20 {
        match db.claim_run(run_id).expect("claim run") {
            Some(claim) => return claim.took_over,
            None => {
                eprintln!("helper: lease still held, waiting...");
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
    panic!("could not claim run {}", run_id);
}

/// Drive the child run to completion starting from its resume plan.
/// Idempotent: if the child is already DONE (kill landed after the child
/// finished but before the parent did), this is a no-op.
fn drive_child(db: &RunsDb, child_id: &str, log_path: &Path) {
    let run = db.get_run(child_id).expect("get child run");
    if run.state == RunState::Done {
        println!("helper: child {} already DONE, skipping", child_id);
        return;
    }
    let took_over = claim_with_retry(db, child_id);
    println!("helper: child {} claimed (took_over={})", child_id, took_over);
    let plan = db.resume_run(child_id).expect("resume child");
    println!(
        "helper: child resume plan: completed={} first_incomplete={:?} needs_review={:?}",
        plan.completed.len(),
        plan.first_incomplete_step_no,
        plan.needs_review_step_no
    );
    assert!(
        plan.needs_review_step_no.is_none(),
        "child must not need review"
    );
    let start = plan.first_incomplete_step_no.unwrap_or(CHILD_STEPS);
    for step_no in start..CHILD_STEPS {
        execute_child_step(db, child_id, step_no, log_path);
    }
    let done = db
        .transition(child_id, RunState::Queued, RunState::Done)
        .expect("transition child to DONE");
    assert!(done, "child must transition to DONE");
    db.release_run(child_id).expect("release child lease");
    println!("helper: child {} DONE", child_id);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut home: Option<PathBuf> = None;
    let mut side_effects: Option<PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--home" => {
                home = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--side-effects" => {
                side_effects = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            _ => {
                eprintln!("usage: subagent_kill_helper --home <dir> --side-effects <path>");
                std::process::exit(2);
            }
        }
    }
    let home = home.expect("--home required");
    let side_effects = side_effects.expect("--side-effects required");

    let db = RunsDb::open_with_ttl(&home, LEASE_TTL_MS).expect("open RunsDb");
    let pointer = home.join("parent_run_id");

    let parent_id = if pointer.exists() {
        // Restart path: reattach to the existing parent and child.
        let parent_id = fs::read_to_string(&pointer)
            .expect("read parent_run_id")
            .trim()
            .to_string();
        println!("helper: reattaching to parent {}", parent_id);
        let took_over = claim_with_retry(&db, &parent_id);
        println!(
            "helper: parent {} claimed (took_over={})",
            parent_id, took_over
        );
        let kids = db.list_children(&parent_id).expect("list children");
        assert_eq!(
            kids.len(),
            1,
            "parent must have exactly one child (no respawn): found {}",
            kids.len()
        );
        let child_id = kids[0].id.clone();
        println!("helper: reattached to existing child {}", child_id);
        drive_child(&db, &child_id, &side_effects);
        parent_id
    } else {
        // Fresh path: create parent + one child subagent run.
        let parent_id = db
            .create_run(None, r#"{"task":"parent"}"#, "{}")
            .expect("create parent run");
        {
            let mut pf = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&pointer)
                .expect("write parent_run_id");
            writeln!(pf, "{}", parent_id).expect("write parent_run_id");
            pf.sync_all().expect("fsync parent_run_id");
        }
        // Fsync the pointer file's directory entry for crash safety.
        let dirf = OpenOptions::new()
            .read(true)
            .open(&home)
            .expect("open home dir");
        dirf.sync_all().expect("fsync home dir");
        println!("helper: created parent {}", parent_id);

        claim_with_retry(&db, &parent_id);
        let child_id = db
            .create_run(
                Some(&parent_id),
                r#"{"task":"child-subagent"}"#,
                "{}",
            )
            .expect("create child run");
        println!("helper: spawned child {}", child_id);
        drive_child(&db, &child_id, &side_effects);
        parent_id
    };

    let parent_run = db.get_run(&parent_id).expect("get parent run");
    if parent_run.state != RunState::Done {
        let done = db
            .transition(&parent_id, RunState::Queued, RunState::Done)
            .expect("transition parent to DONE");
        assert!(done, "parent must transition to DONE");
    } else {
        println!("helper: parent {} already DONE", parent_id);
    }
    db.release_run(&parent_id).expect("release parent lease");
    println!("helper: parent {} DONE", parent_id);
}
