//! Shared session markers + restart state eligibility + recency rules.
//!
//! Ports the test-covered pure logic from the legacy Swift store's
//! cross-process coordination core:
//!
//! - `SharedMarker` file I/O (`sharedMarkerURL`, `writeSharedMarker`,
//!   `readSharedMarker`, `removeSharedMarker`, `sharedMarkerExists`) — the
//!   files the desktop, a headless host, and the phone agree on
//!   (`archived.json`, `title.json`, `read.json`, `provider-session.json`,
//!   `project-override.json`). Markers are authoritative once present;
//!   overlays exist only as migration/write-failure fallbacks.
//! - `replacementRestartAllowsState` — the fail-closed rule deciding whether
//!   a manifest that claims `running` may resume state, given whether the
//!   recorded child still exists and the pid identity verdict (the existing
//!   [`crate::session_host::PidIdentity`] verdict).
//! - `resolvedLastRealActivityAtMs` / `sessionLastRealActivityAtMs` /
//!   `resolvedLifecycleAtMs` / `sessionRecencyMs` — the provider-aware
//!   recency ordering behind Recent and the sidebar date mode.
//!
//! The Swift-only machinery stays with Swift: the RAII
//! `NativeSessionFileLockLease` maps to `app_state::FileLock` plus
//! `session_ops::lock_session_lifecycle`, the `kill(pid, 0)` probe lives in
//! `workspace_move::hosted_child_process_exists`, and the macOS pid probes
//! (`processStartTimeMs`, `processCommandLine`, `manifestPidIdentity`) are
//! macOS-only syscalls in `session_host`.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::{Map, Value};

use crate::app_paths;
use crate::session_host::PidIdentity;

/// A cross-process shared marker file inside one session directory.
///
/// The raw file name doubles as the wire-stable identity: readers in other
/// processes only ever look for these exact names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedMarker {
    Archived,
    Title,
    Read,
    ProviderSession,
    ProjectOverride,
}

impl SharedMarker {
    /// The exact file name Swift's `SharedMarker.rawValue` uses.
    pub fn file_name(self) -> &'static str {
        match self {
            SharedMarker::Archived => "archived.json",
            SharedMarker::Title => "title.json",
            SharedMarker::Read => "read.json",
            SharedMarker::ProviderSession => "provider-session.json",
            SharedMarker::ProjectOverride => "project-override.json",
        }
    }
}

/// Path of a marker under the default supercli home:
/// `<home>/app-sessions/<session_id>/<marker>`.
pub fn shared_marker_path(session_id: &str, marker: SharedMarker) -> PathBuf {
    app_paths::app_sessions_root()
        .join(session_id)
        .join(marker.file_name())
}

/// Path of a marker under an explicit home: `<home>/app-sessions/<session_id>/<marker>`.
///
/// This is the against-the-home twin Swift uses for scoped local-workspace
/// verbs (same write class as Add Project).
pub fn shared_marker_path_at(home: &Path, session_id: &str, marker: SharedMarker) -> PathBuf {
    home.join("app-sessions")
        .join(session_id)
        .join(marker.file_name())
}

fn marker_session_dir(path: &Path) -> Option<&Path> {
    path.parent()
}

