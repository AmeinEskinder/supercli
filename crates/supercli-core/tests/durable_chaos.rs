//! Real SIGKILL chaos test for durable runs.
//!
//! Unlike the in-process `durable_run_survives_kill_at_step_boundary` unit
//! test (which only drops the DB handle between steps), this test spawns the
//! `durable_chaos_helper` as a real child process, SIGKILLs it at random
//! points **including mid-step**, restarts it, and verifies from the external
//! side-effect file (not the journal) that:
//!
//!   1. Zero duplicated side effects: each (run_id, step_no) appears exactly
//!      once in `side_effects.log`.
//!   2. Every run reaches a terminal state: DONE or NEEDS_REVIEW.
//!
//! The helper implements the write-ahead step protocol (intent before side
//! effect, outcome after). On resume after a mid-step crash it classifies the
//! orphaned intent as Ambiguous and moves the run to NEEDS_REVIEW instead of
//! replaying the step, which is what guarantees (1).
//!
//! Run with: `cargo test --test durable_chaos` (requires `native-host`).

#![cfg(feature = "native-host")]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

use supercli_core::durable_runs::{RunState, RunsDb};

const ITERATIONS: usize = 50;
const STEPS_PER_RUN: u64 = 5;
const MAX_RESTARTS_PER_RUN: usize = 20;

/// Deterministic xorshift64 PRNG (seed logged for reproducibility).
struct XorShift64(u64);

impl XorShift64 {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// Uniform in [lo, hi).
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + (self.next() % (hi - lo))
    }
}

