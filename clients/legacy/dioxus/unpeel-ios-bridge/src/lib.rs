//! Rust-side iOS platform bridges for the Dioxus mobile launcher.
//!
//! The iOS native shell links this crate as a static library. Rust drives
//! the Apple frameworks **directly** through `objc2` — no Swift shim needed
//! for the framework calls themselves — and reports back through a single
//! `extern "C"` JSON event callback the shell installs at startup. That is
//! the "reflection" half: every native callback (APNs token, notification
//! tap, dictation result) is reflected into the shell as a JSON event, using
//! the same JSON lingua franca as the existing push bridge.
//!
//! # Threading contract
//!
//! The shell must install the event callback once, then call the entry
//! points below. Framework calls that require the main thread
//! (`UIApplication`) are checked with `MainThreadMarker`; call them from
//! the main thread (or have the shell dispatch there).
//!
//! # Apple-target gating
//!
//! Everything behind `cfg(target_vendor = "apple")` is the real
//! implementation. Every other target compiles inert stubs with identical
//! signatures, so host builds, Linux CI, and `cargo check` on this VM stay
//! green. The genuine Apple-target type-check runs in `apple.yml` on a
//! macOS runner (and, for this ring-free crate, also locally with
//! `cargo check --target aarch64-apple-ios`).
//!
//! # Speech backend note
//!
//! `objc2-speech` 0.3.2 (the newest available) binds only the classic
//! `SFSpeechRecognizer` API — there is **no** `SpeechAnalyzer` binding yet
//! (iOS 26 API). The dictation code is therefore the recognizer path, with
//! the backend choice structured so a future `SpeechAnalyzer` implementation
//! slots in behind the same event stream. The events the launcher consumes
//! (`partial` / `final` / `error`) do not change when the backend does.

use std::ffi::c_char;
use std::sync::Mutex;

/// Event callback installed by the native shell.
///
/// Receives a NUL-terminated UTF-8 JSON document. The pointer is valid only
/// for the duration of the call — the shell must copy what it keeps.
pub type BridgeCallback = Option<unsafe extern "C" fn(*const c_char)>;

static EVENT_CALLBACK: Mutex<BridgeCallback> = Mutex::new(None);

/// Install (or replace, with `None`) the JSON event sink.
pub fn set_event_callback(cb: BridgeCallback) {
    *EVENT_CALLBACK.lock().unwrap() = cb;
}

/// Emit one JSON event to the installed callback. Drops the event when no
/// callback is installed (shell hasn't started listening yet).
pub(crate) fn emit(json: &str) {
    let cb = *EVENT_CALLBACK.lock().unwrap();
    if let Some(cb) = cb {
        let bytes = json.as_bytes();
        // NUL-terminate without going through CString (avoids allocation
        // failure paths on an already-unlikely edge).
        let mut buf = Vec::with_capacity(bytes.len() + 1);
        buf.extend_from_slice(bytes);
        buf.push(0);
        // SAFETY: buf is NUL-terminated and lives for the call; the contract
        // says the shell copies synchronously.
        unsafe { cb(buf.as_ptr() as *const c_char) };
    }
}

/// Minimal JSON string escaper (no serde dependency on purpose — this crate
/// stays dependency-light on Apple targets).
#[cfg_attr(not(target_vendor = "apple"), allow(dead_code))]
pub(crate) fn json_escape(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
}

pub mod ffi;
pub mod notifications;
pub mod speech;

#[cfg(test)]
mod tests {
    use super::json_escape;

    fn esc(s: &str) -> String {
        let mut out = String::new();
        json_escape(&mut out, s);
        out
    }

    #[test]
    fn escapes_quotes_backslashes_and_controls() {
        assert_eq!(esc("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(esc("line1\nline2\r\ttab"), "line1\\nline2\\r\\ttab");
        assert_eq!(esc("unit\u{1}sep"), "unit\\u0001sep");
    }

    #[test]
    fn leaves_plain_text_and_unicode_alone() {
        assert_eq!(esc("hello world 123"), "hello world 123");
        assert_eq!(esc("héllo wörld 🎙"), "héllo wörld 🎙");
        assert_eq!(esc(""), "");
    }

    #[test]
    fn hostile_filenames_stay_verbatim_except_escaping() {
        // A filename containing would-be template markers must survive
        // embedding into a JSON event payload unchanged (modulo escaping).
        let hostile = "{B64} \"quoted\" \\ backslash";
        let e = esc(hostile);
        assert!(e.contains("{B64}"));
        assert!(e.contains("\\\"quoted\\\""));
        assert!(e.contains("\\\\ backslash"));
    }
}
