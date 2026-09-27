//! Port of `Licensing/LicenseManager.swift` — license verification.
//!
//! Offline Ed25519 verification of `SCLI-<payload>.<signature>` keys,
//! key normalization (smart dashes, whitespace), activation/validation
//! against `https://superc.li`, revocation persistence, and the 7-day
//! validation / 30-day offline-grace policy.
//!
//! Pure crypto and normalization are cross-platform and tested. Network
//! activation and Keychain persistence compose them.

use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

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

/// License key prefix. Rebranded from the Swift `CLRTY-` to `SCLI-`;
/// `CLRTY-` keys (legacy unpeel product) are explicitly rejected.
pub const KEY_PREFIX: &str = "SCLI-";

/// Legacy prefix from the unpeel product. Keys with this prefix are rejected
/// with a clear message; supercli has no legacy customers to migrate.
pub const LEGACY_KEY_PREFIX: &str = "CLRTY-";

/// Validation cadence: re-validate at most every 7 days.
pub const VALIDATION_INTERVAL_SECS: u64 = 7 * 24 * 60 * 60;
/// Offline grace: a license stays valid 30 days without re-validation.
pub const OFFLINE_GRACE_SECS: u64 = 30 * 24 * 60 * 60;

/// License payload carried inside the signed key.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LicensePayload {
    pub v: u32,
    pub id: String,
    pub email: String,
    pub plan: String,
    pub seats: u32,
    pub iat: u64,
}

/// License configuration: bundled defaults with environment overrides.
///
/// SECURITY: environment overrides are honored ONLY in dev builds
/// (`cfg(debug_assertions)`). In release builds the env vars are ignored
/// so an attacker cannot bypass license verification by setting
/// `SUPERCLI_LICENSE_PUBLIC_KEY` to their own key.
pub struct LicenseConfig;

impl LicenseConfig {
    /// Public key base64: in dev builds the env override wins, else the
    /// bundled key. Release builds ALWAYS use the bundled key.
    pub fn public_key_base64(environment: &HashMap<String, String>) -> &str {
        Self::public_key_base64_with_dev_override(environment, cfg!(debug_assertions))
    }

    /// Testable core: `allow_env_override` simulates dev (`true`) vs
    /// release (`false`) builds.
    pub fn public_key_base64_with_dev_override(
        environment: &HashMap<String, String>,
        allow_env_override: bool,
    ) -> &str {
        if allow_env_override {
            if let Some(key) = environment.get(PUBLIC_KEY_ENV_VAR) {
                return key.as_str();
            }
        }
        BUNDLED_PUBLIC_KEY_BASE64
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
    /// accepted. The key slot is a TODO for Amein — never generate or
    /// commit a private key.
    pub fn bundled_public_key() -> Result<&'static str, String> {
        if BUNDLED_PUBLIC_KEY_BASE64.trim().is_empty() {
            Err("No bundled license public key configured — refusing. \
                 (TODO: Amein must embed the production Ed25519 public key.)"
                .to_string())
        } else {
            Ok(BUNDLED_PUBLIC_KEY_BASE64)
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
        let after_scheme = s.splitn(2, "://").nth(1).unwrap_or("");
        !after_scheme.is_empty() && !after_scheme.contains(' ')
    }
}

/// Normalizes a pasted license key: trims whitespace (including NBSP and
/// zero-width space), repairs macOS smart-dash substitutions (en/em dash,
/// figure dash, fullwidth hyphen → `-`). Idempotent.
pub fn normalize_license_key(raw: &str) -> String {
    raw.chars()
        .filter_map(|c| match c {
            // Whitespace to strip (incl. exotic).
            ' ' | '\t' | '\n' | '\r' | '\u{00A0}' | '\u{200B}' | '\u{FEFF}' => None,
            // Unicode dash variants → ASCII hyphen.
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{FF0D}' => {
                Some('-')
            }
            _ => Some(c),
        })
        .collect()
}

/// Splits `SCLI-<payload_b64>.<sig_b64>` into (payload_bytes, signature).
/// Returns `None` for malformed keys.
pub fn split_key(normalized: &str) -> Option<(Vec<u8>, Vec<u8>)> {
    let rest = normalized.strip_prefix(KEY_PREFIX)?;
    let (payload_b64, sig_b64) = rest.split_once('.')?;
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let payload = engine.decode(payload_b64).ok()?;
    let sig = engine.decode(sig_b64).ok()?;
    Some((payload, sig))
}

