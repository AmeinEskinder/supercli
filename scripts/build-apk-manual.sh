#!/bin/bash
# Manual Android APK assembly without Gradle — fully self-contained end to end.
#
# Builds supercli-mobile as a cdylib for arm64-v8a and x86_64, resolves the
# androidx AAR/JAR closure from Google's Maven repo, renders wry's Kotlin
# activity templates (package dev.dioxus.main, native lib "main"), compiles
# them with kotlinc, dexes with d8, links with aapt2, packages, zipaligns,
# and signs with a throwaway debug key.
#
# Prerequisites (env vars, or edit the defaults below):
#   ANDROID_SDK   - Android SDK with build-tools 35.0.0, platforms/android-35,
#                   ndk/27.2.12479018   (default: ~/workspace/muse-harness/tmp/android-sdk)
#   JAVA_HOME     - JDK 17+ (or java on PATH)
#   KOTLINC_HOME  - Kotlin compiler 2.x (or kotlinc on PATH)
#   python3       - for the androidx resolver (needs network to dl.google.com)
#   ANDROIDX_DIR  - optional: reuse a previously resolved androidx artifact dir
#
# The script temporarily appends a [lib] cdylib section to
# clients/dioxus/supercli-mobile/Cargo.toml and temporarily writes
# clients/dioxus/.cargo/config.toml for the NDK linkers. Both are restored
# on exit (a pre-existing .cargo/config.toml is preserved, never clobbered).
#
# Output: out/supercli-mobile-manual.apk (signed, zipaligned)
# This is a manual/CI-independent path. It does NOT use Gradle.

set -euo pipefail

fail() { echo "ERROR: $*" >&2; exit 1; }

# --- Configuration ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DIOXUS_DIR="$REPO_ROOT/clients/dioxus"
OUT_DIR="${OUT_DIR:-$REPO_ROOT/out}"
WORK_DIR="${WORK_DIR:-$OUT_DIR/apk-manual-work}"
ANDROIDX_DIR="${ANDROIDX_DIR:-$WORK_DIR/androidx}"

ANDROID_SDK="${ANDROID_SDK:-$HOME/workspace/muse-harness/tmp/android-sdk}"
BUILD_TOOLS="$ANDROID_SDK/build-tools/35.0.0"
ANDROID_JAR="$ANDROID_SDK/platforms/android-35/android.jar"
NDK="$ANDROID_SDK/ndk/27.2.12479018"
NDK_BIN="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"

PACKAGE="dev.dioxus.main"
NATIVE_LIB="main"
MIN_SDK=24
TARGET_SDK=35
APK_OUT="$OUT_DIR/supercli-mobile-manual.apk"

# --- Prerequisite checks ---
[ -x "$BUILD_TOOLS/aapt2" ]    || fail "aapt2 not found in $BUILD_TOOLS (ANDROID_SDK=$ANDROID_SDK)"
[ -x "$BUILD_TOOLS/d8" ]       || fail "d8 not found in $BUILD_TOOLS"
[ -x "$BUILD_TOOLS/zipalign" ] || fail "zipalign not found in $BUILD_TOOLS"
[ -x "$BUILD_TOOLS/apksigner" ]|| fail "apksigner not found in $BUILD_TOOLS"
[ -f "$ANDROID_JAR" ]          || fail "android.jar not found at $ANDROID_JAR"
[ -x "$NDK_BIN/aarch64-linux-android24-clang" ] || fail "NDK clang not found (NDK=$NDK)"
command -v cargo >/dev/null   || fail "cargo not on PATH"
command -v python3 >/dev/null || fail "python3 not on PATH (needed for androidx resolution)"
if [ -n "${JAVA_HOME:-}" ]; then export PATH="$JAVA_HOME/bin:$PATH"; fi
command -v java >/dev/null    || fail "java not on PATH and JAVA_HOME unset (need JDK 17+)"
if [ -n "${KOTLINC_HOME:-}" ]; then
  KOTLINC_BIN="$KOTLINC_HOME/bin/kotlinc"
  KOTLIN_STDLIB="$KOTLINC_HOME/lib/kotlin-stdlib.jar"
elif command -v kotlinc >/dev/null; then
  KOTLINC_BIN="$(command -v kotlinc)"
  KOTLIN_STDLIB="$(dirname "$(command -v kotlinc)")/../lib/kotlin-stdlib.jar"
