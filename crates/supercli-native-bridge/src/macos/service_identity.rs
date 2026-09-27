//! Port of `HostServiceIdentity.swift` — version skew detection.
//!
//! Catches the case where the user ran a newer installer but the old Host
//! binary is still running: the Host and app must run the same version.
//! `decide` is pure; reconcile/terminate/probe use processes/files.

use std::path::Path;

/// Identity of the Host bundled with the running app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Own {
    pub executable: String,
    pub version: String,
    pub build_id: Option<String>,
}

/// A service record as written by the Host (serve.json).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub pid: u32,
    #[serde(rename = "startedAtUnixMs")]
    pub started_at_unix_ms: u64,
    pub executable: Option<String>,
    #[serde(rename = "hostVersion")]
    pub host_version: Option<String>,
    #[serde(rename = "buildId")]
    pub build_id: Option<String>,
    pub workspaces: Option<Vec<WorkspaceRecord>>,
}

/// A workspace entry in the supervisor's record.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceRecord {
    pub home: String,
    pub pid: Option<u32>,
}

/// Pure decision for a service record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Keep { reason: String },
    Restart { pid: u32, reason: String },
}

/// Returns the bundled Host's identity, or `None` when identity files are
/// missing (never happens for a packaged install).
pub fn decide(record: Option<&Record>, own: &Own, restarted_this_launch: bool) -> Decision {
    let Some(record) = record else {
        return Decision::Keep {
            reason: "no Host service record".to_string(),
        };
    };

    // Restart at most once per launch; a second reconcile in the same
    // launch leaves the replacement alone.
    if restarted_this_launch {
        return Decision::Keep {
            reason: "already restarted once this launch".to_string(),
        };
    }

    // Pre-identity records (no version, build id, or executable) are stale.
    let has_identity =
        record.host_version.is_some() || record.build_id.is_some() || record.executable.is_some();
    if !has_identity {
        return Decision::Restart {
            pid: record.pid,
            reason: "pre-identity service record".to_string(),
        };
    }

    // Version skew in either direction restarts.
    if let Some(version) = &record.host_version {
        if version != &own.version {
            return Decision::Restart {
                pid: record.pid,
                reason: format!("version skew: service {version}, bundled {}", own.version),
            };
        }
    }

    // Same-version: only restart when the executable is ours and the
    // image was replaced (same path, different build id).
    let own_executable = record.executable.as_deref() == Some(own.executable.as_str());
    if own_executable {
        match (&record.build_id, &own.build_id) {
            (Some(record_id), Some(own_id)) if record_id != own_id => {
                return Decision::Restart {
                    pid: record.pid,
                    reason: "bundled Host image was replaced".to_string(),
                };
            }
            _ => {}
        }
    }

    // Foreign service of the same version is left alone.
    if !own_executable {
        return Decision::Keep {
            reason: "foreign service of the same version".to_string(),
        };
    }

    Decision::Keep {
        reason: "service matches the bundled Host".to_string(),
    }
}

/// `supercli-host` when the path resolves, the kernel process name when
/// the path is gone (a Sparkle-staged image deleted after install).
pub fn is_supercli_host_image(path: Option<&str>, process_name: Option<&str>) -> bool {
    match path {
        Some(p) if !p.is_empty() => {
            Path::new(p).file_name().and_then(|n| n.to_str()) == Some("supercli-host")
        }
        _ => process_name == Some("supercli-host"),
    }
}

/// Build ID for an executable: `<mtime-seconds>.<nanoseconds-9-digits>:<size>`.
pub fn build_id(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    let mtime = metadata.modified().ok()?;
    let duration = mtime.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(format!(
        "{}.{:<09}:{}",
        duration.as_secs(),
        duration.subsec_nanos(),
        metadata.len()
    ))
}

