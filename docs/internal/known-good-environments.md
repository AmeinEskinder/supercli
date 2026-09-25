# Known-good environments (this VM, verified 2026-09-23)

Check this list before declaring anything "environmentally blocked". All
paths below were verified working on 2026-09-23.

## Rust / native launcher builds

Source `scripts/env-nix-gtk.sh` first. It exports:

- `PATH="$HOME/.cargo/bin:$PATH"`
- `PKG_CONFIG_PATH` — 87 entries: Nix pkgconfig dirs plus
  `~/w1/stubs/lib/pkgconfig`
- `LD_LIBRARY_PATH` — exact Nix library paths:
  - glib 2.84.3
  - WebKitGTK 2.50.4 (+abi=4.1)
  - libsoup 3.6.5
  - xdotool 3.20211022.1 (provides `libxdo.so` for link-time discovery)
- `LIBRARY_PATH="$LD_LIBRARY_PATH"` (link-time discovery needs this, not
  just `LD_LIBRARY_PATH`)

With that env: `cargo check --locked -p unpeel-desktop -p unpeel-mobile`
and `cargo build --locked -p unpeel-desktop -p unpeel-mobile` both succeed
from `clients/dioxus/`.

Do NOT trust apt here: non-root `apt-get update` fails with
`407 Proxy Authentication Required`; with sudo the mirrors are flaky.
The authoritative GTK/WebKit dep list is
`unpeel/.github/workflows/linux.yml`.

> **2026-09-23 ~01:52 — /nix is GONE.** The Nix store was a tmpfs mount
> (Phase 4 W1 single-user workaround; see BUILDLOG ~line 3079) and did not
> survive the VM reboot at ~00:54. `scripts/env-nix-gtk.sh` currently
> exports dead `/nix/store` paths, so native launcher check/build is
> blocked again until /nix is re-provisioned from the Nix binary cache
> (~849 store entries). If `env-nix-gtk.sh` reports
> `PKG_CONFIG_PATH entries: 1`, /nix is absent — do not trust any
> GTK/WebKit build under it.

## Web / Playwright

- Chrome for Testing 153.0.8010.12 (chrome-headless-shell):
  `~/workspace/muse-harness/tmp/downloads/cft2/chrome-headless-shell-linux64/chrome-headless-shell`
- Required export for the suite:
  `PLAYWRIGHT_CHROMIUM_PATH="$HOME/workspace/muse-harness/tmp/downloads/cft2/chrome-headless-shell-linux64/chrome-headless-shell"`
- The checked-in `playwright.config.js` honors `PLAYWRIGHT_CHROMIUM_PATH`.
  `/opt/meta-chromium` is blocked on localhost — do not use it.
- Suite: `clients/dioxus/unpeel-web/tests/web/` (`npm test` → `playwright test`);
  serves the `dx build --platform web -p unpeel-web` bundle via `server.js`.
  13/13 pass (composer 3 + demo 5 + mobai-mirror 5).

## Android

- SDK: `~/workspace/muse-harness/tmp/android-sdk`
- NDK: `~/workspace/muse-harness/tmp/android-sdk/ndk/27.2.12479018`
- build-tools 35.0.0, platform android-35
- No KVM on this VM, so no emulator. `dx bundle --platform android`
  cross-compiles the Rust workspace (valid `libmain.so`); APK assembly via
  Gradle is broken in this sandbox (daemon IPC).

## MobAI

- mobai-ci 0.6.0: `/home/hatch/.local/opt/mobai/mobai-ci`
- Invoke by absolute path — it is NOT on `PATH`.
- `mobai-ci validate ./tests/device` → 5/5 OK (syntax only; never run on a
  device — no devices paired, no bridge).

## Lint

- actionlint: `~/.local/bin/actionlint` (not on `PATH`; invoke by absolute
  path). `.github/actionlint.yaml` declares the real `macos-26` runner
  label because actionlint's built-in DB is stale.

## Standing limits (not fixable from here)

- No real-device testing (iOS/Android).
- iOS push OS-token acquisition needs the native iOS shell / Mac with Xcode.
- No accounts or credentials used anywhere.
