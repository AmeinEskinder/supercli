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

    /// Human-readable hint for an approval prompt, based on the operator's
    /// history with this tool. Returns None when there's no strong signal.
    ///
    /// This is how profile preferences affect approval behavior: the Host
    /// surfaces the hint alongside the prompt (e.g. "you usually deny
    /// `rm -rf` — extra care"), but the profile never auto-allows or
    /// auto-denies.
    pub fn approval_hint(&self, tool: &str) -> Option<String> {
        let (approvals, denials) = self.tool_outcomes.get(tool)?;
        if *approvals >= 3 && *denials == 0 {
            Some(format!(
                "you have approved `{tool}` {approvals} times with no denials"
            ))
        } else if *denials >= 2 && *approvals == 0 {
            Some(format!(
                "you have denied `{tool}` {denials} times with no approvals — extra care"
            ))
        } else {
            None
        }
    }

    /// Advisory auto-allow suggestion for a consistently-approved tool.
    ///
    /// ADVISORY ONLY. The returned [`AutoAllowSuggestion`] is pure data: it
    /// has no authority to grant anything. Turning a suggestion into a grant
    /// requires an explicit user action that goes through the audited grant
    /// path (`grant_writer::persist_grant_grouped`), which records the
    /// actor, scope, and tool in the hash-chained grant audit log.
    ///
    /// The `&self` receiver is load-bearing: this function cannot mutate the
    /// profile, takes no home path, and performs no I/O, so it is
    /// structurally incapable of touching the grants store.
    pub fn suggest_auto_allow(&self, tool: &str) -> Option<AutoAllowSuggestion> {
        let (approvals, denials) = self.tool_outcomes.get(tool)?;
        if *approvals >= 3 && *denials == 0 {
            Some(AutoAllowSuggestion {
                tool: tool.to_string(),
                approvals: *approvals,
                hint: format!(
                    "you usually approve `{tool}` — suggest auto-allow? (requires your explicit confirmation; nothing is granted automatically)"
                ),
            })
        } else {
            None
        }
    }
}

/// An advisory auto-allow suggestion. Pure data — creating an actual grant
/// from one requires an explicit user action through the audited grant
/// path. See [`Profile::suggest_auto_allow`].
#[derive(Debug, Clone, PartialEq)]
pub struct AutoAllowSuggestion {
    pub tool: String,
    pub approvals: u64,
    pub hint: String,
}

/// `<home>/profile.json`.
pub fn profile_path(home: &Path) -> std::path::PathBuf {
    home.join("profile.json")
}

/// Lock file guarding concurrent profile writers. Uses the same
/// flock-on-a-lockfile pattern as the action-review log: the kernel
/// releases the lock on process exit, so a crashed writer can never
/// wedge the profile.
fn profile_lock_path(home: &Path) -> std::path::PathBuf {
    home.join("profile.json.lock")
}

#[cfg(unix)]
fn lock_profile(home: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::io::AsRawFd;
    let lock_path = profile_lock_path(home);
    if let Some(dir) = lock_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
    // Blocking flock: profile writes are rare and fast; a crashed holder
    // is released by the kernel, so this cannot wedge.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(file)
}

#[cfg(not(unix))]
fn lock_profile(_home: &Path) -> std::io::Result<std::fs::File> {
    // Non-Unix: no flock; the atomic rename below still guarantees a
    // non-torn profile, just without cross-process exclusion.
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "flock not available",
    ))
}

pub fn load_profile(home: &Path) -> Profile {
    let path = profile_path(home);
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Profile::default(),
    }
}

/// Atomic, lock-protected save: serialize, write to a temp file, fsync,
/// then rename over `profile.json`. A crash at any point leaves either
/// the old profile or the new one — never a torn file.
///
/// Best-effort: a profile write must never break the caller, so all
/// errors are swallowed. Returns true if the profile was durably written.
pub fn save_profile(home: &Path, profile: &Profile) -> bool {
    let path = profile_path(home);
    if let Some(dir) = path.parent() {
        if fs::create_dir_all(dir).is_err() {
            return false;
        }
    }
    let text = match serde_json::to_string_pretty(profile) {
        Ok(t) => t,
        Err(_) => return false,
    };
    // Hold the lock for the read-modify-write cycle; the caller is
    // expected to have loaded, mutated, and now saves under this lock.
    // If locking fails (non-Unix), fall through to the atomic rename
    // which still prevents torn reads.
    let _lock = lock_profile(home).ok();

    let tmp = path.with_extension("json.tmp");
    let write_ok = (|| -> std::io::Result<()> {
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.flush()?;
        f.sync_all()?;
        Ok(())
    })();
    if write_ok.is_err() {
        let _ = fs::remove_file(&tmp);
        return false;
    }
    if fs::rename(&tmp, &path).is_err() {
        let _ = fs::remove_file(&tmp);
        return false;
    }
    true
}

