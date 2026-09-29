//! FFI surface for the pane layout state machine.
//!
//! Stateless JSON-snapshot protocol: every mutating op takes the current
//! snapshot (JSON of [`PaneLayoutState`]), applies one operation, and
//! returns a result JSON object. A null return means failure; the message
//! is retrievable with `supercli_last_error`.
//!
//! Result shapes:
//! - `single` / `insert`: `{"snapshot": {...}, "pane_id": "<actual pane id>", "group_id": "<group id>"}`
//!   (pane ids pass through [`PaneStableId`] canonicalization, so the
//!   actual id is returned)
//! - `close`: `{"snapshot": {...}, "pane_id": "<suggested new focus or null>", "group_id": ...}`
//! - `resize`: `{"snapshot": {...}, "applied_ratio": <f64>}`
//! - `equalize` / `swap`: `{"snapshot": {...}}`
//! - `reconcile`: `{"snapshot": {...}, "pane_id": "<pane id or null>", "group_id": ...}`
//!   (a lone surviving session is re-homed as a single-pane group; with
//!   zero eligible sessions the snapshot has no groups)
//! - `neighbor`: `"<pane id>"` (a JSON string), or null when there is no
//!   neighbor in that direction (not an error)
//! - `leaf_boxes`: `[{"pane_id":..., "x":..., "y":..., "width":..., "height":...}]`
//!
//! [`PaneLayoutState`]: supercli_client::pane_layout::PaneLayoutState
//! [`PaneStableId`]: supercli_client::pane_layout::PaneStableId

use std::collections::HashSet;
use std::os::raw::c_char;

use super::{c_str_to_str, ffi_guard, set_last_error, to_c_string};
use supercli_client::pane_layout::{
    Pane, PaneContent, PaneEdge, PaneGroup, PaneLayoutState, PaneNode, PaneSplitPath, PaneStableId,
    Rect,
};

/// Maximum accepted snapshot size (fail-closed on oversize input).
const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;

fn arg<'a>(p: *const c_char, name: &str) -> Result<&'a str, String> {
    // SAFETY: the caller contract guarantees a valid borrowed C string.
    unsafe { c_str_to_str(p) }.map_err(|e| format!("{name}: {e}"))
}

fn op_result(f: impl FnOnce() -> Result<String, String>) -> *mut c_char {
    match f() {
        Ok(json) => to_c_string(json),
        Err(e) => {
            set_last_error(e);
            std::ptr::null_mut()
        }
    }
}

fn parse_state(snapshot: &str) -> Result<PaneLayoutState, String> {
    if snapshot.len() > MAX_SNAPSHOT_BYTES {
        return Err(format!(
            "snapshot exceeds {MAX_SNAPSHOT_BYTES} bytes (fail-closed)"
        ));
    }
    serde_json::from_str(snapshot).map_err(|e| format!("invalid snapshot JSON: {e}"))
}

fn snapshot_value(state: &PaneLayoutState) -> Result<serde_json::Value, String> {
    serde_json::to_value(state).map_err(|e| format!("snapshot serialization failed: {e}"))
}

fn parse_edge(edge: &str) -> Result<PaneEdge, String> {
    match edge {
        "left" => Ok(PaneEdge::Left),
        "right" => Ok(PaneEdge::Right),
        "up" => Ok(PaneEdge::Up),
        "down" => Ok(PaneEdge::Down),
        _ => Err(format!(
            "invalid edge '{edge}': expected left|right|up|down"
        )),
    }
}

fn do_single(session_id: &str, pane_id: &str) -> Result<String, String> {
    if session_id.is_empty() {
        return Err("session_id must not be empty".to_string());
    }
    if pane_id.is_empty() {
        return Err("pane_id must not be empty".to_string());
    }
    let pane = Pane::new(
        pane_id,
        PaneContent::Session {
            id: session_id.to_string(),
        },
    );
    let actual_pane_id = pane.id.clone();
    let group = PaneGroup::new(
        PaneStableId::make(),
        actual_pane_id.clone(),
        PaneNode::Leaf(pane),
    );
    let state = PaneLayoutState::new(vec![group]);
    let group_id = state.groups[0].id.clone();
    Ok(serde_json::json!({
        "snapshot": snapshot_value(&state)?,
        "pane_id": actual_pane_id,
        "group_id": group_id,
    })
    .to_string())
}

