//! Phase 9 H1 — fuzz + property tests for the session-event wire decoder
//! (`supercli_client::events`).
//!
//! Same deterministic in-tree harness as `supercli-core/tests/fuzz_review_log.rs`:
//! cargo-fuzz/libFuzzer could not be installed offline, so a seeded
//! xorshift RNG, valid-JSON corpus seeds, and structural mutations drive
//! `serde_json::from_slice::<EventsResponse>` / `<SessionEventWire>` for a
//! bounded number of iterations. Decoders are total (`Result`, never panic).

use std::panic;
use std::time::Instant;
use supercli_client::events::{EventsResponse, SessionEventWire};

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
    std::env::var("SUPERCLI_FUZZ_ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20_000)
}

fn seeds() -> Vec<Vec<u8>> {
    // Real wire shapes (at_ms, variant-specific fields), so the fuzzer
    // exercises every known variant plus the Unknown catch-all.
    let valid_events = [
        r#"{"kind":"turn.started","session_id":"s1","seq":1,"at_ms":1700000000,"turn_id":"t1","trigger":"user"}"#,
        r#"{"kind":"turn.finished","session_id":"s1","seq":2,"at_ms":1700000001,"turn_id":"t1","outcome":"completed"}"#,
        r#"{"kind":"tool.requested","session_id":"s1","seq":3,"at_ms":1700000002,"review_id":"r1","tool":"github.search","summary":"search repos"}"#,
        r#"{"kind":"tool.approved","session_id":"s1","seq":4,"at_ms":1700000003,"review_id":"r1","answered_by":"human:phone-1"}"#,
        r#"{"kind":"tool.denied","session_id":"s1","seq":5,"at_ms":1700000004,"review_id":"r2","answered_by":null}"#,
        r#"{"kind":"needs_review","session_id":"s1","seq":6,"at_ms":1700000005,"review_id":"r2","reason":"ambiguous attempt"}"#,
        r#"{"kind":"turn.cancelled","session_id":"s1","seq":7,"at_ms":1700000006,"reason":"user stop","ambiguous_attempts":["r3"]}"#,
        r#"{"kind":"tool.executed","session_id":"s1","seq":8,"at_ms":1700000007,"review_id":"r1","exit":"ok"}"#,
        r#"{"kind":"tool.ambiguous","session_id":"s1","seq":9,"at_ms":1700000008,"review_id":"r3","reason":"cancelled mid-call"}"#,
        r#"{"kind":"tool.never_ran","session_id":"s1","seq":10,"at_ms":1700000009,"review_id":"r4","reason":"stale lease fence"}"#,
        r#"{"kind":"lease.fenced","session_id":"s1","seq":11,"at_ms":1700000010,"generation":3,"holder":"pid1-1"}"#,
        r#"{"kind":"lease.taken_over","session_id":"s1","seq":12,"at_ms":1700000011,"generation":4,"holder":"pid2-1","previous_holder":"pid1-1"}"#,
        r#"{"kind":"definitely.not.a.real.kind","session_id":"s1","seq":13,"at_ms":1700000012}"#,
        r#"{"kind":"tool.executed","session_id":"s1","seq":14,"at_ms":1700000013,"review_id":"r1","exit":"ok","extra":"ignored","nested":{"a":[1,2,3]}}"#,
    ];
    let mut out: Vec<Vec<u8>> = valid_events.iter().map(|s| s.as_bytes().to_vec()).collect();
    out.push(b"{}".to_vec());
    out.push(b"[]".to_vec());
    out.push(b"null".to_vec());
    out.push(b"not json".to_vec());
    let resp = format!(
        "{{\"events\":[{}],\"next_seq\":{}}}",
        valid_events.join(","),
        valid_events.len() + 1
    );
    out.push(resp.into_bytes());
    out
}

#[test]
fn fuzz_event_wire_decoder_never_panics() {
    let seeds = seeds();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut corpus = seeds[0].clone();
    let t0 = Instant::now();
    let mut ok_single = 0usize;
    let mut ok_resp = 0usize;
    for i in 0..iters() {
        let input = mutate(&mut rng, &corpus, &seeds);
        corpus = input.clone();
        // Single event decode must never panic.
        let single = panic::catch_unwind(|| serde_json::from_slice::<SessionEventWire>(&input));
        // Envelope decode must never panic.
        let resp = panic::catch_unwind(|| serde_json::from_slice::<EventsResponse>(&input));
        let (single, resp) = match (single, resp) {
            (Ok(s), Ok(r)) => (s, r),
            _ => panic!(
                "event decoder panicked on input {i} ({} bytes): {:02x?}",
                input.len(),
                &input[..input.len().min(96)]
            ),
        };
        if let Ok(ev) = single {
            ok_single += 1;
            // Invariants on every successfully decoded event.
            let _ = ev.kind();
            let _ = ev.session_id();
            let _ = ev.seq();
            // Round-trip through the wire form must preserve the value.
            let back: SessionEventWire =
                serde_json::from_value(serde_json::to_value(&ev).unwrap()).unwrap();
            assert_eq!(
                serde_json::to_value(&ev).unwrap(),
                serde_json::to_value(&back).unwrap(),
                "event round-trip changed the value on input {i}"
            );
        }
        if let Ok(envelope) = resp {
            ok_resp += 1;
            // next_seq must be consistent with the events present.
            for ev in &envelope.events {
                if let Some(seq) = ev.seq() {
                    assert!(
                        envelope.next_seq == 0 || seq < envelope.next_seq,
                        "input {i}: event seq {seq} >= next_seq {}",
                        envelope.next_seq
                    );
                }
            }
        }
    }
    eprintln!(
        "fuzz_event_wire_decoder_never_panics: {} iters in {:?} ({} single-ok, {} envelope-ok), no panics",
        iters(),
        t0.elapsed(),
        ok_single,
        ok_resp
    );
}

#[test]
fn property_unknown_kinds_decode_as_unknown() {
    // The protocol is Host-first and additive: a Controller must swallow
    // unknown event kinds, never choke on them.
    let long = "a".repeat(512);
    let kinds = [
        "turn.completed.v2",
        "tool.EXECUTED",
        "",
        "x",
        long.as_str(),
        "tool.executed ", // trailing space is NOT the known kind
    ];
    for kind in kinds {
        let raw = format!(
            r#"{{"kind":{},"session_id":"s","seq":1,"at_ms":1}}"#,
            serde_json::to_string(kind).unwrap()
        );
        let ev: SessionEventWire = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("unknown kind {kind:?} failed to decode: {e}"));
        assert!(
            matches!(ev, SessionEventWire::Unknown),
            "unknown kind {kind:?} must decode as Unknown"
        );
    }
}
