//! C ABI for the Supercli client logic.
//!
//! This crate is the single FFI surface the Dart/gpuidart UI calls into,
//! per Amein's rule: one implementation, in Rust; Dart keeps only UI.
//!
//! # Memory contract
//!
//! Functions returning `*mut c_char` hand ownership to the caller; the caller
//! MUST release the string with [`supercli_string_free`]. All input pointers
//! are borrowed for the duration of the call.
//!
//! # Panic contract
//!
//! Every `extern "C"` entry point is wrapped in [`catch_unwind`]: a Rust
//! panic inside the library NEVER unwinds across the FFI boundary (which
//! would abort the host app). On panic the function returns its documented
//! failure value (null / 0) and records the message, retrievable with
//! [`supercli_last_error`].
//!
//! # Error contract
//!
//! Null input pointers and invalid UTF-8 are NOT silently mapped to `""`.
//! Functions that require a string input return their failure value and
//! record a descriptive error via [`supercli_last_error`]. Callers should
//! check [`supercli_last_error`] (or [`supercli_clear_error`] before the
//! call) when they receive a failure value.
//!
//! # ABI version
//!
//! [`supercli_ffi_abi_version`] returns [`SUPERCLI_FFI_ABI_VERSION`]. Dart
//! checks it at load time and fails fast on mismatch.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_uchar};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use supercli_client::{viewer_presence, workspace_pool};
use supercli_core::{presets, runtime_catalog, terminal_drop_maps, workspace_registry};

/// ABI version of this C surface. Bump when adding/removing/changing any
/// exported symbol's signature. Dart checks this at load time.
pub const SUPERCLI_FFI_ABI_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Error reporting
// ---------------------------------------------------------------------------

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn set_last_error(msg: String) {
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(msg));
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// Run `f` with panics caught. On panic, records the message and returns
/// `on_panic` — panics never cross the FFI boundary.
fn ffi_guard<R>(on_panic: R, f: impl FnOnce() -> R + std::panic::UnwindSafe) -> R {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(payload) => {
            set_last_error(format!("panic in FFI call: {}", panic_message(&payload)));
            on_panic
        }
    }
}

/// ABI version. Dart calls this first and fails fast on mismatch.
#[no_mangle]
pub extern "C" fn supercli_ffi_abi_version() -> u32 {
    SUPERCLI_FFI_ABI_VERSION
}

/// Last recorded FFI error message, or null when none. The caller owns the
/// returned string and MUST free it with [`supercli_string_free`].
#[no_mangle]
pub extern "C" fn supercli_last_error() -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        LAST_ERROR.with(|e| match e.borrow().as_ref() {
            Some(msg) => to_c_string(msg.clone()),
            None => std::ptr::null_mut(),
        })
    })
}

/// Clear the last recorded FFI error.
#[no_mangle]
pub extern "C" fn supercli_clear_error() {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        LAST_ERROR.with(|e| *e.borrow_mut() = None);
    }));
}

// ---------------------------------------------------------------------------
// String helpers
// ---------------------------------------------------------------------------

/// Release a string returned by this library.
///
/// # Safety
///
/// `s` must be either null or a pointer previously returned by this
/// library (and not already freed).
#[no_mangle]
pub unsafe extern "C" fn supercli_string_free(s: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            unsafe { drop(CString::from_raw(s)) };
        }
    }));
}

fn to_c_string(s: String) -> *mut c_char {
    CString::new(s).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Borrow a C string. Null pointers and invalid UTF-8 are errors, NOT
/// silently mapped to `""`.
unsafe fn c_str_to_str<'a>(p: *const c_char) -> Result<&'a str, String> {
    if p.is_null() {
        return Err("null string pointer".to_string());
    }
    unsafe { CStr::from_ptr(p) }
        .to_str()
        .map_err(|e| format!("invalid UTF-8 in string argument: {e}"))
}

/// Borrow a C string, recording the error and returning `None` on failure.
unsafe fn c_str_or_none<'a>(p: *const c_char) -> Option<&'a str> {
    match unsafe { c_str_to_str(p) } {
        Ok(s) => Some(s),
        Err(e) => {
            set_last_error(e);
            None
        }
    }
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
///
/// # Safety
///
/// This function takes no pointer arguments; it is `unsafe` only because
/// it is an `extern "C"` entry point. It never dereferences caller memory.
#[no_mangle]
pub unsafe extern "C" fn supercli_runtime_catalog_json() -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
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
    })
}

