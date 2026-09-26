#!/usr/bin/env bash
# scripts/device-bench.sh — Apple Silicon Mac benchmark for the native scrcpy client.
#
# This is the "prove 60 fps" path. CI runners have no GPU: the emulator
# renders with SwiftShader and software-encodes 1080x2400 on 2-4 vCPUs, so
# CI is a CORRECTNESS gate only (handshake, >=100 packets, pinch/HOME ok,
# misses <= 10%, control p50 < adb p50). Real fps is proven here, on a Mac
# with HVF + a real GPU.
#
# What it does:
#   1. Boots a headless pixel_7 (1080x2400, API 34, arm64) emulator with
#      HVF acceleration and host GPU (-no-window, -no-audio).
#   2. Runs the same e2e as CI (crates/supercli-device/tests/android_e2e.rs):
#      60 s of H.264 while Settings fling-scrolls, 20 control-channel +
#      20 adb-input tap-to-frame trials, two-pointer pinch.
#   3. Prints metrics.json — the same schema CI uploads.
#
# Prerequisites (one-time):
#   - Xcode command line tools:            xcode-select --install
#   - Java 11+ (for sdkmanager/avdmanager): brew install openjdk@17
#   - Android SDK cmdline-tools, platform-tools, emulator:
#       export ANDROID_SDK_ROOT="$HOME/Library/Android/sdk"
#       (install via Android Studio, or:)
#       mkdir -p "$ANDROID_SDK_ROOT/cmdline-tools" && cd "$ANDROID_SDK_ROOT/cmdline-tools"
#       curl -o tools.zip https://dl.google.com/android/repository/commandlinetools-mac-11076708_latest.zip
#       unzip tools.zip && mv cmdline-tools latest
#       yes | "$ANDROID_SDK_ROOT/cmdline-tools/latest/bin/sdkmanager" --licenses
#       "$ANDROID_SDK_ROOT/cmdline-tools/latest/bin/sdkmanager" \
#           "platform-tools" "emulator" "system-images;android-34;google_apis;arm64-v8a"
#     (this script installs the system image itself if missing)
#   - Rust: https://rustup.rs (the script uses ~/.cargo/bin/cargo)
#
# Usage:
#   bash scripts/device-bench.sh
#
# Env overrides:
#   AVD_NAME=supercli-bench   existing AVD is reused; created if missing
#   API_LEVEL=34              system image api level (arm64-v8a google_apis)
#   DEVICE_PROFILE=pixel_7    avdmanager -d profile (1080x2400)
#   ANDROID_SERIAL=emulator-5554
#   OUT_DIR=<repo>/.bench-out metrics.json, e2e.log, screenshot.png land here
#   WIPE_DATA=1               wipe the AVD before boot (default: keep snapshot)
#   KEEP_EMULATOR=1           leave the emulator running afterwards
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
AVD_NAME="${AVD_NAME:-supercli-bench}"
API_LEVEL="${API_LEVEL:-34}"
DEVICE_PROFILE="${DEVICE_PROFILE:-pixel_7}"
SERIAL="${ANDROID_SERIAL:-emulator-5554}"
OUT_DIR="${OUT_DIR:-$REPO/.bench-out}"
WIPE_DATA="${WIPE_DATA:-0}"
KEEP_EMULATOR="${KEEP_EMULATOR:-0}"

SDK="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-$HOME/Library/Android/sdk}}"
EMULATOR="$SDK/emulator/emulator"
ADB="$SDK/platform-tools/adb"
SDKMGR="$SDK/cmdline-tools/latest/bin/sdkmanager"
AVDMGR="$SDK/cmdline-tools/latest/bin/avdmanager"
IMG="system-images;android-${API_LEVEL};google_apis;arm64-v8a"

mkdir -p "$OUT_DIR"

echo "=== device-bench: prerequisites ==="
for bin in "$EMULATOR" "$ADB"; do
  [ -x "$bin" ] || { echo "FATAL: $bin not found. Set ANDROID_SDK_ROOT (see script header)."; exit 127; }
done
command -v curl >/dev/null || { echo "FATAL: curl not on PATH"; exit 127; }
export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null || { echo "FATAL: cargo not on PATH (https://rustup.rs)"; exit 127; }
echo "sdk: $SDK"
echo "repo: $REPO"

echo "=== device-bench: acceleration check (want hvf) ==="
if "$EMULATOR" -accel-check 2>&1 | tee "$OUT_DIR/accel-check.txt" | grep -qi "hvf"; then
  echo "OK: HVF acceleration available"
