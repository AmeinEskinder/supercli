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

echo "--- 4b. supercli.com domain guard (third-party domain, must be superc.li) ---"
domain_matches=$(grep -rli 'supercli\.com' . \
  --exclude-dir=.git \
  --exclude-dir=target \
  --exclude-dir=node_modules \
  --exclude-dir=__pycache__ \
  --exclude-dir=.dart_tool \
  --exclude='rename-guard.yml' \
  --exclude='fresh-clone-verify.sh' \
  | grep -v -e '^./clients/legacy/' \
  || true)
if [ -n "$domain_matches" ]; then
  echo "FAIL: supercli.com references found (must be superc.li):"
  echo "$domain_matches"
  exit 1
fi
echo "domain guard PASS"

echo "--- 4c. contributor identity scrub (no original-developer identity) ---"
ident_matches=$(grep -riE 'tommy|vedvik|uxthemes|claude-501' . \
  --exclude-dir=.git \
  --exclude-dir=target \
  --exclude-dir=node_modules \
  --exclude-dir=__pycache__ \
  --exclude-dir=gpuidart \
  --exclude-dir=.dart_tool \
  --exclude='*.lock' \
  --exclude='rename-guard.yml' \
  --exclude='fresh-clone-verify.sh' \
  2>/dev/null | grep -v -e '^./clients/gpuidart/' || true)
# /Users/<name> paths: flag only non-placeholder usernames (me/test/example/etc are neutral fixtures)
user_matches=$(grep -rhoE '/Users/[a-zA-Z0-9_.-]+' . \
  --exclude-dir=.git \
  --exclude-dir=target \
  --exclude-dir=node_modules \
  --exclude-dir=__pycache__ \
  --exclude-dir=gpuidart \
  --exclude-dir=.dart_tool \
  2>/dev/null | grep -v -e '^./clients/gpuidart/' | sort -u | grep -v -e '^/Users/me$' -e '^/Users/test$' -e '^/Users/testing$' -e '^/Users/example$' -e '^/Users/exampleuser$' -e '^/Users/alice$' -e '^/Users/x$' -e '^/Users/t$' || true)
if [ -n "$ident_matches" ]; then
  echo "FAIL: original-contributor identity references found:"
  echo "$ident_matches" | head -20
  exit 1
fi
if [ -n "$user_matches" ]; then
  echo "FAIL: non-placeholder /Users/<name> paths found (possible identity leak):"
  echo "$user_matches" | head -20
  exit 1
fi
for bin in $(find . \( -name '*.a' -o -name '*.dylib' -o -name '*.so' \) 2>/dev/null | grep -v -e '/.git/' -e '/target/' -e 'gpuidart'); do
  n=$(strings "$bin" 2>/dev/null | grep -ciE 'tommy|vedvik|uxthemes|claude-501|/Users/[a-z]' || true)
  if [ "$n" != "0" ]; then echo "FAIL: $bin contains $n identity mentions"; exit 1; fi
done
echo "identity scrub PASS"

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