/// JSON object of the runtime matching `id` (stable id or legacy slug), or
/// null when unknown. Null/invalid-UTF-8 `id` -> null + recorded error.
///
/// # Safety
///
/// `id` must be either null or point to a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn supercli_runtime_by_id_json(id: *const c_char) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        let id = match unsafe { c_str_or_none(id) } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let catalog = runtime_catalog::builtin_runtime_catalog();
        let d = catalog.by_id(id).or_else(|| catalog.by_legacy_slug(id));
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
    })
}

/// Legacy slug of the runtime that launches `command`, or null.
/// Null/invalid-UTF-8 `command` -> null + recorded error.
///
/// # Safety
///
/// `command` must be either null or point to a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn supercli_runtime_detect_tool(command: *const c_char) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        let command = match unsafe { c_str_or_none(command) } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let catalog = runtime_catalog::builtin_runtime_catalog();
        match presets::descriptor_for_command(catalog, command) {
            Some(d) => to_c_string(d.legacy_slug.clone()),
            None => std::ptr::null_mut(),
        }
    })
}

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

/// 1 when `command` resolves to a quick-launchable runtime, else 0.
/// Null/invalid-UTF-8 `command` -> 0 + recorded error.
///
/// # Safety
///
/// `command` must be either null or point to a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn supercli_preset_tool_is_quick_launchable(command: *const c_char) -> c_uchar {
    ffi_guard(0, || {
        let command = match unsafe { c_str_or_none(command) } {
            Some(s) => s,
            None => return 0,
        };
        let catalog = runtime_catalog::builtin_runtime_catalog();
        u8::from(presets::QuickPresetTool::detect(catalog, command).is_some())
    })
}

/// Display name for a tool's legacy slug, or null when unknown.
/// Null/invalid-UTF-8 `legacy_slug` -> null + recorded error.
///
/// # Safety
///
/// `legacy_slug` must be either null or point to a valid NUL-terminated
/// C string.
#[no_mangle]
pub unsafe extern "C" fn supercli_preset_tool_display_name(
    legacy_slug: *const c_char,
) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        let slug = match unsafe { c_str_or_none(legacy_slug) } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let catalog = runtime_catalog::builtin_runtime_catalog();
        match presets::SetupTool::new(catalog, slug) {
            Some(t) => to_c_string(t.display_name(catalog)),
            None => std::ptr::null_mut(),
        }
    })
}

// ---------------------------------------------------------------------------
// Workspace pool (pure functions)
// ---------------------------------------------------------------------------

/// Exponential backoff delay in ms for `consecutive_failures` (>= 1).
#[no_mangle]
pub extern "C" fn supercli_pool_backoff_delay_ms(consecutive_failures: u32) -> u64 {
    ffi_guard(0, || {
        workspace_pool::backoff_delay_ms(
            consecutive_failures,
            workspace_pool::policy::BACKOFF_BASE_MS,
            workspace_pool::policy::BACKOFF_CAP_MS,
        )
    })
}

/// Pool policy tunables as JSON: `{poll_interval_ms, backoff_base_ms,
/// backoff_cap_ms, maintenance_interval_ms, max_live_remote_connections,
/// organization_hold_secs}`.
#[no_mangle]
pub extern "C" fn supercli_pool_policy_json() -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
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
    })
}

// ---------------------------------------------------------------------------
// Workspace registry
// ---------------------------------------------------------------------------

/// URL-safe slug for a workspace display name.
/// Null/invalid-UTF-8 `name` -> null + recorded error.
///
/// # Safety
///
/// `name` must be either null or point to a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn supercli_registry_slugify(name: *const c_char) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        let name = match unsafe { c_str_or_none(name) } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        to_c_string(workspace_registry::slugify(name))
    })
}

/// `local:` order key for a workspace home path.
/// Null/invalid-UTF-8 `home` -> null + recorded error.
///
/// # Safety
///
/// `home` must be either null or point to a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn supercli_registry_local_key(home: *const c_char) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        let home = match unsafe { c_str_or_none(home) } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        to_c_string(workspace_registry::list_order::local_key(
            PathBuf::from(home).as_path(),
        ))
    })
}

