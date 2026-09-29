//! Preset writes to the shared `~/.supercli/app-state.json` contract.
//!
//! Ported from `clients/legacy/native/SupercliNative/Sources/SupercliNative/PresetStateFile.swift`.
//! Since the overlay migration (2026-08-08) the file's `presets` array — its
//! order included — is the single source of truth for the flat preset list,
//! shared with the terminal UI (`supercli`), which edits the same file.
//! Edits happen at the raw-JSON level so keys this build does not model —
//! top-level or per-preset — survive a rewrite.
//!
//! The lock/edit discipline (`withExclusiveLock`, `edit`, `edit(at:)`,
//! `lockedEdit` in Swift) is **not** duplicated here: it already exists as
//! the single implementation in [`crate::app_state`] (`lock_exclusive`,
//! `edit`, `edit_at`), which this module's callers use directly. This module
//! ports only the preset-specific pieces that had no Rust equivalent:
//! the migration marker, the duplicate-collapse repair, and the raw
//! preset-dict helpers.

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::presets::Preset;

/// Top-level app-state.json marker set by the one-time overlay fold.
/// Once present, both UIs treat the file as the whole preset truth and
/// ignore the legacy `supercli.native.presets`/`presetOrder` defaults.
pub const MIGRATED_KEY: &str = "native_preset_overlay_migrated";

/// Collapse rows that are exact duplicates of an earlier row — same
/// label, command, and project — keeping the first occurrence and its
/// slot. A star on any dropped copy carries over so a user never loses a
/// favorite. Rows that differ in label or command are never touched:
/// this is the one-time repair for 0.4.0's client-mode Add bug, which
/// appended a copy per click, not a deduplication policy.
///
/// Returns the kept rows and the number removed. A row whose command is
/// empty is never treated as a duplicate (mirrors the Swift
/// `!command.isEmpty` guard on the collapse branch).
pub fn collapse_exact_duplicates(
    rows: Vec<Map<String, Value>>,
) -> (Vec<Map<String, Value>>, usize) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut kept: Vec<Map<String, Value>> = Vec::with_capacity(rows.len());
    let mut removed = 0usize;
    for row in rows {
        let label = row
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let command = row
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let project = row.get("project_id").and_then(Value::as_str).unwrap_or("");
        let key = format!("{label}\0{command}\0{project}");
        if let Some(&index) = seen.get(&key) {
            if !command.is_empty() {
                if row.get("quick_launch").and_then(Value::as_bool) == Some(true) {
                    kept[index].insert("quick_launch".to_string(), Value::Bool(true));
                }
                removed += 1;
                continue;
            }
        }
        seen.insert(key, kept.len());
        kept.push(row);
    }
    (kept, removed)
}

/// The file's raw preset dicts (empty when absent/unreadable).
/// Non-dict entries in the `presets` array are skipped, mirroring Swift's
/// `compactMap { $0 as? [String: Any] }`.
pub fn raw_presets(object: &Map<String, Value>) -> Vec<Map<String, Value>> {
    object
        .get("presets")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_object).cloned().collect())
        .unwrap_or_default()
}

