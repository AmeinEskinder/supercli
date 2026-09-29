//! Preset overlay application.
//!
//! Ported from the legacy Swift store module (`SupercliStore`) — the Presets section's
//! `overlaid(_:overlay:)`. Un-migrated installs layer a legacy overlay (from
//! native defaults) over the file-based presets before folding it into the
//! shared file one-shot.
//!
//! Apply order (from the Swift original):
//! 1. `removed_ids` hide base entries.
//! 2. `edited` replaces base entries by id.
//! 3. `added` appends, skipping ids that are removed or already in the base.

use std::collections::{HashMap, HashSet};

/// Anything the overlay can address by id.
pub trait OverlayItem {
    fn overlay_id(&self) -> &str;
}

/// The legacy overlay: removals, edits, and additions layered over the base.
#[derive(Debug, Clone)]
pub struct PresetOverlay<T> {
    pub removed_ids: Vec<String>,
    pub edited: Vec<T>,
    pub added: Vec<T>,
}

impl<T> Default for PresetOverlay<T> {
    fn default() -> Self {
        Self {
            removed_ids: Vec::new(),
            edited: Vec::new(),
            added: Vec::new(),
        }
    }
}

/// Apply the overlay to the base list, preserving base order.
pub fn apply_overlay<T: OverlayItem + Clone>(base: &[T], overlay: &PresetOverlay<T>) -> Vec<T> {
    let removed: HashSet<&str> = overlay.removed_ids.iter().map(String::as_str).collect();
    let edited_by_id: HashMap<&str, &T> = overlay
        .edited
        .iter()
        .map(|item| (item.overlay_id(), item))
        .collect();

    let mut result: Vec<T> = base
        .iter()
        .filter(|item| !removed.contains(item.overlay_id()))
        .map(|item| {
            edited_by_id
                .get(item.overlay_id())
                .map(|e| (*e).clone())
                .unwrap_or_else(|| item.clone())
        })
        .collect();

    let base_ids: HashSet<&str> = base.iter().map(OverlayItem::overlay_id).collect();
    for item in &overlay.added {
        let id = item.overlay_id();
        if !removed.contains(id) && !base_ids.contains(id) {
            result.push(item.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Item {
        id: String,
        name: String,
    }

    impl OverlayItem for Item {
        fn overlay_id(&self) -> &str {
            &self.id
        }
    }

    fn item(id: &str, name: &str) -> Item {
        Item {
            id: id.to_string(),
            name: name.to_string(),
        }
    }

    #[test]
    fn empty_overlay_returns_base_unchanged() {
        let base = vec![item("a", "A"), item("b", "B")];
        let overlay = PresetOverlay::default();
        assert_eq!(apply_overlay(&base, &overlay), base);
    }

    #[test]
    fn removed_ids_hide_base_entries() {
        let base = vec![item("a", "A"), item("b", "B"), item("c", "C")];
        let overlay = PresetOverlay {
            removed_ids: vec!["b".to_string()],
            edited: vec![],
            added: vec![],
        };
        assert_eq!(
            apply_overlay(&base, &overlay),
            vec![item("a", "A"), item("c", "C")]
        );
    }

    #[test]
    fn edited_replaces_base_entries_by_id() {
        let base = vec![item("a", "A"), item("b", "B")];
        let overlay = PresetOverlay {
            removed_ids: vec![],
            edited: vec![item("b", "B2")],
            added: vec![],
        };
        assert_eq!(
            apply_overlay(&base, &overlay),
            vec![item("a", "A"), item("b", "B2")]
        );
    }

    #[test]
    fn added_appends_skipping_removed_and_existing_ids() {
        let base = vec![item("a", "A")];
        let overlay = PresetOverlay {
            removed_ids: vec!["gone".to_string()],
            edited: vec![],
            added: vec![
                item("b", "B"),
                item("a", "A-dup"),   // already in base: skipped
                item("gone", "Gone"), // removed: skipped
            ],
        };
        assert_eq!(
            apply_overlay(&base, &overlay),
            vec![item("a", "A"), item("b", "B")]
        );
    }

    #[test]
    fn combined_overlay() {
        let base = vec![item("a", "A"), item("b", "B"), item("c", "C")];
        let overlay = PresetOverlay {
            removed_ids: vec!["c".to_string()],
            edited: vec![item("a", "A2")],
            added: vec![item("d", "D")],
        };
        assert_eq!(
            apply_overlay(&base, &overlay),
            vec![item("a", "A2"), item("b", "B"), item("d", "D")]
        );
    }
}
