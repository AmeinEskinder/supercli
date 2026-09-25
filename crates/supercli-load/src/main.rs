//! Supercli load client: pooled TLS, open-loop, configurable concurrency/rate.
//!
//! Usage:
//!   supercli-load --hook-port 8080 --phone-port 8081 --token TOKEN \
//!     --concurrency 8 --rate 100 --duration 60
//!
//! Open-loop: requests are scheduled at a fixed rate (Poisson or constant),
//! independent of completions. This measures true host capacity, not the
//! generator's speed.

use clap::Parser;
use hdrhistogram::Histogram;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time::{interval, sleep};

#[derive(Parser, Debug)]
#[command(name = "supercli-load")]
#[command(about = "Supercli Host load tester (open-loop, pooled TLS)")]
struct Args {
    /// Hook listener port (for /mcp/approve-write)
    #[arg(long)]
    hook_port: u16,

    /// Phone port (for /mobile/bootstrap and /mobile/approvals/answer)
    #[arg(long)]
    phone_port: u16,

    /// Auth token for MCP requests.
    /// Phase 14 (0c): Prefer env (SUPERCLI_MCP_TOKEN) over argv to avoid
    /// exposing the bearer token in `ps` output.
    #[arg(long, env = "SUPERCLI_MCP_TOKEN")]
    mcp_token: String,

    /// Auth token for mobile requests.
    /// Phase 14 (0c): Prefer env (SUPERCLI_MOBILE_TOKEN) over argv to avoid
    /// exposing the bearer token in `ps` output.
    #[arg(long, env = "SUPERCLI_MOBILE_TOKEN")]
    mobile_token: String,

    /// Number of concurrent workers
    #[arg(long, default_value = "1")]
    concurrency: usize,

    /// Target rate in requests per second (0 = as fast as possible)
    #[arg(long, default_value = "0")]
    rate: u64,

    /// Test duration in seconds
    #[arg(long, default_value = "60")]
    duration: u64,

    /// Warmup duration in seconds (not measured)
    #[arg(long, default_value = "5")]
    warmup: u64,

    /// Number of requests for measured run (0 = use duration instead).
    /// When set, exactly this many arrivals are scheduled (genuine open-loop,
    /// no arrival skipping) and the run ends when all complete or timeout.
    #[arg(long, default_value = "0")]
    n: u64,

    /// Output machine-readable JSON results to this path (in addition to stdout).
    #[arg(long)]
    json_out: Option<String>,
}

#[derive(Serialize)]
struct ApproveRequest {
    caller_session_id: String,
    target_session_id: String,
}

#[derive(Deserialize, Debug)]
struct BootstrapResponse {
    #[serde(rename = "pendingApprovals", default)]
    pending_approvals: Vec<PendingApproval>,
    // Phase 13 v3 (B): long-poll generation counter.
    #[serde(rename = "approvalGeneration", default)]
    approval_generation: u64,
}

#[derive(Deserialize, Debug)]
struct PendingApproval {
    #[serde(default)]
    id: String,
    #[serde(rename = "callerSessionID", default)]
    caller_session_id: String,
}

#[derive(Serialize)]
struct AnswerRequest {
    id: String,
    approved: bool,
    // Phase 14 (0a): Client-generated nonce for idempotent retry. If the
    // answer POST's response is lost (transport error), the harness retries
    // with the same nonce and the server returns already_resolved with the
    // original decision instead of applying twice.
    nonce: String,
}

struct Stats {
    latencies: std::sync::Mutex<Histogram<u64>>, // microseconds
    errors: AtomicU64,
    completed: AtomicU64,
    offered: AtomicU64,
}

impl Stats {
    fn new() -> Self {
        Self {
            latencies: std::sync::Mutex::new(
                Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap(),
            ),
            errors: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            offered: AtomicU64::new(0),
        }
    }

    fn offered(&self) {
        self.offered.fetch_add(1, Ordering::Relaxed);
    }

    fn record(&self, latency: Duration) {
        let micros = latency.as_micros() as u64;
        if let Ok(mut h) = self.latencies.lock() {
            let _ = h.record(micros);
        }
        self.completed.fetch_add(1, Ordering::Relaxed);
    }

    fn error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Pooled TLS client: reqwest reuses connections by default
    let client = Client::builder()
        .danger_accept_invalid_certs(true) // Test certs are self-signed
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(args.concurrency * 2)
        .build()?;

    let hook_base = format!("https://127.0.0.1:{}", args.hook_port);
    let phone_base = format!("https://127.0.0.1:{}", args.phone_port);

