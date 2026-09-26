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

# Try MINIMAL command first (no optional args) - if this works, our args are wrong
echo "=== smoke: trying MINIMAL server command ==="
adb -s "$SERIAL" shell "CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 2.7" > "$RUNNER_TEMP/smoke-minimal.log" 2>&1 &
MINIMAL_PID=$!
sleep 8
if kill -0 $MINIMAL_PID 2>/dev/null; then
  echo "smoke MINIMAL: server still alive (GOOD)"
  MINIMAL_OK=1
else
  echo "smoke MINIMAL: server DIED (BAD)"
  MINIMAL_OK=0
fi
echo "=== minimal server log ==="
cat "$RUNNER_TEMP/smoke-minimal.log" || true
kill $MINIMAL_PID 2>/dev/null || true
adb -s "$SERIAL" shell "pkill -f com.genymobile.scrcpy" || true
sleep 2
cp "$RUNNER_TEMP/smoke-minimal.log" "$GITHUB_WORKSPACE/smoke-minimal.log" || true

# Try FULL command (with our args)
echo "=== smoke: trying FULL server command ==="
adb -s "$SERIAL" shell "CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 2.7 video_codec=h264 max_size=1920 max_fps=60 video_bit_rate=8000000 audio=false control=true" > "$RUNNER_TEMP/smoke-server.log" 2>&1 &
SMOKE_PID=$!
sleep 8
# Check if server process is alive on device
adb -s "$SERIAL" shell ps -A | grep -i scrcpy || echo "smoke: no scrcpy process found in ps"
# Check if the adb shell session is still alive
if kill -0 $SMOKE_PID 2>/dev/null; then
  echo "smoke FULL: server adb session still alive (GOOD - server running)"
else
  echo "smoke FULL: server adb session DIED (BAD - server crashed)"
fi
echo "=== smoke server log ==="
cat "$RUNNER_TEMP/smoke-server.log" || true
# Clean up: kill the smoke test server
kill $SMOKE_PID 2>/dev/null || true
adb -s "$SERIAL" shell "pkill -f com.genymobile.scrcpy" || true
sleep 2
# Copy smoke log to workspace for artifact upload
cp "$RUNNER_TEMP/smoke-server.log" "$GITHUB_WORKSPACE/smoke-server.log" || true
echo "=== stage: shell_smoke_test_done (minimal_ok=$MINIMAL_OK) ==="

# Real proof: connect through scrcpy_native, read >=600 H.264 packets,
# measure fps / tap latency / pinch.
cd "$GITHUB_WORKSPACE/crates"
echo "=== stage: cargo_test_start ==="
set +e
SUPERCLI_ANDROID_E2E=1 \
ANDROID_SERIAL="$SERIAL" \
METRICS_OUT="$GITHUB_WORKSPACE/metrics.json" \
cargo test -p supercli-device --features device \
  --test android_e2e -- --nocapture > "$GITHUB_WORKSPACE/e2e.log" 2>&1
TEST_STATUS=$?
set -e
echo "=== stage: cargo_test_done exit=$TEST_STATUS (full output in e2e.log) ==="
tail -60 "$GITHUB_WORKSPACE/e2e.log" || true

# Diagnostics are collected even when the test fails (upload step runs
# `if: always()`), so the next failure explains itself.
echo "=== stage: collect_diagnostics ==="
adb -s "$SERIAL" logcat -d > "$GITHUB_WORKSPACE/e2e-logcat.txt" 2>/dev/null || true
adb -s "$SERIAL" shell ps -A 2>/dev/null | grep -i -E "scrcpy|app_process" \
  > "$GITHUB_WORKSPACE/e2e-server-ps.txt" 2>/dev/null || true

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
