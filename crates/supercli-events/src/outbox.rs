//! At-least-once observer delivery (design §9).
//!
//! When an `after_*`/`on_*` event fires, the dispatcher appends the event
//! to `~/.supercli/hook-outbox.jsonl` (fsync) **before** attempting
//! delivery. A background worker in the Host delivers to each registered
//! observer handler; on success the entry is acked, on failure it is
//! retried with backoff, and after the retry budget it moves to the
//! dead-letter file. On Host boot, unacked entries are replayed. Handlers
//! dedup on the event id, which is what makes replay safe.
//!
//! The outbox file is append-only: acks are separate `{"type":"ack"}`
//! lines, so no line is ever rewritten.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const OUTBOX_FILE: &str = "hook-outbox.jsonl";
pub const DEAD_LETTER_FILE: &str = "hook-dead-letter.jsonl";

/// Retry backoff in seconds: 1s, 5s, 30s, 5min, then 1h. After
/// [`MAX_ATTEMPTS`] the entry goes to the dead-letter file.
const BACKOFF_SECS: &[u64] = &[1, 5, 30, 300, 3600];
const MAX_ATTEMPTS: u32 = 8;

/// One pending observer delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub event_id: String,
    pub entity: String,
    pub event: String,
    pub doc_id: String,
    pub doc: serde_json::Value,
    pub actor: String,
    pub enqueued_ms: u64,
    pub attempts: u32,
    pub next_retry_ms: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AckLine {
    #[serde(rename = "type")]
    kind: String,
    event_id: String,
    acked_ms: u64,
}

/// Append-only durable outbox.
pub struct Outbox {
    dir: PathBuf,
}

impl Outbox {
    pub fn new(dir: &Path) -> Self {
        Outbox {
            dir: dir.to_path_buf(),
        }
    }

    fn outbox_path(&self) -> PathBuf {
        self.dir.join(OUTBOX_FILE)
    }

    fn dead_letter_path(&self) -> PathBuf {
        self.dir.join(DEAD_LETTER_FILE)
    }

    fn append_line(&self, path: &Path, line: &str) -> std::io::Result<()> {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    }

    /// Enqueue an observer event. Durable (fsync) before returning — the
    /// caller attempts delivery only after this returns Ok.
    pub fn enqueue(
        &self,
        event_id: &str,
        entity: &str,
        event: &str,
        doc_id: &str,
        doc: &serde_json::Value,
        actor: &str,
    ) -> std::io::Result<()> {
        let entry = OutboxEntry {
            event_id: event_id.to_string(),
            entity: entity.to_string(),
            event: event.to_string(),
            doc_id: doc_id.to_string(),
            doc: doc.clone(),
            actor: actor.to_string(),
            enqueued_ms: now_ms(),
            attempts: 0,
            next_retry_ms: now_ms(),
            last_error: None,
        };
        let entry_line = serde_json::to_string(&entry).unwrap();
        self.append_line(&self.outbox_path(), &entry_line)
    }

    /// Mark an event delivered (dedup record). Idempotent.
    pub fn ack(&self, event_id: &str) -> std::io::Result<()> {
        let ack = AckLine {
            kind: "ack".to_string(),
            event_id: event_id.to_string(),
            acked_ms: now_ms(),
        };
        self.append_line(&self.outbox_path(), &serde_json::to_string(&ack).unwrap())
    }

    /// Record a failed attempt: bump the counter, schedule the next retry
    /// with backoff. Returns `true` when the entry is still retryable,
    /// `false` when it exhausted the budget (caller should dead-letter it).
    pub fn record_attempt(&self, event_id: &str, error: &str) -> std::io::Result<bool> {
        let mut entries = self.load_all()?;
        let Some(entry) = entries.get_mut(event_id) else {
            return Ok(false);
        };
        entry.attempts += 1;
        entry.last_error = Some(error.to_string());
        if entry.attempts >= MAX_ATTEMPTS {
            return Ok(false);
        }
        let backoff = BACKOFF_SECS
            .get(entry.attempts as usize - 1)
            .copied()
            .unwrap_or(3600);
        entry.next_retry_ms = now_ms() + backoff * 1000;
        // Re-append the updated entry (append-only log; latest wins).
        let line = serde_json::to_string(entry).unwrap();
        self.append_line(&self.outbox_path(), &line)?;
        Ok(true)
    }

    /// Move an exhausted entry to the dead-letter file with its history.
    pub fn dead_letter(&self, event_id: &str) -> std::io::Result<()> {
        let entries = self.load_all()?;
        if let Some(entry) = entries.get(event_id) {
            // Snapshot the exhausted entry, then a dead-letter marker.
            let snap = serde_json::to_string(entry).unwrap();
            self.append_line(&self.dead_letter_path(), &snap)?;
            self.append_line(
                &self.dead_letter_path(),
                &format!(
                    "{{\"type\":\"dead_letter\",\"event_id\":{:?},\"at_ms\":{}}}",
                    event_id,
                    now_ms()
                ),
            )?;
        }
        // Ack it in the outbox so replay skips it.
        self.ack(event_id)
    }

