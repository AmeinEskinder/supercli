#!/usr/bin/env bash
# Q5 — CI performance gate.
#
# Runs scripts/perf-bench.py (release build) and asserts:
# - approve latency p50/p95/p99 are within 30% above the recorded baseline
# - approve throughput stays above 70% of the recorded baseline (floor)
#
# PROVISIONAL: The committed baseline was measured on a dev VM (2 vCPU,
# AMD EPYC 9D25, overlay disk), NOT on the CI runner. Thresholds are
# provisional until the baseline is re-measured on the CI runner.
# To establish the CI baseline: run scripts/perf-bench.py on the runner
# and commit the resulting scripts/perf-baseline.json.
#
# Exit 0 = within budget, 1 = regression, 2 = infra failure.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
baseline="$here/perf-baseline.json"

if [[ ! -f "$baseline" ]]; then
  echo "no baseline at $baseline — run scripts/perf-bench.py first" >&2
  exit 2
fi

echo "NOTE: thresholds provisional — baseline not yet measured on CI runner" >&2

# Run the bench (writes $baseline via PERF_OUTPUT override if set).
measured="$(mktemp)"
export PERF_OUTPUT="$measured"
if ! python3 "$here/perf-bench.py"; then
  echo "perf bench failed to run" >&2
  rm -f "$measured"
  exit 2
fi

python3 - "$baseline" "$measured" << 'PYEOF'
import json, sys

with open(sys.argv[1]) as f:
    base = json.load(f)
with open(sys.argv[2]) as f:
    meas = json.load(f)

ok = True
for key in ("p50", "p95", "p99"):
    b = base["approve_latency_ms"][key]
    m = meas["approve_latency_ms"][key]
    limit = b * 1.30
    status = "OK " if m <= limit else "FAIL"
    if m > limit:
        ok = False
    print(f"approve {key}: baseline={b:.2f}ms measured={m:.2f}ms "
          f"limit={limit:.2f}ms [{status}]")

b_tp = base["capacity"]["conc_1"]["throughput_per_s"]
m_tp = meas["capacity"]["conc_1"]["throughput_per_s"]
floor = b_tp * 0.70
status = "OK " if m_tp >= floor else "FAIL"
if m_tp < floor:
    ok = False
print(f"throughput: baseline={b_tp:.2f}/s measured={m_tp:.2f}/s "
      f"floor={floor:.2f}/s [{status}]")

sys.exit(0 if ok else 1)
PYEOF
rc=$?
rm -f "$measured"
exit $rc