/// Create a single-pane layout snapshot.
///
/// # Safety
///
/// `session_id` and `pane_id` must be valid borrowed C strings.
#[no_mangle]
pub unsafe extern "C" fn supercli_pane_layout_single(
    session_id: *const c_char,
    pane_id: *const c_char,
) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        op_result(|| {
            let session_id = arg(session_id, "session_id")?;
            let pane_id = arg(pane_id, "pane_id")?;
            do_single(session_id, pane_id)
        })
    })
}

fn do_insert(
    snapshot: &str,
    session_id: &str,
    target_pane_id: &str,
    edge: &str,
) -> Result<String, String> {
    let mut state = parse_state(snapshot)?;
    let edge = parse_edge(edge)?;
    let location = state
        .insert_session_splitting(session_id, target_pane_id, edge, None)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "snapshot": snapshot_value(&state)?,
        "pane_id": location.pane_id,
        "group_id": location.group_id,
    })
    .to_string())
}

/// Split `target_pane_id` on `edge`, inserting a new session pane.
///
/// # Safety
///
/// All pointer arguments must be valid borrowed C strings.
#[no_mangle]
pub unsafe extern "C" fn supercli_pane_layout_insert(
    snapshot: *const c_char,
    session_id: *const c_char,
    target_pane_id: *const c_char,
    edge: *const c_char,
) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        op_result(|| {
            let snapshot = arg(snapshot, "snapshot")?;
            let session_id = arg(session_id, "session_id")?;
            let target_pane_id = arg(target_pane_id, "target_pane_id")?;
            let edge = arg(edge, "edge")?;
            do_insert(snapshot, session_id, target_pane_id, edge)
        })
    })
}

fn do_close(snapshot: &str, pane_id: &str) -> Result<String, String> {
    let mut state = parse_state(snapshot)?;
    // Capture the surviving session panes of the target group BEFORE
    // detaching: the Rust model dissolves groups below two sessions, and the
    // single-window UI must re-home the remainder instead of losing it.
    // Pane ids are kept verbatim so caller titles/focus stay valid (they are
    // already canonical because they came from this model).
    let survivors: Vec<Pane> = state
        .groups
        .iter()
        .find(|g| g.root.leaf(pane_id).is_some())
        .map(|g| {
            g.root
                .leaves()
                .into_iter()
                .filter(|p| p.id != pane_id && p.content.session_id().is_some())
                .collect()
        })
        .unwrap_or_default();
    let change = state.detach_pane(pane_id).map_err(|e| e.to_string())?;
    if change.dissolved {
        // The Rust model dissolves groups below two sessions; the
        // single-window UI invariant is "at least one pane". Re-home the
        // surviving session as a single-pane group so the window never goes
        // empty. With no survivor this is the last pane: refuse rather than
        // returning an empty layout.
        let Some(survivor) = survivors.into_iter().next() else {
            return Err("cannot close the last pane".to_string());
        };
        let (pane_id, group_id) = rehome_lone_survivor(&mut state, survivor);
        return Ok(serde_json::json!({
            "snapshot": snapshot_value(&state)?,
            "pane_id": pane_id,
            "group_id": group_id,
        })
        .to_string());
    }
    // Suggest the first remaining leaf (preorder) as the new focus.
    let focus = state
        .groups
        .first()
        .and_then(|g| g.root.leaves().into_iter().next())
        .map(|p| p.id);
    let group_id = state
        .groups
        .first()
        .map(|g| g.id.clone())
        .unwrap_or_default();
    Ok(serde_json::json!({
        "snapshot": snapshot_value(&state)?,
        "pane_id": focus,
        "group_id": group_id,
    })
    .to_string())
}

/// Close (detach) a pane. Never leaves the window empty: when the close
/// dissolves the group, the surviving session is re-homed as a single-pane
/// group keeping its pane id. Closing the last pane of the final group is
/// an error (null return, message in `supercli_last_error`).
///
/// # Safety
///
/// All pointer arguments must be valid borrowed C strings.
#[no_mangle]
pub unsafe extern "C" fn supercli_pane_layout_close(
    snapshot: *const c_char,
    pane_id: *const c_char,
) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        op_result(|| {
            let snapshot = arg(snapshot, "snapshot")?;
            let pane_id = arg(pane_id, "pane_id")?;
            do_close(snapshot, pane_id)
        })
    })
}

