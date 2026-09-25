//! Per-session connector attachments: which connectors a session may use.
//!
//! `supercli connector enable <name> --session <id>` records the attachment;
//! the Host reads it when it assembles the session's MCP servers (spawned
//! per session, token injected as `SUPERCLI_CONNECTOR_TOKEN`, tool list
//! filtered to `tools.provides`, calls wrapped in the session's approval
//! policy). `disconnect` detaches the connector from every session.
//!
//! One JSON file per session dir: `<session-dir>/connectors.json`. Every
//! read-modify-write takes an exclusive flock on `connectors.json.lock`
//! and writes via atomic rename — the shared-state rule.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::manifest::ApprovalPolicy;

/// File inside a session dir holding its connector attachments.
pub const ATTACHMENTS_FILE: &str = "connectors.json";

#[derive(Debug, thiserror::Error)]
pub enum SessionAttachmentError {
    #[error("not a session directory: {0}")]
    NotASession(String),
    #[error("read {0}: {1}")]
    Read(String, #[source] std::io::Error),
    #[error("parse {0}: {1}")]
    Parse(String, #[source] serde_json::Error),
    #[error("write {0}: {1}")]
    Write(String, #[source] std::io::Error),
    #[error("lock {0}: {1}")]
    Lock(String, String),
}

/// One connector attached to a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionAttachment {
    /// When the connector was attached (ms since the epoch).
    pub enabled_at_unix_ms: u64,
    /// Per-tool policy overrides. Tightening only: the manifest default is
    /// the ceiling, enforced by `effective_policy` at call time.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub policy: HashMap<String, ApprovalPolicy>,
}

/// All connectors attached to one session.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionAttachments {
    #[serde(default)]
    pub connectors: HashMap<String, SessionAttachment>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Path of the attachment record for a session dir.
pub fn attachments_path(session_dir: &Path) -> PathBuf {
    session_dir.join(ATTACHMENTS_FILE)
}

/// RAII exclusive flock on `<target>.lock`, released on drop (and by the
/// OS if the process dies mid-edit).
struct FileLock(#[allow(dead_code)] std::fs::File);

fn lock_exclusive(target: &Path) -> Result<FileLock, SessionAttachmentError> {
    use std::os::fd::AsRawFd;
    let lock_path = target.with_extension("lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .map_err(|e| {
            SessionAttachmentError::Lock(lock_path.display().to_string(), e.to_string())
        })?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(SessionAttachmentError::Lock(
            lock_path.display().to_string(),
            std::io::Error::last_os_error().to_string(),
        ));
    }
    Ok(FileLock(file))
}

/// Read a session's attachments. A missing file is an empty set; a corrupt
/// file is an error (never silently dropped).
pub fn read_attachments(session_dir: &Path) -> Result<SessionAttachments, SessionAttachmentError> {
    if !session_dir.is_dir() {
        return Err(SessionAttachmentError::NotASession(
            session_dir.display().to_string(),
        ));
    }
    let path = attachments_path(session_dir);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SessionAttachments::default())
        }
        Err(e) => return Err(SessionAttachmentError::Read(path.display().to_string(), e)),
    };
    serde_json::from_slice(&bytes)
        .map_err(|e| SessionAttachmentError::Parse(path.display().to_string(), e))
}

/// Write a session's attachments: flock, atomic rename, never a torn file.
pub fn write_attachments(
    session_dir: &Path,
    attachments: &SessionAttachments,
) -> Result<(), SessionAttachmentError> {
    if !session_dir.is_dir() {
        return Err(SessionAttachmentError::NotASession(
            session_dir.display().to_string(),
        ));
    }
    let path = attachments_path(session_dir);
    let _lock = lock_exclusive(&path)?;
    let bytes = serde_json::to_vec_pretty(attachments).map_err(|e| {
        SessionAttachmentError::Write(path.display().to_string(), std::io::Error::other(e))
    })?;
    // Unique temp name per process so concurrent writers can't collide.
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, &bytes)
        .map_err(|e| SessionAttachmentError::Write(tmp.display().to_string(), e))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| SessionAttachmentError::Write(path.display().to_string(), e))?;
    Ok(())
}

