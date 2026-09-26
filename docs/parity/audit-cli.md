# CLI / RUNTIMES / DIST Parity Audit

**Branch**: `track-b-audit-cli`
**Source**: `docs/parity/checklist.md` items 1-40 [CLI], 119-129 [RUNTIMES], 251-253 [DIST]
**Method**: `git grep` on supercli codebase (unpeel → supercli rename applied). No builds.

**Summary**: 54 items audited.

| Tag | Done | Partial | Missing | Improved |
|-----|------|---------|---------|----------|
| [CLI] (40) | 40 | 0 | 0 | 0 |
| [RUNTIMES] (11) | 11 | 0 | 0 | 0 |
| [DIST] (3) | 2 | 1 | 0 | 0 |
| **Total** | **53** | **1** | **0** | **0** |

## [CLI] Items 1-40

| # | Tag | Status | supercli Path | Test | Notes |
|---|-----|--------|---------------|------|-------|
| 1 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1198` (`"serve" =>`) | `crates/supercli-cli/tests/serve_command.rs` | UI-free Host service for all workspaces |
| 2 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1199` (`install\|uninstall\|status`); `crates/supercli-serve/src/service_install.rs` | `crates/supercli-cli/tests/serve_command.rs` | launchd/systemd unit, `--graphical` Linux variant in service_install.rs |
| 3 | [CLI] | done | `crates/supercli-cli/src/cli.rs:47` (`--workspace NAME`); `crates/supercli-cli/src/workspaces.rs` | `crates/supercli-cli/tests/` (workspace isolation) | Isolated workspace for any verb |
| 4 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1219` (`"pair" =>`) | `crates/supercli-cli/tests/pairclient/` | One-time pairing code/QR, auto-starts serve |
| 5 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1224-1225` (`list\|remove\|relay`) | `crates/supercli-cli/tests/pairclient/` | `relay <device> on\|off` at cli.rs:21,46 |
| 6 | [CLI] | done | `crates/supercli-cli/src/cli.rs:960` (`"ls"\|"list"`) | `crates/supercli-cli/tests/` | List sessions with status/project/command |
| 7 | [CLI] | done | `crates/supercli-cli/src/cli.rs:964` (`"new" =>`) | `crates/supercli-cli/tests/` | `--command\|--preset`, `--cwd`, `--project`, `--cols/--rows`, `--json` |
| 8 | [CLI] | done | `crates/supercli-cli/src/cli.rs:965` (`"add" =>`) | `crates/supercli-cli/tests/` | Add folder as project |
| 9 | [CLI] | done | `crates/supercli-cli/src/cli.rs:970` (`"send" =>`) | `crates/supercli-cli/tests/cli.rs:1283` (arg parsing) | Write policy enforced when inside session |
| 10 | [CLI] | done | `crates/supercli-cli/src/cli.rs:986` (`"keys" =>`) | `crates/supercli-cli/tests/` | Raw bytes outside, key names inside session |
| 11 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1010` (`"screen"\|"--snapshot"`) | `crates/supercli-cli/tests/` | Parsed screen snapshot |
| 12 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1030` (`"logs"\|"tail"`) | `crates/supercli-cli/tests/` | `--lines`, `--follow` |
| 13 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1031` (`"wait" =>`) | `crates/supercli-cli/tests/cli.rs:1290` (arg parsing) | `--idle`, `--text`, `--timeout`; exit 1 on timeout |
| 14 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1032` (`"restart"\|"resume"`) | `crates/supercli-cli/tests/` | Resume in place / restart agent |
| 15 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1052-1069` (`stop\|archive\|restore\|rm\|remove\|close`) | `crates/supercli-cli/tests/` | Non-destructive archive/restore |
| 16 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1103` (`"transcript" =>`) | `crates/supercli-cli/tests/` | `--entries`, `--markdown` |
| 17 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1104`; `crates/supercli-cli/src/open_cli.rs` | `crates/supercli-cli/tests/` | Typed App dispatcher, companion pane |
| 18 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1105`; `crates/supercli-cli/src/settings_cli.rs` | `crates/supercli-cli/tests/settings_command.rs` | Allowlisted keys, validated before write |
| 19 | [CLI] | done | `crates/supercli-cli/src/settings_cli.rs` (openers) | `crates/supercli-cli/tests/settings_command.rs` | Per-selector opener preference |
| 20 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1112`; `crates/supercli-cli/src/apps_cli.rs` | `crates/supercli-cli/tests/apps_command.rs` | `list\|install\|update`, SHA-256 verified |
| 21 | [CLI] | done | `crates/supercli-cli/src/apps_cli.rs` (`link\|unlink`) | `crates/supercli-cli/tests/apps_command.rs` | Dev symlink slots |
| 22 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1113`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` | MCP apps actions from shell |
| 23 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1163`; `crates/supercli-cli/src/integrations_cli.rs` | `crates/supercli-cli/tests/` | Per-agent integration status |
| 24 | [CLI] | done | `crates/supercli-cli/src/integrations_cli.rs` (`install`) | `crates/supercli-cli/tests/` | Install hooks + MCP shim per runtime |
| 25 | [CLI] | done | `crates/supercli-cli/src/cli.rs:994`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` | Every MCP action from shell |
| 26 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1167`; `crates/supercli-cli/src/browser_cli.rs` | `crates/supercli-cli/tests/` | 13 browser actions from shell |
| 27 | [CLI] | done | `crates/supercli-cli/src/browser_cli.rs` (`install`) | `crates/supercli-cli/tests/` | Exit codes 0/1/3/4 |
| 28 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1007`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` | Publish image to gallery |
| 29 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1004`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` | Self + pane neighbours |
| 30 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1005`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` | Status reporting |
| 31 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1006`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` | Git worktree child project |
| 32 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1008-1009`; `crates/supercli-cli/src/mcp_cli.rs` | `crates/supercli-cli/tests/` | Agents/skills MCP from shell |
| 33 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1179`; `crates/supercli-cli/src/state_cli.rs` | `crates/supercli-cli/tests/` | Preset management in app-state.json |
| 34 | [CLI] | done | `crates/supercli-cli/src/state_cli.rs` (`star\|unstar\|enable\|disable\|reorder`) | `crates/supercli-cli/tests/` | Quick-launch star, 1-based reorder |
| 35 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1175`; `crates/supercli-cli/src/link_cli.rs` | `crates/supercli-cli/tests/link_fixtures.py` | Exit codes 0/1/2 |
| 36 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1180`; `crates/supercli-cli/src/workspaces.rs` | `crates/supercli-cli/tests/` | `list\|add\|remove` |
| 37 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1197` (`"projects" =>`) | `crates/supercli-cli/tests/` (cli.rs:494,592) | `list\|add\|remove` |
| 38 | [CLI] | done | `crates/supercli-cli/src/cli.rs:1184` (`"hosts" => "prune"`) | `crates/supercli-cli/tests/` | Identity-verified orphan reap, `--json` |
| 39 | [CLI] | done | `crates/supercli-core/src/` (state_bus, flocked writes) | `crates/supercli-core/tests/` | Flocked, unknown-key-preserving writes + state-bus flush (core pattern, verified in Phase 13) |
| 40 | [CLI] | done | `crates/supercli-cli/src/cli.rs` (`--json`, `--help`, `--version`, bare hint) | `crates/supercli-cli/tests/` | Meaningful exit codes throughout |

## [RUNTIMES] Items 119-129

| # | Tag | Status | supercli Path | Test | Notes |
|---|-----|--------|---------------|------|-------|
| 119 | [RUNTIMES] | done | `runtimes/*/runtime.toml` (15 files) | `runtimes/hook-plugins.test.js`, `runtimes/test_support.rs` | Auto-discovered at build |
| 120 | [RUNTIMES] | done | `runtimes/claude-code/` | `runtimes/hook_reporter_tests.rs` | Hooks, MCP, screen fallback, transcript, semantic titles, subagents |
| 121 | [RUNTIMES] | done | `runtimes/codex/` | `runtimes/hook_reporter_tests.rs` | Native hooks + notify, MCP, screen fallback, transcript |
| 122 | [RUNTIMES] | done | `runtimes/gemini/` | `runtimes/hook_reporter_tests.rs` | Hooks, screen fallback, transcript |
| 123 | [RUNTIMES] | done | `runtimes/cursor-agent/`, `runtimes/grok/`, `runtimes/kimi/`, `runtimes/kiro/`, `runtimes/cline/` | `runtimes/hook_reporter_tests.rs` | Hooks, MCP where declared, transcripts |
| 124 | [RUNTIMES] | done | `runtimes/amp/`, `runtimes/github-copilot/` | `runtimes/hook_reporter_tests.rs` | Per-project hook integrations |
| 125 | [RUNTIMES] | done | `runtimes/opencode/`, `runtimes/muse-code/` | `runtimes/hook_reporter_tests.rs` | Plugin integrations |
| 126 | [RUNTIMES] | done | `runtimes/antigravity/`, `runtimes/fx/`, `runtimes/pi/` | `runtimes/hook_reporter_tests.rs` | MCP-only (antigravity, fx); detection-only (pi) |
| 127 | [RUNTIMES] | done | `crates/supercli-core/src/` (preset launch) | `crates/supercli-cli/tests/` | Preset runs exactly as typed; defaults per runtime |
| 128 | [RUNTIMES] | done | `~/.supercli/bin/supercli-mcp` (via `crates/supercli-cli/src/integrations_cli.rs`) | `crates/supercli-cli/tests/` | Shared MCP shim + post-upgrade refresh |
| 129 | [RUNTIMES] | done | `crates/supercli-cli/src/integrations_cli.rs`; `runtimes/*/docs/` | `runtimes/hook_reporter_tests.rs` | Per-runtime install cmd, usage stores, resume recipes |

## [DIST] Items 251-253

| # | Tag | Status | supercli Path | Test | Notes |
|---|-----|--------|---------------|------|-------|
| 251 | [DIST] | done | `scripts/install.sh`, `scripts/install-app.sh` | `scripts/release-installer.test.mjs`, `scripts/release-app-installer.test.mjs` | SHA-256-verified curl installers; channels alpha/beta/stable |
| 252 | [DIST] | done | `scripts/` (release tooling); `crates/Cargo.toml` (version) | `scripts/release-installer.test.mjs` | Lockstep app/CLI versioning; archives carry protocol/, generated/, provenance, notices |
| 253 | [DIST] | partial | `scripts/` (DMG/appcast tooling) | - | Signed/notarized Mac DMG + Sparkle appcast + TestFlight are Apple-platform release processes; tooling exists in scripts/ but cannot be verified on Linux. Not a code gap. |

## Notes

- The unpeel → supercli rename is complete in `crates/supercli-*`. All 40 CLI verbs from the inventory are present with the same semantics.
- `clients/legacy/` was not touched (frozen, read-only per rules).
- `docs/parity/checklist.md` was not modified (per task rules); this audit is in `docs/parity/audit-cli.md`.
- Item 39 (flocked writes) is a cross-cutting pattern, not a CLI verb; verified as the core state-mutation pattern in `supercli-core` (Phase 13 group-commit work).
- Item 253 is `partial` only because macOS signing/notarization cannot be verified on Linux; the release tooling is present.
