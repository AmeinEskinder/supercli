//! `supercli schedule` — manage and drive scheduled autonomous sessions.
//!
//! A schedule arms an explicit, operator-written list of connector tool
//! calls to run unattended against one named session. Nothing here is
//! agentic: the runner executes exactly the listed tools in order, with no
//! human present — so `Ask` tools are denied outright, explicit `Deny`
//! stays denied, and every trigger appends one audit record to
//! `<session-dir>/scheduled-runs.jsonl`.
//!
//! Safety rules enforced by this module and `supercli_core::scheduled`:
//! * schedules are explicit operator opt-in (`add`); nothing runs without one;
//! * one named session per schedule; the session must exist at `add` time;
//! * defaults are 30 min / 200 steps / 4 MiB / 0 retries, adjustable per
//!   schedule within the absolute ceilings (validated, never silent);
//! * absolute ceilings: 60 s minimum interval, 24 h max duration, 10 000
//!   max steps, 3 max retries;
//! * single-flight per schedule: an overlapping trigger is skipped, never
//!   queued;
//! * persistent failure (after retries) notifies exactly once per trigger;
//! * `daemon` is the only supported driver — do not drive schedules from
//!   system cron, which would break the single-flight guarantee.

use std::path::Path;
use supercli_core::app_paths;
use supercli_core::scheduled::{
    load_schedules, save_schedules, schedules_path, AutonomousPolicy, RunOutcome, RunRecord,
    ScheduleSpec, ScheduledRunner, ScheduledTask, ScheduledToolCall, Scheduler, SystemClock,
};
use supercli_core::session_connectors::SessionConnectors;
use supercli_core::session_host;

pub const HELP: &str = "\
supercli schedule — scheduled autonomous sessions (explicit operator opt-in)

  supercli schedule add --id ID --session SID --interval SECS \\
      --tool TOOL [--arg KEY=VALUE ...] [--tool TOOL ...] \\
      [--max-duration SECS] [--max-steps N] [--max-output BYTES] [--max-retries N]
                                  arm a schedule: run the listed connector
                                  tools in order, every SECS (>= 60)
  supercli schedule list [--json]    list armed schedules
  supercli schedule pause <id>      pause (takes effect before the next trigger)
  supercli schedule resume <id>     resume a paused schedule
  supercli schedule remove <id>     delete a schedule (audit logs are kept)
  supercli schedule run-once <id> [--json]
                                  fire one trigger now (audited like any trigger)
  supercli schedule daemon          fire due triggers until killed; run this
                                  under systemd/launchd, not cron

A scheduled run executes an explicit ordered list of connector tool calls
with no human present: Ask tools are denied immediately, explicit Deny
stays denied. Every trigger appends one record to
<session-dir>/scheduled-runs.jsonl. Nothing runs unless a schedule exists.
";

pub fn run(args: &[String]) -> i32 {
    let Some(sub) = args.first().map(String::as_str) else {
        println!("{HELP}");
        return 0;
    };
    let result: Result<i32, String> = match sub {
        "add" => add(&args[1..]),
        "list" => list(&args[1..]),
        "pause" => set_enabled(&args[1..], false),
        "resume" => set_enabled(&args[1..], true),
        "remove" => remove(&args[1..]),
        "run-once" => run_once(&args[1..]),
        "daemon" => daemon(&args[1..]),
        "--help" | "-h" | "help" => {
            println!("{HELP}");
            Ok(0)
        }
        other => Err(format!("unknown schedule subcommand {other:?}\n{HELP}")),
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("supercli schedule: {message}");
            1
        }
    }
}

fn single_id(args: &[String], verb: &str) -> Result<String, String> {
    match args {
        [id] => Ok(id.clone()),
        _ => Err(format!("usage: supercli schedule {verb} <id>")),
    }
}

// ---------------------------------------------------------------------------
// add
// ---------------------------------------------------------------------------

