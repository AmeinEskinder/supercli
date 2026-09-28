//! Port of `Licensing/LicenseManager.swift` — license verification.
//!
//! Offline Ed25519 verification of `SCLI-<payload>.<signature>` keys,
//! key normalization (smart dashes, whitespace), activation/validation
//! against `https://superc.li`, revocation persistence, and the 7-day
//! validation / 30-day offline-grace policy.
//!
//! The key parsing and signature verification are the single canonical
//! implementation in `supercli_core::license`; this module delegates to it
//! and keeps only the macOS-side configuration/policy surface (bundled key
//! constant, env plumbing, validation cadence, device id).

use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Canonical key parsing/verification, normalization, prefix constants and
/// the shared `LicensePayload` type all live in `supercli-core`; this
/// module re-exports them so there is exactly one implementation.
pub use supercli_core::license::{
    normalize_key as normalize_license_key, LicenseKeyError, LicensePayload, KEY_PREFIX,
    LEGACY_KEY_PREFIX,
};

/// Production Link API base. Swift: `https://superc.li`.
pub const PRODUCTION_API_BASE_URL: &str = "https://superc.li";
/// Bundled production Ed25519 public key (base64). This is the license key
/// v1, provided by Amein (offline-generated). Fingerprint (sha256):
/// `bff14084e409b8f0…`. Any change to this value must be intentional and
/// reviewed — the pinning test below fails CI on accidental modification.
pub const BUNDLED_PUBLIC_KEY_BASE64: &str = "E32qYUoJsxH5TLSRt/xrjQcWxwVwawVAfLJjM+HbpZI=";
/// Env override for the public key (dev).
pub const PUBLIC_KEY_ENV_VAR: &str = "SUPERCLI_LICENSE_PUBLIC_KEY";
/// Env override for the API base URL (dev).
pub const API_BASE_URL_ENV_VAR: &str = "SUPERCLI_LICENSE_API_BASE_URL";
/// Info.plist marker enabling the dev license bypass.
pub const DEVELOPMENT_BUILD_INFO_PLIST_KEY: &str = "SupercliDevelopmentBuild";

/// License key prefix and legacy prefix are defined once in
/// `supercli_core::license` and re-exported above (`KEY_PREFIX`,
/// `LEGACY_KEY_PREFIX`).
/// Validation cadence: re-validate at most every 7 days.
pub const VALIDATION_INTERVAL_SECS: u64 = 7 * 24 * 60 * 60;
/// Offline grace: a license stays valid 30 days without re-validation.
pub const OFFLINE_GRACE_SECS: u64 = 30 * 24 * 60 * 60;

/// License configuration: bundled defaults with environment overrides.
/// The public-key env override is honored in dev builds only; release
/// builds always use the bundled key (the dev-only policy is implemented
/// once in `supercli_core::license`).
pub struct LicenseConfig;

impl LicenseConfig {
    /// Public key base64: dev-only env override wins when permitted, else
    /// the bundled key.
    pub fn public_key_base64(environment: &HashMap<String, String>) -> &str {
        supercli_core::license::resolve_public_key_b64_with(
            environment.get(PUBLIC_KEY_ENV_VAR).map(|s| s.as_str()),
            cfg!(debug_assertions),
            BUNDLED_PUBLIC_KEY_BASE64,
        )
    }

    /// Testable core: `allow_env_override` simulates dev (`true`) vs
    /// release (`false`) builds. Thin shim over the single core
    /// implementation; the override policy itself lives in
    /// `supercli_core::license`.
    pub fn public_key_base64_with_dev_override(
        environment: &HashMap<String, String>,
        allow_env_override: bool,
    ) -> &str {
        supercli_core::license::resolve_public_key_b64_with(
            environment.get(PUBLIC_KEY_ENV_VAR).map(|s| s.as_str()),
            allow_env_override,
            BUNDLED_PUBLIC_KEY_BASE64,
        )
    }

    /// API base URL: in dev builds the env override wins when it parses,
    /// else production. Release builds ALWAYS use the production URL —
    /// the activation endpoint is a trust decision, so it cannot be
    /// redirected by an environment variable in release builds.
    pub fn api_base_url(environment: &HashMap<String, String>) -> &str {
        Self::api_base_url_with_dev_override(environment, cfg!(debug_assertions))
    }

    /// Testable core: `allow_env_override` simulates dev (`true`) vs
    /// release (`false`) builds.
    pub fn api_base_url_with_dev_override(
        environment: &HashMap<String, String>,
        allow_env_override: bool,
    ) -> &str {
        if allow_env_override {
            match environment.get(API_BASE_URL_ENV_VAR) {
                Some(url) if is_valid_url(url) => return url.as_str(),
                _ => {}
            }
        }
        PRODUCTION_API_BASE_URL
    }

