# Parity Audit: [IOS], [APPS], [WEB] (45 items)

Audited against `track-b-parity-docs@de51bb2`. Method: `git grep` on the renamed
supercli code, reading source files. No builds. Strict: `done` requires the
feature to exist and work in supercli today; `partial` means the Rust/backend
logic exists but the surface (gpuidart UI, mobile shell) is not there.

## Summary

| Tag | Total | Done | Partial | Missing | Improved |
|-----|-------|------|---------|---------|----------|
| [APPS] | 17 | 3 | 13 | 1 | 0 |
| [WEB] | 3 | 1 | 0 | 2 | 0 |
| [IOS] | 25 | 0 | 0 | 25 | 0 |
| **Total** | **45** | **4** | **13** | **28** | **0** |

## [APPS] (130-146)

| # | Tag | Status | supercli Path | Test | Notes |
|---|-----|--------|---------------|------|-------|
| 130 | [APPS] | done | `crates/supercli-attach/src/lib.rs` | `focus_filter_drops_exact_focus_in_and_out`, `resize_command_encoding_matches_host_protocol` | Attach client exists: snapshot baseline, event-driven follow, focus-event filtering (always on unless `--forward-focus-events`), resize encoding. |
| 131 | [APPS] | done | `crates/supercli-apps/src/agent.rs` | (unit tests in `agent.rs`) | `send_to_adjacent_agent(token)` resolves the best agent pane via MCP client; `clipboard_sequence(text)` is the OSC 52 fallback. |
| 132 | [APPS] | done | `crates/supercli-core/src/app_open.rs`, `apps_mcp.rs`, `controller_host.rs` | `typed_resources_are_one_shell_safe_argument`, `opener_projection_migrates_legacy_file_keys_and_keeps_typed_resources` | Official App registry with typed resources (`resource:git.working-tree`, `resource:folder`, media types). Agents cannot install Apps. |
| 133 | [APPS] | partial | `crates/supercli-core/src/app_open.rs`, `mcp_host.rs` | — | Backend exists: companion Session startup (`app_open.rs:262`), pane-neighbour context JSON (`mcp_host.rs:4393`). No gpuidart UI for companion panes or App-branded session rows. |
| 134 | [APPS] | partial | `crates/apps/diffs/src/app.rs`, `ui.rs`, `git.rs` | — | Git App Changes tab with status glyphs and unified diffs exists as a Rust TUI. Not on gpuidart. |
| 135 | [APPS] | partial | `crates/apps/diffs/src/highlight.rs` | — | Syntax-highlighted patches via `two_face::syntax` exist as Rust TUI. Not on gpuidart. |
| 136 | [APPS] | partial | `crates/apps/diffs/src/app.rs` (`Tab::History`, `Screen::History`) | — | History tab (paged commits, commit files, patches) exists as Rust TUI. Not on gpuidart. |
| 137 | [APPS] | partial | `crates/apps/diffs/src/git.rs` (`RemoteAction::Fetch`, pull, `push --no-force --no-mirror`) | — | Fetch/Pull/Push exists as Rust TUI; push uses `--no-force` (fast-forward only). Not on gpuidart. |
| 138 | [APPS] | partial | `crates/apps/diffs/src/ui.rs` (`select-diff-lines`, `copy-lines`) | — | Select-lines + Copy lines actions exist as Rust TUI. Not on gpuidart. |
| 139 | [APPS] | partial | `crates/apps/filetree/src/ui.rs` | — | Scoped explorer with filter, context menu (`context_menu`), path drag (`DragSurface::detect`) exists as Rust TUI. Not on gpuidart. |
| 140 | [APPS] | partial | `crates/apps/diffs/src/app.rs:153`, `filetree/src/ui.rs:35` | — | "Follow a neighboring/main agent into another checkout" exists in diffs and filetree TUIs. Usage app follow not verified. Not on gpuidart. |
| 141 | [APPS] | partial | `crates/apps/markdown/src/` (`slash.rs`, `mouse.rs`, `highlight.rs`, block/heading/format) | — | Live-styled editor with slash menu, mouse editing exists as Rust TUI. Not on gpuidart. |
| 142 | [APPS] | partial | `crates/apps/markdown/src/` (`backend.rs`, `picker.rs`, `start.rs`) | — | Auto-save backend, vault/note picker, remembered notes folder exist as Rust TUI. Not on gpuidart. |
| 143 | [APPS] | partial | `crates/apps/usage/src/` (`claude.rs`, `codex.rs`, `grok.rs`, `muse.rs`, `ui.rs`) | — | Per-provider quota gauges, token history exist as Rust TUI. Not on gpuidart. |
| 144 | [APPS] | partial | `clients/legacy/app-kit` (via bridge in `crates/apps/*/Cargo.toml`) | — | App Kit component library exists but is the FROZEN Swift/Rust app-kit bridged via `path = "../../../clients/legacy/app-kit"`. Must be replaced by gpuidart widgets (P1-1). |
| 145 | [APPS] | partial | `clients/legacy/app-kit` (via bridge) | — | Hosted semantic UI bridge exists in the bridged app-kit. Same replacement requirement as 144. |
| 146 | [APPS] | missing | — | — | SwiftUI peer renderer + Kitchen Sink: Swift code, frozen under `clients/legacy/`. No Rust/gpuidart equivalent. |