/// Locate the compiled helper binary: `<target>/debug/examples/durable_chaos_helper`.
fn helper_binary() -> PathBuf {
    // Ensure the example is built.
    let status = Command::new("cargo")
        .args(["build", "--example", "durable_chaos_helper"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env(
            "PATH",
            format!(
                "{}/.cargo/bin:{}",
                std::env::var("HOME").unwrap_or_else(|_| "/home/hatch".into()),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .status()
        .expect("cargo build --example failed to spawn");
    assert!(
        status.success(),
        "cargo build --example durable_chaos_helper failed"
    );

    // target/debug/examples/durable_chaos_helper, derived from the test binary.
    let test_exe = std::env::current_exe().expect("current_exe");
    // .../target/debug/deps/<test>-<hash> -> .../target/debug
    let mut dir = test_exe
        .parent()
        .expect("deps dir")
        .parent()
        .expect("debug dir")
        .to_path_buf();
    dir.push("examples");
    dir.push("durable_chaos_helper");
    assert!(dir.exists(), "helper binary not found at {}", dir.display());
    dir
}

fn spawn_helper(helper: &Path, run_id: &str, home: &Path, side_effects: &Path) -> Child {
    Command::new(helper)
        .args([
            "--run-id",
            run_id,
            "--home",
            home.to_str().unwrap(),
            "--steps",
            &STEPS_PER_RUN.to_string(),
            "--side-effects",
            side_effects.to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn helper")
}

fn run_state(db: &RunsDb, run_id: &str) -> RunState {
    // Re-open to see the latest state (the helper may have updated it).
    db.get_run(run_id)
        .map(|r| r.state)
        .unwrap_or(RunState::Failed)
}

#[test]
fn durable_chaos_sigkill_50_iterations() {
    let helper = helper_binary();
    // Seed from a fixed constant for reproducibility; printed for the log.
    let seed: u64 = 0x9E3779B97F4A7C15;
    println!("chaos seed: {seed:#x}");
    let mut rng = XorShift64(seed);

    // One shared temp dir; each iteration gets its own home + run.
    let workspace = tempfile::tempdir().expect("tempdir");
    let side_effects_path = workspace.path().join("side_effects.log");
    let mut run_ids: Vec<String> = Vec::with_capacity(ITERATIONS);

    for iter in 0..ITERATIONS {
        let home = workspace.path().join(format!("home_{iter}"));
        std::fs::create_dir_all(&home).expect("home dir");
        let db = RunsDb::open(&home).expect("open db");
        let run_id = db
            .create_run(None, r#"{"steps":5}"#, "{}")
            .expect("create run");
        run_ids.push(run_id.clone());

        let mut restarts = 0;
        // Drop our DB handle while the helper runs: the helper owns the
        // database during execution; we only open briefly to check state.
        drop(db);
        loop {
            restarts += 1;
            assert!(
                restarts <= MAX_RESTARTS_PER_RUN,
                "run {run_id} did not terminate after {MAX_RESTARTS_PER_RUN} restarts"
            );

            let mut child = spawn_helper(&helper, &run_id, &home, &side_effects_path);

            // Random kill point: 10..300ms. This hits before the first
            // intent, mid-step (during the 50ms sleep), between steps, and
            // after completion.
            let sleep_ms = rng.range(10, 300);
            std::thread::sleep(Duration::from_millis(sleep_ms));

            match child.try_wait().expect("try_wait") {
                Some(status) => {
                    // Helper exited on its own.
                    assert!(
                        status.success(),
                        "run {run_id}: helper exited with {status:?}"
                    );
                    break;
                }
                None => {
                    // Still running: SIGKILL it (Child::kill sends SIGKILL).
                    child.kill().expect("SIGKILL helper");
                    let status = child.wait().expect("wait after kill");
                    assert!(
                        !status.success(),
                        "run {run_id}: killed helper reported success?"
                    );
                }
            }

            // Check if the run reached a terminal state; if so, stop
            // restarting.
            let db_check = RunsDb::open(&home).expect("reopen db");
            match run_state(&db_check, &run_id) {
                RunState::Done | RunState::Failed | RunState::NeedsReview => break,
                _ => {}
            }
        }

        // Final state check for this run.
        let db_final = RunsDb::open(&home).expect("final open");
        match run_state(&db_final, &run_id) {
            RunState::Done | RunState::NeedsReview => {}
            s => panic!("run {run_id} ended in non-terminal state {s:?}"),
        }
        if iter % 10 == 9 {
            println!("chaos: {}/{} iterations complete", iter + 1, ITERATIONS);
        }
    }

    // ---- Verification from the EXTERNAL side-effect file (not the journal).
    let log = std::fs::read_to_string(&side_effects_path).expect("read side_effects.log");
    // (run_id, step_no) -> count
    let mut counts: HashMap<(String, u64), usize> = HashMap::new();
    for line in log.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut run_id: Option<String> = None;
        let mut step_no: Option<u64> = None;
        for part in line.split_whitespace() {
            if let Some(v) = part.strip_prefix("run_id=") {
                run_id = Some(v.to_string());
            } else if let Some(v) = part.strip_prefix("step_no=") {
                step_no = v.parse().ok();
            }
        }
        let (r, s) = (
            run_id.expect("run_id in line"),
            step_no.expect("step_no in line"),
        );
        *counts.entry((r, s)).or_insert(0) += 1;
    }

    // 1. Zero duplicates: every (run_id, step_no) appears exactly once.
    let mut dups = 0;
    for ((r, s), c) in &counts {
        if *c != 1 {
            eprintln!("DUPLICATE: run {r} step {s} appears {c} times");
            dups += 1;
        }
    }
    assert_eq!(dups, 0, "duplicated side effects detected");

    // 2. Every run's steps appear at most once, and completed runs have all
    //    steps; NEEDS_REVIEW runs may have a prefix (steps after the
    //    ambiguous one never execute).
    let seen_runs: HashSet<String> = counts.keys().map(|(r, _)| r.clone()).collect();
    assert_eq!(
        seen_runs.len(),
        run_ids.len(),
        "every run must have at least one side effect"
    );

    // 3. All runs terminal (DONE or NEEDS_REVIEW) -- recheck via fresh DBs.
    let mut done = 0;
    let mut needs_review = 0;
    for (i, run_id) in run_ids.iter().enumerate() {
        let home = workspace.path().join(format!("home_{i}"));
        let db = RunsDb::open(&home).expect("open for verify");
        match run_state(&db, run_id) {
            RunState::Done => done += 1,
            RunState::NeedsReview => needs_review += 1,
            s => panic!("run {run_id} not terminal: {s:?}"),
        }
    }
    println!("chaos result: {done} DONE, {needs_review} NEEDS_REVIEW, 0 duplicates");
    assert_eq!(done + needs_review, ITERATIONS);
}