    /// Returns the bundled public key, or an error if none is configured.
    ///
    /// Activation and update verification MUST call this and refuse on
    /// error. Fails closed: an empty/missing bundled key is never silently
    /// accepted.
    pub fn bundled_public_key() -> Result<&'static str, String> {
        Self::check_bundled_key(BUNDLED_PUBLIC_KEY_BASE64)
    }

    /// Testable fail-closed core: an empty key slot is an error, never a
    /// silent accept. `bundled_public_key()` above is this with the real
    /// bundled constant.
    fn check_bundled_key(key_b64: &'static str) -> Result<&'static str, String> {
        if key_b64.trim().is_empty() {
            Err("No bundled license public key configured — refusing.".to_string())
        } else {
            Ok(key_b64)
        }
    }

    /// Dev builds bypass license checks via the Info.plist marker.
    pub fn development_build_license_bypass_enabled(
        info_dictionary: &HashMap<String, String>,
    ) -> bool {
        matches!(
            info_dictionary
                .get(DEVELOPMENT_BUILD_INFO_PLIST_KEY)
                .map(|s| s.to_lowercase())
                .as_deref(),
            Some("true") | Some("yes") | Some("1")
        )
    }
}

fn is_valid_url(s: &str) -> bool {
    // Minimal check: scheme + host. The real client uses URL(string:).
    let s = s.trim();
    (s.starts_with("http://") || s.starts_with("https://")) && {
        let after_scheme = s.split_once("://").map(|x| x.1).unwrap_or("");
        !after_scheme.is_empty() && !after_scheme.contains(' ')
    }
}

/// Fully validates a license key via the single canonical implementation in
/// `supercli_core::license` (normalize → split → JSON-decode payload →
/// Ed25519-verify). Returns the payload on success.
///
/// Legacy `CLRTY-` keys (old vendor key format) are rejected with
/// a clear message; supercli has no legacy customers to migrate.
pub fn validate_key(raw: &str, public_key_base64: &str) -> Result<LicensePayload, String> {
    supercli_core::license::validate_key_with(raw, public_key_base64)
        .map_err(|error| error.to_string())
}

/// Device ID: SHA-256 of the hardware UUID, hex-encoded.
pub fn device_id(hardware_uuid: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(hardware_uuid.as_bytes());
    hex::encode(hasher.finalize())
}

/// True when the cached validation is fresh enough to skip re-validation.
pub fn validation_is_fresh(last_validated_unix_secs: u64, now_unix_secs: u64) -> bool {
    now_unix_secs.saturating_sub(last_validated_unix_secs) < VALIDATION_INTERVAL_SECS
}

/// True when the license is still within the offline grace period.
pub fn within_offline_grace(last_validated_unix_secs: u64, now_unix_secs: u64) -> bool {
    now_unix_secs.saturating_sub(last_validated_unix_secs) < OFFLINE_GRACE_SECS
}

// Minimal hex encoding to avoid another dependency.
mod hex {
    const CHARS: &[u8; 16] = b"0123456789abcdef";
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        let bytes = bytes.as_ref();
        let mut out = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            out.push(CHARS[(b >> 4) as usize] as char);
            out.push(CHARS[(b & 0xf) as usize] as char);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use ed25519_dalek::VerifyingKey;

    fn payload() -> LicensePayload {
        LicensePayload {
            v: 1,
            id: "lic_test".to_string(),
            email: "test@example.com".to_string(),
            plan: "personal".to_string(),
            seats: 1,
            iat: 1_700_000_000,
        }
    }

    #[test]
    fn license_config_uses_bundled_production_defaults() {
        assert_eq!(
            LicenseConfig::public_key_base64(&HashMap::new()),
            BUNDLED_PUBLIC_KEY_BASE64
        );
        assert_eq!(
            LicenseConfig::api_base_url(&HashMap::new()),
            "https://superc.li"
        );
    }

