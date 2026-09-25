//! `unpeel backup` / `unpeel restore` — consistent, verifiable snapshots of a
//! workspace home (`UNPEEL_HOME`).
//!
//! A backup is a single `.tar` archive plus a manifest (`manifest.json`,
//! the last entry) carrying the SHA-256 of every archived file. Restore
//! verifies every hash and every staged review hash chain before touching
//! the destination, refuses while a Host holds the workspace serve lease,
//! and installs each file with tmp-file + rename.
//!
//! Consistency model (no copying of live files):
//! * `app-sessions/*/action-reviews.jsonl` is read while holding that
//!   session's review-log [`LogLock`][crate::action_reviews] — the same
//!   lock the write-ahead path takes — so the snapshot is a whole prefix
//!   of the chain, never a torn append.
//! * `schedule-leases.db` is snapshotted through the SQLite online-backup
//!   API (`rusqlite::backup`), never a file copy of the live database.
//! * Everything else the Host writes goes through atomic tmp-file + rename
//!   (`app-state.json`, manifests, `launch.json`), so a plain read is
//!   already atomic. `output.bin` is an append-only journal: it is copied
//!   as-is and the manifest pins exactly the bytes that were captured.
//!
//! Deliberately excluded: `*.lock` files (lock state is meaningless across
//! processes), `*.sock` sockets, `serve.lock`/`serve.json` (transient Host
//! liveness), and `worktrees/` (the user's project checkouts, not Unpeel
//! state).
//!
//! Trust model: the manifest is the trusted reference inside the archive.
//! Byte-level tampering of any archived file is detected (SHA-256 +
//! size). Rewriting the manifest to match tampered files is out of scope —
//! same as any unauthenticated backup. Review logs additionally carry
//! their own tamper-evident hash chain, which restore re-verifies.

use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::action_reviews::{verify_review_bytes, verify_review_chain, LogLock, REVIEWS_FILE};
use crate::schedule_leases::leases_db_path;

/// Archive format version written into the manifest.
pub const BACKUP_FORMAT_VERSION: u32 = 1;
/// Name of the manifest entry inside the archive (always last).
pub const MANIFEST_NAME: &str = "manifest.json";
/// Largest single file accepted on restore (2 GiB). Session journals are
/// retention-bounded; anything bigger is treated as hostile or corrupt.
const MAX_RESTORE_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Per-session files captured under `app-sessions/<id>/`.
const SESSION_FILES: [&str; 6] = [
    "manifest.json",
    "launch.json",
    REVIEWS_FILE,
    "output.bin",
    "output-retention.json",
    "title.json",
];
/// Top-level home files captured (when present).
/// S2: grants.json is sharded from app-state.json and must be backed up.
/// Phase 13 v2: grant-audit.jsonl must be backed up alongside grants.json;
/// a restore with grants but no audit entries would quarantine all grants
/// on startup (grant without chain entry = tamper-evidence violation).
const HOME_FILES: [&str; 7] = [
    "app-state.json",
    "grants.json",
    "grant-audit.jsonl",
    "activity-state.json",
    "session-order.json",
    "schedules.json",
    "pane-layouts.json",
];
/// Pairing state: the paired-device list (device tokens) and the machine
/// identity. Without these a restore loses every paired Controller.
const MOBILE_FILES: [&str; 2] = ["mobile/devices.json", "mobile/mac-id"];
/// Per-session user files (gallery uploads, published images, …) live
/// under `app-sessions/<id>/artifacts/` and are walked recursively.
/// The lease database, snapshotted via the SQLite online-backup API.
const LEASES_DB_NAME: &str = "schedule-leases.db";

/// One archived file's identity in the manifest.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ManifestEntry {
    /// Archive-relative path with `/` separators, e.g.
    /// `app-sessions/abc/manifest.json`.
    pub path: String,
    /// Lowercase hex SHA-256 of the archived bytes.
    pub sha256: String,
    /// Byte length of the archived file.
    pub size_bytes: u64,
}

/// The backup manifest: the trust root for restore verification.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupManifest {
    pub format_version: u32,
    pub created_unix_ms: u64,
    pub unpeel_version: String,
    pub files: Vec<ManifestEntry>,
}

/// Outcome of [`create_backup`].
#[derive(Debug)]
pub struct BackupReport {
    pub archive: PathBuf,
    pub files: usize,
    pub bytes: u64,
    pub sessions: usize,
    /// Sessions whose review log failed chain verification at backup
    /// time. The backup still completes — a damaged live log must not
    /// block the safety tool — but the operator is told.
    pub broken_chains: Vec<String>,
}

/// Outcome of [`restore_backup`].
#[derive(Debug)]
pub struct RestoreReport {
    pub home: PathBuf,
    pub files: usize,
    pub sessions: usize,
    pub chains_verified: usize,
}

