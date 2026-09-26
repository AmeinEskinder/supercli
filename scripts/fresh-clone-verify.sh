#!/usr/bin/env bash
#
# fresh-clone-verify.sh — verify a fresh clone of the supercli-main-v2 candidate tree.
#
# EXCLUSION LIST (deleted from the clone before verification; the run fails if
# any of them is still present afterwards):
#   docs/internal/buildlog.md   — internal build journal, not user documentation
#   handoff.md                  — transient agent handoff notes
#   phases/                     — historical phase working notes from the pre-rename tree
#   pr-draft.md                 — draft PR text, not a deliverable
#
# Flow: fresh-clone (ref + submodules) -> apply exclusions -> run checks.
# Every step reports PASS / FAIL / SKIP; any FAIL makes the exit code non-zero.
#
# Env:
#   SUPERCLI_REMOTE   git URL to clone (default: git@github.com:AmeinEskinder/supercli.git)
#   SUPERCLI_REF      branch to check out (default: supercli-next)
#   TMPDIR            scratch dir for the clone (default: /tmp). Point this at
#                     a large disk for big trees — the clone plus cargo target/
#                     can exceed small tmpfs mounts (e.g. 512M).
#   KEEP_DIR=1        keep the temp clone for inspection (default: deleted on exit)
#   SKIP_DART=1       skip the dart steps (no Dart SDK on this machine)
#   SKIP_NOTICES=1    skip the third-party notices regeneration (slow; needs cargo + network)
#   IDENTITY_SCOPE    all = every reachable commit (default); new = only commits
#                     after merge-base with origin/main (mirrors identity-check.yml)
#
# Dry run before supercli-next exists:
#   SUPERCLI_REMOTE=/path/to/repo SUPERCLI_REF=track-b-phase14 \
#     SKIP_DART=1 scripts/fresh-clone-verify.sh
#
set -uo pipefail

REMOTE="${SUPERCLI_REMOTE:-git@github.com:AmeinEskinder/supercli.git}"
REF="${SUPERCLI_REF:-supercli-next}"
SCOPE="${IDENTITY_SCOPE:-all}"
EXPECTED_NAME="Amein Eskinder"
EXPECTED_EMAIL="62555273+AmeinEskinder@users.noreply.github.com"
EXCLUSIONS="docs/internal/buildlog.md handoff.md phases/ pr-draft.md"
WORKSPACES="crates/Cargo.toml crates/supercli-attach/Cargo.toml crates/apps/Cargo.toml"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/supercli-v2-verify.XXXXXX")"
CLONE="$WORK/tree"
LOG="$WORK/clone.log"
if [ "${KEEP_DIR:-0}" != "1" ]; then
  trap 'rm -rf "$WORK"' EXIT INT TERM
fi

pass=0; fail=0; skip=0
step() { printf '\n==> %s\n' "$1"; }
ok()   { pass=$((pass + 1)); printf 'PASS: %s\n' "$1"; }
no()   { fail=$((fail + 1)); printf 'FAIL: %s\n' "$1"; }
sk()   { skip=$((skip + 1)); printf 'SKIP: %s\n' "$1"; }

# ---------------------------------------------------------------- clone ---

step "fresh clone $REMOTE (ref $REF) with --recurse-submodules"
if git clone --recurse-submodules -b "$REF" "$REMOTE" "$CLONE" >"$LOG" 2>&1; then
  ok "cloned at $(git -C "$CLONE" rev-parse --short HEAD) (log: $LOG)"
else
  no "git clone failed (log: $LOG)"
  tail -20 "$LOG"
  printf '\nRESULT: %d pass, %d fail, %d skip\n' "$pass" "$fail" "$skip"
  exit 1
fi

# ------------------------------------------------------------ exclusions ---

step "apply exclusion list"
p=""
for p in $EXCLUSIONS; do rm -rf "$CLONE/$p"; done
leftover=0
for p in $EXCLUSIONS; do
  if [ -e "$CLONE/$p" ]; then printf '  still present: %s\n' "$p"; leftover=1; fi
done
if [ "$leftover" = "0" ]; then ok "exclusions applied: $EXCLUSIONS"; else no "exclusion paths remain"; fi

# ------------------------------------------------------------- identity ---

