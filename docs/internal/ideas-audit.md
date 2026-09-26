# Ideas Audit: OpenMuse Inspiration → Implementation

**Date:** 2026-09-26
**Method:** Every path verified with `ls`/`test -f`. Every test name verified with `grep -rw` in `crates/`. Unverifiable items marked Missing.

---

## 1. OpenMuse-Inspired Ideas

| Idea | Status | Real file path (verified) | Real test name (verified) |
|------|--------|---------------------------|---------------------------|
| **Persistent memory** — harness remembers user preferences, coding style, past decisions | Partial | `crates/supercli-core/src/memory.rs` (native-host) | `memory_set_get_forget_roundtrip`, `memory_promote_session_to_long_term`, `memory_persists_across_simulated_sessions`, `memory_keys_for_session_are_scoped`, `memory_load_missing_or_corrupt_returns_empty` |
| **Scheduled proactivity** — scheduled triggers that run tasks autonomously | Implemented | `crates/supercli-cli/src/schedule_cli.rs`, `crates/supercli-core/src/scheduled.rs`, `crates/supercli-core/src/schedule_leases.rs` | `schedule_store_round_trips_and_refuses_bad_specs`, `scheduler_fires_due_triggers_and_notifies_on_persistent_failure`, `scheduler_lease_store_gives_single_flight_across_workers` |
| **Connectors** — developer-written plugin system | Implemented | `crates/supercli-connector/` (contains `registry.rs`, `session.rs`, `manifest.rs`, `scaffold.rs`) | `scaffold_defaults_are_valid`, `http_handshake_and_call`, `call_rejects_unknown_tool`, `scaffolded_stub_answers_mcp` |
| **Approval flows** — Allow/Ask/Deny for agent actions | Implemented | `crates/supercli-serve/src/approvals.rs` | `answer_idempotent_retry_returns_original_decision`, `resolved_store_is_bounded`, `idempotent_answer_writes_exactly_one_audit_log_entry`, `eight_concurrent_approvals_all_visible_and_answerable` |
| **Browser worker** — browser with saved logins | Partial | `crates/supercli-cli/src/browser_cli.rs`, `crates/supercli-core/src/browser_engine.rs`, `crates/supercli-core/src/browser_mcp.rs` | Missing (no test names verified for browser engine install) |
| **Browser live takeover** | Partial | `crates/supercli-core/src/browser_takeover.rs` (native-host), CLI `supercli browser takeover` in `crates/supercli-cli/src/browser_cli.rs` | `takeover_lists_targets_from_fake_cdp`, `takeover_capture_screenshot_returns_png`, `takeover_stream_captures_n_frames`, `takeover_handshake_rejected_errors`, `takeover_bad_endpoint_errors`, `takeover_tool_lists_and_captures`, `ws_url_parsing_rejects_non_ws`, `base64_encode_roundtrip_spot_check` |
| **Stable threads** — session lifecycle, archive/restore | Implemented | `crates/supercli-core/src/session_host.rs`, `crates/supercli-core/src/session_io.rs`, `crates/supercli-serve/src/sessions.rs` | `archived_rows_remain_in_the_five_row_archive_preview`, `date_sorted_group_keeps_live_then_recent_stopped_sections` |
| **Ideas surface** — a place to capture and develop ideas | Implemented | `crates/supercli-cli/src/ideas_cli.rs` (wired in `main.rs`, `cli.rs`) | `ideas_add_list_done_roundtrip`, `ideas_rejects_empty_text_and_unknown_id`, `ideas_persist_as_jsonl` |
| **Chat UI layer** — chat UI over the CLI | Partial | `clients/dioxus/unpeel-web/` (directory exists) | Missing (Playwright tests not verified in this audit) |
| **Unified registrar** — registry for skills/runtimes/MCP/hooks | Partial | `crates/supercli-connector/src/registry.rs` | `publish_resolve_fetch_roundtrip` |
| **Proactive learning** — learns coding style over time | Partial | `crates/supercli-core/src/profile.rs` (native-host) | `profile_updates_from_approval_history`, `inferred_pref_confidence_grows_and_resets_on_switch`, `profile_save_load_roundtrip`, `load_missing_profile_returns_default` |

