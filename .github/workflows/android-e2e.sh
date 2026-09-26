#!/usr/bin/env bash
# Headless Android emulator e2e for the native scrcpy client.
#
# WHY THIS FILE EXISTS: ReactiveCircus/android-emulator-runner@v2 executes
# each LINE of its `script:` input as a separate `sh -c '<line>'` invocation
# (see parseScript() in the action source). Multi-line shell constructs —
# backslash continuations, `export`, `cd` — DO NOT work in `script:` because
# every line runs in a fresh shell that exits immediately. A line like
# `--test android_e2e -- --nocapture` (a backslash continuation) was executed
# as its own command -> `/usr/bin/sh` exit 127. The real logic therefore
# lives here, invoked from the workflow via a single line:
#   script: bash "$GITHUB_WORKSPACE/.github/workflows/android-e2e.sh"
set -euxo pipefail

echo "=== tool locations (PATH diagnostics) ==="
command -v adb || { echo "FATAL: adb not on PATH"; exit 127; }
command -v cargo || { echo "FATAL: cargo not on PATH"; exit 127; }
command -v emulator || echo "note: emulator not on PATH (ok, the action manages it)"
command -v sdkmanager || echo "note: sdkmanager not on PATH (ok, the action manages it)"
echo "ANDROID_HOME=${ANDROID_HOME:-unset}"
echo "ANDROID_SDK_ROOT=${ANDROID_SDK_ROOT:-unset}"
echo "ANDROID_SERIAL=${ANDROID_SERIAL:-unset}"

export PATH="$HOME/.cargo/bin:$PATH"

# The action exports ANDROID_SERIAL=emulator-<port> into the script env.
SERIAL="${ANDROID_SERIAL:-emulator-5554}"

echo "=== stage: boot_wait ==="
adb wait-for-device
adb -s "$SERIAL" shell 'while [ "$(getprop sys.boot_completed)" != 1 ]; do sleep 2; done'
adb -s "$SERIAL" shell getprop sys.boot_completed
echo "=== stage: boot_completed ==="
# Also wait for the package manager: pushing/running the jar before pm is
# ready can fail on first boot.
echo "=== stage: pm_wait ==="
adb -s "$SERIAL" shell 'for i in $(seq 1 60); do pm path android >/dev/null 2>&1 && break; sleep 2; done; pm path android'
echo "=== stage: pm_ready ==="
ls -la /dev/kvm || true
# Fresh logcat so the post-test dump only covers this run.
adb -s "$SERIAL" logcat -c || true

# --- Shell smoke test: verify scrcpy-server starts OUTSIDE of Rust --------
# If this fails, the problem is the server/emulator, not our Rust code.
# If this succeeds but cargo test fails, the problem is in Rust.
echo "=== stage: shell_smoke_test ==="
SMOKE_JAR="$RUNNER_TEMP/scrcpy-server-v2.7"
curl -fSL --max-time 60 -o "$SMOKE_JAR" \
  https://github.com/Genymobile/scrcpy/releases/download/v2.7/scrcpy-server-v2.7
ls -lh "$SMOKE_JAR"
echo "$SMOKE_JAR" | sha256sum
adb -s "$SERIAL" push "$SMOKE_JAR" /data/local/tmp/scrcpy-server.jar
adb -s "$SERIAL" shell ls -lh /data/local/tmp/scrcpy-server.jar

# Smoke test with Amein's proven server args. If this succeeds but the
# Rust test fails, the problem is in the Rust handshake (not the server).
echo "=== smoke: trying server command ==="
adb -s "$SERIAL" shell "CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 2.7 tunnel_forward=true audio=false control=true cleanup=false video_codec=h264 max_fps=60" > "$RUNNER_TEMP/smoke-server.log" 2>&1 &
SMOKE_PID=$!
sleep 8
# Check if server process is alive on device
adb -s "$SERIAL" shell ps -A | grep -i scrcpy || echo "smoke: no scrcpy process found in ps"
# Check if the adb shell session is still alive
if kill -0 $SMOKE_PID 2>/dev/null; then
  echo "smoke: server adb session still alive (GOOD - server running)"
else
  echo "smoke: server adb session DIED (BAD - server crashed)"
fi
echo "=== smoke server log ==="
cat "$RUNNER_TEMP/smoke-server.log" || true
# Clean up: kill the smoke test server
kill $SMOKE_PID 2>/dev/null || true
adb -s "$SERIAL" shell "pkill -f com.genymobile.scrcpy" || true
sleep 2
# Copy smoke log to workspace for artifact upload
cp "$RUNNER_TEMP/smoke-server.log" "$GITHUB_WORKSPACE/smoke-server.log" || true
echo "=== stage: shell_smoke_test_done ==="

