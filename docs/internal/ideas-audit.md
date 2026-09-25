# Ideas Audit: OpenMuse Inspiration → Implementation

**Date:** 2026-09-26
**Method:** Every path verified with `ls`/`test -f`. Every test name verified with `grep -rw` in `crates/`. Unverifiable items marked Missing.

---

## 1. OpenMuse-Inspired Ideas

| Idea | Status | Real file path (verified) | Real test name (verified) |
|------|--------|---------------------------|---------------------------|
| **Persistent memory** — harness remembers user preferences, coding style, past decisions | Missing | No `memory.rs` exists in the repo | Missing |
| **Scheduled proactivity** — scheduled triggers that run tasks autonomously | Implemented | `crates/supercli-cli/src/schedule_cli.rs`, `crates/supercli-core/src/scheduled.rs`, `crates/supercli-core/src/schedule_leases.rs` | `schedule_store_round_trips_and_refuses_bad_specs`, `scheduler_fires_due_triggers_and_notifies_on_persistent_failure`, `scheduler_lease_store_gives_single_flight_across_workers` |
| **Connectors** — developer-written plugin system | Implemented | `crates/supercli-connector/` (contains `registry.rs`, `session.rs`, `manifest.rs`, `scaffold.rs`) | `scaffold_defaults_are_valid`, `http_handshake_and_call`, `call_rejects_unknown_tool`, `scaffolded_stub_answers_mcp` |
| **Approval flows** — Allow/Ask/Deny for agent actions | Implemented | `crates/supercli-serve/src/approvals.rs` | `answer_idempotent_retry_returns_original_decision`, `resolved_store_is_bounded`, `idempotent_answer_writes_exactly_one_audit_log_entry`, `eight_concurrent_approvals_all_visible_and_answerable` |
| **Browser worker** — browser with saved logins | Partial | `crates/supercli-cli/src/browser_cli.rs`, `crates/supercli-core/src/browser_engine.rs`, `crates/supercli-core/src/browser_mcp.rs` | Missing (no test names verified for browser engine install) |
| **Browser live takeover** | Missing | No "takeover" functionality found in `browser_engine.rs` | Missing |
| **Stable threads** — session lifecycle, archive/restore | Implemented | `crates/supercli-core/src/session_host.rs`, `crates/supercli-core/src/session_io.rs`, `crates/supercli-serve/src/sessions.rs` | `archived_rows_remain_in_the_five_row_archive_preview`, `date_sorted_group_keeps_live_then_recent_stopped_sections` |
| **Ideas surface** — a place to capture and develop ideas | Missing | No ideas-related module found | Missing |
| **Chat UI layer** — chat UI over the CLI | Partial | `clients/dioxus/unpeel-web/` (directory exists) | Missing (Playwright tests not verified in this audit) |
| **Unified registrar** — registry for skills/runtimes/MCP/hooks | Partial | `crates/supercli-connector/src/registry.rs` | `publish_resolve_fetch_roundtrip` |
| **Proactive learning** — learns coding style over time | Missing | No proactive learning module found | Missing |

---

## 2. Phase Features (verified only)

### Safety model
| Feature | Status | Real file path | Real test name |
|---------|--------|----------------|----------------|
| Stored action-review records (write-ahead) | Implemented | `crates/supercli-core/src/action_reviews.rs` | `review_write_failure_is_fail_closed`, `outcome_record_attaches_to_review_and_extends_chain` |
| Tamper-evident hash chain | Implemented | `crates/supercli-core/src/action_reviews.rs` | `chain_verifies_and_detects_tampering`, `outcome_tamper_is_detected` |
| Explicit actor on every path | Implemented | `crates/supercli-core/src/action_reviews.rs` | `actor_display_is_never_empty` |
| `NeverRan` outcome (stale-lease fence) | Implemented | `crates/supercli-core/src/action_reviews.rs` | `never_ran_outcome_records_and_verifies` |
| Concurrent writer safety | Implemented | `crates/supercli-core/src/action_reviews.rs` | `concurrent_writers_keep_chain_intact` |

### Concurrency
| Feature | Status | Real file path | Real test name |
|---------|--------|----------------|----------------|
| Group-commit grant writer | Implemented | `crates/supercli-core/src/grant_writer.rs` | `concurrent_grouped_writes_do_not_fork_audit_chain`, `mid_batch_panic_errors_all_waiters_no_hang_no_false_ack` |
| Direct grant path removed | Implemented | `crates/supercli-core/src/grant_writer.rs` | `direct_path_is_removed` |

### Backup/Restore
| Feature | Status | Real file path | Real test name |
|---------|--------|----------------|----------------|
| Backup/restore with manifest | Implemented | `crates/supercli-core/src/backup.rs` | `round_trip_preserves_state_and_chains`, `tampered_archive_is_refused`, `backup_refuses_destination_inside_home`, `restore_refuses_while_host_lock_held_and_over_existing_state` |

### i18n
| Feature | Status | Real file path | Real test name |
|---------|--------|----------------|----------------|
| UI string allowlist (no hardcoded strings) | Implemented | `clients/dioxus/unpeel-ui/src/i18n.rs` | `no_hardcoded_ui_strings`, `allowlist_entries_are_still_valid` |

---

## 3. Code-Copy Question

**Question:** Was any OpenMuse code copied?

**Verified evidence:**
- `grep -ri "openmuse"` returns exactly 2 hits in the repo (excluding target/, .git/, and this audit file):
  - `clients/dioxus/unpeel-ui/src/composer.rs`: `/// OpenMuse-style composer.` (doc comment)
  - `clients/dioxus/unpeel-web/src/main.rs`: `"OpenMuse-style composer — deterministic simulation, no Host."` (UI string)
- Both are descriptive labels referencing design inspiration, not copied code.
- No OpenMuse repository exists publicly to copy from (OpenMuse is a product, not open-source code).

**Conclusion:** Ideas only, no code copied. The 2 grep hits are comments/strings, not implementations.
