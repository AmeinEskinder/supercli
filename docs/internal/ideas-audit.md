# Ideas Audit: OpenMuse Inspiration → Implementation

**Date:** 2026-09-26
**Purpose:** Track every idea taken from OpenMuse (the product) and every major feature proposed across the build phases, with implementation status. Answer the code-copy question definitively.

---

## 1. OpenMuse-Inspired Ideas

These are product concepts inspired by Muse (the AI product by Meta), not code. Osman came to Muse via a Tobi Lütke recommendation and holds that "the product harness (persistent computer, memory, scheduler, connectors, approval flows, polish) — not the underlying model weights — is what differentiates Muse."

| Idea | Status | Implementation | Test |
|------|--------|----------------|------|
| **Persistent memory** — harness remembers user preferences, coding style, past decisions | Implemented | `crates/unpeel-core/src/memory.rs` (session memory), `~/memory/` agent-side | `memory_persists_across_sessions` |
| **Scheduled proactivity** — scheduled triggers that run tasks autonomously | Implemented | `crates/unpeel-core/src/scheduler.rs`, `unpeel schedule` CLI | `scheduler_fires_at_time`, `scheduled_trigger_records_actor` |
| **Connectors** — developer-written plugin system (connector.toml) | Implemented | `crates/unpeel-connector/`, `runtimes/` packages, `connector.toml` spec | `connector_loads_toml`, `connector_mcp_roundtrip` |
| **Approval flows** — Allow/Ask/Deny for agent actions with UI | Implemented | `crates/unpeel-core/src/approval.rs`, `ApprovalHub`, Dioxus approval cards | `approval_approve_denies_correctly`, `approval_card_keyboard_approve` |
| **Browser worker** — browser with saved logins, live takeover | Partial | `crates/unpeel-core/src/browser.rs`, `unpeel browser` CLI, agent-browser engine pinning | `browser_install_verifies_hash` (engine download tested; live takeover untested) |
| **Stable threads** — rename/archive/restore/replay | Implemented | `crates/unpeel-core/src/session.rs` (session lifecycle), archive/restore | `session_rename_persists`, `session_archive_restore_roundtrip` |
| **Ideas surface** — a place to capture and develop ideas | Not implemented | — | — |
| **Chat UI layer** — t3.chat/Codex-like chat over the CLI | Partial | `clients/dioxus/unpeel-web/` (Dioxus web UI with composer) | Playwright 17/17 (a11y + keyboard) |
| **Unified registrar** — one registry for skills/runtimes/MCP/hooks, zero per-item config | Implemented | `crates/unpeel-core/src/registry.rs`, `runtimes/` auto-discovery | `registry_discovers_all_runtimes` |
| **Proactive learning** — learns coding style over time | Not implemented | — | — |

---

## 2. Phase Features (Track B)

### Phase 5 — Safety model
| Feature | Status | Implementation | Test |
|---------|--------|----------------|------|
| No hidden retry after ambiguous external writes | Implemented | `crates/unpeel-core/src/attempt.rs` (`AttemptOutcome`) | `ambiguous_does_not_retry_silently` |
| Stored action-review records (write-ahead, fsync before execute) | Implemented | `crates/unpeel-core/src/action_reviews.rs` | `review_written_before_tool_executes` |
| Tamper-evident hash chain (each entry hashes previous) | Implemented | `action_reviews.rs` (`ChainError::NonCanonical`) | `hash_chain_detects_tampered_byte` |
| Explicit actor on every path (human/scheduled/policy) | Implemented | `Actor` enum in `action_reviews.rs` | `actor_parse_display_roundtrip` |
| SQL-lease task claiming with expiry/takeover | Implemented | `crates/unpeel-core/src/leases.rs` | `lease_takeover_after_expiry` |

### Phase 7/8 — Hardening
| Feature | Status | Implementation | Test |
|---------|--------|----------------|------|
| `AttemptOutcome::NeverRan` (stale-lease fence) | Implemented | `crates/unpeel-core/src/attempt.rs` | `never_ran_skips_reconcile` |
| Genuine sealed pairing (E2E encrypted) | Implemented | `crates/unpeel-core/src/pairing.rs` | Sealed pairing E2E 27/27 |
| Hash-chain canonical form | Implemented | `docs/internal/hash-chain-canonical-form.md`, `action_reviews.rs` | `chain_rejects_non_canonical` |
| F1–F4 safety fixes (tool completion records Executed; review-log concurrent writer) | Implemented | `crates/unpeel-core/src/` | F1–F4 regression tests |

### Phase 9 — Fuzzing & release
| Feature | Status | Implementation | Test |
|---------|--------|----------------|------|
| Deterministic fuzz/property harnesses (20k iters) | Implemented | `crates/unpeel-core/` fuzz targets | `fuzz_actor_roundtrip`, `fuzz_envelope` |
| `unpeel migrate` (legacy grants → new format) | Implemented | `crates/unpeel-cli/src/migrate_cli.rs` | `migrate_dry_run_quarantines` |
| 30-min Host soak | Implemented | `scripts/soak.sh` | 19,163 allow / 1,720 ask / 0 errors |

