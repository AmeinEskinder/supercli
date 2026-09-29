#!/usr/bin/env bash
# fresh-clone-verify.sh — verify a branch survives a fresh recursive clone.
#
# Usage:
#   scripts/fresh-clone-verify.sh <branch> [remote]
#     Clones <branch> from <remote> (default: origin's URL) into a temp dir,
#     inits submodules recursively, and runs the cheap structural gates.
#   scripts/fresh-clone-verify.sh --local <dir>
#     Runs ALL guard sections (3-8) on an existing working tree <dir>.
#     Skips the clone (1) and submodule (2) steps.
#
# This exists because a terminal-pane branch once pointed clients/gpuidart at a
# VM-only commit, which would have broken every fresh recursive clone.
set -euo pipefail

LOCAL_MODE=false
LOCAL_DIR=""

if [[ "${1:-}" == "--local" ]]; then
  LOCAL_MODE=true
  LOCAL_DIR="${2:?usage: fresh-clone-verify.sh --local <dir>}"
  [[ -d "$LOCAL_DIR" ]] || { echo "FAIL: not a directory: $LOCAL_DIR"; exit 1; }
  echo "=== fresh-clone-verify: --local $LOCAL_DIR ==="
  cd "$LOCAL_DIR"
else
  BRANCH="${1:?usage: fresh-clone-verify.sh <branch> [remote] | fresh-clone-verify.sh --local <dir>}"
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
fi

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
            -e '^./generated/runtime-catalog.json$' \
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
            -e '.github/workflows/linux.yml' \
            -e 'scripts/sync-main-v2.sh' \
            -e '^\./\.git$' \
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
  --exclude='sync-main-v2.sh' \
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
  --exclude='sync-main-v2.sh' \
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

echo "--- 4d. bundle ID guard (com.supercli -> li.superc) ---"
bundle_matches=$(grep -rl 'com\.supercli' . \
  --exclude-dir=.git \
  --exclude-dir=target \
  --exclude-dir=node_modules \
  --exclude-dir=__pycache__ \
  --exclude-dir=.dart_tool \
  --exclude='rename-guard.yml' \
  --exclude='fresh-clone-verify.sh' \
  --exclude='sync-main-v2.sh' \
  2>/dev/null | grep -v -e '^./clients/legacy/' || true)
if [ -n "$bundle_matches" ]; then
  echo "FAIL: com.supercli bundle IDs found (must be li.superc):"
  echo "$bundle_matches"
  exit 1
fi
echo "bundle ID guard PASS"

echo "--- 4e. CLRTY license prefix guard (must be SCLI-, legacy key-format keys rejected) ---"
# Intentional CLRTY references (rejection logic, not acceptance):
# - crates/supercli-native-bridge/src/macos/license.rs: LEGACY_KEY_PREFIX + rejection
# - crates/supercli-core/src/license.rs: LEGACY_KEY_PREFIX + rejection
# - clients/supercli-app/lib/screens/licensesettings.dart: isLegacyKey helper
# - clients/supercli-app/test/settings_test.dart: rejection tests
# - docs/parity/: sidecar notes on the legacy format
# - docs/security/signing-keys.md: documentation
clrty_matches=$(grep -rl 'CLRTY' . \
  --exclude-dir=.git \
  --exclude-dir=target \
  --exclude-dir=node_modules \
  --exclude-dir=__pycache__ \
  --exclude-dir=gpuidart \
  --exclude-dir=.dart_tool \
  --exclude='*.lock' \
  --exclude='rename-guard.yml' \
  --exclude='fresh-clone-verify.sh' \
  --exclude='sync-main-v2.sh' \
  2>/dev/null | grep -v -e '^./clients/legacy/' \
                        -e '^./crates/supercli-native-bridge/src/macos/license.rs$' \
                        -e '^./crates/supercli-core/src/license.rs$' \
                        -e '^./clients/supercli-app/lib/screens/licensesettings.dart$' \
                        -e '^./clients/supercli-app/test/settings_test.dart$' \
                        -e '^./docs/parity/' \
                        -e '^./docs/security/signing-keys.md$' \
  || true)
if [ -n "$clrty_matches" ]; then
  echo "FAIL: CLRTY references found outside clients/legacy and rejection-logic files (license keys are SCLI-):"
  echo "$clrty_matches"
  exit 1
fi
echo "CLRTY guard PASS"

echo "--- 5. main-v2 exclusions (docs/internal/EXCLUSIONS.md) ---"
# Read exclusions from the markdown table
if [ -f docs/internal/EXCLUSIONS.md ]; then
  while IFS= read -r path; do
    if [ -e "$path" ]; then echo "FAIL: excluded path present: $path"; exit 1; fi
  done <<< "$(grep -oP '^\| `\K[^`]+' docs/internal/EXCLUSIONS.md || true)"
  echo "exclusions OK (from EXCLUSIONS.md)"
else
  # Fallback to hardcoded list
  for p in docs/internal/buildlog.md docs/internal/handoff.md docs/internal/phases docs/internal/pr-draft.md docs/internal/agents.md docs/internal/notice.md docs/internal/release-checklist.md; do
    if [ -e "$p" ]; then echo "FAIL: excluded path present: $p"; exit 1; fi
  done
  echo "exclusions OK (hardcoded)"
fi

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
git rev-parse HEAD 2>/dev/null || echo "(not a git worktree HEAD)"

echo "=== fresh-clone-verify: ALL GREEN ==="
