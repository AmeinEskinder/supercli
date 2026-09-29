//! C ABI for the Supercli client logic.
//!
//! This crate is the single FFI surface the Dart/gpuidart UI calls into,
//! per Amein's rule: one implementation, in Rust; Dart keeps only UI.
//!
//! # Memory contract
//!
//! Functions returning `*mut c_char` hand ownership to the caller; the caller
//! MUST release the string with [`supercli_string_free`]. All input pointers
//! are borrowed for the duration of the call. Null inputs are treated as
//! empty/absent (fail-closed where a value is required).

use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_uchar};
use std::path::PathBuf;

use supercli_client::{viewer_presence, workspace_pool};
use supercli_core::{presets, runtime_catalog, terminal_drop_maps, workspace_registry};

// ---------------------------------------------------------------------------
// String helpers
// ---------------------------------------------------------------------------

/// Release a string returned by this library.
#[no_mangle]
pub unsafe extern "C" fn supercli_string_free(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s)) };
    }
}

fn to_c_string(s: String) -> *mut c_char {
    CString::new(s).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut())
}

unsafe fn c_str_to_str<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(p) }.to_str().unwrap_or("")
}

unsafe fn c_bytes<'a>(p: *const c_uchar, len: usize) -> &'a [u8] {
    if p.is_null() {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(p, len) }
}

// ---------------------------------------------------------------------------
// Runtime catalog
// ---------------------------------------------------------------------------

/// JSON array of all runtime descriptors: `[{id, slug, legacy_slug, label,
/// supports_quick_launch, icon, command_aliases, ...}]`.
#[no_mangle]
pub unsafe extern "C" fn supercli_runtime_catalog_json() -> *mut c_char {
    let catalog = runtime_catalog::builtin_runtime_catalog();
    let runtimes: Vec<serde_json::Value> = catalog
        .descriptors()
        .iter()
        .map(|d| {
            serde_json::json!({
                "id": d.id,
                "slug": d.slug,
                "legacy_slug": d.legacy_slug,
                "label": d.label,
                "supports_quick_launch": d.supports_quick_launch,
                "icon": d.display.icon,
                "kind": format!("{:?}", d.display.kind).to_lowercase(),
                "command_aliases": d.detection.command_aliases,
                "process_aliases": d.detection.process_aliases,
                "install_command": d.install.as_ref().and_then(|i| i.command.clone()),
                "install_url": d.install.as_ref().map(|i| i.official_url.clone()),
            })
        })
        .collect();
    to_c_string(serde_json::to_string(&runtimes).unwrap_or_default())
}

/// JSON object of the runtime matching `id` (stable id or legacy slug), or
/// null when unknown.
#[no_mangle]
pub unsafe extern "C" fn supercli_runtime_by_id_json(id: *const c_char) -> *mut c_char {
    let id = unsafe { c_str_to_str(id) };
    let catalog = runtime_catalog::builtin_runtime_catalog();
    let d = catalog
        .by_id(id)
        .or_else(|| catalog.by_legacy_slug(id));
    match d {
        Some(d) => to_c_string(
            serde_json::json!({
                "id": d.id,
                "slug": d.slug,
                "legacy_slug": d.legacy_slug,
                "label": d.label,
                "supports_quick_launch": d.supports_quick_launch,
                "icon": d.display.icon,
            })
            .to_string(),
        ),
        None => std::ptr::null_mut(),
    }
}

/// Legacy slug of the runtime that launches `command`, or null.
#[no_mangle]
pub unsafe extern "C" fn supercli_runtime_detect_tool(command: *const c_char) -> *mut c_char {
    let command = unsafe { c_str_to_str(command) };
    let catalog = runtime_catalog::builtin_runtime_catalog();
    match presets::descriptor_for_command(catalog, command) {
        Some(d) => to_c_string(d.legacy_slug.clone()),
        None => std::ptr::null_mut(),
    }
}

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

/// 1 when `command` resolves to a quick-launchable runtime, else 0.
#[no_mangle]
pub unsafe extern "C" fn supercli_preset_tool_is_quick_launchable(
    command: *const c_char,
) -> c_uchar {
    let command = unsafe { c_str_to_str(command) };
    let catalog = runtime_catalog::builtin_runtime_catalog();
    u8::from(
        presets::QuickPresetTool::detect(catalog, command).is_some(),
    )
}

/// Display name for a tool's legacy slug, or null when unknown.
#[no_mangle]
pub unsafe extern "C" fn supercli_preset_tool_display_name(
    legacy_slug: *const c_char,
) -> *mut c_char {
    let slug = unsafe { c_str_to_str(legacy_slug) };
    let catalog = runtime_catalog::builtin_runtime_catalog();
    match presets::SetupTool::new(catalog, slug) {
        Some(t) => to_c_string(t.display_name(catalog)),
        None => std::ptr::null_mut(),
    }
}

// ---------------------------------------------------------------------------
// Workspace pool (pure functions)
// ---------------------------------------------------------------------------

/// Exponential backoff delay in ms for `consecutive_failures` (>= 1).
#[no_mangle]
pub extern "C" fn supercli_pool_backoff_delay_ms(consecutive_failures: u32) -> u64 {
    workspace_pool::backoff_delay_ms(
        consecutive_failures,
        workspace_pool::policy::BACKOFF_BASE_MS,
        workspace_pool::policy::BACKOFF_CAP_MS,
    )
}

