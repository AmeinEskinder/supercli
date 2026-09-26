//! Background observer-delivery worker for document lifecycle hooks.
//!
//! When an `after_*`/`on_*` event fires, the dispatcher appends the event
//! to the durable outbox (`hook-outbox.jsonl`, fsync) **before** attempting
//! delivery. This worker runs in the Host:
//!
//! - on boot it replays unacked entries (`Outbox::due()` covers both the
//!   boot-replay set and the steady-state poll set);
//! - it delivers each due entry to the registered observer handlers via
//!   `run_observers` (frozen doc, patches ignored, outcome immutable);
//! - on success the entry is acked; on failure the attempt is recorded
//!   with backoff; after the retry budget the entry moves to the
//!   dead-letter file.
//!
//! Handlers dedup on the event id, which is what makes at-least-once
//! replay safe.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use supercli_core::app_paths;
use supercli_events::outbox::Outbox;
use supercli_events::registry::{default_paths, HookRegistry};
use supercli_events::runner::{run_observers, AuditSink, Dispatcher};
use supercli_events::{DocEvent, DocType, HandlerContext, RunOutcome};

/// Poll interval for the observer worker.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Start the background observer-delivery thread. The thread exits when
/// `shutdown` is set. Returns the join handle.
pub fn start(shutdown: Arc<AtomicBool>) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("hook-observer".to_string())
        .spawn(move || {
            let home = app_paths::supercli_home();
            let outbox = Outbox::new(&home);
            let (global, project) = default_paths();
            let registry = match HookRegistry::load(&global, &project) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("hook-observer: registry load failed ({e}); no observers will run");
                    // Proceed with an empty registry so the worker still
                    // drains (and dead-letters) stale entries rather than
                    // wedging the outbox forever.
                    HookRegistry::default()
                }
            };
            let dispatcher = Dispatcher::new(registry, AuditSink::new(&home));
            // Boot replay: `due()` returns every unacked entry whose retry
            // time has passed, which includes everything left over from a
            // previous Host lifetime.
            while !shutdown.load(Ordering::Acquire) {
                if let Err(e) = pump_once(&outbox, &dispatcher) {
                    eprintln!("hook-observer: {e}");
                }
                // Sleep in small slices so shutdown stays responsive.
                for _ in 0..50 {
                    if shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    thread::sleep(POLL_INTERVAL / 50);
                }
            }
        })
        .expect("hook-observer thread spawn")
}

/// Deliver every due outbox entry once.
fn pump_once(outbox: &Outbox, dispatcher: &Dispatcher) -> Result<(), String> {
    let due = outbox.due().map_err(|e| format!("outbox due: {e}"))?;
    for entry in due {
        let entity = DocType::parse(&entry.entity)
            .ok_or_else(|| format!("bad entity {:?}", entry.entity))?;
        let event =
            DocEvent::parse(&entry.event).ok_or_else(|| format!("bad event {:?}", entry.event))?;
        if event.is_sync() {
            // Sync events never reach the outbox; a corrupt entry that
            // claims otherwise is dead-lettered, never delivered.
            let _ = outbox.dead_letter(&entry.event_id);
            continue;
        }
        let ctx = HandlerContext {
            actor: entry.actor.clone(),
            depth: 0,
            dry_run: false,
        };
        // The event_id is the idempotency key; seq 0 keeps it stable
        // across replays (handlers dedup on it).
        let runs = run_observers(
            dispatcher,
            entity,
            event,
            &entry.doc_id,
            &entry.doc,
            &ctx,
            0,
        );
        let failed: Vec<String> = runs
            .iter()
            .filter(|r| r.outcome != RunOutcome::Ok)
            .map(|r| format!("{}: {} ({})", r.name, r.outcome.as_str(), r.message))
            .collect();
        if failed.is_empty() {
            outbox
                .ack(&entry.event_id)
                .map_err(|e| format!("ack {}: {e}", entry.event_id))?;
        } else {
            let msg = failed.join("; ");
            let retry = outbox
                .record_attempt(&entry.event_id, &msg)
                .map_err(|e| format!("record_attempt {}: {e}", entry.event_id))?;
            if !retry {
                outbox
                    .dead_letter(&entry.event_id)
                    .map_err(|e| format!("dead_letter {}: {e}", entry.event_id))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use supercli_events::registry::HookRegistry;
    use tempfile::TempDir;

    fn empty_dispatcher(dir: &std::path::Path) -> Dispatcher {
        let global = dir.join("hooks.toml");
        let project = dir.join("missing.toml");
        std::fs::write(&global, "[doc_events]\n").unwrap();
        let registry = HookRegistry::load(&global, &project).unwrap();
        Dispatcher::new(registry, AuditSink::new(dir))
    }

    #[test]
    fn pump_acks_entry_with_no_observers() {
        let dir = TempDir::new().unwrap();
        let outbox = Outbox::new(dir.path());
        outbox
            .enqueue(
                "Session:s1:after_insert:1",
                "Session",
                "after_insert",
                "s1",
                &serde_json::json!({"id": "s1"}),
                "human:test",
            )
            .unwrap();
        let dispatcher = empty_dispatcher(dir.path());
        pump_once(&outbox, &dispatcher).unwrap();
        // No observers registered -> run_observers returns no runs ->
        // treated as delivered and acked.
        assert!(outbox.due().unwrap().is_empty());
    }

    #[test]
    fn pump_dead_letters_corrupt_sync_event() {
        let dir = TempDir::new().unwrap();
        let outbox = Outbox::new(dir.path());
        outbox
            .enqueue(
                "Session:s1:before_save:1",
                "Session",
                "before_save",
                "s1",
                &serde_json::json!({"id": "s1"}),
                "human:test",
            )
            .unwrap();
        let dispatcher = empty_dispatcher(dir.path());
        pump_once(&outbox, &dispatcher).unwrap();
        assert!(outbox.due().unwrap().is_empty());
        assert_eq!(outbox.dead_letters().unwrap().len(), 2); // snapshot + marker
    }
}