step "identity check (scope: $SCOPE, expected: $EXPECTED_NAME <$EXPECTED_EMAIL>)"
commits=""
if [ "$SCOPE" = "new" ]; then
  base=""
  if git -C "$CLONE" rev-parse --verify origin/main >/dev/null 2>&1; then
    base="$(git -C "$CLONE" merge-base HEAD origin/main 2>/dev/null || true)"
  fi
  if [ -z "$base" ]; then
    commits="$(git -C "$CLONE" rev-parse HEAD)"
  else
    commits="$(git -C "$CLONE" rev-list "${base}..HEAD")"
  fi
else
  commits="$(git -C "$CLONE" rev-list HEAD)"
fi
bad=0
for sha in $commits; do
  an="$(git -C "$CLONE" log -1 --format='%an' "$sha")"
  ae="$(git -C "$CLONE" log -1 --format='%ae' "$sha")"
  cn="$(git -C "$CLONE" log -1 --format='%cn' "$sha")"
  ce="$(git -C "$CLONE" log -1 --format='%ce' "$sha")"
  if [ "$an" != "$EXPECTED_NAME" ] || [ "$ae" != "$EXPECTED_EMAIL" ] || \
     [ "$cn" != "$EXPECTED_NAME" ] || [ "$ce" != "$EXPECTED_EMAIL" ]; then
    printf '  offender: %s author=%s <%s> committer=%s <%s>\n' "$sha" "$an" "$ae" "$cn" "$ce"
    bad=1
  fi
done
if [ "$bad" = "1" ]; then no "identity check: non-Amein commits present"; else ok "identity check"; fi

# ---------------------------------------------------------- rename guard ---
# NOTE: the forbidden literal is assembled at runtime ("unp""eel") so this
# script itself never contains it — the guard scans this file too.

step "rename guard (0 forbidden references outside allowlist)"
U="unp""eel"
matches="$(grep -rli "$U" "$CLONE" \
  --exclude-dir=.git \
  --exclude-dir=target \
  --exclude-dir=node_modules \
  --exclude-dir=__pycache__ \
  --exclude-dir=gpuidart \
  --exclude-dir=.dart_tool \
  --exclude='*.lock' \
  --exclude='*.pyc' \
  --exclude='*.a' \
  | grep -v -e "^$CLONE/docs/book/" \
           -e "^$CLONE/docs/internal/" \
           -e "^$CLONE/clients/gpuidart/" \
           -e "^$CLONE/clients/legacy/" \
           -e "^$CLONE/tests/device/" \
           -e 'scripts/generate-runtime-client-catalog.mjs$' \
           -e 'scripts/release-app.mjs$' \
           -e 'scripts/release-app-state.mjs$' \
           -e 'scripts/publish-cloudflare-release.mjs$' \
           -e 'scripts/release-app-installer.test.mjs$' \
           -e 'scripts/release-app-state.test.mjs$' \
           -e 'generated/GeneratedRuntimeCatalog.swift$' \
           -e 'CHANGELOG.md' \
           -e 'THIRD_PARTY_NOTICES.txt' \
           -e 'crates/apps/diffs/LICENSE' \
           -e 'crates/apps/filetree/LICENSE' \
           -e "crates/supercli-cli/src/import_${U}_cli.rs" \
           -e 'crates/supercli-cli/src/cli.rs' \
           -e 'crates/supercli-cli/src/main.rs' \
           -e 'crates/supercli-core/fuzz/out/' \
           -e 'package-lock.json' \
           -e 'docs/rename-allowlist.md' \
           -e '.github/workflows/rename-guard.yml' \
  || true)"
if [ -n "$matches" ]; then
  printf '  forbidden references found:\n%s\n' "$matches" | sed 's/^/  /'
  no "rename guard"
else
  ok "rename guard: 0 forbidden references outside allowlist"
fi

# ---------------------------------------------------------- secret scan ---

step "secret scan (gitleaks, full history)"
GITLEAKS=""
if command -v gitleaks >/dev/null 2>&1; then
  GITLEAKS="gitleaks"
else
  mkdir -p "$WORK/bin"
  if curl -sSfL https://github.com/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_linux_x64.tar.gz \
      | tar -xz -C "$WORK/bin" gitleaks 2>/dev/null; then
    chmod +x "$WORK/bin/gitleaks"
    GITLEAKS="$WORK/bin/gitleaks"
  fi
fi
if [ -z "$GITLEAKS" ]; then
  no "secret scan: gitleaks not installed and download failed"
elif [ ! -f "$CLONE/.gitleaks.toml" ]; then
  no "secret scan: $CLONE/.gitleaks.toml missing"
