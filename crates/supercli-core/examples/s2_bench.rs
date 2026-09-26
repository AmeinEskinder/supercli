//! S2 microbenchmark: measures grant persist throughput/latency under concurrency.
//!
//! Modes:
//!   direct  - pre-group-commit path: per-call audit fsync + direct edit_grants
//!   grouped - group commit: one writer, one audit fsync + one grants write per batch
//!
//! Usage: s2_bench <direct|grouped> <concurrency> <n>
//!
//! This isolates the grant persist path from the full approval flow
//! (MCP + phone + etc.). Each thread performs n persists, measuring
//! latency of each. Reports throughput and p50/p95/p99.

//! Note: this benchmark intentionally exercises `persist_grant_direct`
//! (deprecated/removed from production) to compare it against the grouped
//! path. That is the benchmark's purpose.
#![allow(deprecated)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Instant;
use supercli_core::grant_writer::{persist_grant_direct, persist_grant_grouped};

/// Signature shared by the direct and grouped persist paths.
type PersistFn = fn(&str, &str, Option<&str>, Option<&str>) -> Result<(), String>;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode: String = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "grouped".to_string());
    let concurrency: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let n: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(2000);

    if mode != "direct" && mode != "grouped" {
        eprintln!("usage: s2_bench <direct|grouped> <concurrency> <n>");
        std::process::exit(2);
    }

    // Use a temp home
    let home = std::env::temp_dir().join(format!("s2-bench-{}-{}", mode, std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("SUPERCLI_HOME", &home);

    let persist: PersistFn = if mode == "direct" {
        persist_grant_direct
    } else {
        persist_grant_grouped
    };

    let errors = Arc::new(AtomicU64::new(0));
    let barrier = Arc::new(Barrier::new(concurrency));
    let mut handles = vec![];
    let start = Instant::now();

    for t in 0..concurrency {
        let barrier = barrier.clone();
        let errors = errors.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            let mut latencies = Vec::with_capacity(n);
            for i in 0..n {
                // Bounded unique grants (10x10 combinations cycled): reflects
                // production where approvals are deduplicated via the
                // already_granted check. Unique-per-op would be O(n²) in JSON
                // parsing, measuring the wrong thing.
                let caller = format!("bench-t{}-{}", t, i % 10);
                let target = format!("target-{}", i % 10);
                let t0 = Instant::now();
                let r = persist("write", &caller, Some(&target), Some("bench-device"));
                if r.is_err() {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
                latencies.push(t0.elapsed());
            }
            latencies
        }));
    }

    let mut all_latencies = vec![];
    for h in handles {
        all_latencies.extend(h.join().unwrap());
    }

    let elapsed = start.elapsed();
    let total = all_latencies.len() as f64;
    let throughput = total / elapsed.as_secs_f64();
    let err_count = errors.load(Ordering::Relaxed);

    all_latencies.sort();
    let p50 = all_latencies[(total * 0.50) as usize];
    let p95 = all_latencies[(total * 0.95) as usize];
    let p99 = all_latencies[(total * 0.99) as usize];
    let max = all_latencies[all_latencies.len() - 1];

    println!(
        "mode={} concurrency={} n={} total={}",
        mode, concurrency, n, total as usize
    );
    println!("throughput={:.1}/s", throughput);
    println!("p50={:.1}ms", p50.as_secs_f64() * 1000.0);
    println!("p95={:.1}ms", p95.as_secs_f64() * 1000.0);
    println!("p99={:.1}ms", p99.as_secs_f64() * 1000.0);
    println!("max={:.1}ms", max.as_secs_f64() * 1000.0);
    println!("errors={}", err_count);

    // JSON output
    let output = serde_json::json!({
        "mode": mode,
        "concurrency": concurrency,
        "n": n,
        "total": total as usize,
        "throughput_per_s": throughput,
        "p50_ms": p50.as_secs_f64() * 1000.0,
        "p95_ms": p95.as_secs_f64() * 1000.0,
        "p99_ms": p99.as_secs_f64() * 1000.0,
        "max_ms": max.as_secs_f64() * 1000.0,
        "errors": err_count,
    });
    println!("JSON:{}", serde_json::to_string(&output).unwrap());

    std::fs::remove_dir_all(&home).ok();
}
