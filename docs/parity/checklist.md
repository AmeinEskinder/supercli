# supercli Parity Checklist

Parity against upstream unpeel . Each item measured per FEATURE.

**Status values:**  (feature works, tested),  (works but incomplete),  (not implemented),  (exceeds unpeel).

**Done criteria for [DESKTOP]:** rendered screenshot + test exercising the behaviour.

## Summary

| Tag | Total | Done | Partial | Missing | Improved |
|-----|-------|------|---------|---------|----------|
| [APPS] | 17 | 3 | 13 | 1 | 0 |
| [CLI] | 40 | 40 (inherited) | 0 | 0 | 0 |
| [DESKTOP] | 70 | 1 | 27 | 42 | 0 |
| [DIST] | 3 | 2 | 1 | 0 | 0 |
| [HOST] | 78 | 78 (inherited) | 0 | 0 | 0 |
| [IOS] | 25 | 0 | 0 | 25 | 0 |
| [PROTO] | 4 | 4 (inherited) | 0 | 0 | 0 |
| [RUNTIMES] | 11 | 11 (inherited) | 0 | 0 | 0 |
| [SHARED] | 2 | 2 (inherited) | 0 | 0 | 0 |
| [WEB] | 3 | 1 | 0 | 2 | 0 |

## Items

