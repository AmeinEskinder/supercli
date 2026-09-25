# Threat Model — Phases 9–13 (STRIDE)

## Scope

This refresh covers changes from Phase 9 (deterministic fuzzing, migrate,
soak), Phase 10 (R1–R5), Phase 11 (P1–P5: accessibility, backup/restore,
config validation, supply chain, first-run), Phase 12 (Q0–Q6), and Phase 13
(S1–S8: load client, grant store sharding, MCP delay, chaos, fuzzing, STRIDE,
soak, release checklist).

## Assets

- User approvals (grants) in `grants.json`
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

### Tampering

| Threat | Phase | Mitigation |
|--------|-------|------------|
| Grant file tampering | 13-S2 | Temp+fsync+rename+dir fsync (atomic); no hash chain (grants are not the authz record) |
| Review log tampering | 9-H1 | Hash chain with `entry_hash`/`prev_hash`; `ChainError::NonCanonical` on byte mismatch |
| Backup tampering | 11-P2 | SHA-256 manifest; restore verifies before install |
| Config tampering | 11-P3 | Typed schema; host refuses invalid config |

### Repudiation

| Threat | Phase | Mitigation |
|--------|-------|------------|
| Deny approval | 13-S2 | Grant persists after explicit approval; review log records actor |
| Deny tool execution | 9-H1 | Write-ahead review log with actor field (human device id, trigger id, or policy:Allow) |

### Information Disclosure

| Threat | Phase | Mitigation |
|--------|-------|------------|
| Command text in diagnostics | 13 | `doctor --bundle` excludes review payloads and command text (planted assertion) |
| Secrets in logs | 11-P2 | Redaction in `collect_logs`; `[REDACTED]` markers |

### Denial of Service

| Threat | Phase | Mitigation |
|--------|-------|------------|
| Grant lock contention | 13-S2 | Optimistic concurrency (serialization outside lock); sharded from app-state.json |
| Review log lock contention | 12-Q5 | Identified as bottleneck; S2 fix reduces contention |

### Elevation of Privilege

| Threat | Phase | Mitigation |
|--------|-------|------------|
| Grant without approval | 13-S2 | `persist_grant` only called after `hub.request()` returns ok; crash before persist = re-prompt (fail closed) |
| Stale lease abuse | 8-S3 | `AttemptOutcome::NeverRan` for stale-lease fence; takeover re-fires |

## Residual Risks

- **UDP punch tests**: Fail in sandboxes without UDP (environmental, not a product risk).
- **Mobile server**: Requires explicit enablement; not auto-started in test harness.
- **Supply chain**: cargo-deny/audit configured; exceptions documented.
