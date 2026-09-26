#!/usr/bin/env bash
# fresh-clone-verify.sh — verify a branch survives a fresh recursive clone.
#
# Usage: scripts/fresh-clone-verify.sh <branch> [remote]
#   Clones <branch> from <remote> (default: origin's URL) into a temp dir,
#   inits submodules recursively, and runs the cheap structural gates:
#   submodule resolution, rename guard, and tree sanity.
#
# This exists because a terminal-pane branch once pointed clients/gpuidart at a
# VM-only commit, which would have broken every fresh recursive clone.
set -euo pipefail

BRANCH="${1:?usage: fresh-clone-verify.sh <branch> [remote]}"
REMOTE="${2:-$(git config --get remote.origin.url)}"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo "=== fresh-clone-verify: branch=$BRANCH remote=$REMOTE ==="
echo "--- 1. fresh clone ---"
git clone --branch "$BRANCH" --depth 1 "$REMOTE" "$TMPDIR/clone"
cd "$TMPDIR/clone"

echo "--- 2. recursive submodule init ---"
git submodule update --init --recursive
echo "submodules OK:"
git submodule status

echo "--- 3. tree sanity ---"
for p in crates clients/supercli-app docs/parity/checklist.md .github/workflows/rename-guard.yml; do
  test -e "$p" || { echo "FAIL: missing $p"; exit 1; }
done
echo "tree OK"

echo "--- 4. rename guard (same logic as .github/workflows/rename-guard.yml) ---"
matches=$(grep -rli 'unpeel' . \
  --exclude-dir=.git \
  --exclude-dir=target \
  --exclude-dir=node_modules \
  --exclude-dir=__pycache__ \
  --exclude-dir=gpuidart \
  --exclude-dir=.dart_tool \
  --exclude='*.lock' \
  --exclude='*.pyc' \
  --exclude='*.a' \
  --exclude='fresh-clone-verify.sh' \
  | grep -v -e '^./docs/book/' \
            -e '^./docs/internal/' \
            -e '^./docs/parity/' \
            -e '^./clients/gpuidart/' \
            -e '^./clients/legacy/' \
            -e '^./tests/device/' \
            -e '^./scripts/generate-runtime-client-catalog.mjs$' \
            -e '^./scripts/release-app.mjs$' \
            -e '^./scripts/release-app-state.mjs$' \
            -e '^./scripts/publish-cloudflare-release.mjs$' \
            -e '^./scripts/release-app-installer.test.mjs$' \
            -e '^./scripts/release-app-state.test.mjs$' \
            -e '^./generated/GeneratedRuntimeCatalog.swift$' \
            -e 'CHANGELOG.md' \
            -e 'THIRD_PARTY_NOTICES.txt' \
            -e 'crates/apps/diffs/LICENSE' \
            -e 'crates/apps/filetree/LICENSE' \
            -e 'crates/supercli-cli/src/import_unpeel_cli.rs' \
            -e 'crates/supercli-cli/src/cli.rs' \
            -e 'crates/supercli-cli/src/main.rs' \
            -e 'crates/supercli-core/fuzz/out/' \
            -e 'package-lock.json' \
            -e 'docs/rename-allowlist.md' \
            -e '.github/workflows/rename-guard.yml' \
  || true)
if [ -n "$matches" ]; then
  echo "FAIL: unpeel references outside allowlist:"
  echo "$matches"
  exit 1
fi
echo "rename guard PASS"

echo "--- 5. main-v2 exclusions (docs/internal/EXCLUSIONS.md) ---"
for p in docs/internal/buildlog.md docs/internal/handoff.md docs/internal/phases docs/internal/pr-draft.md docs/internal/agents.md docs/internal/notice.md docs/internal/release-checklist.md; do
  if [ -e "$p" ]; then echo "FAIL: excluded path present: $p"; exit 1; fi
done
echo "exclusions OK"

echo "--- 5b. vendored ghostty-vt archives have no unpeel build paths ---"
for a in crates/supercli-core/vendor/ghostty-vt/*/libghostty-vt.a; do
  n=$(strings "$a" 2>/dev/null | grep -ci unpeel || true)
  if [ "$n" != "0" ]; then echo "FAIL: $a contains $n unpeel mentions"; exit 1; fi
done
echo "ghostty-vt archives OK"

echo "--- 6. CHANGELOG is the fresh supercli 0.1.0 (not the old Unpeel one) ---"
head -1 CHANGELOG.md | grep -q '^# Changelog — supercli' || { echo "FAIL: CHANGELOG.md is not the supercli 0.1.0 changelog"; head -3 CHANGELOG.md; exit 1; }
grep -q 'Track B + Phases' CHANGELOG.md && { echo "FAIL: CHANGELOG.md still references old Unpeel phases"; exit 1; }
echo "CHANGELOG OK"

echo "--- 7. docs/book generated output has no stale unpeel mentions ---"
if [ -d docs/book/book ]; then
  stale=$(grep -rli 'unpeel' docs/book/book/ 2>/dev/null || true)
  if [ -n "$stale" ]; then echo "FAIL: docs/book/book/ contains stale unpeel mentions:"; echo "$stale"; exit 1; fi
fi
echo "docs/book OK"

echo "--- 8. HEAD sha ---"
git rev-parse HEAD

echo "=== fresh-clone-verify: ALL GREEN ==="