    println!("Warming up for {}s...", args.warmup);
    // Warm-up uses IDs 0..N, measured run uses IDs starting at 1_000_000
    // to avoid collision with warm-up grants.
    run_load(
        &client,
        &hook_base,
        &phone_base,
        &args.mcp_token,
        &args.mobile_token,
        args.concurrency,
        args.rate,
        Duration::from_secs(args.warmup),
        false,
        0,
        0, // warmup: duration-based
    )
    .await?;

    if args.n > 0 {
        println!(
            "Running load test: n={} at {} concurrency...",
            args.n, args.concurrency
        );
    } else {
        println!("Running load test for {}s...", args.duration);
    }
    let test_start = Instant::now();
    let stats = run_load(
        &client,
        &hook_base,
        &phone_base,
        &args.mcp_token,
        &args.mobile_token,
        args.concurrency,
        args.rate,
        Duration::from_secs(args.duration),
        true,
        1_000_000,
        args.n,
    )
    .await?;
    let test_elapsed = test_start.elapsed().as_secs_f64();

    // Report. Phase 14 (0b): Every offered request must have a final state.
    // pending = offered - completed - errors. These three must sum to offered;
    // a request that vanishes (not in any bucket) is a bug.
    let offered = stats.offered.load(Ordering::Relaxed);
    let completed = stats.completed.load(Ordering::Relaxed);
    let errors = stats.errors.load(Ordering::Relaxed);
    let pending = offered.saturating_sub(completed + errors);

    println!("\n=== Results ===");
    println!(
        "Offered: {}, Completed: {}, Errors: {}, Pending: {}",
        offered, completed, errors, pending
    );
    assert_eq!(
        completed + errors + pending,
        offered,
        "accounting bug: completed({}) + errors({}) + pending({}) != offered({})",
        completed,
        errors,
        pending,
        offered
    );
    let (p50_ms, p95_ms, p99_ms, max_ms, throughput) = if completed > 0 {
        let tput = completed as f64 / test_elapsed;
        if let Ok(h) = stats.latencies.lock() {
            let p50 = h.value_at_quantile(0.50) as f64 / 1000.0;
            let p95 = h.value_at_quantile(0.95) as f64 / 1000.0;
            let p99 = h.value_at_quantile(0.99) as f64 / 1000.0;
            let max = h.max() as f64 / 1000.0;
            println!(
                "Throughput: {:.1}/s ({} completed in {:.1}s)",
                tput, completed, test_elapsed
            );
            println!("p50: {:.1}ms", p50);
            println!("p95: {:.1}ms", p95);
            println!("p99: {:.1}ms", p99);
            println!("max: {:.1}ms", max);
            (p50, p95, p99, max, tput)
        } else {
            (0.0, 0.0, 0.0, 0.0, 0.0)
        }
    } else {
        (0.0, 0.0, 0.0, 0.0, 0.0)
    };

    // Machine-readable JSON output
    if let Some(json_path) = &args.json_out {
        let output = serde_json::json!({
            "offered": offered,
            "completed": completed,
            "errors": errors,
            "pending": pending,
            "throughput_per_s": throughput,
            "p50_ms": p50_ms,
            "p95_ms": p95_ms,
            "p99_ms": p99_ms,
            "max_ms": max_ms,
            "concurrency": args.concurrency,
            "n": args.n,
        });
        std::fs::write(json_path, serde_json::to_string_pretty(&output).unwrap())?;
        println!("Wrote JSON results to {}", json_path);
    }

    Ok(())
}

