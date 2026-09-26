# Rename Allowlist — intentional `unpeel` references

These paths legitimately mention `unpeel` and are EXCLUDED from the rename
guard. Everything else must say `supercli`. Each entry names an explicit
path with its justification — no whole-tree exclusions of active source.

## 1. Migration compatibility (intentional)

- `crates/supercli-cli/src/import_unpeel_cli.rs` — `supercli migrate
  --from-unpeel` one-time import from a legacy `~/.unpeel/` home. References
  `~/.unpeel`, `UNPEEL_HOME`, and the legacy binary name by design.
- `crates/supercli-cli/src/cli.rs`, `crates/supercli-cli/src/main.rs` —
  contain only references to the `import_unpeel_cli` module
  (`mod import_unpeel_cli`, `--from-unpeel` flag, `run_from_unpeel`).
  No other `unpeel` references.

## 2. Frozen legacy clients (Amein directive 2026-09-26)

Amein froze the Swift/Dioxus clients. They keep their original `unpeel`
names and references. Do not rename.

- `clients/legacy/` — all frozen Dioxus, iOS, native, and shared clients
  (including `clients/legacy/shared/SupercliShared/.../GeneratedRuntimeCatalog.swift`,
  whose generated `UnpeelRuntime*` Swift API surface belongs to the frozen client),
  and the frozen app-kit (`clients/legacy/app-kit/`, moved 2026-09-26; do not
  rename).
- `tests/device/` — `.mob` device-test scripts that drive the frozen Dioxus
  mobile client (bundle id `com.unpeel.controller`, `unpeel pair`).
- `scripts/generate-runtime-client-catalog.mjs` — emits the `UnpeelRuntime*`
  Swift API for the frozen Swift client; renaming it would break the frozen
  client's compile-time API.
- `generated/GeneratedRuntimeCatalog.swift` — checked-in output of the above
  generator for the frozen client.
- Frozen native-app release channel (published `Unpeel-*.dmg` / `Unpeel-*.zip`
  artifact names for the frozen macOS app; renaming would break download URLs):
  - `scripts/release-app.mjs`
  - `scripts/release-app-state.mjs`
  - `scripts/publish-cloudflare-release.mjs`
  - `scripts/release-app-installer.test.mjs`
  - `scripts/release-app-state.test.mjs`

## 3. Historical attribution (do not rewrite)

- `THIRD_PARTY_NOTICES.txt` — third-party license notices with historical names.
- `crates/apps/diffs/LICENSE` — `Copyright (c) 2026 Unpeel contributors`.
- `crates/apps/filetree/LICENSE` — `Copyright (c) 2026 Unpeel contributors`.
- `CHANGELOG.md` — historical changelog entries (do not rewrite history).

## 4. Generated / ephemeral (excluded from guard)

- `*.lock`, `package-lock.json` — lockfiles.
- `docs/book/` — generated mdbook output.
- `*/target/*`, `*__pycache__*`, `*.pyc`, `.dart_tool/` — build artifacts.
- `crates/supercli-core/fuzz/out/` — fuzzer output.
- `*.a` — vendored static libraries.

## 5. Design docs (historical reference)

- `docs/internal/` — internal design docs that reference the old name for
  historical context (audit trail of the rename itself).

## 6. Submodule

- `clients/gpuidart/` — external submodule, owned upstream.

## 7. The guard itself

- `docs/rename-allowlist.md` — this file (documents the word being guarded).
- `.github/workflows/rename-guard.yml` — the guard workflow (references the word).

## Deliberately NOT excluded (renamed 2026-09-26)

The following were previously excluded as whole trees and were renamed
instead, because they are active tooling for the renamed product:

- `scripts/e2e-scenario-helpers.py`, `scripts/perf-bench.py`,
  `scripts/soak-host.py` — referenced stale `UNPEEL_HOME` / `UNPEEL_HOST_BIN` /
  `unpeel-host` binary / `UNPEEL:` QR prefix; the renamed `e2e-scenario.sh`
  already exports `SUPERCLI_HOME` / `SUPERCLI_HOST_BIN`, so the helpers were
  broken until renamed.
- `scripts/release-cli.mjs` — stale "Unpeel CLI" header comment.
- `runtimes/opencode/assets/hooks/plugin.js`,
  `runtimes/amp/assets/hooks/plugin.js`, `runtimes/hook-plugins.test.js` —
  referenced `UNPEEL_SESSION_ID`, but the host now sets `SUPERCLI_SESSION_ID`;
  the plugin silently no-op'd until renamed.
- `runtimes/README.md`, `packaging/service/*`, root `README.md`,
  root `package.json` — product docs / service units for the renamed product.