else
  echo "WARNING: HVF not detected — results will be SwiftShader-bound, like CI."
  echo "  On Apple Silicon this usually means the emulator needs Rosetta-free"
  echo "  arm64 images (this script uses arm64-v8a) or a macOS update."
fi

echo "=== device-bench: system image ==="
if ! "$AVDMGR" list avd 2>/dev/null | grep -q "Name: $AVD_NAME"; then
  echo "installing $IMG (one-time, ~1.5 GB) ..."
  [ -x "$SDKMGR" ] || { echo "FATAL: $SDKMGR not found (install cmdline-tools, see header)"; exit 127; }
  yes | "$SDKMGR" --licenses >/dev/null 2>&1 || true
  "$SDKMGR" "$IMG"
  echo "creating AVD '$AVD_NAME' (profile $DEVICE_PROFILE) ..."
  echo "no" | "$AVDMGR" create avd -n "$AVD_NAME" -k "$IMG" -d "$DEVICE_PROFILE" --force
else
  echo "reusing existing AVD '$AVD_NAME'"
fi

# --- Boot headless ---------------------------------------------------------
EMULATOR_PID=""
cleanup() {
  if [ "$KEEP_EMULATOR" = "0" ] && [ -n "$EMULATOR_PID" ] && kill -0 "$EMULATOR_PID" 2>/dev/null; then
    echo "=== device-bench: stopping emulator ==="
    "$ADB" -s "$SERIAL" emu kill 2>/dev/null || kill "$EMULATOR_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

BOOT_ARGS=(-avd "$AVD_NAME" -no-window -no-audio -no-boot-anim -gpu host -no-snapshot-save -port 5554)
if [ "$WIPE_DATA" = "1" ]; then
  BOOT_ARGS+=(-wipe-data)
fi
echo "=== device-bench: booting (${BOOT_ARGS[*]}) ==="
"$EMULATOR" "${BOOT_ARGS[@]}" > "$OUT_DIR/emulator-boot.log" 2>&1 &
EMULATOR_PID=$!
echo "emulator pid: $EMULATOR_PID"

echo "=== device-bench: waiting for boot (up to 10 min) ==="
"$ADB" wait-for-device
deadline=$((SECONDS + 600))
while [ "$("$ADB" -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" != "1" ]; do
  if [ $SECONDS -ge $deadline ]; then
    echo "FATAL: emulator did not finish booting in 10 min (see $OUT_DIR/emulator-boot.log)"
    exit 1
  fi
  sleep 5
done
echo "boot_completed=1"
deadline=$((SECONDS + 120))
until "$ADB" -s "$SERIAL" shell pm path android >/dev/null 2>&1; do
  if [ $SECONDS -ge $deadline ]; then
    echo "FATAL: package manager never became ready"
    exit 1
  fi
  sleep 2
done
echo "package manager ready"
"$ADB" -s "$SERIAL" logcat -c || true

# --- Run the same e2e CI runs ----------------------------------------------
echo "=== device-bench: running android_e2e (60 s animated window + trials + pinch) ==="
cd "$REPO/crates"
set +e
SUPERCLI_ANDROID_E2E=1 \
ANDROID_SERIAL="$SERIAL" \
METRICS_OUT="$OUT_DIR/metrics.json" \
cargo test -p supercli-device --features device \
  --test android_e2e -- --nocapture 2>&1 | tee "$OUT_DIR/e2e.log"
TEST_STATUS=${PIPESTATUS[0]}
set -e
echo "=== device-bench: test exit=$TEST_STATUS ==="

# --- Artifacts ---------------------------------------------------------------
"$ADB" -s "$SERIAL" exec-out screencap -p > "$OUT_DIR/screenshot.png" 2>/dev/null || true
if [ "$TEST_STATUS" -eq 0 ]; then
  "$ADB" -s "$SERIAL" shell screenrecord --time-limit 10 /sdcard/bench.mp4
  "$ADB" -s "$SERIAL" pull /sdcard/bench.mp4 "$OUT_DIR/bench.mp4" >/dev/null
  echo "screen recording: $OUT_DIR/bench.mp4"
fi

echo ""
echo "==================== metrics.json ===================="
if [ -f "$OUT_DIR/metrics.json" ]; then
  cat "$OUT_DIR/metrics.json"
else
  echo "(no metrics.json — the test did not complete)"
fi
echo "======================================================"
echo "log:        $OUT_DIR/e2e.log"
echo "screenshot: $OUT_DIR/screenshot.png"

exit "$TEST_STATUS"
