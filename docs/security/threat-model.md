# Threat Model — Phases 9–13 (STRIDE)

## Scope

This refresh covers changes from Phase 9 (deterministic fuzzing, migrate,
soak), Phase 10 (R1–R5), Phase 11 (P1–P5: accessibility, backup/restore,
config validation, supply chain, first-run), Phase 12 (Q0–Q6), and Phase 13
(S1–S8: load client, grant store sharding, MCP delay, chaos, fuzzing, STRIDE,
soak, release checklist), plus Phase 13 v2 (grant audit hash chain, group-commit
writer, /mobile/events long-poll, quarantine, startup reconciliation).

## Assets

- User approvals (grants) in `grants.json`
- Grant audit hash chain (`grant-audit.jsonl`) — tamper-evident grant creation log
- Review log with hash chain (`action-reviews.jsonl`)
- Pairing secrets and sealed envelopes
- Configuration files
- Backup archives

## STRIDE Analysis

### Spoofing

| Threat | Phase | Mitigation |
|--------|-------|------------|
| Fake approval hub | 13-S2 | Grants only written after `hub.request()` returns ok=true (explicit user approval) |
| Spoofed mobile device | 9-H3 | Pairing requires explicit code/QR; sealed envelope crypto |
| Spoofed grant actor | 13v2 | `grant_created` audit entry records `actor` explicitly; `unknown` (not `policy:Allow`) when unattributable; no silent default |

### Tampering

| Threat | Phase | Mitigation |
|--------|-------|------------|
| Grant file tampering | 13v2 | **CHANGED**: Grants now have tamper-evident audit chain (`grant-audit.jsonl` with `prev_hash`/`entry_hash`); startup reconciliation verifies `grants ⊆ chain`; unaudited grants quarantined |
| Audit chain fork (concurrent writers) | 13v2 | Group-commit writer serializes all grant mutations through single writer thread; batch fsync; direct mode documented unsafe under concurrency |
| Review log tampering | 9-H1 | Hash chain with `entry_hash`/`prev_hash`; `ChainError::NonCanonical` on byte mismatch |
| Backup tampering | 11-P2 | SHA-256 manifest; restore verifies before install |
| Config tampering | 11-P3 | Typed schema; host refuses invalid config |
| Quarantine bypass | 13v2 | Quarantine uses temp+fsync+rename+dir fsync; atomic removal from live grants |

### Repudiation

| Threat | Phase | Mitigation |
|--------|-------|------------|
| Deny approval | 13v2 | Grant persists after explicit approval; **audit chain** records actor, scope, tool with hash linkage (stronger than grants.json alone) |
| Deny tool execution | 9-H1 | Write-ahead review log with actor field (human device id, trigger id, or policy:Allow) |
| Deny grant creation | 13v2 | `grant_created` audit entry is write-ahead (fsynced before grants.json); crash after audit but before grants.json leaves audit-only entry (revoked, not granted) |

### Information Disclosure

| Threat | Phase | Mitigation |
|--------|-------|------------|
| Command text in diagnostics | 13 | `doctor --bundle` excludes review payloads and command text (planted assertion) |
| Secrets in logs | 11-P2 | Redaction in `collect_logs`; `[REDACTED]` markers |
| Grant keys in audit | 13v2 | Audit stores flattened grant keys (not full grant values); keys use %-escaping to prevent injection |

### Denial of Service

| Threat | Phase | Mitigation |
|--------|-------|------------|
| Grant lock contention | 13v2 | **CHANGED**: Single writer thread with group commit; batch fsync amortizes cost; throughput scales 284→793/s (c1→c16); no per-op flock |
| Writer thread panic | 13v2 | Batch commit is panic-contained; panicking batch errors its callers, writer continues; poisoned locks recovered via `into_inner` |
| Writer queue exhaustion | 13v2 | Unbounded channel; callers block on ack (backpressure); no timeout (callers wait for durability or disconnect) |
| Long-poll resource exhaustion | 13v2 | `/mobile/events` max 25s wait; 1s slices allow cancellation; Condvar-based (no busy loop) |
| Review log lock contention | 12-Q5 | Identified as bottleneck; S2 fix reduces contention |

### Elevation of Privilege

| Threat | Phase | Mitigation |
|--------|-------|------------|
| Grant without approval | 13v2 | `persist_grant` only called after `hub.request()` returns ok; **audit entry written first**; crash before audit = no grant (fail closed) |
| Grant without audit (privilege persistence) | 13v2 | Startup reconciliation quarantines grants without audit entries; doctor verifies `grants ⊆ chain` |
| Audit-only entry abused as grant | 13v2 | Audit without grant remains revoked; reconciliation does not create grants from audit |
| Stale lease abuse | 8-S3 | `AttemptOutcome::NeverRan` for stale-lease fence; takeover re-fires |

## Residual Risks

- **UDP punch tests**: Fail in sandboxes without UDP (environmental, not a product risk).
- **Mobile server**: Requires explicit enablement; not auto-started in test harness.
- **Supply chain**: cargo-deny/audit configured; exceptions documented.
- **Exact device provenance**: Mobile approval records `human:paired-device` (generic); exact paired-device ID not yet threaded. The actor is not falsely attributed, but granularity is limited.
- **Separate grant chain**: Grant audit uses separate `grant-audit.jsonl` chain (not merged into action-review chain). 

### Separate grant chain design (pending acceptance)

The grant audit log (`grant-audit.jsonl`) uses a hash chain separate from the per-session action-review chains (`action-reviews.jsonl`). Rationale:
- Grants are global (not per-session); the review chain is per-session.
- Grant lifecycle (create, reconcile, quarantine) is independent of tool execution reviews.
- Merging would require a global review chain, a larger design change.

The separate chain is hash-chained (SHA-256, `prev_hash`/`entry_hash`), fsynced before `grants.json` (audit-first ordering), covered by `unpeel backup` (HOME_FILES includes `grant-audit.jsonl`), verified on restore (chain must verify), and checked by doctor (`grants ⊆ chain`; grant without chain entry is quarantined).

This design is a deliberate choice, not a limitation. It awaits explicit acceptance.

### Cross-process audit fork (FIXED)

**Previous risk**: Two processes (e.g. Host and CLI) appending to `grant-audit.jsonl` could both read the same `prev_hash` and append entries with the same `prev_hash`, forking the chain.

**Fix (2026-09-25)**: `record_grants_created_batch` now acquires an exclusive `flock` on `grant-audit.jsonl.lock` before the read-tail + append + fsync sequence. The lock is held for the entire batch. A crashed holder cannot wedge the lock (kernel releases flock on process exit). The 30s timeout fails closed.

This was a chain-integrity bug, not a residual risk. It is fixed.