### Phase 10 — Correctness
| Feature | Status | Implementation | Test |
|---------|--------|----------------|------|
| Turn-cancel TOCTOU fix (500 → OutcomeUnknown) | Implemented | `crates/unpeel-serve/src/` | `correlated_effect_failure_kind` |
| Lock poisoning audit (27 `.lock().unwrap()` → `into_inner`) | Implemented | `crates/unpeel-core/src/` (27 sites) | `docs/security/poison-recovery-audit.md` |
| Release/dist profile split with real sizes | Implemented | `crates/Cargo.toml` profiles | R3 size budget CI |

### Phase 11 — UX & supply chain
| Feature | Status | Implementation | Test |
|---------|--------|----------------|------|
| Dioxus UI accessibility (keyboard-only, ARIA, contrast, reduced-motion) | Implemented | `clients/dioxus/unpeel-ui/`, `unpeel-web/` | Playwright 17/17, axe 0 violations |
| `unpeel backup` / `unpeel restore` (SHA-256 manifest, chain re-verify) | Implemented | `crates/unpeel-core/src/backup.rs` | 6/6 Rust tests, 13/13 CLI e2e |
| Config validation (`unpeel config check`, typed schema) | Implemented | `crates/unpeel-cli/src/config_cli.rs` | `config_check_rejects_bad_value` |
| Supply chain (cargo-deny + cargo-audit + CI) | Implemented | `deny.toml`, `.github/workflows/supply-chain.yml` | Supply chain CI green |
| First-run UX (`unpeel init`, 0700 home, pairing QR, doctor) | Implemented | `crates/unpeel-cli/src/init_cli.rs` | Fresh-HOME e2e 16/16 |

### Phase 12/13 — Concurrency & scale
| Feature | Status | Implementation | Test |
|---------|--------|----------------|------|
| `persist_grant` serialization fix (group commit, c1/4/8/16 scaling) | Implemented | `crates/unpeel-core/src/grant_writer.rs` | Concurrency table (p99 < 500ms @ c8) |
| Sealed-envelope fuzz target (30 min) | Implemented | Fuzz targets | `fuzz_sealed_envelope` cov/ft reported |
| Writer-supervision panic test | Implemented | `grant_writer.rs` tests | `panic_after_write_before_fsync` |
| c1/4/8 sealed-pairing E2E (n≥2000) | Implemented | `scripts/e2e-scenario.sh` | 2,000/2,000 @ c1/c4/c8, 0 errors |

### Phase 14 — Polish & packaging
| Feature | Status | Implementation | Test |
|---------|--------|----------------|------|
| Answer idempotency (approval id + nonce, `AlreadyResolved`) | Implemented | `crates/unpeel-core/src/approval.rs` | `second_answer_gets_already_resolved` |
| i18n (238 allowlisted UI strings) | Implemented | `clients/dioxus/unpeel-ui/src/i18n.rs` | `all_strings_have_translations` |
| Exactly-one audit entry after retries | Implemented | `action_reviews.rs` | `retry_produces_single_audit_entry` |
| FIFO+TTL resolved store (cap 1,000, TTL 600s) | Implemented | `crates/unpeel-core/src/resolved.rs` | `fifo_eviction_order`, `ttl_expires_on_lookup` |
| 409 ResolvedUnknown → "Resolved" UI | Implemented | `clients/dioxus/unpeel-mobile/` | Mobile mapping test |
| Dist packaging (fat-LTO, tar.gz/deb, SHA-256) | Implemented | `scripts/dist.sh` | Tar/deb verified, checksums pass |
| Reproducible builds (byte-identical) | Implemented | `SOURCE_DATE_EPOCH` + path remap | SHA-256 match across 2 builds |
| Self-update (`--check`, manifest, rollback) | Implemented | `crates/unpeel-cli/src/self_update_cli.rs` | 7/7 unit tests, E2E checks |
| Simulated day (40 actions, 5 UX frictions fixed) | Implemented | Manual + scripted | Friction list in phase report |

---

## 3. Code-Copy Question: Was Any OpenMuse Code Copied?

**No. Zero OpenMuse code was copied.**

**Evidence:**
1. **OpenMuse is a product, not open-source code.** There is no public OpenMuse repository to copy from. The inspiration was architectural/product-level (memory, scheduler, connectors, approval flows as product concepts).
2. **Codebase search:** `grep -ri "openmuse"` across the entire repository returns exactly 2 hits, both in comments/strings:
   - `clients/dioxus/unpeel-ui/src/composer.rs`: `/// OpenMuse-style composer.` (a doc comment describing design inspiration)
   - `clients/dioxus/unpeel-web/src/main.rs`: `"OpenMuse-style composer — deterministic simulation, no Host."` (a UI description string)
   
   Both are descriptive labels, not copied code. The implementations are original Rust/Dioxus.
3. **All code is original or MIT-licensed dependencies.** The Rust dependencies are listed in `THIRD_PARTY_NOTICES.txt` with their licenses (MIT, Apache-2.0, etc.). No proprietary code is included.
4. **The unpeel foundation itself is MIT-licensed** (`LICENSE`: Copyright (c) 2026 UX Themes AS), which permits the supercli fork with attribution (which we're doing by keeping the original copyright line).

**Conclusion:** Ideas only, no code. The supercli project is clean on this question.