struct AddArgs {
    id: Option<String>,
    session: Option<String>,
    interval: Option<u64>,
    tools: Vec<(String, Vec<(String, String)>)>,
    max_duration: Option<u64>,
    max_steps: Option<u32>,
    max_output: Option<u64>,
    max_retries: Option<u8>,
    json: bool,
}

fn flag_value(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

fn parse_add(args: &[String]) -> Result<AddArgs, String> {
    let mut out = AddArgs {
        id: None,
        session: None,
        interval: None,
        tools: Vec::new(),
        max_duration: None,
        max_steps: None,
        max_output: None,
        max_retries: None,
        json: false,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--id" => out.id = Some(flag_value(args, &mut i, "--id")?),
            "--session" => out.session = Some(flag_value(args, &mut i, "--session")?),
            "--interval" => {
                out.interval = Some(
                    flag_value(args, &mut i, "--interval")?
                        .parse::<u64>()
                        .map_err(|_| "invalid --interval: expected seconds")?,
                );
            }
            "--tool" => {
                let tool = flag_value(args, &mut i, "--tool")?;
                out.tools.push((tool, Vec::new()));
            }
            "--arg" => {
                let kv = flag_value(args, &mut i, "--arg")?;
                let (key, value) = kv.split_once('=').ok_or("--arg must be KEY=VALUE")?;
                let last = out
                    .tools
                    .last_mut()
                    .ok_or("--arg given before any --tool")?;
                last.1.push((key.to_string(), value.to_string()));
            }
            "--max-duration" => {
                out.max_duration = Some(
                    flag_value(args, &mut i, "--max-duration")?
                        .parse::<u64>()
                        .map_err(|_| "invalid --max-duration: expected seconds")?,
                );
            }
            "--max-steps" => {
                out.max_steps = Some(
                    flag_value(args, &mut i, "--max-steps")?
                        .parse::<u32>()
                        .map_err(|_| "invalid --max-steps: expected a number")?,
                );
            }
            "--max-output" => {
                out.max_output = Some(
                    flag_value(args, &mut i, "--max-output")?
                        .parse::<u64>()
                        .map_err(|_| "invalid --max-output: expected bytes")?,
                );
            }
            "--max-retries" => {
                out.max_retries = Some(
                    flag_value(args, &mut i, "--max-retries")?
                        .parse::<u8>()
                        .map_err(|_| "invalid --max-retries: expected 0-3")?,
                );
            }
            "--json" => out.json = true,
            other => return Err(format!("unknown flag {other}")),
        }
        i += 1;
    }
    Ok(out)
}

fn add(args: &[String]) -> Result<i32, String> {
    let parsed = parse_add(args)?;
    let id = parsed.id.ok_or("missing --id")?;
    let session_id = parsed.session.ok_or("missing --session")?;
    let interval_secs = parsed.interval.ok_or("missing --interval")?;
    if parsed.tools.is_empty() {
        return Err("at least one --tool is required".to_string());
    }
    // Fail closed at management time: a typo'd session id arms a schedule
    // that can never succeed. (Sessions archived later still fail closed at
    // run time with an audited Failed record.)
    if !session_host::manifest_path(&session_id).exists() {
        return Err(format!(
            "unknown session {session_id:?}: no session manifest found"
        ));
    }
    let mut policy = AutonomousPolicy::default();
    if let Some(v) = parsed.max_duration {
        policy.max_duration_secs = v;
    }
    if let Some(v) = parsed.max_steps {
        policy.max_steps = v;
    }
    if let Some(v) = parsed.max_output {
        policy.max_output_bytes = v;
    }
    if let Some(v) = parsed.max_retries {
        policy.max_retries = v;
    }
    let task = ScheduledTask::ToolCalls(
        parsed
            .tools
            .into_iter()
            .map(|(tool, kvs)| {
                let mut map = serde_json::Map::new();
                for (key, value) in kvs {
                    map.insert(key, serde_json::Value::String(value));
                }
                ScheduledToolCall {
                    tool,
                    arguments: serde_json::Value::Object(map),
                }
            })
            .collect(),
    );
    let spec = ScheduleSpec {
        id,
        session_id,
        interval_secs,
        policy,
        enabled: true,
        task,
    };
    spec.validate()
        .map_err(|e| format!("invalid schedule: {e}"))?;
    let home = app_paths::supercli_home();
    let mut specs = load_schedules(&home).map_err(|e| e.to_string())?;
    if specs.iter().any(|s| s.id == spec.id) {
        return Err(format!("schedule {:?} already exists", spec.id));
    }
    specs.push(spec.clone());
    save_schedules(&home, &specs).map_err(|e| e.to_string())?;
    if parsed.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&spec).map_err(|e| e.to_string())?
        );
    } else {
        println!(
            "armed schedule {:?}: every {}s, {} tool call(s) on session {:?}",
            spec.id,
            spec.interval_secs,
            spec.task.step_count(),
            spec.session_id,
        );
    }
    Ok(0)
}