fn do_resize(snapshot: &str, group_id: &str, pane_id: &str, ratio: f64) -> Result<String, String> {
    let mut state = parse_state(snapshot)?;
    // Resize the innermost split containing the pane: the parent of the
    // pane's path.
    let group = state
        .group(group_id)
        .ok_or_else(|| format!("group not found: {group_id}"))?;
    let mut path = group
        .root
        .path_to_pane(pane_id)
        .ok_or_else(|| format!("pane not found: {pane_id}"))?;
    if path.components.is_empty() {
        return Err(format!("pane {pane_id} is not inside a split"));
    }
    path.components.pop();
    let parent = PaneSplitPath::new(path.components);
    let applied = state
        .resize_split(group_id, &parent, ratio)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "snapshot": snapshot_value(&state)?,
        "applied_ratio": applied,
    })
    .to_string())
}

/// Set the divider ratio of the innermost split containing `pane_id`.
///
/// # Safety
///
/// All pointer arguments must be valid borrowed C strings.
#[no_mangle]
pub unsafe extern "C" fn supercli_pane_layout_resize(
    snapshot: *const c_char,
    group_id: *const c_char,
    pane_id: *const c_char,
    ratio: f64,
) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        op_result(|| {
            let snapshot = arg(snapshot, "snapshot")?;
            let group_id = arg(group_id, "group_id")?;
            let pane_id = arg(pane_id, "pane_id")?;
            do_resize(snapshot, group_id, pane_id, ratio)
        })
    })
}

fn do_equalize(snapshot: &str, group_id: &str) -> Result<String, String> {
    let mut state = parse_state(snapshot)?;
    state.equalize(group_id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "snapshot": snapshot_value(&state)? }).to_string())
}

/// Reset every divider ratio in the group to equal shares.
///
/// # Safety
///
/// All pointer arguments must be valid borrowed C strings.
#[no_mangle]
pub unsafe extern "C" fn supercli_pane_layout_equalize(
    snapshot: *const c_char,
    group_id: *const c_char,
) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        op_result(|| {
            let snapshot = arg(snapshot, "snapshot")?;
            let group_id = arg(group_id, "group_id")?;
            do_equalize(snapshot, group_id)
        })
    })
}

fn do_swap(snapshot: &str, pane_a: &str, pane_b: &str) -> Result<String, String> {
    let mut state = parse_state(snapshot)?;
    state
        .swap_panes(pane_a, pane_b)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "snapshot": snapshot_value(&state)? }).to_string())
}

/// Exchange the positions of two panes in the same group.
///
/// # Safety
///
/// All pointer arguments must be valid borrowed C strings.
#[no_mangle]
pub unsafe extern "C" fn supercli_pane_layout_swap(
    snapshot: *const c_char,
    pane_id_a: *const c_char,
    pane_id_b: *const c_char,
) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        op_result(|| {
            let snapshot = arg(snapshot, "snapshot")?;
            let pane_a = arg(pane_id_a, "pane_id_a")?;
            let pane_b = arg(pane_id_b, "pane_id_b")?;
            do_swap(snapshot, pane_a, pane_b)
        })
    })
}

/// The spatially adjacent pane in `direction`, as a JSON string pane id.
/// Null when there is no neighbor (not an error); null + `supercli_last_error`
/// on malformed input.
///
/// # Safety
///
/// All pointer arguments must be valid borrowed C strings.
#[no_mangle]
pub unsafe extern "C" fn supercli_pane_layout_neighbor(
    snapshot: *const c_char,
    pane_id: *const c_char,
    direction: *const c_char,
) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        let snapshot = match unsafe { c_str_to_str(snapshot) } {
            Ok(s) => s,
            Err(e) => {
                set_last_error(format!("snapshot: {e}"));
                return std::ptr::null_mut();
            }
        };
        let pane_id = match unsafe { c_str_to_str(pane_id) } {
            Ok(s) => s,
            Err(e) => {
                set_last_error(format!("pane_id: {e}"));
                return std::ptr::null_mut();
            }
        };
        let direction = match unsafe { c_str_to_str(direction) } {
            Ok(s) => s,
            Err(e) => {
                set_last_error(format!("direction: {e}"));
                return std::ptr::null_mut();
            }
        };
        let state = match parse_state(snapshot) {
            Ok(s) => s,
            Err(e) => {
                set_last_error(e);
                return std::ptr::null_mut();
            }
        };
        let edge = match parse_edge(direction) {
            Ok(e) => e,
            Err(e) => {
                set_last_error(e);
                return std::ptr::null_mut();
            }
        };
        match state.spatial_neighbor(pane_id, edge) {
            Some(pane) => {
                let json = serde_json::to_string(&pane.id).unwrap_or_default();
                to_c_string(json)
            }
            None => std::ptr::null_mut(),
        }
    })
}

