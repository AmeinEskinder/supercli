//! Chaos-test helper: executes durable-run steps with the write-ahead
//! intent protocol, recording each real side effect externally.
//!
//! Each run executes a mix of step kinds (default):
//! `model,read,file_write,idempotent_http,opaque_write`.
//!
//! - `model`: simulated LLM call (sleep, no side effect) — safe to rerun.
//! - `read`: reads `<home>/read_source.txt` — safe to rerun.
//! - `file_write`: writes `<home>/file_step_<n>.txt` with a reconcile hint
//!   (path + expected SHA-256); resume probes the file before replaying.
//! - `idempotent_http`: simulated remote with an idempotency-keyed server
//!   log at `<home>/idem_server.log`; replays use the SAME key and the
//!   server dedups (one EFFECT per key, DEDUP lines are not effects).
//! - `opaque_write`: appends to the external `--side-effects` file; no
//!   probe and no key exist, so a mid-step crash parks the run in
//!   NEEDS_REVIEW (never replayed).
//!
//! Crash recovery delegates entirely to `RunsDb::resume_run`, which
//! reconciles orphaned intents before falling back to review.
//!
//! Usage:
//!   durable_chaos_helper --run-id <id> --home <dir> --steps <n> \
//!       --side-effects <path> [--kinds <csv>]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use supercli_core::action_reviews::AttemptOutcome;
use supercli_core::browser_engine::sha256_hex;
use supercli_core::durable_runs::{step_input_hash, ReconcileHint, RunState, RunsDb, StepKind};

fn usage() -> ! {
    eprintln!(
        "usage: durable_chaos_helper --run-id <id> --home <dir> \
         --steps <n> --side-effects <path> [--kinds <csv>]"
    );
    std::process::exit(2);
}

fn parse_kind(s: &str) -> StepKind {
    match s {
        "model" => StepKind::Model,
        "read" => StepKind::ReadOnly,
        "file_write" => StepKind::FileWrite,
        "idempotent_http" => StepKind::IdempotentHttp,
        "opaque_write" => StepKind::OpaqueWrite,
        _ => {
            eprintln!("unknown kind: {s}");
            usage();
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut run_id: Option<String> = None;
    let mut home: Option<PathBuf> = None;
    let mut steps: Option<u64> = None;
    let mut side_effects: Option<PathBuf> = None;
    let mut kinds: Option<String> = None;

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
            "--kinds" => {
                i += 1;
                kinds = args.get(i).cloned();
            }
            _ => usage(),
        }
        i += 1;
    }

    let (run_id, home, steps, side_effects) = match (run_id, home, steps, side_effects) {
        (Some(r), Some(h), Some(s), Some(e)) => (r, h, s, e),
        _ => usage(),
    };
    let kinds: Vec<StepKind> = kinds
        .as_deref()
        .unwrap_or("model,read,file_write,idempotent_http,opaque_write")
        .split(',')
        .map(|s| parse_kind(s.trim()))
        .collect();
    if kinds.is_empty() {
        usage();
    }

    if let Err(e) = run(&run_id, &home, steps, &side_effects, &kinds) {
        eprintln!("durable_chaos_helper: error: {e:?}");
        std::process::exit(1);
    }
}

/// Append one line to a file and fsync it (crash-safe external record).
fn append_fsync(path: &Path, line: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(line.as_bytes())?;
    f.flush()?;
    f.sync_all()?;
    Ok(())
}