/// Write a marker body as JSON with the Swift semantics: the write only
/// happens when the session directory already exists (never create it —
/// that would resurrect deleted sessions), serialization failure returns
/// false, and the file lands atomically via temp-file + rename.
pub fn write_shared_marker(
    home: Option<&Path>,
    session_id: &str,
    marker: SharedMarker,
    body: &Map<String, Value>,
) -> bool {
    let path = match home {
        Some(home) => shared_marker_path_at(home, session_id, marker),
        None => shared_marker_path(session_id, marker),
    };
    let Some(session_dir) = marker_session_dir(&path) else {
        return false;
    };
    if !session_dir.is_dir() {
        return false;
    }
    let Ok(serialized) = serde_json::to_vec(body) else {
        return false;
    };
    // Atomic land: temp file in the same directory, then rename. Swift uses
    // NSData's `.atomic` write, which is the same durable pattern.
    let tmp = session_dir.join(format!(".{}.tmp", marker.file_name()));
    if std::fs::write(&tmp, &serialized).is_err() {
        return false;
    }
    if std::fs::rename(&tmp, &path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    true
}

/// Read a marker body. Returns `None` unless the marker exists AND parses
/// as a JSON object — a conservative guard against half-written or
/// foreign files.
pub fn read_shared_marker(
    home: Option<&Path>,
    session_id: &str,
    marker: SharedMarker,
) -> Option<Map<String, Value>> {
    let path = match home {
        Some(home) => shared_marker_path_at(home, session_id, marker),
        None => shared_marker_path(session_id, marker),
    };
    if !shared_marker_exists(home, session_id, marker) {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    serde_json::from_slice::<Value>(&data)
        .ok()
        .and_then(|value| value.as_object().cloned())
}

/// Best-effort marker removal. Any frontend may delete a marker; callers
/// treat errors as "not there".
pub fn remove_shared_marker(home: Option<&Path>, session_id: &str, marker: SharedMarker) {
    let path = match home {
        Some(home) => shared_marker_path_at(home, session_id, marker),
        None => shared_marker_path(session_id, marker),
    };
    let _ = std::fs::remove_file(path);
}

/// Existence check only — a metadata stat instead of an open+parse. Rescan
/// asks this for every live session, so the common "no marker" case must
/// not cost a file read.
pub fn shared_marker_exists(home: Option<&Path>, session_id: &str, marker: SharedMarker) -> bool {
    let path = match home {
        Some(home) => shared_marker_path_at(home, session_id, marker),
        None => shared_marker_path(session_id, marker),
    };
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Restart state eligibility
// ---------------------------------------------------------------------------

/// Fail-closed rule for whether a replacement restart may adopt a
/// manifest's state. Mirrors Swift's `replacementRestartAllowsState`:
///
/// - not in stopped-only mode: state adoption is always allowed;
/// - manifest says `exited`: safe;
/// - manifest says `running`: safe only when the recorded child is
///   definitely absent, or its pid has definitely been recycled onto an
///   unrelated process. Unknown identity plus a live/unknown pid fails
///   closed — a crashed host can leave its final manifest at `running`.
/// - any other manifest state: not allowed.
///
/// The pid verdict is the existing [`PidIdentity`] produced by
/// `session_host::manifest_pid_identity` — no second enum for it.
pub fn replacement_restart_allows_state(
    manifest_state: Option<&str>,
    stopped_only: bool,
    child_process_exists: Option<bool>,
    pid_identity: PidIdentity,
) -> bool {
    if !stopped_only {
        return true;
    }
    if manifest_state == Some("exited") {
        return true;
    }
    if manifest_state != Some("running") {
        return false;
    }
    child_process_exists == Some(false) || pid_identity == PidIdentity::NotOurs
}

// ---------------------------------------------------------------------------
// Recency / activity ordering
// ---------------------------------------------------------------------------

/// Latest real activity signal with the provider-aware rule from the
/// legacy Swift store: hook-capable agents have a truthful durable hook
/// seed; when that seed is absent they have not produced a lifecycle event
/// yet, so a TUI repaint in `output.bin` must NOT make them recent — `None`
/// is returned rather than consulting screen/output repaint signals.
/// Hookless tools use the host's parsed-screen change stamp, falling back
/// to `output.bin` only for manifests that predate that field.
pub fn resolved_last_real_activity_at_ms(
    uses_lifecycle_hooks: bool,
    hook_event_at_ms: Option<i64>,
    screen_changed_at_ms: Option<i64>,
    output_at_ms: Option<i64>,
) -> Option<i64> {
    if uses_lifecycle_hooks {
        return hook_event_at_ms;
    }
    screen_changed_at_ms.or(output_at_ms)
}

/// Canonical timestamp used by Recent/date ordering. Creation is the start
/// event and therefore the floor. The host's `updated_at` joins the rank
/// only after it writes an exited manifest; while running that field is a
/// heartbeat and would otherwise float every live session to now.
pub fn resolved_lifecycle_at_ms(
    created_at_ms: i64,
    uses_lifecycle_hooks: bool,
    hook_event_at_ms: Option<i64>,
    screen_changed_at_ms: Option<i64>,
    output_at_ms: Option<i64>,
    final_exited_at_ms: Option<i64>,
) -> i64 {
    created_at_ms
        .max(
            resolved_last_real_activity_at_ms(
                uses_lifecycle_hooks,
                hook_event_at_ms,
                screen_changed_at_ms,
                output_at_ms,
            )
            .unwrap_or(0),
        )
        .max(final_exited_at_ms.unwrap_or(0))
}

/// Unified Recent recency: the latest lifecycle event or app alert, with
/// creation as its floor. Read receipts are not activity — callers pass the
/// latest *alert* stamp, never a read stamp, so selecting/reading a row
/// never reshuffles a Recent surface.
///
/// Swift's `sessionRecencyMs(_:)` returns 0 for an unknown session id; here
/// the caller supplies the session's `created_at_ms` and owns the lookup,
/// so an unknown session is simply never passed in.
pub fn session_recency_ms(
    created_at_ms: i64,
    lifecycle_at_ms: Option<i64>,
    latest_alert_at_ms: Option<i64>,
) -> i64 {
    created_at_ms
        .max(lifecycle_at_ms.unwrap_or(0))
        .max(latest_alert_at_ms.unwrap_or(0))
}

// ---------------------------------------------------------------------------
// Filesystem-backed activity read
// ---------------------------------------------------------------------------

/// Modification time of `path` in ms since the epoch — the Rust half of
/// Swift's `fileModificationAtMs`. `None` when the file is absent or its
/// timestamp is unreadable.
pub fn file_modification_at_ms(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let millis = modified.duration_since(UNIX_EPOCH).ok()?.as_millis();
    i64::try_from(millis).ok()
}

/// Filesystem-backed last-real-activity read for unread-marker
/// reconciliation. Mirrors Swift's `sessionLastRealActivityAtMs`, keep it
/// command-aware:
///
/// - hook-capable agents: the durable hook seed's mtime. A missing seed
///   means the agent has not produced a lifecycle event yet, so `None` is
///   returned rather than consulting screen/output repaint signals;
/// - hookless tools: the host's parsed-screen change stamp from
///   `manifest.json` (only when positive), falling back to `output.bin`'s
///   mtime for manifests that predate that field.
///
/// Swift detects the tool from `command` via `SetupTool.detect`; here the
/// caller passes the already-resolved hook predicate (the
/// `uses_lifecycle_hooks` the rest of Rust derives from its tool catalog).
pub fn session_last_real_activity_at_ms(
    session_dir: &Path,
    uses_lifecycle_hooks: bool,
) -> Option<i64> {
    if uses_lifecycle_hooks {
        return file_modification_at_ms(&session_dir.join("last-hook-event.json"));
    }
    let manifest_screen_changed_at = std::fs::read(session_dir.join("manifest.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .and_then(|json| json.get("screen_changed_at").and_then(positive_i64));
    manifest_screen_changed_at.or_else(|| file_modification_at_ms(&session_dir.join("output.bin")))
}

/// A JSON number that is positive — Swift's `(as? NSNumber)?.int64Value`
/// with the `stamp > 0` guard.
fn positive_i64(value: &Value) -> Option<i64> {
    let stamp = value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|n| i64::try_from(n).ok()))?;
    (stamp > 0).then_some(stamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_home() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "shared-markers-test-{}-{}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(dir.join("app-sessions").join("s1")).expect("mkdir");
        dir
    }

    fn json_map(value: Value) -> Map<String, Value> {
        value.as_object().cloned().expect("object")
    }

    #[test]
    fn write_read_remove_round_trip() {
        let home = temp_home();
        let body = json_map(json!({"title": "hello", "updated_at": 42}));
        assert!(write_shared_marker(
            Some(&home),
            "s1",
            SharedMarker::Title,
            &body
        ));
        assert!(shared_marker_exists(Some(&home), "s1", SharedMarker::Title));
        assert_eq!(
            read_shared_marker(Some(&home), "s1", SharedMarker::Title),
            Some(body)
        );
        remove_shared_marker(Some(&home), "s1", SharedMarker::Title);
        assert!(!shared_marker_exists(
            Some(&home),
            "s1",
            SharedMarker::Title
        ));
        assert_eq!(
            read_shared_marker(Some(&home), "s1", SharedMarker::Title),
            None
        );
    }

    #[test]
    fn write_refuses_missing_session_dir() {
        let home = temp_home();
        let body = json_map(json!({"title": "ghost"}));
        // The session dir must already exist; writes never resurrect deleted
        // sessions.
        assert!(!write_shared_marker(
            Some(&home),
            "no-such-session",
            SharedMarker::Title,
            &body
        ));
        assert!(!shared_marker_exists(
            Some(&home),
            "no-such-session",
            SharedMarker::Title
        ));
    }

    #[test]
    fn read_returns_none_for_foreign_body() {
        let home = temp_home();
        let session_dir = home.join("app-sessions").join("s1");
        // A non-object JSON body is refused conservatively, like Swift's
        // `as? [String: Any]` cast.
        std::fs::write(session_dir.join("archived.json"), b"[1, 2]").expect("seed");
        assert_eq!(
            read_shared_marker(Some(&home), "s1", SharedMarker::Archived),
            None
        );
        // And truncated garbage is refused too.
        std::fs::write(session_dir.join("read.json"), b"{not json").expect("seed");
        assert_eq!(
            read_shared_marker(Some(&home), "s1", SharedMarker::Read),
            None
        );
    }

    #[test]
    fn marker_paths_match_swift_layout() {
        let home = Path::new("/home/x");
        assert_eq!(
            shared_marker_path_at(home, "s1", SharedMarker::Title),
            Path::new("/home/x/app-sessions/s1/title.json")
        );
        assert_eq!(
            shared_marker_path_at(home, "s1", SharedMarker::ProjectOverride),
            Path::new("/home/x/app-sessions/s1/project-override.json")
        );
        assert_eq!(
            shared_marker_path_at(home, "s1", SharedMarker::ProviderSession),
            Path::new("/home/x/app-sessions/s1/provider-session.json")
        );
    }

    #[test]
    fn restart_allows_state_fails_closed() {
        use PidIdentity as Identity;
        // Not in stopped-only mode: anything goes.
        assert!(replacement_restart_allows_state(
            None,
            false,
            None,
            Identity::Unknown
        ));
        assert!(replacement_restart_allows_state(
            Some("running"),
            false,
            Some(true),
            Identity::Matches
        ));
        // Exited is safe to adopt.
        assert!(replacement_restart_allows_state(
            Some("exited"),
            true,
            None,
            Identity::Unknown
        ));
        // Running with a definitely-absent child is safe.
        assert!(replacement_restart_allows_state(
            Some("running"),
            true,
            Some(false),
            Identity::Unknown
        ));
        // Running with a recycled pid is safe.
        assert!(replacement_restart_allows_state(
            Some("running"),
            true,
            Some(true),
            Identity::NotOurs
        ));
        // Running, child live, pid identity unknown: fail closed.
        assert!(!replacement_restart_allows_state(
            Some("running"),
            true,
            Some(true),
            Identity::Unknown
        ));
        // Running, child live, positively ours: the session is alive.
        assert!(!replacement_restart_allows_state(
            Some("running"),
            true,
            Some(true),
            Identity::Matches
        ));
        // A crashed host may leave `running` without a known pid: unknown
        // child + unknown identity fails closed.
        assert!(!replacement_restart_allows_state(
            Some("running"),
            true,
            None,
            Identity::Unknown
        ));
        // Any other manifest state never allows adoption.
        for state in [None, Some("starting"), Some("paused"), Some("")] {
            assert!(
                !replacement_restart_allows_state(state, true, Some(false), Identity::Unknown),
                "state {state:?} should not allow adoption"
            );
        }
    }

    #[test]
    fn hook_agent_activity_ignores_tui_repaint() {
        // Hook seed present: it wins even against a newer output stamp.
        assert_eq!(
            resolved_last_real_activity_at_ms(true, Some(1000), Some(5000), Some(9000)),
            Some(1000)
        );
        // Hook seed absent: TUI repaint must NOT make the session recent.
        assert_eq!(
            resolved_last_real_activity_at_ms(true, None, Some(5000), Some(9000)),
            None
        );
        // Hookless tools: screen stamp, then output.bin fallback.
        assert_eq!(
            resolved_last_real_activity_at_ms(false, Some(1000), Some(5000), Some(9000)),
            Some(5000)
        );
        assert_eq!(
            resolved_last_real_activity_at_ms(false, Some(1000), None, Some(9000)),
            Some(9000)
        );
        assert_eq!(
            resolved_last_real_activity_at_ms(false, None, None, None),
            None
        );
    }

    #[test]
    fn lifecycle_at_uses_creation_floor_and_exited_heartbeat() {
        // Creation is the floor even with no activity.
        assert_eq!(
            resolved_lifecycle_at_ms(1000, false, None, None, None, None),
            1000
        );
        // Real activity raises it.
        assert_eq!(
            resolved_lifecycle_at_ms(1000, false, None, None, Some(4000), None),
            4000
        );
        // The final exited stamp raises it (while running, updated_at is a
        // heartbeat and must not float the row — callers withhold it).
        assert_eq!(
            resolved_lifecycle_at_ms(1000, false, None, None, Some(4000), Some(6000)),
            6000
        );
        // Stale activity below creation never drags the rank down.
        assert_eq!(
            resolved_lifecycle_at_ms(8000, false, None, Some(1000), None, None),
            8000
        );
    }

    #[test]
    fn recency_uses_latest_of_created_lifecycle_and_alert() {
        assert_eq!(session_recency_ms(1000, Some(2000), Some(3000)), 3000);
        assert_eq!(session_recency_ms(1000, Some(5000), Some(3000)), 5000);
        assert_eq!(session_recency_ms(9000, Some(5000), None), 9000);
        assert_eq!(session_recency_ms(1000, None, None), 1000);
    }

    #[test]
    fn session_last_activity_hook_agent_uses_seed_mtime() {
        let home = temp_home();
        let dir = home.join("app-sessions").join("s1");
        let seed = dir.join("last-hook-event.json");
        std::fs::write(&seed, b"{}").expect("seed");
        assert_eq!(
            session_last_real_activity_at_ms(&dir, true),
            file_modification_at_ms(&seed)
        );
    }

    #[test]
    fn session_last_activity_hook_agent_missing_seed_is_none() {
        let home = temp_home();
        let dir = home.join("app-sessions").join("s1");
        // No seed: a fresh output.bin must NOT make the session recent.
        std::fs::write(dir.join("output.bin"), b"x").expect("seed");
        assert_eq!(session_last_real_activity_at_ms(&dir, true), None);
    }

    #[test]
    fn session_last_activity_hookless_prefers_manifest_stamp() {
        let home = temp_home();
        let dir = home.join("app-sessions").join("s1");
        std::fs::write(
            dir.join("manifest.json"),
            br#"{"screen_changed_at": 1234567890}"#,
        )
        .expect("seed");
        std::fs::write(dir.join("output.bin"), b"x").expect("seed");
        assert_eq!(
            session_last_real_activity_at_ms(&dir, false),
            Some(1_234_567_890)
        );
    }

    #[test]
    fn session_last_activity_hookless_falls_back_to_output_bin() {
        let home = temp_home();
        let dir = home.join("app-sessions").join("s1");
        // A zero stamp defers to output.bin, like Swift's `stamp > 0` guard.
        std::fs::write(dir.join("manifest.json"), br#"{"screen_changed_at": 0}"#).expect("seed");
        let out = dir.join("output.bin");
        std::fs::write(&out, b"x").expect("seed");
        assert_eq!(
            session_last_real_activity_at_ms(&dir, false),
            file_modification_at_ms(&out)
        );
    }

    #[test]
    fn session_last_activity_nothing_present_is_none() {
        let home = temp_home();
        let dir = home.join("app-sessions").join("s1");
        assert_eq!(session_last_real_activity_at_ms(&dir, false), None);
    }
}