/// `host:` order key for a paired host id.
/// Null/invalid-UTF-8 `host_id` -> null + recorded error.
///
/// # Safety
///
/// `host_id` must be either null or point to a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn supercli_registry_paired_key(host_id: *const c_char) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        let host_id = match unsafe { c_str_or_none(host_id) } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        to_c_string(workspace_registry::list_order::paired_key(host_id))
    })
}

// ---------------------------------------------------------------------------
// Viewer presence
// ---------------------------------------------------------------------------

/// Parse one presence feed file; returns JSON `{session_id: [{id,
/// device_id, display_name, last_seen_ms}]}`. Malformed input -> `{}`.
/// Null/invalid-UTF-8 `source` -> null + recorded error.
///
/// # Safety
///
/// `data` must be either null or point to `len` readable bytes. `source`
/// must be either null or point to a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn supercli_presence_parse(
    data: *const c_uchar,
    len: usize,
    source: *const c_char,
) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        let data = unsafe { c_bytes(data, len) };
        let source = match unsafe { c_str_or_none(source) } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
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
    })
}

/// Display name for a `device` ("Name (id)") / `ip` pair. Empty (but valid)
/// inputs are treated as absent, matching the Dart-side convention.
/// Null/invalid-UTF-8 inputs -> null + recorded error.
///
/// # Safety
///
/// `device` and `ip` must each be either null or point to a valid
/// NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn supercli_presence_display_name(
    device: *const c_char,
    ip: *const c_char,
) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        let device = match unsafe { c_str_or_none(device) } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let ip = match unsafe { c_str_or_none(ip) } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let device = if device.is_empty() { None } else { Some(device) };
        let ip = if ip.is_empty() { None } else { Some(ip) };
        to_c_string(viewer_presence::display_name_from_device(device, ip))
    })
}

/// 1 when `device` ("Name (id)") carries a stable device id, else 0.
/// Null/invalid-UTF-8 `device` -> 0 + recorded error.
///
/// # Safety
///
/// `device` must be either null or point to a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn supercli_presence_has_device_id(device: *const c_char) -> c_uchar {
    ffi_guard(0, || {
        let device = match unsafe { c_str_or_none(device) } {
            Some(s) => s,
            None => return 0,
        };
        let device = if device.is_empty() { None } else { Some(device) };
        u8::from(viewer_presence::device_id_from_device(device).is_some())
    })
}

// ---------------------------------------------------------------------------
// Terminal drop maps
// ---------------------------------------------------------------------------

/// 1 when the drop-target map JSON accepts a drop at `(row, column)` at
/// `now_ms`, else 0. Malformed/oversize input -> 0.
///
/// # Safety
///
/// `json` must be either null or point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn supercli_drop_map_accepts(
    json: *const c_uchar,
    len: usize,
    row: u32,
    column: u32,
    now_ms: u64,
) -> c_uchar {
    ffi_guard(0, || {
        let json = unsafe { c_bytes(json, len) };
        let accepts = terminal_drop_maps::DropTargetMap::from_json_bytes(json)
            .is_ok_and(|m| m.accepts(row, column, now_ms));
        u8::from(accepts)
    })
}

/// Host-local path for the path-drag map JSON at `(row, column)` at
/// `now_ms`, or null when unmapped/stale/malformed.
///
/// # Safety
///
/// `json` must be either null or point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn supercli_path_drag_map_path_at(
    json: *const c_uchar,
    len: usize,
    row: u32,
    column: u32,
    now_ms: u64,
) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        let json = unsafe { c_bytes(json, len) };
        match terminal_drop_maps::PathDragMap::from_json_bytes(json) {
            Ok(m) => match m.path_at(row, column, now_ms) {
                Some(p) => to_c_string(p.to_string()),
                None => std::ptr::null_mut(),
            },
            Err(_) => std::ptr::null_mut(),
        }
    })
}

#[allow(dead_code)]
fn _use_types() {
    // Keep imports referenced for future FFI surface growth.
    let _: Option<HashSet<String>> = None;
}

