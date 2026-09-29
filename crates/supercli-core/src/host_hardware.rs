//! Port of `HostHardware.swift` (native, Foundation-only).
//!
//! This Mac's hardware family, resolved once from `sysctl hw.model` and
//! cached. Controllers advertise this in the bootstrap (`hostDeviceKind` /
//! `hostDeviceModel`) so a remote Host shows the right icon and model hint
//! (a MacBook vs a Mac Studio vs a Linux box). Presentation only — never gate
//! behavior on it.
//!
//! There is no public API for the marketing name, so this maps the model
//! identifier *family* prefix to a stable kind string and a coarse family
//! label ("MacBook Pro" vs "MacBook" is not distinguished — a per-board table
//! is deliberately avoided). Unknown Macs still report a Mac model
//! identifier.

use std::sync::OnceLock;

/// Stable kind string matching `RemoteBootstrapSnapshot.hostDeviceKind`:
/// "macbook" | "macMini" | "macStudio" | "imac" | "macPro" | "unknown".
/// (A Linux host reports "linux" from the Rust bootstrap, not here.)
pub fn device_kind() -> &'static str {
    cached().0
}

/// Human-readable model hint, e.g. "MacBook", "Mac Studio", or the raw
/// model identifier when the family is unrecognized.
pub fn device_model() -> &'static str {
    cached().1
}

fn cached() -> &'static (&'static str, &'static str) {
    static CACHED: OnceLock<(&'static str, &'static str)> = OnceLock::new();
    CACHED.get_or_init(|| {
        let (kind, model) = resolve(model_identifier().as_deref());
        // Leak the owned Strings once: resolved a single time per process,
        // matching the Swift `static let cached`.
        let kind: &'static str = Box::leak(kind.into_boxed_str());
        let model: &'static str = Box::leak(model.into_boxed_str());
        (kind, model)
    })
}

/// Map a `hw.model` identifier (e.g. "Mac14,7" or "Macmini9,1") to the
/// `(kind, model)` pair. `None` (identifier unavailable) reports
/// `("unknown", "Mac")`, matching the Swift fallback.
pub fn resolve(identifier: Option<&str>) -> (String, String) {
    let Some(identifier) = identifier.filter(|s| !s.is_empty()) else {
        return ("unknown".to_string(), "Mac".to_string());
    };
    // Apple Silicon Studio ships as "Mac13,1"/"Mac13,2" and
    // "Mac14,13"/"Mac14,14"; the base Mac Pro is "Mac14,8".
    const STUDIO_IDS: [&str; 4] = ["Mac13,1", "Mac13,2", "Mac14,13", "Mac14,14"];
    if STUDIO_IDS.contains(&identifier) {
        return ("macStudio".to_string(), "Mac Studio".to_string());
    }
    if identifier == "Mac14,8" {
        return ("macPro".to_string(), "Mac Pro".to_string());
    }
    if identifier.starts_with("MacBook") {
        return ("macbook".to_string(), "MacBook".to_string());
    }
    if identifier.starts_with("Macmini") {
        return ("macMini".to_string(), "Mac mini".to_string());
    }
    if identifier.starts_with("iMac") {
        return ("imac".to_string(), "iMac".to_string());
    }
    if identifier.starts_with("MacPro") {
        return ("macPro".to_string(), "Mac Pro".to_string());
    }
    // A newer/unrecognized Apple Silicon board ("MacX,Y") is still a Mac,
    // but we cannot name its family — report unknown with the raw id.
    ("unknown".to_string(), identifier.to_string())
}

/// `hw.model` via sysctl on macOS; `None` everywhere else (or when libc is
/// unavailable under the portable `controller-core` feature).
#[cfg(all(target_os = "macos", feature = "native-host"))]
fn model_identifier() -> Option<String> {
    let name = c"hw.model";
    let mut size: libc::size_t = 0;
    // First call: learn the buffer size.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
        || size == 0
    {
        return None;
    }
    let mut buffer = vec![0u8; size];
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr() as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    // sysctl null-terminates the string; drop the trailing NUL(s).
    let end = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
    let value = String::from_utf8_lossy(&buffer[..end]).into_owned();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(not(all(target_os = "macos", feature = "native-host")))]
fn model_identifier() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_identifier_is_unknown_mac() {
        assert_eq!(resolve(None), ("unknown".into(), "Mac".into()));
    }

    #[test]
    fn empty_identifier_is_unknown_mac() {
        assert_eq!(resolve(Some("")), ("unknown".into(), "Mac".into()));
    }

    #[test]
    fn mac_studio_ids() {
        for id in ["Mac13,1", "Mac13,2", "Mac14,13", "Mac14,14"] {
            assert_eq!(
                resolve(Some(id)),
                ("macStudio".into(), "Mac Studio".into()),
                "id={id}"
            );
        }
    }

    #[test]
    fn base_mac_pro_id() {
        assert_eq!(
            resolve(Some("Mac14,8")),
            ("macPro".into(), "Mac Pro".into())
        );
    }

    #[test]
    fn macbook_family_prefix() {
        assert_eq!(
            resolve(Some("MacBookPro18,3")),
            ("macbook".into(), "MacBook".into())
        );
        assert_eq!(
            resolve(Some("MacBookAir10,1")),
            ("macbook".into(), "MacBook".into())
        );
        // "Mac14,7" has no "MacBook" prefix — Swift reports it unknown
        // with the raw id.
        assert_eq!(
            resolve(Some("Mac14,7")),
            ("unknown".into(), "Mac14,7".into())
        );
    }

    #[test]
    fn mac_mini_prefix() {
        assert_eq!(
            resolve(Some("Macmini9,1")),
            ("macMini".into(), "Mac mini".into())
        );
    }

    #[test]
    fn imac_prefix() {
        assert_eq!(resolve(Some("iMac21,1")), ("imac".into(), "iMac".into()));
    }

    #[test]
    fn mac_pro_prefix() {
        assert_eq!(
            resolve(Some("MacPro7,1")),
            ("macPro".into(), "Mac Pro".into())
        );
    }

    #[test]
    fn unrecognized_board_reports_raw_id() {
        assert_eq!(
            resolve(Some("Mac99,9")),
            ("unknown".into(), "Mac99,9".into())
        );
    }

    #[test]
    fn device_kind_and_model_do_not_panic() {
        // On this Linux VM the identifier is unavailable → unknown/Mac.
        let _ = device_kind();
        let _ = device_model();
    }
}
