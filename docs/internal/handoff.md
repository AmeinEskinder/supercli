# Track B Handoff — Phase 3 Hardening

**Date:** 2026-09-22
**Status:** Phase 3 hardening complete. All five items verified; closeout
report posted. Nothing committed or pushed (per standing rule).

## Audit tally (accepted 2026-09-22)
- **Ported:** 169
- **Superseded:** 70
- **Blocked:** 11

Phase 3 H3 implemented the portable session→project move verb end-to-end
(Host accepts `projectID` on `/mobile/session-organization`;
`unpeel-client` `move_session_to_project` + `SESSION_PROJECT_SET` capability
+ `supports_session_project_move` gate; `dto::ProjectSummary` +
`move_destinations` mirror Swift's `SessionMoveRules.destinations`; mobile
organize sheet offers the "Move to" picker). The audit table split
SessionMoveRules.swift into two rows (filing rules + move verb), both ported.
The workspace move (ProjectWorkspaceMove) stays blocked: it is a same-Mac
disk transfer between two UNPEEL_HOME trees, not a portable Controller verb.

## Phase 3 results

### H1 — Playwright mobai-mirror: COMPLETE (faithful flows)
Web bundle rebuilt successfully (`dx build --platform web`, 173 crates).
All 5 flows are now faithful end-to-end transitions, not shallow renders:
- **pair**: demo code `UNPEEL:1:demo` transitions to the Sessions tab (like the
  real mobile launcher after pairing); other codes fail honestly.
- **open-session**: tapping a session row selects it AND navigates to its
  Terminal (verified via `terminal-session-id` test ID).
- **terminal-type**: reached via the faithful pair → sessions → open-session
  path; typed keys echo through the real VT parser.
- **gallery-annotate**: screenshot adds a demo entry; opening it shows the
  real `GalleryDetailView`; the Draw editor opens; completing the annotation
  fires `annotation-done` (test ID added to all three editors: Arrows, Draw,
  Crop) and the demo records the result.
- **dictation-toggle**: Web Speech API + `getUserMedia` stubbed via
  addInitScript; the toggle exercises the control plane (start/stop) without
  a mic.

Result: **5/5 pass** in headless Chromium (Chrome for Testing 153 via
PLAYWRIGHT_CHROMIUM_PATH; /opt/meta-chromium blocked localhost).
Full web suite: **10/10 pass** (mobai-mirror 5/5 + demo 5/5), exit code 0.

Correction (2026-09-22, post-acceptance review): the interim "H1 blocked (no
browser binary)" note was wrong — the Chrome for Testing 153 binary from the
earlier item-4 run was intact at
`~/workspace/muse-harness/tmp/downloads/cft2/chrome-headless-shell-linux64/chrome-headless-shell`
and the checked-in `playwright.config.js` honors `PLAYWRIGHT_CHROMIUM_PATH`.
The full suite was re-run through the checked-in config with that override:
first run 8/10, the 2 failures being `demo.spec.js` drift (demo data now has
a single "Demo Mac" host, not "Studio Mac"; session renamed "harness build",
not "harness scheduler"; successful pairing now transitions to Sessions, so
the old "Pairing…" button-state expectation was replaced with a
session-list-visible assertion). After fixing the spec to the actual demo
behavior: **10/10 pass, exit 0.**

### H2 — mobai-ci validate: COMPLETE
Installed pinned `mobai-ci` 0.6.0 via the official installer
(`https://mobai.run/ci/install.sh`) to `~/.local/opt/mobai/mobai-ci`.
Rewrote all five `tests/device/*.mob` flows from invalid `testid:"..."`
selectors to the documented `@"..."` accessibility-ID format.
Validation: **5/5 OK** (dictation-toggle, gallery-annotate, open-session,
pair, terminal-type). This proves parser validity only; the relationship
between IDs and native device accessibility remains unverified without a
device.

Correction (2026-09-22, post-acceptance review): the interim "H2 blocked
(mobai-ci uninstallable)" note was wrong — the official installer URL was
live (HTTP 200, checksum-verified download from the MobAI-App/mobai-ci
GitHub release; the earlier 404 was a wrong URL form), and the 0.6.0 binary
was also still present from the prior install. It was reinstalled outside
/tmp to `~/.local/opt/mobai/` and `mobai-ci validate ./tests/device` re-ran
**5/5 OK** against the rewritten ID-based flows.

### H3 — Portable session→project move: COMPLETE (verified)
- **Host**: `/mobile/session-organization` accepts and validates `projectID`
  via `validate_session_project_target`; clears the override when targeting
  the manifest project, otherwise writes a project override. Capability
  `session.project.set` in `protocol/host-capabilities-v1.json` and both
  Host capability lists.
