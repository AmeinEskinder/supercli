//! Host filesystem operations for folder pickers and explicit file transfers.
//!
//! Paths are resolved on the authenticated user's Host, never on a Controller,
//! and only inside an explicit scope: the registered project roots plus the
//! user's home minus Supercli's own storage and SSH material. Every walk opens
//! one component at a time with `O_NOFOLLOW` from an opened root, so a
//! symlinked parent can never redirect a read outside the scope. Paired
//! Controllers are owner-equivalent today, so this is defense in depth rather
//! than a sandbox; it is also the seam where per-principal authorization for
//! shared workspaces tightens later.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use base64::Engine as _;
use serde_json::{json, Value};

use crate::controller_api::{ControllerRequest, ControllerResponse};
use crate::session_artifacts::secure_fs;

/// Folder entries per listing page.
pub const DIRECTORY_PAGE_ENTRIES: usize = 128;
/// Default bytes per file-read page; `limit` may raise it to the maximum.
pub const FILE_READ_DEFAULT_BYTES: u64 = 192 * 1024;
pub const FILE_READ_MAX_BYTES: u64 = 1024 * 1024;

pub(crate) type Failure = (u16, Value);

pub(crate) fn fail(status: u16, message: impl Into<String>) -> Failure {
    (status, json!({ "error": message.into() }))
}

/// The folders a Controller may browse, read from, and add as projects.
#[derive(Debug, Clone)]
pub struct ResourceScope {
    home: Option<PathBuf>,
    project_roots: Vec<PathBuf>,
    denied: Vec<PathBuf>,
}

impl ResourceScope {
    /// The live scope: every registered project root from `app-state.json`
    /// plus the Host user's home, minus Supercli's own storage and SSH material.
    pub fn from_host() -> Self {
        let project_roots = crate::app_state::load()
            .ok()
            .and_then(|state| state.get("projects").cloned())
            .and_then(|projects| {
                serde_json::from_value::<Vec<crate::state::Project>>(projects).ok()
            })
            .unwrap_or_default()
            .into_iter()
            .filter(|project| !project.is_folder)
            .map(|project| PathBuf::from(project.path))
            .filter(|path| path.is_absolute())
            .collect();
        Self::new(dirs::home_dir(), project_roots)
    }

    pub fn new(home: Option<PathBuf>, project_roots: Vec<PathBuf>) -> Self {
        let mut denied = vec![crate::app_paths::supercli_home()];
        if let Some(home) = &home {
            denied.push(home.join(".supercli"));
            denied.push(home.join(".ssh"));
        }
        Self {
            home,
            project_roots,
            denied,
        }
    }

    /// Returns true if `path` is inside one of the registered project roots.
    ///
    /// Used by security-sensitive routes (git ops, file writes) that must not
    /// operate on the bare home directory: a paired Controller could otherwise
    /// overwrite shell startup files (`~/.zshrc`), `~/.gitconfig`
    /// (`core.sshCommand`), or `~/Library/LaunchAgents/*.plist` for persistent
    /// code execution. Both sides are canonicalized best-effort so a symlinked
    /// project root still matches.
    pub(crate) fn is_inside_project_root(&self, path: &Path) -> bool {
        self.project_root_for(path).is_some()
    }

