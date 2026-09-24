# Dioxus cross-platform clients

The Rust replacement for the Swift Apple clients (`clients/native`,
`clients/ios`, `clients/shared`). One component tree (`unpeel-ui`), two
thin launchers (`unpeel-desktop`, `unpeel-mobile`), all Host I/O through
the `unpeel-client` crate.

Separate Cargo workspace from `crates/` on purpose — the Dioxus
dependency tree must never slow the Host's gates (same reason
`crates/apps` is separate).

## Layout

| Crate | What |
|---|---|
| `unpeel-ui` | Shared Dioxus components: `SessionList`, `ChatView`, `ApprovalCard`, `Composer`, `ConnectionBar`. Pure renderers over `unpeel_client` DTOs; no I/O. |
| `unpeel-desktop` | Desktop launcher. Codex-like chat GUI: sidebar, chat view, inline approval cards, composer. |
| `unpeel-mobile` | Mobile launcher (iOS/Android). Same components, master/detail navigation. |

## Renderer status (2026-09-19, verified)

- **Dioxus 0.7.10** (stable) is the scaffold target.
- **Desktop/mobile render through the OS webview today** (Wry/Tao — the
  same crates Tauri is built on). This is a real, bounded downgrade from
  libghostty/Metal for the terminal pane.
- **Blitz is at 0.3.0-beta.2** — no longer pre-alpha, but still beta. The
  genuinely native GPU renderer path is gated behind the `native-ui`
  feature on `unpeel-mobile` and a re-evaluation before any commitment.
  Do not promise "native, not webview" until Blitz is stable.

## Run

```sh
# from this directory
UNPEEL_HOST_URL=http://<host>:<port>/mobile \
UNPEEL_DEVICE_TOKEN=<device-token> \
cargo run -p unpeel-desktop
```

Desktop builds need the usual webview system libraries (e.g.
`webkit2gtk` on Linux). Mobile packaging (Xcode/Gradle, signing, push
shims) is not yet scaffolded — the mobile binary is the portable core
those launchers will embed.

## What the Swift clients still own (not yet ported)

- Pairing flow UI (`RemotePairingClient`) and QR/invitation handling
- Relay E2E **transport** (the crypto itself is ported: `unpeel-client`
  passes the `relay-kat-vectors-v1.json` known-answer tests byte-for-byte)
- Push-notification registration and token upload
- Terminal emulator pane (xterm.js-in-webview is the planned interim;
  custom wgpu terminal gated on Blitz)
- Keychain storage (the `keyring` crate is the selected replacement)

The Swift apps keep working throughout — there is no migration cliff.
New development goes here; the Swift clients are in maintenance mode.