# Real proof: connect through scrcpy_native at full device resolution
# (1080x2400, max_size unset) and at max_size=720, measure fps /
# tap latency / pinch for each. Both runs share the same correctness
# gates (>= 100 packets, pinch ok, HOME ok, misses <= 10%,
# control p50 < adb p50); fps is recorded, not gated.
cd "$GITHUB_WORKSPACE/crates"
echo "=== stage: cargo_test_fullres_start ==="
set +e
SUPERCLI_ANDROID_E2E=1 \
ANDROID_SERIAL="$SERIAL" \
METRICS_OUT="$GITHUB_WORKSPACE/metrics-full.json" \
cargo test -p supercli-device --features device \
  --test android_e2e -- --nocapture > "$GITHUB_WORKSPACE/e2e-full.log" 2>&1
FULL_STATUS=$?
set -e
echo "=== stage: cargo_test_fullres_done exit=$FULL_STATUS ==="

# Clean up the first run's scrcpy server before the second run: the server
# exits when the client's video socket closes, but not instantly — a
# lingering server from the full-res run would fight the 720 run's server
# for the display (the 720 run's `am start` failed with exit 224 when this
# cleanup was missing).
echo "=== stage: inter_run_cleanup ==="
adb -s "$SERIAL" shell "pkill -f com.genymobile.scrcpy" || true
sleep 3
adb -s "$SERIAL" shell ps -A | grep -i scrcpy || echo "inter-run: no scrcpy process left (good)"

# Amein (run #18): force-stop Settings before the 720 run so it starts
# cold. A warm Settings ("Activity not started, its current task has been
# brought to the front") raced the resumed-activity check in run #18.
echo "=== stage: inter_run_force_stop_settings ==="
adb -s "$SERIAL" shell "am force-stop com.android.settings" || true
sleep 1

echo "=== stage: cargo_test_720_start ==="
set +e
SUPERCLI_ANDROID_E2E=1 \
ANDROID_SERIAL="$SERIAL" \
SCRCPY_MAX_SIZE=720 \
METRICS_OUT="$GITHUB_WORKSPACE/metrics-720.json" \
cargo test -p supercli-device --features device \
  --test android_e2e -- --nocapture > "$GITHUB_WORKSPACE/e2e-720.log" 2>&1
TEST_720_STATUS=$?
set -e
echo "=== stage: cargo_test_720_done exit=$TEST_720_STATUS ==="

# Merge both runs' metrics into one metrics.json for the artifact.
# The combined file reports both resolutions side by side.
if [ -f "$GITHUB_WORKSPACE/metrics-full.json" ] && [ -f "$GITHUB_WORKSPACE/metrics-720.json" ]; then
  {
    echo "{"
    echo "  \"full_resolution\": "
    sed 's/^/    /' "$GITHUB_WORKSPACE/metrics-full.json" | sed 's/^    {/    {/' 
    echo "  ,"
    echo "  \"max_size_720\": "
    sed 's/^/    /' "$GITHUB_WORKSPACE/metrics-720.json"
    echo "}"
  } > "$GITHUB_WORKSPACE/metrics.json"
  echo "=== merged metrics.json ==="
  cat "$GITHUB_WORKSPACE/metrics.json"
elif [ -f "$GITHUB_WORKSPACE/metrics-full.json" ]; then
  cp "$GITHUB_WORKSPACE/metrics-full.json" "$GITHUB_WORKSPACE/metrics.json"
  echo "=== only full-res metrics available ==="
else
  echo "=== no metrics.json (neither test completed) ==="
fi

# Overall status: both runs must pass.
if [ "$FULL_STATUS" -ne 0 ]; then
  TEST_STATUS=$FULL_STATUS
elif [ "$TEST_720_STATUS" -ne 0 ]; then
  TEST_STATUS=$TEST_720_STATUS
else
  TEST_STATUS=0
fi
echo "=== stage: cargo_test_done full=$FULL_STATUS 720=$TEST_720_STATUS overall=$TEST_STATUS ==="

