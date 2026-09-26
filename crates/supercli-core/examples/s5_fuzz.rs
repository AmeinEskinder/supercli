//! Lightweight fuzzer for S5 (cargo-fuzz ASAN build infeasible for large crate).
//!
//! Usage: s5_fuzz <target> <duration_secs>
//!   target: "sealed_envelope" or "config"
//!
//! Generates random inputs, calls the parser, catches panics.
//! Reports crash count and first crash input.

use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: s5_fuzz <sealed_envelope|config> <duration_secs>");
        std::process::exit(2);
    }
    let target = &args[1];
    let duration = Duration::from_secs(args[2].parse().unwrap());

    let start = Instant::now();
    let mut iterations: u64 = 0;
    let mut crashes: u64 = 0;
    let mut first_crash: Option<Vec<u8>> = None;

    // Simple xorshift RNG (no external deps).
    let mut rng_state: u64 = 0x123456789abcdef;

    while start.elapsed() < duration {
        // Generate random input: 0-256 bytes.
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        let len = (rng_state % 257) as usize;
        let mut input = Vec::with_capacity(len);
        for _ in 0..len {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            input.push((rng_state & 0xFF) as u8);
        }

        // Call parser, catch panics.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if target == "sealed_envelope" {
                let _ = supercli_core::relay_crypto::decode_incoming(&input);
            } else if target == "config" {
                if let Ok(s) = std::str::from_utf8(&input) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                        let _ = supercli_core::config::check_document(&v);
                    }
                }
            }
        }));

        if result.is_err() {
            crashes += 1;
            if first_crash.is_none() {
                first_crash = Some(input.clone());
            }
        }
        iterations += 1;

        if iterations.is_multiple_of(100000) {
            eprintln!(
                "progress: {} iterations, {} crashes, elapsed {:?}",
                iterations,
                crashes,
                start.elapsed()
            );
        }
    }

    println!("S5 fuzz complete:");
    println!("  target: {target}");
    println!("  iterations: {iterations}");
    println!("  crashes: {crashes}");
    println!("  duration: {:?}", start.elapsed());
    if let Some(crash) = first_crash {
        println!("  first crash input (hex): {}", hex_encode(&crash));
    }

    if crashes > 0 {
        std::process::exit(1);
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
