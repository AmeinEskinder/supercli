//! Real SIGKILL chaos test for durable runs, with reconciliation.
//!
//! Unlike the in-process `durable_run_survives_kill_at_step_boundary` unit
//! test (which only drops the DB handle between steps), this test spawns the
//! `durable_chaos_helper` as a real child process, SIGKILLs it at random
//! points **including mid-step**, restarts it, and verifies from external
//! ground truth (not the journal):
//!
//!   1. Zero duplicated side effects:
//!      - `opaque_write` lines in `side_effects.log`: each (run_id, step_no)
//!        appears at most once (opaque orphans are never replayed).
//!      - `file_write`: every run's `<home>/file_step_2.txt` has exactly the
//!        intended content (a rerun overwrites wholesale; a torn write is
//!        detected by the probe and repaired).
//!      - `idempotent_http`: every idempotency key has exactly one EFFECT
//!        line in `<home>/idem_server.log` (replays carry the same key;
//!        the server dedups).
//!   2. Every run reaches a terminal state: DONE or NEEDS_REVIEW.
//!   3. Per-kind reconciliation split: only `opaque_write` orphans may end
//!      in review; every other kind reconciles (rerun or probe-complete).
//!
//! Each run executes the kind mix:
//!   step 0 = model, 1 = read, 2 = file_write, 3 = idempotent_http,
//!   4 = opaque_write.
//!
//! Run with: `cargo test --test durable_chaos` (requires `native-host`).

#![cfg(feature = "native-host")]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

use supercli_core::browser_engine::sha256_hex;
use supercli_core::durable_runs::{RunState, RunsDb, StepKind};

const ITERATIONS: usize = 50;
const STEPS_PER_RUN: u64 = 5;
const MAX_RESTARTS_PER_RUN: usize = 20;
const KINDS: &str = "model,read,file_write,idempotent_http,opaque_write";

/// Step kinds in execution order (must match KINDS).
fn kind_of(step_no: u64) -> StepKind {
    match step_no {
        0 => StepKind::Model,
        1 => StepKind::ReadOnly,
        2 => StepKind::FileWrite,
        3 => StepKind::IdempotentHttp,
        4 => StepKind::OpaqueWrite,
        _ => panic!("unexpected step_no {step_no}"),
    }
}

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
/// Locate the compiled helper binary via CARGO_BIN_EXE.
/// cargo builds the [[bin]] target before running integration tests,
/// so the helper is always present (previously derived from current_exe,
/// which broke when `cargo test` did not build examples).
fn helper_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_durable_chaos_helper"))
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
            "--kinds",
            KINDS,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn helper")
}

fn run_state_of(home: &Path, run_id: &str) -> RunState {
    // Re-open to see the latest state (the helper may have updated it).
    RunsDb::open(home)
        .expect("open db")
        .get_run(run_id)
        .map(|r| r.state)
        .unwrap_or(RunState::Failed)
}

/// How one step's orphan was reconciled, from journal archaeology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepFate {
    /// begin → complete with no crash.
    Normal,
    /// Orphaned intent, then re-begun (≥2 `started` rows): safe rerun.
    Rerun,
    /// Orphaned intent, probe proved the write landed: journaled executed.
    ProbeCompleted,
    /// Orphaned intent still open, run parked: NEEDS_REVIEW.
    Review,
    /// Step never began (after a parked step).
    NeverStarted,
}

fn step_fate(db: &RunsDb, run_id: &str, step_no: u64) -> StepFate {
    let attempts = db.step_attempts(run_id, step_no).expect("step_attempts");
    if attempts.is_empty() {
        return StepFate::NeverStarted;
    }
    let started_count = attempts
        .iter()
        .filter(|a| a.outcome.as_deref() == Some("started"))
        .count();
    let reconciled = attempts.iter().any(|a| {
        a.output
            .as_deref()
            .unwrap_or("")
            .contains("\"reconciled\":\"probe_match\"")
    });
    let latest_outcome = attempts.last().and_then(|a| a.outcome.as_deref());
    if reconciled {
        StepFate::ProbeCompleted
    } else if started_count >= 2 {
        StepFate::Rerun
    } else if latest_outcome == Some("started") {
        StepFate::Review
    } else {
        StepFate::Normal
    }
}

#[derive(Default)]
struct KindStats {
    normal: u64,
    rerun: u64,
    probe_completed: u64,
    review: u64,
    never_started: u64,
}

