//! Client-side Supercli Link authority: the shared durable Link authority
//! record (activation, deactivation, service rejections) and the Host-bound
//! entitlement cache.
//!
//! Rust port of the portable core of Swift `RelayUplinkManager.swift`:
//! `RelayConfig`, `LinkSuppressionReason`, `LinkSuppressionRecord`,
//! `LinkCachedEntitlement`, and `LinkAuthorityStore`. The `@MainActor`
//! `ObservableObject` manager, the entitlement HTTP fetch, and APNs push
//! stay Swift-side; what moves here is the filesystem authority that the
//! native Host and the headless Rust Host share.
//!
//! A cached entitlement is a 30-day bearer, so deleting the cache alone is
//! not a sufficient revocation primitive: an unlink can fail, or another
//! frontend can have already read it. The suppression marker lives outside
//! `mobile/`, is written before cache removal, and is serialized with the
//! Swift side through the same `link-license.lock` flock.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Defaults keys and relay URL resolution. Swift: `RelayConfig`.
pub mod relay_config {
    /// The retired global "Access away from home" toggle's key. New builds
    /// gate the uplink purely on per-device enrollment, but the key stays
    /// alive for downgrade compatibility.
    pub const ENABLED_DEFAULTS_KEY: &str = "supercli.native.remoteAccessEnabled";
    /// One-shot marker for the legacy-preference migration.
    pub const ENROLLMENT_MIGRATED_DEFAULTS_KEY: &str = "supercli.native.linkEnrollmentMigrated";
    /// Hidden override for dev (`ws://127.0.0.1:8787` against `wrangler dev`).
    pub const URL_OVERRIDE_DEFAULTS_KEY: &str = "supercli.native.relayURL";
    /// Production relay URL.
    pub const PRODUCTION_URL: &str = "wss://relay.superc.li";

    /// Resolve the relay URL: the override wins when it parses as ws/wss,
    /// otherwise the production URL. Swift: `RelayConfig.relayURL`.
    /// `override_raw` is the stored defaults value, if any.
    pub fn relay_url(override_raw: Option<&str>) -> String {
        if let Some(raw) = override_raw {
            let trimmed = raw.trim();
            if let Some(scheme) = trimmed.split("://").next() {
                if scheme == "ws" || scheme == "wss" {
                    return trimmed.to_string();
                }
            }
        }
        PRODUCTION_URL.to_string()
    }
}

/// Why Link authority is suppressed. Swift: `LinkSuppressionReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkSuppressionReason {
    UserDisabled,
    AuthorizationRejected,
    /// `/api/activate` and the local key commit succeeded, but a fresh relay
    /// entitlement has not committed yet. Cached authority stays blocked;
    /// automatic refresh is safe across process restart.
    ActivationPending,
}

/// Durable Link suppression marker (`link-disabled.json`).
/// Swift: `LinkSuppressionRecord`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkSuppressionRecord {
    pub version: u32,
    #[serde(default)]
    pub generation: String,
    pub reason: LinkSuppressionReason,
    pub disabled_at: i64,
}

/// Cached relay entitlement (`mobile/relay-entitlement.json`).
/// Swift: `LinkCachedEntitlement`. Note the Swift JSON keys are camelCase
/// (`expiresAt`, `macID`) — preserved here for file compatibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkCachedEntitlement {
    pub entitlement: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: i64,
    #[serde(rename = "macID")]
    pub mac_id: String,
}

/// Snapshot of the shared authority. Swift: `LinkAuthorityStore.LocalState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkLocalState {
    pub suppression: Option<LinkSuppressionRecord>,
    pub cached: Option<LinkCachedEntitlement>,
}

/// Outcome of [`LinkAuthorityStore::suppress`].
/// Swift: `LinkAuthorityStore.SuppressionOutcome`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuppressionOutcome {
    pub record: LinkSuppressionRecord,
    /// A committed marker already fails closed. Keep an unlink diagnostic
    /// for logs/tests without turning a safe deactivation into failure.
    pub cache_removal_error: Option<String>,
}

