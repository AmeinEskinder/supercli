//! Port of `RemoteUnpeelClient.swift` (Mac-as-client v1).
//!
//! Connect THIS Supercli to another Supercli's remote server
//! (`supercli-host __remote__`) and open its sessions as local terminal panes.
//! Credentials come from the other Mac's remote key (its `~/.supercli/remote.json`
//! or the JSON status line its server prints): `{url, token, fingerprint}`.
//! They are persisted to `~/.supercli/remote-peer.json` (0600) so the token
//! never appears in session manifests or shell history.

use serde::{Deserialize, Serialize};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::tls;

/// A remote Supercli peer's credentials: `{url, token, fingerprint}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSupercliPeer {
    pub url: String,
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
}

impl RemoteSupercliPeer {
    /// Parse a pasted remote key (remote.json contents / server status line).
    /// Returns `None` when the JSON is malformed, the URL is missing/invalid,
    /// or the token is missing/empty — mirroring Swift's `guard` chain.
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        let v: serde_json::Value = serde_json::from_str(trimmed).ok()?;
        let url = v.get("url")?.as_str()?;
        // Swift's `URL(string:)` rejects malformed URLs; require an http(s) scheme.
        let lower = url.to_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            return None;
        }
        let token = v.get("token")?.as_str()?;
        if token.is_empty() {
            return None;
        }
        Some(RemoteSupercliPeer {
            url: url.to_string(),
            token: token.to_string(),
            fingerprint: v
                .get("fingerprint")
                .and_then(|f| f.as_str())
                .map(|s| s.to_string()),
            name: v
                .get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string()),
        })
    }

    /// Normalized (lowercase hex, no separators) fingerprint, if present.
    pub fn normalized_fingerprint(&self) -> Option<String> {
        self.fingerprint
            .as_deref()
            .and_then(tls::normalize_fingerprint)
    }
}

/// One remote session row from `GET /api/sessions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSupercliSession {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub command: String,
    #[serde(default = "default_activity")]
    pub activity: String,
}

fn default_activity() -> String {
    "unknown".to_string()
}

impl RemoteSupercliSession {
    /// Build from a hand-rolled JSON row (the server builds snake_case JSON
    /// by hand — deliberately not the SupercliShared DTOs).
    pub fn from_row(row: &serde_json::Value) -> Option<Self> {
        let id = row.get("id")?.as_str()?.to_string();
        let label = row
            .get("label")
            .and_then(|l| l.as_str())
            .unwrap_or(&id)
            .to_string();
        Some(RemoteSupercliSession {
            id,
            label,
            command: row
                .get("command")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string(),
            activity: row
                .get("activity")
                .and_then(|a| a.as_str())
                .unwrap_or("unknown")
                .to_string(),
        })
    }
}

/// Errors from the remote Supercli client.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RemoteSupercliError {
    /// The peer answered with a non-200 HTTP status.
    #[error("remote Supercli HTTP {0}")]
    BadResponse(u16),
    /// The bearer token was rejected.
    #[error("The remote Supercli rejected the token — grab a fresh remote key.")]
    Unauthorized,
    /// Transport-level failure.
    #[error("remote Supercli transport: {0}")]
    Transport(String),
}

impl RemoteSupercliError {
    /// Map an HTTP status to the user-facing error, mirroring Swift's
    /// `errorDescription` (401 gets the token hint).
    pub fn from_status(status: u16) -> Self {
        if status == 401 {
            RemoteSupercliError::Unauthorized
        } else {
            RemoteSupercliError::BadResponse(status)
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            RemoteSupercliError::Unauthorized => {
                "The remote Supercli rejected the token — grab a fresh remote key.".to_string()
            }
            RemoteSupercliError::BadResponse(status) => {
                format!("The remote Supercli answered HTTP {status}.")
            }
            RemoteSupercliError::Transport(msg) => format!("remote Supercli transport: {msg}"),
        }
    }
}

