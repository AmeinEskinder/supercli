//! Workspace registry: multiple isolated app instances on one machine.
//!
//! Moved from the Dart client (`clients/supercli-app/lib/screens/workspace_registry.dart`)
//! per Amein's rule: one implementation, in Rust. The Dart client keeps only UI
//! bindings via `supercli-client-ffi`.
//!
//! A workspace is a separate running instance with its own SUPERCLI_HOME. The
//! released registry lives in the REAL home (`~/.supercli/profiles.json`), never
//! the instance's supercliDir. The on-disk spelling is a released compatibility
//! contract: the collection is encoded as `profiles` even though every source
//! and UI surface calls the product concept Workspaces now.
//!
//! Only the portable logic is here (record codec, slugify, path normalization,
//! list-order keys, create/rename/remove). Platform I/O is injected via the
//! [`WorkspaceRegistryIo`] trait.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A workspace record: one isolated app instance.
///
/// The absolute `home` is minted once at create; rename never moves it (hook
/// configs may already point into it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRecord {
    pub id: String,
    pub name: String,
    pub home: PathBuf,
    pub created_at_ms: u64,
}

/// Platform I/O for the workspace registry. Injected for testability.
pub trait WorkspaceRegistryIo {
    /// Read the registry file bytes, or None if missing.
    fn read_registry(&self) -> Option<Vec<u8>>;
    /// Write the registry file bytes atomically.
    fn write_registry(&mut self, data: &[u8]) -> std::io::Result<()>;
    /// Create a directory (including parents).
    fn create_directory(&mut self, path: &Path) -> std::io::Result<()>;
    /// True if a path exists.
    fn path_exists(&self, path: &Path) -> bool;
    /// Delete a directory tree.
    fn delete_directory(&mut self, path: &Path) -> std::io::Result<()>;
    /// Current time in milliseconds since epoch.
    fn now_ms(&self) -> u64;
    /// Generate a random UUID (lowercased).
    fn new_uuid(&self) -> String;
}

/// Error for workspace operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceError(pub String);

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WorkspaceError: {}", self.0)
    }
}

impl std::error::Error for WorkspaceError {}

/// Decode the registry JSON bytes to records.
/// Returns an empty vec on invalid JSON (fail-open for reads).
pub fn decode_registry(data: &[u8]) -> Vec<WorkspaceRecord> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
        return Vec::new();
    };
    value
        .get("profiles")
        .and_then(|p| p.as_array())
        .map(|profiles| {
            profiles
                .iter()
                .filter_map(|e| {
                    Some(WorkspaceRecord {
                        id: e.get("id")?.as_str()?.to_string(),
                        name: e.get("name")?.as_str()?.to_string(),
                        home: PathBuf::from(e.get("home")?.as_str()?),
                        created_at_ms: e
                            .get("createdAt")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Encode records to the registry JSON bytes.
pub fn encode_registry(workspaces: &[WorkspaceRecord]) -> Vec<u8> {
    let profiles: Vec<serde_json::Value> = workspaces
        .iter()
        .map(|w| {
            serde_json::json!({
                "id": w.id,
                "name": w.name,
                "home": w.home.to_string_lossy(),
                "createdAt": w.created_at_ms,
            })
        })
        .collect();
    let doc = serde_json::json!({ "version": 1, "profiles": profiles });
    serde_json::to_vec(&doc).unwrap_or_default()
}

/// Load the registry, returning empty on any failure.
pub fn load_registry(io: &dyn WorkspaceRegistryIo) -> Vec<WorkspaceRecord> {
    io.read_registry().map(|d| decode_registry(&d)).unwrap_or_default()
}

/// Save the registry.
pub fn save_registry(
    io: &mut dyn WorkspaceRegistryIo,
    workspaces: &[WorkspaceRecord],
) -> std::io::Result<()> {
    io.write_registry(&encode_registry(workspaces))
}

/// Create a workspace record. Returns the new record.
/// Errors on empty name.
pub fn create_workspace(
    io: &mut dyn WorkspaceRegistryIo,
    registry_dir: &Path,
    name: &str,
) -> Result<WorkspaceRecord, WorkspaceError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(WorkspaceError("Give the workspace a name.".into()));
    }
    // Re-read right before mutating: another instance may have edited the
    // registry (atomic last-writer-wins is the concurrency model).
    let mut workspaces = load_registry(io);
    let slug = unique_slug(io, registry_dir, trimmed, &workspaces);
    let home = registry_dir.join("profiles").join(&slug);
    io.create_directory(&home)
        .map_err(|e| WorkspaceError(format!("create workspace dir: {e}")))?;
    let record = WorkspaceRecord {
        id: io.new_uuid().to_lowercase(),
        name: trimmed.to_string(),
        home,
        created_at_ms: io.now_ms(),
    };
    workspaces.push(record.clone());
    save_registry(io, &workspaces)
        .map_err(|e| WorkspaceError(format!("save registry: {e}")))?;
    Ok(record)
}

/// Rename a workspace. No-op on empty name or unknown id.
pub fn rename_workspace(
    io: &mut dyn WorkspaceRegistryIo,
    id: &str,
    name: &str,
) -> std::io::Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let mut workspaces = load_registry(io);
    let Some(index) = workspaces.iter().position(|w| w.id == id) else {
        return Ok(());
    };
    workspaces[index].name = trimmed.to_string();
    save_registry(io, &workspaces)
}

/// Forget a workspace. `delete_data` also removes its home dir — but only if
/// the home is under the managed root.
pub fn remove_workspace(
    io: &mut dyn WorkspaceRegistryIo,
    registry_dir: &Path,
    id: &str,
    delete_data: bool,
) -> std::io::Result<()> {
    let mut workspaces = load_registry(io);
    let Some(index) = workspaces.iter().position(|w| w.id == id) else {
        return Ok(());
    };
    let record = workspaces.remove(index);
    save_registry(io, &workspaces)?;
    if delete_data {
        let normalized = normalize_path(&record.home);
        let root = normalize_path(&registry_dir.join("profiles"));
        if normalized.starts_with(&root) {
            let _ = io.delete_directory(&record.home);
        }
    }
    Ok(())
}

/// Convert a display name to a URL-safe slug.
///
/// Mirrors the native registry's slugify: lowercase ASCII alphanumerics,
/// runs of anything else collapse to one dash, no leading/trailing dashes.
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = true; // suppress leading dashes
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "workspace".to_string()
    } else {
        slug
    }
}

/// Normalize a path: expand tilde, absolutize.
pub fn normalize_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    let expanded = if let Some(rest) = s.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/{rest}")
    } else {
        s.into_owned()
    };
    let p = PathBuf::from(expanded);
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}