/// Failure modes of backup/restore. Every variant renders as a
/// human-readable message; the CLI maps them to exit codes.
#[derive(Debug)]
pub enum BackupError {
    Io(String),
    /// The destination home holds existing Unpeel state and `--force` was
    /// not given.
    HomeNotEmpty(PathBuf),
    /// A Host currently holds this workspace's serve lease.
    HostRunning(PathBuf),
    /// Archive structure problem (bad tar, absolute/`..` path, symlink,
    /// oversize entry, missing or extra manifest entry…).
    ArchiveInvalid(String),
    /// A file's bytes do not match the manifest.
    HashMismatch {
        path: String,
    },
    /// A restored review log's hash chain does not verify.
    ChainInvalid {
        session: String,
        reason: String,
    },
    UnsupportedFormat(u32),
}

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackupError::Io(msg) => write!(f, "{msg}"),
            BackupError::HomeNotEmpty(home) => write!(
                f,
                "refusing to restore over existing Unpeel state in {} (use --force to overwrite)",
                home.display()
            ),
            BackupError::HostRunning(home) => write!(
                f,
                "refusing to restore while a Host is serving {} (stop it first)",
                home.display()
            ),
            BackupError::ArchiveInvalid(msg) => write!(f, "invalid backup archive: {msg}"),
            BackupError::HashMismatch { path } => {
                write!(
                    f,
                    "backup verification failed: {path} does not match the manifest (tampered?)"
                )
            }
            BackupError::ChainInvalid { session, reason } => write!(
                f,
                "restored review chain for session {session} does not verify: {reason}"
            ),
            BackupError::UnsupportedFormat(v) => {
                write!(f, "unsupported backup format version {v}")
            }
        }
    }
}

impl std::error::Error for BackupError {}

impl From<std::io::Error> for BackupError {
    fn from(e: std::io::Error) -> Self {
        BackupError::Io(e.to_string())
    }
}

fn fail<T>(msg: impl Into<String>) -> Result<T, BackupError> {
    Err(BackupError::Io(msg.into()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// True while another process holds this workspace's serve lease.
/// Mirrors `unpeel_serve::driver::is_running_at` (which lives downstream
/// of this crate): a non-blocking exclusive `flock` probe on
/// `<home>/serve.lock`. A stale lockfile never counts — only the kernel
/// lock state matters.
pub fn host_lock_held(home: &Path) -> bool {
    let path = home.join("serve.lock");
    let Ok(file) = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
    else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            let _ = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
            false
        } else {
            std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock
        }
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        // Conservative on non-Unix: a present lockfile blocks restore.
        true
    }
}

/// Read a session's review log while holding its [`LogLock`], so the
/// bytes are a whole chain prefix even under concurrent writers.
/// Returns `None` when the session has no review log yet.
fn snapshot_review_log(session_dir: &Path) -> Result<Option<Vec<u8>>, BackupError> {
    let _lock = LogLock::acquire(session_dir)
        .map_err(|e| BackupError::Io(format!("review-log lock: {e}")))?;
    let path = session_dir.join(REVIEWS_FILE);
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => fail(format!("read {}: {e}", path.display())),
    }
}

/// Snapshot the live lease database through the SQLite online-backup API
/// into `dest`. Never a file copy: the source may be under active write.
fn snapshot_leases_db(home: &Path, dest: &Path) -> Result<bool, BackupError> {
    let src_path = leases_db_path(home);
    if !src_path.exists() {
        return Ok(false);
    }
    let src = rusqlite::Connection::open(&src_path)
        .map_err(|e| BackupError::Io(format!("open lease db {}: {e}", src_path.display())))?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let _ = fs::remove_file(dest);
    let mut dst = rusqlite::Connection::open(dest)
        .map_err(|e| BackupError::Io(format!("open backup staging {}: {e}", dest.display())))?;
    let backup = rusqlite::backup::Backup::new(&src, &mut dst)
        .map_err(|e| BackupError::Io(format!("lease db backup init: {e}")))?;
    backup
        .run_to_completion(100, std::time::Duration::from_millis(50), None)
        .map_err(|e| BackupError::Io(format!("lease db backup step: {e}")))?;
    drop(backup);
    drop(src);
    drop(dst);
    Ok(true)
}

