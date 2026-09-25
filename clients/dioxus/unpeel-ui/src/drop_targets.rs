//! Terminal file-drop targets and path drag maps, ported from
//! `clients/native/SupercliNative/Sources/SupercliNative/TerminalDropTargetMap.swift`
//! and `TerminalPathDragMap.swift`.
//!
//! Hosted Ratatui Apps publish short-lived terminal-cell rectangles that
//! accept semantic file/folder drops, and a short-lived map from visible
//! grid rows to Host-local paths. The Dioxus desktop webview uses the drop
//! map to route file drops into uploads and the path map for path drags;
//! both maps are presentation-only state that never enters Host manifests,
//! remote protocol state, or durable app state.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Drop-target map published by a hosted app: which terminal cells accept
/// file/folder drops right now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DropTargetRegion {
    pub screen_row: i64,
    pub start_column: i64,
    pub end_row: i64,
    pub end_column: i64,
}

impl DropTargetRegion {
    pub fn contains(&self, row: i64, column: i64) -> bool {
        row >= self.screen_row
            && row < self.end_row
            && column >= self.start_column
            && column < self.end_column
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DropTargetMap {
    pub version: i64,
    #[serde(rename = "pid")]
    pub process_id: i32,
    pub updated_at: u64,
    #[serde(default)]
    pub regions: Vec<DropTargetRegion>,
}

impl DropTargetMap {
    pub const FILENAME: &'static str = "terminal-drop-target-map.json";
    pub const EVENT_FILENAME: &'static str = "terminal-drop-target-event.json";
    pub const MAXIMUM_BYTES: u64 = 64 * 1024;
    pub const MAXIMUM_AGE_MS: u64 = 5_000;
    pub const MAXIMUM_FUTURE_SKEW_MS: u64 = 5_000;

    /// Parse a map, enforcing the size cap the Swift loader applies.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() as u64 > Self::MAXIMUM_BYTES {
            return None;
        }
        serde_json::from_slice(data).ok()
    }

    /// Whether a drop at (row, column) is accepted right now. Wrapping
    /// arithmetic matches the Swift `&+` bounds.
    pub fn accepts(&self, row: i64, column: i64, now_ms: u64) -> bool {
        if self.version != 1 || self.process_id <= 0 {
            return false;
        }
        if self.updated_at > now_ms.wrapping_add(Self::MAXIMUM_FUTURE_SKEW_MS) {
            return false;
        }
        if now_ms > self.updated_at.wrapping_add(Self::MAXIMUM_AGE_MS) {
            return false;
        }
        self.regions.iter().any(|r| r.contains(row, column))
    }

    /// Encode a hover/leave/drop event for the session directory.
    pub fn encode_event(event: &DropTargetEvent) -> Option<Vec<u8>> {
        let data = serde_json::to_vec(event).ok()?;
        if data.len() > 1024 * 1024 {
            return None;
        }
        Some(data)
    }
}

/// A drop event written back into the session directory so the hosted app
/// can move its own caret/scroll while a drag is in flight.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DropTargetEvent {
    pub version: i64,
    pub event_id: String,
    pub updated_at: u64,
    pub kind: DropTargetEventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_row: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DropTargetEventKind {
    Hover,
    Leave,
    Drop,
}

/// Path-drag map: visible grid rows → Host-local paths in the session's
/// own hosted directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PathDragRow {
    pub screen_row: i64,
    pub start_column: i64,
    pub end_column: i64,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PathDragMap {
    pub version: i64,
    #[serde(rename = "pid")]
    pub process_id: i32,
    pub updated_at: u64,
    #[serde(default)]
    pub rows: Vec<PathDragRow>,
}

impl PathDragMap {
    pub const FILENAME: &'static str = "terminal-drag-map.json";
    pub const MAXIMUM_BYTES: u64 = 64 * 1024;
    pub const MAXIMUM_AGE_MS: u64 = 5_000;
    pub const MAXIMUM_FUTURE_SKEW_MS: u64 = 5_000;

    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() as u64 > Self::MAXIMUM_BYTES {
            return None;
        }
        serde_json::from_slice(data).ok()
    }

    /// The Host-local path for a drag starting at (row, column), or `None`
    /// when the map is stale or the row has no absolute path. The returned
    /// path is standardized (lexically normalized), like the Swift
    /// `standardizedFileURL`.
    pub fn path_at(&self, row: i64, column: i64, now_ms: u64) -> Option<String> {
        if self.version != 1 || self.process_id <= 0 {
            return None;
        }
        if self.updated_at > now_ms.wrapping_add(Self::MAXIMUM_FUTURE_SKEW_MS) {
            return None;
        }
        if now_ms > self.updated_at.wrapping_add(Self::MAXIMUM_AGE_MS) {
            return None;
        }
        let matched = self
            .rows
            .iter()
            .find(|r| r.screen_row == row && column >= r.start_column && column < r.end_column)?;
        let path = Path::new(&matched.path);
        if !path.is_absolute() {
            return None;
        }
        Some(lexically_normalize(path))
    }
}

/// Lexical normalization without touching the filesystem (no symlink
/// resolution — presentation-only, like `standardizedFileURL` for our
/// purposes).
fn lexically_normalize(path: &Path) -> String {
    use std::path::Component;
    let mut out = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().into_owned()
}