fn unique_slug(
    io: &dyn WorkspaceRegistryIo,
    registry_dir: &Path,
    name: &str,
    existing: &[WorkspaceRecord],
) -> String {
    let base = slugify(name);
    let taken: HashSet<PathBuf> = existing.iter().map(|w| normalize_path(&w.home)).collect();
    let mut candidate = base.clone();
    let mut counter = 2;
    loop {
        let path = registry_dir.join("profiles").join(&candidate);
        if !taken.contains(&normalize_path(&path)) && !io.path_exists(&path) {
            return candidate;
        }
        candidate = format!("{base}-{counter}");
        counter += 1;
    }
}

/// Controller-local, user-chosen display order for the unified workspace list.
///
/// Keys are kind-prefixed so local homes and remote host ids can never
/// collide: `local:<normalized home>`, `host:<hostID>`, `ssh:<id>`.
/// Unknown keys keep their natural build order after the saved ones.
pub mod list_order {
    use super::normalize_path;
    use std::path::Path;

    pub fn local_key(home: &Path) -> String {
        format!("local:{}", normalize_path(home).to_string_lossy())
    }

    pub fn paired_key(host_id: &str) -> String {
        format!("host:{host_id}")
    }

    pub fn ssh_key(id: &str) -> String {
        format!("ssh:{id}")
    }

