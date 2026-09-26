# Parity Audit: [HOST] + [PROTO] + [SHARED]

Audited against `track-b-parity-docs@de51bb2` (supercli = renamed unpeel Rust codebase).
Method: `git grep` on `crates/supercli-*`, file existence, `#[test]` counts. No builds.

## Summary

| Tag | Total | done | partial | missing | improved |
|-----|-------|------|---------|---------|----------|
| [HOST] | 78 | 78 | 0 | 0 | 0 |
| [PROTO] | 4 | 4 | 0 | 0 | 0 |
| [SHARED] | 2 | 2 | 0 | 0 | 0 |
| **Total** | **84** | **84** | **0** | **0** | **0** |

All 84 items exist in the renamed Rust codebase. One caveat: item 44's
"per-session fallback host" was verified via journal-fallback mechanisms, not a
struct literally named FallbackHost — the survival behavior is covered.

## [HOST] (41–118)

| # | Tag | Status | supercli Path | Test | Notes |
|---|-----|--------|---------------|------|-------|
| 41 | [HOST] | done | crates/supercli-serve/src/service.rs | service.rs: 1 #[test] | Machine lease + serve.json status (line 322) |
| 42 | [HOST] | done | crates/supercli-core/src/pty_core.rs | pty_core.rs: 3 #[test] | Shared PTY core per workspace |
| 43 | [HOST] | done | crates/supercli-core/src/fd_pass.rs; crates/supercli-serve/src/pty_core_supervisor.rs | fd_pass.rs: 4, pty_core_supervisor.rs: 10 | SCM_RIGHTS takeover |
| 44 | [HOST] | done | crates/supercli-core/src/session_host.rs | session_host.rs: 82 #[test] | Sessions survive restart (on-disk); journal fallback covers core-unavailable case |
| 45 | [HOST] | done | crates/supercli-core/src/session_host.rs | session_host.rs: 82 | manifest.json / output.bin / session.sock session dirs |
| 46 | [HOST] | done | crates/supercli-core/src/session_host.rs:56 | session_host.rs | SESSION_OUTPUT_JOURNAL_RETAIN_BYTES = 64 MiB |
| 47 | [HOST] | done | crates/supercli-core/src/ghostty_vt.rs | ghostty_vt.rs: 1 | ghostty-vt grid snapshot for attach |
| 48 | [HOST] | done | crates/supercli-core/src/session_host.rs:3624 | session_host.rs | PidIdentity enum; start-time guard |
| 49 | [HOST] | done | crates/supercli-core/src/session_host.rs:5783-5806 | session_host.rs | owner_principal_id, source_preset_id provenance |
| 50 | [HOST] | done | crates/supercli-core/src/session_host.rs; crates/supercli-core/src/session_io.rs | session_host.rs | Auto-titling (OSC / first prompt / off) |
| 51 | [HOST] | done | crates/supercli-core/src/transcripts/mod.rs:775 | transcripts | `supercli-host __auto_title__` |
| 52 | [HOST] | done | crates/supercli-core/src/session_ops.rs:303,332 | session_ops.rs: 23 | archive_session / restore_session |
| 53 | [HOST] | done | crates/supercli-serve/src/auto_archive.rs; crates/supercli-serve/src/driver.rs | auto_archive.rs: 9 | Idle sweep auto-stop-and-archive |
| 54 | [HOST] | done | crates/supercli-core/src/resume.rs | resume.rs: 6 | Resume via hook-captured conversation id |
| 55 | [HOST] | done | crates/supercli-core/src/resume.rs | resume.rs | Resume agent in place (same session id) |
| 56 | [HOST] | done | crates/supercli-core/src/session_ops.rs | session_ops.rs | Session reload (replace host, keep id) |
| 57 | [HOST] | done | crates/supercli-core/src/session_host.rs | session_host.rs | Restart recommendation on old host protocol |
| 58 | [HOST] | done | crates/supercli-core/src/session_host.rs; crates/supercli-core/src/mcp_host.rs | session_host.rs | Bracketed paste, settle, double-Enter safe delivery |
| 59 | [HOST] | done | crates/supercli-serve/src/hook_listener.rs | hook_listener.rs: 6 | Hook listener, port registry |
| 60 | [HOST] | done | crates/supercli-core/src/screen_activity.rs; crates/supercli-serve/src/activity.rs | screen_activity.rs: 4 | Hook-latched busy/idle/attention engine |
| 61 | [HOST] | done | crates/supercli-core/src/screen_activity.rs:48,60 | screen_activity.rs | Screen-tier fallback (rules_for_runtime, classify) |
| 62 | [HOST] | done | crates/supercli-core/src/runtime_observer.rs | runtime_observer.rs: 11 | Hand-started agent observation |
| 63 | [HOST] | done | crates/supercli-core/src/menu_prompt.rs | menu_prompt.rs: 13 | Select-menu detection sets attention |
| 64 | [HOST] | done | crates/supercli-core/src/hook_cancellation.rs | hook_cancellation.rs: 6 | Escape-cancellation fencing |
| 65 | [HOST] | done | crates/supercli-core/src/durable_runs.rs | subagent_kill tests | Subagent/background tracking (M2 durable runs) |
| 66 | [HOST] | done | crates/supercli-serve/src/notifications.rs | notifications.rs: 2 | Lifecycle notification policy (needs_input, opt-in finished) |
| 67 | [HOST] | done | crates/supercli-core/src/app_state.rs | app_state.rs | Unread / mark-read / notify-when-done |
| 68 | [HOST] | done | crates/supercli-serve/src/approvals.rs:517 | approvals.rs: 11 | Shared approval hub (HubGate wired) |
| 69 | [HOST] | done | crates/supercli-serve/src/presence.rs | presence.rs: 3 | Viewer presence leases and files |
| 70 | [HOST] | done | crates/supercli-core/src/controller_host.rs; crates/supercli-core/src/remote_session_backend.rs | remote_server.rs: 32 | Phone-fit resize / desktop-fit restore |
| 71 | [HOST] | done | crates/supercli-core/src/activity_log.rs | activity_log.rs: 7 | Persisted activity-log.jsonl |
| 72 | [HOST] | done | crates/supercli-core/src/local_urls.rs | local_urls.rs: 18 | Local URL detect / verify / find / stop |
| 73 | [HOST] | done | crates/supercli-core/src/first_run.rs | first_run.rs: 2 | First-run preset seeding from PATH |
| 74 | [HOST] | done | crates/supercli-core/src/state.rs; crates/supercli-core/src/app_state.rs | state.rs | Projects, groups, worktree children, pins, manual order |
| 75 | [HOST] | done | crates/supercli-core/src/state_bus.rs | state_bus.rs: 3 | Cross-frontend state bus notifications |
| 76 | [HOST] | done | crates/supercli-core/src/worktrees.rs | worktrees.rs: 3 | Git worktree create/list |
| 77 | [HOST] | done | crates/supercli-core/src/mcp_host.rs | mcp_host.rs: 49 | Unified MCP server (stdio) |
| 78 | [HOST] | done | crates/supercli-core/src/mcp_host.rs | mcp_host.rs | MCP sessions domain (all 9 actions) |
| 79 | [HOST] | done | crates/supercli-core/src/mcp_host.rs:702 | mcp_host.rs | MCP agents domain (AGENTS_TOOL) |
| 80 | [HOST] | done | crates/supercli-core/src/mcp_host.rs:703 | mcp_host.rs | MCP workspace domain (WORKSPACE_TOOL) |
| 81 | [HOST] | done | crates/supercli-core/src/mcp_host.rs:704 | mcp_host.rs | MCP artifacts domain (ARTIFACTS_TOOL) |
| 82 | [HOST] | done | crates/supercli-core/src/browser_mcp.rs | browser_mcp.rs | MCP browser domain (13 actions) |
| 83 | [HOST] | done | crates/supercli-core/src/apps_mcp.rs | apps_mcp.rs | MCP apps domain |
| 84 | [HOST] | done | crates/supercli-core/src/skills_mcp.rs | skills_mcp.rs | MCP skills domain |
| 85 | [HOST] | done | crates/supercli-core/src/mcp_host.rs; crates/supercli-core/src/grant_store.rs | grant_store.rs: 3, mcp_host.rs | Open reads + approval-controlled cross-session writes |
| 86 | [HOST] | done | crates/supercli-core/src/mcp_host.rs:685 | mcp_host.rs | User-only session creation (explicit comment) |
| 87 | [HOST] | done | crates/supercli-core/src/mcp_host.rs; crates/supercli-core/src/session_host.rs:3624 | mcp_host.rs | Caller identity (env + PidIdentity ancestry) |
| 88 | [HOST] | done | crates/supercli-core/src/mcp_host.rs | mcp_host.rs | Lazy per-action help |
| 89 | [HOST] | done | crates/supercli-core/src/mcp_host.rs; crates/supercli-core/src/mcp_gate.rs | mcp_host.rs | Dual-era MCP (initialize + server/discover) |
| 90 | [HOST] | done | crates/supercli-core/src/mcp_cancel.rs | mcp_cancel.rs: 4 | In-flight cancellation with ordered execution |
| 91 | [HOST] | done | crates/supercli-core/src/mcp_auth.rs | mcp_auth.rs | /mcp/* 0600 auth token (ensure_auth_token, verify_auth) |
| 92 | [HOST] | done | crates/supercli-serve/src/app_context.rs | app_context.rs: 3 | app-context.json surfaced to neighbours |
| 93 | [HOST] | done | crates/supercli-core/src/browser_engine.rs | browser_engine.rs: 10 | Pinned agent-browser auto-install (sha256, flock) |
| 94 | [HOST] | done | crates/supercli-core/src/browser_mcp.rs | browser_mcp.rs | Browser shared-window / per-session modes |
| 95 | [HOST] | done | crates/supercli-core/src/browser_mcp.rs; crates/supercli-core/src/config.rs | browser_mcp.rs | Browser access On/Ask/Off (BrowserAccess) |
| 96 | [HOST] | done | crates/supercli-core/src/browser_mcp.rs:965,1832 | browser_mcp.rs | Browser options (headed, cursor overlay) |
| 97 | [HOST] | done | crates/supercli-core/src/browser_mcp.rs; crates/supercli-core/src/browser_takeover.rs | browser_mcp.rs | Remote CDP endpoint mode (remote-cdp.json) |
| 98 | [HOST] | done | crates/supercli-core/src/browser_mcp.rs:2098-2114 | browser_mcp.rs | Browser screenshots/downloads as artifacts |
| 99 | [HOST] | done | crates/supercli-core/src/session_artifacts.rs | session_artifacts.rs: 21 | Artifact store (list/read/resumable upload/delete/thumbnails) |
| 100 | [HOST] | done | crates/supercli-core/src/host_resources.rs | host_resources.rs | Host filesystem ops (scoped to projects) |
| 101 | [HOST] | done | crates/supercli-serve/src/pairing.rs | pairing.rs: 7 | One-time-code pairing, per-device Bearer <redacted> + E2E key |
| 102 | [HOST] | done | crates/supercli-serve/src/pairing.rs:221,282,676 | pairing.rs | Pairing control (begin/status/cancel/revoke/relay-allowed) |
| 103 | [HOST] | done | crates/supercli-serve/src/pairing.rs:248; crates/supercli-core/src/controller_protocol.rs | pairing.rs | Controller-assisted pairing (pairing.invitation) |
| 104 | [HOST] | done | crates/supercli-core/src/remote_server.rs:184-192 | remote_server.rs: 32 | /mobile over TLS with pinned Host certificate |
| 105 | [HOST] | done | crates/supercli-serve/src/remote_streamer.rs:222,278 | remote_streamer.rs | Supervised WSS streamer (crash-loop ceiling) |
| 106 | [HOST] | done | crates/supercli-core/src/remote_server.rs; crates/supercli-core/src/remote_stdio.rs | remote_server.rs | host.sock framed Controller contract |
| 107 | [HOST] | done | crates/supercli-core/src/remote_stdio.rs | remote_stdio.rs: 6 | SSH stdio Host gateway (__remote_stdio__) |
| 108 | [HOST] | done | crates/supercli-core/src/relay_uplink.rs; crates/supercli-core/src/relay_crypto.rs | relay_uplink.rs: 8, relay_crypto.rs: 6 | Link relay uplink, forward-secret E2E |
| 109 | [HOST] | done | crates/supercli-serve/src/relay.rs:361,611-616 | relay.rs | Relay credential recovery (rotation without eviction) |
| 110 | [HOST] | done | crates/supercli-core/src/direct_path.rs; crates/supercli-core/src/direct_path_punch.rs | direct_path.rs: 4 | Direct-path negotiation + UDP NAT punch |
| 111 | [HOST] | done | crates/supercli-core/src/license.rs | license.rs: 6 | Link license (Ed25519 verify) |
| 112 | [HOST] | done | crates/supercli-serve/src/notifications.rs:6 | notifications.rs | APNs push registration/delivery via relay (adapter seam) |
| 113 | [HOST] | done | crates/supercli-serve/src/platform_adapter.rs | platform_adapter.rs: 5 | Platform-adapter seam for native effects |
| 114 | [HOST] | done | crates/supercli-core/src/app_installer.rs; crates/supercli-core/src/app_open.rs | app_installer.rs: 4 | Host-owned App install/open/opener policy |
| 115 | [HOST] | done | crates/supercli-core/src/plugins.rs; crates/supercli-core/src/plugin_updates.rs | plugins.rs: 10 | Plugin activation, ordering, lazy update checks |
| 116 | [HOST] | done | crates/supercli-core/src/remote_attach.rs | remote_attach.rs: 1 | __remote_attach__ network attach |
| 117 | [HOST] | done | crates/supercli-core/src/desktop_session.rs; crates/supercli-serve/src/service_install.rs | desktop_session.rs: 3 | Graphical desktop-session unit + diagnostics |
| 118 | [HOST] | done | crates/supercli-serve/src/tracelog.rs | tracelog.rs | Timestamped trace log (trace.log) |

## [SHARED] (245–246)

| # | Tag | Status | supercli Path | Test | Notes |
|---|-----|--------|---------------|------|-------|
| 245 | [SHARED] | done | crates/supercli-client/src/hosts.rs; crates/supercli-client/src/credentials.rs; crates/supercli-client/src/pairing.rs | supercli-client | Host protocol DTOs, pairing client, paired-host records in Rust (not Swift) |
| 246 | [SHARED] | done | crates/supercli-core/src/relay_crypto.rs; crates/supercli-client/src/crypto.rs; crates/supercli-client/src/relay_conn.rs; protocol/relay-kat-vectors-v2.json | relay_crypto.rs: 6 | Relay forward-secret E2E + WebSocket client, KAT vectors |

## [PROTO] (247–250)

| # | Tag | Status | supercli Path | Test | Notes |
|---|-----|--------|---------------|------|-------|
| 247 | [PROTO] | done | protocol/host-capabilities-v1.json | crates/supercli-cli/tests/cases/host_launch_conformance.py | Versioned capability ledger (52 op ids) |
| 248 | [PROTO] | done | protocol/host-conformance-v1.json; protocol/host-bootstrap-compatibility-v1.json | crates/supercli-cli/tests/cases/host_launch_conformance.py | Conformance + bootstrap-compatibility fixtures |
| 249 | [PROTO] | done | protocol/pane-layout-operations-v1.json; protocol/direct-path-v1.json; protocol/relay-kat-vectors-v2.json; protocol/browser-engine-v1.json; protocol/app-registry.json | crates/supercli-cli/tests/cases/relay_conformance.py | Pane-layout ops, direct-path v1, relay KAT, browser-engine pin, App registry |
| 250 | [PROTO] | done | protocol/supercli-ui-v1.schema.json; protocol/supercli-ui-stream-v1.ndjson; protocol/supercli-ui-fixtures-v1.json | supercli-ui fixtures | UI protocol v1 (NDJSON App-to-Host) schema, stream, fixtures |