/// Pool policy tunables as JSON: `{poll_interval_ms, backoff_base_ms,
/// backoff_cap_ms, maintenance_interval_ms, max_live_remote_connections,
/// organization_hold_secs}`.
#[no_mangle]
pub extern "C" fn supercli_pool_policy_json() -> *mut c_char {
    to_c_string(
        serde_json::json!({
            "poll_interval_ms": workspace_pool::policy::POLL_INTERVAL_MS,
            "backoff_base_ms": workspace_pool::policy::BACKOFF_BASE_MS,
            "backoff_cap_ms": workspace_pool::policy::BACKOFF_CAP_MS,
            "maintenance_interval_ms": workspace_pool::policy::MAINTENANCE_INTERVAL_MS,
            "immediate_refresh_throttle_secs": workspace_pool::policy::IMMEDIATE_REFRESH_THROTTLE_SECS,
            "max_live_remote_connections": workspace_pool::policy::MAX_LIVE_REMOTE_CONNECTIONS,
            "organization_hold_secs": workspace_pool::policy::ORGANIZATION_HOLD_SECS,
        })
        .to_string(),
    )
}

// ---------------------------------------------------------------------------
// Workspace registry
// ---------------------------------------------------------------------------

/// URL-safe slug for a workspace display name.
#[no_mangle]
pub unsafe extern "C" fn supercli_registry_slugify(name: *const c_char) -> *mut c_char {
    let name = unsafe { c_str_to_str(name) };
    to_c_string(workspace_registry::slugify(name))
}

/// `local:` order key for a workspace home path.
#[no_mangle]
pub unsafe extern "C" fn supercli_registry_local_key(home: *const c_char) -> *mut c_char {
    let home = unsafe { c_str_to_str(home) };
    to_c_string(workspace_registry::list_order::local_key(PathBuf::from(home).as_path()))
}

/// `host:` order key for a paired host id.
#[no_mangle]
pub unsafe extern "C" fn supercli_registry_paired_key(host_id: *const c_char) -> *mut c_char {
    let host_id = unsafe { c_str_to_str(host_id) };
    to_c_string(workspace_registry::list_order::paired_key(host_id))
}

// ---------------------------------------------------------------------------
// Viewer presence
// ---------------------------------------------------------------------------

/// Parse one presence feed file; returns JSON `{session_id: [{id,
/// device_id, display_name, last_seen_ms}]}`. Malformed input -> `{}`.
#[no_mangle]
pub unsafe extern "C" fn supercli_presence_parse(
    data: *const c_uchar,
    len: usize,
    source: *const c_char,
) -> *mut c_char {
    let data = unsafe { c_bytes(data, len) };
    let source = unsafe { c_str_to_str(source) };
    let parsed = viewer_presence::parse_presence(data, source);
    let out: HashMap<String, Vec<serde_json::Value>> = parsed
        .into_iter()
        .map(|(session, viewers)| {
            let viewers = viewers
                .into_iter()
                .map(|v| {
                    serde_json::json!({
                        "id": v.id,
                        "device_id": v.device_id,
                        "display_name": v.display_name,
                        "last_seen_ms": v.last_seen_ms,
                    })
                })
                .collect();
            (session, viewers)
        })
        .collect();
    to_c_string(serde_json::to_string(&out).unwrap_or_default())
}

/// Display name for a `device` ("Name (id)") / `ip` pair.
#[no_mangle]
pub unsafe extern "C" fn supercli_presence_display_name(
    device: *const c_char,
    ip: *const c_char,
) -> *mut c_char {
    let device = unsafe { c_str_to_str(device) };
    let ip = unsafe { c_str_to_str(ip) };
    let device = if device.is_empty() { None } else { Some(device) };
    let ip = if ip.is_empty() { None } else { Some(ip) };
    to_c_string(viewer_presence::display_name_from_device(device, ip))
}

// ---------------------------------------------------------------------------
// Terminal drop maps
// ---------------------------------------------------------------------------

/// 1 when the drop-target map JSON accepts a drop at `(row, column)` at
/// `now_ms`, else 0. Malformed/oversize input -> 0.
#[no_mangle]
pub unsafe extern "C" fn supercli_drop_map_accepts(
    json: *const c_uchar,
    len: usize,
    row: u32,
    column: u32,
    now_ms: u64,
) -> c_uchar {
    let json = unsafe { c_bytes(json, len) };
    let accepts = terminal_drop_maps::DropTargetMap::from_json_bytes(json)
        .is_ok_and(|m| m.accepts(row, column, now_ms));
    u8::from(accepts)
}

/// Host-local path for the path-drag map JSON at `(row, column)` at
/// `now_ms`, or null when unmapped/stale/malformed.
#[no_mangle]
pub unsafe extern "C" fn supercli_path_drag_map_path_at(
    json: *const c_uchar,
    len: usize,
    row: u32,
    column: u32,
    now_ms: u64,
) -> *mut c_char {
    let json = unsafe { c_bytes(json, len) };
    match terminal_drop_maps::PathDragMap::from_json_bytes(json) {
        Ok(m) => match m.path_at(row, column, now_ms) {
            Some(p) => to_c_string(p.to_string()),
            None => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

/// 1 when `device` ("Name (id)") carries a stable device id, else 0.
#[no_mangle]
pub unsafe extern "C" fn supercli_presence_has_device_id(device: *const c_char) -> c_uchar {
    let device = unsafe { c_str_to_str(device) };
    let device = if device.is_empty() { None } else { Some(device) };
    u8::from(viewer_presence::device_id_from_device(device).is_some())
}

#[allow(dead_code)]
fn _use_types() {
    // Keep imports referenced for future FFI surface growth.
    let _: Option<HashSet<String>> = None;
}