**Partial-status notes (2026-09-26):**
- **Persistent memory:** the durable store is implemented and tested (`memory.json` under `<SUPERCLI_HOME>`, session/long-term scopes, explicit promotion), but nothing in the harness calls it yet — no session integration, no CLI surface. It becomes Implemented when the agent loop reads/writes it.
- **Proactive learning:** the profile store is implemented and tested (`profile.json`), but it is not wired into the real approval decision path (`crates/supercli-serve/src/approvals.rs`) — updates are only exercised in unit tests. It becomes Implemented when real approve/deny decisions update the profile.
- **Browser live takeover:** CDP attach + screenshot stream + `supercli browser takeover` CLI are implemented and tested against a fake CDP server; there is no web/Dioxus "Take over" button and no live 5 fps surface in the UI. It becomes Implemented when the web UI can start a takeover and display the stream.
- **Chat UI layer** stays Partial: the gpuidart port (`docs/chat-ui-gpuidart.md`) is a design doc only; no gpuidart UI code exists.

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

**Correction (2026-09-26):** The prior version of this section claimed "No OpenMuse
repository exists publicly to copy from (OpenMuse is a product, not open-source
code)." That was FALSE. Two public OpenMuse repositories exist:

1. **CopilotKit/openmuse** — https://github.com/CopilotKit/openmuse
   - License: **MIT** ("Copyright (c) 2026 OpenMuse contributors", verified from the
     repo's LICENSE file)
   - Created: 2026-09-15. Stars: 2199, forks: 244 (as of 2026-09-26).
   - Languages: **TypeScript/JavaScript** (pnpm monorepo: `apps/{computer,mobile,server,worker}`,
     `packages/{backends,domain,integrations}`)
   - Stack: React Native (iOS/Android/web mobile app) + AG-UI (Agent User Interaction Protocol)

2. **0sparsh2/OpenMuse** — https://github.com/0sparsh2/OpenMuse
   - Created: 2026-09-22. **No LICENSE file in the repo** (verified via shallow clone;
     root listing contains no LICENSE/COPYING file; README describes it as "open-source"
     but no license text is present).
   - Languages: **Python** (171 `.py` files, 0 TypeScript; verified via `find`)
   - Stack: Python stdlib + a few packages, NVIDIA NIM models, static single-page app UI
     (no build step). Modules: `agent/` (turn_engine, context_builder), `memory/` (layered,
     embeddings), `policy/` (risk classes R0–R5), `browser/`, `scheduler/`, `subagents/`, etc.
   - Test suites: `demo_*.py` scripts (e.g. `demo_memory.py` 35 checks, `demo_browser.py` 31).

**Copy-check methodology (2026-09-26, both repos shallow-cloned to /tmp):**
- Searched supercli source (`.rs`, `.ts`, `.py`, `.dart`, excluding `target/`, `.git/`,
  `clients/legacy/`, `clients/gpuidart/`) for: `openmuse` (case-insensitive),
  `copilotkit`, `ag-ui`/`agui`, and 0sparsh2-distinctive identifiers
  (`nemotron`, `composio`, `fernet`, `advance_run`, `LayeredMemory`, `turn_engine`,
  `context_builder`).
- Results:
  - `openmuse`: **0 hits** in active source. (The 2 hits cited in the prior audit —
    `clients/dioxus/unpeel-ui/src/composer.rs` doc comment "OpenMuse-style composer"
    and `clients/dioxus/unpeel-web/src/main.rs` UI string — moved verbatim to
    `clients/legacy/` during the client freeze; both are descriptive labels, not code.)
  - `copilotkit`, `ag-ui`: **0 hits**.
  - 0sparsh2 identifiers: **0 hits**. (The substring `riva` matched only inside the
    English word `private` in unrelated files — false positive, not the NVIDIA Riva
    speech service.)
- Structural comparison: supercli's Python files are CLI e2e test harnesses
  (`crates/supercli-cli/tests/cases/*.py`, importing a local `harness` module to drive
  the Rust binaries); 0sparsh2's Python is a full agent runtime. supercli's TypeScript
  is `crates/apps/app-kit/web/` (a UI kit for the terminal multiplexer); CopilotKit's
  TypeScript is a React Native personal-agent app. Different domains, no shared code.
- supercli's core is **Rust**; neither OpenMuse repo contains Rust. Cross-language
  direct code copying is not possible; the identifier searches above confirm no
  adapted snippets, assets, or distinctive strings either.

**Conclusion:** Ideas only, no code copied. The prior audit's "not open-source" claim
is corrected above. Because nothing was copied or adapted, no THIRD_PARTY_NOTICES.txt
entry is required for either repo. (Note: 0sparsh2/OpenMuse ships no LICENSE file,
so its reuse terms are in any case unclear; CopilotKit/openmuse is MIT.)
