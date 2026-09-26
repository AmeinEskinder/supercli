# supercli Parity Checklist

Parity against upstream unpeel . Each item measured per FEATURE.

**Status values:**  (feature works, tested),  (works but incomplete),  (not implemented),  (exceeds unpeel).

**Done criteria for [DESKTOP]:** rendered screenshot + test exercising the behaviour.

## Summary

| Tag | Total | Done | Partial | Missing | Improved |
|-----|-------|------|---------|---------|----------|
| [APPS] | 17 | 0 | 0 | 17 | 0 |
| [CLI] | 40 | 0 | 0 | 40 | 0 |
| [DESKTOP] | 70 | 0 | 0 | 70 | 0 |
| [DIST] | 3 | 0 | 0 | 3 | 0 |
| [HOST] | 78 | 0 | 0 | 78 | 0 |
| [IOS] | 25 | 0 | 0 | 25 | 0 |
| [PROTO] | 4 | 0 | 0 | 4 | 0 |
| [RUNTIMES] | 11 | 0 | 0 | 11 | 0 |
| [SHARED] | 2 | 0 | 0 | 2 | 0 |
| [WEB] | 3 | 0 | 0 | 3 | 0 |

## Items

| # | Tag | Description | Status | supercli Path | Test |
|---|-----|-------------|--------|---------------|------|
| 1 | [CLI] | `unpeel serve`: run the UI-free Host service for all registered workspaces | missing | - | - |
| 2 | [CLI] | `unpeel serve install\|uninstall\|status` for the per-user launchd/systemd unit, with a `--graphical` Linux variant | missing | - | - |
| 3 | [CLI] | `unpeel --workspace NAME <cmd>`: target an isolated workspace for any verb | missing | - | - |
| 4 | [CLI] | `unpeel pair` shows a one-time pairing code/QR (auto-starts serve; `--advertise-host/--advertise-port`) | missing | - | - |
| 5 | [CLI] | `unpeel pair list\|remove <device>\|relay <device> on\|off` | missing | - | - |
| 6 | [CLI] | `unpeel ls [--json]`: list sessions with status/project/command | missing | - | - |
| 7 | [CLI] | `unpeel new` with `--command\|--preset`, `--cwd`, `--project`, `--cols/--rows`, `--json` | missing | - | - |
| 8 | [CLI] | `unpeel add [PATH] [--name] [--here]`: add a folder as a project | missing | - | - |
| 9 | [CLI] | `unpeel send <id> <text> [--enter]`, going through the write policy when run inside a session | missing | - | - |
| 10 | [CLI] | `unpeel keys <id> <seq>`: raw bytes outside a session, key names inside | missing | - | - |
| 11 | [CLI] | `unpeel screen <id>`: parsed screen snapshot | missing | - | - |
| 12 | [CLI] | `unpeel logs\|tail <id> [--lines] [--follow]` | missing | - | - |
| 13 | [CLI] | `unpeel wait <id> [--idle] [--text] [--timeout]` with exit code 1 on timeout | missing | - | - |
| 14 | [CLI] | `unpeel resume\|restart <id>` | missing | - | - |
| 15 | [CLI] | `unpeel stop\|archive\|restore\|rm <id>` | missing | - | - |
| 16 | [CLI] | `unpeel transcript <id> [--entries N] [--markdown]` | missing | - | - |
| 17 | [CLI] | `unpeel open <path\|resource> [--with APP] [--kind] [--media-type]`: typed App dispatcher that creates or reuses a companion pane | missing | - | - |
| 18 | [CLI] | `unpeel settings list\|get\|set` (allowlisted keys, validated before write) | missing | - | - |
| 19 | [CLI] | `unpeel settings openers set <selector> <editor\|system\|app:id>` | missing | - | - |
| 20 | [CLI] | `unpeel apps list\|install\|update [--check] [--yes]` | missing | - | - |
| 21 | [CLI] | `unpeel apps link\|unlink` dev slots | missing | - | - |
| 22 | [CLI] | `unpeel apps describe\|search\|context` | missing | - | - |
| 23 | [CLI] | `unpeel integrations [list]` status per agent | missing | - | - |
| 24 | [CLI] | `unpeel integrations install <runtime> [--project DIR] \| --all` | missing | - | - |
| 25 | [CLI] | `unpeel mcp [<tool> [<action> k=v…]]`: every MCP action from the shell with the same identity and grants | missing | - | - |
| 26 | [CLI] | `unpeel browser open\|snapshot\|click\|fill\|type\|press\|get\|screenshot\|scroll\|wait` | missing | - | - |
| 27 | [CLI] | `unpeel browser install [--check] [--json]` with exit codes 0/1/3/4 | missing | - | - |
| 28 | [CLI] | `unpeel artifacts publish <image>` | missing | - | - |
| 29 | [CLI] | `unpeel current` (self plus pane neighbours) | missing | - | - |
| 30 | [CLI] | `unpeel report <summary> [--status update\|done\|blocked] [--details]` | missing | - | - |
| 31 | [CLI] | `unpeel worktree create <name> [--branch] [--base] [--project]` | missing | - | - |
| 32 | [CLI] | `unpeel agents <action>` / `unpeel skills <action>` | missing | - | - |
| 33 | [CLI] | `unpeel presets list\|add\|remove\|edit` | missing | - | - |
| 34 | [CLI] | `unpeel presets star\|unstar\|enable\|disable\|reorder` | missing | - | - |
| 35 | [CLI] | `unpeel link enroll <key>\|status\|deactivate` with exit codes 0/1/2 | missing | - | - |
| 36 | [CLI] | `unpeel workspaces list\|add\|remove` | missing | - | - |
| 37 | [CLI] | `unpeel projects list\|add\|remove` | missing | - | - |
| 38 | [CLI] | `unpeel hosts prune [--json]` (identity-verified orphan reap) | missing | - | - |
| 39 | [CLI] | Every shared-state mutation is a flocked, unknown-key-preserving write followed by a state-bus flush | missing | - | - |
| 40 | [CLI] | `--json` output on data verbs plus meaningful exit codes; `help`, `--version`, and a bare-invocation hint | missing | - | - |
| 41 | [HOST] | Machine supervisor with one worker per workspace home; machine lease and `serve.json` status | missing | - | - |
| 42 | [HOST] | Shared PTY core per workspace (single reactor, timer and journal threads) | missing | - | - |
| 43 | [HOST] | In-place PTY core upgrade via SCM_RIGHTS takeover (no terminal restart) | missing | - | - |
| 44 | [HOST] | Sessions survive service stop/restart; per-session fallback host when the core is unavailable | missing | - | - |
| 45 | [HOST] | On-disk session dir: manifest.json, output.bin journal, session.sock control | missing | - | - |
| 46 | [HOST] | Bounded append-only journal with monotonic offsets (~64–72 MiB retained) | missing | - | - |
| 47 | [HOST] | Exact VT snapshot for attach from the resident libghostty-vt grid | missing | - | - |
| 48 | [HOST] | PID + start-time identity guard; never signal a recycled pid | missing | - | - |
| 49 | [HOST] | Session ownership/provenance fields (owner principal, device, source preset) | missing | - | - |
| 50 | [HOST] | Auto-titling (agent OSC titles / first prompt / off; skip slash commands; rename wins) | missing | - | - |
| 51 | [HOST] | Titling from a resumed conversation's transcript (`__auto_title__`) | missing | - | - |
| 52 | [HOST] | Archive (non-destructive stop) and restore; Restore & Resume | missing | - | - |
| 53 | [HOST] | Auto-stop-and-archive idle sweep (30m to 24h; never unread-attention or pinned sessions) | missing | - | - |
| 54 | [HOST] | Resume on restart using the hook-captured provider conversation id | missing | - | - |
| 55 | [HOST] | Resume Agent in place (same session id) and restart agent | missing | - | - |
| 56 | [HOST] | Session reload (replace the host, keep the id) | missing | - | - |
| 57 | [HOST] | Restart recommendation when the host protocol version is too old | missing | - | - |
| 58 | [HOST] | Safe text delivery (bracketed paste, settle, double-Enter) | missing | - | - |
| 59 | [HOST] | Hook listener ingesting provider lifecycle events via the port registry | missing | - | - |
| 60 | [HOST] | Hook-latched busy/idle/attention activity engine (output is never evidence of work) | missing | - | - |
| 61 | [HOST] | Screen-tier busy/idle fallback for Claude/Codex/Gemini without hooks | missing | - | - |
| 62 | [HOST] | Runtime observation of agents started by hand in a shell | missing | - | - |
| 63 | [HOST] | Agent-drawn select-menu detection sets attention | missing | - | - |
| 64 | [HOST] | Escape-cancellation fencing of interrupted turns | missing | - | - |
| 65 | [HOST] | Background/subagent tracking keeps the session busy until children stop | missing | - | - |
| 66 | [HOST] | Lifecycle notification policy (needs-input, opt-in finished, App alerts) with viewing-device suppression | missing | - | - |
| 67 | [HOST] | Unread/mark-read and notify-when-done per session | missing | - | - |
| 68 | [HOST] | Shared approval hub (FIFO, coalesced, first answer wins from any Controller) | missing | - | - |
| 69 | [HOST] | Viewer presence leases and presence files | missing | - | - |
| 70 | [HOST] | Phone-fit resize and desktop-fit restore of the shared grid | missing | - | - |
| 71 | [HOST] | Persisted activity log (activity-log.jsonl) | missing | - | - |
| 72 | [HOST] | Local URL detection plus verify/find/stop of session-owned local servers | missing | - | - |
| 73 | [HOST] | First-run preset seeding from agent CLIs found on PATH | missing | - | - |
| 74 | [HOST] | Projects, plain groups, worktree child projects, pins, manual order and date sort | missing | - | - |
| 75 | [HOST] | Cross-frontend state bus notifications | missing | - | - |
| 76 | [HOST] | Git worktree create/list (default base = mainline) | missing | - | - |
| 77 | [HOST] | Unified `unpeel` MCP server (stdio, per agent client, gate outside hosted sessions) | missing | - | - |
| 78 | [HOST] | MCP `sessions` domain (current, list, inspect, read_screen, read_output, wait_for_text, send_text, send_keys, report) | missing | - | - |
| 79 | [HOST] | MCP `agents` domain (list, get, read_transcript, wait) with occurrence-bound refs | missing | - | - |
| 80 | [HOST] | MCP `workspace` domain (list_presets, create_worktree, list_worktrees) | missing | - | - |
| 81 | [HOST] | MCP `artifacts.add_to_gallery` | missing | - | - |
| 82 | [HOST] | MCP `browser` domain (13 actions incl. console, context, close) | missing | - | - |
| 83 | [HOST] | MCP `apps` domain (list, catalog, describe, search, context, open) with agent-openable App panes | missing | - | - |
| 84 | [HOST] | MCP `skills` domain (list, search, get) | missing | - | - |
| 85 | [HOST] | Open reads plus approval-controlled cross-session writes (ask/allow/deny, remembered directional pairs) | missing | - | - |
| 86 | [HOST] | Session creation and closing are user-only (agents refused) | missing | - | - |
| 87 | [HOST] | Caller identity via env or a verified process-ancestry fallback | missing | - | - |
| 88 | [HOST] | Lazy per-action help with terse schemas under a token budget | missing | - | - |
| 89 | [HOST] | Dual-era MCP protocol support (initialize and server/discover) | missing | - | - |
| 90 | [HOST] | MCP in-flight cancellation with ordered execution | missing | - | - |
| 91 | [HOST] | `/mcp/*` routes authenticated with a 0600 auth token | missing | - | - |
| 92 | [HOST] | App live context (`app-context.json`) surfaced to neighbouring agents | missing | - | - |
| 93 | [HOST] | Pinned agent-browser engine auto-install (sha256, flock, background at worker start) | missing | - | - |
| 94 | [HOST] | Browser shared project window (pinned tab per session, persistent logins) or separate-per-session mode | missing | - | - |
| 95 | [HOST] | Browser access On/Ask/Off with remembered session approvals | missing | - | - |
| 96 | [HOST] | Browser options: headed, domain allowlist, custom Chromium path, agent cursor overlay, theme | missing | - | - |
| 97 | [HOST] | Remote CDP endpoint mode (`remote-cdp.json`) | missing | - | - |
| 98 | [HOST] | Browser screenshots/downloads as session artifacts (auto-gallery toggle) | missing | - | - |
| 99 | [HOST] | Session artifact store (list, read, resumable upload, delete, thumbnails) | missing | - | - |
| 100 | [HOST] | Host filesystem ops for folder pickers and file transfer (scoped to projects) | missing | - | - |
| 101 | [HOST] | One-time-code pairing with per-device bearer token and E2E key | missing | - | - |
| 102 | [HOST] | Local pairing control route (begin/status/cancel/devices/revoke/relay-allowed) | missing | - | - |
| 103 | [HOST] | Controller-assisted pairing (pairing.invitation via an assisting Mac proxy) | missing | - | - |
| 104 | [HOST] | Direct `/mobile` over TLS with the pinned Host certificate | missing | - | - |
| 105 | [HOST] | Supervised WSS terminal streamer (backoff, crash-loop ceiling) | missing | - | - |
| 106 | [HOST] | Local `host.sock` framed Controller contract (0600) | missing | - | - |
| 107 | [HOST] | SSH stdio Host gateway (`__remote_stdio__`), including interactive-shell compat mode | missing | - | - |
| 108 | [HOST] | Unpeel Link relay uplink with forward-secret E2E; token rotation without eviction | missing | - | - |
| 109 | [HOST] | Relay credential recovery | missing | - | - |
| 110 | [HOST] | Direct-path negotiation and UDP NAT punch upgrade from relay | missing | - | - |
| 111 | [HOST] | Unpeel Link license activation/entitlement/seat (Ed25519 keys) | missing | - | - |
| 112 | [HOST] | APNs push registration and delivery via relay | missing | - | - |
| 113 | [HOST] | Platform-adapter seam for native effects (notify, push, approvals, overlay, thumbnails, Link, open-in-editor) | missing | - | - |
| 114 | [HOST] | Host-owned App install/open/opener policy (works on SSH/Linux Hosts) | missing | - | - |
| 115 | [HOST] | Plugin activation, ordering and lazy update checks for agents and Apps | missing | - | - |
| 116 | [HOST] | `__remote_attach__` network attach to another Host's session | missing | - | - |
| 117 | [HOST] | Graphical desktop-session service unit and diagnostics (Linux) | missing | - | - |
| 118 | [HOST] | Timestamped trace log for all components | missing | - | - |
| 119 | [RUNTIMES] | Runtime package format (runtime.toml, adapters, hooks, icon) auto-discovered at build | missing | - | - |
| 120 | [RUNTIMES] | Claude Code integration (hooks, MCP, screen fallback, transcript, semantic titles, subagents) | missing | - | - |
| 121 | [RUNTIMES] | Codex integration (native hooks + notify, MCP, screen fallback, transcript) | missing | - | - |
| 122 | [RUNTIMES] | Gemini CLI integration (hooks, screen fallback, transcript) | missing | - | - |
| 123 | [RUNTIMES] | Cursor Agent, Grok, Kimi, Kiro and Cline integrations (hooks, MCP where declared, transcripts) | missing | - | - |
| 124 | [RUNTIMES] | Amp and GitHub Copilot per-project hook integrations | missing | - | - |
| 125 | [RUNTIMES] | OpenCode plugin and Muse Code plugin integrations | missing | - | - |
| 126 | [RUNTIMES] | Antigravity and fx MCP-only integrations; Pi detection-only | missing | - | - |
| 127 | [RUNTIMES] | Provider-neutral launch (preset runs exactly as typed) with suggested default presets per runtime | missing | - | - |
| 128 | [RUNTIMES] | Shared MCP shim (`~/.unpeel/bin/unpeel-mcp`) plus post-upgrade integration refresh | missing | - | - |
| 129 | [RUNTIMES] | Per-runtime install command, usage stores and resume recipes | missing | - | - |
| 130 | [APPS] | `unpeel-attach` client (snapshot/tail replay, event-driven follow, focus filtering, resize) | missing | - | - |
| 131 | [APPS] | `unpeel-apps` "send to adjacent agent" API with clipboard fallback | missing | - | - |
| 132 | [APPS] | Official App registry with typed resources (file media type, folder, git.working-tree) | missing | - | - |
| 133 | [APPS] | App companion panes with reveal revisions and App-branded session rows | missing | - | - |
| 134 | [APPS] | Git App: Changes tab with status glyphs and unified diffs | missing | - | - |
| 135 | [APPS] | Git App: syntax-highlighted patches with size limits | missing | - | - |
| 136 | [APPS] | Git App: History tab (paged commits, commit files, patches) | missing | - | - |
| 137 | [APPS] | Git App: Fetch/Pull/Push control (fast-forward only, no force) | missing | - | - |
| 138 | [APPS] | Git App: select lines, then Copy lines or Send reference to agent | missing | - | - |
| 139 | [APPS] | Files App: scoped explorer with filter and `--ext`, context menu (open/send/copy), path drag | missing | - | - |
| 140 | [APPS] | Files/Git/Usage Apps follow the neighbouring agent's project/worktree | missing | - | - |
| 141 | [APPS] | Markdown App: live-styled editor, slash menu, `\` palette, to-dos, full mouse editing | missing | - | - |
| 142 | [APPS] | Markdown App: auto-save, Open/Save/New note, vault explorer, remembered notes folder | missing | - | - |
| 143 | [APPS] | Usage App: per-provider quota gauges, token history, project/total monthly tables, alerts, themes | missing | - | - |
| 144 | [APPS] | App Kit component library (Page/List/Input/TextBox/Tree/Menu/Gauge/Sparkline/Charts/Media/Surface) | missing | - | - |
| 145 | [APPS] | App Kit hosted semantic UI bridge (revisions, deltas, scoped participant tokens, agent `edit` grants) | missing | - | - |
| 146 | [APPS] | App Kit SwiftUI peer renderer plus Kitchen Sink mini-host and screen audit | missing | - | - |
| 147 | [WEB] | App Kit TypeScript/DOM/ARIA web renderer of the same App tree | missing | - | - |
| 148 | [WEB] | Hosted installer endpoints (`unpeel.com/install.sh`, per-App `install/<app>/install.sh`) with channel selection (alpha/beta/stable) | missing | - | - |
| 149 | [WEB] | Help/docs/download links (unpeel.com/docs, /download/mac, /ios) referenced from the apps | missing | - | - |
| 150 | [DESKTOP] | App shell: resizable vibrancy sidebar, custom titlebar, ⌘B toggle | missing | - | - |
| 151 | [DESKTOP] | Project/session tree with pins, groups, worktree folders, attention dot and busy spinners | missing | - | - |
| 152 | [DESKTOP] | Detached drag of sessions/projects (reorder, move to group, drop-to-split) | missing | - | - |
| 153 | [DESKTOP] | Session context menu (rename, copy ID, copy transcript, notify when done, clear attention, resume, restart app, reveal, pin, stop and archive, remove) | missing | - | - |
| 154 | [DESKTOP] | Project context menu (new worktree, new group, rename, stop all, sort, folder color, archived, open in editor, move to workspace) | missing | - | - |
| 155 | [DESKTOP] | Folder color palette (8 colors) | missing | - | - |
| 156 | [DESKTOP] | Archived sessions library with search, Restore, Restore & Resume, Delete permanently | missing | - | - |
| 157 | [DESKTOP] | Workspace dots, trackpad swipe and workspace selector popover | missing | - | - |
| 158 | [DESKTOP] | Open a workspace in a new window | missing | - | - |
| 159 | [DESKTOP] | Move a project between local workspaces | missing | - | - |
| 160 | [DESKTOP] | Right-side global project sidebar panel (pinned session stack) | missing | - | - |
| 161 | [DESKTOP] | Local-site globe button with open URL / stop server menu | missing | - | - |
| 162 | [DESKTOP] | Titlebar "Open in" menu for 24 external editors/terminals/git clients | missing | - | - |
| 163 | [DESKTOP] | Session launcher (pick a tool) and empty state | missing | - | - |
| 164 | [DESKTOP] | ⌘K command palette (sessions, projects, presets, commands) | missing | - | - |
| 165 | [DESKTOP] | ⌃Tab MRU session switcher | missing | - | - |
| 166 | [DESKTOP] | ⌘1–9 session switching and ⌃1–9 project switching with held-key hints | missing | - | - |
| 167 | [DESKTOP] | Recent activity page (⇧⌘R) and titlebar activity bell dropdown | missing | - | - |
| 168 | [DESKTOP] | Toast notifications (for example, device connected) | missing | - | - |
| 169 | [DESKTOP] | libghostty Metal terminal surfaces, retained per session | missing | - | - |
| 170 | [DESKTOP] | Remote-host panes rendered via in-memory Ghostty surfaces (same UI as local) | missing | - | - |
| 171 | [DESKTOP] | Split Pane Right/Down (⌘D / ⇧⌘D), recursive tree up to 8 panes | missing | - | - |
| 172 | [DESKTOP] | Zoom pane (⇧⌘↩), Equalize splits, spatial focus (⌥⌘ arrows) | missing | - | - |
| 173 | [DESKTOP] | Detach Pane / Exit Multi-Pane View | missing | - | - |
| 174 | [DESKTOP] | Pane header menu with Agents/Plugins launch sections | missing | - | - |
| 175 | [DESKTOP] | Transient launcher pane for new sessions in a group | missing | - | - |
| 176 | [DESKTOP] | Persisted pane layouts per scope (pane-layouts.json) | missing | - | - |
| 177 | [DESKTOP] | Find bar (⌘F, ⌘G, ⇧⌘G) | missing | - | - |
| 178 | [DESKTOP] | Font size increase/decrease/reset (⌘+ ⌘- ⌘0) | missing | - | - |
| 179 | [DESKTOP] | URL/OSC 8 links and OSC 7 cwd tracking | missing | - | - |
| 180 | [DESKTOP] | ⌘-click bare file paths (with line/column) to open in App or editor | missing | - | - |
| 181 | [DESKTOP] | Native file drag out of hosted Apps and drop into Apps/terminals | missing | - | - |
| 182 | [DESKTOP] | Scroll-to-bottom button; exited-session bar (Resume / Start fresh); resume-failure notice | missing | - | - |
| 183 | [DESKTOP] | Restart recommendation banner | missing | - | - |
| 184 | [DESKTOP] | Agent TUI background color matching for chrome | missing | - | - |
| 185 | [DESKTOP] | Viewer presence avatars and "Fit to desktop" control | missing | - | - |
| 186 | [DESKTOP] | In-pane MCP approval overlay (write/browser/app-open) | missing | - | - |
| 187 | [DESKTOP] | Session gallery panel (screenshots, downloads, uploads) | missing | - | - |
| 188 | [DESKTOP] | Gallery arrow + crop markup and "Add to prompt" | missing | - | - |
| 189 | [DESKTOP] | Take Screenshot (⇧⌘S) into the session and attach it to the prompt | missing | - | - |
| 190 | [DESKTOP] | Full main menu set (App/Session/Edit/View/Window/Help) with shortcuts | missing | - | - |
| 191 | [DESKTOP] | Menu-bar status item with activity spinner and popover | missing | - | - |
| 192 | [DESKTOP] | Keep running as a menu-bar agent when the window closes | missing | - | - |
| 193 | [DESKTOP] | Finder "New Unpeel Session Here" service | missing | - | - |
| 194 | [DESKTOP] | Sparkle auto-updates with a beta channel opt-in | missing | - | - |
| 195 | [DESKTOP] | macOS Notification Center banners (needs input / finished / App alerts) plus a test notification | missing | - | - |
| 196 | [DESKTOP] | Keychain-backed Link license | missing | - | - |
| 197 | [DESKTOP] | Bundled Host service lifecycle management via launchd | missing | - | - |
| 198 | [DESKTOP] | Bonjour nearby-host discovery for Add Workspace | missing | - | - |
| 199 | [DESKTOP] | Remote folder picker for launching on remote Hosts | missing | - | - |
| 200 | [DESKTOP] | Settings scope picker (This Mac / workspace / remote Host) with inherit/reset | missing | - | - |
| 201 | [DESKTOP] | Settings ▸ Workspaces (unified list, add local/nearby/code/SSH, rename, color, forget, delete) | missing | - | - |
| 202 | [DESKTOP] | Settings ▸ Agents (install CLI, install/reinstall integration, commands and variants, default, activate, reorder) | missing | - | - |
| 203 | [DESKTOP] | Settings ▸ Plugins (Apps catalog install/update/activate/order) | missing | - | - |
| 204 | [DESKTOP] | Settings ▸ Agent access ▸ Sessions (write policy, worktree permission, auto-gallery, approved pairs/Apps with Revoke) | missing | - | - |
| 205 | [DESKTOP] | Settings ▸ Agent access ▸ Browser (engine status, access mode, approvals, window/cursor/scope/app path, site rules, clear data) | missing | - | - |
| 206 | [DESKTOP] | Settings ▸ Appearance (mode, 8 accent colors, background/surface/transparency, terminal font and size, line height, session title mode, gallery chip) | missing | - | - |
| 207 | [DESKTOP] | Settings ▸ Remote Control (share this Mac/workspace QR/code, paired devices, revoke, per-device Link toggle) | missing | - | - |
| 208 | [DESKTOP] | Add iPhone/iPad to a remote Host (controller-assisted pairing) | missing | - | - |
| 209 | [DESKTOP] | Unpeel Link license section (activate, seats, release seat, get Link) | missing | - | - |
| 210 | [DESKTOP] | Settings ▸ Transcripts (content toggles, info header, range) | missing | - | - |
| 211 | [DESKTOP] | Settings ▸ Notifications (flag select menus, completion, test Mac/phone, delivery diagnostics) | missing | - | - |
| 212 | [DESKTOP] | Settings ▸ Worktrees ("Show agent worktrees", list with create/reveal/remove) | missing | - | - |
| 213 | [DESKTOP] | Settings ▸ Features (Remote workspaces, Git worktrees, Sessions use, Workspaces, Browser use) | missing | - | - |
| 214 | [DESKTOP] | Settings ▸ Advanced (auto-archive cleanup, sidebar archive preview, memory, running hosts by CPU with Stop, sessions folder, trace log) | missing | - | - |
| 215 | [DESKTOP] | Default editor / opener preference | missing | - | - |
| 216 | [DESKTOP] | Presets stored in the shared app-state.json with live pickup of CLI edits | missing | - | - |
| 217 | [DESKTOP] | Remote Host scope uses the same sidebar/content UI with Direct→Link automatic fallback | missing | - | - |
| 218 | [DESKTOP] | Worktree discovery of agent-created checkouts (opt-in, every 5 s) | missing | - | - |
| 219 | [DESKTOP] | Startup presentation cache for instant sidebar | missing | - | - |
| 220 | [IOS] | QR scan or paste-code pairing; multiple paired workspaces with switcher; Forget | missing | - | - |
| 221 | [IOS] | Direct pinned-HTTPS LAN transport plus Link relay E2E transport with credential repair | missing | - | - |
| 222 | [IOS] | Mac-style sessions drawer (projects, sessions, activity pills, ages, workspace header) | missing | - | - |
| 223 | [IOS] | Presets drawer to start a new session on the Host | missing | - | - |
| 224 | [IOS] | Live Ghostty terminal via WSS (HTTP long-poll fallback) with snapshot baseline and resync | missing | - | - |
| 225 | [IOS] | Keyboard never resizes the remote grid; "Fit terminal to screen" and revert to desktop | missing | - | - |
| 226 | [IOS] | Extra-keys accessory bar (esc/ctrl/alt/cmd/tab/arrows/symbols/paste/backspace/hide) | missing | - | - |
| 227 | [IOS] | Select-menu control bar (↑/↓/esc/return) for agent menus | missing | - | - |
| 228 | [IOS] | Mosh-style predictive echo and predictive scrolling; remote mouse-wheel; pinch zoom; long-press text selection | missing | - | - |
| 229 | [IOS] | Push-to-talk dictation with live transcript (paste/discard) | missing | - | - |
| 230 | [IOS] | Optional Apple Intelligence "polish" of dictation (iOS 26+) | missing | - | - |
| 231 | [IOS] | In-session Allow / Don't Allow approval prompts (first answer wins) | missing | - | - |
| 232 | [IOS] | APNs push registration per workspace, retry, tap routes to the session | missing | - | - |
| 233 | [IOS] | Title-bar activity bell with active sessions panel | missing | - | - |
| 234 | [IOS] | Exited-session restart bar (Resume) | missing | - | - |
| 235 | [IOS] | Session organize sheet (rename, notify when done, copy transcript Markdown 20/50/whole, restore/remove, resume) | missing | - | - |
| 236 | [IOS] | Project organize sheet (rename group, sort custom/date, folder color, archive library) | missing | - | - |
| 237 | [IOS] | Archived sessions sheet with Restore / Restore & Resume | missing | - | - |
| 238 | [IOS] | Session gallery with pinch-zoom full-size view and delete | missing | - | - |
| 239 | [IOS] | Upload photos into the session (resumable, JPEG/PNG transcode) | missing | - | - |
| 240 | [IOS] | Arrow/crop and PencilKit freehand image annotation, then "Add to message" | missing | - | - |
| 241 | [IOS] | Request-screenshot action (prompts the agent, polls, opens gallery) | missing | - | - |
| 242 | [IOS] | Face ID / Touch ID / passcode app lock | missing | - | - |
| 243 | [IOS] | Session actions from phone (stop, restart, resume agent, archive, remove, reorder, mark read) | missing | - | - |
| 244 | [IOS] | Capability-gated UI driven by Host bootstrap; connection-lost and push-warning banners; reconnect backoff | missing | - | - |
| 245 | [SHARED] | One Swift implementation of Host protocol DTOs, pairing client and paired-host records for Mac and iOS | missing | - | - |
| 246 | [SHARED] | Relay forward-secret E2E protocol and WebSocket client, pinned by cross-language KAT vectors | missing | - | - |
| 247 | [PROTO] | Versioned Host capability ledger (52 op ids, major 1 / minor 21, additive, capability-checked) | missing | - | - |
| 248 | [PROTO] | Host conformance and bootstrap-compatibility fixtures that every Host implementation must pass | missing | - | - |
| 249 | [PROTO] | Normative pane-layout operations, direct-path v1, relay KAT, browser-engine pin and App registry contracts | missing | - | - |
| 250 | [PROTO] | Unpeel UI protocol v1 (NDJSON App-to-Host semantic UI) schema, stream and fixtures | missing | - | - |
| 251 | [DIST] | SHA-256-verified curl installers for the CLI and each App; channels alpha/beta/stable on R2 | missing | - | - |
| 252 | [DIST] | Lockstep app/CLI versioning; CLI archives carry protocol/, generated/, provenance and notices | missing | - | - |
| 253 | [DIST] | Signed and notarized Mac DMG plus Sparkle appcast; iOS via TestFlight | missing | - | - |
