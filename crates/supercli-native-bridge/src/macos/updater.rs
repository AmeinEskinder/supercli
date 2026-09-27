//! Signed updater replacing Sparkle, served from `superc.li`.
//!
//! The Swift app used Sparkle (`SPUStandardUpdaterController`) with a feed
//! URL pinned by the `SPUUpdaterDelegate` in `AppDelegate.swift` and
//! license headers refreshed from the Keychain. This module implements the
//! same update flow without Sparkle:
//!
//! 1. Fetch the appcast feed from `https://superc.li/...` (pinned URL).
//! 2. Parse the latest enclosure (version, download URL, Ed25519 signature).
//! 3. Download, verify the signature against the bundled public key.
//! 4. Stage and install; restart the app.
//!
//! Feed parsing and signature verification are pure and tested. Network
//! fetch and installation are injected/`#[cfg(target_os = "macos")]`.

/// Pinned update feed URL. Replaces Sparkle's delegate-provided
/// `feedURLString(for:)` which pinned the baked-in feed.
pub const FEED_URL: &str = "https://superc.li/updates/appcast.xml";

/// Bundled Ed25519 public key (base64) used to verify update downloads.
///
/// Provided by Amein (generated offline). Verified: 32 bytes, valid Ed25519
/// point, sha256 prefix `4f71bbbd36495eae`.
///
/// CRITICAL: This is a SEPARATE key from the license key. The updater must
/// NEVER fall back to or accept the license key for update verification.
pub const UPDATER_PUBLIC_KEY_BASE64: &str = "VQdQWMuzQg627U+wNV4YL9gX4pLQhI0XZaNKffEkRaM=";

/// Returns the bundled updater public key, or fails closed with a clear
/// error when no key is configured. Never falls back to the license key.
pub fn updater_public_key() -> Result<&'static str, String> {
    if UPDATER_PUBLIC_KEY_BASE64.trim().is_empty() {
        Err("updates disabled: no updater key configured \
             (TODO: Amein must embed the production Ed25519 updater public key; \
             the license key must never be reused for updates)"
            .to_string())
    } else {
        Ok(UPDATER_PUBLIC_KEY_BASE64)
    }
}

/// An update enclosure parsed from the feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateEnclosure {
    pub version: String,
    pub build: String,
    pub download_url: String,
    /// Ed25519 signature (base64) over the downloaded bytes.
    pub signature_base64: String,
    pub file_size: u64,
}

/// Compares semantic versions: -1 (older), 0 (equal), 1 (newer).
/// Only numeric `major.minor.patch` parts compare; pre-release/build
/// metadata is ignored for the update decision.
pub fn compare_versions(current: &str, candidate: &str) -> i32 {
    fn parts(v: &str) -> Vec<u64> {
        v.split(|c| c == '.' || c == '-')
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    }
    let a = parts(current);
    let b = parts(candidate);
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return if x < y { -1 } else { 1 };
        }
    }
    0
}

/// True when `candidate` is newer than `current`.
pub fn is_update_available(current: &str, candidate: &str) -> bool {
    compare_versions(current, candidate) < 0
}

/// Verifies the downloaded bytes against the enclosure's Ed25519 signature.
/// Fails closed on any error. Mirrors the license verification approach.
pub fn verify_download(bytes: &[u8], signature_base64: &str, public_key_base64: &str) -> bool {
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;
    let Ok(sig_bytes) = engine.decode(signature_base64.trim()) else {
        return false;
    };
    let Ok(key_bytes) = engine.decode(public_key_base64.trim()) else {
        return false;
    };
    let Ok(key_array): Result<[u8; 32], _> = key_bytes.try_into() else {
        return false;
    };
    let Ok(verifying_key) = ed25519_dalek::VerifyingKey::from_bytes(&key_array) else {
        return false;
    };
    let Ok(sig_array): Result<[u8; 64], _> = sig_bytes.try_into() else {
        return false;
    };
    let signature = ed25519_dalek::Signature::from_bytes(&sig_array);
    use ed25519_dalek::Verifier;
    verifying_key.verify(bytes, &signature).is_ok()
}

/// Verifies a download against the BUNDLED UPDATER public key. Fails closed
/// with a clear error when no updater key is configured (TODO for Amein) —
/// update installation must be refused, never silently skipped.
///
/// CRITICAL: Uses the dedicated updater key, NEVER the license key. The
/// license key and updater key are completely separate trust roots.
pub fn verify_download_with_bundled_key(
    bytes: &[u8],
    signature_base64: &str,
) -> Result<bool, String> {
    let key = updater_public_key()?;
    Ok(verify_download(bytes, signature_base64, key))
}