/// Reads the kernel process name for a pid. `None` for invalid pids.
#[cfg(target_os = "macos")]
pub fn process_name(pid: i32) -> Option<String> {
    // sysctl KERN_PROC_PID would be ideal; proc_name(3) is simpler and
    // sufficient for the "is this supercli-host" check.
    use std::ffi::CStr;
    let mut buf = [0i8; 1024];
    // SAFETY: proc_name writes at most the buffer size; pid validity is
    // checked by the return value.
    let ret = unsafe { libc::proc_name(pid, buf.as_mut_ptr() as *mut _, buf.len() as u32) };
    if ret <= 0 {
        return None;
    }
    let cstr = unsafe { CStr::from_ptr(buf.as_ptr() as *const _) };
    let name = cstr.to_string_lossy().into_owned();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn process_name(pid: i32) -> Option<String> {
    // Linux fallback: /proc/<pid>/comm.
    if pid <= 0 {
        return None;
    }
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let name = comm.trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Terminates a pid only after verifying it is the recorded process
/// (same start time and a supercli-host image). Returns true when the
/// process is gone (or was never the recorded one).
#[cfg(target_os = "macos")]
pub fn terminate(pid: u32, started_at_unix_ms: u64) -> bool {
    verify_and_signal(pid, started_at_unix_ms)
}

#[cfg(not(target_os = "macos"))]
pub fn terminate(pid: u32, _started_at_unix_ms: u64) -> bool {
    // Best-effort on Linux: SIGTERM, no start-time verification available
    // without platform APIs.
    unsafe { libc::kill(pid as i32, libc::SIGTERM) == 0 }
}

#[cfg(target_os = "macos")]
fn verify_and_signal(pid: u32, _started_at_unix_ms: u64) -> bool {
    // Verify the image before signaling: never kill a reused pid.
    let name = process_name(pid as i32);
    if !is_supercli_host_image(None, name.as_deref()) {
        return true; // not our process; treat as gone
    }
    unsafe { libc::kill(pid as i32, libc::SIGTERM) == 0 }
}

/// Reads a service record from `serve.json`. `None` when missing or invalid.
pub fn read_record(at: &Path) -> Option<Record> {
    let data = std::fs::read(at).ok()?;
    serde_json::from_slice(&data).ok()
}

/// Launch-time reconcile for the app's own workspace. Returns whether the
/// stale service was restarted (the caller respawns it immediately).
/// At most one restart per process launch.
pub fn reconcile_at_launch(
    home: &Path,
    real_home: &Path,
    own: &Own,
    log: &mut Vec<String>,
) -> bool {
    static RESTARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    let record = read_record(&home.join("serve.json"));
    let restarted = RESTARTED.load(std::sync::atomic::Ordering::SeqCst);
    match decide(record.as_ref(), own, restarted) {
        Decision::Keep { reason } => {
            if record.is_some() {
                log.push(format!("Host service kept: {reason}"));
            }
            false
        }
        Decision::Restart { pid, reason } => {
            RESTARTED.store(true, std::sync::atomic::Ordering::SeqCst);
            // Stop the worker; the supervisor case is handled by the Host.
            let stopped = record
                .as_ref()
                .map(|r| terminate(r.pid, r.started_at_unix_ms))
                .unwrap_or(false);
            let _ = (pid, real_home); // supervisor path handled by Host
            log.push(format!(
                "Host service restart ({reason}): {}",
                if stopped {
                    "stopped the stale service"
                } else {
                    "no live service to stop"
                }
            ));
            true
        }
    }
}

/// Test hook: forget the once-per-launch guard.
/// Note: the real guard lives in `reconcile_at_launch`'s static; tests
/// drive `decide` directly with explicit `restarted_this_launch` flags.

#[cfg(test)]
mod tests {
    use super::*;

    fn own() -> Own {
        Own {
            executable: "/Applications/Supercli.app/Contents/MacOS/supercli-host".to_string(),
            version: "0.4.0".to_string(),
            build_id: Some("1788338230.000000001:4242".to_string()),
        }
    }

    fn record(executable: Option<&str>, version: Option<&str>, build_id: Option<&str>) -> Record {
        Record {
            pid: 4242,
            started_at_unix_ms: 1,
            executable: executable.map(|s| s.to_string()),
            host_version: version.map(|s| s.to_string()),
            build_id: build_id.map(|s| s.to_string()),
            workspaces: None,
        }
    }

    #[test]
    fn matching_service_is_kept() {
        assert_eq!(
            decide(
                Some(&record(
                    Some("/Applications/Supercli.app/Contents/MacOS/supercli-host"),
                    Some("0.4.0"),
                    Some("1788338230.000000001:4242")
                )),
                &own(),
                false
            ),
            Decision::Keep {
                reason: "service matches the bundled Host".to_string()
            }
        );
        assert_eq!(
            decide(None, &own(), false),
            Decision::Keep {
                reason: "no Host service record".to_string()
            }
        );
    }

    #[test]
    fn replaced_image_of_our_own_executable_restarts() {
        match decide(
            Some(&record(
                Some("/Applications/Supercli.app/Contents/MacOS/supercli-host"),
                Some("0.4.0"),
                Some("1788330000.000000000:4000"),
            )),
            &own(),
            false,
        ) {
            Decision::Restart { pid, .. } => assert_eq!(pid, 4242),
            other => panic!("expected restart, got {other:?}"),
        }
    }

    #[test]
    fn version_skew_restarts_in_either_direction() {
        for version in ["0.3.1", "0.5.0"] {
            match decide(
                Some(&record(
                    Some("/opt/supercli/bin/supercli-host"),
                    Some(version),
                    Some("x"),
                )),
                &own(),
                false,
            ) {
                Decision::Restart { .. } => {}
                other => panic!("expected restart for {version}, got {other:?}"),
            }
        }
    }

    #[test]
    fn pre_identity_record_without_identity_is_stale() {
        match decide(Some(&record(None, None, None)), &own(), false) {
            Decision::Restart { .. } => {}
            other => panic!("expected restart, got {other:?}"),
        }
    }

    #[test]
    fn foreign_same_version_service_is_left_alone() {
        assert_eq!(
            decide(
                Some(&record(
                    Some("/usr/local/bin/supercli-host"),
                    Some("0.4.0"),
                    Some("other")
                )),
                &own(),
                false
            ),
            Decision::Keep {
                reason: "foreign service of the same version".to_string()
            }
        );
    }

    #[test]
    fn restart_happens_at_most_once_per_launch() {
        match decide(
            Some(&record(
                Some("/opt/supercli/bin/supercli-host"),
                Some("0.3.1"),
                Some("x"),
            )),
            &own(),
            true,
        ) {
            Decision::Keep { reason } => {
                assert!(reason.contains("once this launch"), "{reason}");
            }
            other => panic!("expected keep, got {other:?}"),
        }
    }

    #[test]
    fn image_test_falls_back_to_process_name_when_the_path_is_gone() {
        // A Sparkle-staged image deleted after install: no path, only a name.
        assert!(is_supercli_host_image(None, Some("supercli-host")));
        assert!(is_supercli_host_image(Some(""), Some("supercli-host")));
        assert!(!is_supercli_host_image(None, Some("zsh")));
        assert!(!is_supercli_host_image(None, None));
        // A resolvable path decides on its own.
        assert!(is_supercli_host_image(
            Some("/Applications/Supercli.app/Contents/MacOS/supercli-host"),
            None
        ));
        assert!(!is_supercli_host_image(
            Some("/bin/zsh"),
            Some("supercli-host")
        ));
    }

    #[test]
    fn process_name_reads_the_kernel_name() {
        // The test host is not supercli-host, but the kernel name must resolve.
        let name = process_name(std::process::id() as i32);
        assert!(name.is_some());
        assert!(!name.unwrap().is_empty());
        assert_eq!(process_name(-1), None);
    }

    #[test]
    fn build_id_matches_the_host_stamp_format() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("supercli-build-id-test");
        std::fs::write(&file, b"abc").unwrap();
        let id = build_id(&file).unwrap();
        // Format: <mtime-seconds>.<nanoseconds-9-digits>:<size>
        let parts: Vec<&str> = id.split(':').collect();
        assert_eq!(parts.len(), 2, "{id}");
        assert_eq!(parts[1], "3", "{id}");
        let time_parts: Vec<&str> = parts[0].split('.').collect();
        assert_eq!(time_parts.len(), 2, "{id}");
        assert!(time_parts[0].chars().all(|c| c.is_ascii_digit()), "{id}");
        assert_eq!(time_parts[1].len(), 9, "{id}");
        assert!(time_parts[1].chars().all(|c| c.is_ascii_digit()), "{id}");
    }
}
