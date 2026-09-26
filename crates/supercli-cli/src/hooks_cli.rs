//! `supercli hooks` — Frappe-style document lifecycle hooks.
//!
//! Users script supercli through `hooks.toml` (global `~/.supercli/hooks.toml`,
//! per-project `.supercli/hooks.toml`). This CLI surfaces the registry:
//!
//! - `hooks list` — every registered handler
//! - `hooks test <name>` — dry-run one handler against a synthetic doc
//! - `hooks trace [--limit N]` — recent handler runs

use supercli_events::registry::{default_paths, HookRegistry};
use supercli_events::{cli as hooks_cli, DocEvent, DocType};

pub const HELP: &str = "\
supercli hooks — document lifecycle hooks (hooks.toml)

  supercli hooks list [--json]     list registered handlers
  supercli hooks test <name> [--entity E] [--event V]
                                  dry-run one handler against a synthetic doc
  supercli hooks trace [--limit N] recent handler runs (in-memory)

Handlers are declared in hooks.toml:
  global:  ~/.supercli/hooks.toml
  project: .supercli/hooks.toml (cwd at invocation)

Example:
  [doc_events.Session]
  after_insert = [{ name = \"notify\", command = [\"notify-send\", \"session\"] }]
";

fn load_registry() -> Result<HookRegistry, String> {
    let (global, project) = default_paths();
    HookRegistry::load(&global, &project)
}

pub fn run(args: &[String]) -> i32 {
    let Some(sub) = args.first().map(String::as_str) else {
        println!("{HELP}");
        return 0;
    };
    let result: Result<i32, String> = match sub {
        "list" => list(&args[1..]),
        "test" => test(&args[1..]),
        "trace" => trace(&args[1..]),
        "--help" | "-h" | "help" => {
            println!("{HELP}");
            Ok(0)
        }
        other => Err(format!("unknown hooks subcommand {other:?}\n{HELP}")),
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("supercli hooks: {message}");
            1
        }
    }
}

fn list(args: &[String]) -> Result<i32, String> {
    let registry = load_registry()?;
    let rows = hooks_cli::list(&registry);
    let json = args.iter().any(|a| a == "--json");
    if json {
        let out: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "entity": r.entity,
                    "event": r.event,
                    "priority": r.priority,
                    "target": r.target,
                    "source": r.source,
                    "disabled": r.disabled,
                    "disabled_reason": r.disabled_reason,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else if rows.is_empty() {
        println!("No hooks registered. Add handlers to ~/.supercli/hooks.toml or .supercli/hooks.toml.");
    } else {
        println!("{:<24} {:<12} {:<16} {:<8} {}", "NAME", "ENTITY", "EVENT", "PRIO", "TARGET");
        for r in &rows {
            if r.disabled {
                println!(
                    "{:<24} {:<12} {:<16} {:<8} [disabled: {}]",
                    r.name,
                    r.entity,
                    r.event,
                    r.priority,
                    r.disabled_reason.as_deref().unwrap_or("?")
                );
            } else {
                println!(
                    "{:<24} {:<12} {:<16} {:<8} {} ({})",
                    r.name, r.entity, r.event, r.priority, r.target, r.source
                );
            }
        }
    }
    Ok(0)
}

fn test(args: &[String]) -> Result<i32, String> {
    let name = args
        .first()
        .ok_or_else(|| "usage: supercli hooks test <name> [--entity E] [--event V]".to_string())?;
    let mut entity = DocType::Session;
    let mut event = DocEvent::AfterInsert;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--entity" => {
                i += 1;
                let s = args.get(i).ok_or("--entity needs a value")?;
                entity = DocType::parse(s)
                    .ok_or_else(|| format!("unknown entity {s:?}"))?;
            }
            "--event" => {
                i += 1;
                let s = args.get(i).ok_or("--event needs a value")?;
                event = DocEvent::parse(s)
                    .ok_or_else(|| format!("unknown event {s:?}"))?;
            }
            other => return Err(format!("unknown flag {other:?}")),
        }
        i += 1;
    }
    let registry = load_registry()?;
    let doc_fields = serde_json::json!({});
    match hooks_cli::test(&registry, name, entity, event, &doc_fields) {
        Ok(run) => {
            println!("handler: {}", run.name);
            println!("decision: {}", run.decision.as_str());
            println!("outcome: {}", run.outcome.as_str());
            println!("elapsed_ms: {}", run.elapsed_ms);
            if !run.message.is_empty() {
                println!("message: {}", run.message);
            }
            if run.outcome.as_str() != "ok" {
                eprintln!("note: handler did not complete cleanly (fail-closed)");
                return Ok(2);
            }
            Ok(0)
        }
        Err(e) => Err(e.to_string()),
    }
}

fn trace(args: &[String]) -> Result<i32, String> {
    // The dispatcher's trace ring is per-process (in-memory). For durable
    // visibility, the outbox persists every observer delivery: pending /
    // retrying entries in hook-outbox.jsonl, exhausted ones in the
    // dead-letter file. Both live under the Supercli home.
    let mut limit = 20usize;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--limit" => {
                i += 1;
                let s = args.get(i).ok_or("--limit needs a value")?;
                limit = s.parse().map_err(|_| "invalid --limit")?;
            }
            other => return Err(format!("unknown flag {other:?}")),
        }
        i += 1;
    }
    let home = supercli_core::app_paths::supercli_home();
    let outbox = supercli_events::outbox::Outbox::new(&home);

    let mut pending = outbox.load_all().map_err(|e| format!("outbox: {e}"))?;
    let mut pending_vec: Vec<_> = pending.drain().map(|(_, e)| e).collect();
    pending_vec.sort_by_key(|e| e.enqueued_ms);
    println!("Pending observer deliveries: {}", pending_vec.len());
    for e in pending_vec.iter().take(limit) {
        let retry_in = e.next_retry_ms.saturating_sub(now_ms()) / 1000;
        println!(
            "  {} {}:{} attempts={} retry_in={}s{}",
            e.event_id,
            e.entity,
            e.event,
            e.attempts,
            retry_in,
            e.last_error
                .as_deref()
                .map(|m| format!(" error={m}"))
                .unwrap_or_default(),
        );
    }

    let dead = outbox
        .dead_letters()
        .map_err(|e| format!("dead letters: {e}"))?;
    println!("Dead letters: {}", dead.len());
    for d in dead.iter().take(limit) {
        println!("  {d}");
    }
    Ok(0)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
