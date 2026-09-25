#!/usr/bin/env bash
# dev-blank.sh — build, stably sign, and launch Supercli.app against a BLANK,
# throwaway state dir so you can test first-run behavior (builtin presets
# seeded, superpowers on by default — there is no onboarding wizard anymore)
# without touching your real ~/.supercli (sessions, projects, presets, license).
#
# How it works: the app and the hosted `supercli-host` it spawns both honor the
# SUPERCLI_HOME env var (LaunchConfig.supercliDir / app_paths::supercli_home). We
# point it at a fresh dir, so the app boots as if installed for the first
# time. SUPERCLI_HOME deliberately does NOT start with
# SUPERCLI_TEST_/SUPERCLI_SNAPSHOT, which would arm the snapshot harnesses.
#
# We launch the executable directly (not `open`) so the env var is inherited —
# `open` does not forward arbitrary environment, and homeDirectoryForCurrentUser
# ignores $HOME, which is why a dedicated SUPERCLI_HOME var is needed.
#
#   ./dev-blank.sh                 # fresh temp dir each run (auto-cleaned hint)
#   SUPERCLI_HOME=~/supercli-dev ./dev-blank.sh   # reuse a fixed dir across runs
#   RESET=1 SUPERCLI_HOME=~/supercli-dev ./dev-blank.sh   # wipe the fixed dir first
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Stable signing identity (same rationale as dev-app.sh): never ad-hoc, so the
# license keychain ACL keeps matching across rebuilds.
if [ -z "${CODESIGN_IDENTITY:-}" ]; then
  CODESIGN_IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
    | grep "Apple Development" \
    | head -n1 \
    | sed -E 's/^[[:space:]]*[0-9]+\)[[:space:]]+[0-9A-F]+[[:space:]]+"(.*)"$/\1/')"
fi

if [ -z "$CODESIGN_IDENTITY" ] || [ "$CODESIGN_IDENTITY" = "-" ]; then
  echo "error: no stable code-signing identity found." >&2
  echo "       Set CODESIGN_IDENTITY to a local cert, e.g.:" >&2
  echo "       security find-identity -v -p codesigning" >&2
  exit 1
fi

# Resolve the blank state dir. Default: a fresh per-run temp dir.
if [ -z "${SUPERCLI_HOME:-}" ]; then
  SUPERCLI_HOME="$(mktemp -d "${TMPDIR:-/tmp}/supercli-blank.XXXXXX")"
  echo "==> blank state dir (temp): $SUPERCLI_HOME"
else
  # Expand a leading ~ for convenience.
  SUPERCLI_HOME="${SUPERCLI_HOME/#\~/$HOME}"
  if [ "${RESET:-0}" = "1" ] && [ -d "$SUPERCLI_HOME" ]; then
    echo "==> RESET: wiping $SUPERCLI_HOME"
    rm -rf "$SUPERCLI_HOME"
  fi
  mkdir -p "$SUPERCLI_HOME"
  echo "==> blank state dir: $SUPERCLI_HOME"
fi
export SUPERCLI_HOME

echo "==> dev build, signing with: $CODESIGN_IDENTITY"
SUPERCLI_DEV_BUILD=1 CODESIGN_IDENTITY="$CODESIGN_IDENTITY" "$HERE/build-app.sh"

EXE="$HERE/dist/Supercli.app/Contents/MacOS/SupercliNative"
echo "==> launching $EXE (SUPERCLI_HOME=$SUPERCLI_HOME)"
echo "    quit the app to return to the shell; the app boots blank on first launch."
exec "$EXE"
