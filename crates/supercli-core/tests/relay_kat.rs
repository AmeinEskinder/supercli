//! Relay conformance known-answer test, as a dedicated integration target.
//!
//! This replays the SAME vectors as
//! `relay_crypto::tests::known_answer_vectors_match_swift_and_js`, but as a
//! standalone `cargo test --test relay_kat` binary. The lib test binary also
//! contains `ghostty_vt::tests::layout_matches_type_json`, which forces the
//! linker to extract the vendored `libghostty-vt.a` zig object; that link is
//! fragile on macOS (see vendor/ghostty-vt/fix-mh-execute-header.py). The
//! relay-conformance workflow runs the KAT through this target so a macOS
//! linker quirk in the VT shim can never gate relay conformance.
//!
//! Fixed inputs; expected outputs are the locked values from
//! `protocol/relay-kat-vectors-v2.json` (supercli-relay-v2 labels).
//! If this test fails, a Rust client could not establish a relay channel
//! — treat it as a release blocker.

use supercli_core::relay_crypto::{transcript_mac, CryptoSession};

fn range(n: usize, f: impl Fn(usize) -> usize) -> Vec<u8> {
    (0..n).map(|i| (f(i) & 0xff) as u8).collect()
}

fn b64_decode(text: &str) -> Vec<u8> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for ch in text
        .bytes()
        .filter(|c| *c != b'=' && !c.is_ascii_whitespace())
    {
        let v = TABLE
            .iter()
            .position(|t| *t == ch)
            .expect("base64 alphabet") as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    out
}

fn field<'a>(json: &'a str, key: &str) -> &'a str {
    let start = json.find(&format!("\"{key}\"")).expect(key);
    let rest = &json[start + key.len() + 2..];
    let open = rest.find('"').unwrap() + 1;
    let close = rest[open..].find('"').unwrap();
    &rest[open..open + close]
}

/// Cross-implementation known-answer test: the SAME fixed inputs as the relay
/// repo's `kat.test.mjs` (WebCrypto) must reproduce the committed vectors in
/// `protocol/relay-kat-vectors-v2.json`. This is the Rust half of relay
/// conformance.
#[test]
fn known_answer_vectors_match_swift_and_js() {
    let json = include_str!("../../../protocol/relay-kat-vectors-v2.json");
    let e2e_key = range(32, |i| i);
    let shared_secret = range(32, |i| 0x40 + i);
    let client_salt = range(16, |i| 0x10 + i);
    let host_salt = range(16, |i| 0xa0 + i);
    let client_eph = range(32, |i| 0x80 + i);
    let host_eph = range(32, |i| 0xc0 + i * 3);
    let mac = transcript_mac(
        &e2e_key,
        "phone-kat-1",
        &client_salt,
        &host_salt,
        &client_eph,
        &host_eph,
    );
    assert_eq!(
        mac,
        b64_decode(field(json, "transcriptMAC")),
        "Rust transcript MAC disagrees with the Swift/JS vector"
    );
    let mut client =
        CryptoSession::new(&e2e_key, &shared_secret, &client_salt, &host_salt, false).unwrap();
    let sealed = client.seal(b"known-answer-plaintext").unwrap();
    assert_eq!(
        sealed,
        b64_decode(field(json, "sealedFrame")),
        "Rust AES-GCM frame disagrees with the Swift/JS vector"
    );
    let mut host =
        CryptoSession::new(&e2e_key, &shared_secret, &client_salt, &host_salt, true).unwrap();
    assert_eq!(host.open(&sealed).unwrap(), b"known-answer-plaintext");
}