/// Failures of the authority store. Unreadable or malformed durable state
/// is deny, never absence.
#[derive(Debug)]
pub enum LinkAuthorityError {
    /// Marker exists but is not a regular file, has a bad version, or an
    /// empty generation.
    InvalidMarker(String),
    /// Marker JSON does not parse.
    MalformedMarker(String),
    /// The observed suppression generation changed while a request was in
    /// flight; the concurrent deactivation wins.
    GenerationMismatch(String),
    /// Filesystem failure.
    Io(std::io::Error),
}

impl std::fmt::Display for LinkAuthorityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkAuthorityError::InvalidMarker(m) => {
                write!(f, "Link disable marker is invalid: {m}")
            }
            LinkAuthorityError::MalformedMarker(m) => {
                write!(f, "Link disable marker malformed: {m}")
            }
            LinkAuthorityError::GenerationMismatch(m) => write!(f, "{m}"),
            LinkAuthorityError::Io(e) => write!(f, "Link authority I/O failed: {e}"),
        }
    }
}

impl std::error::Error for LinkAuthorityError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LinkAuthorityError::Io(e) => Some(e),
            _ => None,
        }
    }
}

fn suppression_url(home: &Path) -> PathBuf {
    home.join("link-disabled.json")
}

fn lock_url(home: &Path) -> PathBuf {
    home.join("link-license.lock")
}

fn cache_url(home: &Path) -> PathBuf {
    home.join("mobile").join("relay-entitlement.json")
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn new_generation() -> String {
    // 128-bit random hex, lowercased like Swift's UUID().uuidString.
    let mut bytes = [0u8; 16];
    if let Ok(mut f) = File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut bytes);
    } else {
        let seed = now_unix() as u64;
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = ((seed >> ((i % 8) * 8)) as u8).wrapping_add(i as u8);
        }
    }
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Exclusive filesystem lock (`link-license.lock`, 0o600) shared with the
/// Swift side. Uses a lock file + `flock(2)` via libc-free `fcntl`
/// emulation: Rust std has no flock, so we use a pid-scoped lock file with
/// `O_CREAT|O_EXCL` retry... — actually `fs2` is unavailable; implement
/// with `libc` if present, else fall back to a blocking open.
///
/// To avoid new dependencies, this uses the `flock(2)` syscall through a
/// tiny `extern "C"` declaration.
mod flock {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::os::unix::io::AsRawFd;

    #[link(name = "c")]
    extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }

    const LOCK_EX: i32 = 2;
    const LOCK_UN: i32 = 8;

    pub struct FileLock {
        file: std::fs::File,
    }

    impl FileLock {
        pub fn exclusive(path: &std::path::Path) -> std::io::Result<Self> {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .mode(0o600)
                .open(path)?;
            // Ensure 0o600 even when the file already existed.
            let mut perms = file.metadata()?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(path, perms)?;
            let ret = unsafe { flock(file.as_raw_fd(), LOCK_EX) };
            if ret != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Self { file })
        }
    }

    impl Drop for FileLock {
        fn drop(&mut self) {
            unsafe {
                flock(self.file.as_raw_fd(), LOCK_UN);
            }
        }
    }
}

/// Write `data` to `url` atomically (tmp file + fsync + rename) with 0o600
/// permissions. Swift: `LinkAuthorityStore.writePrivateAtomically`.
fn write_private_atomically(data: &[u8], url: &Path) -> Result<(), LinkAuthorityError> {
    let directory = url.parent().ok_or_else(|| {
        LinkAuthorityError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "no parent directory",
        ))
    })?;
    fs::create_dir_all(directory).map_err(LinkAuthorityError::Io)?;
    let tmp = directory.join(format!(
        ".{}.{}.tmp",
        url.file_name().and_then(|n| n.to_str()).unwrap_or("tmp"),
        new_generation()
    ));
    {
        let mut f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(LinkAuthorityError::Io)?;
        f.write_all(data).map_err(LinkAuthorityError::Io)?;
        f.sync_all().map_err(LinkAuthorityError::Io)?;
    }
    fs::rename(&tmp, url).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        LinkAuthorityError::Io(e)
    })?;
    Ok(())
}

