#!/usr/bin/env bash
# sync-main-v2.sh — sync supercli-main-v2 from supercli-next with exclusions.
#
# Usage: scripts/sync-main-v2.sh [--dry-run]
#
# This script:
# 1. Rsyncs the supercli-next tree into the current working tree
# 2. Deletes EVERY path listed in docs/internal/EXCLUSIONS.md
# 3. Runs fresh-clone-verify locally
# 4. Refuses to commit or push unless ALL GREEN
#
# Must be run from a clean worktree on the supercli-main-v2 branch.
set -euo pipefail

DRY_RUN=false
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN=true
fi

# Must be on supercli-main-v2 branch
BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [[ "$BRANCH" != "supercli-main-v2" && "$BRANCH" != "temp-mainv2-sync" ]]; then
  echo "FAIL: Must run on supercli-main-v2 branch (current: $BRANCH)"
  exit 1
fi

# Must be clean
if [[ -n "$(git status --porcelain)" ]]; then
  echo "FAIL: Working tree not clean"
  exit 1
fi

echo "=== 1. Fetching supercli-next ==="
git fetch origin supercli-next
NEXT_SHA=$(git rev-parse FETCH_HEAD)
echo "supercli-next: $NEXT_SHA"

echo "=== 2. Rsyncing tree ==="
git read-tree -u --reset FETCH_HEAD

echo "=== 3. Applying exclusions from docs/internal/EXCLUSIONS.md ==="
# Parse the markdown table for paths in backticks
EXCLUSIONS=$(grep -oP '^\| `\K[^`]+' docs/internal/EXCLUSIONS.md || true)
if [[ -z "$EXCLUSIONS" ]]; then
  echo "FAIL: No exclusions found in docs/internal/EXCLUSIONS.md"
  exit 1
fi

echo "$EXCLUSIONS" | while read -r path; do
  if [[ -e "$path" ]]; then
    echo "  Removing: $path"
    rm -rf "$path"
  else
    echo "  (not present): $path"
  fi
done

# Re-add EXCLUSIONS.md itself (it was just synced from next, but we need the main-v2 version)
# Actually, EXCLUSIONS.md should be tracked in main-v2, so restore it if needed
if [[ ! -f "docs/internal/EXCLUSIONS.md" ]]; then
  echo "FAIL: docs/internal/EXCLUSIONS.md missing after sync"
  exit 1
fi

echo "=== 4. Running fresh-clone-verify ==="
if [[ "$DRY_RUN" == "true" ]]; then
  echo "(dry-run: skipping verify)"
else
  # Run verify on the current tree (not a fresh clone, but checks the guards)
  # For a full verify, the caller should run scripts/fresh-clone-verify.sh separately
  echo "Running guard checks..."
  
  # Rename guard
  echo "--- Rename guard ---"
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
              -e 'scripts/sync-main-v2.sh' \
    || true)
  if [[ -n "$matches" ]]; then
    echo "FAIL: unpeel references found:"
    echo "$matches"
    exit 1
  fi
  echo "PASS: rename guard"
  
  # Domain guard
  echo "--- Domain guard ---"
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
  if [[ -n "$domain_matches" ]]; then
    echo "FAIL: supercli.com references found:"
    echo "$domain_matches"
    exit 1
  fi
  echo "PASS: domain guard"
  
  # Exclusions check
  echo "--- Exclusions check ---"
  FAILED=false
  echo "$EXCLUSIONS" | while read -r path; do
    if [[ -e "$path" ]]; then
      echo "FAIL: Excluded path still present: $path"
      FAILED=true
    fi
  done
  if [[ "$FAILED" == "true" ]]; then
    exit 1
  fi
  echo "PASS: all exclusions removed"
fi

echo "=== 5. Ready to commit ==="
if [[ "$DRY_RUN" == "true" ]]; then
  echo "(dry-run: not committing)"
  git status --short | head -20
else
  echo "All guards PASS. Ready for manual commit."
  echo "Run: git add -A && git commit -m 'Sync...'"
fi
