#!/bin/sh
# Moved to scripts/verify-attach.sh (supercli-attach lives in crates/ since 2026-09-03).
exec "$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)/scripts/verify-attach.sh" "$@"