    /// Returns the registered project root that `path` lives under, if any.
    /// Canonicalizes best-effort so symlinked roots still match.
    pub(crate) fn project_root_for(&self, path: &Path) -> Option<PathBuf> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.project_roots
            .iter()
            .filter(|root| {
                let canonical_root = root.canonicalize().unwrap_or_else(|_| (*root).clone());
                canonical.starts_with(&canonical_root)
            })
            .max_by_key(|root| root.components().count())
            .cloned()
    }

    /// Lexically normalizes a Controller-supplied path and binds it to the
    /// scope root it lives under. No filesystem access happens here, so a
    /// denied path is refused before anything is opened.
    pub(crate) fn resolve(&self, raw: &str) -> Result<ResolvedPath, Failure> {
        if raw.len() > 16 * 1024 || raw.chars().any(char::is_control) {
            return Err(fail(400, "Invalid Host path"));
        }
        let home = || {
            self.home
                .clone()
                .ok_or_else(|| fail(500, "Host home is unavailable"))
        };
        let expanded = if raw.is_empty() || raw == "~" {
            home()?
        } else if let Some(relative) = raw.strip_prefix("~/") {
            home()?.join(relative)
        } else {
            PathBuf::from(raw)
        };
        if !expanded.is_absolute() {
            return Err(fail(400, "Use an absolute Host path or ~/"));
        }
        let mut normalized = PathBuf::from("/");
        for component in expanded.components() {
            match component {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(name) => normalized.push(name),
                Component::ParentDir => {
                    return Err(fail(400, "Host paths may not contain .."));
                }
                Component::Prefix(_) => return Err(fail(400, "Invalid Host path")),
            }
        }

        // The most specific registered project root wins over the home rule,
        // but never re-exposes a denied folder from above: a project at `~`
        // or `/` does not make ~/.ssh readable. A worktree registered under
        // ~/.supercli/worktrees is inside the denied prefix itself, so it
        // stays a project rather than Supercli storage.
        let project_root = self
            .project_roots
            .iter()
            .filter(|root| normalized.starts_with(root))
            .max_by_key(|root| root.components().count())
            .cloned();
        let denied_prefix = self
            .denied
            .iter()
            .find(|denied| normalized.starts_with(denied));
        let root = match project_root {
            Some(root) => {
                if denied_prefix.is_some_and(|denied| !root.starts_with(denied)) {
                    return Err(fail(
                        403,
                        "Supercli's own storage and SSH material are never shared with Controllers",
                    ));
                }
                root
            }
            None => {
                let home = self
                    .home
                    .as_ref()
                    .filter(|home| normalized.starts_with(home))
                    .ok_or_else(|| {
                        fail(
                            403,
                            "That path is outside the folders this Host shares with Controllers",
                        )
                    })?;
                if denied_prefix.is_some() {
                    return Err(fail(
                        403,
                        "Supercli's own storage and SSH material are never shared with Controllers",
                    ));
                }
                home.clone()
            }
        };
        let relative = normalized
            .strip_prefix(&root)
            .expect("root is a checked prefix")
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect();
        Ok(ResolvedPath {
            root,
            relative,
            display: normalized,
        })
    }
}

/// A Controller path bound to its scope root: `root` joined with `relative`
/// is `display`, and `relative` is walked component by component.
pub(crate) struct ResolvedPath {
    root: PathBuf,
    relative: Vec<String>,
    display: PathBuf,
}

/// One entry of a directory listing for the files pane.
#[derive(Debug, Clone)]
pub(crate) struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

impl ResolvedPath {
    /// The normalized absolute path, for display and for subprocess use
    /// (e.g. `git -C`).
    pub(crate) fn display_path(&self) -> &Path {
        &self.display
    }

    /// List files and directories (no-follow), sorted dirs-first then by name.
    pub(crate) fn list_file_entries(&self) -> Result<Vec<FileEntry>, Failure> {
        let dir = self.open_dir()?;
        let names = secure_fs::entry_names(&dir).map_err(walk_error)?;
        let mut entries = Vec::new();
        for name in names {
            let Ok(name) = String::from_utf8(name) else {
                continue;
            };
            if name == "." || name == ".." {
                continue;
            }
            let (is_dir, size) = match secure_fs::metadata_at(&dir, name.as_bytes()) {
                Ok(metadata) if metadata.directory => (true, 0),
                Ok(metadata) if metadata.regular_file => {
                    let size = secure_fs::open_regular_read_at(&dir, name.as_bytes())
                        .and_then(|f| f.metadata())
                        .map(|m| m.len())
                        .unwrap_or(0);
                    (false, size)
                }
                // Symlinks and specials are listed without a size; never followed.
                Ok(_) => (false, 0),
                Err(_) => continue,
            };
            entries.push(FileEntry { name, is_dir, size });
            if entries.len() > 2 * DIRECTORY_PAGE_ENTRIES {
                break;
            }
        }
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        entries.truncate(DIRECTORY_PAGE_ENTRIES);
        Ok(entries)
    }

    /// Atomically write bytes to this path (parent walked no-follow; the
    /// leaf itself is never a symlink — `atomic_write_regular_at` refuses
    /// non-regular targets).
    pub(crate) fn write_bytes(&self, bytes: &[u8]) -> Result<(), Failure> {
        let (parent, leaf) = self.open_parent()?;
        secure_fs::atomic_write_regular_at(&parent, leaf.as_bytes(), bytes)
            .map_err(walk_error)
    }

