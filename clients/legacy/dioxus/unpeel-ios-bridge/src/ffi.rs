//! `extern "C"` entry points the iOS native shell links against.
//!
//! Every function is safe to call from Swift/ObjC: null pointers are
//! rejected, panics are caught at the boundary (a panic becomes an error
//! event, never an unwind into the shell), and non-Apple targets get
//! identical no-op symbols so the shell links everywhere.

use crate::{notifications, speech, BridgeCallback};
use std::ffi::c_char;

/// Install the JSON event callback. Call once at startup.
#[unsafe(no_mangle)]
pub extern "C" fn supercli_ios_bridge_set_event_callback(cb: BridgeCallback) {
    crate::set_event_callback(cb);
}

/// Speech: request `SFSpeechRecognizer` authorization.
#[unsafe(no_mangle)]
pub extern "C" fn supercli_speech_request_authorization() {
    catch(speech::request_authorization);
}

/// Speech: start streaming dictation (partial + final result events).
#[unsafe(no_mangle)]
pub extern "C" fn supercli_speech_start() {
    catch(speech::start);
}

/// Speech: stop dictation and tear down the audio session.
#[unsafe(no_mangle)]
pub extern "C" fn supercli_speech_stop() {
    catch(speech::stop);
}

/// Notifications: request alert+badge+sound authorization.
#[unsafe(no_mangle)]
pub extern "C" fn supercli_notifications_request_authorization() {
    catch(notifications::request_authorization);
}

/// Notifications: start the APNs handshake (main thread only). The token or
/// error comes back through the shell's app delegate into the ingest
/// functions below.
#[unsafe(no_mangle)]
pub extern "C" fn supercli_notifications_register_remote() {
    catch(notifications::register_for_remote_notifications);
}

/// Notifications: install the tap-response delegate on the current
/// notification center.
#[unsafe(no_mangle)]
pub extern "C" fn supercli_notifications_install_response_delegate() {
    catch(notifications::install_response_delegate);
}

/// APNs: the device token from the shell's app delegate, as NUL-terminated
/// hex. Null is an error event, not a crash.
#[unsafe(no_mangle)]
pub extern "C" fn supercli_apns_ingest_token(hex: *const c_char) {
    catch(|| notifications::ingest_apns_token(hex));
}

/// APNs: registration failure from the shell's app delegate.
#[unsafe(no_mangle)]
pub extern "C" fn supercli_apns_ingest_error(message: *const c_char) {
    catch(|| notifications::ingest_apns_error(message));
}

/// Run a bridge call with panic containment: a panic becomes an error
/// event; it never unwinds across the FFI boundary into the shell.
fn catch(f: impl FnOnce() + std::panic::UnwindSafe) {
    if std::panic::catch_unwind(f).is_err() {
        crate::emit(
            "{\"kind\":\"error\",\"message\":\"ios bridge panicked (caught at FFI boundary)\"}",
        );
    }
}
