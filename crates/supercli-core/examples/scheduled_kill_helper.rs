//! Helper binary for the scheduled SIGKILL test.
//!
//! Runs a scheduled trigger with 3 steps through `ScheduledRunner`, writing
//! each step's side effect to an external append-only file (the ground truth,
//! not the journal). The test SIGKILLs this process mid-run, restarts it,
//! and verifies from the external file that there are zero duplicates and
//! the same run id is resumed.
//!
//! Usage: scheduled_kill_helper --home <dir> --side-effects <path>

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use supercli_core::durable_runs::RunsDb;
use supercli_core::schedule_leases::LeaseFence;
use supercli_core::scheduled::{
    AutonomousPolicy, ScheduleSpec, ScheduledRunner, ScheduledTask, ScheduledToolCall,
    ScheduledToolExecutor, SystemClock,
};

struct FileExecutor {
    side_effects: PathBuf,
    lease_fence: Option<LeaseFence>,
}

impl FileExecutor {
    fn new(side_effects: PathBuf) -> Self {
        Self {
            side_effects,
            lease_fence: None,
        }
    }

    fn record(&self, tool: &str) {
        // Real side effect: append to external file, fsync.
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.side_effects)
            .expect("open side-effects log");
        writeln!(f, "executed {}", tool).expect("write side effect");
        f.sync_all().expect("fsync side-effects log");
    }
}

impl ScheduledToolExecutor for FileExecutor {
    fn set_autonomous(&mut self, _autonomous: bool) {}
    fn is_autonomous(&self) -> bool {
        true
    }
    fn set_actor(&mut self, _actor: String) {}
    fn set_lease_fence(&mut self, fence: Option<LeaseFence>) {
        self.lease_fence = fence;
    }
    fn lease_fence(&self) -> Option<&LeaseFence> {
        self.lease_fence.as_ref()
    }
    fn call_tool_detailed(
        &mut self,
        tool: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, supercli_core::session_connectors::ToolCallFailure> {
        // Widen the kill window: sleep before the side effect.
        std::thread::sleep(Duration::from_millis(400));
        self.record(tool);
        // Sleep after too, so a kill can land between side effect and
        // the outcome journal write.
        std::thread::sleep(Duration::from_millis(200));
        Ok(format!("ok:{}", tool))
    }
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
                eprintln!("usage: scheduled_kill_helper --home <dir> --side-effects <path>");
                std::process::exit(2);
            }
        }
    }
    let home = home.expect("--home required");
    let side_effects = side_effects.expect("--side-effects required");

    // Open (or create) the durable-runs DB. Fail closed: no journal, no run.
    let db = RunsDb::open(&home).expect("open RunsDb");
    let runner = ScheduledRunner::new(SystemClock).with_durable_runs(db);

    // Fixed schedule id so restarts find the orphaned run.
    let spec = ScheduleSpec {
        id: "kill-test".to_string(),
        session_id: "kill-sess".to_string(),
        interval_secs: 3600,
        policy: AutonomousPolicy::default(),
        enabled: true,
        task: ScheduledTask::ToolCalls(vec![
            ScheduledToolCall {
                tool: "step-1".to_string(),
                arguments: serde_json::json!({}),
            },
            ScheduledToolCall {
                tool: "step-2".to_string(),
                arguments: serde_json::json!({}),
            },
            ScheduledToolCall {
                tool: "step-3".to_string(),
                arguments: serde_json::json!({}),
            },
        ]),
    };

    let session_dir = home.join("sess");
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    let mut executor = FileExecutor::new(side_effects);

    match runner.run_trigger(&spec, &session_dir, &mut executor) {
        Ok(record) => {
            println!(
                "done: outcome={:?} steps={} error={:?}",
                record.outcome, record.steps, record.error
            );
        }
        Err(e) => {
            eprintln!("audit failed: {:?}", e);
            std::process::exit(1);
        }
    }
}

// Helper binary entry point (above).