    fn open_root(&self) -> Result<File, Failure> {
        secure_fs::open_configured_root(&self.root).map_err(walk_error)
    }

    /// Opens the directory at this path, one no-follow component at a time.
    pub(crate) fn open_dir(&self) -> Result<File, Failure> {
        let mut dir = self.open_root()?;
        for component in &self.relative {
            dir = step_into(&dir, component)?;
        }
        Ok(dir)
    }

    /// Opens the parent directory no-follow and returns it with the leaf.
    pub(crate) fn open_parent(&self) -> Result<(File, &str), Failure> {
        let Some((leaf, parents)) = self.relative.split_last() else {
            return Err(fail(
                400,
                "Choose a path inside the folder, not the folder itself",
            ));
        };
        let mut dir = self.open_root()?;
        for component in parents {
            dir = step_into(&dir, component)?;
        }
        Ok((dir, leaf.as_str()))
    }

    /// The parent a picker may navigate to; `None` at the scope root.
    fn parent_in_scope(&self) -> Option<PathBuf> {
        if self.relative.is_empty() {
            None
        } else {
            self.display.parent().map(Path::to_path_buf)
        }
    }
}

/// One no-follow step of the walk. The lstat first gives a symlink its own
/// verdict on every platform: macOS reports `ENOTDIR` rather than `ELOOP`
/// for `O_DIRECTORY | O_NOFOLLOW` on a symlink, and the open that follows
/// still refuses to traverse it.
fn step_into(dir: &File, component: &str) -> Result<File, Failure> {
    match secure_fs::metadata_at(dir, component.as_bytes()) {
        Ok(metadata) if metadata.symlink => {
            return Err(fail(
                403,
                "Symlinked folders are not followed for Controllers",
            ));
        }
        Ok(_) => {}
        Err(error) => return Err(walk_error(error)),
    }
    secure_fs::open_dir_at(dir, component).map_err(walk_error)
}

fn walk_error(error: io::Error) -> Failure {
    if error.raw_os_error() == Some(libc::ELOOP) {
        return fail(403, "Symlinked folders are not followed for Controllers");
    }
    if error.raw_os_error() == Some(libc::ENOTDIR) {
        return fail(400, "That path is not a folder");
    }
    match error.kind() {
        io::ErrorKind::NotFound => fail(404, "No such folder or file on the Host"),
        io::ErrorKind::PermissionDenied => fail(403, "The Host user may not open that path"),
        io::ErrorKind::InvalidInput => fail(400, "Invalid Host path"),
        io::ErrorKind::AlreadyExists => fail(409, "That name already exists"),
        _ => fail(400, error.to_string()),
    }
}

fn list_directory(
    dir: &File,
    resolved: &ResolvedPath,
    after: &str,
    hidden: bool,
) -> Result<Value, Failure> {
    let names = secure_fs::entry_names(dir).map_err(walk_error)?;
    let mut folders = Vec::new();
    for name in names {
        let Ok(name) = String::from_utf8(name) else {
            continue;
        };
        if name == "." || name == ".." || name.as_str() <= after {
            continue;
        }
        if !hidden && name.starts_with('.') {
            continue;
        }
        // lstat: a symlink to a folder is not listed as a folder.
        match secure_fs::metadata_at(dir, name.as_bytes()) {
            Ok(metadata) if metadata.directory => {}
            _ => continue,
        }
        folders.push(name);
        // Keep only one page plus a sentinel even for huge directories so the
        // scan is bounded in memory and pagination stays lexically stable.
        if folders.len() > 2 * DIRECTORY_PAGE_ENTRIES {
            folders.sort();
            folders.truncate(DIRECTORY_PAGE_ENTRIES + 1);
        }
    }
    folders.sort();
    let more = folders.len() > DIRECTORY_PAGE_ENTRIES;
    folders.truncate(DIRECTORY_PAGE_ENTRIES);
    let next = if more { folders.last().cloned() } else { None };
    let path = &resolved.display;
    Ok(json!({
        "path": path,
        "parent": resolved.parent_in_scope(),
        "next": next,
        "entries": folders
            .iter()
            .map(|name| json!({ "name": name, "path": path.join(name) }))
            .collect::<Vec<_>>(),
    }))
}

