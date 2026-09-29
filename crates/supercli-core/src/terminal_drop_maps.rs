//! Terminal drop-target and path-drag maps: short-lived rectangles/rows published
//! by hosted terminal apps that accept semantic file/folder drops and file-URL drags.
//!
//! Moved from the Dart client
//! (`clients/supercli-app/lib/terminal/terminal_drop_target_map.dart`,
//! `clients/supercli-app/lib/terminal/terminal_path_drag_map.dart`)
//! per Amein's rule: one implementation, in Rust. The Dart client keeps only UI
//! bindings via `supercli-client-ffi`.
//!
//! Ports of `TerminalDropTargetMap.swift` / `TerminalPathDragMap.swift`.
//! Only the portable logic is here (region hit tests, JSON wire codecs, TTL
//! validation, and the session-directory loader with its 64 KiB fail-closed
//! gate via `std::fs::metadata`). Wall-clock time is injected by the caller.

use serde::{Deserialize, Serialize};

/// Maps larger than this are rejected (fail-closed).
pub const MAXIMUM_BYTES: usize = 64 * 1024;
/// A map older than this (ms) is stale.
pub const MAXIMUM_AGE_MS: u64 = 5_000;
/// A map from the future beyond this skew (ms) is rejected.
pub const MAXIMUM_FUTURE_SKEW_MS: u64 = 5_000;

// ---------------------------------------------------------------------------
// Drop-target map
// ---------------------------------------------------------------------------

/// A rectangular region of terminal cells that accepts drops.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropTargetRegion {
    pub screen_row: u32,
    pub start_column: u32,
    pub end_row: u32,
    pub end_column: u32,
}

impl DropTargetRegion {
    /// Half-open hit test: `[screen_row, end_row) × [start_column, end_column)`.
    pub fn contains(&self, row: u32, column: u32) -> bool {
        row >= self.screen_row
            && row < self.end_row
            && column >= self.start_column
            && column < self.end_column
    }
}

/// Short-lived map from terminal cells to drop targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropTargetMap {
    pub version: u32,
    #[serde(rename = "pid")]
    pub process_id: i64,
    pub updated_at: u64,
    pub regions: Vec<DropTargetRegion>,
}

impl DropTargetMap {
    /// Name of the map file inside the session directory.
    pub const FILENAME: &'static str = "terminal-drop-target-map.json";
    /// Name of the event file written back into the session directory.
    pub const EVENT_FILENAME: &'static str = "terminal-drop-target-event.json";

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        if bytes.len() > MAXIMUM_BYTES {
            return Err(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "drop-target map exceeds maximum bytes",
            )));
        }
        serde_json::from_slice(bytes)
    }

    /// Whether the map is fresh, well-formed, and covers the given cell.
    ///
    /// Mirrors `TerminalDropTargetMap.accepts(row:column:nowMilliseconds:)`:
    /// version must be 1, pid positive, timestamp within
    /// `[updated_at - futureSkew, updated_at + maxAge]`, and some region must
    /// contain the cell.
    pub fn accepts(&self, row: u32, column: u32, now_ms: u64) -> bool {
        if self.version != 1 {
            return false;
        }
        if self.process_id <= 0 {
            return false;
        }
        if self.updated_at > now_ms.saturating_add(MAXIMUM_FUTURE_SKEW_MS) {
            return false;
        }
        if now_ms > self.updated_at.saturating_add(MAXIMUM_AGE_MS) {
            return false;
        }
        self.regions.iter().any(|r| r.contains(row, column))
    }
}

/// A drop event written back into the session directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropTargetEvent {
    pub version: u32,
    pub event_id: String,
    pub updated_at: u64,
    pub kind: DropTargetEventKind,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub screen_row: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub references: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DropTargetEventKind {
    Hover,
    Leave,
    Drop,
}

// ---------------------------------------------------------------------------
// Path-drag map
// ---------------------------------------------------------------------------

