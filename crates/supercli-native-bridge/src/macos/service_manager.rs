//! Port of `HostServiceManager.swift` — service lifecycle management.
//!
//! Decides whether the app starts the Host via launchd (registered or
//! default workspaces) or forks it directly (private unregistered homes,
//! snapshot/test launches), tracks the service state machine, and
//! serializes launchd operations so overlapping launches don't race.
//!
//! Pure policy (`RetryPolicy`, `ServiceState`, `launch_mode_for`) is
//! cross-platform and tested. Process/launchd execution is injected.

use std::time::{Duration, Instant};

/// Minimum interval between service launches. Swift: 5 seconds.
pub const LAUNCH_RETRY_COOLDOWN: Duration = Duration::from_secs(5);

/// How the app should start the Host for a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    /// Registered or default workspaces: launchd.
    Launchd,
    /// Private unregistered homes, snapshot/test launches: fork directly.
    Direct,
}

/// Decides the launch mode. Test/snapshot launches never start a
/// background service; private unregistered homes fork directly.
pub fn launch_mode_for(
    is_registered: bool,
    is_default_workspace: bool,
    is_test_launch: bool,
    is_snapshot_launch: bool,
) -> LaunchMode {
    if is_test_launch || is_snapshot_launch {
        return LaunchMode::Direct;
    }
    if is_registered || is_default_workspace {
        LaunchMode::Launchd
    } else {
        LaunchMode::Direct
    }
}

/// Service state tracked by the manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceState {
    /// No service known yet.
    Unknown,
    /// A launch is in flight.
    Starting,
    /// The service answered a probe.
    Live,
    /// The service is not reachable; a retry is scheduled or due.
    Unavailable { last_attempt: Option<Instant> },
}

/// Pure retry policy: when may the next launch be attempted?
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    last_attempt: Option<Instant>,
}

impl RetryPolicy {
    pub fn new() -> Self {
        Self { last_attempt: None }
    }

    /// Records an attempt at `now`.
    pub fn record_attempt(&mut self, now: Instant) {
        self.last_attempt = Some(now);
    }

    /// True when `now` is at least `LAUNCH_RETRY_COOLDOWN` after the last
    /// attempt (or there was no attempt yet).
    pub fn may_attempt(&self, now: Instant) -> bool {
        match self.last_attempt {
            None => true,
            Some(last) => now.duration_since(last) >= LAUNCH_RETRY_COOLDOWN,
        }
    }

    /// How long to wait before the next attempt is allowed.
    pub fn wait_for(&self, now: Instant) -> Duration {
        match self.last_attempt {
            None => Duration::ZERO,
            Some(last) => LAUNCH_RETRY_COOLDOWN
                .checked_sub(now.duration_since(last))
                .unwrap_or(Duration::ZERO),
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new()
    }
}

/// Capabilities the platform adapter registers through the bridge.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdapterCapabilities {
    pub notifications: bool,
    pub thumbnails: bool,
    pub overlays: bool,
}

/// A registered platform adapter callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRegistration {
    pub token: String,
    pub capabilities: AdapterCapabilities,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_mode_prefers_launchd_for_registered_workspaces() {
        assert_eq!(
            launch_mode_for(true, false, false, false),
            LaunchMode::Launchd
        );
        assert_eq!(
            launch_mode_for(false, true, false, false),
            LaunchMode::Launchd
        );
    }

    #[test]
    fn launch_mode_forks_directly_for_private_homes() {
        assert_eq!(
            launch_mode_for(false, false, false, false),
            LaunchMode::Direct
        );
    }

    #[test]
    fn launch_mode_never_starts_background_service_for_tests() {
        assert_eq!(launch_mode_for(true, true, true, false), LaunchMode::Direct);
        assert_eq!(launch_mode_for(true, true, false, true), LaunchMode::Direct);
    }

    #[test]
    fn retry_policy_allows_first_attempt_immediately() {
        let policy = RetryPolicy::new();
        assert!(policy.may_attempt(Instant::now()));
        assert_eq!(policy.wait_for(Instant::now()), Duration::ZERO);
    }

    #[test]
    fn retry_policy_enforces_cooldown() {
        let mut policy = RetryPolicy::new();
        let now = Instant::now();
        policy.record_attempt(now);
        assert!(!policy.may_attempt(now));
        assert!(!policy.may_attempt(now + Duration::from_secs(4)));
        assert!(policy.may_attempt(now + Duration::from_secs(5)));
        assert!(policy.may_attempt(now + Duration::from_secs(6)));
    }

    #[test]
    fn retry_policy_wait_for_counts_down() {
        let mut policy = RetryPolicy::new();
        let now = Instant::now();
        policy.record_attempt(now);
        let wait = policy.wait_for(now + Duration::from_secs(2));
        assert!(wait <= Duration::from_secs(3) && wait >= Duration::from_secs(2));
        assert_eq!(
            policy.wait_for(now + Duration::from_secs(5)),
            Duration::ZERO
        );
    }
}