// ---------------------------------------------------------------------------
// list / pause / resume / remove
// ---------------------------------------------------------------------------

fn list(args: &[String]) -> Result<i32, String> {
    let json = args.iter().any(|a| a == "--json");
    if args.iter().any(|a| a != "--json") {
        return Err("usage: supercli schedule list [--json]".to_string());
    }
    let home = app_paths::supercli_home();
    let specs = load_schedules(&home).map_err(|e| e.to_string())?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&specs).map_err(|e| e.to_string())?
        );
    } else if specs.is_empty() {
        println!("no schedules armed");
    } else {
        for spec in &specs {
            let state = if spec.enabled { "armed " } else { "paused" };
            println!(
                "{state}  {:<24} every {:>6}s  session {:<20} {} tool call(s)",
                spec.id,
                spec.interval_secs,
                spec.session_id,
                spec.task.step_count(),
            );
        }
    }
    Ok(0)
}

fn set_enabled(args: &[String], enabled: bool) -> Result<i32, String> {
    let verb = if enabled { "resume" } else { "pause" };
    let id = single_id(args, verb)?;
    let home = app_paths::supercli_home();
    let mut specs = load_schedules(&home).map_err(|e| e.to_string())?;
    let spec = specs
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("unknown schedule {id:?}"))?;
    spec.enabled = enabled;
    save_schedules(&home, &specs).map_err(|e| e.to_string())?;
    println!("schedule {id:?} {verb}d");
    Ok(0)
}

fn remove(args: &[String]) -> Result<i32, String> {
    let id = single_id(args, "remove")?;
    let home = app_paths::supercli_home();
    let mut specs = load_schedules(&home).map_err(|e| e.to_string())?;
    let before = specs.len();
    specs.retain(|s| s.id != id);
    if specs.len() == before {
        return Err(format!("unknown schedule {id:?}"));
    }
    save_schedules(&home, &specs).map_err(|e| e.to_string())?;
    // The audit trail stays: scheduled-runs.jsonl lives in the session dir
    // and is never deleted by removing the schedule.
    println!("removed schedule {id:?} (run history kept in the session dir)");
    Ok(0)
}

// ---------------------------------------------------------------------------
// run-once
// ---------------------------------------------------------------------------

