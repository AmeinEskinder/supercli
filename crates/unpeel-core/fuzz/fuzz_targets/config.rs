#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the config document validator.
    // Must not panic on arbitrary input; parse then validate.
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
            let _ = unpeel_core::config::check_document(&v);
        }
        // Also try as TOML-ish: config uses JSON values; skip.
    }
});