- **Client**: `HostClient::move_session_to_project` posts the exact body
  `{"sessionID":...,"projectID":...}`; `capabilities::SESSION_PROJECT_SET`
  + `supports_session_project_move` (missing descriptor = legacy Host,
  allowed; compatible descriptor requires explicit capability; incompatible
  major = refused). `ProjectSummary` DTO (accepts Host `isGroup` alias);
  `BootstrapSnapshot.projects`; `move_destinations` mirrors Swift's
  home-plus-plain-groups ordering.
- **UI**: `SessionOrganizePatch.project_id`; `SessionOrganizeSheet` accepts
  bootstrap projects; capability-gated "Move to project" selector.
- **Launcher**: mobile `save_organize_session` performs the standard
  organization patch then calls `move_session_to_project` when a destination
  was selected.

Verified: unpeel-client 54 lib + all integration suites green (incl. new
`move_session_to_project_sends_project_id` exact-body test);
unpeel-ui 271/271 (organize 8/8); both launchers type-check clean via temp
crate; unpeel-core controller_protocol 8/8 (ledger aligned); clippy/fmt
clean. Untested on a real device; full launcher builds still blocked
(no GTK/WebKit on this VM).

### H4 — Regression sweep: COMPLETE (with pre-existing failures noted)
- Main workspace (`crates/`): `cargo test --workspace` — all green except
  2 pre-existing environmental failures:
  - `direct_path_punch::tests::punch_over_real_udp_sockets` (raw UDP in sandbox)
  - `signing::tests::keygen_sign_verify_roundtrip` (flaky in full parallel run;
    passes 3/3 in isolation; I did not touch signing code)
- Dioxus: `unpeel-ui` 271/271 pass; `unpeel-web` 0 tests (binary crate).
  Full `cargo test --workspace` on Dioxus blocked by gdk-sys (no GTK dev libs —
  known environmental, not a code issue).
- Clippy: `unpeel-client` clean with `-D warnings`; `unpeel-ui` clean with
  `-D warnings` after fixing 8 pre-existing style warnings (sort_by_key,
  into_iter, derived Default, matches!, needless borrows, == false, vec!).
- Fmt: clean on both workspaces.
- Launcher type-check (temp crate): mobile 0 errors, desktop 0 errors
  (1 pre-existing unused `device` var warning in code I did not touch).
- Playwright: 10/10 pass (see H1).
- Separate target dirs used; private short-path UNPEEL_HOME for tests.

### H5 — Docs: COMPLETE (this file + PR_DRAFT.md + diff summary)

## Git status — full Track B diff stat (verified 2026-09-22)
Method: `git add -N . && git diff --numstat`, excluding
`clients/dioxus/target/` build artifacts (not gitignored in this repo;
~11,400 generated files), then `git reset` to undo the intent-to-add.
Nothing staged, nothing committed.

- **Total: 134 files, 61,876 insertions, 226 deletions**
  - Tracked (12 files): 1,017 insertions, 226 deletions —
    `.github/workflows/clients.yml`, `crates/Cargo.lock`,
    `crates/Cargo.toml`, `crates/unpeel-cli/Cargo.toml`,
    `crates/unpeel-cli/src/cli.rs`, `crates/unpeel-cli/src/main.rs`,
    `crates/unpeel-core/Cargo.toml`, `crates/unpeel-core/src/lib.rs`,
    `crates/unpeel-core/src/mcp_host.rs`,
    `crates/unpeel-serve/src/approvals.rs`,
    `crates/unpeel-serve/src/hook_listener.rs`, `docs/agents/cli.md`
  - Untracked source (122 files): 60,859 insertions, 0 deletions
- The 12-tracked-file number is correct: almost all Track B work lives in
  untracked dirs — `clients/dioxus/` (Dioxus UI, mobile/desktop/web
  launchers, vendored jsQR), `crates/unpeel-client/`,
  `crates/unpeel-connector/`, `tests/device/`, new docs
  (`docs/APPLE_SETUP.md`, `docs/DEVICE_TESTING.md`, `docs/HANDOFF.md`,
  `docs/PR_DRAFT.md`, `docs/connectors.md`), new workflows
  (`.github/workflows/apple.yml`, `android.yml`, `linux.yml`), and new
  CLI/core modules. Largest untracked files: vendored `jsqr-1.4.0.js`
  (10,102 lines), `clients/dioxus/Cargo.lock` (7,383),
  `crates/unpeel-cli/src/connectors_cli.rs` (3,616),
  `clients/dioxus/unpeel-mobile/src/main.rs` (3,238).
- **Nothing committed or pushed.**

