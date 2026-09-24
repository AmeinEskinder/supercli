# Changelog

All notable changes to Unpeel (Track B + Phase 9) are documented here.

## [0.8.0] — 2026-09-25

### Phase 9 H1 — Fuzz/property testing + hash-chain hardening

**Added:**
- Deterministic seeded in-tree fuzz/property harnesses (`UNPEEL_FUZZ_ITERS=20_000` default):
  - Event decoder, pairing envelope, connector manifest, review-log/hash-chain, lease state machine model, `CallOutcome` exhaustive property.

**Fixed (found by fuzzer):**
- `Actor::parse` was not the inverse of `Display` (empty device IDs broke verification on legitimate logs).
- **Tamper-evidence hole:** hash-chain verification hashed the *parsed* entry, not the raw bytes — key-rename, whitespace, and escape mutations were invisible to verification.

**Changed:**
- Added `ChainError::NonCanonical` + strict canonical-byte equality check. The verifier now requires exact byte-equality with the writer's canonical serialization before checking hash/link.
- Writer hashes the canonical projection (entry without `entry_hash`); verifier's byte-equality check makes the chain a pure deterministic function of on-disk bytes.
- **Compatibility:** The writer is byte-identical pre/post fix. Every log that verified before the fix still verifies. The fix only tightened verification.

**Docs corrected** (`docs/hash-chain-canonical-form.md`):
- Genesis `prev_hash` is `"genesis"` (was incorrectly documented as `"GENESIS"`).
- Decision vocabulary is `"approved"`/`"denied"` (was incorrectly documented as ask/allow/deny).
- Added `golden_vector_stored_line` test pinning the exact writer-emitted line.

### Phase 9 H2 — `unpeel migrate`

**Added:**
- New command: `unpeel migrate [--apply] [--json]` (dry-run by default).
  - **Connector grants:** Quarantines legacy bare-string grants (connector identity is never guessed; re-approval required). Namespaced grants untouched.
  - **Review logs:** Re-chains genuinely chainless (pre-chain-format) logs deterministically. Refuses to rewrite broken, tampered, or invalid chained logs (reported, never modified).
  - **Schedules/leases:** Preserves `schedules.json`; initializes/upgrades lease-DB schema idempotently without touching existing rows.
- Backup-first: all mutations create timestamped backups before writing.
- Idempotent: repeated `--apply` is a no-op (no backup spam).

### Phase 9 H3 — Host soak verification

**Verified:**
- 30+ minute Host soak (1900s load): 19,163 allow calls, 1,720 ask approvals, 57 turn-cancels, 21,067 events polled, **0 errors**.
- Event ring buffer: 512-event bound enforced and exercised (eviction verified).
- Memory: no leak (RSS returned to 211 MiB below start after idle).
- FDs: no leak (182 after idle vs 285 at start).
- Review-log lock: 0 timeouts under concurrent load.

**Product bug found (not fixed in this release):**
- `turn-cancel` has a TOCTOU race: it lists in-flight reviews, then marks each `Ambiguous`. A fast concurrent call completing between the list and the record → HTTP 500 ("already has a recorded outcome"). The soak harness uses a dedicated cancel session to avoid triggering this.

### Phase 9 H4 — Release engineering

**Added:**
- `docs/RELEASE_CHECKLIST.md`: workspace version + `cargo update`, test/clippy/fmt matrices, PTY matrix, notices check, 30-min Host soak, release order (CLI → Mac app → website).
- This CHANGELOG.md.

### Phase 9 H5 — README quickstart

**Added:**
- `README.md` `## Quickstart` section: Host installation/start, pairing with `unpeel pair`, first session, approval behavior, stopping/cancelling a turn, ambiguity/no-auto-retry semantics.

---

## [0.7.1] and earlier

See git history for changes prior to Phase 9.
