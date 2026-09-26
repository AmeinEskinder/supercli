//! CLI-level tests for `supercli runs` (durable run management).
//!
//! Seeds runs through the real [`supercli_core::durable_runs`] API, then
//! drives the `supercli` binary against a private `SUPERCLI_HOME`.

use std::process::Command;

use supercli_core::action_reviews::AttemptOutcome;
use supercli_core::durable_runs::{step_input_hash, RunState, RunsDb, StepKind};

fn private_home(tag: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("supercli-runs-cli-{tag}-"))
        .tempdir()
        .unwrap()
}

fn run_cli(home: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_supercli"))
        .env("SUPERCLI_HOME", home)
        .args(args)
        .output()
        .expect("spawn supercli");
    let code = output.status.code().unwrap_or(-1);
    (
        code,
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn seed_run(home: &std::path::Path, tag: &str) -> String {
    let db = RunsDb::open(home).unwrap();
    let id = db.create_run(None, r#"{"task":"demo"}"#, "{}").unwrap();
    db.append_step(
        &id,
        0,
        StepKind::Tool,
        &step_input_hash(&format!("{tag}:0")),
        Some(r#"{"ok":true}"#),
        Some(&AttemptOutcome::Executed { success: true }),
    )
    .unwrap();
    id
}

#[test]
fn runs_help_and_empty_list() {
    let home = private_home("help");
    let (code, out, _) = run_cli(home.path(), &["runs"]);
    assert_eq!(code, 0);
    assert!(out.contains("durable run management"));
    let (code, out, _) = run_cli(home.path(), &["runs", "list"]);
    assert_eq!(code, 0);
    assert!(out.contains("no runs yet"));
}

#[test]
fn runs_list_show_resume_pause_cancel_retry() {
    let home = private_home("flow");
    let id = seed_run(home.path(), "flow");

    // list
    let (code, out, _) = run_cli(home.path(), &["runs", "list"]);
    assert_eq!(code, 0);
    assert!(out.contains(&id), "list shows the run: {out}");
    assert!(out.contains("QUEUED"));

    // show
    let (code, out, _) = run_cli(home.path(), &["runs", "show", &id]);
    assert_eq!(code, 0);
    assert!(out.contains(&id));
    assert!(out.contains("1/1 steps"), "show shows progress: {out}");

    // resume claims and reports the continuation point
    let (code, out, _) = run_cli(home.path(), &["runs", "resume", &id]);
    assert_eq!(code, 0);
    assert!(out.contains("claimed"), "resume claims: {out}");
    assert!(out.contains("continue at step 1"), "resume plan: {out}");

    // pause
    let (code, out, _) = run_cli(home.path(), &["runs", "pause", &id]);
    assert_eq!(code, 0);
    assert!(out.contains("paused"));
    assert_eq!(
        RunsDb::open(home.path())
            .unwrap()
            .get_run(&id)
            .unwrap()
            .state,
        RunState::Paused
    );

    // retry is refused while PAUSED (not FAILED/NEEDS_REVIEW)
    let (code, _, err) = run_cli(home.path(), &["runs", "retry", &id]);
    assert_eq!(code, 1);
    assert!(
        err.contains("not in FAILED or NEEDS_REVIEW"),
        "stderr: {err}"
    );

    // cancel
    let (code, out, _) = run_cli(home.path(), &["runs", "cancel", &id]);
    assert_eq!(code, 0);
    assert!(out.contains("cancelled"));

    // retry from FAILED works and keeps the journal
    let (code, out, _) = run_cli(home.path(), &["runs", "retry", &id]);
    assert_eq!(code, 0);
    assert!(out.contains("queued for retry"));
    let db = RunsDb::open(home.path()).unwrap();
    assert_eq!(db.get_run(&id).unwrap().state, RunState::Queued);
    assert_eq!(db.resume_run(&id).unwrap().completed.len(), 1);

    // unknown subcommand / missing id
    let (code, _, err) = run_cli(home.path(), &["runs", "bogus"]);
    assert_eq!(code, 1);
    assert!(err.contains("unknown runs subcommand"));
    let (code, _, _) = run_cli(home.path(), &["runs", "show"]);
    assert_eq!(code, 1);
}

#[test]
fn runs_list_filters_and_json() {
    let home = private_home("filter");
    let id = seed_run(home.path(), "filter");
    let db = RunsDb::open(home.path()).unwrap();
    db.pause_run(&id).unwrap();

    let (code, out, _) = run_cli(home.path(), &["runs", "list", "--state", "PAUSED"]);
    assert_eq!(code, 0);
    assert!(out.contains(&id));
    let (code, out, _) = run_cli(home.path(), &["runs", "list", "--state", "QUEUED"]);
    assert_eq!(code, 0);
    assert!(!out.contains(&id));

    let (code, out, _) = run_cli(home.path(), &["runs", "list", "--json"]);
    assert_eq!(code, 0);
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(parsed.as_array().unwrap().len(), 1);
    assert_eq!(parsed[0]["state"], "PAUSED");
    assert_eq!(parsed[0]["steps_done"], 1);
}
