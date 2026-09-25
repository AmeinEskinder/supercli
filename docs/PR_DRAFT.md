# Track B Phase 3: Hardening — faithful web flows, device validation, move verb, regression

## Summary

Hardening pass over the Phase 2 Dioxus rewrite:

**H1 — Playwright mobai-mirror:** Web bundle rebuilt (`dx build --platform web`,
173 crates). All 5 MobAI flows are now faithful end-to-end transitions:
pairing → Sessions tab; session row → Terminal; gallery → detail → Draw
editor → annotation-done (test ID added to all three editors); dictation
toggle exercises the control plane with stubbed Web Speech API.
**Result: 5/5 pass** in headless Chromium (Chrome for Testing 153);
full web suite **10/10 pass** (exit 0; `demo.spec.js` fixed for demo-data
drift — single "Demo Mac" host, "harness build" session, pairing now
transitions to Sessions).

**H2 — mobai-ci validate:** Installed pinned `mobai-ci` 0.6.0 via the official
installer to `~/.local/opt/mobai/`. Rewrote all five
`tests/device/*.mob` flows to the documented `@"..."` accessibility-ID format.
**Result: 5/5 OK** (parser validity; device accessibility unverified without
hardware).

**H3 — Portable session→project move:** Implemented end-to-end. Host accepts
`projectID` on `/mobile/session-organization` with validation; `unpeel-client`
`move_session_to_project` + `SESSION_PROJECT_SET` capability +
`supports_session_project_move` gate; `ProjectSummary` DTO +
`move_destinations` mirror Swift's `SessionMoveRules.destinations`; mobile
organize sheet offers the capability-gated "Move to" picker.
**Audit tally: 169 ported / 70 superseded / 11 blocked** (accepted 2026-09-22).
The workspace move (ProjectWorkspaceMove) stays blocked: same-Mac disk
transfer, not a portable Controller verb.

**H4 — Regression sweep:** Main workspace `cargo test --workspace` — all green
except 2 pre-existing environmental failures (raw-UDP punch test in sandbox;
signing test flaky in parallel, 3/3 in isolation — I did not touch signing).
`unpeel-ui`: 271/271. Clippy `-D warnings` clean on both workspaces (fixed 8
pre-existing style warnings). Fmt clean. Both launchers type-check 0 errors
(temp crate). Playwright 10/10.

**H5 — Docs:** HANDOFF.md + this PR draft corrected with verified numbers.

## Diff stat (all Track B changes, tracked + untracked)

**134 files, 61,876 insertions, 226 deletions** (`git add -N .` +
`git diff --numstat`, excluding `clients/dioxus/target/` build artifacts;
intent-to-add undone via `git reset`):
- Tracked (12 files): 1,017 insertions, 226 deletions
- Untracked source (122 files): 60,859 insertions — the 12-tracked number is
  right because nearly all Track B work lives in untracked dirs
  (`clients/dioxus/`, `crates/unpeel-client/`, `crates/unpeel-connector/`,
  `tests/device/`, new docs and workflows)

**Nothing committed or pushed** (per standing rule).

## Test report
| Suite | Result |
|---|---|
| Main workspace `cargo test --workspace` | Pass except 2 pre-existing environmental (UDP punch; signing flake 3/3 isolated) |
| `unpeel-ui` | 271/271 pass |
| `unpeel-client` (lib + integration) | 54 lib + all suites pass |
| `organization_e2e` (incl. new move-verb test) | 8/8 pass |
| Clippy `-D warnings` (both workspaces) | Clean |
| `cargo fmt --check` (both workspaces) | Clean |
| Launcher type-check (temp crate) | 0 errors both |
| Playwright `mobai-mirror.spec.js` | 5/5 pass |
| Playwright full web suite | 10/10 pass |
| `mobai-ci validate tests/device` | 5/5 OK |

## Known limitations
Full native launcher builds blocked (no GTK/WebKit dev libs; apt 407).
Real-device testing not done. iOS push OS-token acquisition needs the Mac
shell build. The signing test flake and UDP test failure are pre-existing
environmental issues, not Phase 3 regressions.

---

## Phase 4/5 — Stored action reviews + lease fencing (accepted 2026-09-22)

