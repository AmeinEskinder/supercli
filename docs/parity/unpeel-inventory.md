# Unpeel Feature Inventory (upstream `origin/main` @ 7f2f5a3) — parity baseline for supercli

All paths are repo-relative to the unpeel tree at `origin/main` (commit 7f2f5a3, protocol major 1 / minor 21, app/CLI release line 0.7.x). Every row cites the file(s) where the behaviour was found; nothing here is inferred from outside the tree. "Retired"/"removed" items are listed only where the code still carries a compatibility surface that a rewrite must decide about.

## Summary

1. **What it is:** Unpeel is an "agent-first terminal multiplexer" written in Rust. Sessions run on machines you own and survive app/client/SSH/upgrade restarts. Provider hooks show busy/idle/needs-you state. Every session gets one built-in `unpeel` MCP server with sessions, agents, workspace, artifacts, browser, apps and skills domains (README.md).
2. **Server side:** `unpeel` CLI (crates/unpeel-cli), the `unpeel serve` Host service (crates/unpeel-serve), the multi-mode helper binary `unpeel-host` (crates/unpeel-host), the shared core library (crates/unpeel-core), the attach client (crates/unpeel-attach), and a C-ABI bridge for the Mac app (crates/unpeel-native-bridge).
3. **Clients:** a macOS SwiftUI + libghostty app (clients/native), an iPhone/iPad remote Controller (clients/ios), and a shared Swift protocol/crypto package (clients/shared). There is no longer an interactive TUI; it was removed 2026-09-03.
4. **Apps:** four first-party "Unpeel Apps" are standalone Ratatui TUIs built on App Kit: Git (diffs), Files (filetree), Markdown and Usage. App Kit also has Swift and web peer renderers (crates/apps/*).
5. **Runtimes:** 15 built-in agent runtime packages (runtimes/*): Claude Code, Codex, Gemini, Cursor Agent, Grok, Kimi, Kiro, Cline, Amp, OpenCode, Muse Code, Antigravity, GitHub Copilot, fx and Pi.
6. **Remote:** pairing uses a one-time QR/code. Transports are Direct LAN over pinned TLS, SSH stdio, and Unpeel Link, an end-to-end-encrypted relay (paid via license key). There is also direct-path UDP punch negotiation and controller-assisted pairing.
7. **Protocol:** one versioned Host protocol with 52 capability ids (protocol/host-capabilities-v1.json), conformance fixtures, pane-layout operations, the App registry and the App UI protocol (protocol/*).
8. **Section counts (rows in the tables below):** Docs/Product 22 · CLI 59 · Host/daemon/serve/core 97 · Attach/Apps/App Kit 38 · Runtimes 22 · macOS desktop 100 · iOS 52 · Shared Swift 9 · Protocol/generated 12 (411 rows in total).
9. **Parity checklist:** 253 distinct numbered items. Tags: [HOST] 78, [DESKTOP] 70, [CLI] 40, [IOS] 25, [APPS] 17, [RUNTIMES] 11, [PROTO] 4, [WEB] 3, [DIST] 3, [SHARED] 2.
10. **Key principles to preserve:** sessions are user-created only (agents can only open App panes); reads are open and cross-session writes need approval; everything is plain files under `~/.unpeel/`; the protocol is capability-advertised and additive; launching is provider-neutral and integrations are installed explicitly.

---

## 1. Product pitch and documented features (README.md, docs/)

| Feature | What it does (user-facing) | Source path(s) |
|---|---|---|
| Sessions outlive everything | Closing the window, quitting the app, dropping SSH, rebooting the client or upgrading Unpeel does not stop the agent | README.md; docs/agents/pty-core.md |
| Shared PTY core per workspace | One detached `unpeel-host __pty_core__` runs every session as an event-driven task (kqueue/epoll reactor, timer thread, journal writer) | docs/agents/pty-core.md; crates/unpeel-core/src/pty_core.rs, core_reactor.rs, session_io.rs |
| In-place core upgrade (no terminal restart) | A new build takes over running PTYs over SCM_RIGHTS (`__pty_core__ --takeover`) | README.md; crates/unpeel-core/src/fd_pass.rs; crates/unpeel-serve/src/pty_core_supervisor.rs |
| Agent state awareness | Busy / idle / needs-you per session, from provider hooks, runtime detection and a viewport scanner | README.md; docs/agents/clients/session-activity.md |
| Notifications to the device you hold | Mac banner when the desktop isn't showing the session; phone push when no phone is viewing it | clients/native/.../DesktopNotifier.swift; crates/unpeel-serve/src/notifications.rs |
| Resume after crash/upgrade | Re-runs the agent with its own conversation id | docs/agents/session-model.md (Resume recipes); crates/unpeel-core/src/resume.rs |
| Built-in `unpeel` MCP server | Every session gets sessions/agents/workspace/artifacts/browser/apps/skills tools | README.md; docs/agents/sessions-mcp.md; crates/unpeel-core/src/mcp_host.rs |
| Agents talk to each other | List siblings, read their screens and transcripts, wait for idle, send text. The first write asks and approved pairs are remembered | README.md; docs/agents/sessions-mcp.md |
| Real isolated browser per session | Host-installed, pinned `agent-browser` engine drives system Chrome; screenshots become gallery artifacts | docs/agents/browser-mcp.md |
| Any agent, any task | Works with 13+ named CLIs or any terminal program; it is "a terminal, not a code editor" | README.md; runtimes/ |
| Steer from anywhere on your own hardware | One-time pairing code; direct on LAN/VPN, Unpeel Link E2E relay away from home | README.md; docs/agents/clients/remote-control.md |
| Plain files, one protocol | Each session is a directory under `~/.unpeel/app-sessions/<id>/` (manifest.json, output.bin, session.sock) | README.md; docs/agents/session-model.md |
| Lean memory footprint | About 0.12 MiB per empty session and 0.39 MiB per 10k-line session, tracked in CI | README.md; scripts/bench-memory.sh; scripts/bench-thresholds.json |
| Headless Linux Host | `unpeel serve` on Linux looks identical to a Mac Host from phone and Mac | README.md; docs/agents/serve.md |
| One-line install | `curl -fsSL https://unpeel.com/install.sh | sh`, SHA-256-verified; installs unpeel, unpeel-host and unpeel-attach | README.md; scripts/install.sh; docs/agents/releases.md |
| Supported platforms | macOS (Apple silicon and Intel), Linux x86_64 and aarch64 (Ubuntu 20.04 / Debian 11+) | README.md; scripts/build-cli-linux.sh |
| Open-source boundary | MIT server and app sources. Only the Link backend (accounts, seats, relay, push) is closed; local and direct use needs no Link | README.md; LICENSE; TRADEMARK.md |
| Workspaces (isolated homes) | Separate `UNPEEL_HOME`s with their own sessions, projects, presets, settings and pairing identity | docs/agents/clients/workspaces.md |
| Git worktrees | Run sessions in isolated worktrees under `~/.unpeel/worktrees/…`; agent-created worktrees can be discovered | docs/agents/clients/worktrees.md |
| Pane layouts | Recursive split tree with up to 8 sessions per group, zoom, equalize and spatial focus | docs/agents/clients/panes.md; protocol/pane-layout-operations-v1.json |
| Viewer presence | Shows which devices are viewing a session; "Fit to desktop" restores the shared PTY grid | docs/agents/clients/presence.md |
| Transcripts API | Normalized provider transcripts (snapshot/stream/history/markdown) for MCP and remote clients | docs/agents/clients/transcripts.md; crates/unpeel-core/src/transcripts/mod.rs |

---

## 2. CLI (`crates/unpeel-cli`)

Dispatcher: `crates/unpeel-cli/src/cli.rs` (USAGE at line 27, dispatch ~line 881). Most verbs take `--json` and return meaningful exit codes. A bare `unpeel` prints a hint because there is no TUI.

| Feature | What it does (user-facing) | Source path(s) |
|---|---|---|
| `unpeel serve` | Runs the UI-free Host service for the default and registered workspaces until SIGINT/SIGTERM | cli.rs; crates/unpeel-serve/src/service.rs |
| `unpeel serve install [--graphical]` | Writes, enables and starts the per-user launchd LaunchAgent or systemd `--user` unit. `--graphical` (Linux) binds it to graphical-session.target | cli.rs (SERVE_USAGE); crates/unpeel-serve/src/service_install.rs; packaging/service/* |
| `unpeel serve uninstall` | Stops the service and removes only the unit file; sessions and data are untouched | same |
| `unpeel serve status` | Shows unit and live service state (exit 0 only while running); on Linux also the unit variant, target state and desktop session | same; crates/unpeel-core/src/desktop_session.rs |
| `unpeel --workspace NAME <cmd>` | Runs any command against an isolated workspace home | cli.rs (`claim_workspace_flag`); crates/unpeel-cli/src/workspaces.rs |
| `unpeel pair [--advertise-host H] [--advertise-port P]` | Shows a one-time code/QR to pair a Controller; starts serve in the background if needed | cli.rs (PAIR_USAGE) |
| `unpeel pair list [--json]` | Lists paired devices | cli.rs |
| `unpeel pair remove <device>` | Revokes a paired device | cli.rs |
| `unpeel pair relay <device> on|off` | Allows or denies Unpeel Link (off-LAN) for a device | cli.rs |
| `unpeel ls|list [--json]` | Lists sessions with status, project and command | cli.rs |
| `unpeel new [--command C | --preset L] [--cwd D] [--project ID] [--cols N] [--rows N] [--json]` | Creates a session (a plain terminal if no command) and prints its id | cli.rs (NEW_USAGE) |
| `unpeel add [PATH] [--name N] [--here] [--json]` | Adds a folder (default: cwd) as a project | cli.rs (`add_here`) |
| `unpeel send <id> <text…> [--enter]` | Sends text into a session. From inside a session it goes through MCP `send_text` with the approval policy | cli.rs; docs/agents/cli.md |
| `unpeel keys <id> <sequence>` | Sends raw bytes (`\r`, `\t`, `\e`); key names inside a session | cli.rs |
| `unpeel screen <id> [--cols N] [--rows N]` | Prints the parsed screen snapshot | cli.rs |
| `unpeel logs|tail <id> [--lines N] [--follow]` | Prints or follows session output | cli.rs |
| `unpeel wait <id> [--idle] [--text S] [--timeout SECONDS]` | Blocks until idle or until text appears; exit 1 on timeout | cli.rs |
| `unpeel resume|restart <id>` | Resumes a returned agent in place, or resumes a stopped terminal | cli.rs |
| `unpeel stop <id>` | Stops a session | cli.rs |
| `unpeel archive <id>` / `restore <id>` | Archives or restores a session (non-destructive) | cli.rs |
| `unpeel rm|remove|close <id>` | Removes a session | cli.rs |
| `unpeel transcript <id> [--entries N] [--markdown]` | Prints the normalized provider transcript | cli.rs |
| `unpeel open <path|resource> [--with APP] [--kind KIND] [--media-type T] [--json]` | Opens a file, folder or `git:working-tree` in the resolved App, creating or reusing a companion pane | crates/unpeel-cli/src/open_cli.rs; docs/agents/cli.md |
| `unpeel settings list|get|set [--json]` | Allowlisted workspace settings: `experimental_features.sessions_mcp`, `.browser_mcp`, `browser_default_access` (on/ask/off), `mcp_nonchild_write_access` (ask/allow/deny), `theme` | crates/unpeel-cli/src/settings_cli.rs; docs/agents/cli.md |
| `unpeel settings openers set <selector> <editor|system|app:id>` | Sets the opener preference per file media type or resource kind | docs/agents/cli.md; settings_cli.rs |
| `unpeel apps list [--json]` | Lists the official App catalog and what is installed | crates/unpeel-cli/src/apps_cli.rs |
| `unpeel apps install <id> [--check] [--yes]` | Installs an official App on the Host, SHA-256-verified; `--check` exits 3 if missing | apps_cli.rs; crates/unpeel-core/src/app_installer.rs |
| `unpeel apps update [<id>] [--check] [--yes]` | Reinstalls Apps that are behind the registry version | apps_cli.rs |
| `unpeel apps link <id> <exe>` / `unlink <id>` | Dev mode: symlinks a local build into the App slot | apps_cli.rs |
| `unpeel apps describe|search|context` | MCP `apps` actions from the shell | crates/unpeel-cli/src/mcp_cli.rs |
| `unpeel integrations [list] [--json]` | Shows per-agent integration status (installed / refreshing / not installed / detection only) | crates/unpeel-cli/src/integrations_cli.rs |
| `unpeel integrations install <runtime> [--project DIR]` / `--all` | Installs an agent's lifecycle hooks and MCP shim into its own config | integrations_cli.rs; crates/unpeel-core/src/integrations/install.rs |
| `unpeel mcp [<tool> [<action> key=value…] [--json '{}']]` | Every MCP action from the shell, with the same identity, grants and approvals | crates/unpeel-cli/src/mcp_cli.rs |
| `unpeel browser open|snapshot|click|fill|type|press|get|screenshot [--full] [--annotate]|scroll|wait` | Drives the session browser from the shell | crates/unpeel-cli/src/browser_cli.rs; docs/agents/cli.md |
| `unpeel browser install [--check] [--json]` | Installs or checks the pinned agent-browser engine. Exit codes 0/1/3/4 (4 = no Chrome found) | browser_cli.rs; crates/unpeel-core/src/browser_engine.rs |
| `unpeel artifacts publish <image>` | Adds an image to the session gallery | mcp_cli.rs |
| `unpeel current` | Shows who you are and your pane neighbours | mcp_cli.rs |
| `unpeel report <summary> [--status update|done|blocked] [--details T]` | Reports status (MCP `sessions.report`) | mcp_cli.rs |
| `unpeel worktree create <name> [--branch B] [--base REF] [--project ID]` | Creates a git worktree child project | mcp_cli.rs; crates/unpeel-core/src/worktrees.rs |
| `unpeel agents <action> [k=v…]` | Occupants, transcripts, wait | mcp_cli.rs |
| `unpeel skills <action> [k=v…]` | Skills list/search/get | mcp_cli.rs |
| `unpeel presets [list | add <label> <cmd> | remove <label>]` | Manages presets in `app-state.json` | crates/unpeel-cli/src/state_cli.rs |
| `unpeel presets star|unstar|enable|disable <label|id>` | Sets the quick-launch star and the enabled flag | state_cli.rs; docs/agents/cli.md |
| `unpeel presets edit <label|id> [--label L] [--command C]` | Edits a preset | state_cli.rs |
| `unpeel presets reorder <label|id> <position>` | 1-based reorder; the first preset per CLI is that CLI's default | state_cli.rs |
| `unpeel link enroll <key> [--json]` | Activates Unpeel Link on this Host. Exit codes 0 / 1 definitive / 2 transient | crates/unpeel-cli/src/link_cli.rs; crates/unpeel-core/src/license.rs |
| `unpeel link status [--json]` | Read-only enrollment and entitlement state | link_cli.rs |
| `unpeel link deactivate` | Stops Link on this machine | link_cli.rs |
| `unpeel workspaces [list | add <name> | remove <name>]` | Manages the workspace registry (`~/.unpeel/profiles.json`) | crates/unpeel-cli/src/workspaces.rs |
| `unpeel projects [list | add <name> <path> | remove <name|path>]` | Manages projects | cli.rs (`projects`) |
| `unpeel hosts prune [--json]` | Reaps orphaned per-session host processes (pid + start-time verified) | cli.rs (HOSTS_USAGE); crates/unpeel-core/src/session_host.rs |
| `unpeel computer …` (retired) | Compatibility stub that prints the retirement reason and exits 1 | crates/unpeel-cli/src/computer_cli.rs |
| `unpeel help` / `--version` | Usage and version | cli.rs |
| `unpeel profiles` (renamed) | Errors with "use `unpeel workspaces`" | cli.rs |
| Safe shared-state writes | Every mutation uses a flocked raw-JSON read-modify-write that preserves unknown keys, then a state-bus flush | docs/agents/cli.md; crates/unpeel-core/src/app_state.rs, state_bus.rs |
| Write-policy-aware shell verbs | `send`/`keys` from inside a session need the same approval as MCP; outside a session, the operator is the user | docs/agents/cli.md |
| Interactive vs noninteractive install prompts | `apps install` prompts on a TTY; automation must pass `--yes` or it fails closed | docs/agents/cli.md |
| `key=value` JSON coercion | MCP verb args parse as JSON when valid, otherwise as strings | docs/agents/cli.md |
| Real-PTY test matrix | ~40 Python PTY cases (approvals, pairing, link, relay, pty-core handoff, …) | crates/unpeel-cli/tests/cases/*.py; tests/run.sh |

---

## 3. Host / daemon / serve / core (`crates/unpeel-host`, `crates/unpeel-serve`, `crates/unpeel-core`)

### 3a. Process model and `unpeel-host` argv modes

| Feature | What it does | Source path(s) |
|---|---|---|
| Machine supervisor + workspace workers | One Host service per user supervises one worker per `UNPEEL_HOME` (discovery, restart, shutdown) | crates/unpeel-serve/src/service.rs, driver.rs; docs/agents/serve.md |
| Machine lease + status files | Per-user lease; `serve.json` publishes localSocket, browserEngine, platformCapabilities, etc. | docs/agents/serve.md (Leases and status files) |
| `__serve__` / `__serve_workspace__` | Native/service entry and private worker child | crates/unpeel-host/src/main.rs; docs/agents/serve.md |
| `__session_host__` | One PTY per session process (legacy path; falls back when the core is unavailable) | crates/unpeel-core/src/session_host.rs |
| `__pty_core__ [--takeover]` | Shared PTY core with flock, `pty-core.sock` and handoff | crates/unpeel-core/src/pty_core.rs |
| `__mcp__` / `__mcp_gate__ unified` | Stdio MCP sidecar per agent client; the gate serves no tools outside a hosted session | crates/unpeel-core/src/mcp_host.rs, mcp_gate.rs |
| `__browser_mcp__` / `__browser_cleanup__` | Legacy standalone browser MCP; closes the session's tab/daemon | crates/unpeel-core/src/browser_mcp.rs |
| `__remote__` | TLS/WSS terminal streamer for paired Controllers | crates/unpeel-core/src/remote_server.rs; crates/unpeel-serve/src/remote_streamer.rs |
| `__remote_stdio__` | SSH-friendly framed Host contract (`ssh -T host unpeel-host __remote_stdio__`) | crates/unpeel-core/src/remote_stdio.rs |
| `__remote_attach__` | Bridges a terminal to a session on another Unpeel's `__remote__` (gated by `UNPEEL_REMOTE_ATTACH=1`) | crates/unpeel-core/src/remote_attach.rs |
| `__transcript__ snapshot|stream|history|markdown` | Provider transcript reads | crates/unpeel-core/src/transcripts/mod.rs; docs/agents/clients/transcripts.md |
| `__viewport__` | Parsed-screen snapshot of a session | crates/unpeel-core/src/terminal_viewport.rs |
| `__auto_title__ <id>` | Titles a session from its resumed conversation | docs/agents/session-model.md |
| `__resume__`, `__resume_agent__`, `__restart_agent__` | Resume/restart recipes | crates/unpeel-host/src/main.rs; crates/unpeel-core/src/resume.rs |
| `__request_screenshot__` | Prompts the agent to take a browser screenshot into the gallery | crates/unpeel-host/src/main.rs; docs/agents/clients/remote-control.md |
| `__check_local_url__`, `__local_site_server__`, `__stop_local_site_server__` | Re-verify detected localhost URLs; find or stop a session-owned server | crates/unpeel-host/src/main.rs; crates/unpeel-core/src/local_urls.rs |
| `__apps__ list` | Lists catalog Apps whose binary is on PATH | crates/unpeel-host/src/main.rs |
| `__metrics__` | One-shot session grid metrics JSON | crates/unpeel-host/src/main.rs |
| `__managed_storage__`, `__kiro_mcp__`, `__computer_cleanup__`, `__relay_probe__` | Pi managed storage path, Kiro MCP compat, retired computer cleanup, relay latency probe | crates/unpeel-host/src/main.rs; crates/unpeel-core/src/relay_probe.rs |

### 3b. Sessions, terminal and state

| Feature | What it does | Source path(s) |
|---|---|---|
| On-disk session contract | `manifest.json` (identity, state, pid + start time), `output.bin` journal, `session.sock` (write/resize/ping/kill/snapshot) | README.md; docs/agents/session-model.md |
| Bounded append-only journal | Monotonic lifetime offsets; hole-punches old blocks and keeps a ~64–72 MiB readable suffix | docs/agents/clients/terminal.md |
| Exact VT snapshot attach | The Host renders its resident libghostty-vt grid (cells, styles, scrollback, modes) for attaching clients | docs/agents/clients/terminal.md; crates/unpeel-core/src/terminal_viewport.rs, ghostty_vt.rs |
| PID identity guard | Never signals a recycled pid (start-time verification) | docs/agents/session-model.md |
| Session ownership metadata | `owner_principal_id`, `created_by_device_id`, `source_preset_id`, tag id | docs/agents/session-model.md; crates/unpeel-core/src/state.rs |
| Auto-titling | Modes: agent OSC titles (semantic-title runtimes), first prompt, or off; slash commands skipped; user rename always wins | docs/agents/session-model.md |
| Session lifecycle ops | Spawn/stop/remove/restart-with-resume against the on-disk contract | crates/unpeel-core/src/session_ops.rs |
| Archive / restore | Stop and keep the session directory; Restore & Resume brings back the conversation | docs/agents/session-model.md (Archiving) |
| Auto-stop-and-archive sweep | Idle resumable sessions archived after 30/60/120/240/480/1440 min; never unread-attention or pinned sessions | crates/unpeel-serve/src/auto_archive.rs; docs/agents/serve.md |
| Resume on restart | Hook-captured provider session id → exact resume command | docs/agents/session-model.md; runtimes/*/adapter/resume.rs |
| Restart recommendation | Recommends restart when a live host's `host_protocol_version` is too old | docs/agents/session-model.md |
| Session reload | Replaces a session's host in place while keeping its id (Reload Terminal / Restart App) | protocol/host-capabilities-v1.json (minor 16) |
| Safe text delivery | Bracketed paste + settle + double-Enter recipe shared by MCP, remote input and typed commands | crates/unpeel-core/src/session_input.rs |
| Shared recent ordering and sort | Manual order file, "Recently updated" sort, pins, archive preview | docs/agents/session-model.md |
| State bus | Cross-frontend change notification for app-state, order and pane files | crates/unpeel-core/src/state_bus.rs |
| First-run seeding | Seeds presets for installed agent CLIs found on PATH | crates/unpeel-core/src/first_run.rs |
| Projects and groups | Projects, plain groups, worktree child projects, cwd buckets | crates/unpeel-core/src/state.rs; docs/agents/clients/worktrees.md |
| Worktrees (Rust) | Create/list worktrees; default base is the mainline, not HEAD | crates/unpeel-core/src/worktrees.rs |
| Local URL detection | Detects `http://localhost:<port>` servers a session exposes | crates/unpeel-core/src/local_urls.rs |
| Activity log | Append-only `activity-log.jsonl`: started, needed input, finished, exited, App alert | crates/unpeel-core/src/activity_log.rs |
| Host filesystem ops | Directory list/create, file read and upload scoped to project roots | crates/unpeel-core/src/host_resources.rs |
| Session artifacts (gallery storage) | List, read, resumable upload and delete of per-session images | crates/unpeel-core/src/session_artifacts.rs |
| Trace log | Timestamped `~/.unpeel/hooks/trace.log` for all components | crates/unpeel-serve/src/tracelog.rs |
| Orphan reaping | Reaps leftover session hosts at startup, on a timer, and via `hosts prune` | crates/unpeel-core/src/session_host.rs |

### 3c. Activity, hooks, notifications, approvals

| Feature | What it does | Source path(s) |
|---|---|---|
| Hook listener | Provider hook scripts POST lifecycle events to the ports in `~/.unpeel/app-ports` | crates/unpeel-serve/src/hook_listener.rs |
| Activity engine | Hook-latched busy/idle/attention with a 5-minute output-rearmed timeout; output growth never counts as work | crates/unpeel-serve/src/activity.rs; docs/agents/clients/session-activity.md |
| Screen-tier fallback | Busy/idle from the viewport's bottom line for Claude/Codex/Gemini when no hook has latched; never sends a completion notification | crates/unpeel-core/src/screen_activity.rs; runtimes/*/runtime.toml `[screen]` |
| Runtime observation | Detects an agent started by hand in a shell (foreground job) for identity, icon and tint | crates/unpeel-core/src/runtime_observer.rs |
| Menu-prompt detection | Detects agent-drawn "pick an option" menus (no hook fires) → attention | crates/unpeel-core/src/menu_prompt.rs; runtimes/claude-code/fixtures/approval-menu.txt |
| Escape cancellation fencing | Fences activity from an interrupted turn | crates/unpeel-core/src/hook_cancellation.rs; docs/agents/clients/session-activity.md |
| Background (subagent) tracking | The session stays busy while tracked children (Claude SubagentStart/Stop) run | docs/agents/clients/session-activity.md (Background agents) |
| Lifecycle notifications | Needs-input, opted-in "finished", App alerts; suppressed on the exact viewing device | crates/unpeel-serve/src/notifications.rs |
| Notify when done (per session) | Per-session opt-in for completion notifications | protocol/host-capabilities-v1.json (`session.notify_when_done.set`) |
| Mark read / unread | Unread badges; mark-read verb | protocol (`session.mark_read`) |
| Approval hub | Shared FIFO queue for write/browser/app-open approvals; first answer from any Controller wins | crates/unpeel-serve/src/approvals.rs; docs/agents/sessions-mcp.md |
| Viewer presence | Direct/Link output leases → `mobile-presence.json` and `presence.json` | crates/unpeel-serve/src/presence.rs; docs/agents/clients/presence.md |
| Phone fit / desktop fit | A phone can resize the shared PTY grid; "fit to desktop" restores it | protocol (`session.resize`, `session.resize_desktop`); docs/agents/clients/presence.md |

### 3d. MCP server domains (`unpeel`)

| Feature | What it does | Source path(s) |
|---|---|---|
| `sessions` domain | current, list, inspect, read_screen, read_output, wait_for_text, send_text, send_keys, report | docs/agents/sessions-mcp.md; crates/unpeel-core/src/mcp_host.rs |
| `agents` domain | list, get, read_transcript, wait, using occurrence-bound `agent_ref` | same |
| `workspace` domain | list_presets, create_worktree, list_worktrees | same |
| `artifacts` domain | add_to_gallery | same |
| `browser` domain | open, snapshot, click, fill, type, press, get, screenshot, wait, scroll, console, close, context (13 actions) | crates/unpeel-core/src/browser_mcp.rs; docs/agents/browser-mcp.md |
| `apps` domain | list, catalog, describe, search, context, open (agents may open App panes without approval) | crates/unpeel-core/src/apps_mcp.rs, app_open.rs |
| `skills` domain | list, search, get (progressive-disclosure docs from capabilities and Apps) | crates/unpeel-core/src/skills_mcp.rs |
| Cooperative write policy | Reads are open; writes to another session follow ask/allow/deny with remembered directional pairs; a session can never write into itself | docs/agents/sessions-mcp.md; crates/unpeel-core/src/state.rs |
| Sessions are user-owned | Agents can't create or close sessions (only App companion panes) | docs/agents/sessions-mcp.md |
| Caller identity | `UNPEEL_SESSION_ID`, or a start-time-verified process-ancestry fallback | docs/agents/sessions-mcp.md |
| Lazy help + terse schemas | `{"action":"help"}` returns full docs; ~1.5k-token schema budget test | docs/agents/sessions-mcp.md |
| Dual-era MCP transport | Legacy initialize plus 2026-07-28 `server/discover`; `-32022` on unsupported versions | docs/agents/sessions-mcp.md |
| In-flight cancellation | `notifications/cancelled` skips queued calls and unwinds the running one in ~250 ms | crates/unpeel-core/src/mcp_cancel.rs |
| `/mcp/*` auth token | `x-unpeel-auth` header against `<home>/mcp/auth-token` (0600) | crates/unpeel-core/src/mcp_auth.rs |
| App live context | Apps publish `app-context.json`, surfaced to neighbouring agents as `app_context` | docs/agents/sessions-mcp.md; crates/unpeel-core/src/pane_context.rs |
| Reference tokens | `[mcp:unpeel.app.markdown README.md LOC:12:32]`, resolved through `skills.get` | docs/agents/sessions-mcp.md |

### 3e. Browser engine

| Feature | What it does | Source path(s) |
|---|---|---|
| Pinned engine install | agent-browser 0.34.0 per platform, sha256-verified, streamed, flock-serialized | crates/unpeel-core/src/browser_engine.rs; protocol/browser-engine-v1.json |
| Shared project window | One Chrome per project tree with a pinned tab per session; logins persist in the project profile | docs/agents/browser-mcp.md |
| Separate per-session mode | Ephemeral browser per session | docs/agents/browser-mcp.md |
| Access modes | On (default) / Ask / Off; Ask approvals remembered per session | docs/agents/browser-mcp.md |
| Options | Headed/headless, site allowlist with wildcards, custom Chromium path, show agent cursor overlay, theme passthrough | docs/agents/browser-mcp.md |
| Remote CDP mode | `~/.unpeel/browser/remote-cdp.json` (wss or loopback port) for container providers such as Upstash | docs/agents/browser-mcp.md |
| Screenshots/downloads as artifacts | Auto-gallery toggle; captures vs gallery directories; downloads directory | docs/agents/browser-mcp.md |

### 3f. Remote access, pairing, Link

| Feature | What it does | Source path(s) |
|---|---|---|
| Pairing (one-time code/QR) | 5-minute single-use code exchanged for a per-device bearer token and E2E key | crates/unpeel-serve/src/pairing.rs; clients/shared/.../RemotePairingClient.swift |
| Pairing control route | Local `POST /_unpeel/pairing`: begin/status/cancel/devices/revoke-device/set-relay-allowed | docs/agents/serve.md |
| Controller-assisted pairing | A paired Mac shows a QR plus a one-shot proxy so a phone can pair with a remote Host (`pairing.invitation`) | docs/agents/clients/controller-assisted-pairing.md; clients/native/.../ControllerPairingProxy.swift |
| Direct `/mobile` over pinned TLS | HTTP/1.1 over TLS with the Host certificate on the LAN | crates/unpeel-serve/src/mobile.rs; docs/agents/serve.md |
| WSS terminal streamer | Binary frames with an offset prefix; supervised with backoff and a crash-loop ceiling | crates/unpeel-serve/src/remote_streamer.rs; crates/unpeel-core/src/remote_server.rs |
| Local `host.sock` framed contract | UPL1 framed Host contract for the Mac app and CLI (mode 0600) | crates/unpeel-serve/src/local_gateway.rs |
| SSH stdio Host | Accountless remote Host over system SSH (plus an interactive-shell compat mode) | crates/unpeel-core/src/remote_stdio.rs, ssh_connection.rs |
| Unpeel Link relay uplink | Outbound WSS to the relay; forward-secret E2E; relay sees only ciphertext | crates/unpeel-serve/src/relay.rs; crates/unpeel-core/src/relay_crypto.rs, relay_uplink.rs |
| Token rotation without eviction | Re-announces rotated device tokens in place | docs/agents/serve.md |
| Relay credential recovery | `relay.credentials.recover` rotates a device's relay token and E2E key | protocol; docs/agents/serve.md |
| Direct-path upgrade (NAT punch) | Candidate negotiation over the sealed tunnel plus HKDF-keyed UDP probes | crates/unpeel-core/src/direct_path*.rs; crates/unpeel-serve/src/direct_path.rs; protocol/direct-path-v1.json |
| Link license | `CLRTY-…` Ed25519 keys; activate/deactivate/entitlement; seats | crates/unpeel-core/src/license.rs |
| Push registration | APNs tokens registered via `/mobile/push-token`; the relay holds the APNs key | protocol (`push.register`) |
| Platform adapter seam | Native app registers callbacks (notify, push, approvals, overlay, thumbnails, Link refresh, open-in-editor) over host.sock | crates/unpeel-serve/src/platform_adapter.rs; docs/agents/serve.md |
| Host-owned App install/open/openers | Install, open and opener policy live on the Host, so they work on SSH/Linux Hosts | crates/unpeel-core/src/app_installer.rs, app_open.rs, app_resources.rs |
| Plugins (activation + update checks) | Workspace activation of agents/Apps, ordering, lazy update checks | crates/unpeel-core/src/plugins.rs, plugin_updates.rs |
| Desktop-session service | Graphical-session unit and diagnostics (display/session bus) | crates/unpeel-core/src/desktop_session.rs; packaging/service/unpeel-desktop-session.target |
| Computer use (retired) | Legacy fields stay false; the domain cannot be reactivated | crates/unpeel-serve/src/computer.rs |

---

## 4. Attach, Apps crate, bundled Apps, App Kit

| Feature | What it does | Source path(s) |
|---|---|---|
| `unpeel-attach <id>` | tmux-style attach: raw mode, snapshot or journal-tail replay, then stdio ↔ session.sock | crates/unpeel-attach/src/main.rs, lib.rs |
| Event-driven follow | kqueue/inotify follow with ~0 idle CPU | crates/unpeel-attach/src/main.rs |
| Focus-report filtering | Drops focus reports unless the workload enabled DEC 1004 (`--forward-focus-events`) | same |
| Resize forwarding / exit on host death | Forwards SIGWINCH as resize; exits when the host dies | same |
| `unpeel-apps` agent API | Detects the adjacent agent pane and pastes a reference into it (clipboard fallback) via the unified MCP | crates/unpeel-apps/src/lib.rs, agent.rs |
| App registry | Official Apps: diffs (Git), filetree (Files), markdown, usage, with media types, resource kinds and defaults | protocol/app-registry.json |
| App runtime detection | Recognizes a running App binary like an agent runtime (row gets the App name and accent) | crates/unpeel-core/src/app_runtime.rs |
| App presentations | Host-owned App instances and bindings with reveal revisions → Controller opens a trailing split | crates/unpeel-core/src/app_presentations.rs |
| App context resolution | `AppContext::current_root()`: an App follows the neighbouring agent's project/worktree | crates/unpeel-serve/src/app_context.rs; crates/apps/app-kit/src/context.rs |
| **Git (unpeel-diffs)**: Changes tab | Staged/unstaged/conflicted/untracked files with status glyphs; unified diff; refresh ~1 s | crates/apps/diffs/README.md |
| Git: syntax highlighting | two-face grammars (Swift, Rust, JS/TS, Python, JSON, shell); size limits | crates/apps/diffs/README.md |
| Git: History tab | Commits newest-first, 100 at a time, "load older"; commit → files → patch | crates/apps/diffs/README.md |
| Git: Fetch/Pull/Push | Top-right control (Pull ↓N / Push ↑N), fast-forward-only pull, no force push | crates/apps/diffs/README.md |
| Git: line selection → Send to agent | Select patch lines; Copy lines / Send `commit:path:line` reference to the agent | crates/apps/diffs/README.md |
| Git: keyboard nav | 1/2, Tab, j/k, g/G, h/l pan, F10 remote menu, r refresh, n older, q quit | crates/apps/diffs/README.md |
| **Files (unpeel-filetree)**: scoped explorer | Flat folder list with `../`, cannot escape the root; filter-as-you-type; `--ext` filter | crates/apps/filetree/README.md |
| Files: context menu | Open in editor / Send to agent / Copy path | crates/apps/filetree/README.md |
| Files: native path dragging | Drag rows into another terminal pane as a shell-quoted bracketed paste | crates/apps/filetree/README.md; clients/native/.../TerminalPathDragMap.swift |
| Files: follows agent worktree | Rebinds its root when the neighbouring agent moves between checkouts | crates/apps/filetree/README.md |
| **Markdown (unpeel-markdown)**: live editor | Live block styling, headings picker, full mouse support | crates/apps/markdown/README.md |
| Markdown: slash commands | `/` menu for headings/lists/to-dos/quotes/code/dividers; `[] ` for a to-do; `\` command palette | crates/apps/markdown/README.md |
| Markdown: auto-save + Ctrl+S/O/N | Auto-save toggle (persisted), Open picker, New note | crates/apps/markdown/README.md |
| Markdown: vault mode | Searchable scoped Markdown Explorer; remembered notes folder (`start.json`) | crates/apps/markdown/README.md |
| Markdown: live context for agents | Publishes current file, cursor line and selection lines | docs/agents/sessions-mcp.md; crates/apps/markdown/README.md |
| **Usage (unpeel-usage)**: provider quotas | Codex, Claude (5-hour / 7-day / model weekly), Grok and Muse usage from local logins/logs | crates/apps/usage/README.md |
| Usage: token history | Current project row, Total usage, monthly table for the last 12 months, usage by project | crates/apps/usage/README.md |
| Usage: gauges/sparklines + alerts | Detail view with Gauge/Sparkline; `a` alert, `r` refresh, `t` theme cycle | crates/apps/usage/README.md; protocol/app-registry.json |
| Usage: dynamic session title | Session title "Usage (N%)" | crates/apps/usage/README.md |
| **App Kit**: Ratatui component library | Page, List/ListItem, Input, TextBox, Tree/Explorer, Menu/PopupMenu, Gauge, Sparkline, Bar/Line charts, Media, Surface/Canvas, scrollbar, spinner, theme | crates/apps/app-kit/src/*.rs; docs/ui-components.md |
| App Kit: `page()`/`reduce()` App model | One struct, one page builder, one reducer, `run_app` | crates/apps/app-kit/docs/writing-an-app.md |
| App Kit: hosted UI bridge | Optional Host-injected endpoint; publishes a semantic tree with revisions and deltas | crates/apps/app-kit/src/ui_bridge.rs, ui_state.rs, ui_auth.rs; protocol/unpeel-ui-v1.schema.json |
| App Kit: participant tokens / grants | Scoped tokens; agents can be participants with `edit` grants | crates/apps/app-kit/protocol/unpeel-ui-participant-token-v1.schema.json |
| App Kit: SwiftUI renderer | `UnpeelAppKitUI` renders the same tree natively (Tree, List, MarkdownEditor, charts, Media, Canvas) | crates/apps/app-kit/swift/Sources/UnpeelAppKitUI/* |
| App Kit: web renderer | TypeScript/DOM/ARIA renderer of the same tree | crates/apps/app-kit/web/src/*.ts |
| App Kit: AppReporter status contract | Plain-file plus local HTTP status/title/alert reporting | crates/apps/markdown/README.md; crates/apps/app-kit/src/host.rs |
| App Kit: drag / drop target maps | Publish path-drag maps and drop rectangles for native drag-and-drop | crates/apps/app-kit/src/drag.rs, drop_target.rs; clients/native/.../TerminalDropTargetMap.swift |
| App Kit: Kitchen Sink mini-host | macOS test rig with Terminal/Native/Web/Split views, tree inspector and a "Walk every screen" audit | crates/apps/app-kit/swift/Examples/KitchenSink/* |
| App Kit examples | todo, charts, list_items, text_box, markdown, media, surface_canvas, surface_planets | crates/apps/app-kit/examples/*.rs |

---

## 5. Runtimes (`runtimes/`)

Common package shape: `runtime.toml` (identity, detection aliases, lifecycle fallback, `[screen]` rules, install command, integration summary, suggested presets, usage stores) plus optional `adapter/{setup,resume,transcript,tests}.rs` and `assets/{icon.svg,hooks/}`. The registry is generated by `crates/unpeel-core/build.rs`. Source: runtimes/README.md; docs/agents/providers.md.

| Runtime | Integration / capabilities | Source path(s) |
|---|---|---|
| Claude Code (`claude`) | Hooks in `~/.claude/settings.json`, MCP in `~/.claude.json`; screen fallback (`❯`); transcript; resume; notify_when_done; semantic terminal title; subagent tracking | runtimes/claude-code/* |
| Codex (`codex --dangerously-bypass-approvals-and-sandbox`) | Native hooks `~/.codex/hooks.json` + notify normalizer + `[mcp_servers.unpeel]`; screen fallback (`›`); transcript | runtimes/codex/* |
| Gemini CLI (`gemini --yolo`) | Hooks in `~/.gemini/settings.json`; screen fallback; transcript | runtimes/gemini/* |
| Cursor Agent (`cursor-agent --force`) | `~/.cursor/hooks.json` + `~/.cursor/mcp.json`; transcript | runtimes/cursor-agent/* |
| Grok (`grok --always-approve`) | `~/.grok/hooks/unpeel.json`; transcript | runtimes/grok/* |
| Kimi Code (`kimi --yolo`) | `[[hooks]]` in config.toml + `~/.kimi-code/mcp.json`; transcript (current + legacy) | runtimes/kimi/* |
| Kiro (`kiro-cli --v3`) | `~/.kiro/hooks/unpeel.json` (v3) + v2 compat agent + MCP settings; transcript | runtimes/kiro/* |
| Cline (`cline`) | Managed hooks under `~/.cline/hooks` + MCP settings; transcript | runtimes/cline/* |
| Amp (`amp`) | JS plugin reporter; per-project install (`--project DIR`) | runtimes/amp/* |
| OpenCode (`opencode`) | Notify plugin in `~/.config/opencode/plugin`; transcript adapter listed | runtimes/opencode/* |
| Muse Code (`muse --yolo`) | Plugin via `muse plugins install` (hooks + MCP); transcript | runtimes/muse-code/* |
| Antigravity (`agy --dangerously-skip-permissions`) | MCP in `~/.gemini/config/mcp_config.json`; resume | runtimes/antigravity/* |
| GitHub Copilot CLI (`copilot`) | Shared reporter; per-repository hooks | runtimes/github-copilot/* |
| fx (Vercel) (`fx`) | MCP in `~/.fx/mcp.json`; resume | runtimes/fx/* |
| Pi (`pi`) | Detection-only (nothing to install); resume via managed session dir | runtimes/pi/* |
| Any command | Plain shell / unknown CLI stays a terminal (no busy animation) | runtimes/README.md |
| Explicit integration install | Installed once per Host (CLI or Settings ▸ Agents); never at launch | runtimes/README.md; crates/unpeel-core/src/integrations/* |
| MCP shim | `~/.unpeel/bin/unpeel-mcp` → `__mcp_gate__ unified`, rewritten on upgrade | docs/agents/sessions-mcp.md |
| Post-upgrade refresh | The worker re-runs installed integrations after an upgrade | docs/agents/cli.md |
| Provider-neutral launch | A preset runs exactly as typed in the login shell; no command rewriting | runtimes/README.md |
| Install-command hints | Each runtime declares its CLI install command (npm/curl) for the Settings "Install" flow | runtimes/*/runtime.toml `[install]` |
| Usage stores | Runtime declares local usage/token stores (feeds the Usage App) | runtimes/*/runtime.toml `[[usage.stores]]` |

---

## 6. macOS desktop app (`clients/native/UnpeelNative`)

Swift + SwiftUI + AppKit embedding libghostty (GhosttyKit, Metal). The app is a Controller of the bundled `unpeel serve` plus a platform adapter. Paths below are under `clients/native/UnpeelNative/Sources/UnpeelNative/`.

### 6a. Window shell, sidebar, navigation

| Feature | What it does | Source path(s) |
|---|---|---|
| App shell | Resizable sidebar (220–520, default 300) plus content, with vibrancy materials and a custom 38px titlebar | Views/RootView.swift; Views/Chrome.swift; ../DESIGN.md |
| Sidebar toggle (⌘B) | Collapse/expand the sidebar; a "+ new session" button appears when collapsed | Views/RootView.swift |
| Project/session tree | Project rows, session rows, pinned section, worktree child folders, plain groups | Views/SidebarView.swift |
| Activity spinners/shimmer | Braille spinner and shimmer sweep for busy sessions; yellow attention dot | Views/SidebarSpinners.swift; Views/SidebarView.swift |
| Detached drag ("Dia feel") | Drag session/project rows as floating cards to reorder, move to a group, or drop on a pane edge to split | Views/SidebarSessionDrag.swift |
| Session context menu | Rename, Copy session ID, Copy transcript (20/50/whole), Notify when done, Clear attention, Resume Agent, Resume, Restart App, Reveal in Finder, Pin/Unpin, Stop and archive, Remove, Move to… | Views/SidebarView.swift; Views/TerminalPaneView.swift |
| Project/folder context menu | New worktree…, New group…, Rename, Remove group/worktree, Stop all, Sort sessions (Custom order / Recently updated), Folder color, Archived (n), Open in editor, Open local site, Move to workspace | Views/SidebarView.swift; UnpeelStore.swift (Project context menu actions) |
| Folder colors | 8-color native sidebar folder palette | Theme.swift (`ProjectFolderColor`); UnpeelStore.swift |
| Pins | Pin sessions and groups to the top of their group | UnpeelStore.swift (Pins) |
| "Show N more" / archive preview | Capped stopped/archived preview rows in the sidebar | Views/SidebarView.swift; Settings ▸ Advanced |
| Add Project / Add Workspace | Footer "+" to add a project folder or a workspace | Views/SidebarView.swift |
| Workspace dots + trackpad swipe | Dia-style workspace switcher dots with swipe between workspaces | Views/SidebarWorkspaceDots.swift |
| Workspace selector popover | Local, registry, paired and SSH hosts; Rename…, Open in New Window, Manage Workspaces… | Views/SidebarWorkspaceSelector.swift |
| Sidebar skeleton/connecting state | Blank sidebar with a spinner for a never-reached or reconnecting host | Views/SidebarSkeleton.swift |
| Project sidebar (right panel) | Right-side stack of sessions pinned to the "global project sidebar", with draggable dividers | Views/ProjectSidebarView.swift |
| Local site globe | Globe button on the project row opens the detected localhost site; menu lists URLs and "Stop server" | Views/LocalSiteMenu.swift |
| Titlebar "Open in" menu | Opens the project/worktree in VS Code, Cursor, Zed, IntelliJ, WebStorm, GitHub Desktop, Fork, Tower, Sourcetree, GitKraken, Sublime Merge, Finder, Terminal, iTerm2, Ghostty, Warp, WezTerm, kitty, Alacritty, Tabby, Hyper, Rio, Wave, Xcode | Views/WorkspaceOpenMenu.swift; WorkspaceOpenTarget.swift |
| Session launcher | Main-area "pick a tool" preset picker with filter ("No matching presets", Manage Agents…) | Views/SessionLauncherView.swift |
| Empty state | "Pick a session in the sidebar, or hit + on a project" | Views/TerminalArea.swift |
| Command palette (⌘K) | Jump to sessions across projects, projects, preset launches, New Terminal, Settings, Toggle Sidebar, All sessions | Views/CommandPaletteView.swift |
| ⌃Tab MRU switcher | Hold ⌃ and Tab to cycle recent sessions; release to switch; Esc cancels | Views/CommandPaletteView.swift; UnpeelStore.swift |
| ⌘1–9 session switching | Switch among the active project's sessions; holding ⌘ shows number hints | AppDelegate.swift; UnpeelStore.swift |
| ⌃1–9 project switching | Switch projects; holding ⌃ shows hints | UnpeelStore.swift; Views/SidebarView.swift |
| Recent activity page (⇧⌘R) | "All recent": live active sessions plus the persisted activity feed grouped by day | Views/RecentActivityView.swift; ActivityLog.swift |
| Titlebar activity bell/popover | Dropdown of recent activity with an "All recent" link | GlobalActivityMenu.swift; Views/RootView.swift |
| Archived sessions library | Per-project archive with search; Restore, Restore & Resume, Delete permanently | Views/ArchivedSessionsView.swift |
| Toasts | Transient capsule toasts (for example, a phone connected) | Views/ToastCenter.swift |

### 6b. Terminal and panes

| Feature | What it does | Source path(s) |
|---|---|---|
| libghostty Metal terminal | One Ghostty surface per visible session running `unpeel-attach`; surfaces are retained and swapped, never destroyed | GhosttyBridge.swift; SurfaceCache.swift; Views/TerminalArea.swift |
| Remote in-memory Ghostty panes | Remote/paired-host sessions render in in-memory Ghostty surfaces fed by the Host protocol | Views/RemoteHostWorkspaceView.swift; RemoteHostRuntime.swift |
| Split panes (⌘D / ⇧⌘D) | Split Pane Right / Down; recursive split tree, up to 8 leaves per group | AppDelegate.swift; PaneLayoutState.swift; PaneLayoutController.swift |
| Zoom pane (⇧⌘↩) | Temporarily maximize the active pane | AppDelegate.swift |
| Equalize splits | Direction-aware equalize | AppDelegate.swift; docs/agents/clients/panes.md |
| Spatial focus (⌥⌘←↑→↓) | Move focus to the neighbouring pane | AppDelegate.swift |
| Detach Pane / Exit Multi-Pane View | Pane header menu verbs (presentation only; sessions keep running) | Views/TerminalPaneView.swift |
| Pane header menu | Agents and Plugins launch sections, Manage…, Copy transcript, Pin, Resume, Restart App, Clear attention | Views/TerminalPaneView.swift |
| Transient launcher pane | New pane shows a launcher row to start a session directly in the group | Views/TerminalPaneView.swift; Views/ProjectSidebarView.swift |
| Pane layout persistence | `pane-layouts.json` per scope; v1 layouts migrated | PaneLayoutState.swift; protocol/pane-layout-operations-v1.json |
| Find bar (⌘F, ⌘G, ⇧⌘G) | Incremental search via libghostty with a match counter | TerminalFindBar.swift; AppDelegate.swift |
| Font size (⌘+ / ⌘= / ⌘- / ⌘0) | Increase/decrease/reset terminal font | AppDelegate.swift |
| URL / OSC 8 / OSC 7 handling | Hover and open links; live cwd tracking | docs/agents/clients/terminal.md; GhosttyBridge.swift |
| ⌘-click file paths | Opens bare paths (`src/x.tsx:42`, `#L12`) in the right App or editor | ClickablePath.swift |
| Path drag out of Apps | Drag file rows from hosted Apps as real AppKit file drags | TerminalPathDragMap.swift |
| Drop targets into Apps | Hosted Apps receive hover/drop events for files/folders | TerminalDropTargetMap.swift |
| Drop files into terminal | Shell-quoted bracketed-paste path insertion (relative to project when possible) | crates/apps/filetree/README.md; Views/TerminalPaneView.swift |
| Scroll to bottom | Button when scrolled up | Views/TerminalArea.swift |
| Exited session bar | "Session exited" with Resume / Start fresh; resume-failure notice | Views/TerminalArea.swift |
| Restart recommendation banner | Dismissible recommendation to restart an outdated host | Views/TerminalArea.swift; UnpeelStore.swift |
| Agent TUI background matching | Titlebar/padding follow OpenCode/Grok truecolor backgrounds | OpenCodeTheme.swift; ProviderThemeReadRequest.swift |
| Viewer avatars + Fit to desktop | Pane header chips for devices viewing the session; restore the desktop grid | Views/ViewerAvatarsView.swift; ViewerPresence.swift |
| In-pane approval overlay | Allow/Deny for session-write, browser and app-open grants, shown on the relevant session with an attention badge | MCPApprovalCenter.swift; MCPApprovalPanel.swift |
| Session gallery panel | Photo chip in the title bar → grid of the session's screenshots/downloads/uploads; Open, Reveal, Delete | Views/SessionGalleryPanel.swift; SessionArtifacts.swift |
| Gallery markup | Arrow and crop annotation, undo, colors; "Add to prompt" attaches the annotated copy | Views/SessionGalleryMarkup.swift |
| Take Screenshot (⇧⌘S) | System `screencapture` crosshair/window picker saves into the session's uploads and attaches to the prompt | Views/SessionScreenshotCapture.swift |
| Workspace-scoped terminals | Other local workspaces attach their own `session.sock` directly (no gateway bytes) | docs/agents/clients/workspaces.md |

### 6c. Menus, menu bar, system integration

| Feature | What it does | Source path(s) |
|---|---|---|
| App menu | About Unpeel, Check for Updates…, Settings… (⌘,), Services, Hide (⌘H), Hide Others (⌥⌘H), Show All, Quit (⌘Q) | AppDelegate.swift |
| Session menu | New Session (⌘N), New Terminal (⌘T), Split Right/Down, Zoom Pane, Equalize Splits, Focus Pane ×4, Collapse All Folders (⌥⌘B), Command Palette (⌘K), Take Screenshot… (⇧⌘S) | AppDelegate.swift |
| Edit menu | Undo/Redo/Cut/Copy/Paste/Select All, Find…, Find Next, Find Previous | AppDelegate.swift |
| View menu | Increase/Decrease/Reset Font Size | AppDelegate.swift |
| Window menu | Close Window (⌘W), Minimize (⌘M), Zoom, Bring All to Front | AppDelegate.swift |
| Help menu | Unpeel Help (docs link) | AppDelegate.swift |
| Menu-bar status item | NSStatusItem with an animated spinner or blocked tag, and a popover mirroring the activity dropdown | MenuBarController.swift; GlobalActivityMenu.swift |
| Menu-bar agent mode | Closing the window keeps the app (and PTYs) alive; Dock reopen restores the window | AppDelegate.swift (`showMainWindow`) |
| Finder service | "New Unpeel Session Here" (Services menu) opens the launcher for a folder | AppDelegate.swift; clients/native/build-app.sh (NSServices) |
| Sparkle auto-updates | SPUStandardUpdaterController, pinned feed, single-updater rule across workspaces | AppDelegate.swift; docs/agents/clients/workspaces.md |
| Beta channel | "Join the Beta" option | Views/SettingsView.swift |
| macOS notifications | Needs-input, opted-in completion and App alerts via Notification Center; test notification | DesktopNotifier.swift; Views/SettingsView.swift |
| Keychain licensing | Link license key stored in Keychain (`com.unpeel.license`) | Licensing/LicenseKeychain.swift, LicenseManager.swift |
| Host service management | Starts or ensures the bundled `unpeel serve` via launchd | HostServiceManager.swift; HostServiceAgent.swift |
| Platform adapter callbacks | Loopback listener for Host callbacks (notify, push, approvals, overlay, thumbnails, Link, open-in-editor) | HookServer.swift; NativeOverlaySnapshotAdapter.swift |
| Bonjour nearby hosts | Discovers nearby Hosts for Add Workspace (a hint, never authority) | NearbyHostBrowser.swift |
| Remote folder picker | Browse/create folders on a remote Host when launching | Views/RemoteFolderPicker.swift |
| Move project between workspaces | Moves a project and its sessions to another local workspace without restarting PTYs | ProjectWorkspaceMove.swift |
| Workspace windows | Open a local workspace in its own window/instance | WorkspacePool.swift; UnpeelWorkspaceRegistry.swift |
| Hardware identity | Advertises the Mac model family so remote clients show the right icon | HostHardware.swift |
| Dev builds | "Unpeel Dev" branding and burnt-orange icon, stable signing, blank-home mode | clients/native/dev-app.sh, dev-blank.sh; docs/agents/clients/dev-builds.md |

### 6d. Settings (Views/SettingsView.swift and panels)

Settings slide into the sidebar (Back row plus tab nav) while content swaps to the panel. There is a scope picker (This Mac / workspace / remote Host) for host-scoped tabs, and a "Feedback & bugs" link.

| Feature | What it does | Source path(s) |
|---|---|---|
| Settings scope picker | Host-scoped tabs follow the selected workspace/Host; per-workspace "Inherits from Default" with a "Use Default's …" reset | Views/SettingsView.swift (`hostScopedCases`) |
| Workspaces tab | One list of local, registry, paired and SSH workspaces; drag reorder, color, rename, Forget Workspace, Remove and delete data | Views/WorkspacesSettingsPanel.swift |
| Add Workspace sheet | New local workspace, Nearby host, pairing code paste, or SSH destination (optional password/API key) | Views/HostPickerView.swift |
| Agents tab | Host agent inventory: install CLI (Installation terminal), Install integration / Reinstall, what it edits, manual MCP command, launch commands and variants, Make Default, Activate, reorder | Views/PluginSettingsPanel.swift; PluginSettingsList.swift; Views/PluginListDrag.swift |
| Plugins tab | Unpeel Apps catalog: install/update, activate, reorder, default opener | Views/PluginSettingsPanel.swift |
| Agent access tab: Sessions | Explains open reads; write policy (ask/allow/deny); "Let sessions create worktrees"; auto-add browser screenshots to gallery; approved session pairs and approved Apps with Revoke | Views/AgentAccessSettingsPanel.swift; Views/SessionsAccessSections.swift |
| Agent access tab: Browser | Engine status (ready/installing/failed/disabled), access On/Ask/Off, approved browser sessions, Show browser window, Show agent cursor, Browser scope (shared project window vs separate per session), Browser app path, Site access rules, Clear project browser data | Views/BrowserAccessSections.swift |
| Appearance tab | Mode (System/Light/Dark), App color (Sky/Blue/Violet/Rose/Amber/Moss/Teal/Graphite), Background/Surface/Transparency sliders, Terminal font family and size, Line height (−20%…+100%), Session titles (agent / first prompt / off), Session gallery photo chip toggle | Views/SettingsView.swift (AppearanceSettingsPanel, TerminalFontSection); Theme.swift |
| Remote Control tab | Share This Mac/Workspace: Generate QR, one-time 5-minute pairing code, Copy Pairing Code, Refresh QR; CLI alternative; paired devices list with Revoke and a per-device "Reachable outside your network (Unpeel Link)" toggle; security details (bearer token, SHA-256 hash) | Views/SettingsView.swift (RemoteSettingsPanel) |
| Add iPhone or iPad to remote Host | Controller-assisted pairing QR for a selected remote Host | Views/HostPickerView.swift; ControllerPairingProxy.swift |
| Unpeel Link section | Paste key / activate, license owner (avatar), seats, Release seat, "Get Unpeel Link", inactive-key state | Views/LicenseSettingsPanel.swift; Views/SettingsView.swift (LinkEnrollmentSection) |
| iPhone beta notice | "Unpeel for iPhone is in beta" with a TestFlight join | Views/SettingsView.swift |
| Transcripts tab | Content toggles (user/assistant messages, reasoning, tool calls & results, file changes & diffs, commands run, plan updates), session info header, range (last 20/50/100/whole) | Views/SettingsView.swift (TranscriptsSettingsPanel) |
| Notifications tab | Attention: "Flag menus waiting for a choice"; completion banners; send test Mac/phone notification; Host delivery diagnostics (last Mac test, last phone push, paired phone tokens); Open Mac Notification Settings | Views/SettingsView.swift (NotificationsSettingsPanel) |
| Worktrees tab | "Show agent worktrees" toggle; list of worktree child projects with create / reveal / remove | Views/WorktreesSettingsPanel.swift |
| Features tab | Toggles: Remote workspaces, Git worktrees, Sessions use, Workspaces, Browser use (experimental) | FeatureFlags.swift; Views/SettingsView.swift (FeaturesSettingsPanel) |
| Advanced tab | Cleanup (auto-stop and archive after X; stopped/archived shown in sidebar), Diagnostics (app memory, running terminal hosts by CPU with Stop, hosted sessions on disk), Sessions folder and hooks trace log (Show in Finder) | Views/SettingsView.swift (AdvancedSettingsPanel) |
| Default editor / opener policy | Editor picker (Default Editor / System Default / App) per file type or resource | Views/SettingsView.swift; Views/PresetsSettingsPanel.swift |
| Remote-host settings panels | Remote appearance/notifications/features panels with "Update Unpeel on host" gating | Views/SettingsView.swift (Remote*SettingsPanel, HostSettingsUpdateRequiredPanel) |

### 6e. Presets, projects, remote workspaces (non-UI logic with user effect)

| Feature | What it does | Source path(s) |
|---|---|---|
| Presets = shared file | Reads and writes `app-state.json` presets (unknown keys preserved); FSEvents watch picks up CLI edits | PresetStateFile.swift; Presets.swift; docs/agents/clients/presets.md |
| Per-CLI default preset | The first preset for a CLI is its default; command variants | UnpeelStore.swift (Per-CLI default preset) |
| Remote Host scope = same UI | Paired/SSH hosts render through the same sidebar/content via a display projection | RemoteHostRuntime.swift; NativeRemoteBackend.swift; SelectedHostScope.swift |
| Link downlink / route selection | Native relay client with automatic Direct → Link fallback | NativeRelayBridge.swift; RelayUplinkManager.swift; docs/agents/clients/remote-control.md |
| Worktree git (native) | `git worktree` add/remove; default base = origin default branch; discovery every 5 s when enabled | WorktreeGit.swift; docs/agents/clients/worktrees.md |
| Startup presentation cache | Instant sidebar on launch from cached snapshot and activity seed | StartupPresentationCache.swift; SessionActivity.swift; Snapshot.swift |
| Pane pre-warming | Pre-warms likely-next surfaces | UnpeelStore.swift (Pane pre-warming) |

---

## 7. iOS / iPadOS app (`clients/ios/UnpeelIOS`)

A remote Controller only. The detail screen is always a live terminal, never a chat UI. Paths below are under `clients/ios/UnpeelIOS/Sources/UnpeelIOS/`.

| Feature | What it does | Source path(s) |
|---|---|---|
| QR pairing | Scan the workspace's QR (camera) or paste the compact code; exchanged for a device token via `/mobile/pair` | PairingView.swift |
| Multiple paired Macs/workspaces | Pair with several Hosts; each workspace appears as its own "Mac"; Add a Workspace; Forget | RemoteConnectionStore.swift; PairingView.swift |
| Keychain credential storage | Tokens and E2E keys in Keychain; legacy migration; hydration | RemoteConnectionStore.swift |
| Direct pinned-HTTPS transport | LAN `/mobile` with a pinned certificate; legacy plaintext fallback learned per Host | RemoteDirectTransport.swift; RemoteTerminalWebSocket.swift |
| Link relay transport | E2E-encrypted relay connection with credential repair and fallback retry | RemoteConnectionStore.swift; clients/shared/.../RemoteRelayConnection.swift |
| Sessions drawer (sidebar) | Mac-style project/session tree, workspace header with tint dot and kind badge, activity pills, relative ages | UnpeelIOSRootView.swift (SessionSidebarView, MacStyleProjectRow/SessionRow) |
| Workspace switcher | Switch among paired workspaces from the drawer header | UnpeelIOSRootView.swift (SidebarWorkspaceHeader); RemoteMacClient.swift (`selectWorkspace`) |
| Disconnected / push-warning banners | "Connection lost" state; warning when push isn't registered | UnpeelIOSRootView.swift |
| Presets drawer → New session | Launch a new session from Host presets | UnpeelIOSRootView.swift (PresetDrawerOverlay) |
| Live terminal (libghostty) | Local Ghostty surface fed by the Host output stream (WSS with HTTP long-poll fallback) | RemoteGhosttyTerminalView.swift; RemoteTerminalStreamTransport.swift |
| Snapshot baseline + frame reconciliation | Resyncs on offset mismatch without teardown | StreamFrameReconciler.swift; docs/agents/clients/remote-control.md |
| Terminal query filtering | Strips terminal query requests so the phone never answers them | TerminalQueryFilter.swift |
| Keyboard-safe sizing | Keyboard focus never resizes the remote grid; columns frozen | RemoteGhosttyTerminalView.swift; docs/agents/clients/terminal.md |
| Fit terminal to screen | Phone-driven resize of the shared PTY grid; revert to desktop | RemoteGhosttyTerminalView.swift; RemoteMacClient.swift (`resizeTerminal`, `revertDesktopTerminal`) |
| Extra-keys accessory bar | esc, ctrl, alt, cmd, tab, arrows, symbols, paste, backspace, hide keyboard, sticky modifiers | clients/native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/* |
| Menu control bar | When an agent draws a select menu: ↑ / ↓ / esc / return buttons | RemoteGhosttyTerminalView.swift (TerminalMenuControlBar); crates/unpeel-core/src/menu_prompt.rs |
| Predictive local echo | Mosh-style provisional keystroke echo over high-latency links | RemoteTerminalPrediction.swift |
| Predictive scrolling | Mosh-style scroll prediction for alternate-screen TUIs | RemoteTerminalScrollPrediction.swift |
| Remote mouse wheel / mouse mode | Translates touch scroll into mouse-wheel events when the TUI enables mouse reporting | RemoteGhosttyTerminalView.swift (RemoteTerminalMouseModeTracker) |
| Pinch zoom | Pinch gesture forwarded as zoom steps | RemoteGhosttyTerminalView.swift; vendor UITerminalView+PinchZoom.swift |
| Long-press text selection | Viewport text in a selection sheet with the pressed word preselected | TerminalTextSelectionSheet.swift |
| Scroll to bottom | Floating button | RemoteGhosttyTerminalView.swift |
| Session LRU cache | Keeps recently viewed terminals warm | TerminalSessionCache.swift |
| Push-to-talk dictation | Mic + on-device speech recognition with live transcript; commit, paste or discard | VoiceDictationController.swift; RemoteGhosttyTerminalView.swift |
| Apple Intelligence polish | Optional FoundationModels cleanup of dictation (iOS 26+) | DictationReflection.swift; PairingView.swift ("Polish with Apple Intelligence") |
| Approval prompts | In-session Allow / Don't Allow for write/browser/app-open grants; first answer wins | UnpeelIOSRootView.swift (ApprovalPromptOverlay) |
| Push notifications | APNs registration per paired workspace; tap routes to the session; retry registration | PushManager.swift; PairingView.swift |
| Title-bar activity bell | Activity sessions panel from the top bar | TerminalDetailView.swift (TitlebarActivityButton, ActivitySessionsPanel) |
| Exited session restart bar | Resume / restart an exited session | TerminalDetailView.swift (ExitedSessionRestartBar) |
| Session organize sheet | Long-press the title bar: rename, notify when done, copy transcript as Markdown (20/50/whole), restore/remove, resume | SessionOrganizeSheet.swift |
| Project organize sheet | Long-press a folder: rename group, sort sessions (custom / date), folder color, archive library | ProjectOrganizeSheet.swift |
| Archived sessions sheet | Restore / Restore & Resume from `/mobile/archive` | ArchivedSessionsSheet.swift |
| Session gallery | Browser/computer screenshots and downloads; open full size with pinch-zoom | BrowserGalleryPanel.swift |
| Upload images to session | PhotosPicker upload (resumable, JPEG/PNG transcode) into session artifacts | BrowserGalleryPanel.swift; RemoteMacClient.swift (ResumableArtifactUploader) |
| Image annotation | Arrow markup and crop (shared geometry with desktop); PencilKit freehand editor | BrowserGalleryPanel.swift (ArrowMarkup); ImageAnnotationView.swift |
| Add to message | Attach an annotated image to the agent prompt | BrowserGalleryPanel.swift |
| Request screenshot | Ask the agent to capture a browser screenshot; polls, then pulses the gallery | RemoteMacClient.swift (`requestScreenshot`); docs/agents/clients/remote-control.md |
| Delete artifact | Remove a gallery item | RemoteMacClient.swift (`deleteArtifact`) |
| Mark read | Clears unread when viewing | RemoteMacClient.swift (`markRead`) |
| Session actions | Stop, restart, resume agent, archive, remove, reorder | RemoteMacClient.swift (`performSessionAction`, `updateSessionOrder`, `restartSession`) |
| App lock | Optional Face ID / Touch ID / passcode lock on background and cold launch | AppLock.swift |
| Mascot | Animated pixel mascot on connection screens | MascotView.swift |
| Brand/tool icons | SVG-rasterized provider and chrome icons | SharedIconViews.swift |
| Feedback & bugs link | Feedback entry point | UnpeelIOSRootView.swift |
| Developer settings | "Show terminal bounds" and the simulator dev bridge (DEBUG only) | DevSettings.swift; Tools/dev_bridge.py |
| iPad support | Universal app (the PRD's "iPad Desk Companion" persona) | clients/ios/PRD.md; project.yml |
| Capability-gated UI | Features appear only when the Host bootstrap advertises the capability id | clients/shared/.../RemoteControlProtocol.swift (`supports`) |
| Transcript markdown | Fetch `/mobile/transcript-markdown` for copy | RemoteMacClient.swift (`transcriptMarkdown`) |
| Terminal metrics | Reads session grid metrics | RemoteMacClient.swift (`terminalMetrics`) |
| Relay output subscription credits | Flow-controlled relay output (subscribe/credit/stop) | RemoteMacClient.swift |
| Reconnect backoff | Terminal reconnect with backoff and a connection notice overlay | RemoteGhosttyTerminalView.swift |
| Privacy manifest / entitlements | Camera, mic, speech and push entitlements | App/Info.plist; App/PrivacyInfo.xcprivacy; App/UnpeelIOS.entitlements |

---

## 8. Shared Swift package (`clients/shared/UnpeelShared`)

| Feature | What it does | Source path(s) |
|---|---|---|
| Remote control protocol | Host contract DTOs, capability `supports()`, major-version compatibility | Sources/UnpeelShared/RemoteControlProtocol.swift |
| Pairing client | Compact code encode/decode; validates token, relay URL/token and 32-byte E2E key | RemotePairingClient.swift |
| Relay protocol | Forward-secret E2E handshake, sealed frames, transcript MAC | RelayProtocol.swift |
| Relay connection | WebSocket relay client used by iOS and the native Link downlink | RemoteRelayConnection.swift |
| Paired host record | Persisted Host identity used for fail-closed reconnects | PairedHostRecord.swift |
| Runtime catalog (generated) | Swift copy of the runtime registry (names, icons, tints) | GeneratedRuntimeCatalog.swift; generated/GeneratedRuntimeCatalog.swift |
| Tool/chrome icons | Shared provider/browser icon art | ToolIcons.swift; ChromeIcons.swift |
| KAT vector tests | Relay crypto pinned to `protocol/relay-kat-vectors-v1.json` | Tests/UnpeelSharedTests/RelayCryptoVectorTests.swift |
| Transport contract tests | Plugin/remote/transport contract tests | Tests/UnpeelSharedTests/* |

---

## 9. Protocol and generated artifacts (`protocol/`, `generated/`)

| Feature | What it does | Source path(s) |
|---|---|---|
| Host capability ledger | 52 stable operation ids with method/path (bootstrap, session.*, artifact.*, apps.*, settings.*, pairing.*, push, relay, filesystem.*, project.add, integrations.install); minor history 15→21 | protocol/host-capabilities-v1.json |
| Host conformance cases | HTTP method/path/status expectations every Host must pass | protocol/host-conformance-v1.json |
| Bootstrap compatibility | Legacy vs current bootstrap fixtures | protocol/host-bootstrap-compatibility-v1.json |
| Pane-layout operations | Normative split-tree operations, migration and equalize/navigation semantics | protocol/pane-layout-operations-v1.json |
| Direct-path v1 | Offer/answer message fixtures for NAT punch negotiation | protocol/direct-path-v1.json |
| Relay KAT vectors | Cross-language crypto known-answer vectors | protocol/relay-kat-vectors-v1.json |
| Browser engine pin | agent-browser version, license and per-platform URL + sha256 | protocol/browser-engine-v1.json |
| App registry | Official Apps allowlist (id, binary, version, icon SVG, media types, resource kinds, defaults) | protocol/app-registry.json |
| Unpeel UI protocol v1 | NDJSON App ↔ Host semantic UI messages (schema, stream, fixtures) | protocol/unpeel-ui-v1.schema.json, unpeel-ui-stream-v1.ndjson, unpeel-ui-fixtures-v1.json |
| Workspace UI / App Kit state schemas | App Kit state and workspace-UI protocol schemas | crates/apps/app-kit/protocol/* |
| Generated runtime catalog | Swift runtime catalog generated from runtimes/ | generated/GeneratedRuntimeCatalog.swift; scripts/generate-runtime-client-catalog.mjs |
| Protocol shipped in every CLI archive | `protocol/`, `generated/`, BUILD_PROVENANCE.json and notices ship in each archive | docs/agents/releases.md |

---

## Parity checklist

1. [CLI] `unpeel serve`: run the UI-free Host service for all registered workspaces
2. [CLI] `unpeel serve install|uninstall|status` for the per-user launchd/systemd unit, with a `--graphical` Linux variant
3. [CLI] `unpeel --workspace NAME <cmd>`: target an isolated workspace for any verb
4. [CLI] `unpeel pair` shows a one-time pairing code/QR (auto-starts serve; `--advertise-host/--advertise-port`)
5. [CLI] `unpeel pair list|remove <device>|relay <device> on|off`
6. [CLI] `unpeel ls [--json]`: list sessions with status/project/command
7. [CLI] `unpeel new` with `--command|--preset`, `--cwd`, `--project`, `--cols/--rows`, `--json`
8. [CLI] `unpeel add [PATH] [--name] [--here]`: add a folder as a project
9. [CLI] `unpeel send <id> <text> [--enter]`, going through the write policy when run inside a session
10. [CLI] `unpeel keys <id> <seq>`: raw bytes outside a session, key names inside
11. [CLI] `unpeel screen <id>`: parsed screen snapshot
12. [CLI] `unpeel logs|tail <id> [--lines] [--follow]`
13. [CLI] `unpeel wait <id> [--idle] [--text] [--timeout]` with exit code 1 on timeout
14. [CLI] `unpeel resume|restart <id>`
15. [CLI] `unpeel stop|archive|restore|rm <id>`
16. [CLI] `unpeel transcript <id> [--entries N] [--markdown]`
17. [CLI] `unpeel open <path|resource> [--with APP] [--kind] [--media-type]`: typed App dispatcher that creates or reuses a companion pane
18. [CLI] `unpeel settings list|get|set` (allowlisted keys, validated before write)
19. [CLI] `unpeel settings openers set <selector> <editor|system|app:id>`
20. [CLI] `unpeel apps list|install|update [--check] [--yes]`
21. [CLI] `unpeel apps link|unlink` dev slots
22. [CLI] `unpeel apps describe|search|context`
23. [CLI] `unpeel integrations [list]` status per agent
24. [CLI] `unpeel integrations install <runtime> [--project DIR] | --all`
25. [CLI] `unpeel mcp [<tool> [<action> k=v…]]`: every MCP action from the shell with the same identity and grants
26. [CLI] `unpeel browser open|snapshot|click|fill|type|press|get|screenshot|scroll|wait`
27. [CLI] `unpeel browser install [--check] [--json]` with exit codes 0/1/3/4
28. [CLI] `unpeel artifacts publish <image>`
29. [CLI] `unpeel current` (self plus pane neighbours)
30. [CLI] `unpeel report <summary> [--status update|done|blocked] [--details]`
31. [CLI] `unpeel worktree create <name> [--branch] [--base] [--project]`
32. [CLI] `unpeel agents <action>` / `unpeel skills <action>`
33. [CLI] `unpeel presets list|add|remove|edit`
34. [CLI] `unpeel presets star|unstar|enable|disable|reorder`
35. [CLI] `unpeel link enroll <key>|status|deactivate` with exit codes 0/1/2
36. [CLI] `unpeel workspaces list|add|remove`
37. [CLI] `unpeel projects list|add|remove`
38. [CLI] `unpeel hosts prune [--json]` (identity-verified orphan reap)
39. [CLI] Every shared-state mutation is a flocked, unknown-key-preserving write followed by a state-bus flush
40. [CLI] `--json` output on data verbs plus meaningful exit codes; `help`, `--version`, and a bare-invocation hint
41. [HOST] Machine supervisor with one worker per workspace home; machine lease and `serve.json` status
42. [HOST] Shared PTY core per workspace (single reactor, timer and journal threads)
43. [HOST] In-place PTY core upgrade via SCM_RIGHTS takeover (no terminal restart)
44. [HOST] Sessions survive service stop/restart; per-session fallback host when the core is unavailable
45. [HOST] On-disk session dir: manifest.json, output.bin journal, session.sock control
46. [HOST] Bounded append-only journal with monotonic offsets (~64–72 MiB retained)
47. [HOST] Exact VT snapshot for attach from the resident libghostty-vt grid
48. [HOST] PID + start-time identity guard; never signal a recycled pid
49. [HOST] Session ownership/provenance fields (owner principal, device, source preset)
50. [HOST] Auto-titling (agent OSC titles / first prompt / off; skip slash commands; rename wins)
51. [HOST] Titling from a resumed conversation's transcript (`__auto_title__`)
52. [HOST] Archive (non-destructive stop) and restore; Restore & Resume
53. [HOST] Auto-stop-and-archive idle sweep (30m to 24h; never unread-attention or pinned sessions)
54. [HOST] Resume on restart using the hook-captured provider conversation id
55. [HOST] Resume Agent in place (same session id) and restart agent
56. [HOST] Session reload (replace the host, keep the id)
57. [HOST] Restart recommendation when the host protocol version is too old
58. [HOST] Safe text delivery (bracketed paste, settle, double-Enter)
59. [HOST] Hook listener ingesting provider lifecycle events via the port registry
60. [HOST] Hook-latched busy/idle/attention activity engine (output is never evidence of work)
61. [HOST] Screen-tier busy/idle fallback for Claude/Codex/Gemini without hooks
62. [HOST] Runtime observation of agents started by hand in a shell
63. [HOST] Agent-drawn select-menu detection sets attention
64. [HOST] Escape-cancellation fencing of interrupted turns
65. [HOST] Background/subagent tracking keeps the session busy until children stop
66. [HOST] Lifecycle notification policy (needs-input, opt-in finished, App alerts) with viewing-device suppression
67. [HOST] Unread/mark-read and notify-when-done per session
68. [HOST] Shared approval hub (FIFO, coalesced, first answer wins from any Controller)
69. [HOST] Viewer presence leases and presence files
70. [HOST] Phone-fit resize and desktop-fit restore of the shared grid
71. [HOST] Persisted activity log (activity-log.jsonl)
72. [HOST] Local URL detection plus verify/find/stop of session-owned local servers
73. [HOST] First-run preset seeding from agent CLIs found on PATH
74. [HOST] Projects, plain groups, worktree child projects, pins, manual order and date sort
75. [HOST] Cross-frontend state bus notifications
76. [HOST] Git worktree create/list (default base = mainline)
77. [HOST] Unified `unpeel` MCP server (stdio, per agent client, gate outside hosted sessions)
78. [HOST] MCP `sessions` domain (current, list, inspect, read_screen, read_output, wait_for_text, send_text, send_keys, report)
79. [HOST] MCP `agents` domain (list, get, read_transcript, wait) with occurrence-bound refs
80. [HOST] MCP `workspace` domain (list_presets, create_worktree, list_worktrees)
81. [HOST] MCP `artifacts.add_to_gallery`
82. [HOST] MCP `browser` domain (13 actions incl. console, context, close)
83. [HOST] MCP `apps` domain (list, catalog, describe, search, context, open) with agent-openable App panes
84. [HOST] MCP `skills` domain (list, search, get)
85. [HOST] Open reads plus approval-controlled cross-session writes (ask/allow/deny, remembered directional pairs)
86. [HOST] Session creation and closing are user-only (agents refused)
87. [HOST] Caller identity via env or a verified process-ancestry fallback
88. [HOST] Lazy per-action help with terse schemas under a token budget
89. [HOST] Dual-era MCP protocol support (initialize and server/discover)
90. [HOST] MCP in-flight cancellation with ordered execution
91. [HOST] `/mcp/*` routes authenticated with a 0600 auth token
92. [HOST] App live context (`app-context.json`) surfaced to neighbouring agents
93. [HOST] Pinned agent-browser engine auto-install (sha256, flock, background at worker start)
94. [HOST] Browser shared project window (pinned tab per session, persistent logins) or separate-per-session mode
95. [HOST] Browser access On/Ask/Off with remembered session approvals
96. [HOST] Browser options: headed, domain allowlist, custom Chromium path, agent cursor overlay, theme
97. [HOST] Remote CDP endpoint mode (`remote-cdp.json`)
98. [HOST] Browser screenshots/downloads as session artifacts (auto-gallery toggle)
99. [HOST] Session artifact store (list, read, resumable upload, delete, thumbnails)
100. [HOST] Host filesystem ops for folder pickers and file transfer (scoped to projects)
101. [HOST] One-time-code pairing with per-device bearer token and E2E key
102. [HOST] Local pairing control route (begin/status/cancel/devices/revoke/relay-allowed)
103. [HOST] Controller-assisted pairing (pairing.invitation via an assisting Mac proxy)
104. [HOST] Direct `/mobile` over TLS with the pinned Host certificate
105. [HOST] Supervised WSS terminal streamer (backoff, crash-loop ceiling)
106. [HOST] Local `host.sock` framed Controller contract (0600)
107. [HOST] SSH stdio Host gateway (`__remote_stdio__`), including interactive-shell compat mode
108. [HOST] Unpeel Link relay uplink with forward-secret E2E; token rotation without eviction
109. [HOST] Relay credential recovery
110. [HOST] Direct-path negotiation and UDP NAT punch upgrade from relay
111. [HOST] Unpeel Link license activation/entitlement/seat (Ed25519 keys)
112. [HOST] APNs push registration and delivery via relay
113. [HOST] Platform-adapter seam for native effects (notify, push, approvals, overlay, thumbnails, Link, open-in-editor)
114. [HOST] Host-owned App install/open/opener policy (works on SSH/Linux Hosts)
115. [HOST] Plugin activation, ordering and lazy update checks for agents and Apps
116. [HOST] `__remote_attach__` network attach to another Host's session
117. [HOST] Graphical desktop-session service unit and diagnostics (Linux)
118. [HOST] Timestamped trace log for all components
119. [RUNTIMES] Runtime package format (runtime.toml, adapters, hooks, icon) auto-discovered at build
120. [RUNTIMES] Claude Code integration (hooks, MCP, screen fallback, transcript, semantic titles, subagents)
121. [RUNTIMES] Codex integration (native hooks + notify, MCP, screen fallback, transcript)
122. [RUNTIMES] Gemini CLI integration (hooks, screen fallback, transcript)
123. [RUNTIMES] Cursor Agent, Grok, Kimi, Kiro and Cline integrations (hooks, MCP where declared, transcripts)
124. [RUNTIMES] Amp and GitHub Copilot per-project hook integrations
125. [RUNTIMES] OpenCode plugin and Muse Code plugin integrations
126. [RUNTIMES] Antigravity and fx MCP-only integrations; Pi detection-only
127. [RUNTIMES] Provider-neutral launch (preset runs exactly as typed) with suggested default presets per runtime
128. [RUNTIMES] Shared MCP shim (`~/.unpeel/bin/unpeel-mcp`) plus post-upgrade integration refresh
129. [RUNTIMES] Per-runtime install command, usage stores and resume recipes
130. [APPS] `unpeel-attach` client (snapshot/tail replay, event-driven follow, focus filtering, resize)
131. [APPS] `unpeel-apps` "send to adjacent agent" API with clipboard fallback
132. [APPS] Official App registry with typed resources (file media type, folder, git.working-tree)
133. [APPS] App companion panes with reveal revisions and App-branded session rows
134. [APPS] Git App: Changes tab with status glyphs and unified diffs
135. [APPS] Git App: syntax-highlighted patches with size limits
136. [APPS] Git App: History tab (paged commits, commit files, patches)
137. [APPS] Git App: Fetch/Pull/Push control (fast-forward only, no force)
138. [APPS] Git App: select lines, then Copy lines or Send reference to agent
139. [APPS] Files App: scoped explorer with filter and `--ext`, context menu (open/send/copy), path drag
140. [APPS] Files/Git/Usage Apps follow the neighbouring agent's project/worktree
141. [APPS] Markdown App: live-styled editor, slash menu, `\` palette, to-dos, full mouse editing
142. [APPS] Markdown App: auto-save, Open/Save/New note, vault explorer, remembered notes folder
143. [APPS] Usage App: per-provider quota gauges, token history, project/total monthly tables, alerts, themes
144. [APPS] App Kit component library (Page/List/Input/TextBox/Tree/Menu/Gauge/Sparkline/Charts/Media/Surface)
145. [APPS] App Kit hosted semantic UI bridge (revisions, deltas, scoped participant tokens, agent `edit` grants)
146. [APPS] App Kit SwiftUI peer renderer plus Kitchen Sink mini-host and screen audit
147. [WEB] App Kit TypeScript/DOM/ARIA web renderer of the same App tree
148. [WEB] Hosted installer endpoints (`unpeel.com/install.sh`, per-App `install/<app>/install.sh`) with channel selection (alpha/beta/stable)
149. [WEB] Help/docs/download links (unpeel.com/docs, /download/mac, /ios) referenced from the apps
150. [DESKTOP] App shell: resizable vibrancy sidebar, custom titlebar, ⌘B toggle
151. [DESKTOP] Project/session tree with pins, groups, worktree folders, attention dot and busy spinners
152. [DESKTOP] Detached drag of sessions/projects (reorder, move to group, drop-to-split)
153. [DESKTOP] Session context menu (rename, copy ID, copy transcript, notify when done, clear attention, resume, restart app, reveal, pin, stop and archive, remove)
154. [DESKTOP] Project context menu (new worktree, new group, rename, stop all, sort, folder color, archived, open in editor, move to workspace)
155. [DESKTOP] Folder color palette (8 colors)
156. [DESKTOP] Archived sessions library with search, Restore, Restore & Resume, Delete permanently
157. [DESKTOP] Workspace dots, trackpad swipe and workspace selector popover
158. [DESKTOP] Open a workspace in a new window
159. [DESKTOP] Move a project between local workspaces
160. [DESKTOP] Right-side global project sidebar panel (pinned session stack)
161. [DESKTOP] Local-site globe button with open URL / stop server menu
162. [DESKTOP] Titlebar "Open in" menu for 24 external editors/terminals/git clients
163. [DESKTOP] Session launcher (pick a tool) and empty state
164. [DESKTOP] ⌘K command palette (sessions, projects, presets, commands)
165. [DESKTOP] ⌃Tab MRU session switcher
166. [DESKTOP] ⌘1–9 session switching and ⌃1–9 project switching with held-key hints
167. [DESKTOP] Recent activity page (⇧⌘R) and titlebar activity bell dropdown
168. [DESKTOP] Toast notifications (for example, device connected)
169. [DESKTOP] libghostty Metal terminal surfaces, retained per session
170. [DESKTOP] Remote-host panes rendered via in-memory Ghostty surfaces (same UI as local)
171. [DESKTOP] Split Pane Right/Down (⌘D / ⇧⌘D), recursive tree up to 8 panes
172. [DESKTOP] Zoom pane (⇧⌘↩), Equalize splits, spatial focus (⌥⌘ arrows)
173. [DESKTOP] Detach Pane / Exit Multi-Pane View
174. [DESKTOP] Pane header menu with Agents/Plugins launch sections
175. [DESKTOP] Transient launcher pane for new sessions in a group
176. [DESKTOP] Persisted pane layouts per scope (pane-layouts.json)
177. [DESKTOP] Find bar (⌘F, ⌘G, ⇧⌘G)
178. [DESKTOP] Font size increase/decrease/reset (⌘+ ⌘- ⌘0)
179. [DESKTOP] URL/OSC 8 links and OSC 7 cwd tracking
180. [DESKTOP] ⌘-click bare file paths (with line/column) to open in App or editor
181. [DESKTOP] Native file drag out of hosted Apps and drop into Apps/terminals
182. [DESKTOP] Scroll-to-bottom button; exited-session bar (Resume / Start fresh); resume-failure notice
183. [DESKTOP] Restart recommendation banner
184. [DESKTOP] Agent TUI background color matching for chrome
185. [DESKTOP] Viewer presence avatars and "Fit to desktop" control
186. [DESKTOP] In-pane MCP approval overlay (write/browser/app-open)
187. [DESKTOP] Session gallery panel (screenshots, downloads, uploads)
188. [DESKTOP] Gallery arrow + crop markup and "Add to prompt"
189. [DESKTOP] Take Screenshot (⇧⌘S) into the session and attach it to the prompt
190. [DESKTOP] Full main menu set (App/Session/Edit/View/Window/Help) with shortcuts
191. [DESKTOP] Menu-bar status item with activity spinner and popover
192. [DESKTOP] Keep running as a menu-bar agent when the window closes
193. [DESKTOP] Finder "New Unpeel Session Here" service
194. [DESKTOP] Sparkle auto-updates with a beta channel opt-in
195. [DESKTOP] macOS Notification Center banners (needs input / finished / App alerts) plus a test notification
196. [DESKTOP] Keychain-backed Link license
197. [DESKTOP] Bundled Host service lifecycle management via launchd
198. [DESKTOP] Bonjour nearby-host discovery for Add Workspace
199. [DESKTOP] Remote folder picker for launching on remote Hosts
200. [DESKTOP] Settings scope picker (This Mac / workspace / remote Host) with inherit/reset
201. [DESKTOP] Settings ▸ Workspaces (unified list, add local/nearby/code/SSH, rename, color, forget, delete)
202. [DESKTOP] Settings ▸ Agents (install CLI, install/reinstall integration, commands and variants, default, activate, reorder)
203. [DESKTOP] Settings ▸ Plugins (Apps catalog install/update/activate/order)
204. [DESKTOP] Settings ▸ Agent access ▸ Sessions (write policy, worktree permission, auto-gallery, approved pairs/Apps with Revoke)
205. [DESKTOP] Settings ▸ Agent access ▸ Browser (engine status, access mode, approvals, window/cursor/scope/app path, site rules, clear data)
206. [DESKTOP] Settings ▸ Appearance (mode, 8 accent colors, background/surface/transparency, terminal font and size, line height, session title mode, gallery chip)
207. [DESKTOP] Settings ▸ Remote Control (share this Mac/workspace QR/code, paired devices, revoke, per-device Link toggle)
208. [DESKTOP] Add iPhone/iPad to a remote Host (controller-assisted pairing)
209. [DESKTOP] Unpeel Link license section (activate, seats, release seat, get Link)
210. [DESKTOP] Settings ▸ Transcripts (content toggles, info header, range)
211. [DESKTOP] Settings ▸ Notifications (flag select menus, completion, test Mac/phone, delivery diagnostics)
212. [DESKTOP] Settings ▸ Worktrees ("Show agent worktrees", list with create/reveal/remove)
213. [DESKTOP] Settings ▸ Features (Remote workspaces, Git worktrees, Sessions use, Workspaces, Browser use)
214. [DESKTOP] Settings ▸ Advanced (auto-archive cleanup, sidebar archive preview, memory, running hosts by CPU with Stop, sessions folder, trace log)
215. [DESKTOP] Default editor / opener preference
216. [DESKTOP] Presets stored in the shared app-state.json with live pickup of CLI edits
217. [DESKTOP] Remote Host scope uses the same sidebar/content UI with Direct→Link automatic fallback
218. [DESKTOP] Worktree discovery of agent-created checkouts (opt-in, every 5 s)
219. [DESKTOP] Startup presentation cache for instant sidebar
220. [IOS] QR scan or paste-code pairing; multiple paired workspaces with switcher; Forget
221. [IOS] Direct pinned-HTTPS LAN transport plus Link relay E2E transport with credential repair
222. [IOS] Mac-style sessions drawer (projects, sessions, activity pills, ages, workspace header)
223. [IOS] Presets drawer to start a new session on the Host
224. [IOS] Live Ghostty terminal via WSS (HTTP long-poll fallback) with snapshot baseline and resync
225. [IOS] Keyboard never resizes the remote grid; "Fit terminal to screen" and revert to desktop
226. [IOS] Extra-keys accessory bar (esc/ctrl/alt/cmd/tab/arrows/symbols/paste/backspace/hide)
227. [IOS] Select-menu control bar (↑/↓/esc/return) for agent menus
228. [IOS] Mosh-style predictive echo and predictive scrolling; remote mouse-wheel; pinch zoom; long-press text selection
229. [IOS] Push-to-talk dictation with live transcript (paste/discard)
230. [IOS] Optional Apple Intelligence "polish" of dictation (iOS 26+)
231. [IOS] In-session Allow / Don't Allow approval prompts (first answer wins)
232. [IOS] APNs push registration per workspace, retry, tap routes to the session
233. [IOS] Title-bar activity bell with active sessions panel
234. [IOS] Exited-session restart bar (Resume)
235. [IOS] Session organize sheet (rename, notify when done, copy transcript Markdown 20/50/whole, restore/remove, resume)
236. [IOS] Project organize sheet (rename group, sort custom/date, folder color, archive library)
237. [IOS] Archived sessions sheet with Restore / Restore & Resume
238. [IOS] Session gallery with pinch-zoom full-size view and delete
239. [IOS] Upload photos into the session (resumable, JPEG/PNG transcode)
240. [IOS] Arrow/crop and PencilKit freehand image annotation, then "Add to message"
241. [IOS] Request-screenshot action (prompts the agent, polls, opens gallery)
242. [IOS] Face ID / Touch ID / passcode app lock
243. [IOS] Session actions from phone (stop, restart, resume agent, archive, remove, reorder, mark read)
244. [IOS] Capability-gated UI driven by Host bootstrap; connection-lost and push-warning banners; reconnect backoff
245. [SHARED] One Swift implementation of Host protocol DTOs, pairing client and paired-host records for Mac and iOS
246. [SHARED] Relay forward-secret E2E protocol and WebSocket client, pinned by cross-language KAT vectors
247. [PROTO] Versioned Host capability ledger (52 op ids, major 1 / minor 21, additive, capability-checked)
248. [PROTO] Host conformance and bootstrap-compatibility fixtures that every Host implementation must pass
249. [PROTO] Normative pane-layout operations, direct-path v1, relay KAT, browser-engine pin and App registry contracts
250. [PROTO] Unpeel UI protocol v1 (NDJSON App-to-Host semantic UI) schema, stream and fixtures
251. [DIST] SHA-256-verified curl installers for the CLI and each App; channels alpha/beta/stable on R2
252. [DIST] Lockstep app/CLI versioning; CLI archives carry protocol/, generated/, provenance and notices
253. [DIST] Signed and notarized Mac DMG plus Sparkle appcast; iOS via TestFlight