/// Collect the (archive-relative path, source bytes) pairs for a backup.
/// Review logs go through the [`LogLock`]; the lease DB through the
/// online-backup API; everything else is an atomic-rename file and is
/// read directly.
fn collect_snapshot(home: &Path, staging: &Path) -> Result<Vec<(String, Vec<u8>)>, BackupError> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut push_file = |rel: String, bytes: Vec<u8>| {
        files.push((rel, bytes));
    };

    for name in HOME_FILES {
        let path = home.join(name);
        if path.is_file() {
            let bytes = fs::read(&path)
                .map_err(|e| BackupError::Io(format!("read {}: {e}", path.display())))?;
            push_file(name.to_string(), bytes);
        }
    }

    // Pairing state (mobile/devices.json, mobile/mac-id).
    for name in MOBILE_FILES {
        let path = home.join(name);
        if path.is_file() {
            let bytes = fs::read(&path)
                .map_err(|e| BackupError::Io(format!("read {}: {e}", path.display())))?;
            push_file(name.to_string(), bytes);
        }
    }

    // Lease DB via the online-backup API into a staging file.
    let db_stage = staging.join(LEASES_DB_NAME);
    if snapshot_leases_db(home, &db_stage)? {
        let bytes = fs::read(&db_stage)
            .map_err(|e| BackupError::Io(format!("read staged lease db: {e}")))?;
        push_file(LEASES_DB_NAME.to_string(), bytes);
        let _ = fs::remove_file(&db_stage);
    }

    // Sessions.
    let sessions_root = home.join("app-sessions");
    if sessions_root.is_dir() {
        let mut ids: Vec<String> = Vec::new();
        for entry in fs::read_dir(&sessions_root)
            .map_err(|e| BackupError::Io(format!("read {}: {e}", sessions_root.display())))?
        {
            let entry = entry.map_err(|e| BackupError::Io(format!("read session entry: {e}")))?;
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if let Some(id) = entry.file_name().to_str() {
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        for id in ids {
            let dir = sessions_root.join(&id);
            for name in SESSION_FILES {
                if name == REVIEWS_FILE {
                    if let Some(bytes) = snapshot_review_log(&dir)? {
                        push_file(format!("app-sessions/{id}/{name}"), bytes);
                    }
                    continue;
                }
                let path = dir.join(name);
                if path.is_file() {
                    let bytes = fs::read(&path)
                        .map_err(|e| BackupError::Io(format!("read {}: {e}", path.display())))?;
                    push_file(format!("app-sessions/{id}/{name}"), bytes);
                }
            }
            // Session user files: app-sessions/<id>/artifacts/**.
            let artifacts = dir.join("artifacts");
            if artifacts.is_dir() {
                let mut stack = vec![artifacts.clone()];
                while let Some(d) = stack.pop() {
                    let entries = fs::read_dir(&d)
                        .map_err(|e| BackupError::Io(format!("read {}: {e}", d.display())))?;
                    for entry in entries {
                        let entry = entry
                            .map_err(|e| BackupError::Io(format!("read artifact entry: {e}")))?;
                        // Never follow symlinks: a link inside artifacts
                        // must not pull outside files into the backup.
                        let ft = entry
                            .file_type()
                            .map_err(|e| BackupError::Io(format!("stat artifact entry: {e}")))?;
                        let p = entry.path();
                        if ft.is_dir() {
                            stack.push(p);
                        } else if ft.is_file() {
                            let rel = p.strip_prefix(&dir).map_err(|e| {
                                BackupError::Io(format!("artifact path prefix: {e}"))
                            })?;
                            let rel = validate_archive_path(rel)?;
                            let bytes = fs::read(&p).map_err(|e| {
                                BackupError::Io(format!("read {}: {e}", p.display()))
                            })?;
                            push_file(format!("app-sessions/{id}/{rel}"), bytes);
                        }
                    }
                }
            }
        }
    }
    Ok(files)
}

/// Write `files` plus the manifest into a `.tar` archive at `dest`.
fn write_archive(dest: &Path, files: &[(String, Vec<u8>)]) -> Result<BackupManifest, BackupError> {
    let out = fs::File::create(dest)
        .map_err(|e| BackupError::Io(format!("create {}: {e}", dest.display())))?;
    let mut builder = tar::Builder::new(out);
    let mut entries = Vec::with_capacity(files.len());
    for (rel, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_mtime(now_ms() / 1000);
        header.set_cksum();
        builder
            .append_data(&mut header, rel, &bytes[..])
            .map_err(|e| BackupError::Io(format!("tar append {rel}: {e}")))?;
        entries.push(ManifestEntry {
            path: rel.clone(),
            sha256: hex_sha256(bytes),
            size_bytes: bytes.len() as u64,
        });
    }
    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        created_unix_ms: now_ms(),
        unpeel_version: env!("CARGO_PKG_VERSION").to_string(),
        files: entries,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| BackupError::Io(format!("manifest: {e}")))?;
    let mut header = tar::Header::new_gnu();
    header.set_size(manifest_bytes.len() as u64);
    header.set_mode(0o600);
    header.set_mtime(now_ms() / 1000);
    header.set_cksum();
    builder
        .append_data(&mut header, MANIFEST_NAME, &manifest_bytes[..])
        .map_err(|e| BackupError::Io(format!("tar append {MANIFEST_NAME}: {e}")))?;
    builder
        .into_inner()
        .map_err(|e| BackupError::Io(format!("tar finish: {e}")))?;
    Ok(manifest)
}

