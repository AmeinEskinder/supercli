//! Persistent agent memory: a small key-value store with scopes.
//!
//! Two scopes:
//! * `Session` — working memory for the current session only. Dropped on
//!   load, never written to disk.
//! * `LongTerm` — promoted facts that survive restarts, persisted to
//!   `<home>/memory.json`.
//!
//! Promotion is explicit (`promote`): the agent (or a hook) decides what is
//! worth keeping. Nothing is promoted automatically — silent accumulation of
//! stale facts is how memory systems rot.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Memory scope.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Session,
    LongTerm,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MemoryEntry {
    pub value: String,
    pub scope: Scope,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MemoryStore {
    #[serde(default)]
    entries: HashMap<String, MemoryEntry>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a fact. Session-scoped entries may carry the session id.
    pub fn set(&mut self, key: &str, value: &str, scope: Scope, session_id: Option<&str>) {
        self.entries.insert(
            key.to_string(),
            MemoryEntry {
                value: value.to_string(),
                scope,
                updated_at: now_secs(),
                session_id: session_id.map(str::to_string),
            },
        );
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|e| e.value.as_str())
    }

    pub fn scope_of(&self, key: &str) -> Option<Scope> {
        self.entries.get(key).map(|e| e.scope)
    }

    /// Promote a session fact to long-term. Returns false if the key is
    /// unknown. Promoting an already-long-term entry is a no-op (true).
    pub fn promote(&mut self, key: &str) -> bool {
        match self.entries.get_mut(key) {
            Some(entry) => {
                entry.scope = Scope::LongTerm;
                entry.session_id = None;
                entry.updated_at = now_secs();
                true
            }
            None => false,
        }
    }

    /// Forget a fact entirely. Returns false if the key was unknown.
    pub fn forget(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }

    /// Keys visible to a session: all long-term facts plus this session's
    /// own session-scoped facts.
    pub fn keys_for_session(&self, session_id: &str) -> Vec<String> {
        let mut keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| {
                e.scope == Scope::LongTerm || e.session_id.as_deref() == Some(session_id)
            })
            .map(|(k, _)| k.clone())
            .collect();
        keys.sort();
        keys
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `<home>/memory.json`.
pub fn memory_path(home: &Path) -> std::path::PathBuf {
    home.join("memory.json")
}

/// Load persisted memory. Session-scoped entries are dropped: only
/// `LongTerm` facts survive a restart. Missing or corrupt files yield an
/// empty store (corrupt is treated as empty, not fatal — memory must never
/// break the host).
pub fn load_memory(home: &Path) -> MemoryStore {
    let path = memory_path(home);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return MemoryStore::new(),
    };
    let mut store: MemoryStore = serde_json::from_str(&text).unwrap_or_default();
    store.entries.retain(|_, e| e.scope == Scope::LongTerm);
    store
}

/// Persist long-term facts only. Best-effort: a memory write must never
/// break the caller.
pub fn save_memory(home: &Path, store: &MemoryStore) {
    let filtered = MemoryStore {
        entries: store
            .entries
            .iter()
            .filter(|(_, e)| e.scope == Scope::LongTerm)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    };
    if let Ok(text) = serde_json::to_string_pretty(&filtered) {
        let _ = fs::write(memory_path(home), text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_set_get_forget_roundtrip() {
        let mut m = MemoryStore::new();
        m.set("user.editor", "hx", Scope::LongTerm, None);
        m.set("turn.plan", "draft", Scope::Session, Some("s1"));
        assert_eq!(m.get("user.editor"), Some("hx"));
        assert_eq!(m.scope_of("turn.plan"), Some(Scope::Session));
        assert!(m.forget("turn.plan"));
        assert!(!m.forget("turn.plan"));
        assert_eq!(m.get("turn.plan"), None);
    }

    #[test]
    fn memory_promote_session_to_long_term() {
        let mut m = MemoryStore::new();
        m.set("user.timezone", "UTC+3", Scope::Session, Some("s1"));
        assert!(m.promote("user.timezone"));
        assert_eq!(m.scope_of("user.timezone"), Some(Scope::LongTerm));
        assert!(!m.promote("nope"));
    }

    #[test]
    fn memory_persists_across_simulated_sessions() {
        let dir = std::env::temp_dir().join(format!(
            "supercli-memory-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        // Session 1: set a session fact and a long-term fact, promote one.
        let mut s1 = load_memory(&dir);
        s1.set("turn.scratch", "tmp", Scope::Session, Some("s1"));
        s1.set("user.name", "amein", Scope::Session, Some("s1"));
        s1.promote("user.name");
        save_memory(&dir, &s1);

        // Session 2 (simulated restart): session facts are gone, long-term stays.
        let s2 = load_memory(&dir);
        assert_eq!(s2.get("turn.scratch"), None);
        assert_eq!(s2.get("user.name"), Some("amein"));
        assert_eq!(s2.scope_of("user.name"), Some(Scope::LongTerm));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn memory_keys_for_session_are_scoped() {
        let mut m = MemoryStore::new();
        m.set("user.name", "amein", Scope::LongTerm, None);
        m.set("turn.a", "1", Scope::Session, Some("s1"));
        m.set("turn.b", "2", Scope::Session, Some("s2"));
        assert_eq!(
            m.keys_for_session("s1"),
            vec!["turn.a".to_string(), "user.name".to_string()]
        );
        assert_eq!(
            m.keys_for_session("s2"),
            vec!["turn.b".to_string(), "user.name".to_string()]
        );
    }

    #[test]
    fn memory_load_missing_or_corrupt_returns_empty() {
        let dir = std::env::temp_dir().join("supercli-memory-test-nope-dir-xyz");
        let _ = fs::remove_dir_all(&dir);
        assert!(load_memory(&dir).is_empty());
        fs::create_dir_all(&dir).unwrap();
        fs::write(memory_path(&dir), b"not json{{{").unwrap();
        assert!(load_memory(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