fn run_once(args: &[String]) -> Result<i32, String> {
    let (id, json) = match args {
        [id] => (id.clone(), false),
        [id, flag] if flag == "--json" => (id.clone(), true),
        _ => return Err("usage: supercli schedule run-once <id> [--json]".to_string()),
    };
    let home = app_paths::supercli_home();
    let specs = load_schedules(&home).map_err(|e| e.to_string())?;
    let spec = specs
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("unknown schedule {id:?}"))?;
    if !spec.enabled {
        return Err(format!("schedule {id:?} is paused; resume it first"));
    }
    let session_dir = session_host::session_dir(&spec.session_id);
    let mut executor = SessionConnectors::resolve(&spec.session_id, &session_dir);
    let runner = ScheduledRunner::new(SystemClock);
    match runner.run_trigger(spec, &session_dir, &mut executor) {
        Ok(record) => {
            if json {
                println!("{}", record.to_json());
            } else {
                println!(
                    "trigger fired: {:?} outcome={:?} steps={} error={:?}",
                    spec.id, record.outcome, record.steps, record.error,
                );
            }
            Ok(if record.outcome == RunOutcome::Completed {
                0
            } else {
                1
            })
        }
        Err(audit_error) => Err(format!(
            "ran but the audit append failed: {}; record: {}",
            audit_error.source,
            audit_error.record.to_json(),
        )),
    }
}

// ---------------------------------------------------------------------------
// daemon
// ---------------------------------------------------------------------------

/// Persistent-failure notification: exactly once per trigger, after retries
/// are exhausted. Goes through the existing notification path (a no-op
/// without a registered platform adapter, and real delivery if one ever
/// registers) AND a stderr line, because this daemon usually runs headless
/// and the operator watches its logs.
fn notify_failure(spec: &ScheduleSpec, record: &RunRecord) {
    let hub = supercli_serve::platform_adapter::PlatformAdapterHub::default();
    let title = format!("Scheduled run failed: {}", spec.id);
    let body = record
        .error
        .clone()
        .unwrap_or_else(|| format!("outcome: {:?}", record.outcome));
    let _ = supercli_serve::notifications::deliver(
        &hub,
        supercli_serve::notifications::NotificationRequest {
            session_id: &spec.session_id,
            title: &title,
            kind: supercli_serve::notifications::NotificationKind::Alert,
            body: Some(&body),
            requires_notify_when_done: true,
            send_desktop: false,
            suppress_device_ids: std::collections::HashSet::new(),
        },
    );
    eprintln!(
        "scheduled failure: schedule={:?} session={:?} record={}",
        spec.id,
        spec.session_id,
        record.to_json(),
    );
}

fn daemon(args: &[String]) -> Result<i32, String> {
    if !args.is_empty() {
        return Err("usage: supercli schedule daemon (no arguments)".to_string());
    }
    let home = app_paths::ensure_supercli_home().map_err(|e| e.to_string())?;
    eprintln!(
        "supercli schedule daemon: watching {} (Ctrl-C to stop)",
        schedules_path(&home).display()
    );
    let scheduler: Scheduler<SystemClock> = Scheduler::new(SystemClock);
    // Cross-process single-flight via SQL leases: if another daemon holds
    // a schedule's lease, this one skips it; a crashed daemon's leases
    // expire and this one takes over. Fail-closed: without the lease DB
    // the daemon refuses to start rather than risk double-firing.
    let leases = supercli_core::schedule_leases::ScheduleLeases::open(
        &home,
        supercli_core::schedule_leases::DEFAULT_TENANT,
    )
    .map_err(|e| format!("cannot open schedule lease database: {e}"))?;
    let mut scheduler = scheduler.with_lease_store(leases);
    let mut make_executor = |session_id: &str, session_dir: &Path| {
        // Fresh connector set per trigger: no state leaks between runs,
        // and the runner puts it in autonomous mode for the run's duration.
        Box::new(SessionConnectors::resolve(session_id, session_dir))
            as Box<dyn supercli_core::scheduled::ScheduledToolExecutor>
    };
    loop {
        let report = scheduler.tick(
            &home,
            &session_host::session_dir,
            &mut make_executor,
            &mut notify_failure,
        );
        for record in &report.records {
            println!(
                "trigger {:?}: outcome={:?} steps={} denied={:?} error={:?}",
                record.schedule_id, record.outcome, record.steps, record.denied_tools, record.error,
            );
        }
        for error in &report.errors {
            eprintln!("supercli schedule daemon: {error}");
        }
        std::thread::sleep(scheduler.next_wake_in());
    }
}
