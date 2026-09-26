//! Provider theme read: one-off request to the Host for the agent provider's
//! brand palette, plus the mascot decision record. Ported from
//! `clients/native/SupercliNative/Sources/SupercliNative/ProviderThemeReadRequest.swift`
//! (one-off request with a 2s read cap; the panel owns the presentation).
//!
//! Mascot note (Swift audits `MacAssetTests.swift`): the mascot asset is
//! deliberately the *placeholder* mascot (never shown to users — it only
//! appears in SwiftUI previews); the shipped empty state uses
//! `EmptyArtwork`. No mascot image is ported.

use serde::{Deserialize, Serialize};

/// One-off request for a provider's brand palette. Mirrors
/// `ProviderThemeReadRequest` exactly: `GET /mobile/theme/provider`,
/// `providerID` percent-encoded, 2-second cap, best-effort (no error
/// surfacing, launcher caches successes only).
pub struct ProviderThemeReadRequest {
    pub host: String,
    pub provider_id: String,
}

impl ProviderThemeReadRequest {
    pub const READ_TIMEOUT_MS: u64 = 2_000;

    pub fn new(host: &str, provider_id: &str) -> Self {
        Self {
            host: host.to_string(),
            provider_id: provider_id.to_string(),
        }
    }

    pub fn url(&self) -> String {
        format!(
            "{}/mobile/theme/provider?providerID={}",
            self.host.trim_end_matches('/'),
            percent_encode(&self.provider_id)
        )
    }
}

/// The Host's brand palette response for a provider.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTheme {
    #[serde(default)]
    pub primary_color_hex: Option<String>,
    #[serde(default)]
    pub secondary_color_hex: Option<String>,
}

impl ProviderTheme {
    pub fn primary(&self) -> Option<&str> {
        self.primary_color_hex.as_deref()
    }
}

/// RFC 3986 percent-encoding (mirrors the Swift escaping of `providerID`).
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// A cache of successfully-read provider themes (failures are dropped and
/// never cached — mirrors the Swift launcher's best-effort read).
#[derive(Clone, Debug, Default)]
pub struct ProviderThemeCache {
    inner: std::collections::HashMap<String, ProviderTheme>,
}

impl ProviderThemeCache {
    pub fn get(&self, provider_id: &str) -> Option<&ProviderTheme> {
        self.inner.get(provider_id)
    }

    pub fn insert(&mut self, provider_id: &str, theme: ProviderTheme) {
        self.inner.insert(provider_id.to_string(), theme);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_percent_encodes_provider_id() {
        let req = ProviderThemeReadRequest::new("https://h:3443", "claude code");
        assert_eq!(
            req.url(),
            "https://h:3443/mobile/theme/provider?providerID=claude%20code"
        );
        let req2 = ProviderThemeReadRequest::new("https://h:3443/", "a/b?c");
        assert_eq!(
            req2.url(),
            "https://h:3443/mobile/theme/provider?providerID=a%2Fb%3Fc"
        );
    }

    #[test]
    fn read_timeout_is_two_seconds() {
        assert_eq!(ProviderThemeReadRequest::READ_TIMEOUT_MS, 2_000);
    }

    #[test]
    fn cache_round_trip() {
        let mut cache = ProviderThemeCache::default();
        assert!(cache.get("claude").is_none());
        cache.insert(
            "claude",
            ProviderTheme {
                primary_color_hex: Some("#D97757".into()),
                secondary_color_hex: None,
            },
        );
        assert_eq!(cache.get("claude").unwrap().primary(), Some("#D97757"));
        assert!(cache.get("opencode").is_none());
    }

    #[test]
    fn theme_deserializes_host_shape() {
        let json = r##"{"primaryColorHex":"#D97757"}"##;
        let theme: ProviderTheme = serde_json::from_str(json).unwrap();
        assert_eq!(theme.primary(), Some("#D97757"));
        assert_eq!(theme.secondary_color_hex, None);
    }
}