fn do_reconcile(snapshot: &str, eligible_ids_json: &str) -> Result<String, String> {
    let mut state = parse_state(snapshot)?;
    let ids: Vec<String> = serde_json::from_str(eligible_ids_json)
        .map_err(|e| format!("invalid eligible_ids JSON: {e}"))?;
    let eligible: HashSet<String> = ids.into_iter().collect();
    // Capture eligible session panes BEFORE reconciling: the Rust model
    // dissolves groups below two sessions, and the single-window UI re-homes
    // a lone survivor instead of going empty.
    let survivors: Vec<Pane> = state
        .groups
        .iter()
        .flat_map(|g| g.root.leaves())
        .filter(|p| {
            p.content
                .session_id()
                .is_some_and(|id| eligible.contains(id))
        })
        .collect();
    state.reconcile(&eligible);
    let (pane_id, group_id) = if state.groups.is_empty() && survivors.len() == 1 {
        let (pane_id, group_id) =
            rehome_lone_survivor(&mut state, survivors.into_iter().next().unwrap());
        (Some(pane_id), group_id)
    } else {
        (
            None,
            state
                .groups
                .first()
                .map(|g| g.id.clone())
                .unwrap_or_default(),
        )
    };
    Ok(serde_json::json!({
        "snapshot": snapshot_value(&state)?,
        "pane_id": pane_id,
        "group_id": group_id,
    })
    .to_string())
}

/// Re-home a lone surviving session as a single-pane group after an op
/// dissolved its group (the Rust model needs two sessions per group; the
/// single-window UI invariant is "at least one pane"). The pane keeps its
/// id so caller titles/focus stay valid. Returns (pane_id, group_id).
fn rehome_lone_survivor(state: &mut PaneLayoutState, survivor: Pane) -> (String, String) {
    let pane_id = survivor.id.clone();
    let group = PaneGroup {
        id: PaneStableId::make(),
        representative_pane_id: pane_id.clone(),
        root: PaneNode::Leaf(survivor),
        pre_launcher_root: None,
    };
    let group_id = group.id.clone();
    state.groups.push(group);
    (pane_id, group_id)
}

/// Drop sessions no longer in `eligible_ids` (JSON array of strings),
/// collapsing and dissolving as the model dictates. A lone surviving
/// session is re-homed as a single-pane group (single-window UI invariant);
/// with zero eligible sessions the snapshot comes back with no groups.
///
/// # Safety
///
/// All pointer arguments must be valid borrowed C strings.
#[no_mangle]
pub unsafe extern "C" fn supercli_pane_layout_reconcile(
    snapshot: *const c_char,
    eligible_ids_json: *const c_char,
) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        op_result(|| {
            let snapshot = arg(snapshot, "snapshot")?;
            let eligible_ids_json = arg(eligible_ids_json, "eligible_ids_json")?;
            do_reconcile(snapshot, eligible_ids_json)
        })
    })
}

