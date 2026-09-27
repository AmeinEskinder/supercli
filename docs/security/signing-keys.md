# Signing Keys — License and Updater

This document describes the two Ed25519 **public** keys: where they live, how
the release build injects them, and the fail-closed behavior when they are
absent.

**Amein generates the Ed25519 keypairs himself, offline. NEVER generate,
request, or commit a private key.** Only the public keys are embedded in the
build. The private keys never leave Amein's offline machine.

## The two public keys

These are **DIFFERENT keys for different purposes**. The updater must NEVER
accept the license key, and license verification must NEVER accept the updater
key. Cross-use is tested and rejected in CI.

### 1. License public key (v1)

**Location:** `crates/supercli-core/src/license.rs`

```rust
pub const LICENSE_PUBLIC_KEY_BASE64: &str = "E32qYUoJsxH5TLSRt/xrjQcWxwVwawVAfLJjM+HbpZI=";
```

- **Version:** License key v1 (provided by Amein, offline-generated).
- **Fingerprint (sha256 of raw 32 bytes):** `bff14084e409b8f0…`
- **Format:** Standard base64 (not URL-safe), 32 bytes → 44 chars with `=` padding.
- **Purpose:** License key verification only — offline Ed25519 verification of
  `SCLI-<payloadB64url>.<signatureB64url>` license keys.
- **Status:** Active.
- **Pinning:** The `bundled_public_key_is_pinned_license_v1` test verifies the
  key decodes to 32 bytes, is a valid Ed25519 point, and the sha256 starts
  with `bff14084e409b8f0`. Any accidental change fails CI.
- **Dev-only override:** `SUPERCLI_LICENSE_PUBLIC_KEY` is honored in dev builds
  only (`cfg(debug_assertions)`), for tests. Release builds ignore it.
- **Fail-closed:** If the key were empty, verification fails for every key and
  activation is refused. An empty bundled key errors clearly at startup.

**CRITICAL:** This is the LICENSE key ONLY. It must NOT be reused for the
updater. The updater has its own separate key (see below).

### 2. Updater public key (v1)

**Location:** `crates/supercli-native-bridge/src/macos/updater.rs` (and the
Host's updater module)

```rust
pub const UPDATER_PUBLIC_KEY_BASE64: &str = "VQdQWMuzQg627U+wNV4YL9gX4pLQhI0XZaNKffEkRaM=";
```

- **Version:** Updater key v1 (provided by Amein, offline-generated; DIFFERENT
  from the license key).
- **Fingerprint (sha256 of raw 32 bytes):** `4f71bbbd36495eae…`
- **Format:** Standard base64 (not URL-safe), 32 bytes → 44 chars with `=`
  padding.
- **Purpose:** Update package verification only — verifies the Ed25519
  signature over downloaded update artifacts.
- **Status:** Active.
- **Pinning:** The `bundled_public_key_is_pinned_updater_v1` test verifies the
  key decodes to 32 bytes, is a valid Ed25519 point, and the sha256 starts
  with `4f71bbbd36495eae`. Any accidental change fails CI.
- **Cross-rejection:** A test asserts that a payload signed for the license
  key is REJECTED by the updater, and a payload signed for the updater key is
  REJECTED by license verification.
- **Fail-closed:** If the key were empty/unset, `verify_download()` returns
  `false` for every download. Updates are refused with a clear error
  ("updates disabled: no updater key configured").

## Key generation (Amein only, offline)

Amein generates both keypairs on an offline machine:

```bash
# Example (illustrative — Amein uses his own offline process):
# Generate Ed25519 keypair, output public key as base64.
```

The private keys are never committed, never transmitted, never stored in CI.
Only the two public keys are embedded at release-build time.

## Revoked keys

| Key | Public key | Status | Notes |
|-----|-----------|--------|-------|
| unpeel legacy license key | `6RfwwHUhth8Ji7T7p/QbDOQjeN9Zrk1S34Hk85cpg54=` | **REVOKED** | The ORIGINAL unpeel product's license public key. Whoever holds unpeel's private key can mint keys that verify under it, so it must NEVER be trusted for supercli licenses. Removed from all code and fixtures; if found anywhere, delete it. |

## Fail-closed summary

| Key | Status | Behavior |
|-----|--------|----------|
| License (v1) | **Set** (`E32qYUoJsxH5TLSRt/xrjQcWxwVwawVAfLJjM+HbpZI=`) | Verifies against bundled key; pinning test fails CI on change |
| Updater (v1) | **Set** (`VQdQWMuzQg627U+wNV4YL9gX4pLQhI0XZaNKffEkRaM=`) | Verifies downloads against bundled key; pinning test fails CI on change |
| unpeel legacy | **Revoked** | Must not appear anywhere; grep in CI |

If either bundled key were cleared, that verification path refuses everything
(fail-closed). Cross-use of license/updater keys is rejected by test.
