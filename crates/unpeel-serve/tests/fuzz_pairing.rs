//! Phase 9 H1 — fuzz + property tests for the sealed pairing envelope
//! (`unpeel_serve::pairing::open_envelope` / `seal_envelope`).
//!
//! Same deterministic in-tree harness as the other H1 fuzz targets:
//! cargo-fuzz/libFuzzer could not be installed offline, so a seeded
//! xorshift RNG, a corpus of valid envelopes, and structural mutations
//! drive the decoder for a bounded number of iterations. `open_envelope`
//! is total (`Option`, never panic); any panic is a bug.
//!
//! Properties:
//! - request envelopes sealed for `phone-to-mac` (mirroring the phone's
//!   KDF/AAD, reimplemented here as an independent second implementation)
//!   round-trip through `open_envelope`, including hostile plaintexts;
//! - wrong token / mac_id / endpoint (AAD binding) fail closed;
//! - any single-byte tamper of the sealed payload fails closed (AES-GCM);
//! - direction confusion (`seal_envelope`'s `mac-to-phone` output fed to
//!   `open_envelope`) fails closed;
//! - malformed envelopes (bad `v`, short salt, short sealed) fail closed.

use std::panic;
use std::time::Instant;
use unpeel_serve::pairing::{open_envelope, seal_envelope};

// ------------------------------------------------------------------ rng ---

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn byte(&mut self) -> u8 {
        self.next() as u8
    }
}

const INTERESTING: &[u8] = &[
    0x00, 0x01, 0x7f, 0x80, 0xff, b'"', b'\\', b'{', b'}', b'[', b']', b':', b',', b'\n',
];

fn mutate(rng: &mut Rng, data: &[u8], seeds: &[Vec<u8>]) -> Vec<u8> {
    let mut buf = if data.is_empty() || rng.below(4) == 0 {
        seeds[rng.below(seeds.len())].clone()
    } else {
        data.to_vec()
    };
    for _ in 0..1 + rng.below(4) {
        if buf.is_empty() {
            buf.push(rng.byte());
            continue;
        }
        match rng.below(8) {
            0 => {
                let i = rng.below(buf.len());
                buf[i] ^= 1 << rng.below(8);
            }
            1 => {
                let i = rng.below(buf.len());
                buf[i] = rng.byte();
            }
            2 => {
                let i = rng.below(buf.len());
                buf[i] = INTERESTING[rng.below(INTERESTING.len())];
            }
            3 => {
                let i = rng.below(buf.len() + 1);
                buf.insert(i, rng.byte());
            }
            4 => {
                buf.remove(rng.below(buf.len()));
            }
            5 => {
                let (i, j) = (rng.below(buf.len()), rng.below(buf.len()));
                buf.swap(i, j);
            }
            6 => buf.truncate(rng.below(buf.len() + 1)),
            _ => {
                let other = &seeds[rng.below(seeds.len())];
                let i = rng.below(buf.len() + 1);
                let j = rng.below(other.len() + 1).min(other.len());
                let mut nb = Vec::with_capacity(i + other.len() - j);
                nb.extend_from_slice(&buf[..i]);
                nb.extend_from_slice(&other[j..]);
                buf = nb;
            }
        }
    }
    buf
}

fn iters() -> usize {
    std::env::var("UNPEEL_FUZZ_ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20_000)
}

// ------------------------------------------- independent phone-side seal ---

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine as _;

/// Mirror of the phone's request seal (`phone-to-mac` direction), as a
/// second implementation pinning the KDF/AAD wire format. Salt/nonce come
/// from the passed RNG so the whole test stays deterministic.
fn phone_seal(
    rng: &mut Rng,
    plaintext: &[u8],
    token: &str,
    mac_id: &str,
    endpoint: &str,
) -> serde_json::Value {
    let mut salt = [0u8; 16];
    for b in salt.iter_mut() {
        *b = rng.byte();
    }
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(&salt), token.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(b"unpeel-pairing-v1:phone-to-mac", &mut key)
        .expect("32 bytes is a valid HKDF length");
    let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
    let mut nonce_bytes = [0u8; 12];
    for b in nonce_bytes.iter_mut() {
        *b = rng.byte();
    }
    let mut aad = Vec::new();
    aad.extend_from_slice(b"unpeel-pairing-v1");
    aad.push(0);
    aad.extend_from_slice(b"phone-to-mac");
    aad.push(0);
    aad.extend_from_slice(mac_id.as_bytes());
    aad.push(0);
    aad.extend_from_slice(endpoint.as_bytes());
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .unwrap();
    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);
    let engine = base64::engine::general_purpose::STANDARD;
    serde_json::json!({
        "v": 1,
        "saltB64": engine.encode(salt),
        "sealedB64": engine.encode(&combined),
    })
}

const TOKEN: &str = "K7ZP2Q4RSTUVWXYZABCDEFGH23";
const MAC: &str = "b5b9a1ff-c0e2-42f1-9801-316d331ddfd3";
const ENDPOINT: &str = "http://192.168.1.20:49152/mobile";