// ---------------------------------------------------------------------------
// Tests: exercise the extern fns directly, as a C caller would.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod ffi_tests {
    use super::*;
    use std::ffi::CString;

    fn c(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    /// Read a returned string WITHOUT freeing (test-only; the real caller
    /// must call supercli_string_free).
    unsafe fn peek(p: *mut c_char) -> String {
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { supercli_string_free(p) };
        s
    }

    #[test]
    fn abi_version_is_one() {
        assert_eq!(supercli_ffi_abi_version(), SUPERCLI_FFI_ABI_VERSION);
        assert_eq!(SUPERCLI_FFI_ABI_VERSION, 1);
    }

    #[test]
    fn catalog_json_roundtrip_and_free() {
        let p = unsafe { supercli_runtime_catalog_json() };
        let s = unsafe { peek(p) };
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert!(v.is_array());
        assert!(!v.as_array().unwrap().is_empty());
    }

    #[test]
    fn runtime_by_id_null_and_unknown() {
        unsafe {
            // Null input -> null + recorded error (not a silent "").
            supercli_clear_error();
            assert!(supercli_runtime_by_id_json(std::ptr::null()).is_null());
            assert!(!supercli_last_error().is_null());
            supercli_string_free(supercli_last_error());

            // Unknown id -> null, no error recorded.
            supercli_clear_error();
            let id = c("no-such-runtime");
            assert!(supercli_runtime_by_id_json(id.as_ptr()).is_null());
            assert!(supercli_last_error().is_null());
        }
    }

    #[test]
    fn runtime_by_id_known() {
        unsafe {
            // Use the catalog's own first descriptor id — robust against
            // catalog content changes.
            let catalog_p = supercli_runtime_catalog_json();
            let catalog_s = peek(catalog_p);
            let v: serde_json::Value = serde_json::from_str(&catalog_s).unwrap();
            let first_id = v.as_array().unwrap()[0]["id"].as_str().unwrap().to_string();
            let first_slug = v.as_array().unwrap()[0]["legacy_slug"].as_str().unwrap().to_string();

            let id = c(&first_id);
            let p = supercli_runtime_by_id_json(id.as_ptr());
            assert!(!p.is_null(), "by_id({first_id}) returned null");
            let s = peek(p);
            assert!(s.contains(&first_id));

            // Legacy slug lookup also works.
            let slug = c(&first_slug);
            let p2 = supercli_runtime_by_id_json(slug.as_ptr());
            assert!(!p2.is_null(), "by_legacy_slug({first_slug}) returned null");
            let s2 = peek(p2);
            assert!(s2.contains(&first_id));
        }
    }

    #[test]
    fn bad_utf8_is_error_not_silent_empty() {
        unsafe {
            // 0xFF is invalid UTF-8.
            let bad = [0xFFu8, 0x00];
            supercli_clear_error();
            let p = supercli_runtime_by_id_json(bad.as_ptr() as *const c_char);
            assert!(p.is_null());
            let err = supercli_last_error();
            assert!(!err.is_null());
            let msg = peek(err);
            assert!(msg.contains("UTF-8"), "unexpected error: {msg}");
        }
    }

    #[test]
    fn detect_tool_null_is_error() {
        unsafe {
            supercli_clear_error();
            assert!(supercli_runtime_detect_tool(std::ptr::null()).is_null());
            assert!(!supercli_last_error().is_null());
            supercli_string_free(supercli_last_error());
        }
    }

    #[test]
    fn quick_launchable_null_is_zero_with_error() {
        unsafe {
            supercli_clear_error();
            assert_eq!(supercli_preset_tool_is_quick_launchable(std::ptr::null()), 0);
            assert!(!supercli_last_error().is_null());
            supercli_string_free(supercli_last_error());
        }
    }

    #[test]
    fn display_name_unknown_slug_is_null() {
        unsafe {
            let slug = c("no-such-slug");
            assert!(supercli_preset_tool_display_name(slug.as_ptr()).is_null());
        }
    }

    #[test]
    fn pool_backoff_monotonic() {
        // Pure function: no string handling, but must not panic.
        let d1 = supercli_pool_backoff_delay_ms(1);
        let d2 = supercli_pool_backoff_delay_ms(2);
        let d3 = supercli_pool_backoff_delay_ms(10);
        assert!(d1 > 0);
        assert!(d2 >= d1);
        assert!(d3 >= d2);
    }

    #[test]
    fn pool_policy_json_has_keys() {
        unsafe {
            let p = supercli_pool_policy_json();
            let s = peek(p);
            let v: serde_json::Value = serde_json::from_str(&s).unwrap();
            for key in [
                "poll_interval_ms",
                "backoff_base_ms",
                "backoff_cap_ms",
                "max_live_remote_connections",
            ] {
                assert!(v.get(key).is_some(), "missing {key}");
            }
        }
    }

    #[test]
    fn registry_slugify_roundtrip() {
        unsafe {
            let name = c("My Workspace!");
            let p = supercli_registry_slugify(name.as_ptr());
            let s = peek(p);
            assert!(!s.is_empty());
            assert!(!s.contains(' '));
            assert!(!s.contains('!'));
        }
    }

    #[test]
    fn registry_keys_null_is_error() {
        unsafe {
            supercli_clear_error();
            assert!(supercli_registry_local_key(std::ptr::null()).is_null());
            assert!(!supercli_last_error().is_null());
            supercli_string_free(supercli_last_error());
            supercli_clear_error();
            assert!(supercli_registry_paired_key(std::ptr::null()).is_null());
            assert!(!supercli_last_error().is_null());
            supercli_string_free(supercli_last_error());
        }
    }

    #[test]
    fn presence_parse_empty_is_empty_object() {
        unsafe {
            let source = c("test");
            let p = supercli_presence_parse(std::ptr::null(), 0, source.as_ptr());
            let s = peek(p);
            assert_eq!(s, "{}");
        }
    }

    #[test]
    fn presence_parse_null_source_is_error() {
        unsafe {
            supercli_clear_error();
            let p = supercli_presence_parse(std::ptr::null(), 0, std::ptr::null());
            assert!(p.is_null());
            assert!(!supercli_last_error().is_null());
            supercli_string_free(supercli_last_error());
        }
    }

    #[test]
    fn presence_display_name_pair() {
        unsafe {
            let device = c("Alice (abc123)");
            let ip = c("10.0.0.1");
            let p = supercli_presence_display_name(device.as_ptr(), ip.as_ptr());
            let s = peek(p);
            assert!(s.contains("Alice"));
        }
    }

    #[test]
    fn presence_has_device_id_null_is_zero_with_error() {
        unsafe {
            supercli_clear_error();
            assert_eq!(supercli_presence_has_device_id(std::ptr::null()), 0);
            assert!(!supercli_last_error().is_null());
            supercli_string_free(supercli_last_error());
        }
    }

    #[test]
    fn drop_map_accepts_malformed_is_zero() {
        unsafe {
            let bad = c("not json");
            // Malformed JSON -> 0, no panic.
            assert_eq!(
                supercli_drop_map_accepts(
                    bad.as_ptr() as *const c_uchar,
                    bad.as_bytes().len() as usize,
                    0,
                    0,
                    0
                ),
                0
            );
            // Null bytes -> 0.
            assert_eq!(
                supercli_drop_map_accepts(std::ptr::null(), 0, 0, 0, 0),
                0
            );
        }
    }

    #[test]
    fn path_drag_map_malformed_is_null() {
        unsafe {
            let bad = c("not json");
            assert!(supercli_path_drag_map_path_at(
                bad.as_ptr() as *const c_uchar,
                bad.as_bytes().len() as usize,
                0,
                0,
                0
            )
            .is_null());
        }
    }

    #[test]
    fn string_free_null_is_noop() {
        unsafe {
            // Must not crash.
            supercli_string_free(std::ptr::null_mut());
        }
    }

    #[test]
    fn last_error_clear_roundtrip() {
        unsafe {
            supercli_clear_error();
            assert!(supercli_last_error().is_null());
            // Trigger an error, then clear.
            supercli_runtime_by_id_json(std::ptr::null());
            assert!(!supercli_last_error().is_null());
            supercli_string_free(supercli_last_error());
            supercli_clear_error();
            assert!(supercli_last_error().is_null());
        }
    }
}
