# Supply-chain security

Date: 2026-09-25. Tools: cargo-deny 0.20.2, cargo-audit (RustSec DB).

## Configuration

- `crates/deny.toml` — licenses, bans, sources, advisories.
- `.github/workflows/supply-chain.yml` — CI runs on dependency changes, PRs, and weekly.

## Exceptions (all with reasons in `deny.toml`)

### Vulnerabilities fixed
- **RUSTSEC-2026-0285** (rustls 0.23.41, TLS 1.3 handshake): fixed by
  `cargo update -p rustls` → 0.23.45.

### Advisory ignored
- **RUSTSEC-2017-0008** (`serial` 0.4.0 unmaintained): transitive dep of
  portable-pty, used only for serial-port enumeration which Unpeel never
  calls. No safe upgrade exists. Risk accepted as dead code.

### Licenses allowed (beyond standard permissive set)
- **MPL-2.0** (`option-ext` 0.2.0 via `dirs`): file-level copyleft, not viral;
  we do not modify the crate.
- **CDLA-Permissive-2.0** (`webpki-roots`): permissive data license for the
  Mozilla CA bundle; no code copyleft.

### Duplicate versions (18 crates)
All are different-major splits in the ecosystem (syn 1/2, thiserror 1/2,
digest 0.10/0.11, etc.) that cannot be unified without breaking third-party
upgrades. Each has a `[[bans.skip]]` entry with a reason in `deny.toml`.

### Wildcards
Intra-workspace path dependencies now carry explicit `version = "0.8.0"`
requirements (16 deps across 5 crates).

## Verification
- `cargo deny --manifest-path crates/Cargo.toml --config crates/deny.toml check`:
  **advisories ok, bans ok, licenses ok, sources ok** (2026-09-25).
- `cargo audit --file crates/Cargo.lock`: **exit 0** (2026-09-25).
  1 warning: `serial` 0.4.0 unmaintained (RUSTSEC-2017-0008, documented above).