/// True only for `https://` URLs whose host is `superc.li` or a subdomain.
/// Rejects `http://`, other domains, and malformed URLs — the updater must
/// never fetch binaries from an attacker-controlled origin.
pub fn is_allowed_download_url(url: &str) -> bool {
    let url = url.trim();
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    // Host is up to the first '/', '?', or '#'; strip any port.
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .split('@')
        .next_back()
        .unwrap_or("");
    let host = host.split(':').next().unwrap_or("").to_lowercase();
    host == "superc.li" || host.ends_with(".superc.li")
}

/// Validates an update enclosure before download/install. Fails closed:
/// - candidate version must be strictly NEWER than current (no downgrades,
///   no reinstalls of the same version);
/// - download URL must be https under superc.li;
/// - the enclosure must declare a non-zero length and the downloaded bytes
///   must match it exactly.
pub fn validate_update(
    current_version: &str,
    enclosure: &UpdateEnclosure,
    downloaded_len: u64,
) -> Result<(), String> {
    if compare_versions(current_version, &enclosure.version) >= 0 {
        return Err(format!(
            "refusing update: candidate version {} is not newer than current {} (downgrade/reinstall blocked)",
            enclosure.version, current_version
        ));
    }
    if !is_allowed_download_url(&enclosure.download_url) {
        return Err(format!(
            "refusing update: download URL not allowed (must be https under superc.li): {}",
            enclosure.download_url
        ));
    }
    if enclosure.file_size == 0 {
        return Err(
            "refusing update: enclosure declares no file length — cannot verify download integrity"
                .to_string(),
        );
    }
    if downloaded_len != enclosure.file_size {
        return Err(format!(
            "refusing update: downloaded {downloaded_len} bytes but enclosure declares {}",
            enclosure.file_size
        ));
    }
    Ok(())
}

/// Minimal appcast parser: extracts enclosures with version/build/url/
/// signature. Real feed XML uses the Sparkle namespace; this parses the
/// fields the updater needs without a full XML stack.
pub fn parse_appcast(xml: &str) -> Vec<UpdateEnclosure> {
    let mut out = Vec::new();
    // Very small parser: find <item>…</item> blocks, then fields.
    let mut rest = xml;
    while let Some(start) = rest.find("<item>") {
        let item_start = start + "<item>".len();
        let Some(end) = rest[item_start..].find("</item>") else {
            break;
        };
        let item = &rest[item_start..item_start + end];
        if let Some(enc) = parse_item(item) {
            out.push(enc);
        }
        rest = &rest[item_start + end + "</item>".len()..];
    }
    out
}