| # | Tag | Description | Status | supercli Path | Test |
|---|-----|-------------|--------|---------------|------|
| 1 | [CLI] | `unpeel serve`: run the UI-free Host service for all registered workspaces | done-inherited | `crates/supercli-cli/src/cli.rs:1198` (`"serve" =>`) | `crates/supercli-cli/tests/serve_command.rs` |
| 2 | [CLI] | `unpeel serve install\|uninstall\|status` for the per-user launchd/systemd unit, with a `--graphical` Linux variant | done-inherited | `crates/supercli-cli/src/cli.rs:1199` (`install\ | uninstall\ |
| 3 | [CLI] | `unpeel --workspace NAME <cmd>`: target an isolated workspace for any verb | done-inherited | `crates/supercli-cli/src/cli.rs:47` (`--workspace NAME`); `crates/supercli-cli/src/workspaces.rs` | `crates/supercli-cli/tests/` (workspace isolation) |
| 4 | [CLI] | `unpeel pair` shows a one-time pairing code/QR (auto-starts serve; `--advertise-host/--advertise-port`) | done-inherited | `crates/supercli-cli/src/cli.rs:1219` (`"pair" =>`) | `crates/supercli-cli/tests/pairclient/` |
| 5 | [CLI] | `unpeel pair list\|remove <device>\|relay <device> on\|off` | done-inherited | `crates/supercli-cli/src/cli.rs:1224-1225` (`list\ | remove\ |
| 6 | [CLI] | `unpeel ls [--json]`: list sessions with status/project/command | done-inherited | `crates/supercli-cli/src/cli.rs:960` (`"ls"\ | "list"`) |
| 7 | [CLI] | `unpeel new` with `--command\|--preset`, `--cwd`, `--project`, `--cols/--rows`, `--json` | done-inherited | `crates/supercli-cli/src/cli.rs:964` (`"new" =>`) | `crates/supercli-cli/tests/` |
| 8 | [CLI] | `unpeel add [PATH] [--name] [--here]`: add a folder as a project | done-inherited | `crates/supercli-cli/src/cli.rs:965` (`"add" =>`) | `crates/supercli-cli/tests/` |
| 9 | [CLI] | `unpeel send <id> <text> [--enter]`, going through the write policy when run inside a session | done-inherited | `crates/supercli-cli/src/cli.rs:970` (`"send" =>`) | `crates/supercli-cli/tests/cli.rs:1283` (arg parsing) |
| 10 | [CLI] | `unpeel keys <id> <seq>`: raw bytes outside a session, key names inside | done-inherited | `crates/supercli-cli/src/cli.rs:986` (`"keys" =>`) | `crates/supercli-cli/tests/` |
| 11 | [CLI] | `unpeel screen <id>`: parsed screen snapshot | done-inherited | `crates/supercli-cli/src/cli.rs:1010` (`"screen"\ | "--snapshot"`) |
| 12 | [CLI] | `unpeel logs\|tail <id> [--lines] [--follow]` | done-inherited | `crates/supercli-cli/src/cli.rs:1030` (`"logs"\ | "tail"`) |
| 13 | [CLI] | `unpeel wait <id> [--idle] [--text] [--timeout]` with exit code 1 on timeout | done-inherited | `crates/supercli-cli/src/cli.rs:1031` (`"wait" =>`) | `crates/supercli-cli/tests/cli.rs:1290` (arg parsing) |
| 14 | [CLI] | `unpeel resume\|restart <id>` | done-inherited | `crates/supercli-cli/src/cli.rs:1032` (`"restart"\ | "resume"`) |
| 15 | [CLI] | `unpeel stop\|archive\|restore\|rm <id>` | done-inherited | `crates/supercli-cli/src/cli.rs:1052-1069` (`stop\ | archive\ |
| 16 | [CLI] | `unpeel transcript <id> [--entries N] [--markdown]` | done-inherited | `crates/supercli-cli/src/cli.rs:1103` (`"transcript" =>`) | `crates/supercli-cli/tests/` |
| 17 | [CLI] | `unpeel open <path\|resource> [--with APP] [--kind] [--media-type]`: typed App dispatcher that creates or reuses a companion pane | done-inherited | `crates/supercli-cli/src/cli.rs:1104`; `crates/supercli-cli/src/open_cli.rs` | `crates/supercli-cli/tests/` |
| 18 | [CLI] | `unpeel settings list\|get\|set` (allowlisted keys, validated before write) | done-inherited | `crates/supercli-cli/src/cli.rs:1105`; `crates/supercli-cli/src/settings_cli.rs` | `crates/supercli-cli/tests/settings_command.rs` |
| 19 | [CLI] | `unpeel settings openers set <selector> <editor\|system\|app:id>` | done-inherited | `crates/supercli-cli/src/settings_cli.rs` (openers) | `crates/supercli-cli/tests/settings_command.rs` |
| 20 | [CLI] | `unpeel apps list\|install\|update [--check] [--yes]` | done-inherited | `crates/supercli-cli/src/cli.rs:1112`; `crates/supercli-cli/src/apps_cli.rs` | `crates/supercli-cli/tests/apps_command.rs` |
| 21 | [CLI] | `unpeel apps link\|unlink` dev slots | done-inherited | `crates/supercli-cli/src/apps_cli.rs` (`link\ | unlink`) |
| 22 | [CLI] | `unpeel apps describe\|search\|context` | done-inherited | `crates/supercli-cli/src/cli.rs:1113`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` |
| 23 | [CLI] | `unpeel integrations [list]` status per agent | done-inherited | `crates/supercli-cli/src/cli.rs:1163`; `crates/supercli-cli/src/integrations_cli.rs` | `crates/supercli-cli/tests/` |
| 24 | [CLI] | `unpeel integrations install <runtime> [--project DIR] \| --all` | done-inherited | `crates/supercli-cli/src/integrations_cli.rs` (`install`) | `crates/supercli-cli/tests/` |
| 25 | [CLI] | `unpeel mcp [<tool> [<action> k=v…]]`: every MCP action from the shell with the same identity and grants | done-inherited | `crates/supercli-cli/src/cli.rs:994`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` |
| 26 | [CLI] | `unpeel browser open\|snapshot\|click\|fill\|type\|press\|get\|screenshot\|scroll\|wait` | done-inherited | `crates/supercli-cli/src/cli.rs:1167`; `crates/supercli-cli/src/browser_cli.rs` | `crates/supercli-cli/tests/` |
| 27 | [CLI] | `unpeel browser install [--check] [--json]` with exit codes 0/1/3/4 | done-inherited | `crates/supercli-cli/src/browser_cli.rs` (`install`) | `crates/supercli-cli/tests/` |
| 28 | [CLI] | `unpeel artifacts publish <image>` | done-inherited | `crates/supercli-cli/src/cli.rs:1007`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` |
| 29 | [CLI] | `unpeel current` (self plus pane neighbours) | done-inherited | `crates/supercli-cli/src/cli.rs:1004`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` |
| 30 | [CLI] | `unpeel report <summary> [--status update\|done\|blocked] [--details]` | done-inherited | `crates/supercli-cli/src/cli.rs:1005`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` |
| 31 | [CLI] | `unpeel worktree create <name> [--branch] [--base] [--project]` | done-inherited | `crates/supercli-cli/src/cli.rs:1006`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` |
| 32 | [CLI] | `unpeel agents <action>` / `unpeel skills <action>` | done-inherited | `crates/supercli-cli/src/cli.rs:1008-1009`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` |
| 33 | [CLI] | `unpeel presets list\|add\|remove\|edit` | done-inherited | `crates/supercli-cli/src/cli.rs:1179`; `crates/supercli-cli/src/state_cli.rs` | `crates/supercli-cli/tests/` |
| 34 | [CLI] | `unpeel presets star\|unstar\|enable\|disable\|reorder` | done-inherited | `crates/supercli-cli/src/state_cli.rs` (`star\ | unstar\ |
| 35 | [CLI] | `unpeel link enroll <key>\|status\|deactivate` with exit codes 0/1/2 | done-inherited | `crates/supercli-cli/src/cli.rs:1175`; `crates/supercli-cli/src/link_cli.rs` | `crates/supercli-cli/tests/link_fixtures.py` |
| 36 | [CLI] | `unpeel workspaces list\|add\|remove` | done-inherited | `crates/supercli-cli/src/cli.rs:1180`; `crates/supercli-cli/src/workspaces.rs` | `crates/supercli-cli/tests/` |
| 37 | [CLI] | `unpeel projects list\|add\|remove` | done-inherited | `crates/supercli-cli/src/cli.rs:1197` (`"projects" =>`) | `crates/supercli-cli/tests/` (cli.rs:494,592) |
| 38 | [CLI] | `unpeel hosts prune [--json]` (identity-verified orphan reap) | done-inherited | `crates/supercli-cli/src/cli.rs:1184` (`"hosts" => "prune"`) | `crates/supercli-cli/tests/` |
| 39 | [CLI] | Every shared-state mutation is a flocked, unknown-key-preserving write followed by a state-bus flush | done-inherited | `crates/supercli-core/src/` (state_bus, flocked writes) | `crates/supercli-core/tests/` |
| 40 | [CLI] | `--json` output on data verbs plus meaningful exit codes; `help`, `--version`, and a bare-invocation hint | done-inherited | `crates/supercli-cli/src/cli.rs` (`--json`, `--help`, `--version`, bare hint) | `crates/supercli-cli/tests/` |
| 41 | [HOST] | Machine supervisor with one worker per workspace home; machine lease and `serve.json` status | done-inherited | crates/supercli-serve/src/service.rs | service.rs: 1 #[test] |
| 42 | [HOST] | Shared PTY core per workspace (single reactor, timer and journal threads) | done-inherited | crates/supercli-core/src/pty_core.rs | pty_core.rs: 3 #[test] |
| 43 | [HOST] | In-place PTY core upgrade via SCM_RIGHTS takeover (no terminal restart) | done-inherited | crates/supercli-core/src/fd_pass.rs; crates/supercli-serve/src/pty_core_supervisor.rs | fd_pass.rs: 4, pty_core_supervisor.rs: 10 |
| 44 | [HOST] | Sessions survive service stop/restart; per-session fallback host when the core is unavailable | done-inherited | crates/supercli-core/src/session_host.rs | session_host.rs: 82 #[test] |
| 45 | [HOST] | On-disk session dir: manifest.json, output.bin journal, session.sock control | done-inherited | crates/supercli-core/src/session_host.rs | session_host.rs: 82 |
| 46 | [HOST] | Bounded append-only journal with monotonic offsets (~64–72 MiB retained) | done-inherited | crates/supercli-core/src/session_host.rs:56 | session_host.rs |
| 47 | [HOST] | Exact VT snapshot for attach from the resident libghostty-vt grid | done-inherited | crates/supercli-core/src/ghostty_vt.rs | ghostty_vt.rs: 1 |
| 48 | [HOST] | PID + start-time identity guard; never signal a recycled pid | done-inherited | crates/supercli-core/src/session_host.rs:3624 | session_host.rs |
| 49 | [HOST] | Session ownership/provenance fields (owner principal, device, source preset) | done-inherited | crates/supercli-core/src/session_host.rs:5783-5806 | session_host.rs |
| 50 | [HOST] | Auto-titling (agent OSC titles / first prompt / off; skip slash commands; rename wins) | done-inherited | crates/supercli-core/src/session_host.rs; crates/supercli-core/src/session_io.rs | session_host.rs |
| 51 | [HOST] | Titling from a resumed conversation's transcript (`__auto_title__`) | done-inherited | crates/supercli-core/src/transcripts/mod.rs:775 | transcripts |
| 52 | [HOST] | Archive (non-destructive stop) and restore; Restore & Resume | done-inherited | crates/supercli-core/src/session_ops.rs:303,332 | session_ops.rs: 23 |
| 53 | [HOST] | Auto-stop-and-archive idle sweep (30m to 24h; never unread-attention or pinned sessions) | done-inherited | crates/supercli-serve/src/auto_archive.rs; crates/supercli-serve/src/driver.rs | auto_archive.rs: 9 |
| 54 | [HOST] | Resume on restart using the hook-captured provider conversation id | done-inherited | crates/supercli-core/src/resume.rs | resume.rs: 6 |
| 55 | [HOST] | Resume Agent in place (same session id) and restart agent | done-inherited | crates/supercli-core/src/resume.rs | resume.rs |
| 56 | [HOST] | Session reload (replace the host, keep the id) | done-inherited | crates/supercli-core/src/session_ops.rs | session_ops.rs |
| 57 | [HOST] | Restart recommendation when the host protocol version is too old | done-inherited | crates/supercli-core/src/session_host.rs | session_host.rs |
| 58 | [HOST] | Safe text delivery (bracketed paste, settle, double-Enter) | done-inherited | crates/supercli-core/src/session_host.rs; crates/supercli-core/src/mcp_host.rs | session_host.rs |
| 59 | [HOST] | Hook listener ingesting provider lifecycle events via the port registry | done-inherited | crates/supercli-serve/src/hook_listener.rs | hook_listener.rs: 6 |
| 60 | [HOST] | Hook-latched busy/idle/attention activity engine (output is never evidence of work) | done-inherited | crates/supercli-core/src/screen_activity.rs; crates/supercli-serve/src/activity.rs | screen_activity.rs: 4 |
| 61 | [HOST] | Screen-tier busy/idle fallback for Claude/Codex/Gemini without hooks | done-inherited | crates/supercli-core/src/screen_activity.rs:48,60 | screen_activity.rs |
| 62 | [HOST] | Runtime observation of agents started by hand in a shell | done-inherited | crates/supercli-core/src/runtime_observer.rs | runtime_observer.rs: 11 |
| 63 | [HOST] | Agent-drawn select-menu detection sets attention | done-inherited | crates/supercli-core/src/menu_prompt.rs | menu_prompt.rs: 13 |
| 64 | [HOST] | Escape-cancellation fencing of interrupted turns | done-inherited | crates/supercli-core/src/hook_cancellation.rs | hook_cancellation.rs: 6 |
| 65 | [HOST] | Background/subagent tracking keeps the session busy until children stop | done-inherited | crates/supercli-core/src/durable_runs.rs | subagent_kill tests |
| 66 | [HOST] | Lifecycle notification policy (needs-input, opt-in finished, App alerts) with viewing-device suppression | done-inherited | crates/supercli-serve/src/notifications.rs | notifications.rs: 2 |
| 67 | [HOST] | Unread/mark-read and notify-when-done per session | done-inherited | crates/supercli-core/src/app_state.rs | app_state.rs |
| 68 | [HOST] | Shared approval hub (FIFO, coalesced, first answer wins from any Controller) | done-inherited | crates/supercli-serve/src/approvals.rs:517 | approvals.rs: 11 |
| 69 | [HOST] | Viewer presence leases and presence files | done-inherited | crates/supercli-serve/src/presence.rs | presence.rs: 3 |
| 70 | [HOST] | Phone-fit resize and desktop-fit restore of the shared grid | done-inherited | crates/supercli-core/src/controller_host.rs; crates/supercli-core/src/remote_session_backend.rs | remote_server.rs: 32 |
| 71 | [HOST] | Persisted activity log (activity-log.jsonl) | done-inherited | crates/supercli-core/src/activity_log.rs | activity_log.rs: 7 |
| 72 | [HOST] | Local URL detection plus verify/find/stop of session-owned local servers | done-inherited | crates/supercli-core/src/local_urls.rs | local_urls.rs: 18 |
| 73 | [HOST] | First-run preset seeding from agent CLIs found on PATH | done-inherited | crates/supercli-core/src/first_run.rs | first_run.rs: 2 |
| 74 | [HOST] | Projects, plain groups, worktree child projects, pins, manual order and date sort | done-inherited | crates/supercli-core/src/state.rs; crates/supercli-core/src/app_state.rs | state.rs |
| 75 | [HOST] | Cross-frontend state bus notifications | done-inherited | crates/supercli-core/src/state_bus.rs | state_bus.rs: 3 |
| 76 | [HOST] | Git worktree create/list (default base = mainline) | done-inherited | crates/supercli-core/src/worktrees.rs | worktrees.rs: 3 |
| 77 | [HOST] | Unified `unpeel` MCP server (stdio, per agent client, gate outside hosted sessions) | done-inherited | crates/supercli-core/src/mcp_host.rs | mcp_host.rs: 49 |
| 78 | [HOST] | MCP `sessions` domain (current, list, inspect, read_screen, read_output, wait_for_text, send_text, send_keys, report) | done-inherited | crates/supercli-core/src/mcp_host.rs | mcp_host.rs |
| 79 | [HOST] | MCP `agents` domain (list, get, read_transcript, wait) with occurrence-bound refs | done-inherited | crates/supercli-core/src/mcp_host.rs:702 | mcp_host.rs |
| 80 | [HOST] | MCP `workspace` domain (list_presets, create_worktree, list_worktrees) | done-inherited | crates/supercli-core/src/mcp_host.rs:703 | mcp_host.rs |
| 81 | [HOST] | MCP `artifacts.add_to_gallery` | done-inherited | crates/supercli-core/src/mcp_host.rs:704 | mcp_host.rs |
| 82 | [HOST] | MCP `browser` domain (13 actions incl. console, context, close) | done-inherited | crates/supercli-core/src/browser_mcp.rs | browser_mcp.rs |
| 83 | [HOST] | MCP `apps` domain (list, catalog, describe, search, context, open) with agent-openable App panes | done-inherited | crates/supercli-core/src/apps_mcp.rs | apps_mcp.rs |
| 84 | [HOST] | MCP `skills` domain (list, search, get) | done-inherited | crates/supercli-core/src/skills_mcp.rs | skills_mcp.rs |
| 85 | [HOST] | Open reads plus approval-controlled cross-session writes (ask/allow/deny, remembered directional pairs) | done-inherited | crates/supercli-core/src/mcp_host.rs; crates/supercli-core/src/grant_store.rs | grant_store.rs: 3, mcp_host.rs |
| 86 | [HOST] | Session creation and closing are user-only (agents refused) | done-inherited | crates/supercli-core/src/mcp_host.rs:685 | mcp_host.rs |
| 87 | [HOST] | Caller identity via env or a verified process-ancestry fallback | done-inherited | crates/supercli-core/src/mcp_host.rs; crates/supercli-core/src/session_host.rs:3624 | mcp_host.rs |
| 88 | [HOST] | Lazy per-action help with terse schemas under a token budget | done-inherited | crates/supercli-core/src/mcp_host.rs | mcp_host.rs |
| 89 | [HOST] | Dual-era MCP protocol support (initialize and server/discover) | done-inherited | crates/supercli-core/src/mcp_host.rs; crates/supercli-core/src/mcp_gate.rs | mcp_host.rs |
| 90 | [HOST] | MCP in-flight cancellation with ordered execution | done-inherited | crates/supercli-core/src/mcp_cancel.rs | mcp_cancel.rs: 4 |
| 91 | [HOST] | `/mcp/*` routes authenticated with a 0600 auth token | done-inherited | crates/supercli-core/src/mcp_auth.rs | mcp_auth.rs |
| 92 | [HOST] | App live context (`app-context.json`) surfaced to neighbouring agents | done-inherited | crates/supercli-serve/src/app_context.rs | app_context.rs: 3 |
| 93 | [HOST] | Pinned agent-browser engine auto-install (sha256, flock, background at worker start) | done-inherited | crates/supercli-core/src/browser_engine.rs | browser_engine.rs: 10 |
| 94 | [HOST] | Browser shared project window (pinned tab per session, persistent logins) or separate-per-session mode | done-inherited | crates/supercli-core/src/browser_mcp.rs | browser_mcp.rs |
| 95 | [HOST] | Browser access On/Ask/Off with remembered session approvals | done-inherited | crates/supercli-core/src/browser_mcp.rs; crates/supercli-core/src/config.rs | browser_mcp.rs |
| 96 | [HOST] | Browser options: headed, domain allowlist, custom Chromium path, agent cursor overlay, theme | done-inherited | crates/supercli-core/src/browser_mcp.rs:965,1832 | browser_mcp.rs |
| 97 | [HOST] | Remote CDP endpoint mode (`remote-cdp.json`) | done-inherited | crates/supercli-core/src/browser_mcp.rs; crates/supercli-core/src/browser_takeover.rs | browser_mcp.rs |
| 98 | [HOST] | Browser screenshots/downloads as session artifacts (auto-gallery toggle) | done-inherited | crates/supercli-core/src/browser_mcp.rs:2098-2114 | browser_mcp.rs |
| 99 | [HOST] | Session artifact store (list, read, resumable upload, delete, thumbnails) | done-inherited | crates/supercli-core/src/session_artifacts.rs | session_artifacts.rs: 21 |
| 100 | [HOST] | Host filesystem ops for folder pickers and file transfer (scoped to projects) | done-inherited | crates/supercli-core/src/host_resources.rs | host_resources.rs |
| 101 | [HOST] | One-time-code pairing with per-device bearer token and E2E key | done-inherited | crates/supercli-serve/src/pairing.rs | pairing.rs: 7 |
| 102 | [HOST] | Local pairing control route (begin/status/cancel/devices/revoke/relay-allowed) | done-inherited | crates/supercli-serve/src/pairing.rs:221,282,676 | pairing.rs |
| 103 | [HOST] | Controller-assisted pairing (pairing.invitation via an assisting Mac proxy) | done-inherited | crates/supercli-serve/src/pairing.rs:248; crates/supercli-core/src/controller_protocol.rs | pairing.rs |
| 104 | [HOST] | Direct `/mobile` over TLS with the pinned Host certificate | done-inherited | crates/supercli-core/src/remote_server.rs:184-192 | remote_server.rs: 32 |
| 105 | [HOST] | Supervised WSS terminal streamer (backoff, crash-loop ceiling) | done-inherited | crates/supercli-serve/src/remote_streamer.rs:222,278 | remote_streamer.rs |
| 106 | [HOST] | Local `host.sock` framed Controller contract (0600) | done-inherited | crates/supercli-core/src/remote_server.rs; crates/supercli-core/src/remote_stdio.rs | remote_server.rs |
| 107 | [HOST] | SSH stdio Host gateway (`__remote_stdio__`), including interactive-shell compat mode | done-inherited | crates/supercli-core/src/remote_stdio.rs | remote_stdio.rs: 6 |
| 108 | [HOST] | Unpeel Link relay uplink with forward-secret E2E; token rotation without eviction | done-inherited | crates/supercli-core/src/relay_uplink.rs; crates/supercli-core/src/relay_crypto.rs | relay_uplink.rs: 8, relay_crypto.rs: 6 |
| 109 | [HOST] | Relay credential recovery | done-inherited | crates/supercli-serve/src/relay.rs:361,611-616 | relay.rs |
| 110 | [HOST] | Direct-path negotiation and UDP NAT punch upgrade from relay | done-inherited | crates/supercli-core/src/direct_path.rs; crates/supercli-core/src/direct_path_punch.rs | direct_path.rs: 4 |
| 111 | [HOST] | Unpeel Link license activation/entitlement/seat (Ed25519 keys) | done-inherited | crates/supercli-core/src/license.rs | license.rs: 6 |
| 112 | [HOST] | APNs push registration and delivery via relay | done-inherited | crates/supercli-serve/src/notifications.rs:6 | notifications.rs |
| 113 | [HOST] | Platform-adapter seam for native effects (notify, push, approvals, overlay, thumbnails, Link, open-in-editor) | done-inherited | crates/supercli-serve/src/platform_adapter.rs | platform_adapter.rs: 5 |
| 114 | [HOST] | Host-owned App install/open/opener policy (works on SSH/Linux Hosts) | done-inherited | crates/supercli-core/src/app_installer.rs; crates/supercli-core/src/app_open.rs | app_installer.rs: 4 |
| 115 | [HOST] | Plugin activation, ordering and lazy update checks for agents and Apps | done-inherited | crates/supercli-core/src/plugins.rs; crates/supercli-core/src/plugin_updates.rs | plugins.rs: 10 |
| 116 | [HOST] | `__remote_attach__` network attach to another Host's session | done-inherited | crates/supercli-core/src/remote_attach.rs | remote_attach.rs: 1 |
| 117 | [HOST] | Graphical desktop-session service unit and diagnostics (Linux) | done-inherited | crates/supercli-core/src/desktop_session.rs; crates/supercli-serve/src/service_install.rs | desktop_session.rs: 3 |
| 118 | [HOST] | Timestamped trace log for all components | done-inherited | crates/supercli-serve/src/tracelog.rs | tracelog.rs |
| 119 | [RUNTIMES] | Runtime package format (runtime.toml, adapters, hooks, icon) auto-discovered at build | done-inherited | `runtimes/*/runtime.toml` (15 files) | `runtimes/hook-plugins.test.js`, `runtimes/test_support.rs` |
| 120 | [RUNTIMES] | Claude Code integration (hooks, MCP, screen fallback, transcript, semantic titles, subagents) | done-inherited | `runtimes/claude-code/` | `runtimes/hook_reporter_tests.rs` |
| 121 | [RUNTIMES] | Codex integration (native hooks + notify, MCP, screen fallback, transcript) | done-inherited | `runtimes/codex/` | `runtimes/hook_reporter_tests.rs` |
| 122 | [RUNTIMES] | Gemini CLI integration (hooks, screen fallback, transcript) | done-inherited | `runtimes/gemini/` | `runtimes/hook_reporter_tests.rs` |
| 123 | [RUNTIMES] | Cursor Agent, Grok, Kimi, Kiro and Cline integrations (hooks, MCP where declared, transcripts) | done-inherited | `runtimes/cursor-agent/`, `runtimes/grok/`, `runtimes/kimi/`, `runtimes/kiro/`, `runtimes/cline/` | `runtimes/hook_reporter_tests.rs` |
| 124 | [RUNTIMES] | Amp and GitHub Copilot per-project hook integrations | done-inherited | `runtimes/amp/`, `runtimes/github-copilot/` | `runtimes/hook_reporter_tests.rs` |
| 125 | [RUNTIMES] | OpenCode plugin and Muse Code plugin integrations | done-inherited | `runtimes/opencode/`, `runtimes/muse-code/` | `runtimes/hook_reporter_tests.rs` |
| 126 | [RUNTIMES] | Antigravity and fx MCP-only integrations; Pi detection-only | done-inherited | `runtimes/antigravity/`, `runtimes/fx/`, `runtimes/pi/` | `runtimes/hook_reporter_tests.rs` |
| 127 | [RUNTIMES] | Provider-neutral launch (preset runs exactly as typed) with suggested default presets per runtime | done-inherited | `crates/supercli-core/src/` (preset launch) | `crates/supercli-cli/tests/` |
| 128 | [RUNTIMES] | Shared MCP shim (`~/.unpeel/bin/unpeel-mcp`) plus post-upgrade integration refresh | done-inherited | `~/.supercli/bin/supercli-mcp` (via `crates/supercli-cli/src/integrations_cli.rs`) | `crates/supercli-cli/tests/` |
| 129 | [RUNTIMES] | Per-runtime install command, usage stores and resume recipes | done-inherited | `crates/supercli-cli/src/integrations_cli.rs`; `runtimes/*/docs/` | `runtimes/hook_reporter_tests.rs` |
| 130 | [APPS] | `unpeel-attach` client (snapshot/tail replay, event-driven follow, focus filtering, resize) | done | `crates/supercli-attach/src/lib.rs` | `focus_filter_drops_exact_focus_in_and_out`, `resize_command_encoding_matches_host_protocol` |
| 131 | [APPS] | `unpeel-apps` "send to adjacent agent" API with clipboard fallback | done | `crates/supercli-apps/src/agent.rs` | (unit tests in `agent.rs`) |
| 132 | [APPS] | Official App registry with typed resources (file media type, folder, git.working-tree) | done | `crates/supercli-core/src/app_open.rs`, `apps_mcp.rs`, `controller_host.rs` | `typed_resources_are_one_shell_safe_argument`, `opener_projection_migrates_legacy_file_keys_and_keeps_typed_resources` |
| 133 | [APPS] | App companion panes with reveal revisions and App-branded session rows | partial | `crates/supercli-core/src/app_open.rs`, `mcp_host.rs` | — |
| 134 | [APPS] | Git App: Changes tab with status glyphs and unified diffs | partial | `crates/apps/diffs/src/app.rs`, `ui.rs`, `git.rs`; gpuidart: `clients/supercli-app/lib/screens/gitpaneview.dart`, `lib/widgets/git_widgets.dart` (stub `StubGitDataSource` — no Host git route yet, see `docs/gpuidart-gaps-apps.md` GAP-A1) | `test/apps_panes_test.dart` (21 tests); `docs/internal/proofs/proof-screenshots/parity-apps-git.png` (snapshot-raster proof, not native window) |
| 135 | [APPS] | Git App: syntax-highlighted patches with size limits | partial | `crates/apps/diffs/src/highlight.rs`; gpuidart: `lib/widgets/git_widgets.dart` (`diffColumn`, `kMaxDiffLinesPerFile = 200` + truncation marker). Line-level colors only — token-level syntax highlighting has no upstream primitive (see `docs/gpuidart-gaps-apps.md` GAP-A3) | `test/apps_panes_test.dart` (diff classification + truncation) |
| 136 | [APPS] | Git App: History tab (paged commits, commit files, patches) | partial | `crates/apps/diffs/src/app.rs` (`Tab::History`, `Screen::History`); gpuidart: `clients/supercli-app/lib/screens/gitpaneview.dart` (History tab with commit list + selected patch; stub data) | `test/apps_panes_test.dart` (history tab renders commits + patch) |
| 137 | [APPS] | Git App: Fetch/Pull/Push control (fast-forward only, no force) | partial | `crates/apps/diffs/src/git.rs` (`RemoteAction::Fetch`, pull, `push --no-force --no-mirror`); gpuidart: `clients/supercli-app/lib/screens/gitpaneview.dart` (Fetch/Pull/Push buttons + `git.fetch/pull/push` `UiAction`s and callbacks; no live backend — GAP-A1) | `test/apps_panes_test.dart` (remote actions exposed) |
| 138 | [APPS] | Git App: select lines, then Copy lines or Send reference to agent | partial | `crates/apps/diffs/src/ui.rs` (`select-diff-lines`, `copy-lines`); gpuidart: `lib/screens/gitpaneview.dart` (`onCopyLines` callback; no upstream clipboard API — GAP-A4) | `test/apps_panes_test.dart` (stage callback; copy is a shell-wired callback) |
| 139 | [APPS] | Files App: scoped explorer with filter and `--ext`, context menu (open/send/copy), path drag | partial | `crates/apps/filetree/src/ui.rs`; gpuidart: `clients/supercli-app/lib/screens/filespaneview.dart` (scoped tree, filter, preview, `onOpen`/`onSendToAgent`/`onCopyPath` callbacks — no upstream context-menu API, GAP-A4; stub `StubFilesDataSource`, GAP-A2) | `test/apps_panes_test.dart` (tree order, filtering, preview, callbacks); `docs/internal/proofs/proof-screenshots/parity-apps-files.png` (snapshot-raster proof) |
| 140 | [APPS] | Files/Git/Usage Apps follow the neighbouring agent's project/worktree | partial | `crates/apps/diffs/src/app.rs:153`, `filetree/src/ui.rs:35`; gpuidart: modelled via `rootPath` on the Files stub (`lib/screens/filespaneview.dart`); live follow needs the Host to report the adjacent agent's worktree (GAP-A2) | `test/apps_panes_test.dart` (root path in header) |
| 141 | [APPS] | Markdown App: live-styled editor, slash menu, `\` palette, to-dos, full mouse editing | partial | `crates/apps/markdown/src/` (`slash.rs`, `mouse.rs`, `highlight.rs`, block/heading/format); gpuidart: `clients/supercli-app/lib/screens/markdownpaneview.dart` (read-only rendered preview: headings, lists, task items, quotes, fenced code, rules; block parser + inline bold/italic/code/links). The live editable surface is `lib/widgets/markdowneditorview.dart` (separate) | `test/apps_panes_test.dart` (block + inline parser, preview render); `docs/internal/proofs/proof-screenshots/parity-apps-markdown.png` (snapshot-raster proof) |
| 142 | [APPS] | Markdown App: auto-save, Open/Save/New note, vault explorer, remembered notes folder | partial | `crates/apps/markdown/src/` (`backend.rs`, `picker.rs`, `start.rs`); gpuidart: `clients/supercli-app/lib/screens/markdownpaneview.dart` (vault explorer, `+ New note`, Open/Save buttons + `onNewNote`/`onOpenNote`/`onSaveNote` callbacks; no upstream file-dialog API — GAP-A4; stub `StubMarkdownDataSource`, GAP-A2) | `test/apps_panes_test.dart` (vault list, note selection) |
| 143 | [APPS] | Usage App: per-provider quota gauges, token history, project/total monthly tables, alerts, themes | partial | `crates/apps/usage/src/` (`claude.rs`, `codex.rs`, `grok.rs`, `muse.rs`, `ui.rs`); gpuidart: `clients/supercli-app/lib/screens/usagepaneview.dart` (per-provider text gauges, 14-day sparkline, monthly project/total table, threshold alerts, `UsageTheme`; stub `StubUsageDataSource`, GAP-A2) | `test/apps_panes_test.dart` (gauge math, alerts, table totals); `docs/internal/proofs/proof-screenshots/parity-apps-usage.png` (snapshot-raster proof) |
| 144 | [APPS] | App Kit component library (Page/List/Input/TextBox/Tree/Menu/Gauge/Sparkline/Charts/Media/Surface) | partial | `clients/legacy/app-kit` (via bridge in `crates/apps/*/Cargo.toml`) | — |
| 145 | [APPS] | App Kit hosted semantic UI bridge (revisions, deltas, scoped participant tokens, agent `edit` grants) | partial | `clients/legacy/app-kit` (via bridge) | — |
| 146 | [APPS] | App Kit SwiftUI peer renderer plus Kitchen Sink mini-host and screen audit | missing | — | — |
| 147 | [WEB] | App Kit TypeScript/DOM/ARIA web renderer of the same App tree | partial | `clients/supercli-app/lib/web/dom_renderer.dart`, `page.dart` (PoC: UiNode JSON → HTML+ARIA; 6 upstream node kinds, graceful unknown-kind fallback) | `test/web_renderer_test.dart` (11 tests), `tool/render_web_sample.dart` → `docs/internal/proofs/proof-screenshots/parity-web-147.html` |
| 148 | [WEB] | Hosted installer endpoints (`unpeel.com/install.sh`, per-App `install/<app>/install.sh`) with channel selection (alpha/beta/stable) | done | `scripts/install.sh`, `scripts/install-app.sh` | — |
| 149 | [WEB] | Help/docs/download links (unpeel.com/docs, /download/mac, /ios) referenced from the apps | missing | — | — |
| 150 | [DESKTOP] | App shell: resizable vibrancy sidebar, custom titlebar, ⌘B toggle | partial | clients/supercli-app/lib/screens/rootview.dart, chrome.dart (scaffold bbd8b70) | screens_test.dart (RootView layout) |
| 151 | [DESKTOP] | Project/session tree with pins, groups, worktree folders, attention dot and busy spinners | done | clients/supercli-app/lib/screens/sidebarview.dart, projectsidebarview.dart (track-b-parity-sidebar) | test/sidebar_test.dart (31 tests: rows, filter, pinned-first, worktree dedup) + proof-screenshots/parity-sidebar.png |
| 152 | [DESKTOP] | Detached drag of sessions/projects (reorder, move to group, drop-to-split) | partial | clients/supercli-app/lib/screens/projectsidebarview.dart `SidebarSessionDrag` (track-b-parity-sidebar); live drag needs gpuidart DnD, see docs/gpuidart-gaps-sidebar.md G-1 | test/sidebar_test.dart (drag validation: self-drop, same-group no-op, split target) |
| 153 | [DESKTOP] | Session context menu (rename, copy ID, copy transcript, notify when done, clear attention, resume, restart app, reveal, pin, stop and archive, remove) | partial | clients/supercli-app/lib/screens/sidebarview.dart `SessionContextMenu` (track-b-parity-sidebar); all 11 items render as buttons, click-id decoding; popup anchoring needs framework menu, see docs/gpuidart-gaps-sidebar.md G-2 | test/sidebar_test.dart (item list, unique actions, id decoding) + proof-screenshots/parity-sidebar.png |
| 154 | [DESKTOP] | Project context menu (new worktree, new group, rename, stop all, sort, folder color, archived, open in editor, move to workspace) | partial | clients/supercli-app/lib/screens/sidebarview.dart `ProjectContextMenu` (track-b-parity-sidebar); all 9 items render as buttons; popup anchoring needs framework menu, see docs/gpuidart-gaps-sidebar.md G-2 | test/sidebar_test.dart (item list) + proof-screenshots/parity-sidebar.png |
| 155 | [DESKTOP] | Folder color palette (8 colors) | missing | - | - |
| 156 | [DESKTOP] | Archived sessions library with search, Restore, Restore & Resume, Delete permanently | partial | clients/supercli-app/lib/screens/archivedsessionsview.dart (34 lines, bbd8b70) | - |
| 157 | [DESKTOP] | Workspace dots, trackpad swipe and workspace selector popover | partial | clients/supercli-app/lib/screens/workspaceopenmenu.dart (85 lines, bbd8b70) | - |
| 158 | [DESKTOP] | Open a workspace in a new window | missing | - | - |
| 159 | [DESKTOP] | Move a project between local workspaces | missing | - | - |
| 160 | [DESKTOP] | Right-side global project sidebar panel (pinned session stack) | partial | clients/supercli-app/lib/screens/projectsidebarview.dart (111 lines, bbd8b70) | - |
| 161 | [DESKTOP] | Local-site globe button with open URL / stop server menu | missing | clients/supercli-app/lib/screens/localsitemenu.dart (4-line re-export, bbd8b70) | - |
| 162 | [DESKTOP] | Titlebar "Open in" menu for 24 external editors/terminals/git clients | missing | - | - |
| 163 | [DESKTOP] | Session launcher (pick a tool) and empty state | partial | clients/supercli-app/lib/screens/sessionlauncherview.dart (35 lines, bbd8b70) | - |
| 164 | [DESKTOP] | ⌘K command palette (sessions, projects, presets, commands) | partial | clients/supercli-app/lib/screens/commandpaletteview.dart (61 lines, bbd8b70) | screens_test.dart (palette) |
| 165 | [DESKTOP] | ⌃Tab MRU session switcher | missing | - | - |
| 166 | [DESKTOP] | ⌘1–9 session switching and ⌃1–9 project switching with held-key hints | missing | - | - |
| 167 | [DESKTOP] | Recent activity page (⇧⌘R) and titlebar activity bell dropdown | partial | clients/supercli-app/lib/screens/recentactivityview.dart, globalactivitymenu.dart (bbd8b70) | - |
| 168 | [DESKTOP] | Toast notifications (for example, device connected) | done | clients/supercli-app/lib/screens/toastcenter.dart (queue-driven, auto-dismiss TTL, click-to-focus; RLE fallback) + lib/notifications.dart (NotificationQueue: FIFO, bounded eviction, TTL prune) | clients/supercli-app/test/approvals_panel_test.dart (25 tests: queue FIFO/evict/TTL/dismiss, toast render/dismiss/focus); screenshot docs/internal/proofs/proof-screenshots/parity-approvals.png |
| 169 | [DESKTOP] | libghostty Metal terminal surfaces, retained per session | partial | clients/supercli-app/lib/screens/terminalpaneview.dart (54 lines, bbd8b70) | - |
| 170 | [DESKTOP] | Remote-host panes rendered via in-memory Ghostty surfaces (same UI as local) | missing | - | - |
| 171 | [DESKTOP] | Split Pane Right/Down (⌘D / ⇧⌘D), recursive tree up to 8 panes | done | clients/supercli-app/lib/pane_layout.dart (PaneLayout split H/V, 8-pane limit, track-b-parity-panes) | clients/supercli-app/test/pane_layout_test.dart |
| 172 | [DESKTOP] | Zoom pane (⇧⌘↩), Equalize splits, spatial focus (⌥⌘ arrows) | done | clients/supercli-app/lib/pane_layout.dart (zoom/unzoom/toggle, equalize, focusDirection, track-b-parity-panes) | clients/supercli-app/test/pane_layout_test.dart |
| 173 | [DESKTOP] | Detach Pane / Exit Multi-Pane View | partial | clients/supercli-app/lib/screens/terminalpanewindow.dart (TerminalPaneWindow exists; needs gpuidart multi-window P0-14) | - |
| 174 | [DESKTOP] | Pane header menu with Agents/Plugins launch sections | missing | - | - |
| 175 | [DESKTOP] | Transient launcher pane for new sessions in a group | partial | clients/supercli-app/lib/screens/sessionlauncherview.dart (35 lines, bbd8b70) | - |
| 176 | [DESKTOP] | Persisted pane layouts per scope (pane-layouts.json) | partial | clients/supercli-app/lib/pane_layout.dart (toJson ready; host persistence hook not yet wired) | - |
| 177 | [DESKTOP] | Find bar (⌘F, ⌘G, ⇧⌘G) | partial | clients/supercli-app/lib/screens/terminalfindbar.dart (35 lines, bbd8b70) | - |
| 178 | [DESKTOP] | Font size increase/decrease/reset (⌘+ ⌘- ⌘0) | missing | - | - |
| 179 | [DESKTOP] | URL/OSC 8 links and OSC 7 cwd tracking | partial | clients/supercli-app/lib/screens/clickablepath.dart (23 lines, bbd8b70) | - |
| 180 | [DESKTOP] | ⌘-click bare file paths (with line/column) to open in App or editor | partial | clients/supercli-app/lib/screens/clickablepath.dart (23 lines, bbd8b70) | - |
| 181 | [DESKTOP] | Native file drag out of hosted Apps and drop into Apps/terminals | missing | - | - |
| 182 | [DESKTOP] | Scroll-to-bottom button; exited-session bar (Resume / Start fresh); resume-failure notice | missing | - | - |
| 183 | [DESKTOP] | Restart recommendation banner | missing | - | - |
| 184 | [DESKTOP] | Agent TUI background color matching for chrome | missing | - | - |
| 185 | [DESKTOP] | Viewer presence avatars and "Fit to desktop" control | partial | clients/supercli-app/lib/screens/vieweravatarsview.dart (20 lines, bbd8b70) | - |
| 186 | [DESKTOP] | In-pane MCP approval overlay (write/browser/app-open) | done | clients/supercli-app/lib/screens/mcpapprovalpanel.dart (McpApprovalPanel: attention dot, tool/summary/detail, N-more-waiting, Allow Ctrl+Enter / Don't Allow Ctrl+Shift+Enter / Edit Ctrl+E; ApprovalsPanel list with selection; RLE fallback) + lib/app.dart (de51bb2 inline card) | clients/supercli-app/test/approvals_panel_test.dart (25 tests: approve/deny/edit decision flow, key bindings, list empty/count/selection) + test/app_test.dart; screenshots docs/internal/proofs/proof-screenshots/approve-{before,after}.png, deny-{before,after}.png, parity-approvals.png |
| 187 | [DESKTOP] | Session gallery panel (screenshots, downloads, uploads) | partial | clients/supercli-app/lib/screens/sessiongallerypanel.dart (50 lines, bbd8b70) | - |
| 188 | [DESKTOP] | Gallery arrow + crop markup and "Add to prompt" | missing | clients/supercli-app/lib/screens/sessiongallerymarkup.dart (6-line re-export, bbd8b70) | - |
| 189 | [DESKTOP] | Take Screenshot (⇧⌘S) into the session and attach it to the prompt | partial | clients/supercli-app/lib/screens/sessionscreenshotcapture.dart (21 lines, bbd8b70) | - |
| 190 | [DESKTOP] | Full main menu set (App/Session/Edit/View/Window/Help) with shortcuts | missing | - | - |
| 191 | [DESKTOP] | Menu-bar status item with activity spinner and popover | missing | - | - |
| 192 | [DESKTOP] | Keep running as a menu-bar agent when the window closes | missing | - | - |
| 193 | [DESKTOP] | Finder "New Unpeel Session Here" service | missing | - | - |
| 194 | [DESKTOP] | Sparkle auto-updates with a beta channel opt-in | missing | - | - |
| 195 | [DESKTOP] | macOS Notification Center banners (needs input / finished / App alerts) plus a test notification | missing | - | - |
| 196 | [DESKTOP] | Keychain-backed Link license | missing | - | - |
| 197 | [DESKTOP] | Bundled Host service lifecycle management via launchd | missing | - | - |
| 198 | [DESKTOP] | Bonjour nearby-host discovery for Add Workspace | missing | - | - |
| 199 | [DESKTOP] | Remote folder picker for launching on remote Hosts | missing | clients/supercli-app/lib/screens/remotefolderpicker.dart (4-line re-export, bbd8b70) | - |
| 200 | [DESKTOP] | Settings scope picker (This Mac / workspace / remote Host) with inherit/reset | partial | clients/supercli-app/lib/screens/settingsview.dart (152 lines) + settingspanels.dart SettingsScope (519 lines) | - |
| 201 | [DESKTOP] | Settings ▸ Workspaces (unified list, add local/nearby/code/SSH, rename, color, forget, delete) | partial | clients/supercli-app/lib/screens/workspacessettingspanel.dart (57 lines, real panel + dataset) | - |
| 202 | [DESKTOP] | Settings ▸ Agents (install CLI, install/reinstall integration, commands and variants, default, activate, reorder) | missing | - | - |
| 203 | [DESKTOP] | Settings ▸ Plugins (Apps catalog install/update/activate/order) | partial | clients/supercli-app/lib/screens/pluginsettingspanel.dart (81 lines, real panel + dataset + row actions) | - |
| 204 | [DESKTOP] | Settings ▸ Agent access ▸ Sessions (write policy, worktree permission, auto-gallery, approved pairs/Apps with Revoke) | partial | clients/supercli-app/lib/screens/sessionsaccesssections.dart (203 lines, real panel) | - |
| 205 | [DESKTOP] | Settings ▸ Agent access ▸ Browser (engine status, access mode, approvals, window/cursor/scope/app path, site rules, clear data) | partial | clients/supercli-app/lib/screens/sessionsaccesssections.dart BrowserAccessSections (203 lines, real panel) | - |
| 206 | [DESKTOP] | Settings ▸ Appearance (mode, 8 accent colors, background/surface/transparency, terminal font and size, line height, session title mode, gallery chip) | partial | clients/supercli-app/lib/screens/settingspanels.dart GeneralSettingsPanel (519 lines, theme/accent/font/size) | - |
| 207 | [DESKTOP] | Settings ▸ Remote Control (share this Mac/workspace QR/code, paired devices, revoke, per-device Link toggle) | partial | clients/supercli-app/lib/screens/hostpickerview.dart (84 lines, bbd8b70) | - |
| 208 | [DESKTOP] | Add iPhone/iPad to a remote Host (controller-assisted pairing) | missing | clients/supercli-app/lib/screens/remotehostworkspaceview.dart (4-line re-export, bbd8b70) | - |
| 209 | [DESKTOP] | Unpeel Link license section (activate, seats, release seat, get Link) | partial | clients/supercli-app/lib/screens/licensesettings.dart (63 lines, real panel + seats dataset) | - |
| 210 | [DESKTOP] | Settings ▸ Transcripts (content toggles, info header, range) | partial | clients/supercli-app/lib/screens/settingspanels.dart TranscriptsSettingsPanel (519 lines) | - |
| 211 | [DESKTOP] | Settings ▸ Notifications (flag select menus, completion, test Mac/phone, delivery diagnostics) | partial | clients/supercli-app/lib/screens/settingspanels.dart NotificationsSettingsPanel (519 lines) | - |
| 212 | [DESKTOP] | Settings ▸ Worktrees ("Show agent worktrees", list with create/reveal/remove) | partial | clients/supercli-app/lib/screens/worktreessettingspanel.dart (72 lines, real panel + filter) | - |
| 213 | [DESKTOP] | Settings ▸ Features (Remote workspaces, Git worktrees, Sessions use, Workspaces, Browser use) | partial | clients/supercli-app/lib/screens/settingspanels.dart FeaturesSettingsPanel (519 lines) | - |
| 214 | [DESKTOP] | Settings ▸ Advanced (auto-archive cleanup, sidebar archive preview, memory, running hosts by CPU with Stop, sessions folder, trace log) | partial | clients/supercli-app/lib/screens/settingspanels.dart AdvancedSettingsPanel (519 lines) | - |
| 215 | [DESKTOP] | Default editor / opener preference | missing | - | - |
| 216 | [DESKTOP] | Presets stored in the shared app-state.json with live pickup of CLI edits | partial | clients/supercli-app/lib/screens/presetssettingspanel.dart (88 lines, real panel + JSON roundtrip) | - |
| 217 | [DESKTOP] | Remote Host scope uses the same sidebar/content UI with Direct→Link automatic fallback | missing | - | - |
| 218 | [DESKTOP] | Worktree discovery of agent-created checkouts (opt-in, every 5 s) | missing | - | - |
| 219 | [DESKTOP] | Startup presentation cache for instant sidebar | missing | clients/supercli-app/lib/screens/sidebarskeleton.dart (4-line re-export, bbd8b70) | - |
| 220 | [IOS] | QR scan or paste-code pairing; multiple paired workspaces with switcher; Forget | missing | — | — |
| 221 | [IOS] | Direct pinned-HTTPS LAN transport plus Link relay E2E transport with credential repair | missing | — | — |
| 222 | [IOS] | Mac-style sessions drawer (projects, sessions, activity pills, ages, workspace header) | missing | — | — |
| 223 | [IOS] | Presets drawer to start a new session on the Host | missing | — | — |
| 224 | [IOS] | Live Ghostty terminal via WSS (HTTP long-poll fallback) with snapshot baseline and resync | missing | — | — |
| 225 | [IOS] | Keyboard never resizes the remote grid; "Fit terminal to screen" and revert to desktop | missing | — | — |
| 226 | [IOS] | Extra-keys accessory bar (esc/ctrl/alt/cmd/tab/arrows/symbols/paste/backspace/hide) | missing | — | — |
| 227 | [IOS] | Select-menu control bar (↑/↓/esc/return) for agent menus | missing | — | — |
| 228 | [IOS] | Mosh-style predictive echo and predictive scrolling; remote mouse-wheel; pinch zoom; long-press text selection | missing | — | — |
| 229 | [IOS] | Push-to-talk dictation with live transcript (paste/discard) | missing | — | — |
| 230 | [IOS] | Optional Apple Intelligence "polish" of dictation (iOS 26+) | missing | — | — |
| 231 | [IOS] | In-session Allow / Don't Allow approval prompts (first answer wins) | missing | — | — |
| 232 | [IOS] | APNs push registration per workspace, retry, tap routes to the session | missing | — | — |
| 233 | [IOS] | Title-bar activity bell with active sessions panel | missing | — | — |
| 234 | [IOS] | Exited-session restart bar (Resume) | missing | — | — |
| 235 | [IOS] | Session organize sheet (rename, notify when done, copy transcript Markdown 20/50/whole, restore/remove, resume) | missing | — | — |
| 236 | [IOS] | Project organize sheet (rename group, sort custom/date, folder color, archive library) | missing | — | — |
| 237 | [IOS] | Archived sessions sheet with Restore / Restore & Resume | missing | — | — |
| 238 | [IOS] | Session gallery with pinch-zoom full-size view and delete | missing | — | — |
| 239 | [IOS] | Upload photos into the session (resumable, JPEG/PNG transcode) | missing | — | — |
| 240 | [IOS] | Arrow/crop and PencilKit freehand image annotation, then "Add to message" | missing | — | — |
| 241 | [IOS] | Request-screenshot action (prompts the agent, polls, opens gallery) | missing | — | — |
| 242 | [IOS] | Face ID / Touch ID / passcode app lock | missing | — | — |
| 243 | [IOS] | Session actions from phone (stop, restart, resume agent, archive, remove, reorder, mark read) | missing | — | — |
| 244 | [IOS] | Capability-gated UI driven by Host bootstrap; connection-lost and push-warning banners; reconnect backoff | missing | — | — |
| 245 | [SHARED] | One Swift implementation of Host protocol DTOs, pairing client and paired-host records for Mac and iOS | done-inherited | crates/supercli-client/src/hosts.rs; crates/supercli-client/src/credentials.rs; crates/supercli-client/src/pairing.rs | supercli-client |
| 246 | [SHARED] | Relay forward-secret E2E protocol and WebSocket client, pinned by cross-language KAT vectors | done-inherited | crates/supercli-core/src/relay_crypto.rs; crates/supercli-client/src/crypto.rs; crates/supercli-client/src/relay_conn.rs; protocol/relay-kat-vectors-v2.json | relay_crypto.rs: 6 |
| 247 | [PROTO] | Versioned Host capability ledger (52 op ids, major 1 / minor 21, additive, capability-checked) | done-inherited | protocol/host-capabilities-v1.json | crates/supercli-cli/tests/cases/host_launch_conformance.py |
| 248 | [PROTO] | Host conformance and bootstrap-compatibility fixtures that every Host implementation must pass | done-inherited | protocol/host-conformance-v1.json; protocol/host-bootstrap-compatibility-v1.json | crates/supercli-cli/tests/cases/host_launch_conformance.py |
| 249 | [PROTO] | Normative pane-layout operations, direct-path v1, relay KAT, browser-engine pin and App registry contracts | done-inherited | protocol/pane-layout-operations-v1.json; protocol/direct-path-v1.json; protocol/relay-kat-vectors-v2.json; protocol/browser-engine-v1.json; protocol/app-registry.json | crates/supercli-cli/tests/cases/relay_conformance.py |
| 250 | [PROTO] | Unpeel UI protocol v1 (NDJSON App-to-Host semantic UI) schema, stream and fixtures | done-inherited | protocol/supercli-ui-v1.schema.json; protocol/supercli-ui-stream-v1.ndjson; protocol/supercli-ui-fixtures-v1.json | supercli-ui fixtures |
| 251 | [DIST] | SHA-256-verified curl installers for the CLI and each App; channels alpha/beta/stable on R2 | done | `scripts/install.sh`, `scripts/install-app.sh` | `scripts/release-installer.test.mjs`, `scripts/release-app-installer.test.mjs` |
| 252 | [DIST] | Lockstep app/CLI versioning; CLI archives carry protocol/, generated/, provenance and notices | done | `scripts/` (release tooling); `crates/Cargo.toml` (version) | `scripts/release-installer.test.mjs` |
| 253 | [DIST] | Signed and notarized Mac DMG plus Sparkle appcast; iOS via TestFlight | partial | `scripts/` (DMG/appcast tooling) | - |
