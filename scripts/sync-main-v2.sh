#!/usr/bin/env bash
# sync-main-v2.sh — sync supercli-main-v2 from supercli-next with exclusions.
#
# Usage: scripts/sync-main-v2.sh [--dry-run]
#
# This script:
# 1. Rsyncs the supercli-next tree into the current working tree
# 2. Restores main-v2-only files (EXCLUSIONS.md, sync script, verify script)
# 3. Deletes EVERY path listed in docs/internal/EXCLUSIONS.md
# 4. Runs scripts/fresh-clone-verify.sh --local (ONE source of truth for guards)
# 5. Commits and pushes non-force to supercli-main-v2, ONLY if verify is ALL GREEN
#
# Refuses to commit or push unless fresh-clone-verify exits 0.
# Must be run from a clean worktree on the supercli-main-v2 branch.
set -euo pipefail

DRY_RUN=false
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN=true
fi

# Must be on supercli-main-v2 branch (or temp worktree)
BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [[ "$BRANCH" != "supercli-main-v2" && "$BRANCH" != "temp-mainv2-sync" ]]; then
  echo "FAIL: Must run on supercli-main-v2 branch (current: $BRANCH)"
  exit 1
fi

# Must be clean
if [[ -n "$(git status --porcelain)" ]]; then
  echo "FAIL: Working tree not clean"
  git status --short | head -10
  exit 1
fi

echo "=== 1. Fetching supercli-next ==="
git fetch origin supercli-next
NEXT_SHA=$(git rev-parse FETCH_HEAD)
echo "supercli-next: $NEXT_SHA"

echo "=== 2. Rsyncing tree ==="
git read-tree -u --reset FETCH_HEAD

echo "=== 2b. Restoring main-v2-only files ==="
# These live only on main-v2, not on next
git show HEAD:docs/internal/EXCLUSIONS.md > docs/internal/EXCLUSIONS.md
git show HEAD:scripts/sync-main-v2.sh > scripts/sync-main-v2.sh
git show HEAD:scripts/fresh-clone-verify.sh > scripts/fresh-clone-verify.sh
chmod +x scripts/sync-main-v2.sh scripts/fresh-clone-verify.sh
echo "Restored docs/internal/EXCLUSIONS.md, scripts/sync-main-v2.sh, scripts/fresh-clone-verify.sh"

echo "=== 3. Applying exclusions from docs/internal/EXCLUSIONS.md ==="
EXCLUSIONS=$(grep -oP '^\| `\K[^`]+' docs/internal/EXCLUSIONS.md || true)
if [[ -z "$EXCLUSIONS" ]]; then
  echo "FAIL: No exclusions found in docs/internal/EXCLUSIONS.md"
  exit 1
fi

# FIXED: use herestring (<<<) not pipe, so the loop runs in this shell
# and 'exit 1' actually aborts the script.
while IFS= read -r path; do
  if [[ -e "$path" ]]; then
    echo "  Removing: $path"
    rm -rf "$path"
  else
    echo "  (not present): $path"
  fi
done <<< "$EXCLUSIONS"

echo "=== 4. Running fresh-clone-verify.sh --local (ONE source of truth) ==="
if ! bash scripts/fresh-clone-verify.sh --local "$PWD"; then
  echo ""
  echo "FAIL: fresh-clone-verify --local did not go ALL GREEN."
  echo "Refusing to commit or push. Fix the issues above and re-run."
  exit 1
fi

echo ""
echo "=== 5. Verify ALL GREEN — committing ==="
if [[ "$DRY_RUN" == "true" ]]; then
  echo "(dry-run: not committing)"
  git status --short | head -20
  exit 0
fi

git add -A
git commit -m "Sync supercli-main-v2 with supercli-next@${NEXT_SHA:0:7} via sync-main-v2.sh

fresh-clone-verify --local ALL GREEN.
Exclusions applied per docs/internal/EXCLUSIONS.md."

echo "=== 6. Pushing non-force to supercli-main-v2 ==="
git push origin HEAD:supercli-main-v2

echo ""
echo "=== sync-main-v2.sh: DONE ==="
git log --oneline -1
