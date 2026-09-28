//! C ABI for `supercli-client`'s iOS multi-Mac pairing store.
//!
//! Two API styles for the Dart client:
//!
//! 1. **Stateful**: an opaque [`PairedMacsStore`] handle with in-memory
//!    keychain/record storage. The embedder seeds platform-loaded state
//!    (records JSON, tokens, active id) via
//!    [`paired_macs_store_seed`], then drives the store.
//! 2. **Stateless JSON**: pure functions over JSON documents
//!    ([`paired_macs_hydrate_json`], [`paired_macs_upsert_json`],
//!    [`paired_macs_remove_json`]) for embedders that own persistence
//!    entirely (platform keychain/UserDefaults via Dart platform
//!    channels) and only want Rust's decision logic.
//!
//! All returned strings are NUL-terminated UTF-8 allocated by Rust;
//! release them with [`paired_macs_string_free`]. Null input pointers
//! are treated as empty/absent; every function is null-safe.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

use supercli_client::paired_macs::{
    paired_mac_collection, paired_mac_hydration, KeychainReadResult, PairedMacRecord, RecordStore,
    RemoteConnectionStore, TokenStore,
};

// ---------------------------------------------------------------------------
// String helpers
// ---------------------------------------------------------------------------

fn c_str_to_str<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("")
}

fn opt_c_str_to_opt(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let s = c_str_to_str(ptr);
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn string_to_c(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

/// Release a string returned by this crate.
///
/// # Safety
/// `s` must be a pointer returned by this crate (or null).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_string_free(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = CString::from_raw(s);
        }
    }
}

// ---------------------------------------------------------------------------
// In-memory storage backends (seeded by the embedder)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MemTokens {
    tokens: HashMap<String, String>,
    unavailable: Vec<String>,
}

impl TokenStore for MemTokens {
    fn read_token(&self, mac_id: &str) -> KeychainReadResult<String> {
        if self.unavailable.iter().any(|m| m == mac_id) {
            return KeychainReadResult::TemporarilyUnavailable("locked".to_string());
        }
        match self.tokens.get(mac_id) {
            Some(t) => KeychainReadResult::Found(t.clone()),
            None => KeychainReadResult::NotFound,
        }
    }

    fn save_token(&mut self, mac_id: &str, token: &str) -> bool {
        self.tokens.insert(mac_id.to_string(), token.to_string());
        true
    }

    fn delete_token(&mut self, mac_id: &str) {
        self.tokens.remove(mac_id);
    }
}

#[derive(Default)]
struct MemRecords {
    records: Vec<PairedMacRecord>,
    active_mac_id: Option<String>,
}

impl RecordStore for MemRecords {
    fn load_records(&self) -> Vec<PairedMacRecord> {
        self.records.clone()
    }
    fn save_records(&mut self, records: &[PairedMacRecord]) {
        self.records = records.to_vec();
    }
    fn load_active_mac_id(&self) -> Option<String> {
        self.active_mac_id.clone()
    }
    fn save_active_mac_id(&mut self, mac_id: Option<&str>) {
        self.active_mac_id = mac_id.map(|s| s.to_string());
    }
    fn load_device_id(&self) -> Option<String> {
        None
    }
    fn save_device_id(&mut self, _device_id: &str) {}
}

/// Opaque handle type (never constructed by C callers).
pub struct PairedMacsStore {
    inner: RemoteConnectionStore<MemTokens, MemRecords>,
}

/// Create a store. `dev_bridge_available` mirrors the iOS build flag
/// (simulator/dev builds may fall back to the localhost bridge).
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_store_new(dev_bridge_available: bool) -> *mut PairedMacsStore {
    let store = PairedMacsStore {
        inner: RemoteConnectionStore::new(
            MemTokens::default(),
            MemRecords::default(),
            dev_bridge_available,
        ),
    };
    Box::into_raw(Box::new(store))
}

/// Destroy a store created by [`paired_macs_store_new`].
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_store_free(handle: *mut PairedMacsStore) {
    if !handle.is_null() {
        unsafe {
            let _ = Box::from_raw(handle);
        }
    }
}