/// Verify a peer certificate's SHA-256 fingerprint against the expected value.
/// `None` expected means loopback/dev only — accept (matches the CLI).
/// Uses the shared [`tls`] pinning primitives.
pub fn verify_peer_fingerprint(cert_der: &[u8], expected: Option<&str>) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    if expected.is_empty() {
        return true;
    }
    let Some(normalized) = tls::normalize_fingerprint(expected) else {
        return false;
    };
    tls::leaf_fingerprint_hex(cert_der) == normalized
}

/// Path to `remote-peer.json` under the Supercli home dir.
/// Owner-only (0600) so the token stays out of command lines and histories.
pub fn peer_file_path(supercli_dir: &Path) -> PathBuf {
    supercli_dir.join("remote-peer.json")
}

/// Atomically write the peer file with 0600 permissions.
/// Only `url`, `token`, and `fingerprint` are persisted (not `name`),
/// matching Swift's explicit JSON object.
pub fn save_peer(supercli_dir: &Path, peer: &RemoteSupercliPeer) -> std::io::Result<()> {
    let path = peer_file_path(supercli_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut obj = serde_json::Map::new();
    obj.insert(
        "url".to_string(),
        serde_json::Value::String(peer.url.clone()),
    );
    obj.insert(
        "token".to_string(),
        serde_json::Value::String(peer.token.clone()),
    );
    if let Some(fp) = &peer.fingerprint {
        obj.insert(
            "fingerprint".to_string(),
            serde_json::Value::String(fp.clone()),
        );
    }
    let data = serde_json::to_vec(&obj)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    // Atomic via temp-file + rename.
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &data)?;
    set_owner_only(&tmp)?;
    fs::rename(&tmp, &path)?;
    // Ensure final permissions even if the file already existed.
    set_owner_only(&path)?;
    Ok(())
}

/// Restrict a file to owner-only (0600) on Unix. No-op elsewhere.
#[cfg(unix)]
fn set_owner_only(path: &Path) -> std::io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

/// Restrict a file to owner-only (0600) on Unix. No-op elsewhere.
#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Load the peer from `remote-peer.json`. Returns `None` when absent or corrupt.
pub fn load_peer(supercli_dir: &Path) -> Option<RemoteSupercliPeer> {
    let data = fs::read(peer_file_path(supercli_dir)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&data)).ok()?;
    let url = v.get("url")?.as_str()?.to_string();
    let token = v.get("token")?.as_str()?.to_string();
    if token.is_empty() {
        return None;
    }
    Some(RemoteSupercliPeer {
        url,
        token,
        fingerprint: v
            .get("fingerprint")
            .and_then(|f| f.as_str())
            .map(|s| s.to_string()),
        name: None,
    })
}