/// Load, mutate, and atomically save the profile under the lock.
/// The mutation runs while holding the exclusive lock, so concurrent
/// writers cannot lose updates.
pub fn update_profile(home: &Path, f: impl FnOnce(&mut Profile)) {
    let _lock = lock_profile(home).ok();
    let mut profile = load_profile(home);
    f(&mut profile);
    // Save without re-locking (we already hold it); the rename is atomic.
    let path = profile_path(home);
    if let Ok(text) = serde_json::to_string_pretty(&profile) {
        let tmp = path.with_extension("json.tmp");
        use std::io::Write;
        let ok = (|| -> std::io::Result<()> {
            let mut fh = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp)?;
            fh.write_all(text.as_bytes())?;
            fh.flush()?;
            fh.sync_all()?;
            Ok(())
        })()
        .is_ok()
            && fs::rename(&tmp, &path).is_ok();
        if !ok {
            let _ = fs::remove_file(&tmp);
        }
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
        assert!(save_profile(&dir, &p));
        let loaded = load_profile(&dir);
        assert_eq!(loaded, p);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn approval_hint_reflects_history() {
        let mut p = Profile::default();
        assert_eq!(p.approval_hint("cargo fmt"), None);

        p.record_approval("cargo fmt", true);
        p.record_approval("cargo fmt", true);
        assert_eq!(p.approval_hint("cargo fmt"), None); // only 2

        p.record_approval("cargo fmt", true);
        let hint = p.approval_hint("cargo fmt").unwrap();
        assert!(hint.contains("cargo fmt") && hint.contains("3 times"));

        p.record_approval("rm -rf", false);
        assert_eq!(p.approval_hint("rm -rf"), None); // only 1 denial
        p.record_approval("rm -rf", false);
        let hint = p.approval_hint("rm -rf").unwrap();
        assert!(hint.contains("extra care"));
    }

    #[test]
    fn load_missing_profile_returns_default() {
        let dir = std::env::temp_dir().join("supercli-profile-test-nonexistent-dir-xyz");
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(load_profile(&dir), Profile::default());
    }

    #[test]
    fn update_profile_is_atomic_under_concurrent_writers() {
        use std::sync::{Arc, Barrier};
        let dir = std::env::temp_dir().join(format!(
            "supercli-profile-test-atomic-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let dir = Arc::new(dir);
        let barrier = Arc::new(Barrier::new(8));
        let mut handles = Vec::new();
        for i in 0..8 {
            let dir = dir.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                for j in 0..25 {
                    let tool = format!("tool-{i}-{j}");
                    update_profile(&dir, |p| {
                        p.record_approval(&tool, true);
                    });
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // All 200 updates landed: no lost updates, no torn JSON.
        let loaded = load_profile(&dir);
        assert_eq!(loaded.tool_outcomes.len(), 200);
        // The file is valid JSON (never torn).
        let text = fs::read_to_string(profile_path(&dir)).unwrap();
        let _: serde_json::Value = serde_json::from_str(&text).unwrap();
        let _ = fs::remove_dir_all(&*dir);
    }

    #[test]
    fn suggest_auto_allow_never_grants_by_itself() {
        // Per Amein's review: a "suggest auto-allow" must NEVER grant
        // anything by itself. Only an explicit user action through the
        // audited grant path creates a grant.
        let dir = std::env::temp_dir().join(format!(
            "supercli-profile-suggest-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        // Operator approved `cargo fmt` 3 times: it becomes a suggestion
        // candidate.
        let mut p = Profile::default();
        p.record_approval("cargo fmt", true);
        p.record_approval("cargo fmt", true);
        p.record_approval("cargo fmt", true);
        assert!(save_profile(&dir, &p));

        // 1. The suggestion exists (advisory data).
        let suggestion = p.suggest_auto_allow("cargo fmt");
        assert!(suggestion.is_some());
        let suggestion = suggestion.unwrap();
        assert_eq!(suggestion.tool, "cargo fmt");
        assert_eq!(suggestion.approvals, 3);
        // A tool without the track record gets no suggestion.
        assert!(p.suggest_auto_allow("rm -rf").is_none());

        // 2. The suggestion created NOTHING: no grants file, no audit log.
        // (suggest_auto_allow takes &self and performs no I/O, so this is
        // structural, but assert it against the store anyway.)
        assert!(
            !dir.join("grants.json").exists(),
            "suggestion must not create grants.json"
        );
        assert!(
            !dir.join("grant-audit.jsonl").exists(),
            "suggestion must not write the grant audit log"
        );

        // 3. Explicit user action: the normal audited grant path (the same
        // two steps the production group-commit writer performs:
        // write-ahead audit entry, then the grants.json mutation).
        let grant_key = "write:cargo fmt:/tmp/x";
        let entries = crate::grant_audit::record_grants_created_batch_at(
            &dir,
            &[("human:test-device", "write", "write", grant_key)],
        )
        .expect("audit write must succeed");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].actor, "human:test-device");
        crate::grant_store::edit_grants_at(&dir, |root| {
            crate::grant_writer::apply_grant_mutation(root, "write", "cargo fmt", Some("/tmp/x"));
            Ok::<(), String>(())
        })
        .expect("grants.json write must succeed");

        // 4. Now the grant exists AND the audit log records who created it.
        let grants_text = fs::read_to_string(dir.join("grants.json")).unwrap();
        let grants: serde_json::Value = serde_json::from_str(&grants_text).unwrap();
        let targets = grants["mcp_write_approvals"]["cargo fmt"]
            .as_array()
            .expect("grant must be recorded");
        assert!(targets.iter().any(|v| v.as_str() == Some("/tmp/x")));

        let audit_text = fs::read_to_string(dir.join("grant-audit.jsonl")).unwrap();
        let lines: Vec<&str> = audit_text.lines().collect();
        assert_eq!(lines.len(), 1, "exactly one audited grant creation");
        let entry: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(entry["actor"], "human:test-device");
        assert_eq!(entry["grant_key"], grant_key);

        let _ = fs::remove_dir_all(&dir);
    }
}