else
  fail "kotlinc not found: set KOTLINC_HOME or put kotlinc on PATH"
fi
[ -x "$KOTLINC_BIN" ]   || fail "kotlinc not executable at $KOTLINC_BIN"
[ -f "$KOTLIN_STDLIB" ] || fail "kotlin-stdlib.jar not found at $KOTLIN_STDLIB"

# --- Work dir (clean) and restore trap ---
CARGO_TOML="$DIOXUS_DIR/supercli-mobile/Cargo.toml"
CARGO_CFG="$DIOXUS_DIR/.cargo/config.toml"
rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"/{lib/arm64-v8a,lib/x86_64,dex,classes,res/values} "$ANDROIDX_DIR" "$OUT_DIR"
CARGO_TOML_BAK="$WORK_DIR/Cargo.toml.bak"
CARGO_CFG_BAK="$WORK_DIR/cargo-config.toml.bak"
CFG_CREATED=0
cleanup() {
  if [ -f "$CARGO_TOML_BAK" ]; then cp "$CARGO_TOML_BAK" "$CARGO_TOML"; fi
  if [ -f "$CARGO_CFG_BAK" ]; then
    cp "$CARGO_CFG_BAK" "$CARGO_CFG"
  elif [ "$CFG_CREATED" = 1 ]; then
    rm -f "$CARGO_CFG"
  fi
}
trap cleanup EXIT

echo "=== Manual APK build for supercli-mobile ==="
echo "Work dir: $WORK_DIR"

# --- 1. NDK toolchain wrappers (cc-rs needs unversioned names) ---
mkdir -p "$WORK_DIR/ndk-bin"
for arch in aarch64 x86_64; do
  cat > "$WORK_DIR/ndk-bin/${arch}-linux-android-clang" <<EOF
#!/bin/sh
exec $NDK_BIN/${arch}-linux-android24-clang "\$@"
EOF
  cat > "$WORK_DIR/ndk-bin/${arch}-linux-android-ar" <<EOF
#!/bin/sh
exec $NDK_BIN/llvm-ar "\$@"
EOF
  chmod +x "$WORK_DIR/ndk-bin/${arch}-linux-android-clang" \
            "$WORK_DIR/ndk-bin/${arch}-linux-android-ar"
done
export PATH="$WORK_DIR/ndk-bin:$PATH"
export CC_aarch64_linux_android="$WORK_DIR/ndk-bin/aarch64-linux-android-clang"
export CC_x86_64_linux_android="$WORK_DIR/ndk-bin/x86_64-linux-android-clang"
export AR_aarch64_linux_android="$WORK_DIR/ndk-bin/aarch64-linux-android-ar"
export AR_x86_64_linux_android="$WORK_DIR/ndk-bin/x86_64-linux-android-ar"

# --- 2. Cargo linker config (temporary; pre-existing file is preserved) ---
if [ -f "$CARGO_CFG" ]; then
  cp "$CARGO_CFG" "$CARGO_CFG_BAK"
  echo "note: existing $CARGO_CFG backed up, will be restored"
else
  CFG_CREATED=1
fi
mkdir -p "$DIOXUS_DIR/.cargo"
cat > "$CARGO_CFG" <<EOF
[target.aarch64-linux-android]
linker = "$NDK_BIN/aarch64-linux-android24-clang"
[target.x86_64-linux-android]
linker = "$NDK_BIN/x86_64-linux-android24-clang"
EOF

# --- 3. Build Rust cdylib for both ABIs ---
# Temporarily add a [lib] cdylib section (a bin crate cannot mix crate types);
# reverted by the EXIT trap even on failure.
cp "$CARGO_TOML" "$CARGO_TOML_BAK"
cat >> "$CARGO_TOML" <<'EOF'

[lib]
name = "supercli_mobile"
path = "src/main.rs"
crate-type = ["cdylib"]
EOF

echo "--- Building Rust for aarch64-linux-android ---"
# NOTE: cargo discovers .cargo/config.toml from the working directory, not
# from --manifest-path, so the builds must run with cwd inside the workspace.
cd "$DIOXUS_DIR"
cargo build --locked -p supercli-mobile --target aarch64-linux-android --lib
cp "target/aarch64-linux-android/debug/libsupercli_mobile.so" \
   "$WORK_DIR/lib/arm64-v8a/libmain.so"