## Known limitations
- Full native launcher builds blocked (no GTK/WebKit dev libs; apt 407)
- Real-device testing not done (no devices paired)
- The signing test flake and UDP test failure are pre-existing environmental
  issues, not regressions from Phase 3 work

---

# Phase 4 — Stored action-review system (accepted 2026-09-22)

## P5-1 — No hidden retry after uncertain external writes: ACCEPTED
Uncertain external writes are never automatically retried; replacements
require fresh human approval. The timeout limitation was accepted because it
fails in the safe direction.

## P5-2 — Durable write-ahead review + tamper evidence + explicit actors: ACCEPTED
- **Write-ahead:** the review record is durably written (write + fsync of the
  JSONL, or equivalent atomic append) BEFORE the tool executes. If the review
  write fails, fail closed: the action does not run and the caller gets a
  distinct error.
- **Tamper evidence:** each review entry carries the hash of the previous
  entry (per-session hash chain) with a verify function; a test flips one
  byte and detects it.
- **Explicit actors:** every path records the actor (human device id,
  scheduled trigger id, or policy:Allow) — never empty, including
  autonomous/scheduled runs.

# Phase 5 — Lease fencing addendum (accepted 2026-09-22)

Pre-restart implementation left verification incomplete (906 passed / 2
failed; one failure was a test arming bug fixed but never re-run). The
restart forced an honest re-verification before reporting:

- **Fencing token:** `lease_generation` column + migration; `claim`/`takeover`
  bump generation via conditional SQL update; clean release tombstones the
  row (owner = '', expiry zero) so reclaim continues the sequence; per-store
  owner ids (`pid<process>-<sequence>`), not PID alone. Stale workers get
  `ToolCallFailure::stale_lease` (distinct from `uncertain`) at four gates:
  before review write, before connector invocation, before attempt append,
  before normal completion audit.
- **DB-side clock:** expiry compared and computed inside SQL from
  `CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)` (SQLite via
  rusqlite's bundled SQLite; millisecond-accurate, unlike
  `strftime('%s','now')`). Postgres/D1 porting notes documented.
- **Takeover suppression:** lapsed run's review/attempt logs inspected;
  in-flight or `ambiguous` attempts → `RunOutcome::NeedsReview` + human
  notification, no re-fire. Replacement links collected before evaluation
  (order-independent). Explicit human replacement resolves uncertainty.

Post-restart verification: `cargo test -p unpeel-core --lib` → **907 passed,
1 failed** (environmental: `direct_path_punch` UDP `PermissionDenied` in
sandbox; module untouched), 3 ignored. All 12 fencing/takeover tests pass.
fmt clean; touched files clippy-clean.

# Phase 6 R1 — CI green (2026-09-22)

Fixed 13 clippy/rustc denials across 4 crates (unpeel-connector ×2,
unpeel-core ×5, unpeel-serve ×5, unpeel-cli ×1) so
`cargo clippy --workspace --all-targets --all-features -- -D warnings`
passes on `crates/`; `unpeel-ui`/`unpeel-web`/`unpeel-ios-bridge` also clean.
Notable: the `request_shutdown as sighandler_t` casts tripped rustc's
`function_casts_as_integer`, not clippy's `fn_to_numeric_cast` — an
`#[allow(clippy::...)]` did nothing; fixed per the compiler's suggestion
(`as *const () as sighandler_t`). actionlint 1.7.7 installed to
`~/.local/bin`; one finding (`macos-26` unknown label) verified as a real
GA runner label via web search — actionlint's DB is stale; fixed with
`.github/actionlint.yaml`. Blocked (CI-only): desktop/mobile launcher
clippy needs GTK/WebKit system libs.

# W2 self-correction (2026-09-22)

During the W2 closeout I flagged that the manual APK script's Kotlin-compile
and d8-dex stages were placeholders, not real builds — the "manual APK" path
assembles an APK without running the actual Kotlin/d8 toolchain. This was
my own catch, recorded here so the limitation is not lost: the
`scripts/build-apk-manual.sh` + `scripts/build-apk-androidx.py` path is a
packaging scaffold, and a real APK still needs either the Gradle path
(blocked: daemon IPC broken in this sandbox) or a machine with the Android
SDK. The repaired scripts are in 04-launchers.patch; the placeholder stages
are marked as such in the scripts.

## Git status — full Track B diff stat (Phase 6, verified 2026-09-22)
Method: `git add -A -N` (intent-to-add; `out/` reset out), then
`git diff --numstat HEAD`, excluding `clients/dioxus/target/` build
artifacts and `out/`.

- **Total: 145 files, 65,830 insertions, 247 deletions**
- Nothing committed or pushed (per standing rule).

---

# Phase 6 final closeout (2026-09-23)

