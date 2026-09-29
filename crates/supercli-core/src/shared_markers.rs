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
//!
//! Recency ordering (`resolvedLastRealActivityAtMs`, `sessionLastRealActivityAtMs`,
//! `resolvedLifecycleAtMs`) already lives in Rust as
//! `session_ops::{last_activity_ms, latest_lifecycle_ms, recents_recency_ms}`
//! (which additionally cover the background-hooks generation dir); the
//! latest-alert combiner lives there too as `session_ops::session_recency_ms`.
//! This module does not duplicate them.
//!
//! The Swift-only machinery stays with Swift: the RAII
//! `NativeSessionFileLockLease` maps to `app_state::FileLock` plus
//! `session_ops::lock_session_lifecycle`, the `kill(pid, 0)` probe lives in
//! `workspace_move::hosted_child_process_exists`, and the macOS pid probes
//! (`processStartTimeMs`, `processCommandLine`, `manifestPidIdentity`) are
//! macOS-only syscalls in `session_host`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

use crate::app_paths;
use crate::session_host::PidIdentity;

/// Process-wide sequence for temp-file names: pid separates processes,
/// the sequence separates concurrent writers inside one process, and the
/// wall-clock nanos separate a restarted process that reuses a pid.
static MARKER_TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Unique temp-file name in `session_dir` for an atomic marker write.
/// The name must be unique per writer: concurrent writers sharing one
/// fixed temp name would clobber each other's in-flight bytes.
fn marker_tmp_path(session_dir: &Path, marker: SharedMarker) -> PathBuf {
    let seq = MARKER_TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    session_dir.join(format!(
        ".{}.{}.{}.{}.tmp",
        marker.file_name(),
        std::process::id(),
        nanos,
        seq
    ))
}

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
    // Atomic land: unique temp file in the same directory, then rename.
    // Swift uses NSData's `.atomic` write, which is the same durable pattern.
    let tmp = marker_tmp_path(session_dir, marker);
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
}
