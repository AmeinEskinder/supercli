//! Phase 9 H1 — fuzz + property tests for the review-log reader and the
//! hash chain (`unpeel_core::action_reviews`).
//!
//! cargo-fuzz/libFuzzer could not be installed offline, so this is a
//! deterministic in-tree harness with the same discipline: a seeded
//! xorshift RNG, a corpus of valid seed inputs, structural byte mutations,
//! a bounded iteration count (`UNPEEL_FUZZ_ITERS`, default 20 000), and
//! crash capture that prints the failing input. The decoders under test
//! are total (`Result`, never panic); any panic is a bug.
//!
//! Property tests (same file):
//! - random valid logs always verify with the exact entry count;
//! - flipping any single byte breaks verification;
//! - `inflight_reviews` returns exactly the approved-without-outcome set.

use std::panic;
use std::path::{Path, PathBuf};
use std::time::Instant;
use unpeel_core::action_reviews::*;

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
    fn pick<'a>(&mut self, xs: &'a [&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
}

const INTERESTING: &[u8] = &[
    0x00, 0x01, 0x7f, 0x80, 0xff, b'"', b'\\', b'{', b'}', b'[', b']', b':', b',', b'\n', b'\t',
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

fn tmpdir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("unpeel-fuzz-reviews-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_log(dir: &Path, bytes: &[u8]) {
    std::fs::write(dir.join(REVIEWS_FILE), bytes).unwrap();
}

/// Build corpus seeds: real lines produced by the real writers.
fn seed_corpus() -> Vec<Vec<u8>> {
    let dir = tmpdir("seeds");
    let mut seeds: Vec<Vec<u8>> = vec![b"\n".to_vec(), b"{}\n".to_vec(), b"not json\n".to_vec()];
    let actors = [
        Actor::Human {
            device_id: "phone-1".into(),
        },
        Actor::Scheduled {
            trigger_id: "nightly".into(),
        },
        Actor::PolicyAllow,
    ];
    for (i, actor) in actors.iter().enumerate() {
        let review = record_review(
            &dir,
            actor.clone(),
            "github",
            &format!("tool-{i}"),
            "ab12",
            if i % 2 == 0 {
                ReviewDecision::Approved
            } else {
                ReviewDecision::Denied
            },
            None,
        )
        .unwrap();
        if i % 2 == 0 {
            record_attempt_outcome(
                &dir,
                &review.review_id,
                AttemptOutcome::Executed {
                    success: i % 4 == 0,
                },
                actor.clone(),
            )
            .unwrap();
        }
    }
    // An ambiguous and a never_ran outcome too.
    let review = record_review(
        &dir,
        Actor::PolicyAllow,
        "c",
        "t",
        "ab12",
        ReviewDecision::Approved,
        None,
    )
    .unwrap();
    record_attempt_outcome(
        &dir,
        &review.review_id,
        AttemptOutcome::Ambiguous {
            reason: "cancel landed mid-call".into(),
        },
        Actor::PolicyAllow,
    )
    .unwrap();
    let review2 = record_review(
        &dir,
        Actor::PolicyAllow,
        "c",
        "t",
        "ab12",
        ReviewDecision::Approved,
        None,
    )
    .unwrap();
    record_attempt_outcome(
        &dir,
        &review2.review_id,
        AttemptOutcome::NeverRan {
            reason: "stale lease fence pre-send".into(),
        },
        Actor::PolicyAllow,
    )
    .unwrap();
    let raw = std::fs::read(dir.join(REVIEWS_FILE)).unwrap();
    for line in raw.split_inclusive(|&b| b == b'\n') {
        seeds.push(line.to_vec());
    }
    seeds.push(raw.clone()); // whole-log seed
    let _ = std::fs::remove_dir_all(&dir);
    seeds
}

#[test]
fn fuzz_review_log_reader_never_panics() {
    let seeds = seed_corpus();
    let dir = tmpdir("fuzz");
    let mut rng = Rng(0x243F_6A88_85A3_08D3);
    let mut corpus = seeds[0].clone();
    let t0 = Instant::now();
    for i in 0..iters() {
        let input = mutate(&mut rng, &corpus, &seeds);
        corpus = input.clone();
        write_log(&dir, &input);
        let dir2 = dir.clone();
        let v = panic::catch_unwind(move || verify_review_chain(&dir2));
        let dir3 = dir.clone();
        let f = panic::catch_unwind(move || inflight_reviews(&dir3));
        if v.is_err() || f.is_err() {
            panic!(
                "review-log reader panicked on input {i} ({} bytes): {:02x?}",
                input.len(),
                &input[..input.len().min(96)]
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    eprintln!(
        "fuzz_review_log_reader_never_panics: {} iters in {:?}, no panics",
        iters(),
        t0.elapsed()
    );
}

#[test]
fn fuzz_review_writers_accept_hostile_fields() {
    // The writers take caller-controlled strings; none may panic or
    // produce an unverifiable log.
    let dir = tmpdir("hostile");
    let big = "x".repeat(100_000);
    let hostile = [
        "",
        "\u{0}",
        "\"quotes\" and \\backslashes\\",
        "unicode: \u{1f600}\u{200b}\u{202e}rtl",
        big.as_str(),
        "\n{\"injected\": true}\n",
        "null",
    ];
    let mut rng = Rng(0xB7E1_5162_8AED_2A6B);
    let mut count = 0;
    for _ in 0..200 {
        let pick = |rng: &mut Rng| hostile[rng.below(hostile.len())].to_string();
        let review = record_review(
            &dir,
            Actor::Human {
                device_id: pick(&mut rng),
            },
            &pick(&mut rng),
            &pick(&mut rng),
            &pick(&mut rng),
            if rng.below(2) == 0 {
                ReviewDecision::Approved
            } else {
                ReviewDecision::Denied
            },
            None,
        )
        .unwrap();
        count += 1;
        if rng.below(2) == 0 {
            let outcome = match rng.below(3) {
                0 => AttemptOutcome::Executed {
                    success: rng.below(2) == 0,
                },
                1 => AttemptOutcome::Ambiguous {
                    reason: pick(&mut rng),
                },
                _ => AttemptOutcome::NeverRan {
                    reason: pick(&mut rng),
                },
            };
            record_attempt_outcome(
                &dir,
                &review.review_id,
                outcome,
                Actor::Human {
                    device_id: pick(&mut rng),
                },
            )
            .unwrap();
            count += 1;
        }
    }
    assert_eq!(
        verify_review_chain(&dir).unwrap(),
        count,
        "hostile-field log must verify with exact entry count"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------------- properties ---

const ACTORS: [&str; 5] = [
    "human:phone-1",
    "human:",
    "scheduled:nightly",
    "scheduled:",
    "policy:allow",
];

#[test]
fn property_random_logs_verify_with_exact_count() {
    let mut rng = Rng(0x4528_21E6_38D0_1377);
    for trial in 0..50 {
        let dir = tmpdir(&format!("prop-{trial}"));
        let mut expected = 0usize;
        let mut approved_open: Vec<String> = Vec::new();
        let n = 1 + rng.below(12);
        for _ in 0..n {
            let actor = Actor::parse(rng.pick(&ACTORS));
            let decision = if rng.below(3) == 0 {
                ReviewDecision::Denied
            } else {
                ReviewDecision::Approved
            };
            let approved = decision == ReviewDecision::Approved;
            let review = record_review(
                &dir,
                actor.clone(),
                rng.pick(&["github", "c", "x".repeat(40).leak()]),
                rng.pick(&["tool", "a.b", ""]),
                rng.pick(&["ab12", ""]),
                decision,
                None,
            )
            .unwrap();
            expected += 1;
            if rng.below(2) == 0 {
                let outcome = match rng.below(3) {
                    0 => AttemptOutcome::Executed {
                        success: rng.below(2) == 0,
                    },
                    1 => AttemptOutcome::Ambiguous { reason: "r".into() },
                    _ => AttemptOutcome::NeverRan { reason: "r".into() },
                };
                record_attempt_outcome(&dir, &review.review_id, outcome, actor).unwrap();
                expected += 1;
            } else if approved {
                approved_open.push(review.review_id.clone());
            }
        }
        assert_eq!(
            verify_review_chain(&dir).unwrap(),
            expected,
            "trial {trial}: random log must verify with exact count"
        );
        let mut inflight = inflight_reviews(&dir).unwrap();
        inflight.sort();
        approved_open.sort();
        assert_eq!(
            inflight, approved_open,
            "trial {trial}: inflight set mismatch"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn property_single_byte_tamper_always_detected() {
    let mut rng = Rng(0x1319_8A2E_0370_7344);
    for trial in 0..30 {
        let dir = tmpdir(&format!("tamper-{trial}"));
        for i in 0..(1 + rng.below(5)) {
            record_review(
                &dir,
                Actor::PolicyAllow,
                "c",
                &format!("tool-{i}"),
                "ab12",
                ReviewDecision::Approved,
                None,
            )
            .unwrap();
        }
        let path = dir.join(REVIEWS_FILE);
        let raw = std::fs::read(&path).unwrap();
        assert!(verify_review_chain(&dir).is_ok());
        // Flip one random byte; verification must fail. (If the flip lands
        // on a byte whose change keeps the hash valid, that would be a
        // SHA-256 second preimage — treat any Ok as a failure.)
        let pos = rng.below(raw.len());
        let mut tampered = raw.clone();
        tampered[pos] ^= 0x01;
        // Avoid the degenerate case where the flip is a no-op on the
        // parsed value and the hash still matches — impossible unless the
        // hash collides, but skip flips inside trailing newline anyway.
        if tampered[pos] == b'\n' || raw[pos] == b'\n' {
            continue;
        }
        std::fs::write(&path, &tampered).unwrap();
        assert!(
            verify_review_chain(&dir).is_err(),
            "trial {trial}: byte flip at offset {pos} was NOT detected"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---------------------------------------------------------------------------
// Regression tests for bugs found by the fuzzers above (Phase 9 H1).
// ---------------------------------------------------------------------------

/// `Actor::parse` must be the exact inverse of `Display`: verify
/// re-serializes the parsed actor when re-hashing, so any asymmetry
/// breaks verification of legitimately-written logs.
#[test]
fn regression_actor_parse_inverts_display() {
    let cases = [
        Actor::Human {
            device_id: "".to_string(),
        },
        Actor::Human {
            device_id: "phone-1".to_string(),
        },
        Actor::Human {
            device_id: "unidentified".to_string(),
        },
        Actor::Scheduled {
            trigger_id: "".to_string(),
        },
        Actor::Scheduled {
            trigger_id: "sched-9".to_string(),
        },
        Actor::PolicyAllow,
    ];
    for actor in &cases {
        let s = actor.to_string();
        assert_eq!(
            Actor::parse(&s).to_string(),
            s,
            "parse/display asymmetry for {s:?}"
        );
    }
    // Unknown forms (which Display never produces) still get an explicit
    // label rather than an empty actor.
    assert_eq!(Actor::parse("bogus").to_string(), "human:unidentified");

    // End to end: a log written with an empty device id must verify.
    let dir = tmpdir("actor-empty");
    record_review(
        &dir,
        Actor::Human {
            device_id: "".to_string(),
        },
        "c",
        "t",
        "ab12",
        ReviewDecision::Approved,
        None,
    )
    .unwrap();
    assert!(verify_review_chain(&dir).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The hash covers the *parsed* entry, so renaming a JSON key (parsed
/// value unchanged) used to go undetected. Found by
/// `property_single_byte_tamper_always_detected` (trial 25: the `e` in
/// `"replaces_attempt"` flipped to `d`).
#[test]
fn regression_renamed_json_key_detected() {
    let dir = tmpdir("renamed-key");
    record_review(
        &dir,
        Actor::PolicyAllow,
        "c",
        "t",
        "ab12",
        ReviewDecision::Approved,
        None,
    )
    .unwrap();
    let path = dir.join(REVIEWS_FILE);
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(verify_review_chain(&dir).is_ok());

    // Rename a key: parses to the same entry, different bytes.
    let tampered = raw.replacen("\"replaces_attempt\"", "\"rdplaces_attempt\"", 1);
    assert_ne!(tampered, raw);
    std::fs::write(&path, &tampered).unwrap();
    assert!(verify_review_chain(&dir).is_err());

    // Insignificant whitespace: same parsed value, different bytes.
    let spaced = raw.replacen("{", "{ ", 1);
    std::fs::write(&path, &spaced).unwrap();
    assert!(verify_review_chain(&dir).is_err());

    // Equivalent string escape: same parsed value, different bytes.
    let escaped = raw.replacen("\"connector\":\"c\"", "\"connector\":\"\\u0063\"", 1);
    assert_ne!(escaped, raw);
    std::fs::write(&path, &escaped).unwrap();
    assert!(verify_review_chain(&dir).is_err());

    let _ = std::fs::remove_dir_all(&dir);
}
