//! Host git, file-write, and usage routes for the desktop/mobile clients.
//!
//! These back the Git/Files/Usage app panes (`clients/supercli-app`). Paths
//! are resolved through [`ResourceScope`] from `host_resources`, so a
//! Controller can only touch the registered project roots and the Host
//! user's home (minus Supercli's own storage and SSH material) — the same
//! scope as the existing file routes. Git itself runs via the `git` CLI
//! with `GIT_OPTIONAL_LOCKS=0`; mutating commands (`stage`, `commit`,
//! `fetch`, `pull`, `push`) additionally require the resolved path to be a
//! git work tree, and `commit` refuses an empty message.

use std::io;
use std::path::Path;
use std::process::Command;

use base64::Engine as _;
use serde_json::{json, Value};

use crate::controller_api::{ControllerRequest, ControllerResponse};
use crate::host_resources::{fail, Failure, ResourceScope};

fn git(repo: &Path, args: &[&str]) -> io::Result<std::process::Output> {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
}

fn git_ok(repo: &Path, args: &[&str], what: &str) -> Result<String, Failure> {
    let output = git(repo, args).map_err(|e| fail(500, format!("{what}: {e}")))?;
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

/// Resolve a Controller-supplied path and require it to be inside a git
/// work tree. Returns the repository root.
fn resolve_repo(scope: &ResourceScope, request: &ControllerRequest) -> Result<std::path::PathBuf, Failure> {
    let raw = request
        .body
        .get("path")
        .and_then(|v| v.as_str())
        .or_else(|| request.query.get("path").map(String::as_str))
        .unwrap_or("");
    let resolved = scope.resolve(raw)?;
    let output = git(resolved.display_path(), &["rev-parse", "--show-toplevel"])
        .map_err(|e| fail(500, format!("git status: {e}")))?;
    if !output.status.success() {
        return Err(fail(404, "path is not inside a git repository"));
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if root.is_empty() {
        return Err(fail(500, "git returned an empty repository root"));
    }
    Ok(std::path::PathBuf::from(root))
}

fn git_status(scope: &ResourceScope, request: &ControllerRequest) -> Result<Value, Failure> {
    let root = resolve_repo(scope, request)?;
    let branch = git_ok(&root, &["symbolic-ref", "--quiet", "--short", "HEAD"], "git branch")
        .ok()
        .and_then(|b| {
            let b = b.trim().to_owned();
            if b.is_empty() { None } else { Some(b) }
        })
        .or_else(|| {
            git_ok(&root, &["rev-parse", "--short", "HEAD"], "git rev-parse")
                .ok()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        });
    let (ahead, behind) = upstream_counts(&root);
    let porcelain = git_ok(&root, &["status", "--porcelain=v1", "-uall"], "git status")?;
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
    let upstream = match git(root, &["rev-parse", "--abbrev-ref", "@{upstream}"]) {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        _ => return (0, 0),
    };
    if upstream.is_empty() {
        return (0, 0);
    }
    let counts = match git(root, &["rev-list", "--left-right", "--count", "HEAD...@{upstream}", "--"])
    {
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
    let diff = git_ok(&root, &["diff", "HEAD", "--", file], "git diff")?;
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
    let log = git_ok(
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
        let (Some(sha), Some(author), Some(date), Some(message)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
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
        let path = file.as_str().ok_or_else(|| fail(400, "files must be strings"))?;
        if path.is_empty() || path.contains('\0') || path.starts_with('/') || path.contains("..") {
            return Err(fail(400, format!("invalid file path: {path}")));
        }
        out.push(path.to_owned());
    }
    Ok(out)
}

fn git_stage(scope: &ResourceScope, request: &ControllerRequest, unstage: bool) -> Result<Value, Failure> {
    let root = resolve_repo(scope, request)?;
    let files = body_files(request)?;
    let mut args: Vec<&str> = if unstage {
        vec!["restore", "--staged", "--"]
    } else {
        vec!["add", "--"]
    };
    args.extend(files.iter().map(String::as_str));
    let what = if unstage { "git unstage" } else { "git stage" };
    git_ok(&root, &args, what)?;
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
    git_ok(&root, &["commit", "-m", &message], "git commit")?;
    Ok(json!({ "ok": true }))
}

fn git_remote_op(scope: &ResourceScope, request: &ControllerRequest, op: &str) -> Result<Value, Failure> {
    let root = resolve_repo(scope, request)?;
    let args: &[&str] = match op {
        "fetch" => &["fetch", "--prune"],
        "pull" => &["pull", "--ff-only"],
        "push" => &["push"],
        _ => return Err(fail(400, "unknown git operation")),
    };
    // Remote ops can take a while; the HTTP layer already applies its own
    // timeout, so no extra timeout here.
    git_ok(&root, args, &format!("git {op}"))?;
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
    let resolved = scope.resolve(raw)?;
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
        for (name, dir) in [
            ("claude", ".claude/projects"),
            ("codex", ".codex/sessions"),
        ] {
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

    fn post(scope: &ResourceScope, path: &str, body: Value) -> (u16, Value) {
        let mut req = request("POST", path);
        req.body = body;
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
}