fn ts_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// The side effect for one step. Returns the reconcile hint to journal
/// with the intent (if any).
fn execute_step(
    kind: StepKind,
    run_id: &str,
    home: &Path,
    step_no: u64,
    side_effects_path: &Path,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    match kind {
        StepKind::Model => {
            // Simulated LLM call: no external side effect.
            std::thread::sleep(std::time::Duration::from_millis(30));
            Ok(None)
        }
        StepKind::ReadOnly => {
            // Side-effect-free read.
            let src = home.join("read_source.txt");
            if !src.exists() {
                std::fs::write(&src, b"chaos read source v1\n")?;
            }
            let _ = std::fs::read(&src)?;
            Ok(None)
        }
        StepKind::FileWrite => {
            let content = format!("chaos-file:{run_id}:{step_no}\n");
            let path = home.join(format!("file_step_{step_no}.txt"));
            let hash = sha256_hex(content.as_bytes());
            let hint = ReconcileHint {
                path: Some(path.to_string_lossy().to_string()),
                expected_hash: Some(hash),
                idempotency_key: None,
            };
            // Full overwrite + fsync: a torn write is detectable by the
            // probe (hash mismatch) and the rerun overwrites wholesale.
            std::fs::write(&path, &content)?;
            let f = OpenOptions::new().read(true).open(&path)?;
            f.sync_all()?;
            drop(f);
            // Parent dir fsync so the rename/write is durable.
            if let Ok(d) = OpenOptions::new().read(true).open(home) {
                let _ = d.sync_all();
            }
            Ok(hint.to_json())
        }
        StepKind::IdempotentHttp => {
            // The step id IS the idempotency key: derived deterministically
            // so every replay carries the SAME key.
            let key = format!("idem:{run_id}:{step_no}");
            let server = home.join("idem_server.log");
            let seen = std::fs::read_to_string(&server)
                .unwrap_or_default()
                .lines()
                .any(|l| l == format!("EFFECT key={key}"));
            if seen {
                // Remote dedups: no new effect, just a dedup marker (not
                // an effect — duplicates here are harmless).
                append_fsync(&server, &format!("DEDUP key={key}\n"))?;
            } else {
                append_fsync(&server, &format!("EFFECT key={key}\n"))?;
            }
            let hint = ReconcileHint {
                path: None,
                expected_hash: None,
                idempotency_key: Some(key),
            };
            Ok(hint.to_json())
        }
        StepKind::OpaqueWrite => {
            // The hard case: an external effect with no probe and no
            // idempotency key. Exactly-once is enforced by NEVER replaying.
            let pid = std::process::id();
            append_fsync(
                side_effects_path,
                &format!(
                    "run_id={run_id} step_no={step_no} kind=opaque_write \
                     pid={pid} ts_nanos={}\n",
                    ts_nanos()
                ),
            )?;
            Ok(None)
        }
        StepKind::Tool | StepKind::Subagent => {
            eprintln!("chaos helper does not use legacy kinds");
            std::process::exit(2);
        }
    }
}

fn run(
    run_id: &str,
    home: &Path,
    steps: u64,
    side_effects_path: &Path,
    kinds: &[StepKind],
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

    // 3. Resume: orphaned intents are reconciled inside resume_run
    //    (probe / same-key replay) before falling back to review.
    let plan = db.resume_run(run_id)?;
    if plan.needs_review_step_no.is_some() {
        return Ok(());
    }
    // Re-check terminal: resume_run may have marked NEEDS_REVIEW.
    match db.get_run(run_id)?.state {
        RunState::Done | RunState::Failed | RunState::NeedsReview => return Ok(()),
        _ => {}
    }
    let first = plan.first_incomplete_step_no.unwrap_or(steps);

    // 4. Execute the remaining steps with the write-ahead protocol.
    for step_no in first..steps {
        let kind = kinds[(step_no as usize) % kinds.len()];
        let input_hash = step_input_hash(&format!("chaos2:{run_id}:{step_no}:{}", kind.as_str()));

        // (a) Intent first, fsync'd (with reconcile hint).
        let hint = execute_hint_placeholder(kind, run_id, home, step_no)?;
        let intent =
            db.begin_step_with_hint(run_id, step_no, kind, &input_hash, hint.as_deref())?;

        // (b) The real side effect.
        let actual_hint = execute_step(kind, run_id, home, step_no, side_effects_path)?;
        // The hint journaled at (a) must equal the one for the effect we
        // just ran (deterministic derivation); a mismatch is a bug.
        assert_eq!(hint, actual_hint, "hint drift for step {step_no}");

        // (c) Widen the crash window: kill -9 here orphans the intent.
        std::thread::sleep(std::time::Duration::from_millis(30));

        // (d) Outcome after.
        db.complete_step(
            &intent,
            &AttemptOutcome::Executed { success: true },
            Some("ok"),
        )?;
    }

    // 5. All steps done -> DONE (conditional; safe if a prior attempt won).
    let _ = db.transition(run_id, RunState::ExecutingTools, RunState::Done);
    let _ = db.transition(run_id, RunState::Queued, RunState::Done);
    Ok(())
}

/// Derive the reconcile hint for a step WITHOUT executing the effect, so
/// it can be journaled in the intent row before the side effect runs.
/// Must be deterministic and identical to what [`execute_step`] uses.
fn execute_hint_placeholder(
    kind: StepKind,
    run_id: &str,
    home: &Path,
    step_no: u64,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    match kind {
        StepKind::Model | StepKind::ReadOnly | StepKind::OpaqueWrite => Ok(None),
        StepKind::FileWrite => {
            let content = format!("chaos-file:{run_id}:{step_no}\n");
            let path = home.join(format!("file_step_{step_no}.txt"));
            let hint = ReconcileHint {
                path: Some(path.to_string_lossy().to_string()),
                expected_hash: Some(sha256_hex(content.as_bytes())),
                idempotency_key: None,
            };
            Ok(hint.to_json())
        }
        StepKind::IdempotentHttp => {
            let key = format!("idem:{run_id}:{step_no}");
            let hint = ReconcileHint {
                path: None,
                expected_hash: None,
                idempotency_key: Some(key),
            };
            Ok(hint.to_json())
        }
        StepKind::Tool | StepKind::Subagent => {
            eprintln!("chaos helper does not use legacy kinds");
            std::process::exit(2);
        }
    }
}