    #[test]
    fn normalize_license_key_repairs_smart_dashes_and_whitespace() {
        let clean = "SCLI-eyJhIjoxfQ.c2ln-Zm9v_YmFy";
        // macOS smart-dash substitution turns every hyphen into an en-dash;
        // paste can also inject wrapping newlines / stray spaces.
        let mangled = "SCLI\u{2013}eyJhIjoxfQ.c2ln\u{2013}Zm9v_YmFy";
        assert_eq!(normalize_license_key(mangled), clean);

        let with_whitespace = "  SCLI-eyJhIjoxfQ.\nc2ln-Zm9v_YmFy  ";
        assert_eq!(normalize_license_key(with_whitespace), clean);

        // Em-dash, NBSP, fullwidth hyphen, zero-width space.
        let exotic = "SCLI\u{2014}eyJhIjoxfQ.\u{00A0}c2ln\u{FF0D}Zm9v_YmF\u{200B}y";
        assert_eq!(normalize_license_key(exotic), clean);

        // A well-formed key is unchanged (idempotent).
        assert_eq!(normalize_license_key(clean), clean);
    }

    #[test]
    fn license_config_uses_dev_environment_overrides() {
        let mut environment = HashMap::new();
        environment.insert(PUBLIC_KEY_ENV_VAR.to_string(), "dev-public-key".to_string());
        environment.insert(
            API_BASE_URL_ENV_VAR.to_string(),
            "http://localhost:5173".to_string(),
        );

        // Dev builds (debug_assertions on in tests): overrides honored.
        assert_eq!(
            LicenseConfig::public_key_base64_with_dev_override(&environment, true),
            "dev-public-key"
        );
        assert_eq!(
            LicenseConfig::api_base_url_with_dev_override(&environment, true),
            "http://localhost:5173"
        );
        // The convenience wrappers agree in this dev/test build.
        assert_eq!(
            LicenseConfig::public_key_base64(&environment),
            "dev-public-key"
        );
        assert_eq!(
            LicenseConfig::api_base_url(&environment),
            "http://localhost:5173"
        );
    }

    #[test]
    fn release_builds_ignore_env_key_override() {
        let mut environment = HashMap::new();
        environment.insert(
            PUBLIC_KEY_ENV_VAR.to_string(),
            "attacker-controlled-key".to_string(),
        );
        // Simulate release: the env var MUST be ignored.
        assert_eq!(
            LicenseConfig::public_key_base64_with_dev_override(&environment, false),
            BUNDLED_PUBLIC_KEY_BASE64
        );
        assert_ne!(
            LicenseConfig::public_key_base64_with_dev_override(&environment, false),
            "attacker-controlled-key"
        );
    }

    #[test]
    fn release_builds_ignore_env_api_url_override() {
        let mut environment = HashMap::new();
        environment.insert(
            API_BASE_URL_ENV_VAR.to_string(),
            "https://evil.example.com".to_string(),
        );
        // Simulate release: activation endpoint cannot be redirected.
        assert_eq!(
            LicenseConfig::api_base_url_with_dev_override(&environment, false),
            PRODUCTION_API_BASE_URL
        );
    }

    #[test]
    fn bundled_public_key_fails_closed_when_empty() {
        // The fail-closed contract: an empty key slot refuses, never
        // silently accepts. The bundled key is now set (Amein's key v1,
        // pinned by bundled_public_key_is_pinned_license_v1), so the
        // empty-slot path is exercised through the testable helper.
        let err =
            LicenseConfig::check_bundled_key("").expect_err("empty bundled key must fail closed");
        assert!(
            err.contains("No bundled license public key configured"),
            "unexpected error: {err}"
        );
        assert!(err.contains("refusing"), "unexpected error: {err}");
        // The real bundled key is populated and accepted.
        assert!(LicenseConfig::bundled_public_key().is_ok());
    }

    #[test]
    fn license_config_ignores_invalid_api_override() {
        let mut environment = HashMap::new();
        environment.insert(API_BASE_URL_ENV_VAR.to_string(), "not a url".to_string());
        assert_eq!(
            LicenseConfig::api_base_url(&environment),
            "https://superc.li"
        );
    }

    #[test]
    fn development_build_license_bypass_reads_info_plist_marker() {
        let mut dict = HashMap::new();
        dict.insert(
            DEVELOPMENT_BUILD_INFO_PLIST_KEY.to_string(),
            "true".to_string(),
        );
        assert!(LicenseConfig::development_build_license_bypass_enabled(
            &dict
        ));
        dict.insert(
            DEVELOPMENT_BUILD_INFO_PLIST_KEY.to_string(),
            "yes".to_string(),
        );
        assert!(LicenseConfig::development_build_license_bypass_enabled(
            &dict
        ));
        assert!(!LicenseConfig::development_build_license_bypass_enabled(
            &HashMap::new()
        ));
    }