/// Attach a connector to a session (read-modify-write under one lock).
/// Re-enabling refreshes the timestamp and replaces the policy overrides.
/// Returns true when the connector was newly attached.
pub fn enable_attachment(
    session_dir: &Path,
    name: &str,
    policy: HashMap<String, ApprovalPolicy>,
) -> Result<bool, SessionAttachmentError> {
    if !session_dir.is_dir() {
        return Err(SessionAttachmentError::NotASession(
            session_dir.display().to_string(),
        ));
    }
    let path = attachments_path(session_dir);
    let _lock = lock_exclusive(&path)?;
    let mut attachments = match std::fs::read(&path) {
        Ok(b) => serde_json::from_slice(&b)
            .map_err(|e| SessionAttachmentError::Parse(path.display().to_string(), e))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SessionAttachments::default(),
        Err(e) => return Err(SessionAttachmentError::Read(path.display().to_string(), e)),
    };
    let is_new = !attachments.connectors.contains_key(name);
    attachments.connectors.insert(
        name.to_string(),
        SessionAttachment {
            enabled_at_unix_ms: now_ms(),
            policy,
        },
    );
    let bytes = serde_json::to_vec_pretty(&attachments).map_err(|e| {
        SessionAttachmentError::Write(path.display().to_string(), std::io::Error::other(e))
    })?;
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, &bytes)
        .map_err(|e| SessionAttachmentError::Write(tmp.display().to_string(), e))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| SessionAttachmentError::Write(path.display().to_string(), e))?;
    Ok(is_new)
}

/// Detach a connector from a session. Returns true when something was
/// removed; a missing record (or missing file) is not an error.
pub fn disable_attachment(session_dir: &Path, name: &str) -> Result<bool, SessionAttachmentError> {
    if !session_dir.is_dir() {
        return Err(SessionAttachmentError::NotASession(
            session_dir.display().to_string(),
        ));
    }
    let path = attachments_path(session_dir);
    let _lock = lock_exclusive(&path)?;
    let mut attachments: SessionAttachments = match std::fs::read(&path) {
        Ok(b) => serde_json::from_slice(&b)
            .map_err(|e| SessionAttachmentError::Parse(path.display().to_string(), e))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(false);
        }
        Err(e) => return Err(SessionAttachmentError::Read(path.display().to_string(), e)),
    };
    if attachments.connectors.remove(name).is_none() {
        return Ok(false);
    }
    let bytes = serde_json::to_vec_pretty(&attachments).map_err(|e| {
        SessionAttachmentError::Write(path.display().to_string(), std::io::Error::other(e))
    })?;
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, &bytes)
        .map_err(|e| SessionAttachmentError::Write(tmp.display().to_string(), e))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| SessionAttachmentError::Write(path.display().to_string(), e))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_session() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "supercli-conn-session-test-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn enable_then_disable_roundtrip() {
        let dir = tmp_session();
        assert!(enable_attachment(&dir, "gmail", HashMap::new()).unwrap());
        // Re-enabling is idempotent: not new, timestamp refreshed.
        assert!(!enable_attachment(&dir, "gmail", HashMap::new()).unwrap());
        let read = read_attachments(&dir).unwrap();
        assert!(read.connectors.contains_key("gmail"));

        let mut policy = HashMap::new();
        policy.insert("mail.send".to_string(), ApprovalPolicy::Deny);
        enable_attachment(&dir, "gmail", policy).unwrap();
        let read = read_attachments(&dir).unwrap();
        assert_eq!(
            read.connectors["gmail"].policy["mail.send"],
            ApprovalPolicy::Deny
        );

        assert!(disable_attachment(&dir, "gmail").unwrap());
        assert!(!disable_attachment(&dir, "gmail").unwrap());
        assert!(read_attachments(&dir).unwrap().connectors.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let dir = tmp_session();
        assert!(read_attachments(&dir).unwrap().connectors.is_empty());
        // Disabling a never-attached connector is a clean no-op.
        assert!(!disable_attachment(&dir, "gmail").unwrap());
        // …and must not create the file.
        assert!(!attachments_path(&dir).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn not_a_session_dir_errors() {
        let missing = std::env::temp_dir().join("supercli-conn-session-test-no-such-dir");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(matches!(
            read_attachments(&missing),
            Err(SessionAttachmentError::NotASession(_))
        ));
        assert!(matches!(
            enable_attachment(&missing, "gmail", HashMap::new()),
            Err(SessionAttachmentError::NotASession(_))
        ));
    }

    #[test]
    fn corrupt_file_is_an_error_not_silent_empty() {
        let dir = tmp_session();
        std::fs::write(attachments_path(&dir), b"{oops").unwrap();
        assert!(matches!(
            read_attachments(&dir),
            Err(SessionAttachmentError::Parse(..))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
