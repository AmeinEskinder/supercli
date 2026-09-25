# Installation

## From source

Supercli is a Rust workspace. You need a Rust toolchain (1.75+) and, for the
desktop/mobile launchers, the GTK/WebKit system libraries (see
`.github/workflows/linux.yml` for the authoritative dependency list).

```sh
git clone <repo>
cd supercli
cargo build --release -p supercli-cli -p supercli-host
```

The binaries land in `target/release/`:

- `supercli` — the CLI
- `supercli-host` — the Host service (started by `supercli serve`)

## Native launchers

The Dioxus desktop and mobile launchers build with the Nix-provided
GTK/WebKit environment:

```sh
source scripts/env-nix-gtk.sh
cargo build --release --manifest-path clients/dioxus/Cargo.toml \
  --locked -p supercli-desktop -p supercli-mobile
```

## Verify

```sh
supercli --version
supercli doctor
```

`doctor` checks home-directory permissions, review-chain integrity, stale
leases, and clock skew. See [Doctor and troubleshooting](doctor.md).