fn create_directory(resolved: &ResolvedPath) -> Result<Value, Failure> {
    let (parent, leaf) = resolved.open_parent()?;
    let dir = secure_fs::open_or_create_dir_at(&parent, leaf).map_err(walk_error)?;
    list_directory(&dir, resolved, "", false)
}

/// Proves the folder exists inside the scope and returns the path to record.
fn project_path(scope: &ResourceScope, raw: &str) -> Result<String, Failure> {
    let resolved = scope.resolve(raw)?;
    resolved.open_dir()?;
    resolved
        .display
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| fail(400, "Folder path must be UTF-8"))
}

fn persist_project(path: &str) -> Result<Value, Failure> {
    crate::app_state::edit(|state| {
        let projects = state
            .entry("projects")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Invalid projects state")?;
        if let Some(project) = projects.iter().find(|project| {
            project["path"].as_str() == Some(path) && project["is_folder"].as_bool() != Some(true)
        }) {
            return Ok(project.clone());
        }
        let project = json!({
            "id": format!("host-{}", uuid::Uuid::new_v4()),
            "name": Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or("/"),
            "path": path,
            "sort_order": projects.len(),
            "is_folder": false,
        });
        projects.push(project.clone());
        Ok(project)
    })
    .map_err(|error| fail(500, error))
}

fn read_file(resolved: &ResolvedPath, offset: u64, limit: u64) -> Result<Value, Failure> {
    let (parent, leaf) = resolved.open_parent()?;
    // lstat before open: a FIFO or device opened read-only would block the
    // worker, and a symlinked leaf is refused rather than followed.
    match secure_fs::metadata_at(&parent, leaf.as_bytes()) {
        Ok(metadata) if metadata.regular_file => {}
        Ok(metadata) if metadata.symlink => {
            return Err(fail(
                403,
                "Symlinked files are not followed for Controllers",
            ));
        }
        Ok(_) => return Err(fail(400, "Choose a regular file")),
        Err(error) => return Err(walk_error(error)),
    }
    let mut file = secure_fs::open_regular_read_at(&parent, leaf.as_bytes()).map_err(|error| {
        if error.kind() == io::ErrorKind::InvalidData {
            fail(400, "Choose a regular file")
        } else {
            walk_error(error)
        }
    })?;
    let total_size = file
        .metadata()
        .map_err(|error| fail(500, error.to_string()))?
        .len();
    if offset > total_size {
        return Err(fail(416, "Offset exceeds file size"));
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| fail(500, error.to_string()))?;
    let mut bytes = Vec::new();
    file.take(limit.clamp(1, FILE_READ_MAX_BYTES))
        .read_to_end(&mut bytes)
        .map_err(|error| fail(500, error.to_string()))?;
    Ok(json!({
        "path": resolved.display,
        "offset": offset,
        "nextOffset": offset + bytes.len() as u64,
        "totalSize": total_size,
        "dataBase64": base64::engine::general_purpose::STANDARD.encode(bytes),
    }))
}

fn body_path(request: &ControllerRequest) -> Result<&str, Failure> {
    request.body["path"]
        .as_str()
        .ok_or_else(|| fail(400, "path required"))
}