async fn run_load(
    client: &Client,
    hook_base: &str,
    phone_base: &str,
    mcp_token: &str,
    mobile_token: &str,
    concurrency: usize,
    rate: u64,
    duration: Duration,
    measure: bool,
    id_offset: u64,
    n: u64,
) -> anyhow::Result<Arc<Stats>> {
    let stats = Arc::new(Stats::new());
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let start = Instant::now();
    // `end` is only used in duration-based mode (n == 0). In n-based mode,
    // the loop breaks when offered >= n.
    let end = start + duration;

    // Genuine open-loop: arrivals are scheduled independently of completions.
    // If n > 0, schedule exactly n arrivals (no skipping). If the system
    // falls behind, arrivals queue (we track offered vs completed explicitly).
    // If n == 0, use duration-based with a ticker (legacy mode).
    let use_n_based = n > 0;
    let mut ticker = if !use_n_based && rate > 0 {
        let mut i = interval(Duration::from_micros(1_000_000 / rate));
        // In duration mode, we still don't skip: use Delay to preserve
        // arrival count, but this can cause bursts under overload.
        // For accurate measurement, use n-based mode.
        i.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Some(i)
    } else {
        None
    };

    let mut req_id: u64 = id_offset;
    let mut offered: u64 = 0;
    let inter_arrival = if rate > 0 {
        Duration::from_micros(1_000_000 / rate)
    } else {
        Duration::from_micros(0)
    };

    loop {
        // Check termination
        if use_n_based {
            if offered >= n {
                break;
            }
        } else if Instant::now() >= end {
            break;
        }

        if use_n_based {
            // N-based: schedule arrival, then sleep for inter-arrival time.
            // Do NOT skip if behind — this is genuine open-loop.
            if offered > 0 && rate > 0 {
                sleep(inter_arrival).await;
            }
        } else if let Some(t) = ticker.as_mut() {
            t.tick().await;
        }

        // Open-loop: spawn task immediately (timer-driven arrival).
        // The TASK acquires the semaphore, not the main loop. This ensures
        // arrivals are independent of completions. If the semaphore is
        // saturated, tasks queue (measuring queueing delay in latency).
        let semaphore = semaphore.clone();
        let client = client.clone();
        let hook_base = hook_base.to_string();
        let phone_base = phone_base.to_string();
        let mcp_token = mcp_token.to_string();
        let mobile_token = mobile_token.to_string();
        let stats = stats.clone();
        let id = req_id;
        req_id += 1;
        offered += 1;
        stats.offered();

        tokio::spawn(async move {
            // Acquire inside the task (open-loop: arrival already happened)
            let _permit = match semaphore.acquire_owned().await {
                Ok(p) => p,
                Err(e) => {
                    if measure {
                        stats.error();
                    }
                    eprintln!("Semaphore acquire failed: {}", e);
                    return;
                }
            };
            match do_approve_cycle(
                &client,
                &hook_base,
                &phone_base,
                &mcp_token,
                &mobile_token,
                id,
            )
            .await
            {
                Ok(latency) => {
                    if measure {
                        stats.record(latency);
                    }
                }
                Err(e) => {
                    if measure {
                        stats.error();
                        // Debug: print first few errors
                        if stats.errors.load(Ordering::Relaxed) < 5 {
                            eprintln!("Request {} failed: {}", id, e);
                        }
                    }
                }
            }
            // _permit dropped here, releasing the semaphore
        });

        // If rate is 0 (as fast as possible), don't sleep
        if rate == 0 {
            // Yield to avoid starving the runtime
            tokio::task::yield_now().await;
        }
    }

    // Wait for in-flight requests. Phase 14 (0b): Drain ALL in-flight
    // before stopping. The harness must not abandon requests; every offered
    // request gets a final state (completed, error, or pending if the drain
    // timeout hits).
    let drain_start = Instant::now();
    let drain_timeout = Duration::from_secs(120);
    loop {
        let offered = stats.offered.load(Ordering::Relaxed);
        let completed = stats.completed.load(Ordering::Relaxed);
        let errors = stats.errors.load(Ordering::Relaxed);
        let in_flight = offered.saturating_sub(completed + errors);
        if in_flight == 0 {
            break;
        }
        if drain_start.elapsed() > drain_timeout {
            eprintln!(
                "WARNING: drain timeout after {:?}; {} requests still in-flight (will report as pending)",
                drain_timeout, in_flight
            );
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }

    Ok(stats)
}

/// One approve cycle: MCP POST (blocks) + phone poll + answer.
/// Returns the total latency.
async fn do_approve_cycle(
    client: &Client,
    hook_base: &str,
    phone_base: &str,
    mcp_token: &str,
    mobile_token: &str,
    id: u64,
) -> anyhow::Result<Duration> {
    let t0 = Instant::now();
    let caller = format!("load-{}", id);

    // Spawn MCP request (blocks until answered)
    // Note: MCP endpoint is HTTP (not HTTPS) with x-supercli-auth header
    let mcp_client = client.clone();
    let mcp_hook = hook_base.replace("https://", "http://");
    let mcp_tok = mcp_token.to_string();
    let mcp_caller = caller.clone();
    let mcp_handle = tokio::spawn(async move {
        let resp = mcp_client
            .post(format!("{}/mcp/approve-write", mcp_hook))
            .header("x-supercli-auth", mcp_tok)
            .json(&ApproveRequest {
                caller_session_id: mcp_caller.clone(),
                target_session_id: format!("{}-t", mcp_caller),
            })
            .send()
            .await?;
        // Check status
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("MCP failed: {}", body));
        }
        Ok(())
    });

    // Poll bootstrap until approval appears (GET, per working Python client).
    // Phase 13 v3 (A): Increased from 1s to 10s to handle server load.
    // Also check if MCP completed early (already_granted fast-path).
    // Phase 13 v3 (B): Use bootstrap long-poll (?wait_ms= +
    // ?after_approval_generation=) instead of timer polling. The server
    // blocks until the approval generation changes (new approval enqueued
    // or answered) or the timeout elapses. This is the same mechanism the
    // phone client uses — no 2-10s polling fallback.
    let mut pending_id: Option<String> = None;
    let mut after_gen: u64 = 0;
    // Initial fetch to get the current generation (no wait).
    for _ in 0..20 {
        // Check if MCP already completed (already_granted fast-path).
        // If so, no approval was created and we should not poll.
        if mcp_handle.is_finished() {
            // MCP done; check result.
            match mcp_handle.await {
                Ok(Ok(())) => {
                    // Already granted, success without approval.
                    return Ok(t0.elapsed());
                }
                Ok(Err(e)) => {
                    return Err(anyhow::anyhow!("MCP failed fast: {}", e));
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("MCP join failed: {}", e));
                }
            }
        }
        // Long-poll: wait up to 5s for the approval generation to change.
        let resp = client
            .get(format!(
                "{}/mobile/bootstrap?wait_ms=5000&after_approval_generation={}",
                phone_base, after_gen
            ))
            .header("Authorization", format!("Bearer {}", mobile_token))
            .send()
            .await?;
        // Validate status before parsing
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "bootstrap failed: {} | {}",
                status,
                &body[..body.len().min(200)]
            ));
        }
        let resp_text = resp.text().await?;
        let resp: BootstrapResponse = serde_json::from_str(&resp_text).map_err(|e| {
            anyhow::anyhow!(
                "JSON parse failed: {} | body: {}",
                e,
                &resp_text[..resp_text.len().min(500)]
            )
        })?;
        // Track generation for the next long-poll iteration.
        after_gen = resp.approval_generation;
        if let Some(a) = resp
            .pending_approvals
            .iter()
            .find(|a| a.caller_session_id == caller)
        {
            pending_id = Some(a.id.clone());
            break;
        }
    }

    let pid = pending_id.ok_or_else(|| anyhow::anyhow!("approval not found"))?;

    // Answer it. Phase 13 v3 (A): Check response status; 429 means rate
    // limited and the approval was NOT answered.
    // Phase 14 (0a): Generate one nonce per approval answer. If the POST
    // itself fails with a transport error (unknown outcome), retry with the
    // SAME nonce; the server returns already_resolved with the original
    // decision instead of applying twice.
    let answer_nonce = format!(
        "load-{}-{}",
        pid,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let send_answer = || {
        let client = client.clone();
        let phone_base = phone_base.clone();
        let mobile_token = mobile_token.clone();
        let pid = pid.clone();
        let answer_nonce = answer_nonce.clone();
        async move {
            client
                .post(format!("{}/mobile/approvals/answer", phone_base))
                .header("Authorization", format!("Bearer {}", mobile_token))
                .json(&AnswerRequest {
                    id: pid.clone(),
                    approved: true,
                    nonce: answer_nonce.clone(),
                })
                .send()
                .await
        }
    };
    let mut answer_resp = match send_answer().await {
        Ok(resp) => resp,
        Err(e) => {
            // Transport error on first attempt: outcome unknown. Retry once
            // with the same nonce; idempotency makes this safe.
            eprintln!(
                "answer transport error (unknown outcome), retrying with same nonce: {}",
                e
            );
            send_answer().await?
        }
    };
    // Phase 13 v3 (1): 429 retry with Retry-After. The phone must show the
    // rate limit, retry after the server's Retry-After, and never drop the
    // approval silently. The approval stays visible in bootstrap until it
    // is successfully answered.
    let mut answer_resp = answer_resp;
    for attempt in 0..5 {
        if answer_resp.status().is_success() {
            break;
        }
        if answer_resp.status().as_u16() == 429 {
            // Extract Retry-After (seconds); default to 1s.
            let retry_after: u64 = answer_resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(1)
                .clamp(1, 30);
            eprintln!(
                "answer rate-limited (429), retrying after {}s (attempt {}/5), approval {} stays pending",
                retry_after,
                attempt + 1,
                pid
            );
            tokio::time::sleep(Duration::from_secs(retry_after)).await;
            answer_resp = send_answer().await?;
            continue;
        }
        let status = answer_resp.status();
        let body = answer_resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "answer failed: {} | {}",
            status,
            &body[..body.len().min(200)]
        ));
    }
    if !answer_resp.status().is_success() {
        let status = answer_resp.status();
        let body = answer_resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "answer failed after retries: {} | {}",
            status,
            &body[..body.len().min(200)]
        ));
    }

    // Wait for MCP to complete
    mcp_handle
        .await?
        .map_err(|e| anyhow::anyhow!("MCP task failed: {}", e))?;

    Ok(t0.elapsed())
}
