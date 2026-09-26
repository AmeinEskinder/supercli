#!/usr/bin/env bash
# Sourceable Nix GTK/WebKit environment for native Dioxus launcher builds.
#
# Usage:
#   source scripts/env-nix-gtk.sh
#   cd clients/dioxus && cargo check --locked -p supercli-desktop -p supercli-mobile
#
# What this does: the VM's apt is unusable (proxy 407), so GTK/WebKit system
# libraries come from the Phase 4 W1 Nix store paths instead. PKG_CONFIG_PATH
# is assembled from every pkgconfig dir under /nix/store plus the W1 stub
# dir; LD_LIBRARY_PATH pins the curated glib/webkitgtk/libsoup/xdotool libs
# the launchers link against.
#
# Verified 2026-09-23 (Phase 7 C1): cargo check AND cargo build for
# supercli-desktop and supercli-mobile succeed under this environment.
#
# NOTE: /nix is a tmpfs and does not survive VM reboots. If the store is
# missing, sourcing this file aborts with the re-provision command
# (bash ~/w1/reprovision-nix-gtk.sh) instead of exporting dead paths.

# Rust toolchain (dx, cargo live here).
export PATH="$HOME/.cargo/bin:$PATH"

# The Nix store is a tmpfs that does NOT survive VM reboots. If it is
# missing, refuse to export dead paths: print the re-provision command
# and stop. (A silent export previously yielded a single PKG_CONFIG_PATH
# entry and cryptic gdk-sys build failures.)
NIX_STORE_ENTRIES="$(ls /nix/store 2>/dev/null | wc -l)"
if [ ! -d /nix/store ] || [ "$NIX_STORE_ENTRIES" -lt 100 ]; then
  echo "env-nix-gtk: ERROR — /nix/store is missing or nearly empty ($NIX_STORE_ENTRIES entries)." >&2
  echo "env-nix-gtk: The tmpfs Nix store did not survive a reboot." >&2
  echo "env-nix-gtk: Re-provision with:  bash ~/w1/reprovision-nix-gtk.sh" >&2
  return 1 2>/dev/null || exit 1
fi

# Every pkgconfig dir in the Nix store, plus the W1 stub overlay.
export PKG_CONFIG_PATH="$(find /nix/store -maxdepth 4 -type d -name pkgconfig 2>/dev/null | tr '\n' ':')$HOME/w1/stubs/lib/pkgconfig"

# Curated runtime libs for the native launchers.
export LD_LIBRARY_PATH="/nix/store/sgap1pr1czm1k2pp8sdkp9hs9v3ahx27-glib-2.84.3/lib:/nix/store/nisriichhfvakp3v23kim3d3h6xx62ca-webkitgtk-2.50.4+abi=4.1/lib:/nix/store/lyifb392qssvw1qmar5jbx3rjwid0hw2-libsoup-3.6.5/lib:/nix/store/3zsdz4bs3c1j9azwv6vz2dsh7svgznha-xdotool-3.20211022.1/lib"
# Same dirs for the link step: rust-lld needs them at link time
# (LD_LIBRARY_PATH is runtime-only). Fixes "unable to find library -lxdo".
export LIBRARY_PATH="$LD_LIBRARY_PATH"

echo "env-nix-gtk: PKG_CONFIG_PATH entries: $(echo "$PKG_CONFIG_PATH" | tr ':' '\n' | grep -c .)"
echo "env-nix-gtk: LD_LIBRARY_PATH=$LD_LIBRARY_PATH"
