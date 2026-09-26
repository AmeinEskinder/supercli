//! Remote-content banner policy. Ported from
//! `clients/native/SupercliNative/Sources/SupercliNative/RemoteContentBannerPolicy.swift`
//! (mirroring `clients/ios/SupercliIOS/Sources/SupercliIOS/RemoteContentBannerPolicy.swift`).
//!
//! Decides whether a session's terminal output should show the
//! "Remote content" banner and, when `alwaysShow` is on, whether the
//! per-session "remote" badge should be painted.
//!
//! A session is remote when it is not bound to this workspace's own
//! process tree — i.e. it came through SSH, a remote workspace's
//! `ssh-*`/remote record, the relay (the address is a relay address),
//! OR the Host says the session is not local. Everything else (Direct
//! and Link over a workspace address) is local.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionMode {
    Direct,
    Link,
    Ssh,
    Relay,
}

impl ConnectionMode {
    pub fn is_ssh(&self) -> bool {
        matches!(self, ConnectionMode::Ssh)
    }
}

/// A remote-host record the banner policy consults.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteHostRecord {
    /// True for `ssh-*`/remote records (their traffic can only ever be remote).
    pub is_ssh_record: bool,
    /// True when the Host reports itself remote (SSH or otherwise).
    pub host_reports_remote: bool,
}

/// Whether the session's terminal output should show the "Remote content"
/// banner.
pub fn shows_remote_banner(
    connection: &ConnectionMode,
    record: Option<&RemoteHostRecord>,
    host_reports_remote: bool,
) -> bool {
    // SSH is always remote: the bytes cross a network.
    if connection.is_ssh() {
        return true;
    }
    let record = record.cloned().unwrap_or_default();
    if record.is_ssh_record {
        return true;
    }
    if record.host_reports_remote || host_reports_remote {
        return true;
    }
    if connection == &ConnectionMode::Relay {
        return true;
    }
    false
}

/// Whether the per-session "remote" badge should be painted when
/// `alwaysShow` is on: only when the banner would NOT already be shown
/// (the banner is the stronger signal).
pub fn shows_always_show_remote_badge(always_show: bool, banner_visible: bool) -> bool {
    always_show && !banner_visible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_is_always_remote() {
        let record = RemoteHostRecord::default();
        assert!(shows_remote_banner(
            &ConnectionMode::Ssh,
            Some(&record),
            false
        ));
        assert!(shows_remote_banner(&ConnectionMode::Ssh, None, false));
    }

    #[test]
    fn direct_and_link_local_without_records() {
        let record = RemoteHostRecord::default();
        assert!(!shows_remote_banner(
            &ConnectionMode::Direct,
            Some(&record),
            false
        ));
        assert!(!shows_remote_banner(
            &ConnectionMode::Link,
            Some(&record),
            false
        ));
        assert!(!shows_remote_banner(&ConnectionMode::Direct, None, false));
    }

    #[test]
    fn relay_is_remote() {
        assert!(shows_remote_banner(&ConnectionMode::Relay, None, false));
    }

    #[test]
    fn ssh_record_or_remote_host_qualifies() {
        let ssh_record = RemoteHostRecord {
            is_ssh_record: true,
            ..Default::default()
        };
        assert!(shows_remote_banner(
            &ConnectionMode::Direct,
            Some(&ssh_record),
            false
        ));
        let remote_host = RemoteHostRecord {
            host_reports_remote: true,
            ..Default::default()
        };
        assert!(shows_remote_banner(
            &ConnectionMode::Link,
            Some(&remote_host),
            false
        ));
        assert!(shows_remote_banner(&ConnectionMode::Direct, None, true));
    }

    #[test]
    fn always_show_badge_only_when_no_banner() {
        assert!(shows_always_show_remote_badge(true, false));
        assert!(!shows_always_show_remote_badge(true, true));
        assert!(!shows_always_show_remote_badge(false, false));
    }
}