unsafe fn with_store<T>(
    handle: *mut PairedMacsStore,
    f: impl FnOnce(&mut PairedMacsStore) -> T,
) -> T {
    assert!(!handle.is_null(), "null PairedMacsStore handle");
    let store = unsafe { &mut *handle };
    f(store)
}

/// Seed in-memory storage with platform-loaded state, then hydrate.
/// - `records_json`: JSON array of [`PairedMacRecord`]
/// - `tokens_json`: JSON object `{"tokens": {"mac-id": "token"}, "unavailable": ["mac-id"]}`
/// - `active_mac_id`: preferred active Mac, or null
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_store_seed(
    handle: *mut PairedMacsStore,
    records_json: *const c_char,
    tokens_json: *const c_char,
    active_mac_id: *const c_char,
) {
    with_store(handle, |store| {
        let records: Vec<PairedMacRecord> =
            serde_json::from_str(c_str_to_str(records_json)).unwrap_or_default();
        let doc: serde_json::Value =
            serde_json::from_str(c_str_to_str(tokens_json)).unwrap_or(serde_json::Value::Null);
        let mut tokens = MemTokens::default();
        if let Some(map) = doc.get("tokens").and_then(|v| v.as_object()) {
            for (k, v) in map {
                if let Some(t) = v.as_str() {
                    tokens.tokens.insert(k.clone(), t.to_string());
                }
            }
        }
        if let Some(list) = doc.get("unavailable").and_then(|v| v.as_array()) {
            for v in list {
                if let Some(m) = v.as_str() {
                    tokens.unavailable.push(m.to_string());
                }
            }
        }
        // A fresh store hydrates from the seeded storage on construction.
        store.inner = RemoteConnectionStore::new(
            tokens,
            MemRecords {
                records,
                active_mac_id: opt_c_str_to_opt(active_mac_id),
            },
            false,
        );
    });
}

/// Re-run hydration (e.g. after unlock). Returns true when conclusive.
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_store_hydrate(handle: *mut PairedMacsStore) -> bool {
    with_store(handle, |store| {
        store.inner.hydrate(true);
        true
    })
}

/// Paired records as JSON array. Free with [`paired_macs_string_free`].
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_store_paired_macs_json(
    handle: *mut PairedMacsStore,
) -> *mut c_char {
    with_store(handle, |store| {
        string_to_c(serde_json::to_string(store.inner.paired_macs()).unwrap_or_default())
    })
}

/// Active mac id, or empty string. Free with [`paired_macs_string_free`].
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_store_active_mac_id(
    handle: *mut PairedMacsStore,
) -> *mut c_char {
    with_store(handle, |store| {
        string_to_c(store.inner.active_mac_id().unwrap_or("").to_string())
    })
}

/// Connection epoch (bumped on every identity change).
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_store_epoch(handle: *mut PairedMacsStore) -> i64 {
    with_store(handle, |store| store.inner.epoch())
}

/// Whether the pairing sheet should be shown.
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_store_needs_pairing(handle: *mut PairedMacsStore) -> bool {
    with_store(handle, |store| store.inner.needs_pairing())
}

/// Whether currently paired (vs dev-bridge fallback).
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_store_is_paired(handle: *mut PairedMacsStore) -> bool {
    with_store(handle, |store| {
        matches!(
            store.inner.mode(),
            supercli_client::paired_macs::ConnectionMode::Paired(_)
        )
    })
}

/// Switch the active Mac. Returns false when the switch did not happen
/// (unknown id, already active, or keychain temporarily unavailable).
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_store_switch_to(
    handle: *mut PairedMacsStore,
    mac_id: *const c_char,
) -> bool {
    with_store(handle, |store| store.inner.switch_to(c_str_to_str(mac_id)))
}

/// Commit a pairing: `record_json` is one [`PairedMacRecord`], `token`
/// the bearer. Fail-closed: false means nothing was persisted.
/// Freeing not needed (bool return).
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_store_commit_pairing(
    handle: *mut PairedMacsStore,
    record_json: *const c_char,
    token: *const c_char,
) -> bool {
    with_store(handle, |store| {
        let record: PairedMacRecord = match serde_json::from_str(c_str_to_str(record_json)) {
            Ok(r) => r,
            Err(_) => return false,
        };
        store.inner.commit_pairing(record, c_str_to_str(token))
    })
}