pub fn route(request: &ControllerRequest) -> Option<ControllerResponse> {
    if let Some(response) = crate::host_git::route(request) {
        return Some(response);
    }
    let query = |key: &str| {
        request
            .query
            .get(key)
            .map(String::as_str)
            .unwrap_or_default()
    };
    let result = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/mobile/directories") => ResourceScope::from_host()
            .resolve(query("path"))
            .and_then(|resolved| {
                let dir = resolved.open_dir()?;
                list_directory(&dir, &resolved, query("after"), query("hidden") == "true")
            }),
        ("POST", "/mobile/directories/create") => body_path(request)
            .and_then(|raw| ResourceScope::from_host().resolve(raw))
            .and_then(|resolved| create_directory(&resolved)),
        ("POST", "/mobile/projects/add") => body_path(request)
            .and_then(|raw| project_path(&ResourceScope::from_host(), raw))
            .and_then(|path| persist_project(&path)),
        ("GET", "/mobile/files/read") => {
            let offset = query("offset")
                .parse()
                .map_err(|_| fail(400, "offset required"));
            let limit = match query("limit") {
                "" => Ok(FILE_READ_DEFAULT_BYTES),
                raw => raw.parse().map_err(|_| fail(400, "invalid limit")),
            };
            offset.and_then(|offset| {
                let limit = limit?;
                let resolved = ResourceScope::from_host().resolve(query("path"))?;
                read_file(&resolved, offset, limit)
            })
        }
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    struct Fixture {
        _root: tempfile::TempDir,
        scope: ResourceScope,
        home: PathBuf,
        project: PathBuf,
        outside: PathBuf,
    }

    fn fixture() -> Fixture {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let project = root.path().join("proj");
        let outside = root.path().join("outside");
        for dir in [
            home.join("docs"),
            home.join(".supercli"),
            home.join(".ssh"),
            project.join("src"),
            outside.clone(),
        ] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(home.join("docs/notes.txt"), b"hello notes").unwrap();
        std::fs::write(home.join(".supercli/app-state.json"), b"{}").unwrap();
        std::fs::write(home.join(".ssh/id_ed25519"), b"secret").unwrap();
        std::fs::write(project.join("src/main.rs"), b"fn main() {}").unwrap();
        std::fs::write(outside.join("secret.txt"), b"never").unwrap();
        std::os::unix::fs::symlink(&outside, home.join("link")).unwrap();
        let scope = ResourceScope::new(Some(home.clone()), vec![project.clone()]);
        Fixture {
            _root: root,
            scope,
            home,
            project,
            outside,
        }
    }

    fn read(scope: &ResourceScope, raw: &str) -> Result<Value, Failure> {
        let resolved = scope.resolve(raw)?;
        read_file(&resolved, 0, FILE_READ_DEFAULT_BYTES)
    }

    fn status(result: Result<Value, Failure>) -> u16 {
        result.map(|_| 200).unwrap_or_else(|(status, _)| status)
    }

    #[test]
    fn supercli_storage_and_ssh_material_are_denied_under_home() {
        let fixture = fixture();
        let supercli = fixture.home.join(".supercli/app-state.json");
        assert_eq!(
            status(read(&fixture.scope, supercli.to_str().unwrap())),
            403
        );
        assert_eq!(status(read(&fixture.scope, "~/.ssh/id_ed25519")), 403);
        let listing = fixture
            .scope
            .resolve("~/.supercli")
            .and_then(|resolved| resolved.open_dir().map(|_| json!(null)));
        assert_eq!(status(listing), 403);
    }

    #[test]
    fn paths_outside_the_scope_are_denied_without_touching_disk() {
        let fixture = fixture();
        let secret = fixture.outside.join("secret.txt");
        assert_eq!(status(read(&fixture.scope, secret.to_str().unwrap())), 403);
        assert_eq!(status(fixture.scope.resolve("/").map(|_| json!(null))), 403);
        assert_eq!(
            status(fixture.scope.resolve("/etc/passwd").map(|_| json!(null))),
            403
        );
        assert_eq!(
            status(fixture.scope.resolve("relative/path").map(|_| json!(null))),
            400
        );
    }

    #[test]
    fn symlinked_parents_inside_the_scope_are_not_followed() {
        let fixture = fixture();
        let through_link = fixture.home.join("link/secret.txt");
        assert_eq!(
            status(read(&fixture.scope, through_link.to_str().unwrap())),
            403
        );
        let listing = fixture
            .scope
            .resolve(fixture.home.join("link").to_str().unwrap())
            .and_then(|resolved| resolved.open_dir().map(|_| json!(null)));
        assert_eq!(status(listing), 403);
        // The symlink is not offered as a folder either.
        let dir = fixture.scope.resolve("~").unwrap();
        let page = list_directory(&dir.open_dir().unwrap(), &dir, "", false).unwrap();
        let names: Vec<_> = page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["name"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(names, vec!["docs".to_owned()]);
        assert!(
            page["parent"].is_null(),
            "the scope root has no parent to climb to"
        );
    }

    #[test]
    fn a_project_root_above_home_does_not_re_expose_denied_folders() {
        let fixture = fixture();
        // Registering the home itself (or any parent of it) as a project
        // must not turn ~/.ssh or Supercli's storage into project files.
        let parent = fixture.home.parent().unwrap().to_path_buf();
        let scope = ResourceScope::new(
            Some(fixture.home.clone()),
            vec![parent, fixture.home.clone()],
        );
        assert_eq!(status(read(&scope, "~/.ssh/id_ed25519")), 403);
        let supercli = fixture.home.join(".supercli/app-state.json");
        assert_eq!(status(read(&scope, supercli.to_str().unwrap())), 403);
        // Ordinary home files stay readable through the project root.
        assert_eq!(status(read(&scope, "~/docs/notes.txt")), 200);
        // A worktree registered inside ~/.supercli is a project, not storage.
        let worktree = fixture.home.join(".supercli/worktrees/feature");
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(worktree.join("README.md"), b"wt").unwrap();
        let scope = ResourceScope::new(Some(fixture.home.clone()), vec![worktree.clone()]);
        assert_eq!(
            status(read(&scope, worktree.join("README.md").to_str().unwrap())),
            200
        );
        assert_eq!(status(read(&scope, supercli.to_str().unwrap())), 403);
    }

    #[test]
    fn dot_dot_is_rejected_before_resolution() {
        let fixture = fixture();
        let sneaky = fixture.home.join("docs/../.ssh/id_ed25519");
        assert_eq!(status(read(&fixture.scope, sneaky.to_str().unwrap())), 400);
    }

    #[test]
    fn registered_project_roots_are_readable_outside_home() {
        let fixture = fixture();
        let main = fixture.project.join("src/main.rs");
        let page = read(&fixture.scope, main.to_str().unwrap()).unwrap();
        assert_eq!(page["totalSize"], 12);
        assert_eq!(page["dataBase64"], "Zm4gbWFpbigpIHt9");
        let resolved = fixture.scope.resolve(main.to_str().unwrap()).unwrap();
        let tail = read_file(&resolved, 10, 2).unwrap();
        assert_eq!(tail["dataBase64"], "e30=");
        assert_eq!(tail["nextOffset"], 12);
        assert_eq!(
            status(read_file(&resolved, 13, 8).map(|_| json!(null))),
            416
        );
        let dir_as_file = fixture
            .scope
            .resolve(fixture.project.join("src").to_str().unwrap())
            .unwrap();
        assert_eq!(
            status(read_file(&dir_as_file, 0, 8).map(|_| json!(null))),
            400
        );
        assert_eq!(
            project_path(
                &fixture.scope,
                fixture.project.join("src").to_str().unwrap()
            )
            .unwrap(),
            fixture.project.join("src").to_str().unwrap()
        );
        assert_eq!(
            status(
                project_path(&fixture.scope, fixture.outside.to_str().unwrap())
                    .map(|_| json!(null))
            ),
            403
        );
    }

    #[test]
    fn folder_pages_are_bounded_and_do_not_skip_entries() {
        let fixture = fixture();
        let many = fixture.home.join("many");
        for index in 0..300 {
            std::fs::create_dir_all(many.join(format!("folder-{index:03}"))).unwrap();
        }
        std::fs::write(many.join("document.pdf"), b"file").unwrap();
        let resolved = fixture.scope.resolve(many.to_str().unwrap()).unwrap();
        let dir = resolved.open_dir().unwrap();
        let mut after = String::new();
        let mut count = 0;
        loop {
            let page = list_directory(&dir, &resolved, &after, false).unwrap();
            count += page["entries"].as_array().unwrap().len();
            match page["next"].as_str() {
                Some(next) => after = next.to_owned(),
                None => break,
            }
        }
        assert_eq!(count, 300);
    }

    #[test]
    fn folders_are_created_only_inside_the_scope() {
        let fixture = fixture();
        let inside = fixture.home.join("docs/new");
        let created = fixture
            .scope
            .resolve(inside.to_str().unwrap())
            .and_then(|resolved| create_directory(&resolved))
            .unwrap();
        assert_eq!(created["path"], inside.to_str().unwrap());
        assert!(inside.is_dir());
        let outside = fixture.outside.join("new");
        let denied = fixture
            .scope
            .resolve(outside.to_str().unwrap())
            .and_then(|resolved| create_directory(&resolved));
        assert_eq!(status(denied), 403);
        assert!(!outside.exists());
        let through_link = fixture.home.join("link/new");
        let denied = fixture
            .scope
            .resolve(through_link.to_str().unwrap())
            .and_then(|resolved| create_directory(&resolved));
        assert_eq!(status(denied), 403);
        assert!(!fixture.outside.join("new").exists());
    }
}
