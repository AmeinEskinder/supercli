//! Phase 9 H1 — fuzz + property tests for the connector manifest parser
//! (`supercli_connector::manifest::parse_manifest`).
//!
//! Same deterministic in-tree harness as the other H1 fuzz targets:
//! cargo-fuzz/libFuzzer could not be installed offline, so a seeded
//! xorshift RNG, a corpus of valid manifests, and structural mutations
//! drive the parser for a bounded number of iterations. The parser is
//! total (`Result`, never panic); any panic is a bug.
//!
//! Properties on every successful parse:
//! - `name` satisfies the parser's own name grammar;
//! - `provides` is non-empty with every tool name valid;
//! - every `policy` key names a provided tool;
//! - `default_policy` agrees with the parsed `policy` map (Ask default).

use std::panic;
use std::time::Instant;
use supercli_connector::manifest::{parse_manifest, ApprovalPolicy};

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
    0x00, 0x01, 0x7f, 0x80, 0xff, b'"', b'\\', b'{', b'}', b'[', b']', b':', b',', b'\n', b'=',
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

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

fn valid_tool_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|seg| {
            !seg.is_empty()
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

fn corpus() -> Vec<Vec<u8>> {
    vec![
        br#"[connector]
name = "github"
version = "1.2.3"
display_name = "GitHub"
description = "GitHub API"
kind = "mcp-http"

[auth]
flow = "oauth2"
scopes = ["repo", "read:user"]

[tools]
provides = ["search", "mail.search", "get-issue"]

[policy]
search = "ask"
"mail.search" = "allow"

[oauth]
authorize_url = "https://github.com/login/oauth/authorize"
token_url = "https://github.com/login/oauth/access_token"
"#
        .to_vec(),
        br#"[connector]
name = "x"
version = "0.0.1"
display_name = "X"
description = "minimal"
kind = "builtin"

[tools]
provides = ["t"]
"#
        .to_vec(),
        br#"[connector]
name = "bad name!"
version = "not-semver"
display_name = ""
description = ""
kind = "nope"

[tools]
provides = []
"#
        .to_vec(),
        b"".to_vec(),
        b"[unclosed".to_vec(),
        {
            // Invalid UTF-8 inside the TOML.
            let mut v = b"[connector]\nname = \"".to_vec();
            v.extend_from_slice(&[0xff, 0xfe]);
            v.extend_from_slice(b"\"");
            v
        },
        br#"[connector]
name = "dup"
version = "1.0.0"
display_name = "D"
description = "D"
kind = "mcp-stdio"
[connector]
name = "dup2"
"#
        .to_vec(),
    ]
}

#[test]
fn fuzz_manifest_parser_never_panics() {
    let seeds = corpus();
    let mut rng = Rng(0xAB98_4E6B_3C2D_1F0A);
    let mut buf = seeds[0].clone();
    let t0 = Instant::now();
    let mut ok = 0usize;
    for i in 0..iters() {
        let input = mutate(&mut rng, &buf, &seeds);
        buf = input.clone();
        // TOML is lossy under arbitrary bytes; parse what we can. A panic
        // here (in toml parsing or in our validation) is the bug we hunt.
        let text = String::from_utf8_lossy(&input);
        let r = panic::catch_unwind(|| parse_manifest(&text));
        match r {
            Ok(Ok(m)) => {
                ok += 1;
                // Invariants on every accepted manifest.
                assert!(valid_name(&m.name), "input {i}: bad name accepted");
                assert!(!m.provides.is_empty(), "input {i}: empty provides accepted");
                for t in &m.provides {
                    assert!(
                        valid_tool_name(t),
                        "input {i}: bad tool name {t:?} accepted"
                    );
                }
                for tool in m.policy.keys() {
                    assert!(
                        m.provides.contains(tool),
                        "input {i}: policy for unprovided tool {tool:?}"
                    );
                }
                for tool in &m.provides {
                    let expected = m.policy.get(tool).copied().unwrap_or(ApprovalPolicy::Ask);
                    assert_eq!(
                        m.default_policy(tool),
                        expected,
                        "input {i}: default_policy disagrees with policy map"
                    );
                }
                assert_eq!(
                    m.default_policy("definitely.not.provided"),
                    ApprovalPolicy::Ask,
                    "input {i}: unlisted tool must default to Ask"
                );
            }
            Ok(Err(_)) => {}
            Err(_) => panic!(
                "parse_manifest panicked on input {i} ({} bytes): {:02x?}",
                input.len(),
                &input[..input.len().min(96)]
            ),
        }
    }
    eprintln!(
        "fuzz_manifest_parser_never_panics: {} iters in {:?} ({} accepted), no panics",
        iters(),
        t0.elapsed(),
        ok
    );
}

#[test]
fn property_manifest_rejections_are_stable() {
    // Rejection must be deterministic: the same input always yields the
    // same error, never an accept-after-reject flip.
    let bad = [
        "[connector]\nname = \"UPPER\"\nversion = \"1.0.0\"\ndisplay_name = \"d\"\ndescription = \"d\"\nkind = \"builtin\"\n[tools]\nprovides = [\"t\"]\n",
        "[connector]\nname = \"ok\"\nversion = \"1.0.0\"\ndisplay_name = \"d\"\ndescription = \"d\"\nkind = \"builtin\"\n[tools]\nprovides = [\"t\"]\n[policy]\nunknown = \"ask\"\n",
        "[connector]\nname = \"ok\"\nversion = \"1.0.0\"\ndisplay_name = \"d\"\ndescription = \"d\"\nkind = \"builtin\"\n",
    ];
    for (i, text) in bad.iter().enumerate() {
        let first = format!("{:?}", parse_manifest(text));
        for _ in 0..10 {
            assert_eq!(
                format!("{:?}", parse_manifest(text)),
                first,
                "case {i}: rejection not deterministic"
            );
        }
        assert!(
            parse_manifest(text).is_err(),
            "case {i}: expected rejection"
        );
    }
}
