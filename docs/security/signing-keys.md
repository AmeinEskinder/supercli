# Signing Keys — License and Updater

This document describes where the two Ed25519 **public** keys live, how the
release build injects them, and the fail-closed behavior when they are absent.

**Amein generates the Ed25519 keypairs himself, offline. NEVER generate,
request, or commit a private key.** Only the public keys are embedded in the
build. The private keys never leave Amein's offline machine.

## The two public keys

### 1. License public key

**Location:** `crates/supercli-native-bridge/src/macos/license.rs`

```rust
pub const BUNDLED_PUBLIC_KEY_BASE64: &str = "E32qYUoJsxH5TLSRt/xrjQcWxwVwawVAfLJjM+HbpZI=";
```

- **Version:** License key v1 (provided by Amein, offline-generated).
- **Fingerprint (sha256 of raw 32 bytes):** `bff14084e409b8f0…`
- **Format:** Standard base64 (not URL-safe), 32 bytes → 44 chars with `=` padding.
- **Used by:** `verify_signature()` — offline Ed25519 verification of
  `SCLI-<payloadB64url>.<signatureB64url>` license keys.
- **Pinning:** The `bundled_public_key_is_pinned_license_v1` test verifies the
  key decodes to 32 bytes, is a valid Ed25519 point, and the sha256 starts
  with `bff14084e409b8f0`. Any accidental change fails CI.
- **Fail-closed:** If this were empty, `verify_signature()` returns `false`
  for every key and activation is refused. The key is now set, so this path
  is only relevant if someone clears it.

**CRITICAL:** This is the LICENSE key ONLY. It must NOT be reused for the
updater. The updater has its own separate key slot (see below).

### 2. Updater public key

**Location:** `crates/supercli-native-bridge/src/macos/updater.rs`

The updater verifies download signatures via `verify_download(bytes,
signature_base64, public_key_base64)`. The bundled updater public key slot
is **EMPTY** and fails closed:

- When the bundled key is empty/unset, `verify_download()` returns `false`
  for every download. Updates are refused with a clear error.
- Amein will provide a SEPARATE updater key (not the license key). The
  license key (`E32qYUoJsxH5TLSRt/xrjQcWxwVwawVAfLJjM+HbpZI=`) must NOT be
  reused for the updater.
- Until Amein sends the updater key, the slot stays empty and the updater
  fails closed. This is intentional.

**Format:** Same as the license key — standard base64, 32-byte Ed25519
public key (when provided).

## Key generation (Amein only, offline)

Amein generates both keypairs on an offline machine:

```bash
# Example (illustrative — Amein uses his own offline process):
# Generate Ed25519 keypair, output public key as base64.
```

The private keys are never committed, never transmitted, never stored in CI.
Only the two public keys are embedded at release-build time.

## Fail-closed summary

| Key | Status | Behavior |
|-----|--------|----------|
| License (v1) | **Set** (`E32qYUoJsxH5TLSRt/xrjQcWxwVwawVAfLJjM+HbpZI=`) | Verifies against bundled key; pinning test fails CI on change |
| Updater | **Empty** (TODO) | `verify_download()` returns `false`; updates refused |

If the license key were cleared, activation would be refused (fail-closed).
The updater stays fail-closed until Amein provides its separate key.
