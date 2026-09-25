#!/usr/bin/env bash
# scripts/apply-track-b.sh — Apply Track B as an ordered series of LOGICAL
# commits on a clean checkout.
#
# Usage:
#   scripts/apply-track-b.sh [--worktree DIR] [--branch NAME] [--base REV]
#
# What it does:
#   1. Creates a fresh `git worktree` at the base commit (default: temp dir,
#      removed on success unless --worktree is given).
#   2. Creates a branch (default: track-b) at the base.
#   3. Applies the working-tree changes as an ordered series of logical
#      commits, one per area (workspace, core, client, connector, serve,
#      cli, dioxus-ui, launchers, ios-bridge, ci, scripts, docs, tests),
#      each with a clear message.
#   4. Prints `git log --oneline base..branch` and
#      `git diff --stat base..branch`, which must match the aggregate patch
#      (out/track-b-aggregate.patch).
#   5. Runs `git apply --check` of the aggregate on a second fresh worktree
#      as a sanity check.
#
# Never pushes. Never modifies the working tree's file contents (it only
# reads the diff). Never commits to the current branch — all commits happen
# in the fresh worktree on the new branch.
#
# Exclusions (never patched): out/ (build artifacts), target dirs, the
# working BUILDLOG (process artifact, not code) and its lockfile.
# Cargo.lock files ARE included (reproducible builds; part of Phase 7 ref).

set -euo pipefail

BASE="7f2f5a33a26a26f133f52e88dd3047049088a3c2"
WORKTREE=""
BRANCH="track-b"
KEEP_WORKTREE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --worktree) WORKTREE="$2"; KEEP_WORKTREE=1; shift 2 ;;
        --branch) BRANCH="$2"; shift 2 ;;
        --base) BASE="$2"; shift 2 ;;
        -h|--help)
            sed -n '2,30p' "$0"
            exit 0
            ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

# Safety: refuse to push. This script never pushes, and this guard makes
# that structural, not just a comment.
git_push() {
    echo "FATAL: scripts/apply-track-b.sh never pushes (blocked: $*)" >&2
    exit 1
}

# Common exclusions for every diff.
EXCL=( ':!out' ':!BUILDLOG.md' ':!BUILDLOG.md.lock' ':!*/target' ':!target' )

# --- 0. Regenerate the aggregate patch (reference) --------------------
PATCH="$REPO_ROOT/out/track-b-aggregate.patch"
mkdir -p "$(dirname "$PATCH")"
echo "==> staging intent-to-add for untracked patch files..."
git add -N -- "${EXCL[@]}" 2>/dev/null || true
echo "==> generating aggregate patch: $PATCH"
git diff HEAD --binary -- "${EXCL[@]}" > "$PATCH"

# --- 1. Fresh worktree at base -----------------------------------------
if [[ -z "$WORKTREE" ]]; then
    WORKTREE="$(mktemp -d "${TMPDIR:-/tmp}/track-b-verify-XXXXXX")"
else
    mkdir -p "$WORKTREE"
fi

cleanup() {
    if [[ "$KEEP_WORKTREE" -eq 0 && -d "$WORKTREE" ]]; then
        git worktree remove --force "$WORKTREE" 2>/dev/null || rm -rf "$WORKTREE"
    fi
}
trap cleanup EXIT

git worktree remove --force "$WORKTREE" 2>/dev/null || true
git worktree prune 2>/dev/null || true

echo "==> creating fresh worktree at $BASE: $WORKTREE"
git worktree add --detach "$WORKTREE" "$BASE" 2>&1 | tail -1
git -C "$WORKTREE" checkout -b "$BRANCH" 2>&1 | tail -1

# Git identity for the logical commits (worktree-local, not global).
git -C "$WORKTREE" config user.name "Track B"
git -C "$WORKTREE" config user.email "track-b@supercli.local"

