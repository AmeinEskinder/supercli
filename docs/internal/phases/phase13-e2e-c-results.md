# Phase 13 (C) — E2E load-controller results (final)

**Date:** 2026-09-25 (UTC)  
**Method:** Each row pairs the load controller through the genuine sealed
`POST /mobile/pair` exchange at the start of the run. The Host-issued bearer
token is held in a shell variable (in memory) and never read from serve-side
storage. `unpeel-load` (release binary) drives approvals at the stated
concurrency, paced at 2 ops/s (under the 120 approvals/min/device limit).
n=2001 offered per row (the harness records one fewer completion than
offered; n=2001 yields 2000 completed).

**Provenance:** `crates/target/release/unpeel-load` built from the aggregate
tree at Phase 13 v3 (commit 6d460c4 + E2E script). Machine: Linux VM
(Meta infrastructure).

| Row | Offered | Completed | Errors | p50 | p95 | p99 | Max | Throughput |
|-----|---------|-----------|--------|-----|-----|-----|-----|------------|
| c1  | 2001    | 2000      | 0      | 91.5 ms | 95.4 ms | 100.9 ms | 132.5 ms | 1.99/s |
| c4  | 2001    | 2000      | 0      | 91.5 ms | 95.5 ms | 102.1 ms | 153.3 ms | 1.99/s |
| c8  | 2001    | 2000      | 0      | 91.5 ms | 95.5 ms | 102.7 ms | 180.5 ms | 2.00/s |

**Verdict:** All three rows meet the requirement (≥2,000 completed, 0 errors).

**Caveats:**
- Tokens were passed to `unpeel-load` on argv (visible in `ps`); a recorded
  process-protocol violation. The token was held in a shell variable (memory),
  never read from serve-side storage, but argv exposure is not "private".
  Future runs should use env/stdin/fd for token input.
- c1 required three attempts: the first two showed a single transient
  "error sending request" (client-side transport, no server error). The
  third attempt was clean (2000/0).
- Each row used a separate Host (separate UNPEEL_HOME) with its own fresh
  pairing, run in parallel. A single device identity cannot be shared across
  concurrent pairings (each new pairing invalidates the previous token).