/// Remove a pairing entirely.
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_store_unpair(
    handle: *mut PairedMacsStore,
    mac_id: *const c_char,
) {
    with_store(handle, |store| store.inner.unpair(c_str_to_str(mac_id)));
}

/// Last pairing error, or empty string. Free with [`paired_macs_string_free`].
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_store_pairing_error(
    handle: *mut PairedMacsStore,
) -> *mut c_char {
    with_store(handle, |store| {
        string_to_c(store.inner.pairing_error().unwrap_or("").to_string())
    })
}

// ---------------------------------------------------------------------------
// Stateless JSON API (embedder owns persistence)
// ---------------------------------------------------------------------------

/// Resolve records against token state (pure logic).
/// Input `tokens_json`: `{"tokens": {"mac-id": "token"}, "unavailable": ["mac-id"]}`.
/// Returns `{"records":[...],"active_mac_id":"...","active_token":"...","unavailable":[...]}`.
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_hydrate_json(
    records_json: *const c_char,
    active_mac_id: *const c_char,
    tokens_json: *const c_char,
) -> *mut c_char {
    let records: Vec<PairedMacRecord> =
        serde_json::from_str(c_str_to_str(records_json)).unwrap_or_default();
    let doc: serde_json::Value =
        serde_json::from_str(c_str_to_str(tokens_json)).unwrap_or(serde_json::Value::Null);
    let tokens: HashMap<String, String> = doc
        .get("tokens")
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let unavailable: Vec<String> = doc
        .get("unavailable")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let hydration = paired_mac_hydration::resolve(
        &records,
        opt_c_str_to_opt(active_mac_id).as_deref(),
        |mac_id| {
            if unavailable.iter().any(|m| m == mac_id) {
                KeychainReadResult::TemporarilyUnavailable("locked".to_string())
            } else {
                match tokens.get(mac_id) {
                    Some(t) => KeychainReadResult::Found(t.clone()),
                    None => KeychainReadResult::NotFound,
                }
            }
        },
    );

    let out = serde_json::json!({
        "records": hydration.records,
        "active_mac_id": hydration.active_record.as_ref().map(|r| r.mac_id.as_str()),
        "active_token": hydration.active_token,
        "unavailable": hydration.unavailable_statuses,
    });
    string_to_c(out.to_string())
}

/// Upsert one record (JSON) into a records array (JSON). Returns the new array.
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_upsert_json(
    records_json: *const c_char,
    record_json: *const c_char,
) -> *mut c_char {
    let records: Vec<PairedMacRecord> =
        serde_json::from_str(c_str_to_str(records_json)).unwrap_or_default();
    let record: PairedMacRecord = match serde_json::from_str(c_str_to_str(record_json)) {
        Ok(r) => r,
        Err(_) => return string_to_c("[]".to_string()),
    };
    let out = paired_mac_collection::upserting(&records, record);
    string_to_c(serde_json::to_string(&out).unwrap_or_default())
}

/// Remove one mac id from a records array (JSON). Returns the new array.
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_remove_json(
    records_json: *const c_char,
    mac_id: *const c_char,
) -> *mut c_char {
    let records: Vec<PairedMacRecord> =
        serde_json::from_str(c_str_to_str(records_json)).unwrap_or_default();
    let out = paired_mac_collection::removing(&records, c_str_to_str(mac_id));
    string_to_c(serde_json::to_string(&out).unwrap_or_default())
}

/// Opaque-type sanity: returns the size of the handle pointer (always
/// non-null when the crate linked correctly). Used by Dart's DynamicLibrary
/// load check.
///
/// # Safety
/// All pointer arguments must be valid for the documented use
/// (non-null store handles from `paired_macs_store_new`,
/// NUL-terminated C strings or null where noted).
#[no_mangle]
pub unsafe extern "C" fn paired_macs_ffi_version() -> *mut c_char {
    string_to_c("supercli-client-ffi/0.9.0 paired_macs/1".to_string())
}