## Closeout corrections (all accepted)
1. **Composer fixes** (`clients/dioxus/unpeel-ui/src/composer.rs`,
   `unpeel-web/tests/web/composer.spec.js`): `type="text"` on the queued-edit
   input (Playwright selector), `edit_text` hoisted out of the render loop
   (hooks-in-loop violation), edit loads queued text / Save updates
   `ComposerState`, per-session draft restoration via `session_id` →
   `session_sig` mirroring (plus `turn_running` → `running_sig`).
2. **Desktop launcher regressions** (`clients/dioxus/unpeel-desktop/src/main.rs`):
   `git blame` showed the failures came from uncommitted R4 additions, not
   Phase 4 code — wasm-only `gloo_timers::future::TimeoutFuture` →
   `std::thread::sleep`, `DesktopView.selected` → `selected_session`,
   `sessions.clone()` for the later borrow.
3. **Poll-loop bug (review catch, real)**: the R4 event poll's blocking sleep
   sat inside `spawn(async move {…})` on the Dioxus runtime — it would have
   stalled the UI thread every 2s (the loop body also makes a blocking
   `client.events()` call). Fixed with a dedicated `std::thread::spawn` loop;
   that surfaced a second latent issue: `event_turn_running` was a plain
   `use_signal` (`!Send`), so it became `use_signal_sync` (`SyncSignal`), per
   the file's standing rule that thread-mutated UI state lives in a
   `SyncSignal`.

## Final regression (2026-09-23, serial)
- Root workspace `cargo test --workspace --no-fail-fast`: all green except 4
  environmental UDP-punch failures — unpeel-core
  `direct_path_punch::tests::punch_over_real_udp_sockets` (1) and unpeel-serve
  `direct_path::tests::*` (3: `PunchTimeout`/`Probing`). The sandbox has no
  real UDP path; `direct_path.rs` is untouched by this diff (pre-existing,
  previously masked by fail-fast).
- Dioxus: `unpeel-ui` 275/275, `unpeel-ios-bridge` 3/3, `unpeel-web` 0 tests
  (binary crate). Full-workspace test and desktop/mobile test targets blocked:
  gdk-sys needs GTK (see environment note).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  clean on `crates/`; clean on `unpeel-ui` and `unpeel-ios-bridge`. Launcher
  clippy still needs GTK system libs.
- `cargo fmt --all --check`: was dirty in 6 spots across Phase 6/R4-touched
  files (`events.rs`, `session_events.rs`, desktop `main.rs`, web `main.rs`) —
  ran `cargo fmt`; now clean on both workspaces.
- actionlint 1.7.7: clean.
- Playwright on a freshly rebuilt web bundle (Chrome for Testing 153 via
  `PLAYWRIGHT_CHROMIUM_PATH`): **13/13 pass** (composer 3 + demo 5 +
  mobai-mirror 5), 14.4s.
- Launcher `cargo check` via `scripts/env-nix-gtk.sh`: **blocked** — see
  environment note. (Was green at 00:31, before the reboot.)

## Environment event (2026-09-23 ~01:52)
The VM rebooted at ~00:54. `/nix/store` was a tmpfs mount (Phase 4 W1
single-user workaround) and is gone; `env-nix-gtk.sh` currently exports dead
paths. Its `PKG_CONFIG_PATH entries: 1` output is the canary — a healthy run
reports 87. Re-provision /nix from the Nix binary cache (~849 store entries)
to restore native launcher check/build. Recorded in the new
`docs/KNOWN_GOOD_ENVIRONMENTS.md`.

## Patches
`out/patches/` regenerated from the current tree: 8 patches, split by area,
verified with sequential `git apply --check` on a fresh clean worktree at
`7f2f5a3` (then applied; worktree removed).
**Diff stat (final): 156 files, 69,203 insertions, 250 deletions** (`git diff --numstat 7f2f5a3`, excluding `out/` and `clients/dioxus/target/`)
Nothing committed or pushed (per standing rule).

## What Amein must do
- [ ] Review the 8 patches in `out/patches/` (apply order in `out/patches/README.md`).
- [ ] Commit and push — nothing has been committed (standing rule).
- [ ] Apple: provide the Team ID, App Store Connect API key, APNs .p8, and bundle ID before any device/TestFlight run.
- [ ] MobAI: provide the MobAI key and a Host pair code before device smoke runs.
- [ ] Re-provision /nix (or build on a machine with GTK/WebKit dev libs) to restore native launcher check/build.
- [ ] Known gap: the Host has no protocol-level turn-cancel verb. Desktop Stop sends raw Ctrl-C (terminal fallback, labeled as such in code/docs); mobile has no Stop button. A real solution needs an additive Host interrupt verb — Host-first, capability-advertised, conformance-aligned.