echo "--- Building Rust for x86_64-linux-android ---"
cargo build --locked -p supercli-mobile --target x86_64-linux-android --lib
cp "target/x86_64-linux-android/debug/libsupercli_mobile.so" \
   "$WORK_DIR/lib/x86_64/libmain.so"
cd - >/dev/null

# Sanity: the cdylibs must export the Dioxus Android JNI entry points
for abi_so in "$WORK_DIR/lib/arm64-v8a/libmain.so" "$WORK_DIR/lib/x86_64/libmain.so"; do
  "$NDK_BIN/llvm-nm" -D --defined-only "$abi_so" | grep -q "Java_dev_dioxus_main" \
    || fail "$abi_so has no Java_dev_dioxus_main JNI exports"
done
echo "JNI exports present in both ABIs"

# --- 4. Resolve androidx AAR/JAR closure ---
echo "--- Resolving androidx artifacts ---"
python3 "$SCRIPT_DIR/build-apk-androidx.py" --out "$ANDROIDX_DIR"
JAR_COUNT="$(ls "$ANDROIDX_DIR"/*.jar 2>/dev/null | wc -l)"
[ "$JAR_COUNT" -gt 0 ] || fail "no jars resolved in $ANDROIDX_DIR"
echo "androidx jars: $JAR_COUNT"