    #[test]
    fn license_config_public_key_override_is_dev_only() {
        // Release builds must ignore SUPERCLI_LICENSE_PUBLIC_KEY even when
        // set: pin the dev-only policy (implemented once in supercli-core)
        // with an explicit release flag, using this crate's bundled key.
        assert_eq!(
            supercli_core::license::resolve_public_key_b64_with(
                Some("attacker-key"),
                false,
                BUNDLED_PUBLIC_KEY_BASE64,
            ),
            BUNDLED_PUBLIC_KEY_BASE64
        );
        // Empty override falls back to bundled in any build.
        assert_eq!(
            supercli_core::license::resolve_public_key_b64_with(
                Some("  "),
                true,
                BUNDLED_PUBLIC_KEY_BASE64,
            ),
            BUNDLED_PUBLIC_KEY_BASE64
        );
    }

    #[test]
    fn validate_key_rejects_malformed_keys() {
        // Malformed envelopes fail closed via the unified core implementation.
        assert!(validate_key("SCLI-", "irrelevant").is_err());
        assert!(validate_key("SCLI-nodot", "irrelevant").is_err());
        assert!(validate_key("WRONG-eyJhIjoxfQ.c2ln", "irrelevant").is_err());
        assert!(validate_key("SCLI-!!!.@@@", "irrelevant").is_err());
        // Garbage public key: fails closed, never panics.
        assert!(validate_key("SCLI-eyJhIjoxfQ.c2ln", "").is_err());
        assert!(validate_key("SCLI-eyJhIjoxfQ.c2ln", "not-base64!!!").is_err());
    }

    #[test]
    fn validate_key_rejects_bad_signature() {
        // Well-formed envelope, invalid signature.
        let key = "SCLI-eyJhIjoxfQ.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        assert!(validate_key(key, &"A".repeat(44)).is_err());
    }

    #[test]
    fn validate_key_rejects_legacy_clrty_keys() {
        // CLRTY- keys use the legacy key format; supercli has no
        // legacy customers to migrate. They must be rejected with a clear
        // message, not a generic "malformed" error.
        let legacy = "CLRTY-eyJhIjoxfQ.c2ln";
        let err = validate_key(legacy, &"A".repeat(44)).unwrap_err();
        assert!(
            err.contains("CLRTY- keys use the legacy key format"),
            "unexpected error: {err}"
        );
        // Even a well-formed CLRTY- key (valid base64) is rejected.
        let legacy_wellformed = "CLRTY-eyJhIjoxfQ.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let err = validate_key(legacy_wellformed, &"A".repeat(44)).unwrap_err();
        assert!(err.contains("legacy key format"), "unexpected error: {err}");
    }

    #[test]
    fn device_id_is_sha256_hex() {
        let id = device_id("test-uuid");
        assert_eq!(id.len(), 64);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        // Deterministic.
        assert_eq!(device_id("test-uuid"), id);
        assert_ne!(device_id("other-uuid"), id);
    }

    #[test]
    fn validation_freshness_and_offline_grace() {
        let now = 1_800_000_000u64;
        // Fresh: validated 1 day ago.
        assert!(validation_is_fresh(now - 86_400, now));
        // Stale: validated 8 days ago.
        assert!(!validation_is_fresh(now - 8 * 86_400, now));
        // Within grace: 29 days ago.
        assert!(within_offline_grace(now - 29 * 86_400, now));
        // Beyond grace: 31 days ago.
        assert!(!within_offline_grace(now - 31 * 86_400, now));
    }

    #[test]
    fn payload_round_trips_through_json() {
        let p = payload();
        let json = serde_json::to_vec(&p).unwrap();
        let back: LicensePayload = serde_json::from_slice(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn bundled_public_key_is_pinned_license_v1() {
        // The bundled license public key (v1, provided by Amein) is pinned.
        // Any accidental change must fail CI. This is the LICENSE key only;
        // it must NOT be reused for the updater.
        let engine = base64::engine::general_purpose::STANDARD;
        let key_bytes = engine
            .decode(BUNDLED_PUBLIC_KEY_BASE64.trim())
            .expect("bundled key must be valid base64");
        assert_eq!(key_bytes.len(), 32, "Ed25519 public key must be 32 bytes");
        let key_array: [u8; 32] = key_bytes.try_into().unwrap();
        // Must be a valid Ed25519 point.
        VerifyingKey::from_bytes(&key_array).expect("bundled key must be a valid Ed25519 point");
        // Fingerprint pinning: sha256 of the raw key starts with bff14084e409b8f0.
        let mut hasher = Sha256::new();
        hasher.update(key_array);
        let fingerprint = hex::encode(hasher.finalize());
        assert!(
            fingerprint.starts_with("bff14084e409b8f0"),
            "bundled key fingerprint mismatch: {fingerprint}"
        );
    }
}
