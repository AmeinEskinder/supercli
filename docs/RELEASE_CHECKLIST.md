# Release checklist — unpeel CLI/Host workspace

For cutting a release of the Rust workspace (`crates/`). Source of truth
for gates: `unpeel/AGENTS.md` ("Tests to run", "Release order").
The website changelog is read from the website repo at release time
(`scripts/release-changelog.mjs`); the workspace `CHANGELOG.md` at the repo
root is the crate-level record.

## 1. Version

- [ ] Bump `[workspace.package] version` in `crates/Cargo.toml`
      (all crates inherit `version.workspace = true`), then
      `cargo update --workspace`.
- [ ] `git diff --stat` on lockfiles shows only the version bumps, no
      dependency drift (`crates/Cargo.lock`; `crates/unpeel-attach/`
      has its own lockfile — update only if it changed).

## 2. Changelog

- [ ] `CHANGELOG.md` has an entry for the new version: user-visible
      changes, security-relevant fixes, protocol changes, migration notes.
- [ ] Security fixes name the tightened behavior (e.g. tamper-evidence)
      so downstream can assess upgrade urgency.

## 3. Verification gates (all must be green)

- [ ] `cargo test --manifest-path crates/Cargo.toml --workspace`
      (includes unpeel-native-bridge)
- [ ] `cargo test --manifest-path crates/unpeel-attach/Cargo.toml`
- [ ] `cargo test --manifest-path crates/apps/Cargo.toml --workspace`
      (first-party Apps + App Kit, own workspace)
- [ ] `cargo clippy --manifest-path crates/Cargo.toml --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo fmt --all -- --check`
- [ ] `crates/unpeel-cli/tests/run.sh` — the PTY matrix, real binaries
      (~10 min); `./run.sh <filter>` for a subset. Required after CLI
      changes.
- [ ] Only failures allowed are the documented environmental ones:
      real-UDP punch tests fail in sandboxes without UDP;
      `direct_path.rs` is untouched by most changes — confirm any
      failure is in that known class.
- [ ] `node --test scripts/release-*.test.mjs` (release-guards dry run)
- [ ] `node --check` on release scripts touched since the last release;
      `sh -n` on shell scripts touched.
- [ ] `scripts/check-notices.sh` — regenerates `THIRD_PARTY_NOTICES.txt`
      and diffs against the committed snapshot; every vendored artifact
      under `crates/**/vendor/` carries a LICENSE file.

## 4. Pre-release soak (Host changes)

- [ ] `scripts/soak-host.py` on the release binaries: 30+ min load,
      verdict PASS (zero tool-call errors, ring buffer within bound,
      no review-log lock timeouts, bounded RSS/fd growth).

## 5. Publish (release order: CLI -> Mac app -> website)

- [ ] CLI: `bun run release:cli -- --channel <ch>` (three archives from
      one commit, published to R2).
- [ ] Mac app: `bun run release:mac -- --channel <ch> --build <n>`
      (builds server binaries + bridge from this tree at the same
      commit; Developer ID sign, notarize, staple, DMG + Sparkle ZIP,
      appcast). Agents cannot cut a real release — it needs the
      operator's Developer ID, notary, Sparkle, and R2 credentials;
      validate pipeline changes with `--dry-run`.
- [ ] Website: the `## <version>` changelog entry goes live from the
      separate `unpeel-cloud` repo.
- [ ] Push only the release branch; never force-push, never touch other
      refs. (Outbound SSH permission is owned by Osman/Amein — confirm
      the setting before attempting.)
- [ ] Tag per the repo's tag convention after the push succeeds.
- [ ] No SBOM generation path exists in this repo as of 2026-09-25 —
      nothing to run; revisit if release tooling adds one.

## 6. v0.9.0 Phase 13 gates

- [ ] S2: Before/after concurrency tables (1/4/8/16, n≥2000 each) show
      throughput increasing through 16 and p99 <500ms at concurrency 8.
- [ ] S2: Grant store uses optimistic concurrency (serialization outside
      lock); crash safety preserved (temp+fsync+rename+dir fsync under lock).
- [ ] S2: `unpeel migrate` moves grants from app-state.json to grants.json
      (idempotent, fixture test passes).
- [ ] S2: Backup includes grants.json; backup-during-active-writes test
      covers concurrent grant writes.
- [ ] S3: MCP→visible delay investigated; polling replaced with event-driven
      if confirmed, or evidence reported if not.
- [ ] S4: 50x kill -9/restart chaos at concurrency 8 passes (no lost grants,
      no stuck locks, chains verify).
- [ ] S5: cargo-fuzz 30min each on pairing/sealed-envelope and config parsers;
      executions/crashes/fixes reported.
- [ ] S6: STRIDE threat model refreshed for Phases 9-13.
- [ ] S7: 2-hour release soak at concurrency 4 passes.
- [ ] Doctor --bundle excludes review payloads and command text (planted
      assertion passes).
- [ ] Amein-dependent items: SSH push permission, CI, Apple credentials,
      MobAI credentials (all BLOCKED pending Amein).
