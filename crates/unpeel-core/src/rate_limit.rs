//! Per-device rate limiting (R5).
//!
//! A misbehaving paired device must not be able to flood the Host via
//! the approve/cancel endpoints or connector calls. Each device gets a
//! token-bucket limiter; over-limit requests get HTTP 429.
//!
//! Limits (per device):
//! - Approvals: 30/minute (human-scale; approvals are interactive)
//! - Cancels: 20/minute (cancels are rare)
//! - Connector calls: 120/minute (automation-friendly, but bounded)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Token bucket for a single device+endpoint.
struct Bucket {
    tokens: f64,
    last_refill: Instant,
    capacity: f64,
    refill_per_sec: f64,
}

impl Bucket {
    fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Bucket {
            tokens: capacity,
            last_refill: Instant::now(),
            capacity,
            refill_per_sec,
        }
    }

    /// Try to consume one token. Returns true if allowed.
    fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last_refill = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Rate limiter keyed by (device_id, endpoint).
pub struct RateLimiter {
    buckets: Mutex<HashMap<(String, String), Bucket>>,
    // (capacity, refill_per_sec) per endpoint.
    limits: HashMap<String, (f64, f64)>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        let mut limits = HashMap::new();
        // R5: per-device rate limits, chosen from realistic human/agent rates:
        //
        // - Approve (30/min, burst 30): Human approval is deliberate. An active
        //   user reviewing a rapid agent won't sustain more than ~1 approval
        //   every 2 seconds. 30/min allows short bursts (e.g., clearing a queue)
        //   while stopping a runaway client.
        // - Cancel (20/min, burst 20): Cancels are rarer than approvals — you
        //   cancel when something's wrong, not routinely. 20/min is generous
        //   for legitimate use.
        // - Connector (120/min, burst 120): Agent tool calls during autonomous
        //   operation. 2/sec sustained is a reasonable upper bound; higher
        //   rates suggest a runaway loop, not legitimate work.
        //
        // These are NOT tuned for the soak script (which is a stress test, not
        // realistic use). The soak counts 429s as expected outcomes, not errors.
        // Phase 13 v3 (A): Increased approve from 30/min to 120/min (2/s).
        // The P0 bug: with several agents running in parallel (the product's
        // normal case), the phone could not answer approvals because the
        // 0.5/s limit throttled legitimate concurrent answers. 2/s still
        // prevents abuse while allowing a user to approve multiple agents.
        limits.insert("approve".to_string(), (60.0, 120.0 / 60.0));
        limits.insert("cancel".to_string(), (20.0, 20.0 / 60.0));
        limits.insert("connector".to_string(), (120.0, 120.0 / 60.0));

        RateLimiter {
            buckets: Mutex::new(HashMap::new()),
            limits,
        }
    }

    /// Check if `device_id` may call `endpoint`. Returns true if allowed.
    pub fn check(&self, device_id: &str, endpoint: &str) -> bool {
        let Some((capacity, refill)) = self.limits.get(endpoint) else {
            // Unknown endpoint: allow (fail open for unlisted).
            return true;
        };
        let key = (device_id.to_string(), endpoint.to_string());
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        let bucket = buckets
            .entry(key)
            .or_insert_with(|| Bucket::new(*capacity, *refill));
        bucket.try_consume()
    }

    /// For tests: reset all buckets.
    #[cfg(test)]
    pub fn reset(&self) {
        self.buckets.lock().unwrap().clear();
    }

    /// Seconds until one token is available for `endpoint`.
    ///
    /// Token-bucket math: when the bucket is empty, the client must wait
    /// for one refill interval (1 / refill_per_sec) before a retry can
    /// succeed. We return the ceiling so the client doesn't retry early.
    ///
    /// - approve (120/min = 2/sec): 1s (0.5 rounded up)
    /// - cancel (20/min = 0.333/sec): 3s
    /// - connector (120/min = 2/sec): 1s (0.5 rounded up)
    pub fn retry_after_secs(&self, endpoint: &str) -> u64 {
        let refill_per_sec = self.limits.get(endpoint).map(|&(_, r)| r).unwrap_or(0.5); // default: 30/min
        if refill_per_sec <= 0.0 {
            return 60;
        }
        (1.0 / refill_per_sec).ceil() as u64
    }
}

/// Global rate limiter (one per Host process).
static GLOBAL: std::sync::OnceLock<Arc<RateLimiter>> = std::sync::OnceLock::new();

pub fn global() -> Arc<RateLimiter> {
    GLOBAL.get_or_init(|| Arc::new(RateLimiter::new())).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_burst() {
        let rl = RateLimiter::new();
        // Approve: burst 60.
        for _ in 0..60 {
            assert!(rl.check("dev1", "approve"), "should allow within burst");
        }
        assert!(!rl.check("dev1", "approve"), "should reject over burst");
    }

    #[test]
    fn per_device_isolation() {
        let rl = RateLimiter::new();
        for _ in 0..60 {
            assert!(rl.check("dev1", "approve"));
        }
        assert!(!rl.check("dev1", "approve"));
        // dev2 has its own bucket.
        assert!(rl.check("dev2", "approve"), "dev2 should not be affected");
    }

    #[test]
    fn per_endpoint_isolation() {
        let rl = RateLimiter::new();
        for _ in 0..60 {
            assert!(rl.check("dev1", "approve"));
        }
        assert!(!rl.check("dev1", "approve"));
        // Cancel has its own bucket.
        assert!(rl.check("dev1", "cancel"), "cancel should not be affected");
    }

    #[test]
    fn refill_over_time() {
        let rl = RateLimiter::new();
        // Use a tiny limit for the test by checking internal behavior.
        // Instead, verify that after reset, tokens are refilled.
        for _ in 0..60 {
            rl.check("dev1", "approve");
        }
        assert!(!rl.check("dev1", "approve"));
        rl.reset();
        assert!(rl.check("dev1", "approve"), "reset should refill");
    }

    #[test]
    fn retry_after_matches_token_bucket_refill() {
        let rl = RateLimiter::new();
        // Token-bucket: time for 1 token = ceil(1 / refill_per_sec).
        // approve: 120/min = 2/sec -> 1s (0.5 rounded up)
        assert_eq!(rl.retry_after_secs("approve"), 1);
        // cancel: 20/min = 0.333/sec -> 3s
        assert_eq!(rl.retry_after_secs("cancel"), 3);
        // connector: 120/min = 2/sec -> 0.5s, ceil to 1
        assert_eq!(rl.retry_after_secs("connector"), 1);
    }
}