fn corpus(rng: &mut Rng) -> Vec<Vec<u8>> {
    let mut out = vec![
        b"{}".to_vec(),
        b"null".to_vec(),
        b"[]".to_vec(),
        b"not json".to_vec(),
        br#"{"v":2,"saltB64":"AAAAAAAAAAAAAAAAAAAAAA==","sealedB64":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#.to_vec(),
        br#"{"v":1}"#.to_vec(),
        br#"{"v":1,"saltB64":"!!!not-base64!!!","sealedB64":"!!!"}"#.to_vec(),
        br#"{"v":1,"saltB64":"AA==","sealedB64":"AA=="}"#.to_vec(), // short salt + short sealed
    ];
    for pt in [
        b"{}".to_vec(),
        b"".to_vec(),
        vec![0u8; 1024],
        b"\xff\xfe binary \x00 junk".to_vec(),
        "{\"token\":\"".repeat(500).into_bytes(),
    ] {
        let env = phone_seal(rng, &pt, TOKEN, MAC, ENDPOINT);
        out.push(serde_json::to_vec(&env).unwrap());
    }
    // A mac-to-phone (response-direction) envelope: must fail closed.
    let resp = seal_envelope(b"hello", TOKEN, MAC, ENDPOINT).unwrap();
    out.push(serde_json::to_vec(&resp).unwrap());
    out
}

#[test]
fn fuzz_open_envelope_never_panics() {
    let mut rng = Rng(0xC2B2_AE3D_27D4_EB4F);
    let seeds = corpus(&mut rng);
    let mut buf = seeds[0].clone();
    let t0 = Instant::now();
    let mut opened = 0usize;
    // Hostile binding contexts too: the AAD inputs are caller-controlled.
    let tokens = ["", TOKEN, "short", &"T".repeat(500)];
    let macs = ["", MAC, "x", &"M".repeat(500)];
    let endpoints = ["", ENDPOINT, "http://x/mobile", &"E".repeat(500)];
    for i in 0..iters() {
        let input = mutate(&mut rng, &buf, &seeds);
        buf = input.clone();
        let token = tokens[rng.below(tokens.len())];
        let mac = macs[rng.below(macs.len())];
        let endpoint = endpoints[rng.below(endpoints.len())];
        let r = panic::catch_unwind(|| {
            serde_json::from_slice::<serde_json::Value>(&input)
                .ok()
                .and_then(|v| open_envelope(&v, token, mac, endpoint))
        });
        match r {
            Ok(Some(_)) => opened += 1,
            Ok(None) => {}
            Err(_) => panic!(
                "open_envelope panicked on input {i} ({} bytes): {:02x?}",
                input.len(),
                &input[..input.len().min(96)]
            ),
        }
    }
    eprintln!(
        "fuzz_open_envelope_never_panics: {} iters in {:?} ({} opened), no panics",
        iters(),
        t0.elapsed(),
        opened
    );
}

#[test]
fn property_envelope_round_trip_and_fail_closed() {
    let mut rng = Rng(0x1656_67B1_9B48_7C11);
    let hostile_plaintexts: Vec<Vec<u8>> = vec![
        vec![],
        vec![0u8],
        b"{}".to_vec(),
        vec![0xff; 64],
        "unicode \u{1f600} \u{202e} rtl".as_bytes().to_vec(),
        vec![b'A'; 1 << 20], // 1 MiB
        (0..256).map(|i| i as u8).collect(),
    ];
    for (i, pt) in hostile_plaintexts.iter().enumerate() {
        let env = phone_seal(&mut rng, pt, TOKEN, MAC, ENDPOINT);
        let opened = open_envelope(&env, TOKEN, MAC, ENDPOINT);
        assert_eq!(
            opened.as_deref(),
            Some(pt.as_slice()),
            "case {i}: round-trip"
        );
        // AAD binding: wrong mac / endpoint / token fail closed.
        assert!(open_envelope(&env, TOKEN, "other-mac", ENDPOINT).is_none());
        assert!(open_envelope(&env, TOKEN, MAC, "http://evil/mobile").is_none());
        assert!(open_envelope(&env, "WRONG", MAC, ENDPOINT).is_none());
        // Tamper: flip one random byte of the sealed payload -> auth fails.
        let raw = serde_json::to_vec(&env).unwrap();
        let mut v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        let engine = base64::engine::general_purpose::STANDARD;
        let mut sealed = engine.decode(v["sealedB64"].as_str().unwrap()).unwrap();
        let pos = rng.below(sealed.len());
        sealed[pos] ^= 0x01;
        v["sealedB64"] = serde_json::Value::String(engine.encode(&sealed));
        assert!(
            open_envelope(&v, TOKEN, MAC, ENDPOINT).is_none(),
            "case {i}: tampered envelope opened"
        );
    }
    // Direction confusion: a response-direction envelope must not open as
    // a request.
    let resp = seal_envelope(b"hello", TOKEN, MAC, ENDPOINT).unwrap();
    assert!(open_envelope(&resp, TOKEN, MAC, ENDPOINT).is_none());
    // Malformed envelopes fail closed without panicking.
    for bad in [
        serde_json::json!({"v": 2, "saltB64": "AAAAAAAAAAAAAAAAAAAAAA==", "sealedB64": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}),
        serde_json::json!({"v": 1}),
        serde_json::json!({"v": 1, "saltB64": "AA==", "sealedB64": "AA=="}),
        serde_json::json!({"v": 1, "saltB64": "AAAAAAAAAAAAAAAAAAAAAA==", "sealedB64": "AA=="}),
        serde_json::json!([]),
        serde_json::json!(null),
    ] {
        assert!(open_envelope(&bad, TOKEN, MAC, ENDPOINT).is_none());
    }
    // seal_envelope output is structurally well-formed.
    let env = seal_envelope(b"data", TOKEN, MAC, ENDPOINT).unwrap();
    assert_eq!(env["v"], 1);
    let engine = base64::engine::general_purpose::STANDARD;
    assert_eq!(
        engine
            .decode(env["saltB64"].as_str().unwrap())
            .unwrap()
            .len(),
        16
    );
    assert!(
        engine
            .decode(env["sealedB64"].as_str().unwrap())
            .unwrap()
            .len()
            >= 12 + 16
    );
}
