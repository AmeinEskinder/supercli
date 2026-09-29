//! Port of `StartupPresentationCache.swift` (SupercliNative).
//!
//! A disposable Controller display cache, never a source of Host lifecycle or
//! authorization. One bounded file avoids opening every manifest before paint.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Maximum cache file size: 4 MiB. Larger payloads are refused, matching Swift.
pub const MAXIMUM_BYTES: usize = 4 * 1024 * 1024;

/// File name within the home directory.
const CACHE_FILE_NAME: &str = "native-startup-cache.json";

/// Cached startup presentation. The `nodes` and `pins` payloads are opaque to
/// the cache — it never interprets them, only round-trips the JSON.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StartupPresentation {
    /// Schema version. Only version 1 is accepted on load.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Home directory this cache was built for.
    pub home: String,
    /// Host ID this cache was built for.
    #[serde(rename = "hostID")]
    pub host_id: String,
    /// Project tree nodes (opaque to the cache).
    pub nodes: Vec<serde_json::Value>,
    /// Pinned sidebar sessions by section (opaque to the cache).
    pub pins: HashMap<String, Vec<serde_json::Value>>,
    /// Archived session IDs.
    #[serde(rename = "archivedIDs")]
    pub archived_ids: HashSet<String>,
    /// Unread session IDs.
    #[serde(rename = "unreadIDs")]
    pub unread_ids: HashSet<String>,
}

fn default_version() -> u32 {
    1
}

impl StartupPresentation {
    /// Current schema version.
    pub const VERSION: u32 = 1;
}

/// Bounded, atomic file cache for the startup presentation.
///
/// Loads validate the schema version, home, host ID, and size bound before
/// returning a value. Saves are atomic via temp-file + rename and refuse
/// payloads over [`MAXIMUM_BYTES`].
#[derive(Debug, Clone)]
pub struct StartupPresentationCache {
    file_path: PathBuf,
}

impl StartupPresentationCache {
    /// Create a cache rooted at `home`. The file lives at
    /// `<home>/native-startup-cache.json`, matching Swift.
    pub fn new(home: &Path) -> Self {
        Self {
            file_path: home.join(CACHE_FILE_NAME),
        }
    }

    /// Path of the cache file.
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// Load and validate the cache.
    ///
    /// Returns `None` when the file is missing, unreadable, over the size
    /// bound, fails to decode, or has a mismatched version/home/host ID —
    /// mirroring Swift's fail-closed `load`.
    pub fn load(&self, home: &str, host_id: &str) -> Option<StartupPresentation> {
        let data = fs::read(&self.file_path).ok()?;
        if data.len() > MAXIMUM_BYTES {
            return None;
        }
        let value: StartupPresentation = serde_json::from_slice(&data).ok()?;
        if value.version != StartupPresentation::VERSION {
            return None;
        }
        if value.home != home || value.host_id != host_id {
            return None;
        }
        Some(value)
    }

    /// Save the presentation atomically.
    ///
    /// Serializes to JSON, refuses payloads over [`MAXIMUM_BYTES`], and
    /// writes via temp-file + rename so a crash never leaves a torn file.
    /// Returns `false` (and leaves the existing file untouched) on any
    /// failure, matching Swift's silent-failure `save`.
    pub fn save(&self, value: &StartupPresentation) -> bool {
        let data = match serde_json::to_vec(value) {
            Ok(d) => d,
            Err(_) => return false,
        };
        if data.len() > MAXIMUM_BYTES {
            return false;
        }
        // Temp file in the same directory so rename is atomic.
        let tmp_path = self.file_path.with_extension("json.tmp");
        let mut tmp = match fs::File::create(&tmp_path) {
            Ok(f) => f,
            Err(_) => return false,
        };
        if tmp.write_all(&data).is_err() {
            let _ = fs::remove_file(&tmp_path);
            return false;
        }
        if tmp.sync_all().is_err() {
            let _ = fs::remove_file(&tmp_path);
            return false;
        }
        drop(tmp);
        if fs::rename(&tmp_path, &self.file_path).is_err() {
            let _ = fs::remove_file(&tmp_path);
            return false;
        }
        true
    }

