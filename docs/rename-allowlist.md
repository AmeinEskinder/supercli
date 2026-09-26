# Rename Allowlist — intentional `unpeel` references

These files legitimately mention `unpeel` and are EXCLUDED from the rename guard.
Everything else must say `supercli`.

## 1. Migration compatibility (intentional)

- `crates/supercli-cli/src/import_unpeel_cli.rs` — `supercli migrate --from-unpeel`
  one-time import from a legacy `~/.unpeel/` home. References `~/.unpeel`,
  `UNPEEL_HOME`, and the legacy binary name by design.
- `crates/supercli-cli/src/cli.rs`, `crates/supercli-cli/src/main.rs` — contain
  only references to the `import_unpeel_cli` module (`mod import_unpeel_cli`,
  `--from-unpeel` flag, `run_from_unpeel`). No other `unpeel` references.

## 2. Frozen legacy clients (Amein directive 2026-09-26)

Amein froze the Swift/Dioxus clients. They keep their original `unpeel` names
and references. Do not rename.

- `clients/legacy/` — all frozen Dioxus, iOS, native, and shared clients.
- `crates/apps/app-kit/` — frozen Swift/TypeScript app framework.

## 3. Historical attribution (do not rewrite)

- `THIRD_PARTY_NOTICES.txt` — third-party license notices with historical names.
- `crates/apps/diffs/LICENSE` — `Copyright (c) 2026 Unpeel contributors`.
- `crates/apps/filetree/LICENSE` — `Copyright (c) 2026 Unpeel contributors`.
- `clients/native/licenses/` — vendored license texts (under clients/legacy).
- `clients/native/release-notes/*.html` — published historical release notes.
- `CHANGELOG.md` — historical changelog entries (do not rewrite history).

## 4. Generated / ephemeral (excluded from guard)

- `*.lock`, `package-lock.json` — lockfiles.
- `docs/book/` — generated mdbook output.
- `*/target/*`, `*__pycache__*`, `*.pyc` — build artifacts.
- `crates/supercli-core/fuzz/out/` — fuzzer output.
- `*.a` — vendored static libraries.

## 5. Design docs (historical reference)

- `docs/internal/` — internal design docs that reference the old name for
  historical context (audit trail of the rename itself).

## 6. Submodule

- `clients/gpuidart/` — external submodule, owned upstream.

## 7. Out of Rust crate scope (not renamed per 2026-09-26 directive)

The 2026-09-26 rename completion covers Rust crates + protocol only.
These paths retain `unpeel` references and are out of scope:

- `scripts/` — Python/JS build and test scripts.
- `packaging/` — systemd/service files.
- `runtimes/` — JS hook plugins.
- `tests/device/` — .mob device test scripts.
- `generated/` — generated Swift catalog.
- `package.json`, `README.md` (root) — product docs.

## 8. The guard itself

- `docs/rename-allowlist.md` — this file (documents the word being guarded).
- `.github/workflows/rename-guard.yml` — the guard workflow (references the word).
