//! Chaos-test helper: executes durable-run steps with a write-ahead intent
//! protocol, recording each real side effect to an external append-only file.
//!
//! This is a stopgap implementation of the write-ahead step protocol (task a)
//! for the SIGKILL chaos test. It demonstrates the correct crash-recovery
//! behavior; the protocol will move into `durable_runs.rs` core when (a) lands.
//!
//! Usage:
//!   durable_chaos_helper --run-id <id> --home <dir> --steps <n> \
//!       --side-effects <path>
//!
//! Protocol per step:
//!   1. `begin_step`: append intent row (outcome=None) to the journal.
//!   2. Perform the "side effect": append one line to the external
//!      side-effects file and fsync it. This is the ground truth.
//!   3. Sleep 50ms (widens the SIGKILL window).
//!   4. `complete_step`: append outcome row (Executed).
//!
//! Crash recovery (on startup):
//!   - If the run is terminal (DONE/FAILED/NEEDS_REVIEW): exit 0.
//!   - Call `resume_run`. If `needs_review_step_no` is set: exit 0.
//!   - Let `n = first_incomplete_step_no`, `total = run.steps_total`.
//!     If `n < total`, step `n` has an intent row without an outcome: we
//!     crashed mid-step. The side effect may or may not have happened, so
//!     classify as Ambiguous (never replay) and let `resume_run` move the
//!     run to NEEDS_REVIEW. Exit 0.
//!   - Otherwise execute steps `n..steps`, then transition to DONE.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use supercli_core::action_reviews::AttemptOutcome;
use supercli_core::durable_runs::{step_input_hash, RunState, RunsDb, StepKind};

fn usage() -> ! {
    eprintln!(
        "usage: durable_chaos_helper --run-id <id> --home <dir> \
         --steps <n> --side-effects <path>"
    );
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut run_id: Option<String> = None;
    let mut home: Option<PathBuf> = None;
    let mut steps: Option<u64> = None;
    let mut side_effects: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--run-id" => {
                i += 1;
                run_id = args.get(i).cloned();
            }
            "--home" => {
                i += 1;
                home = args.get(i).map(PathBuf::from);
            }
            "--steps" => {
                i += 1;
                steps = args.get(i).and_then(|s| s.parse().ok());
            }
            "--side-effects" => {
                i += 1;
                side_effects = args.get(i).map(PathBuf::from);
            }
            _ => usage(),
        }
        i += 1;
    }

    let (run_id, home, steps, side_effects) = match (run_id, home, steps, side_effects) {
        (Some(r), Some(h), Some(s), Some(e)) => (r, h, s, e),
        _ => usage(),
    };

    if let Err(e) = run(&run_id, &home, steps, &side_effects) {
        eprintln!("durable_chaos_helper: error: {e:?}");
        std::process::exit(1);
    }
}

fn run(
    run_id: &str,
    home: &std::path::Path,
    steps: u64,
    side_effects_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = RunsDb::open(home)?;

    // 1. If the run is already terminal, nothing to do.
    let run = db.get_run(run_id)?;
    match run.state {
        RunState::Done | RunState::Failed | RunState::NeedsReview => return Ok(()),
        _ => {}
    }

    // 2. Move QUEUED -> EXECUTING_TOOLS (idempotent via conditional update).
    if matches!(run.state, RunState::Queued) {
        let _ = db.transition(run_id, RunState::Queued, RunState::ExecutingTools);
    }

    // 3. Ask the journal where to resume.
    let plan = db.resume_run(run_id)?;
    if plan.needs_review_step_no.is_some() {
        // Already classified ambiguous; resume_run marked NEEDS_REVIEW.
        return Ok(());
    }
    let first = plan.first_incomplete_step_no.unwrap_or(steps);
    let run = db.get_run(run_id)?;
    // Re-check terminal: resume_run may have marked NEEDS_REVIEW.
    match run.state {
        RunState::Done | RunState::Failed | RunState::NeedsReview => return Ok(()),
        _ => {}
    }

    // 4. Mid-step crash detection (stopgap classifier for task a):
    //    if the first incomplete step_no is < steps_total, the journal holds
    //    an intent row (outcome=None) for it: we crashed after begin_step.
    //    The side effect may have happened -> Ambiguous, never replay.
    if first < run.steps_total {
        let input_hash = step_input_hash(&format!("chaos:{run_id}:{first}"));
        db.append_step(
            run_id,
            first,
            StepKind::Tool,
            &input_hash,
            None,
            Some(&AttemptOutcome::Ambiguous {
                reason: "chaos: crashed mid-step; side effect may have executed".to_string(),
            }),
        )?;
        // Let resume_run see the ambiguous outcome and mark NEEDS_REVIEW.
        let _ = db.resume_run(run_id)?;
        return Ok(());
    }

    // 5. Execute the remaining steps with the write-ahead protocol.
    for step_no in first..steps {
        let input_hash = step_input_hash(&format!("chaos:{run_id}:{step_no}"));

        // (a) begin_step: intent first.
        db.append_step(run_id, step_no, StepKind::Tool, &input_hash, None, None)?;

        // (b) The real side effect, external to the journal, fsynced.
        record_side_effect(side_effects_path, run_id, step_no)?;

        // (c) Widen the crash window.
        std::thread::sleep(std::time::Duration::from_millis(50));

        // (d) complete_step: outcome after.
        db.append_step(
            run_id,
            step_no,
            StepKind::Tool,
            &input_hash,
            Some("ok"),
            Some(&AttemptOutcome::Executed { success: true }),
        )?;
    }

    // 6. All steps done -> DONE (conditional; safe if a prior attempt won).
    let _ = db.transition(run_id, RunState::ExecutingTools, RunState::Done);
    // Also handle the case where we never left QUEUED (0 steps).
    let _ = db.transition(run_id, RunState::Queued, RunState::Done);
    Ok(())
}

/// Append one side-effect line and fsync: this file is the ground truth for
/// the chaos test, independent of the journal.
fn record_side_effect(
    path: &std::path::Path,
    run_id: &str,
    step_no: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let pid = std::process::id();
    let ts_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(
        f,
        "run_id={run_id} step_no={step_no} pid={pid} ts_nanos={ts_nanos}"
    )?;
    f.flush()?;
    f.sync_all()?;
    Ok(())
}