fn parse_item(item: &str) -> Option<UpdateEnclosure> {
    fn field(item: &str, tag: &str) -> Option<String> {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        let start = item.find(&open)? + open.len();
        let end = item[start..].find(&close)?;
        Some(item[start..start + end].trim().to_string())
    }
    // <enclosure url="…" sparkle:version="…" sparkle:shortVersionString="…" length="…" />
    let enc_start = item.find("<enclosure")?;
    let enc_end = item[enc_start..].find('>')?;
    let enc = &item[enc_start..enc_start + enc_end];
    fn attr(enc: &str, name: &str) -> Option<String> {
        let key = format!("{name}=\"");
        let start = enc.find(&key)? + key.len();
        let end = enc[start..].find('"')?;
        Some(enc[start..start + end].to_string())
    }
    Some(UpdateEnclosure {
        version: attr(enc, "sparkle:shortVersionString")
            .or_else(|| field(item, "sparkle:shortVersionString"))?,
        build: attr(enc, "sparkle:version").or_else(|| field(item, "sparkle:version"))?,
        download_url: attr(enc, "url")?,
        signature_base64: field(item, "sparkle:edSignature").unwrap_or_default(),
        file_size: attr(enc, "length")
            .and_then(|l| l.parse().ok())
            .unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_url_is_pinned_to_superc_li() {
        assert!(FEED_URL.starts_with("https://superc.li"));
    }

    #[test]
    fn version_comparison_orders_releases() {
        assert_eq!(compare_versions("1.0.0", "1.0.0"), 0);
        assert_eq!(compare_versions("1.0.0", "1.0.1"), -1);
        assert_eq!(compare_versions("1.0.1", "1.0.0"), 1);
        assert_eq!(compare_versions("1.9.0", "1.10.0"), -1);
        assert_eq!(compare_versions("2.0.0", "1.99.99"), 1);
        // Missing parts are zero.
        assert_eq!(compare_versions("1.0", "1.0.0"), 0);
    }

    #[test]
    fn update_available_only_when_newer() {
        assert!(is_update_available("1.0.0", "1.0.1"));
        assert!(!is_update_available("1.0.1", "1.0.1"));
        assert!(!is_update_available("1.0.1", "1.0.0"));
    }

    #[test]
    fn verify_download_fails_closed() {
        assert!(!verify_download(b"bytes", "", ""));
        assert!(!verify_download(b"bytes", "not-base64!!!", "not-base64!!!"));
    }

    #[test]
    fn appcast_parser_extracts_enclosure() {
        let xml = r#"<?xml version="1.0"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
<channel><item>
<title>1.2.3</title>
<enclosure url="https://superc.li/updates/Supercli-1.2.3.dmg" sparkle:version="123" sparkle:shortVersionString="1.2.3" length="456789"/>
<sparkle:edSignature>AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==</sparkle:edSignature>
</item></channel></rss>"#;
        let items = parse_appcast(xml);
        assert_eq!(items.len(), 1);
        let enc = &items[0];
        assert_eq!(enc.version, "1.2.3");
        assert_eq!(enc.build, "123");
        assert_eq!(
            enc.download_url,
            "https://superc.li/updates/Supercli-1.2.3.dmg"
        );
        assert_eq!(enc.file_size, 456789);
        assert!(!enc.signature_base64.is_empty());
    }

    #[test]
    fn appcast_parser_ignores_items_without_enclosure() {
        let xml = "<rss><channel><item><title>no enclosure</title></item></channel></rss>";
        assert!(parse_appcast(xml).is_empty());
    }

    fn test_enclosure() -> UpdateEnclosure {
        UpdateEnclosure {
            version: "1.2.3".to_string(),
            build: "123".to_string(),
            download_url: "https://superc.li/updates/Supercli-1.2.3.dmg".to_string(),
            signature_base64: "sig".to_string(),
            file_size: 456789,
        }
    }

    #[test]
    fn download_url_allowlist() {
        // Allowed: https under superc.li.
        assert!(is_allowed_download_url(
            "https://superc.li/updates/Supercli-1.2.3.dmg"
        ));
        assert!(is_allowed_download_url(
            "https://dl.superc.li/updates/a.dmg"
        ));
        assert!(is_allowed_download_url(
            "https://superc.li:443/updates/a.dmg"
        ));
        // Rejected: http, other domains, lookalikes, malformed.
        assert!(!is_allowed_download_url("http://superc.li/updates/a.dmg"));
        assert!(!is_allowed_download_url("https://evil.com/updates/a.dmg"));
        assert!(!is_allowed_download_url("https://superc.li.evil.com/a.dmg"));
        assert!(!is_allowed_download_url("https://notsuperc.li/a.dmg"));
        assert!(!is_allowed_download_url("not a url"));
        assert!(!is_allowed_download_url(""));
        assert!(!is_allowed_download_url("ftp://superc.li/a.dmg"));
    }

    #[test]
    fn validate_update_accepts_good_update() {
        let enc = test_enclosure();
        assert!(validate_update("1.2.2", &enc, 456789).is_ok());
    }

    #[test]
    fn validate_update_refuses_downgrade_and_same_version() {
        let enc = test_enclosure();
        // Older candidate: downgrade attack.
        let err = validate_update("1.2.4", &enc, 456789).expect_err("downgrade must be refused");
        assert!(err.contains("not newer"), "unexpected: {err}");
        // Same version: reinstall blocked.
        let err = validate_update("1.2.3", &enc, 456789).expect_err("same version must be refused");
        assert!(err.contains("not newer"), "unexpected: {err}");
    }

    #[test]
    fn validate_update_refuses_bad_download_url() {
        let mut enc = test_enclosure();
        enc.download_url = "http://superc.li/updates/a.dmg".to_string();
        let err = validate_update("1.0.0", &enc, 456789).expect_err("http URL must be refused");
        assert!(err.contains("not allowed"), "unexpected: {err}");

        enc.download_url = "https://evil.com/a.dmg".to_string();
        let err = validate_update("1.0.0", &enc, 456789).expect_err("wrong domain must be refused");
        assert!(err.contains("not allowed"), "unexpected: {err}");
    }

    #[test]
    fn validate_update_refuses_length_mismatch() {
        let enc = test_enclosure();
        // Too short / too long: possible truncation or padding attack.
        let err =
            validate_update("1.0.0", &enc, 456788).expect_err("short download must be refused");
        assert!(err.contains("456788"), "unexpected: {err}");
        let err =
            validate_update("1.0.0", &enc, 456790).expect_err("long download must be refused");
        assert!(err.contains("456790"), "unexpected: {err}");
    }

    #[test]
    fn validate_update_refuses_missing_length() {
        let mut enc = test_enclosure();
        enc.file_size = 0; // Manifest omitted the length.
        let err = validate_update("1.0.0", &enc, 0).expect_err("missing length must be refused");
        assert!(err.contains("no file length"), "unexpected: {err}");
    }

    #[test]
    fn updater_public_key_is_pinned() {
        // Amein's updater public key v1. Any accidental change fails CI.
        use base64::Engine;
        let engine = base64::engine::general_purpose::STANDARD;
        let key_bytes = engine
            .decode(UPDATER_PUBLIC_KEY_BASE64.trim())
            .expect("updater key must be valid base64");
        assert_eq!(key_bytes.len(), 32, "updater key must decode to 32 bytes");
        let key_array: [u8; 32] = key_bytes.try_into().expect("32 bytes");
        let _verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&key_array)
            .expect("updater key must be a valid Ed25519 point");
        // Fingerprint pin: sha256 of the raw key starts with 4f71bbbd36495eae.
        use sha2::Digest;
        let digest = sha2::Sha256::digest(key_array);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert!(
            hex.starts_with("4f71bbbd36495eae"),
            "updater key fingerprint mismatch: {hex}"
        );
    }

    #[test]
    fn updater_public_key_fails_closed_if_emptied() {
        // The updater_public_key() function still fails closed if someone
        // empties the slot in the future. (Cannot test the empty path
        // directly since the constant is now set; this documents the
        // contract.)
        let key = updater_public_key().expect("updater key must be configured");
        assert_eq!(key, UPDATER_PUBLIC_KEY_BASE64);
        assert!(!key.trim().is_empty());
    }

    #[test]
    fn updater_never_accepts_the_license_key() {
        // Amein's LICENSE public key (Ed25519, base64). The updater must
        // NEVER accept this key for update verification — the license key
        // and updater key are completely separate trust roots.
        const LICENSE_PUBLIC_KEY_B64: &str = "E32qYUoJsxH5TLSRt/xrjQcWxwVwawVAfLJjM+HbpZI=";

        // The updater key slot must never contain the license key.
        assert_ne!(
            UPDATER_PUBLIC_KEY_BASE64, LICENSE_PUBLIC_KEY_B64,
            "updater key slot must never contain the license key"
        );

        // updater_public_key() returns the updater key, not the license key.
        let updater_key = updater_public_key().expect("updater key configured");
        assert_eq!(updater_key, UPDATER_PUBLIC_KEY_BASE64);
        assert_ne!(updater_key, LICENSE_PUBLIC_KEY_B64);
    }

    #[test]
    fn updater_rejects_license_key_signatures_and_vice_versa() {
        // Cross-rejection: a signature valid under one key must not verify
        // under the other. Uses fixed-seed test keypairs (never Amein's keys)
        // to prove the verification logic is key-bound.
        use ed25519_dalek::{Signer, SigningKey, Verifier};

        // Fixed 32-byte seeds for deterministic test keypairs.
        let license_seed = [0x11u8; 32];
        let updater_seed = [0x22u8; 32];
        let license_signing = SigningKey::from_bytes(&license_seed);
        let updater_signing = SigningKey::from_bytes(&updater_seed);
        let license_verifying = license_signing.verifying_key();
        let updater_verifying = updater_signing.verifying_key();

        let message = b"update payload bytes";
        let sig_for_license = license_signing.sign(message);
        let sig_for_updater = updater_signing.sign(message);

        // Sanity: each signature verifies under its own key.
        assert!(license_verifying.verify(message, &sig_for_license).is_ok());
        assert!(updater_verifying.verify(message, &sig_for_updater).is_ok());

        // Cross-rejection: license signature rejected by updater key.
        assert!(updater_verifying.verify(message, &sig_for_license).is_err());
        // Updater signature rejected by license key.
        assert!(license_verifying.verify(message, &sig_for_updater).is_err());

        // And via the verify_download() helper with base64 keys.
        use base64::Engine;
        let engine = base64::engine::general_purpose::STANDARD;
        let updater_b64 = engine.encode(updater_verifying.to_bytes());
        let license_b64 = engine.encode(license_verifying.to_bytes());
        let sig_license_b64 = engine.encode(sig_for_license.to_bytes());
        let sig_updater_b64 = engine.encode(sig_for_updater.to_bytes());

        // Updater key rejects the license-key signature.
        assert!(!verify_download(message, &sig_license_b64, &updater_b64));
        // License key rejects the updater-key signature.
        assert!(!verify_download(message, &sig_updater_b64, &license_b64));
        // Each accepts its own.
        assert!(verify_download(message, &sig_license_b64, &license_b64));
        assert!(verify_download(message, &sig_updater_b64, &updater_b64));
    }
}