/// Create a consistent backup of `home` as a `.tar` archive at `dest`.
/// The destination's parent directory is created; an existing `dest` is
/// overwritten. Refuses when `dest` would land inside `home`: the archive
/// is written after the snapshot, so run N would silently swallow run
/// N-1's archive on the next backup.
pub fn create_backup(home: &Path, dest: &Path) -> Result<BackupReport, BackupError> {
    if !home.is_dir() {
        return fail(format!("home {} is not a directory", home.display()));
    }
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
        // Compare canonical paths; a missing dest canonicalizes to its
        // parent, which is enough to detect "inside home".
        let home_canon = fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
        let dest_canon = if dest.exists() {
            fs::canonicalize(dest).unwrap_or_else(|_| dest.to_path_buf())
        } else {
            fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf())
        };
        if dest_canon.starts_with(&home_canon) {
            return fail(format!(
                "refusing to write the backup archive inside the backed-up home {} (choose --to outside it)",
                home.display()
            ));
        }
    }
    // Staging lives beside the archive (same filesystem), never inside
    // the backed-up home.
    let staging = dest.with_extension("staging-tmp");
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    let result = (|| -> Result<BackupReport, BackupError> {
        let files = collect_snapshot(home, &staging)?;
        let sessions = files
            .iter()
            .filter_map(|(rel, _)| {
                rel.strip_prefix("app-sessions/")
                    .and_then(|rest| rest.split('/').next())
            })
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let bytes: u64 = files.iter().map(|(_, b)| b.len() as u64).sum();
        let manifest = write_archive(dest, &files)?;
        // Chain health is advisory at backup time: a damaged log must not
        // block the safety tool, but the operator must know. Verify the
        // captured snapshot bytes, not the live log — re-reading the live
        // file races with concurrent writers and could report a torn
        // mid-append read as a broken chain.
        let mut broken_chains = Vec::new();
        for (rel, bytes) in &files {
            if rel.ends_with(&format!("/{REVIEWS_FILE}")) {
                if let Some(rest) = rel.strip_prefix("app-sessions/") {
                    if let Some(id) = rest.split('/').next() {
                        if let Err(e) = verify_review_bytes(bytes, rel) {
                            broken_chains.push(format!("{id}: {e}"));
                        }
                    }
                }
            }
        }
        Ok(BackupReport {
            archive: dest.to_path_buf(),
            files: manifest.files.len(),
            bytes,
            sessions,
            broken_chains,
        })
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

/// Validate one archive path from the tar: relative, no `..`, no
/// absolute path, no symlink/hardlink.
fn validate_archive_path(path: &Path) -> Result<String, BackupError> {
    if path.is_absolute() {
        return Err(BackupError::ArchiveInvalid(format!(
            "absolute path {}",
            path.display()
        )));
    }
    let mut rel = String::new();
    for comp in path.components() {
        match comp {
            Component::Normal(part) => {
                let s = part.to_str().ok_or_else(|| {
                    BackupError::ArchiveInvalid("non-UTF8 path in archive".to_string())
                })?;
                if !rel.is_empty() {
                    rel.push('/');
                }
                rel.push_str(s);
            }
            _ => {
                return Err(BackupError::ArchiveInvalid(format!(
                    "unsafe path {} in archive",
                    path.display()
                )))
            }
        }
    }
    if rel.is_empty() {
        return Err(BackupError::ArchiveInvalid(
            "empty path in archive".to_string(),
        ));
    }
    Ok(rel)
}

/// Extract `archive` into `staging`, returning the parsed manifest.
/// Every path is validated; symlinks, hardlinks, absolute paths, `..`,
/// and oversize entries are rejected before any byte is written.
fn extract_archive(archive: &Path, staging: &Path) -> Result<BackupManifest, BackupError> {
    let file = fs::File::open(archive)
        .map_err(|e| BackupError::Io(format!("open {}: {e}", archive.display())))?;
    let mut tar = tar::Archive::new(file);
    let mut manifest: Option<BackupManifest> = None;
    let entries = tar
        .entries()
        .map_err(|e| BackupError::ArchiveInvalid(format!("read tar: {e}")))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|e| BackupError::ArchiveInvalid(format!("tar entry: {e}")))?;
        let kind = entry.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            return Err(BackupError::ArchiveInvalid(
                "archive contains a symlink/hardlink".to_string(),
            ));
        }
        if !kind.is_file() {
            continue;
        }
        let size = entry
            .header()
            .size()
            .map_err(|e| BackupError::ArchiveInvalid(format!("tar header size: {e}")))?;
        if size > MAX_RESTORE_FILE_BYTES {
            return Err(BackupError::ArchiveInvalid(format!(
                "entry {} exceeds the {}-byte restore limit",
                entry
                    .path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                MAX_RESTORE_FILE_BYTES
            )));
        }
        let raw_path = entry
            .path()
            .map_err(|e| BackupError::ArchiveInvalid(format!("tar path: {e}")))?;
        let rel = validate_archive_path(&raw_path)?;
        if rel == MANIFEST_NAME {
            if manifest.is_some() {
                return Err(BackupError::ArchiveInvalid(
                    "duplicate manifest in archive".to_string(),
                ));
            }
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| BackupError::ArchiveInvalid(format!("read manifest: {e}")))?;
            manifest = Some(
                serde_json::from_slice(&bytes)
                    .map_err(|e| BackupError::ArchiveInvalid(format!("parse manifest: {e}")))?,
            );
            continue;
        }
        let dest = staging.join(&rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&dest)
            .map_err(|e| BackupError::Io(format!("stage {}: {e}", dest.display())))?;
        std::io::copy(&mut entry, &mut out)?;
    }
    manifest.ok_or_else(|| BackupError::ArchiveInvalid("archive has no manifest".to_string()))
}

