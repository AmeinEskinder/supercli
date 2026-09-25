//! S4 chaos worker: runs c8 concurrent grant submissions until killed.
//!
//! Usage: s4_chaos_worker <unpeel_home> <concurrency> <ops_per_thread>
//!
//! Each thread submits grants via persist_grant_grouped. The process is
//! expected to be kill -9'd at a random point; the driver script then
//! restarts and verifies.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: s4_chaos_worker <unpeel_home> <concurrency> <ops_per_thread>");
        std::process::exit(2);
    }
    let home = args[1].clone();
    let concurrency: usize = args[2].parse().unwrap();
    let ops: usize = args[3].parse().unwrap();

    std::env::set_var("UNPEEL_HOME", &home);

    let barrier = Arc::new(Barrier::new(concurrency));
    let completed = Arc::new(AtomicU64::new(0));
    let acked = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];
    for t in 0..concurrency {
        let barrier = barrier.clone();
        let completed = completed.clone();
        let acked = acked.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            for i in 0..ops {
                // Bounded unique grants so the file doesn't grow unbounded.
                let caller = format!("s4-t{t}-{}", i % 20);
                let target = format!("s4-target-{}", i % 20);
                match unpeel_core::grant_writer::persist_grant_grouped(
                    "write",
                    &caller,
                    Some(&target),
                    Some("s4-device"),
                ) {
                    Ok(()) => {
                        acked.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        eprintln!("grant failed: {e}");
                    }
                }
                completed.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        let _ = h.join();
    }
    println!(
        "worker done: completed={} acked={}",
        completed.load(Ordering::Relaxed),
        acked.load(Ordering::Relaxed)
    );
}