/// One mapped row: the columns `[start_column, end_column)` show `path`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathDragRow {
    pub screen_row: u32,
    pub start_column: u32,
    pub end_column: u32,
    pub path: String,
}

/// Short-lived map from terminal grid rows to Host-local paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathDragMap {
    pub version: u32,
    #[serde(rename = "pid")]
    pub process_id: i64,
    pub updated_at: u64,
    pub rows: Vec<PathDragRow>,
}

impl PathDragMap {
    /// Name of the map file inside the session directory.
    pub const FILENAME: &'static str = "terminal-drag-map.json";

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        if bytes.len() > MAXIMUM_BYTES {
            return Err(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "path-drag map exceeds maximum bytes",
            )));
        }
        serde_json::from_slice(bytes)
    }

    /// Loads the map from the session directory, failing closed when the
    /// marker file is missing, oversized, or malformed.
    ///
    /// Mirrors `TerminalPathDragMap.load(from:)`: the 64 KiB gate is enforced
    /// via `std::fs::metadata` before reading, so an oversized marker is
    /// rejected without loading it into memory.
    pub fn load_from_dir(dir: &std::path::Path) -> Option<Self> {
        let marker = dir.join(Self::FILENAME);
        let meta = std::fs::metadata(&marker).ok()?;
        if meta.len() > MAXIMUM_BYTES as u64 {
            return None;
        }
        let bytes = std::fs::read(&marker).ok()?;
        Self::from_json_bytes(&bytes).ok()
    }

    /// Resolves the Host-local path for the given cell, or None when the map
    /// is stale, malformed, or the cell is unmapped.
    ///
    /// Mirrors `TerminalPathDragMap.path(atScreenRow:column:nowMilliseconds:)`:
    /// version must be 1, pid positive, timestamp within
    /// `[updated_at - futureSkew, updated_at + maxAge]`; the first row whose
    /// `screen_row` matches and whose `[start_column, end_column)` covers the
    /// column wins; relative paths fail closed (only absolute paths resolve).
    pub fn path_at(&self, row: u32, column: u32, now_ms: u64) -> Option<&str> {
        if self.version != 1 {
            return None;
        }
        if self.process_id <= 0 {
            return None;
        }
        if self.updated_at > now_ms.saturating_add(MAXIMUM_FUTURE_SKEW_MS) {
            return None;
        }
        if now_ms > self.updated_at.saturating_add(MAXIMUM_AGE_MS) {
            return None;
        }
        self.rows.iter().find_map(|r| {
            if r.screen_row == row
                && column >= r.start_column
                && column < r.end_column
                && is_absolute_path(&r.path)
            {
                Some(r.path.as_str())
            } else {
                None
            }
        })
    }
}