/// Verify every staged file against the manifest (SHA-256 + size).
/// Missing files, extra files, and hash mismatches all fail.
fn verify_staged_manifest(staging: &Path, manifest: &BackupManifest) -> Result<(), BackupError> {
    if manifest.format_version != BACKUP_FORMAT_VERSION {
        return Err(BackupError::UnsupportedFormat(manifest.format_version));
    }
    let mut seen = std::collections::BTreeSet::new();
    for entry in &manifest.files {
        let rel_path = Path::new(&entry.path);
        let rel = validate_archive_path(rel_path)?;
        if !seen.insert(rel.clone()) {
            return Err(BackupError::ArchiveInvalid(format!(
                "duplicate manifest entry {rel}"
            )));
        }
        let staged = staging.join(&rel);
        let bytes =
            fs::read(&staged).map_err(|_| BackupError::HashMismatch { path: rel.clone() })?;
        if bytes.len() as u64 != entry.size_bytes || hex_sha256(&bytes) != entry.sha256 {
            return Err(BackupError::HashMismatch { path: rel });
        }
    }
    // No extra files beyond the manifest.
    let mut extra = Vec::new();
    let mut stack = vec![staging.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).map_err(|e| BackupError::Io(e.to_string()))? {
            let e = e.map_err(|e| BackupError::Io(e.to_string()))?;
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(rel) = p.strip_prefix(staging) {
                let rel = validate_archive_path(rel)?;
                if !seen.contains(&rel) {
                    extra.push(rel);
                }
            }
        }
    }
    if !extra.is_empty() {
        return Err(BackupError::ArchiveInvalid(format!(
            "archive contains files outside the manifest: {}",
            extra.join(", ")
        )));
    }
    Ok(())
}

/// Known Unpeel state files: restore refuses to overwrite these unless
/// `--force` is given.
fn home_has_state(home: &Path) -> bool {
    const MARKERS: [&str; 4] = [
        "app-state.json",
        "app-sessions",
        LEASES_DB_NAME,
        "mobile/devices.json",
    ];
    MARKERS.iter().any(|m| home.join(m).exists())
}