## [WEB] (147-149)

| # | Tag | Status | supercli Path | Test | Notes |
|---|-----|--------|---------------|------|-------|
| 147 | [WEB] | missing | — | — | No TypeScript/DOM/ARIA web renderer exists outside `clients/legacy/`. |
| 148 | [WEB] | done | `scripts/install.sh`, `scripts/install-app.sh` | — | Installers exist with channel selection: `SUPERCLI_CHANNEL` must be `alpha\|beta\|stable` (`install.sh:16,28`). Per-App installer exists. |
| 149 | [WEB] | missing | — | — | No `unpeel.com/docs`, `/download/mac`, `/ios` help/download links found in apps or CLI source. |

## [IOS] (220-244)

All 25 items are **missing**. `clients/supercli-app/` on this branch contains
`lib/app.dart`, `lib/host_client.dart`, `lib/models.dart` — no mobile screens,
no iOS-specific glue. iOS is planned as Rust core + gpuidart mobile once
gpuidart ships mobile shells (P1). The Swift iOS app is frozen under
`clients/legacy/`.

| # | Tag | Status | supercli Path | Test | Notes |
|---|-----|--------|---------------|------|-------|
| 220 | [IOS] | missing | — | — | QR/paste-code pairing, workspace switcher: no mobile UI. (Host-side pairing exists; this item is the iOS surface.) |
| 221 | [IOS] | missing | — | — | iOS transport UI: no mobile shell. (Rust transport core exists in `supercli-client`.) |
| 222 | [IOS] | missing | — | — | Sessions drawer: no mobile UI. |
| 223 | [IOS] | missing | — | — | Presets drawer: no mobile UI. |
| 224 | [IOS] | missing | — | — | Live Ghostty terminal via WSS: no mobile UI. (P0-8/P0-10 terminal widget is the desktop prerequisite.) |
| 225 | [IOS] | missing | — | — | Fit-terminal-to-screen: no mobile UI. |
| 226 | [IOS] | missing | — | — | Extra-keys accessory bar: no mobile UI. |
| 227 | [IOS] | missing | — | — | Select-menu control bar: no mobile UI. |
| 228 | [IOS] | missing | — | — | Predictive echo/scrolling, pinch zoom, long-press selection: no mobile UI. |
| 229 | [IOS] | missing | — | — | Push-to-talk dictation: no mobile UI. (objc2 speech bindings exist in `supercli-native-bridge` for macOS; iOS shell missing.) |
| 230 | [IOS] | missing | — | — | Apple Intelligence dictation polish: no mobile UI; iOS 26+ API. |
| 231 | [IOS] | missing | — | — | In-session approval prompts: no mobile UI. (ApprovalHub exists host-side.) |
| 232 | [IOS] | missing | — | — | APNs push per workspace: objc2 push bindings exist (`supercli-native-bridge/src/platform/push.rs`, iOS-gated) but no iOS app shell to register/deliver. |
| 233 | [IOS] | missing | — | — | Activity bell: no mobile UI. |
| 234 | [IOS] | missing | — | — | Exited-session restart bar: no mobile UI. |
| 235 | [IOS] | missing | — | — | Session organize sheet: no mobile UI. |
| 236 | [IOS] | missing | — | — | Project organize sheet: no mobile UI. |
| 237 | [IOS] | missing | — | — | Archived sessions sheet: no mobile UI. |
| 238 | [IOS] | missing | — | — | Session gallery: no mobile UI. |
| 239 | [IOS] | missing | — | — | Photo upload: no mobile UI. (Artifact store exists host-side.) |
| 240 | [IOS] | missing | — | — | Image annotation (PencilKit): no mobile UI; PencilKit has no gpuidart equivalent logged yet. |
| 241 | [IOS] | missing | — | — | Request-screenshot: no mobile UI. |
| 242 | [IOS] | missing | — | — | Face ID / Touch ID / passcode lock: no mobile UI. |
| 243 | [IOS] | missing | — | — | Session actions from phone: no mobile UI. (MCP/host APIs exist.) |
| 244 | [IOS] | missing | — | — | Capability-gated UI, banners, reconnect backoff: no mobile UI. |