# --- Publish key diagnostics to the GitHub Step Summary -------------------
# The step summary is visible on the public Actions run page WITHOUT
# needing artifact download auth. This is the primary diagnostic channel
# for agents without GitHub API tokens.
{
  echo "## Android E2E Diagnostics"
  echo ""
  echo "**Full-res test exit code:** $FULL_STATUS"
  echo "**max_size=720 test exit code:** $TEST_720_STATUS"
  echo ""
  echo "### Last 30 lines of e2e-full.log"
  echo '```'
  tail -30 "$GITHUB_WORKSPACE/e2e-full.log" 2>/dev/null || echo "(no e2e-full.log)"
  echo '```'
  echo ""
  echo "### Last 30 lines of e2e-720.log"
  echo '```'
  tail -30 "$GITHUB_WORKSPACE/e2e-720.log" 2>/dev/null || echo "(no e2e-720.log)"
  echo '```'
  echo ""
  echo "### scrcpy errors from logcat"
  echo '```'
  grep -i "scrcpy\|E/" "$GITHUB_WORKSPACE/e2e-logcat.txt" 2>/dev/null | tail -20 || echo "(no matches)"
  echo '```'
  echo ""
  echo "### scrcpy server stdout/stderr (per-session, by scid)"
  echo '```'
  for f in "${TMPDIR:-/tmp}"/scrcpy-server-*.stdout.log "${TMPDIR:-/tmp}"/scrcpy-server-*.stderr.log; do
    [ -f "$f" ] || continue
    echo "--- $(basename "$f") ---"
    tail -15 "$f"
  done 2>/dev/null || echo "(no scrcpy-server-*.log files)"
  echo '```'
  echo ""
  echo "### metrics.json (both resolutions)"
  echo '```json'
  cat "$GITHUB_WORKSPACE/metrics.json" 2>/dev/null || echo "(no metrics.json)"
  echo '```'
} >> "$GITHUB_STEP_SUMMARY" || true
echo "=== stage: summary_written ==="
echo "--- e2e-full.log tail ---"
tail -30 "$GITHUB_WORKSPACE/e2e-full.log" || true
echo "--- e2e-720.log tail ---"
tail -30 "$GITHUB_WORKSPACE/e2e-720.log" || true

# Diagnostics are collected even when the test fails (upload step runs
# `if: always()`), so the next failure explains itself.
echo "=== stage: collect_diagnostics ==="
adb -s "$SERIAL" logcat -d > "$GITHUB_WORKSPACE/e2e-logcat.txt" 2>/dev/null || true
adb -s "$SERIAL" logcat -d 2>/dev/null | grep -i scrcpy > "$GITHUB_WORKSPACE/e2e-logcat-scrcpy.txt" || true
adb -s "$SERIAL" shell ps -A 2>/dev/null | grep -i -E "scrcpy|app_process" \
  > "$GITHUB_WORKSPACE/e2e-server-ps.txt" 2>/dev/null || true
# scrcpy server stdout/stderr, captured per-session (filenames carry the
# scid) by ScrcpyNative::connect. On a server bind failure or crash these
# hold the server's own error — copy them into the workspace for upload.
cp "${TMPDIR:-/tmp}"/scrcpy-server-*.stdout.log "${TMPDIR:-/tmp}"/scrcpy-server-*.stderr.log \
  "$GITHUB_WORKSPACE/" 2>/dev/null || echo "(no scrcpy-server-*.log files found)"

# On failure, surface the tail of both logs as workflow annotations. These
# are visible on the public run page AND via the unauthenticated check-runs
# API, so a failure can be diagnosed without artifact-download auth.
if [ "$TEST_STATUS" -ne 0 ]; then
  echo "::error::android e2e failed (full=$FULL_STATUS 720=$TEST_720_STATUS)"
  for log in "$GITHUB_WORKSPACE/e2e-full.log" "$GITHUB_WORKSPACE/e2e-720.log"; do
    grep -E "panicked|FAILED|failures:|e2e: (FATAL|stage=)" "$log" 2>/dev/null \
      | tail -8 | while IFS= read -r line; do
        # Annotations must be single-line; truncate pathological lines.
        echo "::error::[$(basename "$log")] ${line:0:400}"
      done
  done
fi

# Screenshot artifact (best effort on failure).
adb -s "$SERIAL" exec-out screencap -p > "$GITHUB_WORKSPACE/screenshot.png" 2>/dev/null || true

# 10 s screen recording only on success: it costs ~15 s and is a demo
# artifact, not a diagnostic.
if [ "$TEST_STATUS" -eq 0 ]; then
  adb -s "$SERIAL" shell screenrecord --time-limit 10 /sdcard/test.mp4
  adb -s "$SERIAL" pull /sdcard/test.mp4 "$RUNNER_TEMP/test.mp4"
  sudo apt-get install -y ffmpeg
  ffmpeg -y -i "$RUNNER_TEMP/test.mp4" -c copy "$GITHUB_WORKSPACE/test-10s.mkv"
fi

if [ -f "$GITHUB_WORKSPACE/metrics.json" ]; then
  echo "=== metrics.json ==="
  cat "$GITHUB_WORKSPACE/metrics.json"
else
  echo "=== no metrics.json (test did not complete) ==="
fi

exit "$TEST_STATUS"
