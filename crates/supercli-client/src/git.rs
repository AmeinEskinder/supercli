//! Git repository introspection shared by the Host and portable clients.
//!
//! [`head_branch`] reads the current branch (or the short commit for a
//! detached HEAD) by parsing `.git/HEAD` directly, following worktree
//! `.git` files to the real gitdir — matching the native Host's
//! `GitHeadReader` and the Swift `GitHeadReader.currentBranch(repoPath:)`
//! it was ported from.
//!
//! Single implementation: previously duplicated as `supercli-core`'s private
//! `controller_host::git_head_branch` (now re-exported from this module as
//! `supercli_core::git::head_branch`). This crate is the home because the
//! workspace dependency graph forbids `supercli-client` from depending on
//! `supercli-core` (`core → connector → client` would cycle).
//!
//! Web-safe: compiles for `wasm32-unknown-unknown` (`std::fs` compiles on
//! wasm32; the calls fail gracefully at runtime where there is no FS).

use std::path::Path;

/// Current HEAD branch of the checkout at `repo_path`.
///
/// Follows a worktree `.git` file to the real gitdir. Returns `None` when
/// there is no `.git`, HEAD is unreadable or empty, or the ref is malformed
/// (e.g. `ref: refs/heads/` with an empty branch name). For a detached HEAD,
/// returns the 7-char short commit instead of nothing.
pub fn head_branch(repo_path: &str) -> Option<String> {
    let git_entry = Path::new(repo_path).join(".git");
    let meta = std::fs::metadata(&git_entry).ok()?;
    let head_path = if meta.is_dir() {
        git_entry.join("HEAD")
    } else {
        let contents = std::fs::read_to_string(&git_entry).ok()?;
        let gitdir = contents.lines().find_map(|line| {
            line.strip_prefix("gitdir:")
                .map(|rest| rest.trim().to_owned())
        })?;
        let resolved = if Path::new(&gitdir).is_absolute() {
            std::path::PathBuf::from(gitdir)
        } else {
            Path::new(repo_path).join(gitdir)
        };
        resolved.join("HEAD")
    };
    let head = std::fs::read_to_string(head_path).ok()?;
    let head = head.trim();
    if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
        // An empty branch name is a malformed HEAD, not a branch.
        if branch.is_empty() {
            return None;
        }
        Some(branch.to_string())
    } else if !head.is_empty() {
        Some(head.chars().take(7).collect())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_branch_reads_checkout_worktree_and_detached_head() {
        // Moved from supercli-core's controller_host tests: single
        // implementation, single home.
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(head_branch(repo.to_str().unwrap()).as_deref(), Some("main"));

        let worktree = root.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        let gitdir = repo.join(".git/worktrees/feature");
        std::fs::create_dir_all(&gitdir).unwrap();
        std::fs::write(gitdir.join("HEAD"), "ref: refs/heads/feature/x\n").unwrap();
        std::fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", gitdir.display()),
        )
        .unwrap();
        assert_eq!(
            head_branch(worktree.to_str().unwrap()).as_deref(),
            Some("feature/x")
        );

        std::fs::write(repo.join(".git/HEAD"), "abcdef1234567890\n").unwrap();
        assert_eq!(
            head_branch(repo.to_str().unwrap()).as_deref(),
            Some("abcdef1")
        );

        let empty = root.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(head_branch(empty.to_str().unwrap()), None);
    }

    #[test]
    fn head_branch_reads_branch() {
        // Moved from remote_dto_adapters' git_head_reader tests.
        let dir = std::env::temp_dir().join(format!("git-test-branch-{}", std::process::id()));
        let git = dir.join(".git");
        std::fs::create_dir_all(&git).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/feature-x\n").unwrap();
        assert_eq!(
            head_branch(dir.to_str().unwrap()),
            Some("feature-x".to_string())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn head_branch_detached_head_short_commit() {
        // Moved from remote_dto_adapters' git_head_reader tests.
        let dir = std::env::temp_dir().join(format!("git-test-detached-{}", std::process::id()));
        let git = dir.join(".git");
        std::fs::create_dir_all(&git).unwrap();
        std::fs::write(git.join("HEAD"), "abc1234567890\n").unwrap();
        assert_eq!(
            head_branch(dir.to_str().unwrap()),
            Some("abc1234".to_string())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn head_branch_follows_worktree_gitdir_file() {
        // Moved from remote_dto_adapters' git_head_reader tests.
        let dir = std::env::temp_dir().join(format!("git-test-worktree-{}", std::process::id()));
        let real_git = dir.join("real-git");
        std::fs::create_dir_all(&real_git).unwrap();
        std::fs::write(real_git.join("HEAD"), "ref: refs/heads/wt-branch\n").unwrap();
        // `.git` is a FILE pointing at the real gitdir.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".git"), "gitdir: real-git\n").unwrap();
        assert_eq!(
            head_branch(dir.to_str().unwrap()),
            Some("wt-branch".to_string())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn head_branch_missing_repo_returns_none() {
        // Moved from remote_dto_adapters' git_head_reader tests.
        assert_eq!(head_branch("/nonexistent/path/xyz"), None);
    }

    #[test]
    fn head_branch_empty_branch_ref_returns_none() {
        let dir = std::env::temp_dir().join(format!("git-test-emptyref-{}", std::process::id()));
        let git = dir.join(".git");
        std::fs::create_dir_all(&git).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/\n").unwrap();
        assert_eq!(head_branch(dir.to_str().unwrap()), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