/// Write `preset`'s modelled fields into a raw dict, keeping any keys
/// this build does not model (`project_id`, Tauri-era extras).
pub fn apply_preset(preset: &Preset, mut dict: Map<String, Value>) -> Map<String, Value> {
    dict.insert("id".to_string(), Value::String(preset.id.clone()));
    dict.insert("label".to_string(), Value::String(preset.label.clone()));
    dict.insert("command".to_string(), Value::String(preset.command.clone()));
    dict.insert("enabled".to_string(), Value::Bool(preset.enabled));
    dict.insert("quick_launch".to_string(), Value::Bool(preset.quick_launch));
    dict
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dict(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn migrated_key_matches_swift() {
        assert_eq!(MIGRATED_KEY, "native_preset_overlay_migrated");
    }

    #[test]
    fn collapse_removes_exact_duplicates_keeping_first() {
        let rows = vec![
            dict(json!({"id": "a", "label": "Build", "command": "make", "project_id": "p1"})),
            dict(json!({"id": "b", "label": "Build", "command": "make", "project_id": "p1"})),
            dict(json!({"id": "c", "label": "Test", "command": "make test", "project_id": "p1"})),
        ];
        let (kept, removed) = collapse_exact_duplicates(rows);
        assert_eq!(removed, 1);
        assert_eq!(kept.len(), 2);
        // First occurrence keeps its slot and id.
        assert_eq!(kept[0]["id"], "a");
        assert_eq!(kept[1]["id"], "c");
    }

    #[test]
    fn collapse_carries_star_from_dropped_copy() {
        let rows = vec![
            dict(json!({"id": "a", "label": "Build", "command": "make", "project_id": "p1"})),
            dict(
                json!({"id": "b", "label": "Build", "command": "make", "project_id": "p1", "quick_launch": true}),
            ),
        ];
        let (kept, removed) = collapse_exact_duplicates(rows);
        assert_eq!(removed, 1);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0]["quick_launch"], true);
    }

    #[test]
    fn collapse_ignores_label_and_command_whitespace() {
        let rows = vec![
            dict(json!({"id": "a", "label": "Build", "command": "make", "project_id": "p1"})),
            dict(json!({"id": "b", "label": "  Build  ", "command": "make ", "project_id": "p1"})),
        ];
        let (kept, removed) = collapse_exact_duplicates(rows);
        assert_eq!(removed, 1);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn collapse_keeps_rows_with_different_project() {
        let rows = vec![
            dict(json!({"id": "a", "label": "Build", "command": "make", "project_id": "p1"})),
            dict(json!({"id": "b", "label": "Build", "command": "make", "project_id": "p2"})),
        ];
        let (kept, removed) = collapse_exact_duplicates(rows);
        assert_eq!(removed, 0);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn collapse_never_dedups_empty_command() {
        // Mirrors the Swift `!command.isEmpty` guard: two rows with empty
        // commands are both kept even though their keys match.
        let rows = vec![
            dict(json!({"id": "a", "label": "Build", "command": "", "project_id": "p1"})),
            dict(json!({"id": "b", "label": "Build", "command": "", "project_id": "p1"})),
        ];
        let (kept, removed) = collapse_exact_duplicates(rows);
        assert_eq!(removed, 0);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn raw_presets_extracts_dicts_and_skips_junk() {
        let object = dict(json!({
            "presets": [
                {"id": "a", "label": "Build"},
                "not a dict",
                42,
                {"id": "b", "label": "Test"},
            ],
            "theme": "midnight",
        }));
        let presets = raw_presets(&object);
        assert_eq!(presets.len(), 2);
        assert_eq!(presets[0]["id"], "a");
        assert_eq!(presets[1]["id"], "b");
    }

    #[test]
    fn raw_presets_empty_when_absent() {
        let object = dict(json!({"theme": "midnight"}));
        assert!(raw_presets(&object).is_empty());
    }

    #[test]
    fn apply_preset_writes_modelled_fields_and_keeps_unknown_keys() {
        let preset = Preset {
            id: "p1".to_string(),
            label: "Build".to_string(),
            command: "make".to_string(),
            enabled: false,
            quick_launch: true,
        };
        let dict = dict(json!({
            "project_id": "proj-9",
            "tauri_extra": {"x": 1},
            "label": "Old",
        }));
        let out = apply_preset(&preset, dict);
        assert_eq!(out["id"], "p1");
        assert_eq!(out["label"], "Build");
        assert_eq!(out["command"], "make");
        assert_eq!(out["enabled"], false);
        assert_eq!(out["quick_launch"], true);
        // Unmodelled keys survive.
        assert_eq!(out["project_id"], "proj-9");
        assert_eq!(out["tauri_extra"]["x"], 1);
    }
}
