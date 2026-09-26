#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|plaintext: &[u8]| {
    // Phase 13 v3 (2): Fuzz the POST-DECRYPTION parsing path.
    //
    // The original sealed_envelope target feeds arbitrary bytes to
    // decode_incoming, which rejects 99.9% at the first check (length/magic/
    // auth tag). 2.1B execs yielded only 25 coverage units — the inner
    // parser was never reached.
    //
    // This target builds a VALID envelope with a fixed test key around
    // fuzzer-controlled plaintext, so decode_incoming always passes the
    // crypto checks and the post-decryption frame parsing gets fuzzed.
    //
    // Must not panic on arbitrary plaintext; returns Option/Result.

    // Fixed test key material (NOT secret — test only).
    let e2e_key = [0x42u8; 32];
    let shared = [0x24u8; 32];
    let client_salt = [0x11u8; 16];
    let host_salt = [0x22u8; 16];

    // Create a crypto session (is_host=false = client side).
    let mut session = match supercli_core::relay_crypto::CryptoSession::new(
        &e2e_key,
        &shared,
        &client_salt,
        &host_salt,
        false, // is_host
    ) {
        Ok(s) => s,
        Err(_) => return, // Should not happen with valid test keys.
    };

    // Seal the fuzzer-controlled plaintext into a valid envelope.
    // If plaintext is too large, seal returns Err — that's fine, skip.
    let envelope = match session.seal(plaintext) {
        Ok(env) => env,
        Err(_) => return,
    };

    // Now feed the VALID envelope to decode_incoming. This exercises the
    // post-decryption parsing (frame type dispatch, payload validation,
    // etc.) which the original target never reached.
    let _ = supercli_core::relay_crypto::decode_incoming(&envelope);
});
