//! Local Host client launch policy.
//!
//! Rust port of the portable core of Swift `LocalHostClientFeature.swift`.
//! The app's own Local scope is always a Controller of the canonical Rust
//! workspace worker (`supercli-host __serve__`); there is no in-app Host.
//! The launch policy starts (or restarts a stale) bundled service and then
//! keeps the already-scanned disk view visible while the worker comes up.
//!
//! A service that never answers `host.sock` is reported as unavailable
//! (fail-closed: the app stays a client, it never hosts) and retried with a
//! bounded relaunch. The AppKit `HostServiceManager` orchestration and the
//! `@MainActor` `resolveForLaunch` stay Swift-side; what moves here is the
//! pure launch policy (`resolve`), the controller-owner header value, and
//! the launch-trace writer.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Bounded wait for the bundled service to answer host.sock at launch
/// before the UI reports it as unavailable. Everything keeps working as a
/// client during that window; only the status changes afterwards.
/// Swift: `LocalHostClientFeature.launchDeadline`.
pub const LAUNCH_DEADLINE: Duration = Duration::from_secs(5);

/// Interval between host.sock probes during the launch window.
/// Swift: the `sleep(0.1)` in `resolve`.
pub const LAUNCH_PROBE_INTERVAL: Duration = Duration::from_millis(100);

/// Advertised on every native loopback response. The worker probes this
/// independently of the platform-adapter socket so a transient adapter
/// reconnect can never make Direct/Link ownership bounce back to Swift.
/// Swift: `LocalHostClientFeature.controllerOwnerHeaderValue`.
pub const CONTROLLER_OWNER_HEADER_VALUE: &str = "serve";

/// Outcome of the launch probe. Both outcomes leave the app a client.
/// Swift: `LocalHostClientFeature.LaunchResolution`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchResolution {
    /// The service answered host.sock within the deadline.
    Client,
    /// The service did not answer within the deadline. The app stays a
    /// client (fail closed) and surfaces the reason; it never hosts.
    Unavailable { reason: String },
}

/// Pure launch policy: poll `probe` until it answers or `deadline` passes.
/// Both outcomes leave the app a client.
/// Swift: `LocalHostClientFeature.resolve`.
///
/// `probe` returns true when the service answers. `sleep` is injectable
/// for tests; production passes a 100ms thread sleep.
pub fn resolve_launch(
    mut probe: impl FnMut() -> bool,
    deadline: Duration,
    sleep: impl Fn(Duration),
) -> LaunchResolution {
    let start = Instant::now();
    let mut attempts: u64 = 0;
    loop {
        attempts += 1;
        if probe() {
            return LaunchResolution::Client;
        }
        if start.elapsed() >= deadline {
            return LaunchResolution::Unavailable {
                reason: format!(
                    "Host service did not answer host.sock within {}s ({attempts} probes)",
                    deadline.as_secs()
                ),
            };
        }
        sleep(LAUNCH_PROBE_INTERVAL);
    }
}

/// Append a timestamped line to `~/.supercli/hooks/trace.log` (under `home`),
/// next to the hook and worker lines so one file tells the story of a launch.
/// Swift: `LaunchTrace.append`.
pub fn append_launch_trace(home: &Path, line: &str) {
    let url = home.join("hooks").join("trace.log");
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let stamped = format!("{millis} {line}\n");
    if fs::create_dir_all(url.parent().unwrap()).is_err() {
        return;
    }
    // Open for append (creating if needed); fall back to a truncating write
    // only if the append open fails, mirroring Swift's FileHandle/write path.
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&url) {
        let _ = f.write_all(stamped.as_bytes());
    } else if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&url)
    {
        let _ = f.write_all(stamped.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_returns_client_when_probe_answers_immediately() {
        let r = resolve_launch(|| true, LAUNCH_DEADLINE, |_| {});
        assert_eq!(r, LaunchResolution::Client);
    }

    #[test]
    fn resolve_returns_client_when_probe_answers_before_deadline() {
        let mut calls = 0;
        let r = resolve_launch(
            || {
                calls += 1;
                calls >= 3
            },
            LAUNCH_DEADLINE,
            |_| {},
        );
        assert_eq!(r, LaunchResolution::Client);
        assert_eq!(calls, 3);
    }

    #[test]
    fn resolve_returns_unavailable_after_deadline_with_probe_count() {
        let mut probes = 0u64;
        let r = resolve_launch(
            || {
                probes += 1;
                false
            },
            Duration::from_millis(50),
            |_| {},
        );
        match r {
            LaunchResolution::Unavailable { reason } => {
                assert!(
                    reason.contains("Host service did not answer host.sock within 0s"),
                    "{reason}"
                );
                assert!(reason.contains(&format!("({probes} probes)")), "{reason}");
                assert!(probes >= 1);
            }
            LaunchResolution::Client => panic!("expected unavailable"),
        }
    }

    #[test]
    fn resolve_never_hosts_fail_closed() {
        // Both outcomes keep the app a client: there is no Host variant.
        let ok = resolve_launch(|| true, LAUNCH_DEADLINE, |_| {});
        let bad = resolve_launch(|| false, Duration::from_millis(1), |_| {});
        assert!(matches!(ok, LaunchResolution::Client));
        assert!(matches!(bad, LaunchResolution::Unavailable { .. }));
    }

    #[test]
    fn launch_deadline_is_five_seconds() {
        assert_eq!(LAUNCH_DEADLINE, Duration::from_secs(5));
    }

    #[test]
    fn probe_interval_is_100ms() {
        assert_eq!(LAUNCH_PROBE_INTERVAL, Duration::from_millis(100));
    }

    #[test]
    fn controller_owner_header_value_is_serve() {
        assert_eq!(CONTROLLER_OWNER_HEADER_VALUE, "serve");
    }

    #[test]
    fn launch_trace_appends_timestamped_lines() {
        let dir = std::env::temp_dir().join(format!(
            "supercli-launch-trace-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        append_launch_trace(&dir, "native-app test line one");
        append_launch_trace(&dir, "native-app test line two");
        let content =
            fs::read_to_string(dir.join("hooks").join("trace.log")).expect("trace.log written");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        for (line, expected) in lines.iter().zip(["test line one", "test line two"]) {
            let mut parts = line.splitn(2, ' ');
            let millis: u128 = parts.next().unwrap().parse().expect("millis timestamp");
            assert!(millis > 0);
            assert!(parts.next().unwrap().contains(expected));
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_trace_creates_hooks_dir() {
        let dir = std::env::temp_dir().join(format!(
            "supercli-launch-trace-mkdir-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        append_launch_trace(&dir, "hello");
        assert!(dir.join("hooks").join("trace.log").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
