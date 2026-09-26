//! Host git, file-write, and usage routes for the desktop/mobile clients.
//!
//! These back the Git/Files/Usage app panes (`clients/supercli-app`).
//!
//! ## Security model
//!
//! A paired Controller is owner-equivalent today, so these routes are defense
//! in depth. The rules below exist so a compromised or malicious Controller
//! cannot achieve *persistent* code execution that bypasses agent approvals:
//!
//! - **Writes are project-root-only.** `files_write` refuses anything outside
//!   the registered project roots (Fix 1). The bare home directory is off
//!   limits: `~/.zshrc`, `~/.bashrc`, `~/.gitconfig` (`core.sshCommand`), and
//!   `~/Library/LaunchAgents/*.plist` are all persistent code-execution
//!   primitives.
//! - **No dotfiles, ever.** Any path component starting with `.` is refused
//!   for writes, even inside a project root. This covers `.git/` internals
//!   (`hooks/`, `config`, `info/`), `.env`, and friends.
//! - **Mutating ops require human approval via the Host's ApprovalHub.**
//!   Every state-changing route (`files_write`, `git stage/unstage/commit/
//!   fetch/pull/push`) is gated in `supercli-serve` (see
//!   `crates/supercli-serve/src/git_approval.rs`): the request creates a
//!   `PendingApproval` answered through the existing approval flow (desktop
//!   approval panel, mobile). Only after an explicit Allow does the operation
//!   execute. Denials, timeouts, and missing approvers fail closed (403).
//!   `before_*`/`after_*` lifecycle events are emitted on the
//!   `supercli-events` bus (`FileWrite`/`GitOp` doctypes) so hooks can reject
//!   or audit via `supercli hooks trace`.
//! - **Repo toplevel must stay inside a project root** (Fix 3).
//!   `git rev-parse --show-toplevel` is not trusted on its own: a path inside
//!   a registered root whose enclosing repo has `$HOME` as its toplevel
//!   (e.g. a dotfiles repo) would otherwise escape the root.
//! - **Untrusted-repo git config is neutralized** (Fix 4). Read ops pass
//!   `-c core.fsmonitor= -c core.hooksPath=/dev/null -c diff.external=`
//!   plus `--no-ext-diff --no-textconv` so a repo's `.git/config` cannot
//!   execute arbitrary commands via fsmonitor/textconv/external diff.
//!   Network ops (`fetch`/`pull`/`push`) set
//!   `GIT_SSH_COMMAND='ssh -o BatchMode=yes -o ConnectTimeout=10'` and run
//!   under a hard timeout that **kills the child process** (the HTTP-layer
//!   timeout alone would leave the git child running).

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use base64::Engine as _;
use serde_json::{json, Value};

use crate::controller_api::{ControllerRequest, ControllerResponse};
use crate::host_resources::{fail, Failure, ResourceScope};

// ---------------------------------------------------------------------------
// NOTE: Approval gating for mutating routes lives in supercli-serve
// (crates/supercli-serve/src/git_approval.rs), where the ApprovalHub lives.
// It follows the same pattern as devices.rs (HubGate -> ApprovalHub):
// the request creates a PendingApproval, and only after an explicit Allow
// does the core route handler below execute. This module enforces the
// scope/toplevel/git-hardening rules; it does NOT make approval decisions.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Hardened git invocation (Fix 4)
// ---------------------------------------------------------------------------

/// Config overrides that neutralize code execution from an untrusted repo's
/// `.git/config` for read operations:
///
/// - `core.fsmonitor=`: a repo can otherwise run an arbitrary fsmonitor
///   command on `git status`.
/// - `core.hooksPath=/dev/null`: defense in depth; read ops should not run
///   hooks at all.
/// - `diff.external=` + `--no-ext-diff` + `--no-textconv`: `git diff` can
///   otherwise run external diff drivers and textconv filters (arbitrary
///   commands) from `.gitattributes`/`.git/config`.
fn read_op_config_args() -> Vec<String> {
    vec![
        "-c".to_owned(),
        "core.fsmonitor=".to_owned(),
        "-c".to_owned(),
        "core.hooksPath=/dev/null".to_owned(),
        "-c".to_owned(),
        "diff.external=".to_owned(),
    ]
}