/// Mirrors `(match.path as NSString).isAbsolutePath`: absolute iff it starts
/// with `/`.
fn is_absolute_path(path: &str) -> bool {
    path.starts_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drop_map() -> DropTargetMap {
        DropTargetMap {
            version: 1,
            process_id: 1234,
            updated_at: 100_000,
            regions: vec![DropTargetRegion {
                screen_row: 2,
                start_column: 4,
                end_row: 5,
                end_column: 10,
            }],
        }
    }

    #[test]
    fn region_contains_is_half_open() {
        let r = DropTargetRegion {
            screen_row: 2,
            start_column: 4,
            end_row: 5,
            end_column: 10,
        };
        assert!(r.contains(2, 4));
        assert!(r.contains(4, 9));
        assert!(!r.contains(5, 4)); // end_row exclusive
        assert!(!r.contains(2, 10)); // end_column exclusive
        assert!(!r.contains(1, 4));
    }

    #[test]
    fn drop_map_accepts_validates_all_gates() {
        let m = drop_map();
        assert!(m.accepts(3, 5, 100_000));
        assert!(!m.accepts(0, 0, 100_000)); // no region covers
        assert!(!m.accepts(3, 5, 100_000 + MAXIMUM_AGE_MS + 1)); // stale
        assert!(!m.accepts(3, 5, 100_000 - MAXIMUM_FUTURE_SKEW_MS - 1)); // future skew
        let mut bad_version = m.clone();
        bad_version.version = 2;
        assert!(!bad_version.accepts(3, 5, 100_000));
        let mut bad_pid = m.clone();
        bad_pid.process_id = 0;
        assert!(!bad_pid.accepts(3, 5, 100_000));
    }

    #[test]
    fn drop_map_json_wire_contract() {
        let json = br#"{"version":1,"pid":42,"updated_at":999,"regions":[{"screen_row":1,"start_column":2,"end_row":3,"end_column":4}]}"#;
        let m = DropTargetMap::from_json_bytes(json).unwrap();
        assert_eq!(m.process_id, 42);
        assert_eq!(m.regions.len(), 1);
        assert_eq!(m.regions[0].screen_row, 1);
        // Oversize rejected
        let big = vec![b'x'; MAXIMUM_BYTES + 1];
        assert!(DropTargetMap::from_json_bytes(&big).is_err());
    }

    #[test]
    fn drop_event_json_omits_nulls() {
        let e = DropTargetEvent {
            version: 1,
            event_id: "e1".into(),
            updated_at: 5,
            kind: DropTargetEventKind::Drop,
            screen_row: Some(3),
            column: None,
            text: None,
            references: None,
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "drop");
        assert!(v.get("column").is_none());
        assert!(v.get("text").is_none());
    }

    #[test]
    fn path_drag_map_resolves_first_matching_absolute_path() {
        let m = PathDragMap {
            version: 1,
            process_id: 7,
            updated_at: 50_000,
            rows: vec![
                PathDragRow {
                    screen_row: 3,
                    start_column: 0,
                    end_column: 20,
                    path: "relative/path".into(),
                },
                PathDragRow {
                    screen_row: 3,
                    start_column: 0,
                    end_column: 20,
                    path: "/abs/path".into(),
                },
            ],
        };
        // First row matches the cell but is relative -> fail closed, skip to next
        assert_eq!(m.path_at(3, 5, 50_000), Some("/abs/path"));
        assert_eq!(m.path_at(9, 5, 50_000), None); // unmapped row
        assert_eq!(m.path_at(3, 5, 50_000 + MAXIMUM_AGE_MS + 1), None); // stale
        let mut bad = m.clone();
        bad.version = 9;
        assert_eq!(bad.path_at(3, 5, 50_000), None);
    }

    #[test]
    fn path_drag_map_json_wire_contract() {
        let json = br#"{"version":1,"pid":7,"updated_at":50,"rows":[{"screen_row":3,"start_column":0,"end_column":20,"path":"/x"}]}"#;
        let m = PathDragMap::from_json_bytes(json).unwrap();
        assert_eq!(m.rows.len(), 1);
        assert_eq!(m.rows[0].path, "/x");
    }

    #[test]
    fn loader_rejects_oversized_markers() {
        // Mirrors TerminalPathDragMapTests.testLoaderRejectsOversizedMarkers:
        // a marker file larger than 64 KiB fails closed via the metadata gate.
        let dir = std::env::temp_dir().join(format!(
            "supercli-path-drag-map-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock before epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join(PathDragMap::FILENAME);
        std::fs::write(&marker, vec![b' '; MAXIMUM_BYTES + 1]).unwrap();
        assert!(PathDragMap::load_from_dir(&dir).is_none());

        // A valid marker loads fine.
        std::fs::write(
            &marker,
            br#"{"version":1,"pid":7,"updated_at":50,"rows":[]}"#,
        )
        .unwrap();
        let loaded = PathDragMap::load_from_dir(&dir).expect("valid marker loads");
        assert_eq!(loaded.process_id, 7);
        assert!(loaded.rows.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
