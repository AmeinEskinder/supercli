# Rename Allowlist — intentional `unpeel` references

These files legitimately mention `unpeel` and are EXCLUDED from the rename guard.
Everything else must say `supercli`.

## 1. Migration compatibility (intentional)

- `crates/supercli-cli/src/import_unpeel_cli.rs` — `supercli migrate --from-unpeel`
  one-time import from a legacy `~/.unpeel/` home. References `~/.unpeel`,
  `UNPEEL_HOME`, and the legacy binary name by design.

## 2. Historical attribution (do not rewrite)

- `THIRD_PARTY_NOTICES.txt` — third-party license notices with historical names.
- `crates/apps/diffs/LICENSE` — `Copyright (c) 2026 Unpeel contributors`.
- `crates/apps/filetree/LICENSE` — `Copyright (c) 2026 Unpeel contributors`.
- `clients/native/licenses/` — vendored license texts.
- `clients/native/release-notes/*.html` — published historical release notes.
- `CHANGELOG.md` — historical changelog entries (do not rewrite history).

## 3. Generated / ephemeral (excluded from guard)

- `*.lock`, `package-lock.json` — lockfiles.
- `docs/book/` — generated mdbook output.
- `*/target/*`, `*__pycache__*`, `*.pyc` — build artifacts.
- `crates/supercli-core/fuzz/out/` — fuzzer output.
- `*.a` — vendored static libraries.

## 4. Design docs (historical reference)

- `docs/internal/` — internal design docs that reference the old name for
  historical context (audit trail of the rename itself).

## 5. Submodule

- `clients/gpuidart/` — external submodule, owned upstream.

## 6. The guard itself

- `docs/rename-allowlist.md` — this file (documents the word being guarded).
- `.github/workflows/rename-guard.yml` — the guard workflow (references the word).

## 3. Frozen legacy clients (do not touch)

- `clients/legacy/` — entire directory tree frozen per Amein 2026-09-26.
  Contains the pre-gpuidart clients (Dioxus desktop/mobile/web/ui/ios-bridge,
  native-shell, macOS Swift, iOS Swift, shared Swift) moved verbatim from
  `clients/dioxus/`, `clients/native/`, `clients/ios/`, `clients/shared/`.
  No renames, fixes, or features. Exempt from the rename guard.
  Amein decides deletion once gpuidart apps reach parity.
