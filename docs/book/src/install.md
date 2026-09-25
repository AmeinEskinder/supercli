# Installation

## From source

Unpeel is a Rust workspace. You need a Rust toolchain (1.75+) and, for the
desktop/mobile launchers, the GTK/WebKit system libraries (see
`.github/workflows/linux.yml` for the authoritative dependency list).

```sh
git clone <repo>
cd unpeel
cargo build --release -p unpeel-cli -p unpeel-host
```

The binaries land in `target/release/`:

- `unpeel` — the CLI
- `unpeel-host` — the Host service (started by `unpeel serve`)

## Native launchers

The Dioxus desktop and mobile launchers build with the Nix-provided
GTK/WebKit environment:

```sh
source scripts/env-nix-gtk.sh
cargo build --release --manifest-path clients/dioxus/Cargo.toml \
  --locked -p unpeel-desktop -p unpeel-mobile
```

## Verify

```sh
unpeel --version
unpeel doctor
```

`doctor` checks home-directory permissions, review-chain integrity, stale
leases, and clock skew. See [Doctor and troubleshooting](doctor.md).