**Phase 4:** uncertain external writes are never auto-retried (P5-1);
durable write-ahead review (fsync'd JSONL before tool execution, fail-closed
on write failure), per-session hash-chain tamper evidence with a
flip-one-byte detection test, and explicit actors on every path (P5-2).

**Phase 5 addendum:** lease fencing — monotonic `lease_generation` with
conditional-SQL claim/takeover, tombstone release, per-store owner ids;
`ToolCallFailure::stale_lease` refused at four gates before any side effect;
DB-side expiry clock (`julianday` millisecond expression; Postgres/D1 notes);
takeover of in-flight/`ambiguous` work escalates to `needs_review` with
human notification instead of re-firing. Verified post-restart:
`unpeel-core` 907 passed / 1 environmental failure (sandbox UDP
`PermissionDenied`; module untouched).

**Phase 6 R1:** 13 clippy/rustc denials fixed across 4 crates;
`cargo clippy --workspace --all-targets --all-features -- -D warnings`
clean on `crates/`; actionlint clean (one false positive on the real
`macos-26` runner label, fixed via `.github/actionlint.yaml`).

**W2 self-correction:** the manual APK script's Kotlin-compile and d8-dex
stages are placeholders, not real builds — flagged by me during W2 closeout
and recorded so the limitation is not lost. Real APK assembly still needs
the Gradle path (blocked in sandbox) or an Android SDK machine.

## Diff stat (all Track B changes, tracked + untracked — Phase 6)

**145 files, 65,830 insertions, 247 deletions** (`git add -A -N` +
`git diff --numstat HEAD`, excluding `clients/dioxus/target/` and `out/`).

**Nothing committed or pushed** (per standing rule).

---

## Final closeout (2026-09-23, accepted)

**Closeout corrections:** composer fixes (queued-edit input `type="text"`,
hooks-in-loop fix, per-session draft restoration); desktop launcher
regressions owned via `git blame` (`gloo_timers` → `std::thread::sleep`,
`selected_session`, `sessions.clone()`); poll-loop bug caught by review —
blocking sleep inside a Dioxus async spawn would have stalled the UI thread
every 2s, fixed with a dedicated `std::thread` + `SyncSignal`
(`use_signal_sync`; plain `Signal` is `!Send`).

**Final regression (serial):** root workspace `cargo test --workspace`
green except 4 environmental UDP-punch failures (pre-existing;
`direct_path.rs` untouched — previously masked by fail-fast);
`unpeel-ui` 275/275, `unpeel-ios-bridge` 3/3; clippy `-D warnings` clean
(`crates/` `--all-targets --all-features`; `unpeel-ui`; `unpeel-ios-bridge`);
`cargo fmt --check` clean on both workspaces (was dirty in 6 spots, fixed);
actionlint 1.7.7 clean; Playwright **13/13** on a rebuilt bundle (Chrome for
Testing 153). Launcher `cargo check` blocked: the VM rebooted ~00:54,
wiping the tmpfs `/nix/store` — re-provision from the Nix binary cache to
restore (see `docs/KNOWN_GOOD_ENVIRONMENTS.md`).

**Diff stat (final): 156 files, 69,203 insertions, 250 deletions** (`git diff --numstat 7f2f5a3`, excluding `out/` and `clients/dioxus/target/`)
Nothing committed or pushed (per standing rule).

## What Amein must do
- [ ] Review the 8 patches in `out/patches/` (apply order in `out/patches/README.md`).
- [ ] Commit and push — nothing has been committed (standing rule).
- [ ] Apple: provide the Team ID, App Store Connect API key, APNs .p8, and bundle ID before any device/TestFlight run.
- [ ] MobAI: provide the MobAI key and a Host pair code before device smoke runs.
- [ ] Re-provision /nix (or build on a machine with GTK/WebKit dev libs) to restore native launcher check/build.
- [ ] Known gap: the Host has no protocol-level turn-cancel verb. Desktop Stop sends raw Ctrl-C (terminal fallback, labeled as such); mobile has no Stop button. A real solution needs an additive Host interrupt verb — Host-first, capability-advertised, conformance-aligned.
