#!/bin/sh
# Local twin of .github/workflows/notices-check.yml: regenerate the
# third-party notices snapshot for the three CLI crates and diff it against
# the committed THIRD_PARTY_NOTICES.txt. Exit 0 = in sync.
set -eu
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
tmp=$(mktemp "${TMPDIR:-/tmp}/supercli-notices.XXXXXX")
trap 'rm -f "$tmp"' EXIT INT TERM
cargo run --quiet --locked --manifest-path "$repo_root/crates/Cargo.toml" -p supercli-license-notices -- \
  --manifest-path "$repo_root/crates/Cargo.toml" --package supercli-cli --package supercli-host \
  --manifest-path "$repo_root/crates/supercli-attach/Cargo.toml" --package supercli-attach \
  --target aarch64-apple-darwin --target x86_64-apple-darwin \
  --target x86_64-unknown-linux-gnu --target aarch64-unknown-linux-gnu \
  --output "$tmp"
if diff -u "$repo_root/THIRD_PARTY_NOTICES.txt" "$tmp"; then
  echo "THIRD_PARTY_NOTICES.txt is in sync"
else
  echo "error: THIRD_PARTY_NOTICES.txt is stale — regenerate it (see NOTICE.md) and commit" >&2
  exit 1
fi