impl KindStats {
    fn add(&mut self, fate: StepFate) {
        match fate {
            StepFate::Normal => self.normal += 1,
            StepFate::Rerun => self.rerun += 1,
            StepFate::ProbeCompleted => self.probe_completed += 1,
            StepFate::Review => self.review += 1,
            StepFate::NeverStarted => self.never_started += 1,
        }
    }
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
            .create_run(
                None,
                r#"{"steps":5,"kinds":"model,read,file_write,idempotent_http,opaque_write"#,
                "{}",
            )
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
            // intent, mid-step (during sleeps), between steps, and after
            // completion.
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
                    if status.success() {
                        // TOCTOU: the helper exited on its own in the window
                        // between try_wait and kill; the SIGKILL landed on an
                        // already-exited (zombie) process and wait() reaped
                        // its success status. Treat as a clean exit.
                        break;
                    }
                }
            }

            // Check if the run reached a terminal state; if so, stop
            // restarting.
            match run_state_of(&home, &run_id) {
                RunState::Done | RunState::Failed | RunState::NeedsReview => break,
                _ => {}
            }
        }

        // Final state check for this run.
        match run_state_of(&home, &run_id) {
            RunState::Done | RunState::NeedsReview => {}
            s => panic!("run {run_id} ended in non-terminal state {s:?}"),
        }
        if iter % 10 == 9 {
            println!("chaos: {}/{} iterations complete", iter + 1, ITERATIONS);
        }
    }

    // ---- Verification 1: opaque_write exactly-once from the EXTERNAL log.
    let log = std::fs::read_to_string(&side_effects_path).unwrap_or_default();
    // (run_id, step_no) -> count, opaque_write lines only.
    let mut counts: HashMap<(String, u64), usize> = HashMap::new();
    for line in log.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut run_id: Option<String> = None;
        let mut step_no: Option<u64> = None;
        let mut kind: Option<String> = None;
        for part in line.split_whitespace() {
            if let Some(v) = part.strip_prefix("run_id=") {
                run_id = Some(v.to_string());
            } else if let Some(v) = part.strip_prefix("step_no=") {
                step_no = v.parse().ok();
            } else if let Some(v) = part.strip_prefix("kind=") {
                kind = Some(v.to_string());
            }
        }
        assert_eq!(
            kind.as_deref(),
            Some("opaque_write"),
            "unexpected line: {line}"
        );
        let (r, s) = (
            run_id.expect("run_id in line"),
            step_no.expect("step_no in line"),
        );
        // Opaque steps are always step 4 in the mix.
        assert_eq!(s, 4, "opaque_write at unexpected step {s}");
        *counts.entry((r, s)).or_insert(0) += 1;
    }
    let mut dups = 0;
    for ((r, s), c) in &counts {
        if *c > 1 {
            eprintln!("DUPLICATE opaque effect: run {r} step {s} appears {c} times");
            dups += 1;
        }
    }
    assert_eq!(dups, 0, "duplicated opaque side effects detected");

    // ---- Verification 2: file_write content correctness (external truth).
    // Every run executes steps in order; only opaque_write (step 4) can
    // park the run, so step 2 (file_write) always completed.
    for (i, run_id) in run_ids.iter().enumerate() {
        let home = workspace.path().join(format!("home_{i}"));
        let path = home.join("file_step_2.txt");
        let expected = format!("chaos-file:{run_id}:2\n");
        let actual = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("run {run_id}: file_step_2.txt missing"));
        assert_eq!(actual, expected, "run {run_id}: file content mismatch");
        assert_eq!(
            sha256_hex(actual.as_bytes()),
            sha256_hex(expected.as_bytes())
        );
    }

    // ---- Verification 3: idempotency server dedup (external truth).
    // Each key must have exactly one EFFECT line; DEDUP lines are fine.
    for (i, run_id) in run_ids.iter().enumerate() {
        let home = workspace.path().join(format!("home_{i}"));
        let server_log = home.join("idem_server.log");
        let content = std::fs::read_to_string(&server_log)
            .unwrap_or_else(|_| panic!("run {run_id}: idem_server.log missing"));
        let key = format!("idem:{run_id}:3");
        let effects = content
            .lines()
            .filter(|l| *l == format!("EFFECT key={key}"))
            .count();
        assert_eq!(
            effects, 1,
            "run {run_id}: key {key} has {effects} EFFECT lines (want exactly 1)"
        );
    }

    // ---- Verification 4: per-kind reconciliation split (journal archaeology).
    let mut stats: HashMap<StepKind, KindStats> = HashMap::new();
    let mut done = 0u64;
    let mut needs_review = 0u64;
    for (i, run_id) in run_ids.iter().enumerate() {
        let home = workspace.path().join(format!("home_{i}"));
        let db = RunsDb::open(&home).expect("open for verify");
        match run_state_of(&home, run_id) {
            RunState::Done => done += 1,
            RunState::NeedsReview => needs_review += 1,
            s => panic!("run {run_id} not terminal: {s:?}"),
        }
        for step_no in 0..STEPS_PER_RUN {
            let kind = kind_of(step_no);
            let fate = step_fate(&db, run_id, step_no);
            stats.entry(kind).or_default().add(fate);
        }
    }
    assert_eq!(done + needs_review, ITERATIONS as u64);

    // Only opaque_write orphans may end in review.
    let mut non_opaque_reviews = 0u64;
    for (kind, st) in &stats {
        println!(
            "kind {:15} normal={:3} rerun={:3} probe_completed={:3} review={:3} never_started={:3}",
            format!("{kind:?}"),
            st.normal,
            st.rerun,
            st.probe_completed,
            st.review,
            st.never_started
        );
        if *kind != StepKind::OpaqueWrite {
            non_opaque_reviews += st.review;
        }
    }
    println!("chaos result: {done} DONE, {needs_review} NEEDS_REVIEW, 0 duplicates");
    assert_eq!(
        non_opaque_reviews, 0,
        "only opaque_write steps may need review"
    );

    // Sanity: every run is accounted for in the opaque stats.
    let opaque_total = stats
        .get(&StepKind::OpaqueWrite)
        .map(|s| s.normal + s.rerun + s.probe_completed + s.review + s.never_started)
        .unwrap_or(0);
    assert_eq!(opaque_total, ITERATIONS as u64);

    // The set of runs seen is exactly the set created.
    let seen_runs: HashSet<String> = counts.keys().map(|(r, _)| r.clone()).collect();
    for r in &seen_runs {
        assert!(run_ids.contains(r), "unknown run {r} in side effects log");
    }
}
