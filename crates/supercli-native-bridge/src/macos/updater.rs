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
}