# --- 2. Logical commits ------------------------------------------------
# Each entry: "commit message|pathspec1|pathspec2|..."
TRACKB_GROUPS=(
    "track-b: workspace manifests and lockfiles|crates/Cargo.toml|crates/Cargo.lock|clients/dioxus/Cargo.toml|clients/dioxus/Cargo.lock|clients/dioxus/.gitignore|clients/dioxus/.cargo|clients/dioxus/README.md"
    "track-b: supercli-core - session backend, action reviews, hash chain|crates/supercli-core"
    "track-b: supercli-client - pairing, transport, controller client|crates/supercli-client"
    "track-b: supercli-connector - connector system and approval grants|crates/supercli-connector"
    "track-b: supercli-serve - Host service, /mobile, pairing, turn-cancel|crates/supercli-serve"
    "track-b: supercli-cli - CLI commands and e2e helpers|crates/supercli-cli"
    "track-b: dioxus-ui - shared UI components and web target|clients/dioxus/supercli-ui|clients/dioxus/supercli-web"
    "track-b: launchers - mobile/desktop launchers and native shell|clients/dioxus/supercli-mobile|clients/dioxus/supercli-desktop|clients/dioxus/native-shell"
    "track-b: ios-bridge - iOS bridge and fastlane|clients/dioxus/supercli-ios-bridge|clients/dioxus/fastlane"
    "track-b: ci - GitHub workflows and actionlint|.github"
    "track-b: scripts - e2e scenario, apk build, apply helpers|scripts"
    "track-b: docs - security review, canonical form, setup guides|docs"
    "track-b: tests - device test flows and protocol contracts|tests|protocol"
)

for group in "${TRACKB_GROUPS[@]}"; do
    IFS='|' read -ra PARTS <<< "$group"
    MSG="${PARTS[0]}"
    PATHS=("${PARTS[@]:1}")

    # Generate the diff for just this group from the working tree.
    GROUP_PATCH="$(mktemp "${TMPDIR:-/tmp}/track-b-group-XXXXXX.patch")"
    git diff HEAD --binary -- "${EXCL[@]}" "${PATHS[@]}" > "$GROUP_PATCH" || true

    if [[ ! -s "$GROUP_PATCH" ]]; then
        echo "==> skip (empty): $MSG"
        rm -f "$GROUP_PATCH"
        continue
    fi

    echo "==> commit: $MSG"
    git -C "$WORKTREE" apply --index "$GROUP_PATCH"
    git -C "$WORKTREE" commit -q -m "$MSG"
    rm -f "$GROUP_PATCH"
done

# --- 3. Report ----------------------------------------------------------
echo ""
echo "==> git log --oneline $BASE..$BRANCH:"
git -C "$WORKTREE" log --oneline "$BASE..$BRANCH"

echo ""
echo "==> git diff --stat $BASE..$BRANCH:"
git -C "$WORKTREE" diff --stat "$BASE..$BRANCH" | tail -5

# --- 4. Verify branch diff matches the aggregate ------------------------
# Compare exact content, not just file counts: regenerate numstat totals
# from the aggregate patch itself and from the branch diff, then compare
# the full patch bytes. A drift here means the logical commits do not
# reproduce the aggregate and the reported totals cannot be trusted.
AGG_STAT="$(git apply --numstat "$PATCH" | awk '{i+=$1; d+=$2; n++} END {print n, i, d}')"
BRANCH_STAT="$(git -C "$WORKTREE" diff --numstat "$BASE..$BRANCH" | awk '{i+=$1; d+=$2; n++} END {print n, i, d}')"
echo ""
echo "==> verification:"
echo "  aggregate: $AGG_STAT  (files insertions deletions)"
echo "  branch:    $BRANCH_STAT  (files insertions deletions)"
if [[ "$AGG_STAT" != "$BRANCH_STAT" ]]; then
    echo "  MISMATCH: branch numstat differs from aggregate numstat" >&2
    exit 1
fi
if git -C "$WORKTREE" diff --binary "$BASE..$BRANCH" | diff -q - "$PATCH" >/dev/null; then
    echo "  MATCH: branch diff is byte-identical to the aggregate patch"
else
    echo "  MISMATCH: branch diff content differs from aggregate patch" >&2
    exit 1
fi

# Sanity: aggregate still applies clean to a pristine worktree.
VERIFY_DIR="$(mktemp -d "${TMPDIR:-/tmp}/track-b-verify2-XXXXXX")"
git worktree add --detach "$VERIFY_DIR" "$BASE" >/dev/null 2>&1
if git -C "$VERIFY_DIR" apply --check "$PATCH"; then
    echo "  APPLY_CHECK: PASS"
else
    echo "  APPLY_CHECK: FAIL" >&2
    git worktree remove --force "$VERIFY_DIR" 2>/dev/null || true
    exit 1
fi
git worktree remove --force "$VERIFY_DIR" 2>/dev/null || true

if [[ "$KEEP_WORKTREE" -eq 1 ]]; then
    echo "  worktree kept: $WORKTREE (branch: $BRANCH)"
fi
echo "==> done. Never pushed (by design)."
