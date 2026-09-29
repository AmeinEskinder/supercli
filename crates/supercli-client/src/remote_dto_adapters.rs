//! Port of `RemoteDTOAdapters.swift` (SupercliNative).
//!
//! Pure adapters between local model types and the remote DTOs sent to
//! mobile Controllers. Covers:
//!
//! - [`git_head_reader::current_branch`] — read the current git branch
//!   without spawning `git` (parse `.git/HEAD` directly, following worktree
//!   `.git` files);
//! - session status/activity mappings to the remote enum values;
//! - [`MobilePaneGroupProjection::scope_id`] — map a mux selection key to a
//!   Controller scope ID.
//!
//! The Swift original also has extensions on `Project`, `Preset`,
//! `SessionEntry`, and `SupercliStore` that build full DTOs; those depend on
//! app-local model types with no Rust equivalent yet and are not ported
//! here. The pure, testable logic is.
//!
//! Web-safe: compiles for `wasm32-unknown-unknown`.

/// Read the current git branch without spawning `git`.
///
/// Parses `.git/HEAD` directly. Worktree checkouts have a `.git` FILE
/// pointing at the real gitdir — follow it. Cheap enough to run per project
/// on every bootstrap.
///
/// Returns `None` if the repo has no `.git` or HEAD is unreadable. For a
/// detached HEAD, returns the 7-char short commit instead of nothing.
///
/// Port of `GitHeadReader.currentBranch(repoPath:)` from
/// `RemoteDTOAdapters.swift`.
pub mod git_head_reader {
    use std::path::Path;

    /// Read the current branch (or short commit for detached HEAD) for the
    /// repo at `repo_path`. Returns `None` if unavailable.
    pub fn current_branch(repo_path: &str) -> Option<String> {
        let git_entry = Path::new(repo_path).join(".git");
        let mut head_path = git_entry.join("HEAD");

        // Check if .git exists and whether it's a file (worktree) or dir.
        let meta = std::fs::metadata(&git_entry).ok()?;
        if meta.is_file() {
            // Worktree: `.git` is a file containing `gitdir: <path>`.
            let contents = std::fs::read_to_string(&git_entry).ok()?;
            let gitdir_line = contents.lines().find(|l| l.starts_with("gitdir:"))?;
            let gitdir = gitdir_line["gitdir:".len()..].trim();
            let resolved = if Path::new(gitdir).is_absolute() {
                Path::new(gitdir).to_path_buf()
            } else {
                Path::new(repo_path).join(gitdir)
            };
            head_path = resolved.join("HEAD");
        } else if !meta.is_dir() {
            return None;
        }

        let head = std::fs::read_to_string(&head_path).ok()?;
        let trimmed = head.trim();
        if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
            if branch.is_empty() {
                return None;
            }
            return Some(branch.to_string());
        }
        // Detached HEAD: show the short commit instead of nothing.
        if trimmed.is_empty() {
            return None;
        }
        Some(trimmed.chars().take(7).collect())
    }
}

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

    #[test]
    fn git_head_reader_branch() {
        let dir = std::env::temp_dir().join(format!("dto-adapters-test-{}", std::process::id()));
        let git = dir.join(".git");
        std::fs::create_dir_all(&git).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/feature-x\n").unwrap();
        assert_eq!(
            git_head_reader::current_branch(dir.to_str().unwrap()),
            Some("feature-x".to_string())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn git_head_reader_detached() {
        let dir =
            std::env::temp_dir().join(format!("dto-adapters-detached-{}", std::process::id()));
        let git = dir.join(".git");
        std::fs::create_dir_all(&git).unwrap();
        std::fs::write(git.join("HEAD"), "abc1234567890\n").unwrap();
        assert_eq!(
            git_head_reader::current_branch(dir.to_str().unwrap()),
            Some("abc1234".to_string())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn git_head_reader_worktree_file() {
        let dir =
            std::env::temp_dir().join(format!("dto-adapters-worktree-{}", std::process::id()));
        let real_git = dir.join("real-git");
        std::fs::create_dir_all(&real_git).unwrap();
        std::fs::write(real_git.join("HEAD"), "ref: refs/heads/wt-branch\n").unwrap();
        // `.git` is a FILE pointing at the real gitdir.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".git"), "gitdir: real-git\n").unwrap();
        assert_eq!(
            git_head_reader::current_branch(dir.to_str().unwrap()),
            Some("wt-branch".to_string())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn git_head_reader_missing_returns_none() {
        assert_eq!(
            git_head_reader::current_branch("/nonexistent/path/xyz"),
            None
        );
    }
}