fn is_regular_file(path: &Path) -> Result<bool, LinkAuthorityError> {
    match fs::symlink_metadata(path) {
        Ok(md) => Ok(md.file_type().is_file()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(LinkAuthorityError::Io(e)),
    }
}

/// Shared durable Link authority. All methods take the `link-license.lock`
/// flock. Swift: `LinkAuthorityStore`.
pub struct LinkAuthorityStore;

impl LinkAuthorityStore {
    fn with_lock<T>(
        home: &Path,
        body: impl FnOnce() -> Result<T, LinkAuthorityError>,
    ) -> Result<T, LinkAuthorityError> {
        fs::create_dir_all(home).map_err(LinkAuthorityError::Io)?;
        let _lock = flock::FileLock::exclusive(&lock_url(home)).map_err(LinkAuthorityError::Io)?;
        body()
    }

    fn read_suppression_unlocked(
        home: &Path,
    ) -> Result<Option<LinkSuppressionRecord>, LinkAuthorityError> {
        let url = suppression_url(home);
        if !is_regular_file(&url)? {
            // Present but not a regular file (or missing): missing → None;
            // non-regular → invalid marker (fail closed).
            if fs::symlink_metadata(&url).is_ok() {
                return Err(LinkAuthorityError::InvalidMarker(
                    "Link disable marker is not a regular file".to_string(),
                ));
            }
            return Ok(None);
        }
        let data = fs::read(&url).map_err(LinkAuthorityError::Io)?;
        let record: LinkSuppressionRecord = serde_json::from_slice(&data).map_err(|e| {
            LinkAuthorityError::MalformedMarker(format!("could not parse marker: {e}"))
        })?;
        if record.version != 1 || record.generation.is_empty() {
            return Err(LinkAuthorityError::InvalidMarker(
                "Link disable marker is invalid".to_string(),
            ));
        }
        Ok(Some(record))
    }

    fn remove_cache_unlocked(home: &Path) -> Option<String> {
        let url = cache_url(home);
        match fs::remove_file(&url) {
            Ok(()) => None,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => Some(format!("could not remove cached Link entitlement: {e}")),
        }
    }

    /// Snapshot the durable authority: suppression marker plus the cached
    /// entitlement (only when no suppression is active and the cache belongs
    /// to `mac_id`).
    pub fn local_state(home: &Path, mac_id: &str) -> Result<LinkLocalState, LinkAuthorityError> {
        Self::with_lock(home, || {
            let suppression = Self::read_suppression_unlocked(home)?;
            let mut cached = None;
            if suppression.is_none() && is_regular_file(&cache_url(home))? {
                if let Ok(data) = fs::read(cache_url(home)) {
                    if let Ok(ent) = serde_json::from_slice::<LinkCachedEntitlement>(&data) {
                        if ent.mac_id == mac_id {
                            cached = Some(ent);
                        }
                    }
                }
            }
            Ok(LinkLocalState {
                suppression,
                cached,
            })
        })
    }

    /// Read the suppression marker, if any.
    pub fn suppression(home: &Path) -> Result<Option<LinkSuppressionRecord>, LinkAuthorityError> {
        Self::with_lock(home, || Self::read_suppression_unlocked(home))
    }

    /// Publish a suppression marker (durable deny) and invalidate the cached
    /// bearer. A late transport/service rejection may strengthen an active
    /// or pending state, but it must never weaken an explicit user off.
    pub fn suppress(
        home: &Path,
        reason: LinkSuppressionReason,
    ) -> Result<SuppressionOutcome, LinkAuthorityError> {
        Self::with_lock(home, || {
            if reason == LinkSuppressionReason::AuthorizationRejected {
                if let Some(current) = Self::read_suppression_unlocked(home)? {
                    if current.reason == LinkSuppressionReason::UserDisabled {
                        return Ok(SuppressionOutcome {
                            record: current,
                            cache_removal_error: Self::remove_cache_unlocked(home),
                        });
                    }
                }
            }
            let record = LinkSuppressionRecord {
                version: 1,
                generation: new_generation(),
                reason,
                disabled_at: now_unix(),
            };
            let data = serde_json::to_vec(&record).map_err(|e| {
                LinkAuthorityError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })?;
            write_private_atomically(&data, &suppression_url(home))?;
            Ok(SuppressionOutcome {
                record,
                cache_removal_error: Self::remove_cache_unlocked(home),
            })
        })
    }

    /// Convert only the authority generation observed before `/api/activate`
    /// began. A deactivation that happened while the request was in flight
    /// writes a different generation and therefore wins. Even a fresh
    /// activation gets a pending marker: a legacy/pre-marker cache must never
    /// authorize a newly activated (possibly different) key.
    ///
    /// Returns the pending generation. Swift: `markActivationPending`.
    pub fn mark_activation_pending(
        home: &Path,
        expected_suppression_generation: Option<&str>,
    ) -> Result<Option<String>, LinkAuthorityError> {
        Self::with_lock(home, || {
            let current = Self::read_suppression_unlocked(home)?;
            if current.as_ref().map(|r| r.generation.as_str()) != expected_suppression_generation {
                return Err(LinkAuthorityError::GenerationMismatch(
                    "Link was disabled while activating".to_string(),
                ));
            }
            let pending = LinkSuppressionRecord {
                version: current.as_ref().map(|r| r.version).unwrap_or(1),
                generation: current
                    .as_ref()
                    .map(|r| r.generation.clone())
                    .unwrap_or_else(new_generation),
                reason: LinkSuppressionReason::ActivationPending,
                disabled_at: current
                    .as_ref()
                    .map(|r| r.disabled_at)
                    .unwrap_or_else(now_unix),
            };
            let data = serde_json::to_vec(&pending).map_err(|e| {
                LinkAuthorityError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })?;
            write_private_atomically(&data, &suppression_url(home))?;
            // The marker already makes the retained bearer unusable; keep a
            // removal failure as a diagnostic only.
            let _ = Self::remove_cache_unlocked(home);
            Ok(Some(pending.generation))
        })
    }

    /// Commit a fresh entitlement only if no newer deactivation/rejection won
    /// while the network request was in flight. Publishing the cache before
    /// clearing the exact marker keeps every crash point fail-closed.
    pub fn commit(
        home: &Path,
        entitlement: &LinkCachedEntitlement,
        expected_suppression_generation: Option<&str>,
    ) -> Result<(), LinkAuthorityError> {
        Self::with_lock(home, || {
            let current = Self::read_suppression_unlocked(home)?;
            if current.as_ref().map(|r| r.generation.as_str()) != expected_suppression_generation {
                return Err(LinkAuthorityError::GenerationMismatch(
                    "Link authority changed while authorizing".to_string(),
                ));
            }
            let data = serde_json::to_vec(entitlement).map_err(|e| {
                LinkAuthorityError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })?;
            write_private_atomically(&data, &cache_url(home))?;
            if current.is_some() {
                fs::remove_file(suppression_url(home)).map_err(|e| {
                    LinkAuthorityError::Io(std::io::Error::new(
                        e.kind(),
                        format!("could not clear Link disable marker: {e}"),
                    ))
                })?;
            }
            Ok(())
        })
    }
}

/// True for HTTP statuses the relay treats as Link authority events (the
/// cached bearer must be distrusted). Swift:
/// `RelayUplinkManager.isAuthorizationRejection`.
pub fn is_authorization_rejection(status_code: Option<u16>) -> bool {
    matches!(status_code, Some(401) | Some(403))
}

/// Human-readable push failure labels for operators.
/// Swift: `RelayUplinkManager.pushFailureLabel`.
pub fn push_failure_label(reason: Option<&str>) -> String {
    match reason {
        Some("remote-disabled") => "Supercli Link is turned off",
        Some("no-mac") => "The Host has no Link identity",
        Some("no-entitlement") => "Link entitlement unavailable",
        Some("forbidden") => "Link entitlement rejected",
        Some("bad-url") => "Invalid Link service URL",
        Some("network") => "Could not reach Supercli Link",
        Some("apns-not-configured") => "Link push is not configured",
        Some("BadDeviceToken") | Some("Unregistered") => "APNs rejected the device token",
        Some("too many pushes") => "Link push rate limit reached",
        Some("bad-token")
        | Some("bad-message")
        | Some("bad-metadata")
        | Some("message-too-large") => "Push request was rejected",
        Some(other) => return format!("Push failed: {other}"),
        None => "Link returned an invalid push response",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn authority_home() -> (PathBuf, impl FnOnce()) {
        let dir = std::env::temp_dir().join(format!(
            "supercli-link-authority-test-{}-{}",
            std::process::id(),
            now_unix_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let cleanup_dir = dir.clone();
        (dir, move || {
            let _ = fs::remove_dir_all(&cleanup_dir);
        })
    }

    fn now_unix_nanos() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }

    #[test]
    fn durable_suppression_precedes_and_survives_cache_removal_failure() {
        let (home, cleanup) = authority_home();
        // A directory at the cache path makes POSIX unlink fail while staying
        // readable enough to reproduce the restart-safety edge.
        let cache = cache_url(&home);
        fs::create_dir_all(&cache).unwrap();

        let outcome =
            LinkAuthorityStore::suppress(&home, LinkSuppressionReason::UserDisabled).unwrap();

        assert!(outcome.cache_removal_error.is_some());
        let restarted = LinkAuthorityStore::local_state(&home, "host-1").unwrap();
        assert_eq!(restarted.suppression, Some(outcome.record.clone()));
        assert_eq!(restarted.cached, None);
        assert_eq!(
            restarted.suppression.as_ref().unwrap().reason,
            LinkSuppressionReason::UserDisabled
        );
        // Marker is written 0o600.
        let perms = fs::symlink_metadata(suppression_url(&home))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(perms, 0o600);
        cleanup();
    }

    #[test]
    fn malformed_suppression_fails_closed() {
        let (home, cleanup) = authority_home();
        fs::write(suppression_url(&home), b"not-json").unwrap();
        assert!(LinkAuthorityStore::local_state(&home, "host-1").is_err());
        cleanup();
    }

    #[test]
    fn non_regular_suppression_marker_fails_closed() {
        let (home, cleanup) = authority_home();
        fs::create_dir_all(suppression_url(&home)).unwrap();
        assert!(LinkAuthorityStore::local_state(&home, "host-1").is_err());
        cleanup();
    }

    #[test]
    fn late_entitlement_cannot_clear_newer_suppression_generation() {
        let (home, cleanup) = authority_home();
        let original =
            LinkAuthorityStore::suppress(&home, LinkSuppressionReason::AuthorizationRejected)
                .unwrap()
                .record;
        let newer = LinkAuthorityStore::suppress(&home, LinkSuppressionReason::UserDisabled)
            .unwrap()
            .record;
        let entitlement = LinkCachedEntitlement {
            entitlement: "late-bearer".to_string(),
            expires_at: now_unix() + 3600,
            mac_id: "host-1".to_string(),
        };
        assert!(
            LinkAuthorityStore::commit(&home, &entitlement, Some(&original.generation)).is_err()
        );
        assert_eq!(LinkAuthorityStore::suppression(&home).unwrap(), Some(newer));
        cleanup();
    }

    #[test]
    fn late_authorization_rejection_cannot_weaken_user_disable() {
        let (home, cleanup) = authority_home();
        let disabled = LinkAuthorityStore::suppress(&home, LinkSuppressionReason::UserDisabled)
            .unwrap()
            .record;
        let rejected =
            LinkAuthorityStore::suppress(&home, LinkSuppressionReason::AuthorizationRejected)
                .unwrap()
                .record;
        assert_eq!(rejected, disabled);
        assert_eq!(
            LinkAuthorityStore::suppression(&home)
                .unwrap()
                .unwrap()
                .reason,
            LinkSuppressionReason::UserDisabled
        );
        cleanup();
    }

    #[test]
    fn activation_pending_survives_restart_and_needs_fresh_entitlement() {
        let (home, cleanup) = authority_home();
        let disabled = LinkAuthorityStore::suppress(&home, LinkSuppressionReason::UserDisabled)
            .unwrap()
            .record;
        let generation =
            LinkAuthorityStore::mark_activation_pending(&home, Some(&disabled.generation)).unwrap();
        let restarted = LinkAuthorityStore::local_state(&home, "host-1").unwrap();
        assert_eq!(generation.as_deref(), Some(disabled.generation.as_str()));
        assert_eq!(
            restarted.suppression.as_ref().unwrap().reason,
            LinkSuppressionReason::ActivationPending
        );
        assert_eq!(restarted.cached, None);

        let entitlement = LinkCachedEntitlement {
            entitlement: "fresh-after-restart".to_string(),
            expires_at: now_unix() + 3600,
            mac_id: "host-1".to_string(),
        };
        LinkAuthorityStore::commit(&home, &entitlement, generation.as_deref()).unwrap();
        let recovered = LinkAuthorityStore::local_state(&home, "host-1").unwrap();
        assert_eq!(recovered.suppression, None);
        assert_eq!(recovered.cached, Some(entitlement));
        cleanup();
    }

    #[test]
    fn fresh_activation_cannot_reuse_legacy_cached_bearer() {
        let (home, cleanup) = authority_home();
        let cache = cache_url(&home);
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        let legacy = LinkCachedEntitlement {
            entitlement: "old-key-bearer".to_string(),
            expires_at: now_unix() + 30 * 24 * 3600,
            mac_id: "host-1".to_string(),
        };
        fs::write(&cache, serde_json::to_vec(&legacy).unwrap()).unwrap();

        let generation = LinkAuthorityStore::mark_activation_pending(&home, None).unwrap();
        let restarted = LinkAuthorityStore::local_state(&home, "host-1").unwrap();
        assert!(generation.is_some());
        assert_eq!(
            restarted.suppression.as_ref().unwrap().reason,
            LinkSuppressionReason::ActivationPending
        );
        assert_eq!(restarted.cached, None);
        assert!(!cache.exists());
        cleanup();
    }

    #[test]
    fn deactivation_during_activation_wins_generation_race() {
        let (home, cleanup) = authority_home();
        let observed =
            LinkAuthorityStore::suppress(&home, LinkSuppressionReason::AuthorizationRejected)
                .unwrap()
                .record;
        let disabled = LinkAuthorityStore::suppress(&home, LinkSuppressionReason::UserDisabled)
            .unwrap()
            .record;
        assert!(
            LinkAuthorityStore::mark_activation_pending(&home, Some(&observed.generation)).is_err()
        );
        assert_eq!(
            LinkAuthorityStore::suppression(&home).unwrap(),
            Some(disabled)
        );
        cleanup();
    }

    #[test]
    fn fresh_entitlement_clears_only_captured_suppression() {
        let (home, cleanup) = authority_home();
        let suppression =
            LinkAuthorityStore::suppress(&home, LinkSuppressionReason::AuthorizationRejected)
                .unwrap()
                .record;
        let entitlement = LinkCachedEntitlement {
            entitlement: "fresh-bearer".to_string(),
            expires_at: now_unix() + 3600,
            mac_id: "host-1".to_string(),
        };
        LinkAuthorityStore::commit(&home, &entitlement, Some(&suppression.generation)).unwrap();
        let state = LinkAuthorityStore::local_state(&home, "host-1").unwrap();
        assert_eq!(state.suppression, None);
        assert_eq!(state.cached, Some(entitlement));
        cleanup();
    }

    #[test]
    fn cached_entitlement_is_mac_id_gated() {
        let (home, cleanup) = authority_home();
        let entitlement = LinkCachedEntitlement {
            entitlement: "bearer".to_string(),
            expires_at: now_unix() + 3600,
            mac_id: "host-1".to_string(),
        };
        LinkAuthorityStore::commit(&home, &entitlement, None).unwrap();
        let other = LinkAuthorityStore::local_state(&home, "host-2").unwrap();
        assert_eq!(other.cached, None);
        let same = LinkAuthorityStore::local_state(&home, "host-1").unwrap();
        assert_eq!(same.cached, Some(entitlement));
        cleanup();
    }

    #[test]
    fn relay_url_prefers_ws_wss_override() {
        use relay_config::*;
        assert_eq!(relay_url(None), PRODUCTION_URL);
        assert_eq!(
            relay_url(Some("ws://127.0.0.1:8787")),
            "ws://127.0.0.1:8787"
        );
        assert_eq!(
            relay_url(Some("  wss://example.com/x  ")),
            "wss://example.com/x"
        );
        assert_eq!(relay_url(Some("http://evil.example")), PRODUCTION_URL);
        assert_eq!(relay_url(Some("not a url")), PRODUCTION_URL);
    }

    #[test]
    fn authorization_rejection_statuses() {
        assert!(is_authorization_rejection(Some(401)));
        assert!(is_authorization_rejection(Some(403)));
        assert!(!is_authorization_rejection(Some(402)));
        assert!(!is_authorization_rejection(None));
        assert!(!is_authorization_rejection(Some(200)));
    }

    #[test]
    fn push_failure_labels_distinguish_operator_actions() {
        assert_eq!(
            push_failure_label(Some("no-entitlement")),
            "Link entitlement unavailable"
        );
        assert_eq!(
            push_failure_label(Some("BadDeviceToken")),
            "APNs rejected the device token"
        );
        assert_eq!(
            push_failure_label(Some("network")),
            "Could not reach Supercli Link"
        );
        assert_eq!(
            push_failure_label(Some("Unregistered")),
            "APNs rejected the device token"
        );
        assert_eq!(
            push_failure_label(Some("remote-disabled")),
            "Supercli Link is turned off"
        );
        assert_eq!(push_failure_label(Some("mystery")), "Push failed: mystery");
        assert_eq!(
            push_failure_label(None),
            "Link returned an invalid push response"
        );
    }

    #[test]
    fn suppression_record_json_uses_snake_case_reason() {
        let record = LinkSuppressionRecord {
            version: 1,
            generation: "abc".to_string(),
            reason: LinkSuppressionReason::ActivationPending,
            disabled_at: 42,
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"reason\":\"activation_pending\""));
        assert!(json.contains("\"disabled_at\":42"));
        let back: LinkSuppressionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back, record);
    }

    #[test]
    fn cached_entitlement_json_preserves_swift_camel_case() {
        // Swift's LinkCachedEntitlement has no CodingKeys: the on-disk keys
        // are `expiresAt`/`macID`. Preserve them for file compatibility.
        let ent = LinkCachedEntitlement {
            entitlement: "b".to_string(),
            expires_at: 7,
            mac_id: "host-1".to_string(),
        };
        let json = serde_json::to_string(&ent).unwrap();
        assert!(json.contains("\"expiresAt\":7"), "{json}");
        assert!(json.contains("\"macID\":\"host-1\""), "{json}");
        let back: LinkCachedEntitlement = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ent);
        // And it must read the actual Swift-written camelCase form.
        let swift_json = r#"{"entitlement":"b","expiresAt":7,"macID":"host-1"}"#;
        let from_swift: LinkCachedEntitlement = serde_json::from_str(swift_json).unwrap();
        assert_eq!(from_swift, ent);
    }
}
