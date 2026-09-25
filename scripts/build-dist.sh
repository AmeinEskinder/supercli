#!/usr/bin/env bash
# Phase 14 (2): One-script distribution packaging.
#
# Builds the release binaries with the fat-LTO `dist` profile, packages
# them as .tar.gz and .deb, writes SHA-256 checksums, and generates a
# CycloneDX SBOM (via cargo-cyclonedx if installed).
#
# Usage: scripts/build-dist.sh [--out DIR]
# Output: <out>/unpeel-<version>-<target>.tar.gz
#         <out>/unpeel_<version>_<arch>.deb
#         <out>/SHA256SUMS
#         <out>/sbom.cdx.json (if cargo-cyclonedx is available)
#
# Local only. No network, no upload, no credentials.

set -euo pipefail

export PATH="$HOME/.cargo/bin:/usr/local/cargo/bin:$PATH"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/dist}"
if [[ "${1:-}" == "--out" ]]; then
    OUT_DIR="${2:-$REPO_ROOT/dist}"
fi

VERSION="$(grep '^version' "$REPO_ROOT/crates/Cargo.toml" | head -1 | sed 's/.*"\([^"]*\)".*/\1/')"
TARGET="$(rustc -vV | grep host | cut -d' ' -f2)"
ARCH="$(dpkg --print-architecture 2>/dev/null || echo amd64)"

echo "==> Building unpeel v$VERSION ($TARGET) with dist profile (fat LTO)..."
cd "$REPO_ROOT/crates"
cargo build --profile dist --bin unpeel --bin unpeel-host
cargo build --profile dist --manifest-path "$REPO_ROOT/crates/unpeel-attach/Cargo.toml"

BIN_DIR="$REPO_ROOT/crates/target/dist"
ATTACH_BIN="$REPO_ROOT/crates/unpeel-attach/target/dist/unpeel-attach"
mkdir -p "$OUT_DIR"

# --- tar.gz ---
TARBALL="$OUT_DIR/unpeel-${VERSION}-${TARGET}.tar.gz"
echo "==> Creating $TARBALL..."
STAGE="$(mktemp -d)"
mkdir -p "$STAGE/unpeel-$VERSION/bin"
cp "$BIN_DIR/unpeel" "$BIN_DIR/unpeel-host" "$ATTACH_BIN" "$STAGE/unpeel-$VERSION/bin/"
cp "$REPO_ROOT/README.md" "$STAGE/unpeel-$VERSION/" 2>/dev/null || true
cp "$REPO_ROOT/CHANGELOG.md" "$STAGE/unpeel-$VERSION/" 2>/dev/null || true
tar -czf "$TARBALL" -C "$STAGE" "unpeel-$VERSION"
rm -rf "$STAGE"
echo "    $(du -h "$TARBALL" | cut -f1)"

# --- .deb ---
DEB_DIR="$(mktemp -d)"
DEB_PKG="$OUT_DIR/unpeel_${VERSION}_${ARCH}.deb"
echo "==> Creating $DEB_PKG..."
mkdir -p "$DEB_DIR/DEBIAN" "$DEB_DIR/usr/bin" "$DEB_DIR/usr/share/doc/unpeel"
chmod 755 "$DEB_DIR" "$DEB_DIR/DEBIAN"
cp "$BIN_DIR/unpeel" "$BIN_DIR/unpeel-host" "$ATTACH_BIN" "$DEB_DIR/usr/bin/"
cp "$REPO_ROOT/CHANGELOG.md" "$DEB_DIR/usr/share/doc/unpeel/" 2>/dev/null || true
cat > "$DEB_DIR/DEBIAN/control" <<EOF
Package: unpeel
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: Unpeel <noreply@example.com>
Description: AI-native terminal workspace for running CLI agents
 The Unpeel CLI, session host, and attach client.
EOF
dpkg-deb --build "$DEB_DIR" "$DEB_PKG" >/dev/null
rm -rf "$DEB_DIR"
echo "    $(du -h "$DEB_PKG" | cut -f1)"

# --- SHA-256 ---
echo "==> Writing SHA256SUMS..."
cd "$OUT_DIR"
sha256sum "unpeel-${VERSION}-${TARGET}.tar.gz" "unpeel_${VERSION}_${ARCH}.deb" > SHA256SUMS
cat SHA256SUMS

# --- SBOM ---
if command -v cargo-cyclonedx >/dev/null 2>&1; then
    echo "==> Generating CycloneDX SBOM..."
    cd "$REPO_ROOT/crates"
    cargo cyclonedx --format json --output "$OUT_DIR/sbom.cdx.json" 2>/dev/null || \
        echo "    WARNING: cargo-cyclonedx failed; SBOM not generated"
else
    echo "    SKIP: cargo-cyclonedx not installed; SBOM not generated (UNTESTED)"
fi

echo "==> Done. Artifacts in $OUT_DIR/"
ls -la "$OUT_DIR/"