fn do_leaf_boxes(
    snapshot: &str,
    group_id: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<String, String> {
    let state = parse_state(snapshot)?;
    let group = state
        .group(group_id)
        .ok_or_else(|| format!("group not found: {group_id}"))?;
    let slots = group.root.leaf_slots(Rect::new(x, y, width, height));
    let boxes: Vec<serde_json::Value> = slots
        .iter()
        .map(|(pane, rect)| {
            serde_json::json!({
                "pane_id": pane.id,
                "x": rect.x,
                "y": rect.y,
                "width": rect.width,
                "height": rect.height,
            })
        })
        .collect();
    serde_json::to_string(&boxes).map_err(|e| format!("boxes serialization failed: {e}"))
}

/// Leaf geometry for rendering/hit-testing: JSON array of
/// `{pane_id, x, y, width, height}` in the given bounds.
///
/// # Safety
///
/// All pointer arguments must be valid borrowed C strings.
#[no_mangle]
pub unsafe extern "C" fn supercli_pane_layout_leaf_boxes(
    snapshot: *const c_char,
    group_id: *const c_char,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        op_result(|| {
            let snapshot = arg(snapshot, "snapshot")?;
            let group_id = arg(group_id, "group_id")?;
            do_leaf_boxes(snapshot, group_id, x, y, width, height)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};

    fn c(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    fn single_snapshot() -> (String, String, String) {
        let sid = c("sess-1");
        let pid = c("pane-1");
        let p = unsafe { supercli_pane_layout_single(sid.as_ptr(), pid.as_ptr()) };
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(p) };
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let snapshot = v["snapshot"].to_string();
        let pane_id = v["pane_id"].as_str().unwrap().to_string();
        let group_id = v["group_id"].as_str().unwrap().to_string();
        (snapshot, pane_id, group_id)
    }

    #[test]
    fn single_creates_one_pane_snapshot() {
        let (snapshot, pane_id, group_id) = single_snapshot();
        assert!(!pane_id.is_empty());
        assert!(!group_id.is_empty());
        let state: PaneLayoutState = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(state.groups.len(), 1);
        assert_eq!(state.groups[0].root.leaves().len(), 1);
    }

    #[test]
    fn snapshot_json_shape_is_stable() {
        let (snapshot, _, _) = single_snapshot();
        let v: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
        let root = &v["groups"][0]["root"];
        assert_eq!(root["kind"], "leaf");
        assert_eq!(root["content"]["kind"], "session");
        assert_eq!(root["content"]["id"], "sess-1");
    }

    #[test]
    fn insert_splits_target_pane() {
        let (snapshot, pane_id, _) = single_snapshot();
        let snap_c = c(&snapshot);
        let sess_c = c("sess-2");
        let target_c = c(&pane_id);
        let edge_c = c("right");
        let p = unsafe {
            supercli_pane_layout_insert(
                snap_c.as_ptr(),
                sess_c.as_ptr(),
                target_c.as_ptr(),
                edge_c.as_ptr(),
            )
        };
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(p) };
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let state: PaneLayoutState = serde_json::from_value(v["snapshot"].clone()).unwrap();
        assert_eq!(state.groups[0].root.leaves().len(), 2);
        assert!(!v["pane_id"].as_str().unwrap().is_empty());
    }

    #[test]
    fn insert_invalid_edge_fails_null() {
        let (snapshot, pane_id, _) = single_snapshot();
        crate::supercli_clear_error();
        let snap_c = c(&snapshot);
        let sess_c = c("sess-2");
        let target_c = c(&pane_id);
        let edge_c = c("diagonal");
        let p = unsafe {
            supercli_pane_layout_insert(
                snap_c.as_ptr(),
                sess_c.as_ptr(),
                target_c.as_ptr(),
                edge_c.as_ptr(),
            )
        };
        assert!(p.is_null());
        let e = crate::supercli_last_error();
        assert!(!e.is_null());
        let msg = unsafe { CStr::from_ptr(e) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(e) };
        assert!(msg.contains("invalid edge"), "unexpected: {msg}");
    }

    #[test]
    fn close_last_two_panes_recreates_single() {
        let (snapshot, pane_id, _) = single_snapshot();
        // Insert a second pane.
        let snap_c = c(&snapshot);
        let sess_c = c("sess-2");
        let target_c = c(&pane_id);
        let edge_c = c("right");
        let p = unsafe {
            supercli_pane_layout_insert(
                snap_c.as_ptr(),
                sess_c.as_ptr(),
                target_c.as_ptr(),
                edge_c.as_ptr(),
            )
        };
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(p) };
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let snap2 = v["snapshot"].to_string();
        let new_pane = v["pane_id"].as_str().unwrap().to_string();
        // Close the new pane: the group would dissolve, so FFI recreates one.
        let snap2_c = c(&snap2);
        let close_c = c(&new_pane);
        let p2 = unsafe { supercli_pane_layout_close(snap2_c.as_ptr(), close_c.as_ptr()) };
        assert!(!p2.is_null());
        let s2 = unsafe { CStr::from_ptr(p2) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(p2) };
        let v2: serde_json::Value = serde_json::from_str(&s2).unwrap();
        let state: PaneLayoutState = serde_json::from_value(v2["snapshot"].clone()).unwrap();
        assert_eq!(state.groups.len(), 1);
        assert_eq!(state.groups[0].root.leaves().len(), 1);
        // The surviving pane keeps its id and session.
        let survivor = &state.groups[0].root.leaves()[0];
        assert_eq!(survivor.id, pane_id);
        assert_eq!(
            survivor.content.session_id(),
            Some("sess-1"),
            "survivor must be the remaining session"
        );
        assert_eq!(
            v2["pane_id"].as_str().unwrap(),
            pane_id,
            "returned pane id must be the survivor"
        );
    }

    #[test]
    fn close_last_pane_fails() {
        let (snapshot, pane_id, _) = single_snapshot();
        crate::supercli_clear_error();
        let snap_c = c(&snapshot);
        let close_c = c(&pane_id);
        let p = unsafe { supercli_pane_layout_close(snap_c.as_ptr(), close_c.as_ptr()) };
        assert!(p.is_null());
        let e = crate::supercli_last_error();
        assert!(!e.is_null());
        let msg = unsafe { CStr::from_ptr(e) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(e) };
        assert!(msg.contains("last pane"), "unexpected: {msg}");
    }

    #[test]
    fn resize_equalize_swap_roundtrip() {
        let (snapshot, pane_id, group_id) = single_snapshot();
        let snap_c = c(&snapshot);
        let sess_c = c("sess-2");
        let target_c = c(&pane_id);
        let edge_c = c("right");
        let p = unsafe {
            supercli_pane_layout_insert(
                snap_c.as_ptr(),
                sess_c.as_ptr(),
                target_c.as_ptr(),
                edge_c.as_ptr(),
            )
        };
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(p) };
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let snap2 = v["snapshot"].to_string();
        let pane_b = v["pane_id"].as_str().unwrap().to_string();

        // Resize the split containing pane_b.
        let snap2_c = c(&snap2);
        let gid_c = c(&group_id);
        let pb_c = c(&pane_b);
        let p2 = unsafe {
            supercli_pane_layout_resize(snap2_c.as_ptr(), gid_c.as_ptr(), pb_c.as_ptr(), 0.7)
        };
        assert!(!p2.is_null());
        let s2 = unsafe { CStr::from_ptr(p2) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(p2) };
        let v2: serde_json::Value = serde_json::from_str(&s2).unwrap();
        assert!((v2["applied_ratio"].as_f64().unwrap() - 0.7).abs() < 1e-9);

        // Equalize.
        let snap3 = v2["snapshot"].to_string();
        let snap3_c = c(&snap3);
        let p3 = unsafe { supercli_pane_layout_equalize(snap3_c.as_ptr(), gid_c.as_ptr()) };
        assert!(!p3.is_null());
        unsafe { crate::supercli_string_free(p3) };

        // Swap.
        let snap4_c = c(&snap3);
        let pa_c = c(&pane_id);
        let p4 =
            unsafe { supercli_pane_layout_swap(snap4_c.as_ptr(), pa_c.as_ptr(), pb_c.as_ptr()) };
        assert!(!p4.is_null());
        unsafe { crate::supercli_string_free(p4) };
    }

    #[test]
    fn neighbor_finds_spatially_adjacent_pane() {
        let (snapshot, pane_id, _) = single_snapshot();
        let snap_c = c(&snapshot);
        let sess_c = c("sess-2");
        let target_c = c(&pane_id);
        let edge_c = c("right");
        let p = unsafe {
            supercli_pane_layout_insert(
                snap_c.as_ptr(),
                sess_c.as_ptr(),
                target_c.as_ptr(),
                edge_c.as_ptr(),
            )
        };
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(p) };
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let snap2 = v["snapshot"].to_string();
        let pane_b = v["pane_id"].as_str().unwrap().to_string();

        // pane_b is right of pane_id.
        let snap2_c = c(&snap2);
        let pa_c = c(&pane_id);
        let dir_c = c("right");
        let pn = unsafe {
            supercli_pane_layout_neighbor(snap2_c.as_ptr(), pa_c.as_ptr(), dir_c.as_ptr())
        };
        assert!(!pn.is_null());
        let ns = unsafe { CStr::from_ptr(pn) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(pn) };
        let neighbor_id: String = serde_json::from_str(&ns).unwrap();
        assert_eq!(neighbor_id, pane_b);

        // Nothing to the left of pane_id.
        let dir_l = c("left");
        let pn2 = unsafe {
            supercli_pane_layout_neighbor(snap2_c.as_ptr(), pa_c.as_ptr(), dir_l.as_ptr())
        };
        assert!(pn2.is_null());
    }

    #[test]
    fn reconcile_drops_ineligible_sessions() {
        let (snapshot, pane_id, _) = single_snapshot();
        let snap_c = c(&snapshot);
        let sess_c = c("sess-2");
        let target_c = c(&pane_id);
        let edge_c = c("right");
        let p = unsafe {
            supercli_pane_layout_insert(
                snap_c.as_ptr(),
                sess_c.as_ptr(),
                target_c.as_ptr(),
                edge_c.as_ptr(),
            )
        };
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(p) };
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let snap2 = v["snapshot"].to_string();

        // Reconcile keeping only sess-1: the group would dissolve (Rust model
        // invariant), so the FFI re-homes the lone survivor as a single-pane
        // group keeping its pane id.
        let snap2_c = c(&snap2);
        let elig_c = c(r#"["sess-1"]"#);
        let p2 = unsafe { supercli_pane_layout_reconcile(snap2_c.as_ptr(), elig_c.as_ptr()) };
        assert!(!p2.is_null());
        let s2 = unsafe { CStr::from_ptr(p2) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(p2) };
        let v2: serde_json::Value = serde_json::from_str(&s2).unwrap();
        let state: PaneLayoutState = serde_json::from_value(v2["snapshot"].clone()).unwrap();
        assert_eq!(state.groups.len(), 1);
        let survivor = &state.groups[0].root.leaves()[0];
        assert_eq!(survivor.id, pane_id);
        assert_eq!(survivor.content.session_id(), Some("sess-1"));
    }

    #[test]
    fn reconcile_empty_eligible_empties_groups() {
        let (snapshot, _, _) = single_snapshot();
        let snap_c = c(&snapshot);
        let elig_c = c(r#"[]"#);
        let p = unsafe { supercli_pane_layout_reconcile(snap_c.as_ptr(), elig_c.as_ptr()) };
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(p) };
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let state: PaneLayoutState = serde_json::from_value(v["snapshot"].clone()).unwrap();
        assert!(state.groups.is_empty());
    }

    #[test]
    fn leaf_boxes_covers_unit_bounds() {
        let (snapshot, _, group_id) = single_snapshot();
        let snap_c = c(&snapshot);
        let gid_c = c(&group_id);
        let p = unsafe {
            supercli_pane_layout_leaf_boxes(snap_c.as_ptr(), gid_c.as_ptr(), 0.0, 0.0, 800.0, 600.0)
        };
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { crate::supercli_string_free(p) };
        let boxes: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(boxes.as_array().unwrap().len(), 1);
        let b = &boxes[0];
        assert_eq!(b["width"].as_f64().unwrap(), 800.0);
        assert_eq!(b["height"].as_f64().unwrap(), 600.0);
    }

    #[test]
    fn malformed_snapshot_fails_with_error() {
        crate::supercli_clear_error();
        let bad_c = c("{not json");
        let pid_c = c("pane-1");
        let p = unsafe { supercli_pane_layout_close(bad_c.as_ptr(), pid_c.as_ptr()) };
        assert!(p.is_null());
        let e = crate::supercli_last_error();
        assert!(!e.is_null());
        unsafe { crate::supercli_string_free(e) };
    }

    #[test]
    fn null_pointers_fail_closed() {
        crate::supercli_clear_error();
        let p = unsafe { supercli_pane_layout_close(std::ptr::null(), std::ptr::null()) };
        assert!(p.is_null());
        let e = crate::supercli_last_error();
        assert!(!e.is_null());
        unsafe { crate::supercli_string_free(e) };
    }
}