/// Run a git *read* command with untrusted-config hardening (Fix 4).
fn git_read(repo: &Path, args: &[&str]) -> io::Result<Output> {
    let mut cmd = Command::new("git");
    for arg in read_op_config_args() {
        cmd.arg(arg);
    }
    cmd.arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
}

/// Run a git command with a hard timeout that kills the child on expiry.
///
/// The HTTP layer has its own timeout, but that only abandons the handler —
/// the git child would keep running (e.g. `git push` hanging on an SSH
/// prompt). This spawns the child, then kills it if it has not exited within
/// `timeout`.
fn git_with_timeout(
    repo: &Path,
    config_args: &[String],
    args: &[&str],
    extra_env: &[(&str, &str)],
    timeout: Duration,
) -> io::Result<Output> {
    let mut child = Command::new("git");
    for arg in config_args {
        child.arg(arg);
    }
    child
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0");
    for (k, v) in extra_env {
        child.env(k, v);
    }
    let mut child = child
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait()? {
            Some(status) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_end(&mut stdout);
                }
                if let Some(mut err) = child.stderr.take() {
                    let _ = err.read_to_end(&mut stderr);
                }
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            None => {
                if std::time::Instant::now() >= deadline {
                    // Hard kill: do not leave the child running after we
                    // give up on it.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("git command timed out after {}s", timeout.as_secs()),
                    ));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
}

/// Hard timeouts for network operations (Fix 4).
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const PUSH_PULL_TIMEOUT: Duration = Duration::from_secs(60);

/// Run a git *network* op (`fetch`/`pull`/`push`) with SSH hardening and a
/// hard child-killing timeout (Fix 4).
///
/// - `GIT_SSH_COMMAND='ssh -o BatchMode=yes -o ConnectTimeout=10'`: never
///   prompt for passwords/passphrases (which would hang forever); fail fast
///   on unreachable hosts.
/// - `GIT_TERMINAL_PROMPT=0`: no credential prompts.
/// - The timeout kills the child; see [`git_with_timeout`].
fn git_remote(repo: &Path, args: &[&str], timeout: Duration) -> io::Result<Output> {
    git_with_timeout(
        repo,
        &read_op_config_args(),
        args,
        &[(
            "GIT_SSH_COMMAND",
            "ssh -o BatchMode=yes -o ConnectTimeout=10",
        )],
        timeout,
    )
}

