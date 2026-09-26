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

adb wait-for-device
adb -s "$SERIAL" shell 'while [ "$(getprop sys.boot_completed)" != 1 ]; do sleep 2; done'
adb -s "$SERIAL" shell getprop sys.boot_completed
ls -la /dev/kvm || true

# Real proof: connect through scrcpy_native, read >=600 H.264 packets,
# measure fps / tap latency / pinch.
cd "$GITHUB_WORKSPACE/crates"
SUPERCLI_ANDROID_E2E=1 \
ANDROID_SERIAL="$SERIAL" \
METRICS_OUT="$GITHUB_WORKSPACE/metrics.json" \
cargo test -p supercli-device --features device \
  --test android_e2e -- --nocapture

# Screenshot artifact.
adb -s "$SERIAL" exec-out screencap -p > "$GITHUB_WORKSPACE/screenshot.png"

# 10 s screen recording, remuxed to mkv.
adb -s "$SERIAL" shell screenrecord --time-limit 10 /sdcard/test.mp4
adb -s "$SERIAL" pull /sdcard/test.mp4 "$RUNNER_TEMP/test.mp4"
sudo apt-get install -y ffmpeg
ffmpeg -y -i "$RUNNER_TEMP/test.mp4" -c copy "$GITHUB_WORKSPACE/test-10s.mkv"

echo "=== metrics.json ==="
cat "$GITHUB_WORKSPACE/metrics.json"
