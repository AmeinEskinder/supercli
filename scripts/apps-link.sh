#!/usr/bin/env bash
# Dev mode for the first-party Apps: build every App in crates/apps and point
# the Host's managed slot (~/.supercli/apps/bin) at the build with
# `supercli apps link`, so each rebuild is what the next launch runs. Pass a
# subset of slugs to build/link only those.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SUPERCLI="${SUPERCLI_BIN:-$ROOT/crates/target/release/supercli}"
if [ ! -x "$SUPERCLI" ]; then
  echo "==> building supercli CLI"
  cargo build --release --manifest-path "$ROOT/crates/Cargo.toml" -p supercli-cli
fi
slugs=("$@")
if [ ${#slugs[@]} -eq 0 ]; then
  slugs=(markdown filetree diffs usage)
fi
echo "==> building Apps: ${slugs[*]}"
args=()
for slug in "${slugs[@]}"; do args+=(-p "supercli-$slug"); done
cargo build --release --manifest-path "$ROOT/crates/apps/Cargo.toml" "${args[@]}"
for slug in "${slugs[@]}"; do
  "$SUPERCLI" apps link "supercli.app.$slug" "$ROOT/crates/apps/target/release/supercli-$slug"
done
"$SUPERCLI" apps list
