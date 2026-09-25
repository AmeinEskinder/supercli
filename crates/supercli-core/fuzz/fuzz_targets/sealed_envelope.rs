#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the sealed-envelope / relay frame decoder.
    // Must not panic on arbitrary input; returns Option.
    let _ = supercli_core::relay_crypto::decode_incoming(data);
});