# --- 5. Render Kotlin sources from wry's android templates ---
echo "--- Rendering Kotlin sources ---"
WRY_VER="$(grep -A2 '^name = "wry"$' "$DIOXUS_DIR/Cargo.lock" | grep '^version' | head -1 | cut -d'"' -f2)"
[ -n "$WRY_VER" ] || fail "could not determine wry version from Cargo.lock"
WRY_KOTLIN=""
for d in "$HOME"/.cargo/registry/src/*/wry-"$WRY_VER"/src/android/kotlin; do
  if [ -d "$d" ]; then WRY_KOTLIN="$d"; break; fi
done
[ -n "$WRY_KOTLIN" ] || fail "wry $WRY_VER android kotlin templates not found in cargo registry"
echo "wry templates: $WRY_KOTLIN (wry $WRY_VER)"
PKG_DIR="$WORK_DIR/kotlin-src/dev/dioxus/main"
mkdir -p "$PKG_DIR"
for t in "$WRY_KOTLIN"/*.kt; do
  sed -e 's/{{package}}/dev.dioxus.main/g' \
      -e 's/{{library}}/main/g' \
      -e 's/{{class-extension}}//g' \
      -e 's/{{class-init}}//g' \
      "$t" > "$PKG_DIR/$(basename "$t")"
done
# Dioxus Android entry point: thin subclass of wry's activity
cat > "$PKG_DIR/MainActivity.kt" <<'EOF'
package dev.dioxus.main

class MainActivity : WryActivity()
EOF
# Stub for the Gradle-generated BuildConfig referenced by wry's Logger.kt
cat > "$PKG_DIR/BuildConfig.kt" <<'EOF'
package dev.dioxus.main
object BuildConfig { const val DEBUG = true }
EOF
grep -r '{{' "$PKG_DIR" && fail "unrendered template placeholder left in Kotlin sources"
echo "kotlin sources: $(find "$PKG_DIR" -name '*.kt' | wc -l) files"

# --- 6. Compile Kotlin ---
echo "--- Compiling Kotlin ---"
CP="$ANDROID_JAR:$KOTLIN_STDLIB"
for j in "$ANDROIDX_DIR"/*.jar; do CP="$CP:$j"; done
CP="$(printf '%s' "$CP" | tr ':' '\n' | awk '!seen[$0]++' | paste -sd: -)"
# shellcheck disable=SC2086
"$KOTLINC_BIN" -cp "$CP" -d "$WORK_DIR/classes" $(find "$WORK_DIR/kotlin-src" -name '*.kt')
[ -f "$WORK_DIR/classes/dev/dioxus/main/MainActivity.class" ] \
  || fail "kotlinc produced no MainActivity.class"
echo "compiled classes: $(find "$WORK_DIR/classes" -name '*.class' | wc -l)"

# --- 7. DEX with d8 (classes.dex is required, not optional) ---
echo "--- Running d8 ---"
# d8 chokes on kotlinc's META-INF/*.kotlin_module metadata, so jar up the
# classes without META-INF first.
rm -f "$WORK_DIR/app-classes.jar"
(cd "$WORK_DIR/classes" && zip -q -r "$WORK_DIR/app-classes.jar" . -x 'META-INF/*')
"$BUILD_TOOLS/d8" --lib "$ANDROID_JAR" --min-api "$MIN_SDK" \
  --output "$WORK_DIR/dex" "$WORK_DIR/app-classes.jar" "$ANDROIDX_DIR"/*.jar
[ -s "$WORK_DIR/dex/classes.dex" ] || fail "d8 produced no classes.dex"
echo "classes.dex: $(du -h "$WORK_DIR/dex/classes.dex" | cut -f1)"

# --- 8. aapt2 compile/link ---
cat > "$WORK_DIR/AndroidManifest.xml" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="$PACKAGE"
    android:versionCode="1"
    android:versionName="1.0">
    <uses-sdk android:minSdkVersion="$MIN_SDK" android:targetSdkVersion="$TARGET_SDK"/>
    <uses-permission android:name="android.permission.INTERNET"/>
    <application android:label="Unpeel Mobile" android:theme="@android:style/Theme.Material.Light">
        <activity android:name=".MainActivity"
            android:exported="true"
            android:configChanges="orientation|screenSize|keyboardHidden">
            <intent-filter>
                <action android:name="android.intent.action.MAIN"/>
                <category android:name="android.intent.category.LAUNCHER"/>
            </intent-filter>
        </activity>
    </application>
</manifest>
EOF

cat > "$WORK_DIR/res/values/strings.xml" <<'EOF'
<?xml version="1.0" encoding="utf-8"?>
<resources>
    <string name="app_name">Unpeel Mobile</string>
</resources>
EOF

"$BUILD_TOOLS/aapt2" compile --dir "$WORK_DIR/res" -o "$WORK_DIR/compiled-res.zip"
"$BUILD_TOOLS/aapt2" link -o "$WORK_DIR/base.apk" \
  -I "$ANDROID_JAR" \
  --manifest "$WORK_DIR/AndroidManifest.xml" \
  "$WORK_DIR/compiled-res.zip"

# --- 9. Package classes.dex + native libs (stored, not compressed) ---
cp "$WORK_DIR/dex/classes.dex" "$WORK_DIR/classes.dex"
cd "$WORK_DIR"
zip -0 base.apk classes.dex
zip -0 -r base.apk lib
cd - >/dev/null
unzip -l "$WORK_DIR/base.apk" | grep -E 'classes.dex|libmain.so' \
  || fail "base.apk is missing classes.dex or native libs"

# --- 10. zipalign ---
"$BUILD_TOOLS/zipalign" -f 4 "$WORK_DIR/base.apk" "$WORK_DIR/aligned.apk"

# --- 11. Sign with throwaway debug key ---
if [ ! -f "$WORK_DIR/debug.keystore" ]; then
  keytool -genkeypair -keystore "$WORK_DIR/debug.keystore" -alias androiddebugkey \
    -keyalg RSA -keysize 2048 -validity 365 \
    -storepass android -keypass android \
    -dname "CN=Android Debug,O=Android,C=US"
fi
"$BUILD_TOOLS/apksigner" sign --ks "$WORK_DIR/debug.keystore" \
  --ks-pass pass:android --key-pass pass:android \
  --out "$APK_OUT" "$WORK_DIR/aligned.apk"

# --- 12. Verify ---
echo "=== Verifying ==="
"$BUILD_TOOLS/apksigner" verify --print-certs "$APK_OUT"
BADGING="$("$BUILD_TOOLS/aapt2" dump badging "$APK_OUT")"
echo "$BADGING" | head -8
echo "$BADGING" | grep -q "package: name='dev.dioxus.main'" \
  || fail "badging: wrong package name"
echo "$BADGING" | grep -q "launchable-activity: name='dev.dioxus.main.MainActivity'" \
  || fail "badging: MainActivity not launchable"

echo ""
echo "=== DONE: $APK_OUT ==="
ls -lh "$APK_OUT"
