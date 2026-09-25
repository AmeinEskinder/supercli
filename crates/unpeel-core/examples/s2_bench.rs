//! S2 microbenchmark: measures edit_grants throughput/latency under concurrency.
//!
//! This isolates the grant_store lock contention from the full approval flow
//! (MCP + phone + etc.). Each thread performs n edit_grants calls, measuring
//! latency of each. Reports throughput and p50/p95/p99.

use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
use unpeel_core::grant_store::edit_grants;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let concurrency: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2000);
    
    // Use a temp home
    let home = std::env::temp_dir().join(format!("s2-bench-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("UNPEEL_HOME", &home);
    
    let barrier = Arc::new(Barrier::new(concurrency));
    let mut handles = vec![];
    let start = Instant::now();
    
    for t in 0..concurrency {
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            let mut latencies = Vec::with_capacity(n);
            for i in 0..n {
                let caller = format!("bench-t{}-{}", t, i);
                let t0 = Instant::now();
                let _ = edit_grants(|map| {
                    let entry = map
                        .entry("mcp_write_approvals".to_string())
                        .or_insert(serde_json::Value::Object(serde_json::Map::new()));
                    if let serde_json::Value::Object(obj) = entry {
                        obj.insert(
                            caller.clone(),
                            serde_json::Value::Array(vec![serde_json::Value::String(
                                "target".to_string(),
                            )]),
                        );
                    }
                    Ok::<(), String>(())
                });
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
    
    all_latencies.sort();
    let p50 = all_latencies[(total * 0.50) as usize];
    let p95 = all_latencies[(total * 0.95) as usize];
    let p99 = all_latencies[(total * 0.99) as usize];
    let max = all_latencies[all_latencies.len() - 1];
    
    println!("concurrency={} n={} total={}", concurrency, n, total as usize);
    println!("throughput={:.1}/s", throughput);
    println!("p50={:.1}ms", p50.as_secs_f64() * 1000.0);
    println!("p95={:.1}ms", p95.as_secs_f64() * 1000.0);
    println!("p99={:.1}ms", p99.as_secs_f64() * 1000.0);
    println!("max={:.1}ms", max.as_secs_f64() * 1000.0);
    
    // JSON output
    let output = serde_json::json!({
        "concurrency": concurrency,
        "n": n,
        "total": total as usize,
        "throughput_per_s": throughput,
        "p50_ms": p50.as_secs_f64() * 1000.0,
        "p95_ms": p95.as_secs_f64() * 1000.0,
        "p99_ms": p99.as_secs_f64() * 1000.0,
        "max_ms": max.as_secs_f64() * 1000.0,
    });
    println!("JSON:{}", serde_json::to_string(&output).unwrap());
    
    std::fs::remove_dir_all(&home).ok();
}