    /// Load the latest state of every entry: last write wins per event_id,
    /// minus acked ones.
    pub fn load_all(&self) -> std::io::Result<HashMap<String, OutboxEntry>> {
        let mut entries: HashMap<String, OutboxEntry> = HashMap::new();
        let mut acked: HashSet<String> = HashSet::new();
        let path = self.outbox_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(entries),
            Err(e) => return Err(e),
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let v: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue, // torn tail line: ignore; writer fsyncs whole lines
            };
            match v.get("type").and_then(|t| t.as_str()) {
                Some("ack") => {
                    if let Some(id) = v.get("event_id").and_then(|i| i.as_str()) {
                        acked.insert(id.to_string());
                    }
                }
                _ => {
                    if let Ok(entry) = serde_json::from_value::<OutboxEntry>(v) {
                        entries.insert(entry.event_id.clone(), entry);
                    }
                }
            }
        }
        entries.retain(|id, _| !acked.contains(id));
        Ok(entries)
    }

    /// Entries due for delivery now (unacked, retry time reached), oldest
    /// first. This is the Host boot replay set and the worker's poll set.
    pub fn due(&self) -> std::io::Result<Vec<OutboxEntry>> {
        let now = now_ms();
        let mut entries: Vec<_> = self
            .load_all()?
            .into_values()
            .filter(|e| e.next_retry_ms <= now)
            .collect();
        entries.sort_by_key(|e| e.enqueued_ms);
        Ok(entries)
    }

    /// `true` when the event was delivered (acked) — the handler-side
    /// dedup check.
    pub fn is_delivered(&self, event_id: &str) -> std::io::Result<bool> {
        Ok(!self.load_all()?.contains_key(event_id) && self.was_ever_enqueued(event_id)?)
    }

    fn was_ever_enqueued(&self, event_id: &str) -> std::io::Result<bool> {
        let path = self.outbox_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        for line in text.lines() {
            if line.contains(event_id) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Dead-letter entries (for operator inspection).
    pub fn dead_letters(&self) -> std::io::Result<Vec<serde_json::Value>> {
        let path = self.dead_letter_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        Ok(text
            .lines()
            .filter_map(|l| serde_json::from_str(l.trim()).ok())
            .collect())
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::TempDir::new().unwrap()
    }

    #[test]
    fn enqueue_ack_dedup_cycle() {
        let d = tmp();
        let ob = Outbox::new(d.path());
        ob.enqueue(
            "E:1:on_update:1",
            "E",
            "on_update",
            "1",
            &serde_json::json!({}),
            "human:x",
        )
        .unwrap();
        assert_eq!(ob.due().unwrap().len(), 1);
        assert!(!ob.is_delivered("E:1:on_update:1").unwrap());
        ob.ack("E:1:on_update:1").unwrap();
        assert!(ob.due().unwrap().is_empty());
        assert!(ob.is_delivered("E:1:on_update:1").unwrap());
        // Ack is idempotent.
        ob.ack("E:1:on_update:1").unwrap();
        assert!(ob.due().unwrap().is_empty());
    }

    #[test]
    fn retry_backoff_then_dead_letter() {
        let d = tmp();
        let ob = Outbox::new(d.path());
        ob.enqueue(
            "E:2:on_update:1",
            "E",
            "on_update",
            "2",
            &serde_json::json!({}),
            "human:x",
        )
        .unwrap();
        // Exhaust the budget.
        for _ in 0..MAX_ATTEMPTS {
            let retryable = ob.record_attempt("E:2:on_update:1", "boom").unwrap();
            if !retryable {
                break;
            }
        }
        let retryable = ob.record_attempt("E:2:on_update:1", "boom").unwrap();
        assert!(!retryable);
        ob.dead_letter("E:2:on_update:1").unwrap();
        assert!(ob.due().unwrap().is_empty());
        assert_eq!(ob.dead_letters().unwrap().len(), 2); // snapshot + marker
    }

    #[test]
    fn at_least_once_dedup_across_restart() {
        use std::sync::{Arc, Mutex};
        let d = tmp();
        // "Host" 1: enqueue, then die mid-delivery (no ack).
        let ob1 = Outbox::new(d.path());
        ob1.enqueue(
            "E:3:on_update:1",
            "E",
            "on_update",
            "3",
            &serde_json::json!({"n": 1}),
            "human:x",
        )
        .unwrap();

        // Idempotent handler: applies the effect once per event_id.
        let effects: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
        let deliver = |ob: &Outbox, effects: &Arc<Mutex<HashSet<String>>>| -> usize {
            let mut delivered = 0;
            for entry in ob.due().unwrap() {
                // Handler-side dedup on the event id.
                let mut fx = effects.lock().unwrap();
                if fx.insert(entry.event_id.clone()) {
                    delivered += 1; // effect applied
                }
                drop(fx);
                ob.ack(&entry.event_id).unwrap();
            }
            delivered
        };

        // First delivery attempt "crashes" after applying the effect but
        // before acking: simulate by applying without ack.
        {
            let entry = ob1.due().unwrap().pop().unwrap();
            effects.lock().unwrap().insert(entry.event_id.clone());
            // no ack — crash here
        }
        // "Host" 2 boots: replay finds the unacked entry.
        let ob2 = Outbox::new(d.path());
        assert_eq!(ob2.due().unwrap().len(), 1);
        let applied = deliver(&ob2, &effects);
        assert_eq!(
            applied, 0,
            "effect already applied; dedup made replay a no-op"
        );
        assert_eq!(
            effects.lock().unwrap().len(),
            1,
            "effect applied exactly once"
        );
        assert!(ob2.due().unwrap().is_empty());
    }
}
