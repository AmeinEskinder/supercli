//! `supercli runs` — durable run management (docs/durable-runs.md).
//!
//! Crash-safe agent runs: every run keeps a SQLite journal at
//! `<home>/runs.db`. A run that dies (kill -9, OOM, reboot) is picked up
//! after restart and continues at the first incomplete step instead of
//! vanishing or re-firing side effects.

use supercli_core::app_paths;
use supercli_core::durable_runs::{RunState, RunsDb};

pub const HELP: &str = "\
supercli runs — durable run management

  supercli runs list [--state <STATE>] [--json]
                                   list runs (newest first)
  supercli runs show <id> [--json]
                                   run detail: state, plan, step journal, lease
  supercli runs resume <id>        claim the run and show where it continues
  supercli runs pause <id>         pause a run (never auto-claimed while paused)
  supercli runs cancel <id>        cancel a run (FAILED + cancelled flag)
  supercli runs retry <id>         retry from FAILED / NEEDS_REVIEW

States: QUEUED AWAITING_MODEL EXECUTING_TOOLS AWAITING_APPROVAL
        PAUSED DONE FAILED NEEDS_REVIEW
";

fn db() -> Result<RunsDb, String> {
    let home = app_paths::supercli_home();
    RunsDb::open(&home).map_err(|e| e.to_string())
}

fn print_run_human(run: &supercli_core::durable_runs::Run, db: &RunsDb) {
    println!("id:       {}", run.id);
    if let Some(parent) = &run.parent_run {
        println!("parent:   {parent}");
    }
    println!("state:    {}", run.state.as_str());
    println!("progress: {}/{} steps", run.steps_done, run.steps_total);
    match (&run.lease_owner, run.lease_generation) {
        (Some(owner), gen) if gen > 0 => {
            println!(
                "lease:    owner={owner} gen={gen} expires_at={}",
                run.lease_expires_at
            )
        }
        _ => println!("lease:    unclaimed"),
    }
    println!("plan:     {}", run.plan);
    println!("budgets:  {}", run.budgets);
    match db.resume_run(&run.id) {
        Ok(plan) => {
            if let Some(no) = plan.needs_review_step_no {
                println!("resume:   NEEDS_REVIEW at step {no} (ambiguous — not replayed)");
            } else if let Some(no) = plan.first_incomplete_step_no {
                println!(
                    "resume:   {} completed step(s), continue at step {no}",
                    plan.completed.len()
                );
            }
        }
        Err(e) => println!("resume:   <error: {e}>"),
    }
}

pub fn run(args: &[String]) -> i32 {
    let result: Result<i32, String> = match args.first().map(String::as_str) {
        Some("list") => {
            let state = args
                .windows(2)
                .find(|w| w[0] == "--state")
                .map(|w| w[1].as_str());
            let json = args.iter().any(|a| a == "--json");
            match db().and_then(|d| d.list_runs(state).map_err(|e| e.to_string())) {
                Ok(runs) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&runs_json(&runs))
                                .unwrap_or_else(|_| "[]".into())
                        );
                    } else if runs.is_empty() {
                        println!("no runs yet");
                    } else {
                        for r in &runs {
                            let parent = r
                                .parent_run
                                .as_deref()
                                .map(|p| format!(" parent={p}"))
                                .unwrap_or_default();
                            println!(
                                "{}  {}  {}/{} steps{}",
                                r.id,
                                r.state.as_str(),
                                r.steps_done,
                                r.steps_total,
                                parent
                            );
                        }
                    }
                    Ok(0)
                }
                Err(e) => Err(e),
            }
        }
        Some("show") => {
            let json = args.iter().any(|a| a == "--json");
            match args.get(1) {
                Some(id) => match db().and_then(|d| {
                    let run = d.get_run(id).map_err(|e| e.to_string())?;
                    Ok((d, run))
                }) {
                    Ok((d, run)) => {
                        if json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&runs_json(std::slice::from_ref(
                                    &run
                                )))
                                .unwrap_or_else(|_| "{}".into())
                            );
                        } else {
                            print_run_human(&run, &d);
                        }
                        Ok(0)
                    }
                    Err(e) => Err(e),
                },
                None => Err("usage: supercli runs show <id>".to_string()),
            }
        }
        Some("resume") => match args.get(1) {
            Some(id) => {
                let outcome: Result<(), String> = (|| {
                    let d = db()?;
                    let claim = d
                        .claim_run(id)
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| format!("run {id} is held by another live worker"))?;
                    let run = d.get_run(id).map_err(|e| e.to_string())?;
                    println!(
                        "claimed {} (generation {}, took_over={})",
                        id, claim.generation, claim.took_over
                    );
                    print_run_human(&run, &d);
                    // Nudge a QUEUED run into the model loop; a worker that
                    // died mid-step leaves its last state behind.
                    if run.state == RunState::Queued {
                        let _ = d.transition(id, RunState::Queued, RunState::AwaitingModel);
                    }
                    Ok(())
                })();
                match outcome {
                    Ok(()) => Ok(0),
                    Err(e) => Err(e),
                }
            }
            None => Err("usage: supercli runs resume <id>".to_string()),
        },
        Some("pause") => match args.get(1) {
            Some(id) => match db().and_then(|d| d.pause_run(id).map_err(|e| e.to_string())) {
                Ok(true) => {
                    println!("{id} paused");
                    Ok(0)
                }
                Ok(false) => Err(format!("run {id} cannot be paused from its current state")),
                Err(e) => Err(e),
            },
            None => Err("usage: supercli runs pause <id>".to_string()),
        },
        Some("cancel") => match args.get(1) {
            Some(id) => match db().and_then(|d| d.cancel_run(id).map_err(|e| e.to_string())) {
                Ok(true) => {
                    println!("{id} cancelled");
                    Ok(0)
                }
                Ok(false) => Err(format!("run {id} is already terminal")),
                Err(e) => Err(e),
            },
            None => Err("usage: supercli runs cancel <id>".to_string()),
        },
        Some("retry") => match args.get(1) {
            Some(id) => match db().and_then(|d| d.retry_run(id).map_err(|e| e.to_string())) {
                Ok(true) => {
                    println!("{id} queued for retry (journal kept)");
                    Ok(0)
                }
                Ok(false) => Err(format!("run {id} is not in FAILED or NEEDS_REVIEW")),
                Err(e) => Err(e),
            },
            None => Err("usage: supercli runs retry <id>".to_string()),
        },
        Some("--help" | "-h" | "help") | None => {
            println!("{HELP}");
            Ok(0)
        }
        Some(other) => Err(format!("unknown runs subcommand {other:?}\n{HELP}")),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("supercli runs: {e}");
            1
        }
    }
}

fn runs_json(runs: &[supercli_core::durable_runs::Run]) -> serde_json::Value {
    serde_json::Value::Array(
        runs.iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "parent_run": r.parent_run,
                    "state": r.state.as_str(),
                    "steps_done": r.steps_done,
                    "steps_total": r.steps_total,
                    "plan": r.plan,
                    "budgets": r.budgets,
                    "lease_owner": r.lease_owner,
                    "lease_generation": r.lease_generation,
                    "lease_expires_at": r.lease_expires_at,
                    "created_at_ms": r.created_at_ms,
                    "updated_at_ms": r.updated_at_ms,
                })
            })
            .collect(),
    )
}