    /// Stable sort by saved position; unsaved keys follow in build order.
    pub fn apply<T>(rows: Vec<T>, saved_order: &[String], key: impl Fn(&T) -> String) -> Vec<T> {
        let position: std::collections::HashMap<&str, usize> = saved_order
            .iter()
            .enumerate()
            .map(|(i, k)| (k.as_str(), i))
            .collect();
        let mut indexed: Vec<(usize, T)> = rows.into_iter().enumerate().collect();
        indexed.sort_by(|(ia, a), (ib, b)| {
            let pa = position.get(key(a).as_str());
            let pb = position.get(key(b).as_str());
            match (pa, pb) {
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => ia.cmp(ib),
            }
        });
        indexed.into_iter().map(|(_, v)| v).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct MemIo {
        registry: Option<Vec<u8>>,
        dirs: HashSet<PathBuf>,
        now: u64,
        uuid_counter: RefCell<u32>,
    }

    impl MemIo {
        fn new() -> Self {
            Self {
                registry: None,
                dirs: HashSet::new(),
                now: 1_700_000_000_000,
                uuid_counter: RefCell::new(0),
            }
        }
    }

    impl WorkspaceRegistryIo for MemIo {
        fn read_registry(&self) -> Option<Vec<u8>> {
            self.registry.clone()
        }
        fn write_registry(&mut self, data: &[u8]) -> std::io::Result<()> {
            self.registry = Some(data.to_vec());
            Ok(())
        }
        fn create_directory(&mut self, path: &Path) -> std::io::Result<()> {
            self.dirs.insert(path.to_path_buf());
            Ok(())
        }
        fn path_exists(&self, path: &Path) -> bool {
            self.dirs.contains(path)
        }
        fn delete_directory(&mut self, path: &Path) -> std::io::Result<()> {
            self.dirs.remove(path);
            Ok(())
        }
        fn now_ms(&self) -> u64 {
            self.now
        }
        fn new_uuid(&self) -> String {
            let mut c = self.uuid_counter.borrow_mut();
            *c += 1;
            format!("uuid-{c:04}")
        }
    }

    #[test]
    fn slugify_matches_dart() {
        assert_eq!(slugify("My Workspace"), "my-workspace");
        assert_eq!(slugify("  Leading"), "leading");
        assert_eq!(slugify("Trailing  "), "trailing");
        assert_eq!(slugify("a--b__c"), "a-b-c");
        assert_eq!(slugify("!!!"), "workspace");
        assert_eq!(slugify(""), "workspace");
        assert_eq!(slugify("Café"), "caf");
    }

    #[test]
    fn codec_round_trip() {
        let records = vec![WorkspaceRecord {
            id: "id1".into(),
            name: "Work".into(),
            home: PathBuf::from("/Users/me/.supercli/profiles/work"),
            created_at_ms: 123,
        }];
        let bytes = encode_registry(&records);
        let back = decode_registry(&bytes);
        assert_eq!(back, records);
        // Invalid JSON -> empty
        assert!(decode_registry(b"not json").is_empty());
    }

    #[test]
    fn create_rename_remove() {
        let mut io = MemIo::new();
        let dir = PathBuf::from("/real");
        let rec = create_workspace(&mut io, &dir, "My Work").unwrap();
        assert_eq!(rec.name, "My Work");
        assert_eq!(rec.home, PathBuf::from("/real/profiles/my-work"));
        // Duplicate name gets a numeric suffix
        let rec2 = create_workspace(&mut io, &dir, "My Work").unwrap();
        assert_eq!(rec2.home, PathBuf::from("/real/profiles/my-work-2"));

        rename_workspace(&mut io, &rec.id, "Renamed").unwrap();
        let loaded = load_registry(&io);
        assert_eq!(loaded.iter().find(|r| r.id == rec.id).unwrap().name, "Renamed");
        // Empty rename is a no-op
        rename_workspace(&mut io, &rec.id, "   ").unwrap();
        let loaded = load_registry(&io);
        assert_eq!(loaded.iter().find(|r| r.id == rec.id).unwrap().name, "Renamed");

        // Remove without deleting data keeps the dir
        remove_workspace(&mut io, &dir, &rec.id, false).unwrap();
        assert!(io.dirs.contains(&PathBuf::from("/real/profiles/my-work")));
        // Remove with delete_data removes a managed home
        remove_workspace(&mut io, &dir, &rec2.id, true).unwrap();
        assert!(!io.dirs.contains(&PathBuf::from("/real/profiles/my-work-2")));

        // Empty name errors
        assert!(create_workspace(&mut io, &dir, "   ").is_err());
    }

    #[test]
    fn remove_never_deletes_unmanaged_home() {
        let mut io = MemIo::new();
        let dir = PathBuf::from("/real");
        // Hand-register a record pointing outside the managed root
        let outside = PathBuf::from("/elsewhere/data");
        io.dirs.insert(outside.clone());
        let rec = WorkspaceRecord {
            id: "x".into(),
            name: "X".into(),
            home: outside.clone(),
            created_at_ms: 0,
        };
        save_registry(&mut io, std::slice::from_ref(&rec)).unwrap();
        remove_workspace(&mut io, &dir, "x", true).unwrap();
        // The outside dir must survive
        assert!(io.dirs.contains(&outside));
        assert!(load_registry(&io).is_empty());
    }

    #[test]
    fn list_order_keys_and_apply() {
        use list_order::*;
        assert_eq!(paired_key("h1"), "host:h1");
        assert_eq!(ssh_key("s1"), "ssh:s1");
        assert!(local_key(Path::new("/a/b")).starts_with("local:"));

        let rows = vec!["a", "b", "c", "d"];
        let saved = vec!["k3".to_string(), "k1".to_string()];
        let key = |s: &&str| match *s {
            "a" => "k1".to_string(),
            "b" => "k2".to_string(),
            "c" => "k3".to_string(),
            _ => "k9".to_string(),
        };
        let out = apply(rows, &saved, key);
        // k3 first, k1 second, then unsaved in build order (b=k2, d=k9)
        assert_eq!(out, vec!["c", "a", "b", "d"]);
    }

    #[test]
    fn load_returns_empty_on_missing_or_corrupt() {
        let io = MemIo::new();
        assert!(load_registry(&io).is_empty());
        let mut io2 = MemIo::new();
        io2.registry = Some(b"{{{".to_vec());
        assert!(load_registry(&io2).is_empty());
    }
}