/// Verifies the Ed25519 signature over the payload bytes. Fails closed on
/// any decode or verification error.
pub fn verify_signature(payload: &[u8], signature: &[u8], public_key_base64: &str) -> bool {
    let engine = base64::engine::general_purpose::STANDARD;
    let Ok(key_bytes) = engine.decode(public_key_base64.trim()) else {
        return false;
    };
    let Ok(key_array): Result<[u8; 32], _> = key_bytes.try_into() else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&key_array) else {
        return false;
    };
    let Ok(sig_array): Result<[u8; 64], _> = signature.try_into() else {
        return false;
    };
    let signature = Signature::from_bytes(&sig_array);
    verifying_key.verify(payload, &signature).is_ok()
}

/// Fully validates a license key: normalize → split → JSON-decode payload
/// → Ed25519-verify. Returns the payload on success.
///
/// Legacy `CLRTY-` keys (issued by the old unpeel product) are rejected with
/// a clear message; supercli has no legacy customers to migrate.
pub fn validate_key(raw: &str, public_key_base64: &str) -> Result<LicensePayload, String> {
    let normalized = normalize_license_key(raw);
    if normalized.starts_with(LEGACY_KEY_PREFIX) {
        return Err(
            "CLRTY- keys are from the legacy unpeel product and are not accepted".to_string(),
        );
    }
    let (payload_bytes, sig_bytes) =
        split_key(&normalized).ok_or_else(|| "malformed license key".to_string())?;
    if !verify_signature(&payload_bytes, &sig_bytes, public_key_base64) {
        return Err("invalid license signature".to_string());
    }
    serde_json::from_slice(&payload_bytes).map_err(|e| format!("invalid license payload: {e}"))
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
        // BUNDLED_PUBLIC_KEY_BASE64 is currently "" (TODO for Amein):
        // activation and update verification must refuse, not proceed.
        let err =
            LicenseConfig::bundled_public_key().expect_err("empty bundled key must fail closed");
        assert!(
            err.contains("No bundled license public key configured"),
            "unexpected error: {err}"
        );
        assert!(err.contains("refusing"), "unexpected error: {err}");
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
    fn split_key_rejects_malformed_keys() {
        assert!(split_key("SCLI-").is_none());
        assert!(split_key("SCLI-nodot").is_none());
        assert!(split_key("WRONG-eyJhIjoxfQ.c2ln").is_none());
        assert!(split_key("SCLI-!!!.@@@").is_none());
    }

    #[test]
    fn verify_signature_fails_closed() {
        // Empty key, wrong length, garbage signature: all false, never panic.
        assert!(!verify_signature(b"payload", &[0u8; 64], ""));
        assert!(!verify_signature(b"payload", &[0u8; 64], "not-base64!!!"));
        assert!(!verify_signature(b"payload", &[0u8; 32], &"A".repeat(44)));
    }

    #[test]
    fn validate_key_rejects_bad_signature() {
        // Well-formed envelope, invalid signature.
        let key = "SCLI-eyJhIjoxfQ.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        assert!(validate_key(key, &"A".repeat(44)).is_err());
    }

    #[test]
    fn validate_key_rejects_legacy_clrty_keys() {
        // CLRTY- keys are from the legacy unpeel product; supercli has no
        // legacy customers to migrate. They must be rejected with a clear
        // message, not a generic "malformed" error.
        let legacy = "CLRTY-eyJhIjoxfQ.c2ln";
        let err = validate_key(legacy, &"A".repeat(44)).unwrap_err();
        assert!(
            err.contains("CLRTY- keys are from the legacy unpeel product"),
            "unexpected error: {err}"
        );
        // Even a well-formed CLRTY- key (valid base64) is rejected.
        let legacy_wellformed = "CLRTY-eyJhIjoxfQ.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let err = validate_key(legacy_wellformed, &"A".repeat(44)).unwrap_err();
        assert!(
            err.contains("legacy unpeel product"),
            "unexpected error: {err}"
        );
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