/// Delete the peer file. Idempotent.
pub fn clear_peer(supercli_dir: &Path) {
    let _ = fs::remove_file(peer_file_path(supercli_dir));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_peer_json() -> &'static str {
        r#"{"url":"https://192.168.1.10:17661","token":"abc123","fingerprint":"aabbcc","name":"office-mac"}"#
    }

    #[test]
    fn parse_valid_remote_key() {
        let peer = RemoteSupercliPeer::parse(sample_peer_json()).unwrap();
        assert_eq!(peer.url, "https://192.168.1.10:17661");
        assert_eq!(peer.token, "abc123");
        assert_eq!(peer.fingerprint.as_deref(), Some("aabbcc"));
        assert_eq!(peer.name.as_deref(), Some("office-mac"));
    }

    #[test]
    fn parse_trims_surrounding_whitespace() {
        let raw = format!("  \n{}\n  ", sample_peer_json());
        assert!(RemoteSupercliPeer::parse(&raw).is_some());
    }

    #[test]
    fn parse_rejects_missing_url() {
        assert!(RemoteSupercliPeer::parse(r#"{"token":"x"}"#).is_none());
    }

    #[test]
    fn parse_rejects_missing_token() {
        assert!(RemoteSupercliPeer::parse(r#"{"url":"https://h"}"#).is_none());
    }

    #[test]
    fn parse_rejects_empty_token() {
        assert!(RemoteSupercliPeer::parse(r#"{"url":"https://h","token":""}"#).is_none());
    }

    #[test]
    fn parse_rejects_non_http_url() {
        assert!(RemoteSupercliPeer::parse(r#"{"url":"ftp://h","token":"x"}"#).is_none());
    }

    #[test]
    fn parse_rejects_malformed_json() {
        assert!(RemoteSupercliPeer::parse("not json").is_none());
    }

    #[test]
    fn parse_optional_fields_absent() {
        let peer =
            RemoteSupercliPeer::parse(r#"{"url":"http://127.0.0.1:17661","token":"t"}"#).unwrap();
        assert_eq!(peer.fingerprint, None);
        assert_eq!(peer.name, None);
    }

    #[test]
    fn session_from_row_defaults() {
        let row: serde_json::Value = serde_json::from_str(r#"{"id":"s1"}"#).unwrap();
        let s = RemoteSupercliSession::from_row(&row).unwrap();
        assert_eq!(s.id, "s1");
        assert_eq!(s.label, "s1"); // falls back to id
        assert_eq!(s.command, "");
        assert_eq!(s.activity, "unknown");
    }

    #[test]
    fn session_from_row_full() {
        let row: serde_json::Value =
            serde_json::from_str(r#"{"id":"s2","label":"dev","command":"zsh","activity":"busy"}"#)
                .unwrap();
        let s = RemoteSupercliSession::from_row(&row).unwrap();
        assert_eq!(s.label, "dev");
        assert_eq!(s.command, "zsh");
        assert_eq!(s.activity, "busy");
    }

    #[test]
    fn session_from_row_requires_id() {
        let row: serde_json::Value = serde_json::from_str(r#"{"label":"x"}"#).unwrap();
        assert!(RemoteSupercliSession::from_row(&row).is_none());
    }

    #[test]
    fn error_401_is_unauthorized() {
        let e = RemoteSupercliError::from_status(401);
        assert_eq!(e, RemoteSupercliError::Unauthorized);
        assert!(e.user_message().contains("rejected the token"));
    }

    #[test]
    fn error_other_status_is_bad_response() {
        let e = RemoteSupercliError::from_status(500);
        assert_eq!(e, RemoteSupercliError::BadResponse(500));
        assert!(e.user_message().contains("HTTP 500"));
    }

    #[test]
    fn fingerprint_none_accepts() {
        assert!(verify_peer_fingerprint(&[1, 2, 3], None));
    }

    #[test]
    fn fingerprint_empty_accepts() {
        assert!(verify_peer_fingerprint(&[1, 2, 3], Some("")));
    }

    #[test]
    fn fingerprint_mismatch_rejects() {
        // cert DER [1,2,3] hashes to something != deadbeef
        assert!(!verify_peer_fingerprint(&[1, 2, 3], Some("deadbeef")));
    }

    #[test]
    fn fingerprint_match_accepts() {
        let der = [0x30, 0x82, 0x01, 0x0a];
        let expected = tls::leaf_fingerprint_hex(&der);
        assert!(verify_peer_fingerprint(&der, Some(&expected)));
        // case is normalized
        assert!(verify_peer_fingerprint(
            &der,
            Some(&expected.to_uppercase())
        ));
    }

    #[test]
    fn peer_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "remote-peer-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let peer = RemoteSupercliPeer {
            url: "https://10.0.0.5:17661".to_string(),
            token: "sekret".to_string(),
            fingerprint: Some("aabbcc".to_string()),
            name: Some("ignored-in-file".to_string()),
        };
        save_peer(&dir, &peer).unwrap();
        let loaded = load_peer(&dir).unwrap();
        assert_eq!(loaded.url, peer.url);
        assert_eq!(loaded.token, peer.token);
        assert_eq!(loaded.fingerprint, peer.fingerprint);
        // permissions are owner-only on Unix
        #[cfg(unix)]
        {
            let mode = fs::metadata(peer_file_path(&dir))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        clear_peer(&dir);
        assert!(load_peer(&dir).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn peer_file_missing_is_none() {
        let dir = std::env::temp_dir().join("remote-peer-test-nonexistent-dir-xyz");
        assert!(load_peer(&dir).is_none());
    }
}