elif ( cd "$CLONE" && "$GITLEAKS" git --config .gitleaks.toml --no-banner --redact --verbose . >"$WORK/gitleaks.log" 2>&1 ); then
  ok "secret scan: no leaks (log: $WORK/gitleaks.log)"
else
  no "secret scan: leaks found (log: $WORK/gitleaks.log)"
  tail -20 "$WORK/gitleaks.log"
fi

# ------------------------------------------------------------ licenses ---

step "license check (LICENSE + NOTICE attributions preserved)"
lic_ok=1
[ -f "$CLONE/LICENSE" ] || { printf '  missing: LICENSE\n'; lic_ok=0; }
[ -f "$CLONE/THIRD_PARTY_NOTICES.txt" ] || { printf '  missing: THIRD_PARTY_NOTICES.txt\n'; lic_ok=0; }
while IFS= read -r artifact; do
  if [ ! -f "$artifact/LICENSE" ] && [ ! -f "$artifact/LICENSE.md" ] && [ ! -f "$artifact/LICENSE.txt" ]; then
    printf '  vendored artifact without LICENSE: %s\n' "$artifact"
    lic_ok=0
  fi
done < <(for dir in $(find "$CLONE/crates" -type d -name vendor -not -path '*/target/*' 2>/dev/null); do
           for artifact in "$dir"/*/; do [ -d "$artifact" ] && printf '%s\n' "$artifact"; done
         done)
if [ "$lic_ok" = "1" ]; then ok "license check"; else no "license check"; fi

# -------------------------------------------------------------- notices ---

step "third-party notices snapshot in sync"
if [ "${SKIP_NOTICES:-0}" = "1" ]; then
  sk "notices check (SKIP_NOTICES=1)"
elif [ ! -x "$CLONE/scripts/check-notices.sh" ]; then
  no "notices check: scripts/check-notices.sh missing or not executable"
elif "$CLONE/scripts/check-notices.sh" >"$WORK/notices.log" 2>&1; then
  ok "notices check: THIRD_PARTY_NOTICES.txt in sync"
else
  no "notices check: THIRD_PARTY_NOTICES.txt stale (log: $WORK/notices.log)"
  tail -20 "$WORK/notices.log"
fi

# ---------------------------------------------------------- cargo build ---

if ! command -v cargo >/dev/null 2>&1; then
  step "cargo build (workspace)"; no "cargo not installed"
  step "cargo test (workspace)";  no "cargo not installed"
else
  for ws in $WORKSPACES; do
    step "cargo build $ws"
    if [ ! -f "$CLONE/$ws" ]; then
      sk "workspace manifest missing: $ws"
    elif cargo build --locked --workspace --manifest-path "$CLONE/$ws" >"$WORK/build-$(basename "$(dirname "$ws")").log" 2>&1; then
      ok "cargo build $ws"
    else
      no "cargo build $ws"
    fi
  done
  for ws in $WORKSPACES; do
    step "cargo test $ws"
    if [ ! -f "$CLONE/$ws" ]; then
      sk "workspace manifest missing: $ws"
    elif cargo test --locked --workspace --manifest-path "$CLONE/$ws" >"$WORK/test-$(basename "$(dirname "$ws")").log" 2>&1; then
      ok "cargo test $ws"
    else
      no "cargo test $ws"
    fi
  done
fi

# ----------------------------------------------------------------- dart ---

step "dart analyze + dart test (clients/gpuidart)"
if [ "${SKIP_DART:-0}" = "1" ]; then
  sk "dart steps (SKIP_DART=1)"
elif ! command -v dart >/dev/null 2>&1; then
  no "dart not installed"
elif [ ! -d "$CLONE/clients/gpuidart" ]; then
  no "clients/gpuidart missing"
else
  if ( cd "$CLONE/clients/gpuidart" && dart analyze >"$WORK/dart-analyze.log" 2>&1 ); then
    ok "dart analyze: 0 issues"
  else
    no "dart analyze"
    tail -20 "$WORK/dart-analyze.log"
  fi
  if ( cd "$CLONE/clients/gpuidart" && dart test >"$WORK/dart-test.log" 2>&1 ); then
    ok "dart test"
  else
    no "dart test"
    tail -20 "$WORK/dart-test.log"
  fi
fi

# --------------------------------------------------------------- summary ---

printf '\n========================================\n'
printf 'RESULT: %d pass, %d fail, %d skip\n' "$pass" "$fail" "$skip"
printf 'workdir: %s\n' "$WORK"
printf '========================================\n'
[ "$fail" = "0" ]
