//! Heuristic user profile: proactive learning without ML.
//!
//! The profile records operator preferences inferred from approval decisions
//! and explicit settings: preferred editor, coding-style prefs (tabs vs
//! spaces, line length), and frequently approved tool patterns. It is
//! heuristics only — counters and last-seen timestamps, no models.
//!
//! Storage: `<home>/profile.json`. The profile never gates an action; it is
//! advisory context the Host may surface (e.g. "you usually approve `cargo
//! fmt` — suggest auto-allow?").

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// One inferred preference: what was observed, how strongly, when.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Preference {
    pub value: String,
    pub confidence: f64,
    pub observations: u64,
    pub last_seen: u64,
}

/// The persisted operator profile.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Profile {
    /// e.g. "editor" -> Preference{value: "hx", ...}
    #[serde(default)]
    pub prefs: HashMap<String, Preference>,
    /// tool name -> (approvals, denials)
    #[serde(default)]
    pub tool_outcomes: HashMap<String, (u64, u64)>,
    #[serde(default)]
    pub updated_at: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Profile {
    /// Record one approval decision for a tool call. `approved=true` means
    /// the operator allowed it; false means denied (or it timed out as deny).
    ///
    /// Heuristic: 3+ approvals with 0 denials and the tool becomes a
    /// candidate for "usually approved" surfacing; it never auto-allows.
    pub fn record_approval(&mut self, tool: &str, approved: bool) {
        let entry = self.tool_outcomes.entry(tool.to_string()).or_insert((0, 0));
        if approved {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
        self.updated_at = now_secs();
    }

    /// Tools the operator approves consistently (>=3 approvals, no denials).
    pub fn usually_approved(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .tool_outcomes
            .iter()
            .filter(|(_, (a, d))| *a >= 3 && *d == 0)
            .map(|(t, _)| t.clone())
            .collect();
        out.sort();
        out
    }

    /// Tools the operator consistently denies (>=2 denials, no approvals).
    pub fn usually_denied(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .tool_outcomes
            .iter()
            .filter(|(_, (a, d))| *d >= 2 && *a == 0)
            .map(|(t, _)| t.clone())
            .collect();
        out.sort();
        out
    }

    /// Set an explicit preference (e.g. from `supercli settings` or an
    /// observed editor launch). Explicit prefs get full confidence.
    pub fn set_pref(&mut self, key: &str, value: &str) {
        self.prefs.insert(
            key.to_string(),
            Preference {
                value: value.to_string(),
                confidence: 1.0,
                observations: 1,
                last_seen: now_secs(),
            },
        );
        self.updated_at = now_secs();
    }

    /// Infer a preference from repeated observations of the same value.
    /// Confidence grows with observations, capped at 0.9 (never 1.0:
    /// inferred is never as strong as explicit).
    pub fn observe_pref(&mut self, key: &str, value: &str) {
        let pref = self.prefs.entry(key.to_string()).or_insert(Preference {
            value: value.to_string(),
            confidence: 0.0,
            observations: 0,
            last_seen: 0,
        });
        if pref.value != value {
            // Value changed: reset, the operator switched.
            pref.value = value.to_string();
            pref.observations = 1;
            pref.confidence = 0.1;
        } else {
            pref.observations += 1;
            pref.confidence = (0.1 * pref.observations as f64).min(0.9);
        }
        pref.last_seen = now_secs();
        self.updated_at = now_secs();
    }

    pub fn pref(&self, key: &str) -> Option<&Preference> {
        self.prefs.get(key)
    }
}

/// `<home>/profile.json`.
pub fn profile_path(home: &Path) -> std::path::PathBuf {
    home.join("profile.json")
}

pub fn load_profile(home: &Path) -> Profile {
    let path = profile_path(home);
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Profile::default(),
    }
}

/// Best-effort save; a profile write must never break the caller.
pub fn save_profile(home: &Path, profile: &Profile) {
    let path = profile_path(home);
    if let Ok(text) = serde_json::to_string_pretty(profile) {
        let _ = fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_updates_from_approval_history() {
        let mut p = Profile::default();
        p.record_approval("cargo fmt", true);
        p.record_approval("cargo fmt", true);
        assert!(p.usually_approved().is_empty()); // only 2 so far
        p.record_approval("cargo fmt", true);
        assert_eq!(p.usually_approved(), vec!["cargo fmt".to_string()]);

        // One denial removes it from usually-approved.
        p.record_approval("cargo fmt", false);
        assert!(p.usually_approved().is_empty());

        p.record_approval("rm -rf", false);
        p.record_approval("rm -rf", false);
        assert_eq!(p.usually_denied(), vec!["rm -rf".to_string()]);
    }

    #[test]
    fn inferred_pref_confidence_grows_and_resets_on_switch() {
        let mut p = Profile::default();
        p.observe_pref("editor", "hx");
        p.observe_pref("editor", "hx");
        let pref = p.pref("editor").unwrap();
        assert_eq!(pref.value, "hx");
        assert!(pref.confidence > 0.0 && pref.confidence < 1.0);

        // Operator switches editor: confidence resets.
        p.observe_pref("editor", "zeditor");
        let pref = p.pref("editor").unwrap();
        assert_eq!(pref.value, "zeditor");
        assert_eq!(pref.observations, 1);

        // Explicit set wins with full confidence.
        p.set_pref("editor", "hx");
        assert_eq!(p.pref("editor").unwrap().confidence, 1.0);
    }

    #[test]
    fn profile_save_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "supercli-profile-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let mut p = Profile::default();
        p.record_approval("cargo test", true);
        p.set_pref("shell", "fish");
        save_profile(&dir, &p);
        let loaded = load_profile(&dir);
        assert_eq!(loaded, p);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_profile_returns_default() {
        let dir = std::env::temp_dir().join("supercli-profile-test-nonexistent-dir-xyz");
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(load_profile(&dir), Profile::default());
    }
}