fn git_ok_read(repo: &Path, args: &[&str], what: &str) -> Result<String, Failure> {
    let output = git_read(repo, args).map_err(|e| fail(500, format!("{what}: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(fail(
            422,
            if stderr.is_empty() {
                format!("{what} failed")
            } else {
                format!("{what} failed: {stderr}")
            },
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn git_ok_remote(
    repo: &Path,
    args: &[&str],
    what: &str,
    timeout: Duration,
) -> Result<String, Failure> {
    let output = git_remote(repo, args, timeout).map_err(|e| {
        if e.kind() == io::ErrorKind::TimedOut {
            fail(504, format!("{what}: {e}"))
        } else {
            fail(500, format!("{what}: {e}"))
        }
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(fail(
            422,
            if stderr.is_empty() {
                format!("{what} failed")
            } else {
                format!("{what} failed: {stderr}")
            },
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

// ---------------------------------------------------------------------------
// Path resolution (Fix 1, Fix 3)
// ---------------------------------------------------------------------------

/// Resolve a Controller-supplied path and require it to be inside a git work
/// tree whose toplevel is itself inside a registered project root.
///
/// The toplevel check (Fix 3) closes the dotfiles-repo escape: without it, a
/// path like `<root>/sub` where the enclosing repo's toplevel is `$HOME`
/// would let every git op run against the home directory repo.
fn resolve_repo(scope: &ResourceScope, request: &ControllerRequest) -> Result<PathBuf, Failure> {
    let raw = request
        .body
        .get("path")
        .and_then(|v| v.as_str())
        .or_else(|| request.query.get("path").map(String::as_str))
        .unwrap_or("");
    let resolved = scope.resolve(raw)?;
    let output = git_read(resolved.display_path(), &["rev-parse", "--show-toplevel"])
        .map_err(|e| fail(500, format!("git rev-parse: {e}")))?;
    if !output.status.success() {
        return Err(fail(404, "path is not inside a git repository"));
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if root.is_empty() {
        return Err(fail(500, "git returned an empty repository root"));
    }
    let root_path = PathBuf::from(&root);
    if !scope.is_inside_project_root(&root_path) {
        return Err(fail(
            403,
            "repository root is outside the registered project roots",
        ));
    }
    Ok(root_path)
}

/// Resolve a Controller-supplied path for writing (Fix 1).
///
/// - Must be inside a registered project root (the bare home directory is
///   off limits: `~/.zshrc`, `~/.gitconfig`, `~/Library/LaunchAgents/` are
///   persistent code-execution primitives).
/// - No dotfiles or dot-directories: any path component starting with `.`
///   is refused, even inside a root. This covers `.git/` internals
///   (`hooks/`, `config`, `info/`), `.env`, `.ssh`, and friends.
fn resolve_write_path(
    scope: &ResourceScope,
    raw: &str,
) -> Result<crate::host_resources::ResolvedPath, Failure> {
    let resolved = scope.resolve(raw)?;
    let display = resolved.display_path();
    // Find the project root this path is under (there must be one; the check
    // below enforces it). Only the components *relative to the root* are
    // subject to the dotfile rule — the root itself may legitimately live
    // under a dotted parent (e.g. a tempdir like `/tmp/.tmpXXX/repo`).
    let root = scope.project_root_for(display).ok_or_else(|| {
        fail(
            403,
            "writes are only allowed inside registered project roots",
        )
    })?;
    let relative = display.strip_prefix(&root).map_err(|_| {
        fail(
            403,
            "writes are only allowed inside registered project roots",
        )
    })?;
    // Deny dotfiles/dot-directories in the relative path. `scope.resolve`
    // already normalized the path, so a lexical component check is sufficient
    // (no `..` survives normalization). This covers `.git/` internals
    // (`hooks/`, `config`, `info/`), `.env`, `.ssh`, and friends, even inside
    // a project root.
    for component in relative.components() {
        if let std::path::Component::Normal(name) = component {
            if name.to_string_lossy().starts_with('.') {
                return Err(fail(
                    403,
                    "writes to dotfiles and dot-directories are not allowed",
                ));
            }
        }
    }
    Ok(resolved)
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

fn git_status(scope: &ResourceScope, request: &ControllerRequest) -> Result<Value, Failure> {
    let root = resolve_repo(scope, request)?;
    let branch = git_ok_read(
        &root,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        "git branch",
    )
    .ok()
    .and_then(|b| {
        let b = b.trim().to_owned();
        if b.is_empty() {
            None
        } else {
            Some(b)
        }
    })
    .or_else(|| {
        git_ok_read(&root, &["rev-parse", "--short", "HEAD"], "git rev-parse")
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    });
    let (ahead, behind) = upstream_counts(&root);
    let porcelain = git_ok_read(&root, &["status", "--porcelain=v1", "-uall"], "git status")?;
    let mut files = Vec::new();
    for line in porcelain.lines() {
        if line.len() < 4 {
            continue;
        }
        let index = line.chars().next().unwrap_or(' ');
        let worktree = line.chars().nth(1).unwrap_or(' ');
        let path = line[3..].to_owned();
        files.push(json!({
            "path": path,
            "indexStatus": index.to_string(),
            "worktreeStatus": worktree.to_string(),
            "staged": index != ' ' && index != '?',
        }));
    }
    Ok(json!({
        "repoRoot": root.to_string_lossy(),
        "branch": branch,
        "ahead": ahead,
        "behind": behind,
        "files": files,
    }))
}

fn upstream_counts(root: &Path) -> (u64, u64) {
    let upstream = match git_read(root, &["rev-parse", "--abbrev-ref", "@{upstream}"]) {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        _ => return (0, 0),
    };
    if upstream.is_empty() {
        return (0, 0);
    }
    let counts = match git_read(
        root,
        &[
            "rev-list",
            "--left-right",
            "--count",
            "HEAD...@{upstream}",
            "--",
        ],
    ) {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        _ => return (0, 0),
    };
    let mut parts = counts.split_whitespace();
    let ahead = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let behind = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    (ahead, behind)
}

fn git_diff(scope: &ResourceScope, request: &ControllerRequest) -> Result<Value, Failure> {
    let root = resolve_repo(scope, request)?;
    let file = request
        .query
        .get("file")
        .map(String::as_str)
        .or_else(|| request.body.get("file").and_then(|v| v.as_str()))
        .unwrap_or("");
    if file.is_empty() || file.contains('\0') || file.contains("..") {
        return Err(fail(400, "file required"));
    }
    // `git diff HEAD -- <file>` covers staged + unstaged in one view.
    // `--no-ext-diff`/`--no-textconv` neutralize external diff drivers and
    // textconv filters from `.gitattributes`/`.git/config` (Fix 4); the
    // `-c diff.external=` in `git_read` is the belt to these suspenders.
    let diff = git_ok_read(
        &root,
        &["diff", "--no-ext-diff", "--no-textconv", "HEAD", "--", file],
        "git diff",
    )?;
    Ok(json!({ "file": file, "diff": diff }))
}

fn git_history(scope: &ResourceScope, request: &ControllerRequest) -> Result<Value, Failure> {
    let root = resolve_repo(scope, request)?;
    let limit: usize = request
        .query
        .get("limit")
        .map(String::as_str)
        .unwrap_or("50")
        .parse()
        .unwrap_or(50)
        .clamp(1, 200);
    let log = git_ok_read(
        &root,
        &[
            "log",
            &format!("-{limit}"),
            "--format=%H%x00%an%x00%ad%x00%s%x1e",
            "--date=iso",
        ],
        "git log",
    )?;
    let mut commits = Vec::new();
    for record in log.split('\x1e') {
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let mut fields = record.split('\0');
        let (Some(sha), Some(author), Some(date), Some(message)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        commits.push(json!({
            "sha": sha,
            "author": author,
            "date": date,
            "message": message,
        }));
    }
    Ok(json!({ "commits": commits }))
}

fn body_files(request: &ControllerRequest) -> Result<Vec<String>, Failure> {
    let files = request
        .body
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or_else(|| fail(400, "files array required"))?;
    let mut out = Vec::with_capacity(files.len());
    for file in files {
        let path = file
            .as_str()
            .ok_or_else(|| fail(400, "files must be strings"))?;
        if path.is_empty() || path.contains('\0') || path.starts_with('/') || path.contains("..") {
            return Err(fail(400, format!("invalid file path: {path}")));
        }
        // Dotfiles are never stageable through this route: staging
        // `.git/config` or `.env` via a Controller is not a thing.
        if path.split('/').any(|c| c.starts_with('.')) {
            return Err(fail(403, format!("dotfiles are not stageable: {path}")));
        }
        out.push(path.to_owned());
    }
    Ok(out)
}

fn git_stage(
    scope: &ResourceScope,
    request: &ControllerRequest,
    unstage: bool,
) -> Result<Value, Failure> {
    let what = if unstage { "git unstage" } else { "git stage" };
    let root = resolve_repo(scope, request)?;
    let files = body_files(request)?;
    let mut args: Vec<&str> = if unstage {
        vec!["restore", "--staged", "--"]
    } else {
        vec!["add", "--"]
    };
    args.extend(files.iter().map(String::as_str));
    // `git add`/`restore` do not read diff config, but keep the fsmonitor
    // guard for consistency.
    let output = git_read(&root, &args).map_err(|e| fail(500, format!("{what}: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(fail(
            422,
            if stderr.is_empty() {
                format!("{what} failed")
            } else {
                format!("{what} failed: {stderr}")
            },
        ));
    }
    Ok(json!({ "ok": true }))
}

fn git_commit(scope: &ResourceScope, request: &ControllerRequest) -> Result<Value, Failure> {
    let root = resolve_repo(scope, request)?;
    let message = request
        .body
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_owned();
    if message.is_empty() {
        return Err(fail(400, "commit message required"));
    }
    if message.len() > 4096 {
        return Err(fail(400, "commit message too long"));
    }
    // `commit` can run hooks; neutralize the code-execution vectors.
    let output = git_read(&root, &["commit", "-m", &message])
        .map_err(|e| fail(500, format!("git commit: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(fail(
            422,
            if stderr.is_empty() {
                "git commit failed".to_owned()
            } else {
                format!("git commit failed: {stderr}")
            },
        ));
    }
    Ok(json!({ "ok": true }))
}

fn git_remote_op(
    scope: &ResourceScope,
    request: &ControllerRequest,
    op: &str,
) -> Result<Value, Failure> {
    let root = resolve_repo(scope, request)?;
    let (args, timeout): (&[&str], Duration) = match op {
        "fetch" => (&["fetch", "--prune"], FETCH_TIMEOUT),
        "pull" => (&["pull", "--ff-only"], PUSH_PULL_TIMEOUT),
        "push" => (&["push"], PUSH_PULL_TIMEOUT),
        _ => return Err(fail(400, "unknown git operation")),
    };
    // Network ops run under a hard timeout that kills the git child (Fix 4);
    // the HTTP layer timeout alone would leave it running.
    git_ok_remote(&root, args, &format!("git {op}"), timeout)?;
    Ok(json!({ "ok": true }))
}

fn files_list(scope: &ResourceScope, request: &ControllerRequest) -> Result<Value, Failure> {
    let raw = request.query.get("path").map(String::as_str).unwrap_or("");
    let resolved = scope.resolve(raw)?;
    let entries = resolved.list_file_entries()?;
    Ok(json!({
        "path": resolved.display_path(),
        "entries": entries.iter().map(|e| json!({
            "name": e.name,
            "isDir": e.is_dir,
            "size": e.size,
        })).collect::<Vec<_>>(),
    }))
}

fn files_write(scope: &ResourceScope, request: &ControllerRequest) -> Result<Value, Failure> {
    let raw = request
        .body
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let content_b64 = request
        .body
        .get("contentBase64")
        .and_then(|v| v.as_str())
        .ok_or_else(|| fail(400, "contentBase64 required"))?;
    if content_b64.len() > 8 * 1024 * 1024 {
        return Err(fail(400, "content too large (max 8 MiB)"));
    }
    // Fix 1: project roots only, no dotfiles/dot-directories.
    let resolved = resolve_write_path(scope, raw)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content_b64)
        .map_err(|_| fail(400, "contentBase64 is not valid base64"))?;
    resolved.write_bytes(&bytes)?;
    Ok(json!({ "ok": true, "bytesWritten": bytes.len() }))
}

fn usage_stats() -> Value {
    // Host-side usage: session counts plus per-provider transcript presence.
    // Token/quota figures come from the providers' own APIs (see the usage
    // app); the Host reports what it can measure locally.
    let sessions_root = crate::session_host::app_sessions_root();
    let (mut total, mut running) = (0usize, 0usize);
    if let Ok(entries) = std::fs::read_dir(&sessions_root) {
        for entry in entries.flatten() {
            let manifest = entry.path().join("manifest.json");
            if !manifest.is_file() {
                continue;
            }
            total += 1;
            if let Ok(text) = std::fs::read_to_string(&manifest) {
                if let Ok(value) = serde_json::from_str::<Value>(&text) {
                    if value
                        .get("state")
                        .and_then(|v| v.as_str())
                        .is_some_and(|state| state == "running")
                    {
                        running += 1;
                    }
                }
            }
        }
    }
    let home = dirs::home_dir();
    let mut providers = Vec::new();
    if let Some(home) = &home {
        for (name, dir) in [("claude", ".claude/projects"), ("codex", ".codex/sessions")] {
            let path = home.join(dir);
            let sessions_on_disk = std::fs::read_dir(&path)
                .map(|entries| entries.count())
                .unwrap_or(0);
            providers.push(json!({
                "provider": name,
                "transcriptDir": path.to_string_lossy(),
                "transcriptSessions": sessions_on_disk,
            }));
        }
    }
    json!({
        "hostVersion": env!("CARGO_PKG_VERSION"),
        "sessionsTotal": total,
        "sessionsRunning": running,
        "providers": providers,
    })
}

/// Route git/files/usage requests. Returns `None` for unowned routes.
pub fn route(request: &ControllerRequest) -> Option<ControllerResponse> {
    route_with_scope(&ResourceScope::from_host(), request)
}

/// Same as [`route`] but with an explicit scope (used by tests).
pub(crate) fn route_with_scope(
    scope: &ResourceScope,
    request: &ControllerRequest,
) -> Option<ControllerResponse> {
    let result = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/mobile/git/status") => git_status(scope, request),
        ("GET", "/mobile/git/diff") => git_diff(scope, request),
        ("GET", "/mobile/git/history") => git_history(scope, request),
        ("POST", "/mobile/git/stage") => git_stage(scope, request, false),
        ("POST", "/mobile/git/unstage") => git_stage(scope, request, true),
        ("POST", "/mobile/git/commit") => git_commit(scope, request),
        ("POST", "/mobile/git/fetch") => git_remote_op(scope, request, "fetch"),
        ("POST", "/mobile/git/pull") => git_remote_op(scope, request, "pull"),
        ("POST", "/mobile/git/push") => git_remote_op(scope, request, "push"),
        ("GET", "/mobile/files/list") => files_list(scope, request),
        ("POST", "/mobile/files/write") => files_write(scope, request),
        ("GET", "/mobile/usage/stats") => Ok(usage_stats()),
        _ => return None,
    };
    let (status, body) = match result {
        Ok(body) => (200, body),
        Err(failure) => failure,
    };
    Some(ControllerResponse {
        id: request.id.clone(),
        status,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn request(method: &str, path: &str) -> ControllerRequest {
        ControllerRequest {
            id: Some("test-git".into()),
            method: method.into(),
            path: path.into(),
            query: HashMap::new(),
            body: Value::Null,
            content_type: None,
            body_base64: None,
            principal: crate::controller_api::ControllerPrincipal::OwnerTransport {
                transport: "test".into(),
                subject: None,
                principal_id: None,
            },
        }
    }

    /// POST request helper. NOTE: approval gating now lives in supercli-serve
    /// (git_approval.rs); these core tests exercise the scope/toplevel/git
    /// hardening rules directly, without the approval layer.
    fn post(scope: &ResourceScope, path: &str, body: Value) -> (u16, Value) {
        let mut req = request("POST", path);
        req.body = body;
        let resp = route_with_scope(scope, &req).expect("route owned");
        (resp.status, resp.body)
    }

    struct Fixture {
        _root: tempfile::TempDir,
        scope: ResourceScope,
        repo: std::path::PathBuf,
        home: std::path::PathBuf,
    }

    fn sh(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .output()
            .expect("git failed to run");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn fixture() -> Fixture {
        let root = tempfile::tempdir().expect("tempdir");
        let home = root.path().join("home");
        let repo = root.path().join("repo");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        sh(&repo, &["init", "-b", "main"]);
        sh(&repo, &["config", "user.email", "test@example.com"]);
        sh(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-m", "initial"]);
        let scope = ResourceScope::new(Some(home.clone()), vec![repo.clone()]);
        Fixture {
            _root: root,
            scope,
            repo,
            home,
        }
    }

    fn get(scope: &ResourceScope, path: &str, query: &[(&str, &str)]) -> (u16, Value) {
        let mut req = request("GET", path);
        for (k, v) in query {
            req.query.insert((*k).into(), (*v).into());
        }
        let resp = route_with_scope(scope, &req).expect("route owned");
        (resp.status, resp.body)
    }

    #[test]
    fn git_status_reports_branch_and_clean_tree() {
        let f = fixture();
        let repo = f.repo.to_string_lossy().into_owned();
        let (status, body) = get(&f.scope, "/mobile/git/status", &[("path", &repo)]);
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["branch"], json!("main"));
        assert_eq!(body["ahead"], json!(0));
        assert!(body["files"].as_array().unwrap().is_empty());
    }

    #[test]
    fn git_status_reports_modified_and_untracked_files() {
        let f = fixture();
        let repo = f.repo.to_string_lossy().into_owned();
        std::fs::write(f.repo.join("a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(f.repo.join("new.txt"), "new\n").unwrap();
        let (status, body) = get(&f.scope, "/mobile/git/status", &[("path", &repo)]);
        assert_eq!(status, 200, "{body}");
        let files = body["files"].as_array().unwrap();
        assert_eq!(files.len(), 2);
        let a = files.iter().find(|f| f["path"] == "a.txt").unwrap();
        assert_eq!(a["worktreeStatus"], json!("M"));
        assert_eq!(a["staged"], json!(false));
    }

    #[test]
    fn git_status_rejects_non_repo() {
        let f = fixture();
        let home = f.home.to_string_lossy().into_owned();
        let (status, _) = get(&f.scope, "/mobile/git/status", &[("path", &home)]);
        assert_eq!(status, 404);
    }

    #[test]
    fn git_stage_commit_and_diff_roundtrip() {
        let f = fixture();
        let repo = f.repo.to_string_lossy().into_owned();
        std::fs::write(f.repo.join("a.txt"), "one\ntwo\n").unwrap();

        let (status, _) = post(
            &f.scope,
            "/mobile/git/stage",
            json!({"path": repo, "files": ["a.txt"]}),
        );
        assert_eq!(status, 200);

        let (status, body) = get(&f.scope, "/mobile/git/status", &[("path", &repo)]);
        assert_eq!(status, 200, "{body}");
        let a = body["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["path"] == "a.txt")
            .unwrap();
        assert_eq!(a["staged"], json!(true));

        let (status, body) = post(
            &f.scope,
            "/mobile/git/commit",
            json!({"path": repo, "message": "second"}),
        );
        assert_eq!(status, 200, "{body}");

        let (status, body) = get(
            &f.scope,
            "/mobile/git/history",
            &[("path", &repo), ("limit", "5")],
        );
        assert_eq!(status, 200, "{body}");
        let commits = body["commits"].as_array().unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0]["message"], json!("second"));

        // Diff of the committed change is empty against HEAD now.
        let (status, body) = get(
            &f.scope,
            "/mobile/git/diff",
            &[("path", &repo), ("file", "a.txt")],
        );
        assert_eq!(status, 200, "{body}");
        assert!(body["diff"].as_str().unwrap().is_empty());
    }

    #[test]
    fn git_commit_rejects_empty_message() {
        let f = fixture();
        let repo = f.repo.to_string_lossy().into_owned();
        let (status, _) = post(
            &f.scope,
            "/mobile/git/commit",
            json!({"path": repo, "message": "   "}),
        );
        assert_eq!(status, 400);
    }

    #[test]
    fn git_diff_shows_unstaged_changes() {
        let f = fixture();
        let repo = f.repo.to_string_lossy().into_owned();
        std::fs::write(f.repo.join("a.txt"), "one\ntwo\n").unwrap();
        let (status, body) = get(
            &f.scope,
            "/mobile/git/diff",
            &[("path", &repo), ("file", "a.txt")],
        );
        assert_eq!(status, 200, "{body}");
        let diff = body["diff"].as_str().unwrap();
        assert!(diff.contains("+two"), "{diff}");
    }

    #[test]
    fn files_list_and_write_roundtrip() {
        let f = fixture();
        let repo = f.repo.to_string_lossy().into_owned();
        let (status, body) = get(&f.scope, "/mobile/files/list", &[("path", &repo)]);
        assert_eq!(status, 200, "{body}");
        let names: Vec<_> = body["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap().to_owned())
            .collect();
        assert!(names.contains(&"a.txt".to_owned()), "{names:?}");

        let content = base64::engine::general_purpose::STANDARD.encode("written");
        let (status, body) = post(
            &f.scope,
            "/mobile/files/write",
            json!({"path": format!("{repo}/b.txt"), "contentBase64": content}),
        );
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            std::fs::read_to_string(f.repo.join("b.txt")).unwrap(),
            "written"
        );
    }

    #[test]
    fn files_write_rejects_path_outside_scope() {
        let f = fixture();
        let content = base64::engine::general_purpose::STANDARD.encode("evil");
        let (status, _) = post(
            &f.scope,
            "/mobile/files/write",
            json!({"path": "/etc/evil.txt", "contentBase64": content}),
        );
        assert!(status == 403 || status == 400, "got {status}");
    }

    #[test]
    fn usage_stats_returns_host_shape() {
        let f = fixture();
        let (status, body) = get(&f.scope, "/mobile/usage/stats", &[]);
        assert_eq!(status, 200, "{body}");
        assert!(body.get("sessionsTotal").is_some());
        assert!(body.get("providers").is_some());
    }

    #[test]
    fn unknown_git_route_falls_through() {
        let f = fixture();
        let req = request("GET", "/mobile/git/nope");
        assert!(route_with_scope(&f.scope, &req).is_none());
    }

    // -----------------------------------------------------------------------
    // Security tests (Fix 5)
    // -----------------------------------------------------------------------

    /// Process-global hook tests must not run in parallel.

    /// Fix 5.1: writes to shell startup files, gitconfig, and LaunchAgents
    /// are rejected even though they are inside the scope's home directory.
    #[test]
    fn files_write_rejects_home_dotfiles_and_launch_agents() {
        let f = fixture();
        // The fixture home is NOT a project root, so these are doubly
        // rejected; the point is the sensitive paths never get written.
        for target in [
            f.home.join(".zshrc"),
            f.home.join(".bashrc"),
            f.home.join(".gitconfig"),
            f.home.join("Library/LaunchAgents/evil.plist"),
        ] {
            let content = base64::engine::general_purpose::STANDARD.encode("evil");
            let (status, body) = post(
                &f.scope,
                "/mobile/files/write",
                json!({
                    "path": target.to_string_lossy(),
                    "contentBase64": content,
                }),
            );
            assert_eq!(status, 403, "target {target:?}: {body}");
            assert!(!target.exists(), "sensitive target was written: {target:?}");
        }
    }

    /// Fix 5.1 (continued): dotfiles are rejected even *inside* a project root.
    #[test]
    fn files_write_rejects_dotfiles_inside_project_root() {
        let f = fixture();
        for target in [
            f.repo.join(".git").join("config"),
            f.repo.join(".git").join("hooks").join("pre-commit"),
            f.repo.join(".env"),
        ] {
            let content = base64::engine::general_purpose::STANDARD.encode("evil");
            let (status, _) = post(
                &f.scope,
                "/mobile/files/write",
                json!({
                    "path": target.to_string_lossy(),
                    "contentBase64": content,
                }),
            );
            assert_eq!(status, 403, "target {target:?}");
        }
        // The real .git/config must be untouched.
        let config = std::fs::read_to_string(f.repo.join(".git/config")).unwrap();
        assert!(!config.contains("evil"), "{config}");
    }

    /// Fix 5.2: a repo whose toplevel is outside the registered project roots
    /// is rejected, even when the requested path is inside a root.
    #[test]
    fn resolve_repo_rejects_toplevel_outside_project_roots() {
        let f = fixture();
        // A second repo under the fixture home: resolvable by the scope
        // (home is in scope) but NOT a registered project root.
        let other = f.home.join("other-repo");
        std::fs::create_dir_all(&other).unwrap();
        sh(&other, &["init", "-b", "main"]);
        sh(&other, &["config", "user.email", "test@example.com"]);
        sh(&other, &["config", "user.name", "Test"]);
        std::fs::write(other.join("x.txt"), "x\n").unwrap();
        sh(&other, &["add", "."]);
        sh(&other, &["commit", "-m", "init"]);

        let other_str = other.to_string_lossy().into_owned();
        let (status, body) = get(&f.scope, "/mobile/git/status", &[("path", &other_str)]);
        assert_eq!(status, 403, "{body}");
        assert!(body["error"]
            .as_str()
            .unwrap_or("")
            .contains("outside the registered project roots"));
    }

    /// Fix 5.3: `core.fsmonitor` from the repo's `.git/config` is not
    /// executed by the status handler.
    #[test]
    fn git_status_does_not_execute_fsmonitor() {
        let f = fixture();
        let marker = f._root.path().join("pwned-marker");
        // Plant a malicious fsmonitor in the repo config. Use printf to avoid
        // depending on shell quoting edge cases.
        sh(
            &f.repo,
            &[
                "config",
                "core.fsmonitor",
                &format!("touch {}", marker.to_string_lossy()),
            ],
        );
        let repo = f.repo.to_string_lossy().into_owned();
        let (status, body) = get(&f.scope, "/mobile/git/status", &[("path", &repo)]);
        assert_eq!(status, 200, "{body}");
        assert!(
            !marker.exists(),
            "fsmonitor was executed: marker {marker:?} exists"
        );
        // Clean up the planted config so other tests are unaffected.
        sh(&f.repo, &["config", "--unset", "core.fsmonitor"]);
    }
}
