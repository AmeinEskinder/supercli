//! Wire contract: the Host is the source of truth for field names.
//!
//! This test verifies that `protocol/fixtures/*.json` uses the same wire
//! field names that the running Host (supercli-serve + supercli-core)
//! actually sends and accepts. If the Host changes a field name, this test
//! breaks — forcing the fixtures, Rust DTOs, and Dart decoders to be
//! updated in lockstep.
//!
//! Source-of-truth references:
//! - `isGroup`: `crates/supercli-serve/src/sessions.rs` (sidebar projects
//!   insert `"isGroup"`); Swift `RemoteControlProtocol` also uses `isGroup`.
//! - `wid`: `crates/supercli-core/src/controller_api.rs` (`write_session`
//!   reads `body.get("wid")`); Swift `writeID` encodes as `wid`.
//! - `columns`: `crates/supercli-core/src/controller_api.rs`
//!   (`resize_session` reads `body.get("columns")`); Swift uses `columns`.
//! - `column` (cell runs): Swift `RemoteTerminalCellRun` uses `column`
//!   (the Host does not emit viewport patches).

use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/supercli-serve -> repo root -> protocol/fixtures
    dir.pop();
    dir.pop();
    dir.join("protocol").join("fixtures")
}

fn load_fixture(name: &str) -> serde_json::Value {
    let path = fixtures_dir().join(name);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&content).unwrap_or_else(|e| panic!("failed to parse {name}: {e}"))
}

#[test]
fn project_summary_uses_host_is_group() {
    let v = load_fixture("project_summary.json");
    // The Host sends `isGroup` (sessions.rs). The fixture must match.
    assert_eq!(
        v.get("isGroup"),
        Some(&serde_json::Value::Bool(true)),
        "project_summary.json must use `isGroup` (Host wire name)"
    );
    // The old Rust-canonical `isFolder` must be gone.
    assert!(
        v.get("isFolder").is_none(),
        "project_summary.json must not contain `isFolder`"
    );
}

#[test]
fn terminal_write_request_uses_host_wid() {
    let v = load_fixture("terminal_write_request.json");
    // The Host reads `wid` (controller_api.rs, remote_server.rs).
    assert_eq!(
        v.get("wid"),
        Some(&serde_json::Value::String("write-123".to_string())),
        "terminal_write_request.json must use `wid` (Host wire name)"
    );
    assert!(
        v.get("idempotencyKey").is_none(),
        "terminal_write_request.json must not contain `idempotencyKey`"
    );
    assert!(
        v.get("writeID").is_none(),
        "terminal_write_request.json must not contain `writeID`"
    );
}

#[test]
fn terminal_resize_request_uses_host_columns() {
    let v = load_fixture("terminal_resize_request.json");
    // The Host reads `columns` (controller_api.rs `resize_session`).
    assert_eq!(
        v.get("columns"),
        Some(&serde_json::Value::Number(120.into())),
        "terminal_resize_request.json must use `columns` (Host wire name)"
    );
    assert!(
        v.get("cols").is_none(),
        "terminal_resize_request.json must not contain `cols`"
    );
}

#[test]
fn viewport_patch_cell_run_uses_column() {
    let v = load_fixture("viewport_patch.json");
    let runs = v
        .get("changedRuns")
        .and_then(serde_json::Value::as_array)
        .expect("viewport_patch.json must have changedRuns array");
    for run in runs {
        assert!(
            run.get("column").is_some(),
            "cell run must use `column` (Swift wire name)"
        );
        assert!(
            run.get("startColumn").is_none(),
            "cell run must not contain `startColumn`"
        );
    }

    // Same for the standalone terminal_cell_run fixture.
    let v = load_fixture("terminal_cell_run.json");
    assert!(
        v.get("column").is_some(),
        "terminal_cell_run.json must use `column`"
    );
    assert!(
        v.get("startColumn").is_none(),
        "terminal_cell_run.json must not contain `startColumn`"
    );
}