/// JavaScript installed by the desktop webview: file drops on the terminal
/// surface are reported to Rust (which consults the drop map and uploads),
/// and path drags consult the path map. Mirrors the native destination's
/// hover/drop event flow.
pub const TERMINAL_DND_JS: &str = r#"
(function () {
  if (window.__supercliDndInstalled) return;
  window.__supercliDndInstalled = true;
  const surface = document.querySelector('.terminal-wrap');
  if (!surface) return;
  let dragDepth = 0;
  surface.addEventListener('dragenter', (e) => {
    e.preventDefault();
    dragDepth++;
    const r = surface.getBoundingClientRect();
    window.__supercliDndHover && window.__supercliDndHover(e.clientX - r.left, e.clientY - r.top);
  });
  surface.addEventListener('dragleave', () => {
    dragDepth = Math.max(0, dragDepth - 1);
    if (dragDepth === 0 && window.__supercliDndLeave) window.__supercliDndLeave();
  });
  surface.addEventListener('dragover', (e) => { e.preventDefault(); });
  surface.addEventListener('drop', (e) => {
    e.preventDefault();
    dragDepth = 0;
    const files = Array.from(e.dataTransfer.files || []).map((f) => f.name);
    const r = surface.getBoundingClientRect();
    window.__supercliDndDrop && window.__supercliDndDrop(e.clientX - r.left, e.clientY - r.top, files);
  });
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn drop_map(now: u64) -> DropTargetMap {
        DropTargetMap {
            version: 1,
            process_id: 1234,
            updated_at: now,
            regions: vec![DropTargetRegion {
                screen_row: 2,
                start_column: 4,
                end_row: 6,
                end_column: 20,
            }],
        }
    }

    #[test]
    fn drop_region_contains_edges() {
        let r = DropTargetRegion {
            screen_row: 2,
            start_column: 4,
            end_row: 6,
            end_column: 20,
        };
        assert!(r.contains(2, 4));
        assert!(r.contains(5, 19));
        assert!(!r.contains(6, 4)); // end_row exclusive
        assert!(!r.contains(2, 20)); // end_column exclusive
        assert!(!r.contains(1, 4));
    }

    #[test]
    fn drop_accepts_fresh_map_only() {
        let now = 1_000_000u64;
        let map = drop_map(now);
        assert!(map.accepts(3, 10, now + 1_000));
        assert!(!map.accepts(0, 0, now + 1_000)); // outside regions
        assert!(!map.accepts(3, 10, now + 5_001)); // stale
        assert!(map.accepts(3, 10, now - 1)); // updated_at in the future is fine within skew
        let future = DropTargetMap {
            updated_at: now + 6_000,
            ..drop_map(now)
        };
        assert!(!future.accepts(3, 10, now)); // beyond future skew
        let bad_version = DropTargetMap {
            version: 2,
            ..drop_map(now)
        };
        assert!(!bad_version.accepts(3, 10, now));
        let dead_pid = DropTargetMap {
            process_id: 0,
            ..drop_map(now)
        };
        assert!(!dead_pid.accepts(3, 10, now));
    }

    #[test]
    fn drop_parse_enforces_size_cap() {
        let big = vec![b'x'; (DropTargetMap::MAXIMUM_BYTES + 1) as usize];
        assert!(DropTargetMap::parse(&big).is_none());
        let json = serde_json::to_vec(&drop_map(42)).unwrap();
        let parsed = DropTargetMap::parse(&json).unwrap();
        assert_eq!(parsed.updated_at, 42);
    }

    #[test]
    fn drop_event_encoding() {
        let event = DropTargetEvent {
            version: 1,
            event_id: "e1".into(),
            updated_at: 99,
            kind: DropTargetEventKind::Drop,
            screen_row: Some(3),
            column: Some(10),
            text: None,
            references: Some(vec!["file:///tmp/a".into()]),
        };
        let data = DropTargetMap::encode_event(&event).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&data).unwrap();
        assert_eq!(v["kind"], "drop");
        assert_eq!(v["screen_row"], 3);
        assert_eq!(v["event_id"], "e1");
        assert!(v.get("text").is_none()); // skipped when None
    }

    #[test]
    fn path_map_resolves_absolute_paths() {
        let now = 2_000_000u64;
        let map = PathDragMap {
            version: 1,
            process_id: 99,
            updated_at: now,
            rows: vec![
                PathDragRow {
                    screen_row: 7,
                    start_column: 0,
                    end_column: 40,
                    path: "/tmp/./a/../b.txt".into(),
                },
                PathDragRow {
                    screen_row: 8,
                    start_column: 0,
                    end_column: 40,
                    path: "relative/c.txt".into(),
                },
            ],
        };
        assert_eq!(map.path_at(7, 5, now + 100).as_deref(), Some("/tmp/b.txt"));
        assert_eq!(map.path_at(8, 5, now + 100), None); // not absolute
        assert_eq!(map.path_at(9, 5, now + 100), None); // no row
        assert_eq!(map.path_at(7, 40, now + 100), None); // end_column exclusive
        assert_eq!(map.path_at(7, 5, now + 9_999), None); // stale
    }

    #[test]
    fn path_parse_enforces_size_cap() {
        let big = vec![b'x'; (PathDragMap::MAXIMUM_BYTES + 1) as usize];
        assert!(PathDragMap::parse(&big).is_none());
    }
}