    /// Remove the cache file, ignoring errors.
    pub fn clear(&self) {
        let _ = fs::remove_file(&self.file_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn test_home() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "supercli-startup-cache-test-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn sample_presentation(home: &str, host_id: &str) -> StartupPresentation {
        StartupPresentation {
            version: 1,
            home: home.to_string(),
            host_id: host_id.to_string(),
            nodes: vec![serde_json::json!({"id": "proj-1"})],
            pins: {
                let mut m = HashMap::new();
                m.insert(
                    "pinned".to_string(),
                    vec![serde_json::json!({"key": "session:abc"})],
                );
                m
            },
            archived_ids: {
                let mut s = HashSet::new();
                s.insert("archived-1".to_string());
                s
            },
            unread_ids: HashSet::new(),
        }
    }

    fn unique_home(prefix: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "supercli-startup-cache-{}-{}-{}",
            prefix,
            std::process::id(),
            n
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn save_then_load_roundtrip() {
        let home_dir = unique_home("roundtrip");
        let home_str = home_dir.to_string_lossy().to_string();
        let cache = StartupPresentationCache::new(&home_dir);
        let value = sample_presentation(&home_str, "host-1");
        assert!(cache.save(&value));
        let loaded = cache.load(&home_str, "host-1");
        assert_eq!(loaded, Some(value));
        let _ = fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn load_missing_file_returns_none() {
        let home_dir = unique_home("missing");
        let home_str = home_dir.to_string_lossy().to_string();
        let cache = StartupPresentationCache::new(&home_dir);
        assert_eq!(cache.load(&home_str, "host-1"), None);
        let _ = fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn load_rejects_wrong_home() {
        let home_dir = unique_home("wrong-home");
        let home_str = home_dir.to_string_lossy().to_string();
        let cache = StartupPresentationCache::new(&home_dir);
        let value = sample_presentation(&home_str, "host-1");
        assert!(cache.save(&value));
        assert_eq!(cache.load("/some/other/home", "host-1"), None);
        let _ = fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn load_rejects_wrong_host_id() {
        let home_dir = unique_home("wrong-host");
        let home_str = home_dir.to_string_lossy().to_string();
        let cache = StartupPresentationCache::new(&home_dir);
        let value = sample_presentation(&home_str, "host-1");
        assert!(cache.save(&value));
        assert_eq!(cache.load(&home_str, "host-2"), None);
        let _ = fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn load_rejects_bad_version() {
        let home_dir = unique_home("bad-version");
        let home_str = home_dir.to_string_lossy().to_string();
        let cache = StartupPresentationCache::new(&home_dir);
        let mut value = sample_presentation(&home_str, "host-1");
        value.version = 999;
        assert!(cache.save(&value));
        assert_eq!(cache.load(&home_str, "host-1"), None);
        let _ = fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn load_rejects_corrupt_json() {
        let home_dir = unique_home("corrupt");
        let home_str = home_dir.to_string_lossy().to_string();
        let cache = StartupPresentationCache::new(&home_dir);
        fs::write(cache.file_path(), b"{not valid json").unwrap();
        assert_eq!(cache.load(&home_str, "host-1"), None);
        let _ = fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn load_rejects_oversize_file() {
        let home_dir = unique_home("oversize");
        let home_str = home_dir.to_string_lossy().to_string();
        let cache = StartupPresentationCache::new(&home_dir);
        // Write a file just over the limit directly (bypassing save's guard).
        let big = vec![b'x'; MAXIMUM_BYTES + 1];
        fs::write(cache.file_path(), &big).unwrap();
        assert_eq!(cache.load(&home_str, "host-1"), None);
        let _ = fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn save_refuses_oversize_payload() {
        let home_dir = unique_home("save-oversize");
        let cache = StartupPresentationCache::new(&home_dir);
        // A single node string large enough to push the JSON over the limit.
        let big_string = "x".repeat(MAXIMUM_BYTES);
        let value = StartupPresentation {
            version: 1,
            home: "h".to_string(),
            host_id: "host".to_string(),
            nodes: vec![serde_json::Value::String(big_string)],
            pins: HashMap::new(),
            archived_ids: HashSet::new(),
            unread_ids: HashSet::new(),
        };
        assert!(!cache.save(&value));
        // No file should have been created.
        assert!(!cache.file_path().exists());
        let _ = fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn save_is_atomic_no_torn_file() {
        let home_dir = unique_home("atomic");
        let home_str = home_dir.to_string_lossy().to_string();
        let cache = StartupPresentationCache::new(&home_dir);
        let v1 = sample_presentation(&home_str, "host-1");
        assert!(cache.save(&v1));
        // Overwrite with a different value; the file must always decode.
        let mut v2 = v1.clone();
        v2.unread_ids.insert("unread-9".to_string());
        assert!(cache.save(&v2));
        let loaded = cache.load(&home_str, "host-1").expect("must decode");
        assert_eq!(loaded, v2);
        // No temp file left behind.
        assert!(!cache.file_path().with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn clear_removes_file() {
        let home_dir = unique_home("clear");
        let home_str = home_dir.to_string_lossy().to_string();
        let cache = StartupPresentationCache::new(&home_dir);
        let value = sample_presentation(&home_str, "host-1");
        assert!(cache.save(&value));
        assert!(cache.file_path().exists());
        cache.clear();
        assert!(!cache.file_path().exists());
        // Clearing a missing file is fine.
        cache.clear();
        let _ = fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn json_field_names_match_swift() {
        // Swift Codable keys: version, home, hostID, nodes, pins,
        // archivedIDs, unreadIDs.
        let value = sample_presentation("/home/u", "host-1");
        let json = serde_json::to_value(&value).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("version"));
        assert!(obj.contains_key("home"));
        assert!(obj.contains_key("hostID"));
        assert!(obj.contains_key("nodes"));
        assert!(obj.contains_key("pins"));
        assert!(obj.contains_key("archivedIDs"));
        assert!(obj.contains_key("unreadIDs"));
        // And it decodes back.
        let back: StartupPresentation = serde_json::from_value(json).unwrap();
        assert_eq!(back, value);
    }

    #[test]
    fn test_home_helper_creates_dir() {
        // The test_home helper is used to ensure temp dirs exist.
        let dir = test_home();
        assert!(dir.is_dir());
    }
}
