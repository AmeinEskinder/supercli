//! Port of `RemoteDTOAdapters.swift` (SupercliNative).
//!
//! Pure adapters between local model types and the remote DTOs sent to
//! mobile Controllers. Covers:
//!
//! - session status/activity mappings to the remote enum values;
//! - [`MobilePaneGroupProjection::scope_id`] — map a mux selection key to a
//!   Controller scope ID.
//!
//! Git HEAD reading (the Swift `GitHeadReader.currentBranch`) lives in the
//! single implementation at [`crate::git::head_branch`] (re-exported for the
//! Host as `supercli_core::git::head_branch`).
//!
//! The Swift original also has extensions on `Project`, `Preset`,
//! `SessionEntry`, and `SupercliStore` that build full DTOs; those depend on
//! app-local model types with no Rust equivalent yet and are not ported
//! here. The pure, testable logic is.
//!
//! Web-safe: compiles for `wasm32-unknown-unknown`.

/// Local session lifecycle status (mirrors the Swift `SessionStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Starting,
    Busy,
    Idle,
    Attention,
    Exited,
}

/// Remote session status sent to Controllers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteSessionStatus {
    Running,
    Exited,
}

impl SessionStatus {
    /// Port of `SessionStatus.remoteStatus` from `RemoteDTOAdapters.swift`.
    pub fn remote_status(self) -> RemoteSessionStatus {
        match self {
            SessionStatus::Exited => RemoteSessionStatus::Exited,
            SessionStatus::Starting
            | SessionStatus::Busy
            | SessionStatus::Idle
            | SessionStatus::Attention => RemoteSessionStatus::Running,
        }
    }
}

/// Local session activity status (mirrors the Swift `SessionActivityStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionActivityStatus {
    Starting,
    Working,
    Blocked,
    Done,
    Idle,
    Exited,
}

/// Remote activity state sent to Controllers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteActivityState {
    Starting,
    Working,
    Blocked,
    Done,
    Idle,
}

impl SessionActivityStatus {
    /// Port of `SessionActivityStatus.remoteActivity` from
    /// `RemoteDTOAdapters.swift`.
    pub fn remote_activity(self) -> RemoteActivityState {
        match self {
            SessionActivityStatus::Starting => RemoteActivityState::Starting,
            SessionActivityStatus::Working => RemoteActivityState::Working,
            SessionActivityStatus::Blocked => RemoteActivityState::Blocked,
            SessionActivityStatus::Done => RemoteActivityState::Done,
            SessionActivityStatus::Idle | SessionActivityStatus::Exited => {
                RemoteActivityState::Idle
            }
        }
    }
}

/// Projection of pane groups for mobile Controllers.
///
/// Port of `MobilePaneGroupProjection` from `RemoteDTOAdapters.swift`.
pub struct MobilePaneGroupProjection;

impl MobilePaneGroupProjection {
    /// Map a mobile workspace mux selection key to a Controller scope ID.
    ///
    /// - `None` → `"local"` (no selection = local workspace);
    /// - `"local:<home>"` → `"workspace:<home>"`;
    /// - `"ssh:<hostID>"` → `"host:<hostID>"`;
    /// - `"host:<...>"` → unchanged (must be longer than the prefix);
    /// - anything else → `None`.
    ///
    /// Port of `MobilePaneGroupProjection.scopeID(forSelectionKey:)` from
    /// `RemoteDTOAdapters.swift`.
    pub fn scope_id(selection_key: Option<&str>) -> Option<String> {
        let key = selection_key?;
        if key.is_empty() {
            // Swift: `guard let selectionKey` — empty string is not nil in
            // Swift, so it falls through to the prefix checks and returns
            // nil. Match that: empty → nil.
            return None;
        }
        // Note: the Swift original returns "local" for nil; we take
        // Option<&str> so nil is None. Callers that want the Swift nil
        // behavior should map None → "local" themselves.
        if let Some(home) = key.strip_prefix("local:") {
            if home.is_empty() {
                return None;
            }
            return Some(format!("workspace:{home}"));
        }
        if let Some(host_id) = key.strip_prefix("ssh:") {
            if host_id.is_empty() {
                return None;
            }
            return Some(format!("host:{host_id}"));
        }
        if let Some(rest) = key.strip_prefix("host:") {
            if rest.is_empty() {
                return None;
            }
            return Some(key.to_string());
        }
        None
    }

    /// Scope ID for a nil selection key (Swift `guard let ... else return
    /// "local"`).
    pub fn scope_id_or_local(selection_key: Option<&str>) -> String {
        Self::scope_id(selection_key).unwrap_or_else(|| "local".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_status_maps_to_remote() {
        assert_eq!(
            SessionStatus::Exited.remote_status(),
            RemoteSessionStatus::Exited
        );
        for s in [
            SessionStatus::Starting,
            SessionStatus::Busy,
            SessionStatus::Idle,
            SessionStatus::Attention,
        ] {
            assert_eq!(s.remote_status(), RemoteSessionStatus::Running, "{s:?}");
        }
    }

    #[test]
    fn activity_status_maps_to_remote() {
        assert_eq!(
            SessionActivityStatus::Starting.remote_activity(),
            RemoteActivityState::Starting
        );
        assert_eq!(
            SessionActivityStatus::Working.remote_activity(),
            RemoteActivityState::Working
        );
        assert_eq!(
            SessionActivityStatus::Blocked.remote_activity(),
            RemoteActivityState::Blocked
        );
        assert_eq!(
            SessionActivityStatus::Done.remote_activity(),
            RemoteActivityState::Done
        );
        assert_eq!(
            SessionActivityStatus::Idle.remote_activity(),
            RemoteActivityState::Idle
        );
        assert_eq!(
            SessionActivityStatus::Exited.remote_activity(),
            RemoteActivityState::Idle
        );
    }

    #[test]
    fn scope_id_nil_is_none() {
        assert_eq!(MobilePaneGroupProjection::scope_id(None), None);
        assert_eq!(MobilePaneGroupProjection::scope_id_or_local(None), "local");
    }

    #[test]
    fn scope_id_local_prefix() {
        assert_eq!(
            MobilePaneGroupProjection::scope_id(Some("local:/Users/amein")),
            Some("workspace:/Users/amein".to_string())
        );
        // Empty home → None.
        assert_eq!(MobilePaneGroupProjection::scope_id(Some("local:")), None);
    }

    #[test]
    fn scope_id_ssh_prefix() {
        assert_eq!(
            MobilePaneGroupProjection::scope_id(Some("ssh:mac-1")),
            Some("host:mac-1".to_string())
        );
        assert_eq!(MobilePaneGroupProjection::scope_id(Some("ssh:")), None);
    }

    #[test]
    fn scope_id_host_prefix_passthrough() {
        assert_eq!(
            MobilePaneGroupProjection::scope_id(Some("host:mac-1")),
            Some("host:mac-1".to_string())
        );
        // Bare "host:" → None (must be longer than the prefix).
        assert_eq!(MobilePaneGroupProjection::scope_id(Some("host:")), None);
    }

    #[test]
    fn scope_id_unknown_returns_none() {
        assert_eq!(MobilePaneGroupProjection::scope_id(Some("bogus")), None);
        assert_eq!(MobilePaneGroupProjection::scope_id(Some("")), None);
    }
}