/// Restore `archive` into `home`.
///
/// * Refuses while a Host holds the workspace serve lease.
/// * Verifies every file against the manifest before installing.
/// * Verifies every staged review hash chain before installing, so a
///   chain failure can never leave a partially restored home.
/// * Refuses to overwrite existing Unpeel state unless `force`.
/// * Installs each file with tmp-file + rename (atomic per file) and
///   mode 0600 — restored state can carry device tokens.
pub fn restore_backup(
    archive: &Path,
    home: &Path,
    force: bool,
) -> Result<RestoreReport, BackupError> {
    if host_lock_held(home) {
        return Err(BackupError::HostRunning(home.to_path_buf()));
    }
    if !force && home_has_state(home) {
        return Err(BackupError::HomeNotEmpty(home.to_path_buf()));
    }
    let staging = archive.with_extension("restore-tmp");
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    let result = (|| -> Result<RestoreReport, BackupError> {
        let manifest = extract_archive(archive, &staging)?;
        verify_staged_manifest(&staging, &manifest)?;
        // Verify every staged review chain BEFORE installing anything:
        // the manifest proves the bytes are the backed-up ones; the
        // chain proves they are untampered history. A failure here
        // aborts with the destination untouched.
        let mut sessions = std::collections::BTreeSet::new();
        let mut chained = std::collections::BTreeSet::new();
        for entry in &manifest.files {
            if let Some(rest) = entry.path.strip_prefix("app-sessions/") {
                if let Some(id) = rest.split('/').next() {
                    sessions.insert(id.to_string());
                    if rest.ends_with(&format!("/{REVIEWS_FILE}")) {
                        chained.insert(id.to_string());
                    }
                }
            }
        }
        for id in &chained {
            let dir = staging.join("app-sessions").join(id);
            verify_review_chain(&dir).map_err(|e| BackupError::ChainInvalid {
                session: id.clone(),
                reason: e.to_string(),
            })?;
        }
        let chains_verified = chained.len();
        // Phase 13 v2: Verify the grant audit chain in the restored archive.
        // A tampered grant-audit.jsonl must fail the restore.
        {
            let audit_path = staging.join("grant-audit.jsonl");
            if audit_path.exists() {
                // Temporarily set UNPEEL_HOME to staging for verification.
                // (verify_grant_audit uses audit_path() which reads UNPEEL_HOME.)
                let old_home = std::env::var("UNPEEL_HOME").ok();
                std::env::set_var("UNPEEL_HOME", &staging);
                let result = crate::grant_audit::verify_grant_audit();
                if let Some(old) = old_home {
                    std::env::set_var("UNPEEL_HOME", old);
                } else {
                    std::env::remove_var("UNPEEL_HOME");
                }
                result.map_err(|e| BackupError::ChainInvalid {
                    session: "grant-audit".to_string(),
                    reason: e,
                })?;
            }
        }
        // Install: tmp-file + rename per file, so a crash or IO error
        // leaves old-or-new, never a torn file. Restored state can carry
        // device tokens, so everything lands 0600 (the home itself is
        // expected to be 0700, enforced by ensure_unpeel_home).
        fs::create_dir_all(home)?;
        let mut files = 0usize;
        for entry in &manifest.files {
            let rel = validate_archive_path(Path::new(&entry.path))?;
            let src = staging.join(&rel);
            let dest = home.join(&rel);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            install_file(&src, &dest)
                .map_err(|e| BackupError::Io(format!("install {}: {e}", dest.display())))?;
            files += 1;
        }
        Ok(RestoreReport {
            home: home.to_path_buf(),
            files,
            sessions: sessions.len(),
            chains_verified,
        })
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

/// Copy `src` to `dest` via a tmp file + rename in the destination
/// directory, with mode 0600. The rename is atomic on one filesystem;
/// staging and home are expected to share one (same as the Host's own
/// atomic writes).
fn install_tmp_name(dest: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    dest.with_extension(format!("restore-tmp-{}-{n}", std::process::id()))
}

#[cfg(unix)]
fn install_file(src: &Path, dest: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let tmp = install_tmp_name(dest);
    let result = (|| -> std::io::Result<()> {
        fs::copy(src, &tmp)?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
        fs::rename(&tmp, dest)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(not(unix))]
fn install_file(src: &Path, dest: &Path) -> std::io::Result<()> {
    let tmp = install_tmp_name(dest);
    let result = (|| -> std::io::Result<()> {
        fs::copy(src, &tmp)?;
        fs::rename(&tmp, dest)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_reviews::{record_review, Actor, ReviewDecision};

    /// A short-path private home (sockaddr_un caps socket paths near 104
    /// bytes; keep every test home short for the same reason).
    fn test_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("up-bak-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_review_log(dir: &Path, n: u32) {
        fs::create_dir_all(dir).unwrap();
        for i in 0..n {
            record_review(
                dir,
                Actor::PolicyAllow,
                "test-connector",
                "test-tool",
                &format!("argshash{i:04}"),
                ReviewDecision::Approved,
                None,
            )
            .unwrap();
        }
    }

    fn seed_home(home: &Path) -> (PathBuf, PathBuf) {
        // Session with a chained review log + metadata.
        let session_dir = home.join("app-sessions").join("sess-1");
        write_review_log(&session_dir, 5);
        fs::write(session_dir.join("manifest.json"), r#"{"id":"sess-1"}"#).unwrap();
        fs::write(session_dir.join("output.bin"), b"fake pty bytes").unwrap();
        fs::write(session_dir.join("title.json"), r#"{"title":"demo"}"#).unwrap();
        let artifact = session_dir.join("artifacts").join("uploads");
        fs::create_dir_all(&artifact).unwrap();
        fs::write(artifact.join("shot.png"), b"fake png bytes").unwrap();
        // Top-level state.
        fs::write(home.join("app-state.json"), r#"{"v":1}"#).unwrap();
        fs::write(home.join("pane-layouts.json"), r#"{"layout":"tall"}"#).unwrap();
        // Pairing state.
        let mobile = home.join("mobile");
        fs::create_dir_all(&mobile).unwrap();
        fs::write(mobile.join("devices.json"), r#"{"devices":[]}"#).unwrap();
        fs::write(mobile.join("mac-id"), b"test-machine-id").unwrap();
        // Lease DB with schema (open initialises it).
        let _leases = crate::schedule_leases::ScheduleLeases::open(home, "test-tenant").unwrap();
        (home.to_path_buf(), session_dir)
    }

    #[test]
    fn round_trip_preserves_state_and_chains() {
        let home = test_home("roundtrip");
        let (_h, session_dir) = seed_home(&home);
        let archive = home
            .join("..")
            .join(format!("up-bak-rt-{}.tar", std::process::id()));

        let report = create_backup(&home, &archive).unwrap();
        assert!(report.files >= 5, "files: {}", report.files);
        assert_eq!(report.sessions, 1);
        assert!(report.broken_chains.is_empty());
        assert!(archive.is_file());

        // Restore into a fresh home.
        let dest = test_home("roundtrip-dest");
        let r = restore_backup(&archive, &dest, false).unwrap();
        assert_eq!(r.sessions, 1);
        assert_eq!(r.chains_verified, 1);

        // Bytes identical, chain verifies.
        let orig_log = fs::read(session_dir.join(REVIEWS_FILE)).unwrap();
        let new_log =
            fs::read(dest.join("app-sessions").join("sess-1").join(REVIEWS_FILE)).unwrap();
        assert_eq!(orig_log, new_log);
        assert_eq!(fs::read(dest.join("app-state.json")).unwrap(), b"{\"v\":1}");
        assert!(dest.join(LEASES_DB_NAME).is_file());
        // New scope: pairing state, session title, artifacts, layouts.
        assert_eq!(
            fs::read(dest.join("mobile").join("devices.json")).unwrap(),
            b"{\"devices\":[]}"
        );
        assert_eq!(
            fs::read(dest.join("mobile").join("mac-id")).unwrap(),
            b"test-machine-id"
        );
        assert_eq!(
            fs::read(dest.join("app-sessions").join("sess-1").join("title.json")).unwrap(),
            b"{\"title\":\"demo\"}"
        );
        assert_eq!(
            fs::read(
                dest.join("app-sessions")
                    .join("sess-1")
                    .join("artifacts")
                    .join("uploads")
                    .join("shot.png")
            )
            .unwrap(),
            b"fake png bytes"
        );
        assert_eq!(
            fs::read(dest.join("pane-layouts.json")).unwrap(),
            b"{\"layout\":\"tall\"}"
        );
        // Restored files land 0600 (they can carry device tokens).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dest.join("mobile").join("devices.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "restored devices.json mode {mode:o}");
        }
        verify_review_chain(&dest.join("app-sessions").join("sess-1")).unwrap();

        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&dest);
        let _ = fs::remove_file(&archive);
    }

    /// Flip one byte of a file's payload inside the tar (not the
    /// manifest): restore must refuse with a hash mismatch.
    fn tamper_tar_payload(archive: &Path, target_name: &str) {
        let mut bytes = fs::read(archive).unwrap();
        // Walk 512-byte records; find the header naming target_name, then
        // flip the first byte of the following data block.
        let mut off = 0usize;
        while off + 512 <= bytes.len() {
            let header = &bytes[off..off + 512];
            if header.iter().all(|&b| b == 0) {
                break;
            }
            let name = String::from_utf8_lossy(&header[..100]);
            let name = name.trim_matches('\0');
            let size_octal = String::from_utf8_lossy(&header[124..136]);
            let size = usize::from_str_radix(size_octal.trim_matches('\0').trim(), 8).unwrap_or(0);
            off += 512;
            if name == target_name && size > 0 {
                bytes[off] ^= 0xFF;
                fs::write(archive, &bytes).unwrap();
                return;
            }
            off += size.div_ceil(512) * 512;
        }
        panic!("target {target_name} not found in tar");
    }

    #[test]
    fn tampered_archive_is_refused() {
        let home = test_home("tamper");
        seed_home(&home);
        let archive = home
            .join("..")
            .join(format!("up-bak-tp-{}.tar", std::process::id()));
        create_backup(&home, &archive).unwrap();

        tamper_tar_payload(&archive, &format!("app-sessions/sess-1/{REVIEWS_FILE}"));

        let dest = test_home("tamper-dest");
        let err = restore_backup(&archive, &dest, false).unwrap_err();
        assert!(
            matches!(err, BackupError::HashMismatch { .. }),
            "unexpected: {err}"
        );
        // Nothing was installed.
        assert!(!dest.join("app-sessions").exists());

        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&dest);
        let _ = fs::remove_file(&archive);
    }

    #[test]
    fn broken_chain_backup_warns_and_restore_installs_nothing() {
        let home = test_home("brokenchain");
        let (home, session_dir) = seed_home(&home);
        // Corrupt the live review log: the backup still completes (the
        // safety tool must not be blocked) but reports the advisory…
        use std::io::Write;
        let mut log = fs::OpenOptions::new()
            .append(true)
            .truncate(false)
            .open(session_dir.join(REVIEWS_FILE))
            .unwrap();
        writeln!(log, "not-json{{{{").unwrap();
        drop(log);

        let archive = home
            .join("..")
            .join(format!("up-bak-bc-{}.tar", std::process::id()));
        let report = create_backup(&home, &archive).unwrap();
        assert_eq!(report.sessions, 1);
        assert!(
            !report.broken_chains.is_empty(),
            "advisory must flag the broken chain"
        );

        // …and restore refuses with the destination untouched: the chain
        // is verified in staging BEFORE anything is installed.
        let dest = test_home("brokenchain-dest");
        let err = restore_backup(&archive, &dest, false).unwrap_err();
        assert!(
            matches!(err, BackupError::ChainInvalid { .. }),
            "unexpected: {err}"
        );
        assert!(
            !dest.join("app-sessions").exists(),
            "partial restore must not happen"
        );
        assert!(
            !dest.join("app-state.json").exists(),
            "no file may be installed before chain verification"
        );

        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&dest);
        let _ = fs::remove_file(&archive);
    }

    #[test]
    fn backup_refuses_destination_inside_home() {
        let home = test_home("insidedest");
        seed_home(&home);
        let err = create_backup(&home, &home.join("backup.tar")).unwrap_err();
        assert!(
            err.to_string().contains("inside the backed-up home"),
            "unexpected: {err}"
        );
        assert!(!home.join("backup.tar").exists());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn restore_refuses_while_host_lock_held_and_over_existing_state() {
        let home = test_home("refuse");
        seed_home(&home);
        let archive = home
            .join("..")
            .join(format!("up-bak-rf-{}.tar", std::process::id()));
        create_backup(&home, &archive).unwrap();

        // Simulate a running Host: hold the serve.lock flock.
        let lock_path = home.join("serve.lock");
        let lock_file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            assert_eq!(
                unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
                0
            );
        }
        assert!(host_lock_held(&home));
        let dest_err = restore_backup(&archive, &home, true).unwrap_err();
        assert!(
            matches!(dest_err, BackupError::HostRunning(_)),
            "unexpected: {dest_err}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_UN) };
        }
        drop(lock_file);

        // Without --force, existing state blocks restore.
        let err = restore_backup(&archive, &home, false).unwrap_err();
        assert!(
            matches!(err, BackupError::HomeNotEmpty(_)),
            "unexpected: {err}"
        );
        // With --force it proceeds (lock released).
        let _ = fs::remove_file(&lock_path);
        let r = restore_backup(&archive, &home, true).unwrap();
        assert_eq!(r.chains_verified, 1);

        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_file(&archive);
    }

    #[test]
    fn backup_during_active_writes_stays_chain_valid() {
        let home = test_home("active");
        let session_dir = home.join("app-sessions").join("sess-w");
        fs::create_dir_all(&session_dir).unwrap();
        write_review_log(&session_dir, 3);

        // Writers append while backups run.
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut writers = Vec::new();
        for w in 0..4 {
            let dir = session_dir.clone();
            let stop = stop.clone();
            writers.push(std::thread::spawn(move || {
                let mut i = 0u32;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = record_review(
                        &dir,
                        Actor::PolicyAllow,
                        "w",
                        "t",
                        &format!("w{w}-{i:06}"),
                        ReviewDecision::Approved,
                        None,
                    );
                    i += 1;
                }
            }));
        }

        // S2: Grant writers also run concurrently. Each writes a unique
        // grant; the backup must capture a self-consistent grants.json.
        std::env::set_var("UNPEEL_HOME", &home);
        let grant_stop = stop.clone();
        let grant_writer = std::thread::spawn(move || {
            let mut i = 0u32;
            while !grant_stop.load(std::sync::atomic::Ordering::Relaxed) {
                let caller = format!("backup-test-session-{i:06}");
                let _ = crate::grant_store::edit_grants(|map| {
                    let entry = map
                        .entry("mcp_write_approvals".to_string())
                        .or_insert(serde_json::Value::Object(serde_json::Map::new()));
                    if let serde_json::Value::Object(obj) = entry {
                        obj.insert(
                            caller.clone(),
                            serde_json::Value::Array(vec![serde_json::Value::String(
                                "backup-test-target".to_string(),
                            )]),
                        );
                    }
                    Ok::<(), String>(())
                });
                i += 1;
            }
            i
        });

        let mut verified = 0;
        for round in 0..3 {
            let archive = home
                .join("..")
                .join(format!("up-bak-aw-{}-{round}.tar", std::process::id()));
            // Retry the backup if a writer holds the lock at the wrong
            // moment — LogLock acquisition can time out under contention,
            // which is a contention signal, not a corruption signal.
            let report = create_backup(&home, &archive).unwrap();
            assert_eq!(report.sessions, 1);
            let dest = test_home(&format!("active-d{round}"));
            let r = restore_backup(&archive, &dest, false).unwrap();
            assert_eq!(r.chains_verified, 1);
            verified += 1;

            // S2: Verify grants.json in the restored archive is valid JSON
            // and self-consistent (no half-written grants).
            let restored_grants = dest.join("grants.json");
            if restored_grants.exists() {
                let content = fs::read_to_string(&restored_grants).unwrap();
                let parsed: serde_json::Value = serde_json::from_str(&content)
                    .expect("grants.json in backup must be valid JSON");
                assert!(parsed.is_object(), "grants.json must be an object");
                // Every mcp_write_approvals entry must have the expected shape
                if let Some(approvals) = parsed.get("mcp_write_approvals") {
                    if let Some(obj) = approvals.as_object() {
                        for (caller, targets) in obj {
                            assert!(
                                targets.as_array().map(|a| !a.is_empty()).unwrap_or(false),
                                "grant for {} must have non-empty targets",
                                caller
                            );
                        }
                    }
                }
            }

            let _ = fs::remove_dir_all(&dest);
            let _ = fs::remove_file(&archive);
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        for w in writers {
            w.join().unwrap();
        }
        let grants_written = grant_writer.join().unwrap();
        assert!(
            grants_written > 0,
            "grant writer should have written grants"
        );
        assert_eq!(verified, 3);
        // The live log is still a valid chain after the storm.
        verify_review_chain(&session_dir).unwrap();
        // The live grants.json is still valid after the storm.
        let live_grants = home.join("grants.json");
        if live_grants.exists() {
            let content = fs::read_to_string(&live_grants).unwrap();
            let _: serde_json::Value =
                serde_json::from_str(&content).expect("live grants.json must be valid JSON");
        }
        let _ = fs::remove_dir_all(&home);
    }
}
