//! Known-answer tests pinning byte compatibility with the Swift
//! (`CryptoKit`) and JS (`WebCrypto`) relay crypto implementations.
//!
//! Fixed inputs mirror `RelayCryptoVectorTests.swift` exactly; expected
//! outputs are the locked values from `protocol/relay-kat-vectors-v1.json`.
//! If this test fails, a Rust client could not establish a relay channel
//! with a Swift phone or Mac — treat it as a release blocker.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use supercli_client::crypto::{handshake, RelayCryptoSession};

fn seq(base: u16, len: usize) -> Vec<u8> {
    (0..len).map(|i| ((base + i as u16) & 0xff) as u8).collect()
}

#[test]
fn transcript_mac_known_answer() {
    let e2e: [u8; 32] = seq(0x00, 32).try_into().unwrap();
    let cs: [u8; 16] = seq(0x10, 16).try_into().unwrap();
    let hs: [u8; 16] = seq(0xA0, 16).try_into().unwrap();
    let client_eph = seq(0x80, 32);
    // Swift: UInt8(truncatingIfNeeded: 0xC0 + i * 3) — wrapping arithmetic.
    let host_eph: Vec<u8> = (0..32)
        .map(|i| 0xC0u8.wrapping_add((i as u8).wrapping_mul(3)))
        .collect();

    let mac = handshake::transcript_mac(&e2e, "phone-kat-1", &cs, &hs, &client_eph, &host_eph);
    assert_eq!(
        B64.encode(mac),
        "+BBTo0DBUwkP829M9w6eviupf+3pv5XxzrtNnUeYNQc=",
        "transcript MAC drifted — Rust and Swift/JS handshakes would disagree"
    );
}

#[test]
fn sealed_frame_known_answer() {
    let e2e: [u8; 32] = seq(0x00, 32).try_into().unwrap();
    let ss: [u8; 32] = seq(0x40, 32).try_into().unwrap();
    let cs: [u8; 16] = seq(0x10, 16).try_into().unwrap();
    let hs: [u8; 16] = seq(0xA0, 16).try_into().unwrap();

    // Client-direction session with the fixed shared secret.
    let mut client = RelayCryptoSession::new(&e2e, &ss, &cs, &hs, false).unwrap();
    let sealed = client.seal(b"known-answer-plaintext").unwrap();

    // The counter-prefixed layout is deterministic; the GCM ciphertext is
    // deterministic too (fixed key + counter nonce). Lock the whole frame.
    assert_eq!(
        B64.encode(&sealed),
        "AAAAAAAAAAFXoFTergM+a27Rbw/LTzDUy/OhPJRbGDcDIEpfVPJbKdy1zzcoCQ==",
        "sealed frame drifted — Rust and Swift/JS AEAD would disagree"
    );

    // And it must open on the matching host session.
    let mut host = RelayCryptoSession::new(&e2e, &ss, &cs, &hs, true).unwrap();
    assert_eq!(host.open(&sealed).unwrap(), b"known-answer-plaintext");
}
