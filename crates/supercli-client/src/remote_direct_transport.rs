//! Port of `RemoteDirectTransport.swift` (SupercliIOS).
//!
//! Pure decisions behind the phone's Direct (LAN) `/mobile` transport and
//! the relay's request budget:
//!
//! - which scheme a paired Host gets (pinned HTTPS vs legacy plaintext) and
//!   how that decision is learned from bootstrap / pairing and persisted;
//! - the bootstrap deadline per transport (4 s health poll on the LAN, a
//!   wider budget over the relay so one slow cellular round-trip cannot
//!   feed the reconnect loop);
//! - the push-token registration route for each paired Mac.
//!
//! Everything here is socket-free and unit-tested. The connection store
//! applies these decisions; the HTTP client executes them.
//!
//! Web-safe: compiles for `wasm32-unknown-unknown`.

/// Semantic server version (`"0.5.3"`, `"0.10.0-beta.2"`). Pre-release and
/// build suffixes are ignored: `0.5.3-beta.1` is 0.5.3 for gating purposes
/// because the feature ships with the release line, not the tag.
///
/// Port of `RemoteServerVersion` from `RemoteDirectTransport.swift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RemoteServerVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl RemoteServerVersion {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parse `"0.5.3"`, `"v0.10.0-beta.2"`, `"1.2"` (patch defaults to 0).
    /// Returns `None` when the core is not 1–3 dot-separated non-negative
    /// integers.
    pub fn parse(raw: Option<&str>) -> Option<Self> {
        let raw = raw?;
        let mut core = raw.trim();
        if let Some(stripped) = core.strip_prefix('v').or_else(|| core.strip_prefix('V')) {
            core = stripped;
        }
        // Cut pre-release (`-`) and build (`+`) suffixes.
        let cut = core.find(['-', '+']).unwrap_or(core.len());
        core = &core[..cut];
        let parts: Vec<&str> = core.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return None;
        }
        let mut numbers = [0u32; 3];
        for (i, part) in parts.iter().enumerate() {
            // Swift's `Int(part)` rejects empty strings and signs; match it.
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            numbers[i] = part.parse::<u32>().ok()?;
        }
        Some(Self::new(numbers[0], numbers[1], numbers[2]))
    }
}

/// What a Host said about its Direct transport, extracted at the wire
/// boundary from a bootstrap snapshot or a sealed pairing response.
///
/// Port of `RemoteDirectTransportAdvertisement` from
/// `RemoteDirectTransport.swift`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteDirectTransportAdvertisement {
    /// Lowercase hex SHA-256 of the Host's self-signed TLS leaf — the same
    /// certificate on the `__remote__` WSS port and the `/mobile` port.
    pub certificate_fingerprint: Option<String>,
    pub server_version: Option<String>,
    /// `hostProtocol.capabilities`; `None` on a pre-ledger Host.
    pub host_capabilities: Option<Vec<String>>,
}

impl RemoteDirectTransportAdvertisement {
    pub fn new(
        certificate_fingerprint: Option<String>,
        server_version: Option<String>,
        host_capabilities: Option<Vec<String>>,
    ) -> Self {
        Self {
            certificate_fingerprint,
            server_version,
            host_capabilities,
        }
    }
}

/// The scheme a paired Host's Direct `/mobile` requests use.
///
/// Port of `RemoteDirectTransportDecision` from `RemoteDirectTransport.swift`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteDirectTransportDecision {
    /// Pinned HTTPS. The bearer only ever rides this.
    Tls { fingerprint: String },
    /// The Host said it serves TLS but advertised no certificate to pin to.
    /// Stay on whatever the record already uses; never send the bearer to
    /// an unpinned TLS endpoint, and never assume plaintext is acceptable.
    TlsUnpinnable,
    /// The Host conclusively predates TLS on `/mobile` (a version below the
    /// minimum). Plaintext is the only transport it accepts.
    Plaintext,
    /// The Host said nothing either way (pre-version, pre-ledger). Keep the
    /// record's current transport.
    Unknown,
}

/// Capability flag a Host advertises when it serves TLS on `/mobile`.
/// Mirrors `RemoteControlProtocol.mobileTLSCapability`.
pub const MOBILE_TLS_CAPABILITY: &str = "mobile-tls";

/// Minimum server version that serves TLS on `/mobile`.
/// Mirrors `RemoteControlProtocol.mobileTLSMinimumServerVersion`.
pub const MOBILE_TLS_MINIMUM_SERVER_VERSION: &str = "0.9.0";

/// Pure transport-policy decisions.
///
/// Port of `RemoteDirectTransportPolicy` from `RemoteDirectTransport.swift`.
pub struct RemoteDirectTransportPolicy;

impl RemoteDirectTransportPolicy {
    /// Trim, lowercase; empty becomes `None`.
    pub fn normalized_fingerprint(raw: Option<&str>) -> Option<String> {
        let raw = raw?;
        let trimmed = raw.trim().to_lowercase();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    /// The capability flag wins outright; otherwise a reported server version
    /// at/after the TLS minimum means TLS and a lower one means plaintext.
    /// No signal keeps the current transport.
    pub fn decision(
        advertisement: &RemoteDirectTransportAdvertisement,
    ) -> RemoteDirectTransportDecision {
        let fingerprint =
            Self::normalized_fingerprint(advertisement.certificate_fingerprint.as_deref());
        if advertisement
            .host_capabilities
            .as_deref()
            .map(|caps| caps.iter().any(|c| c == MOBILE_TLS_CAPABILITY))
            .unwrap_or(false)
        {
            return match fingerprint {
                Some(fp) => RemoteDirectTransportDecision::Tls { fingerprint: fp },
                None => RemoteDirectTransportDecision::TlsUnpinnable,
            };
        }
        let version = RemoteServerVersion::parse(advertisement.server_version.as_deref());
        let minimum = RemoteServerVersion::parse(Some(MOBILE_TLS_MINIMUM_SERVER_VERSION));
        match (version, minimum) {
            (Some(v), Some(m)) => {
                if v < m {
                    RemoteDirectTransportDecision::Plaintext
                } else {
                    match fingerprint {
                        Some(fp) => RemoteDirectTransportDecision::Tls { fingerprint: fp },
                        None => RemoteDirectTransportDecision::TlsUnpinnable,
                    }
                }
            }
            _ => RemoteDirectTransportDecision::Unknown,
        }
    }

    /// Whether a plaintext `/mobile` reply is the Host refusing the bearer
    /// over plaintext (the transition-era `426 Upgrade Required`, or a `401`
    /// whose message points at HTTPS/TLS). Any other 4xx keeps its meaning.
    pub fn is_plaintext_refusal(status_code: u16, server_message: Option<&str>) -> bool {
        if status_code == 426 {
            return true;
        }
        if status_code != 401 {
            return false;
        }
        let Some(message) = server_message else {
            return false;
        };
        let lowered = message.to_lowercase();
        lowered.contains("https") || lowered.contains("tls")
    }

    /// Persisted `/mobile` endpoints are always spelled `http://` — the pin,
    /// not the stored scheme, decides the wire. Normalizing here keeps the
    /// endpoint-equality checks in the generation guards stable across a
    /// Host that starts advertising `https://`.
    ///
    /// Port of `RemoteDirectTransportPolicy.canonicalStoredEndpoint`: rewrites
    /// an `https:` scheme to `http:`, leaving anything else untouched.
    pub fn canonical_stored_endpoint(endpoint: &str) -> String {
        // Split off the scheme without a URL parser (web-safe, no deps).
        let Some(colon) = endpoint.find(':') else {
            return endpoint.to_string();
        };
        let (scheme, rest) = endpoint.split_at(colon);
        if scheme.eq_ignore_ascii_case("https") {
            format!("http{rest}")
        } else {
            endpoint.to_string()
        }
    }
}

/// Bootstrap deadlines per transport.
///
/// Port of `RemoteBootstrapDeadline` from `RemoteDirectTransport.swift`.
pub struct RemoteBootstrapDeadline;

impl RemoteBootstrapDeadline {
    /// Health-poll deadline on the LAN, in seconds.
    pub const DIRECT_SECS: f64 = 4.0;
    /// Relay floor/ceiling, in seconds.
    pub const RELAY_MINIMUM_SECS: f64 = 10.0;
    pub const RELAY_MAXIMUM_SECS: f64 = 20.0;
    /// Multiplier on the last measured relay round-trip.
    pub const RELAY_ROUND_TRIP_MULTIPLIER: f64 = 5.0;

    /// Bootstrap is the connection health signal, so its deadline is short on
    /// the LAN. Over the relay a bootstrap crosses the tunnel twice plus the
    /// Host's own work; on cellular that legitimately misses 4 s, and each
    /// miss used to be read as "connection lost". Budget it from the measured
    /// path.
    pub fn seconds(is_relay: bool, measured_round_trip_secs: Option<f64>) -> f64 {
        if !is_relay {
            return Self::DIRECT_SECS;
        }
        let Some(rtt) = measured_round_trip_secs else {
            return Self::RELAY_MINIMUM_SECS;
        };
        if !rtt.is_finite() || rtt <= 0.0 {
            return Self::RELAY_MINIMUM_SECS;
        }
        let scaled = rtt * Self::RELAY_ROUND_TRIP_MULTIPLIER;
        scaled.clamp(Self::RELAY_MINIMUM_SECS, Self::RELAY_MAXIMUM_SECS)
    }
}

/// How an APNs token reaches one paired Mac.
///
/// Port of `PushTokenRegistrationRoute` from `RemoteDirectTransport.swift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PushTokenRegistrationRoute {
    /// POST over the paired Direct endpoint (pinned HTTPS or legacy HTTP).
    Direct,
    /// POST through the connection store's live Link connection for the
    /// active Mac — no second socket, no LAN wait.
    ActiveRelayClient,
    /// POST through a short-lived Link connection built for this Mac.
    TransientRelay,
}

impl PushTokenRegistrationRoute {
    /// Order of attempts for one Mac. The active Mac already on the relay
    /// skips the LAN entirely: that attempt is known to fail and used to hold
    /// the registration for the full 10 s POST timeout before opening a
    /// throwaway relay socket next to the live one.
    pub fn plan(
        is_active_mac: bool,
        using_relay: bool,
        has_relay_credentials: bool,
    ) -> Vec<PushTokenRegistrationRoute> {
        if is_active_mac && using_relay {
            return vec![PushTokenRegistrationRoute::ActiveRelayClient];
        }
        let mut routes = vec![PushTokenRegistrationRoute::Direct];
        if has_relay_credentials {
            routes.push(PushTokenRegistrationRoute::TransientRelay);
        }
        routes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- RemoteServerVersion ---

    #[test]
    fn parses_plain_semver() {
        assert_eq!(
            RemoteServerVersion::parse(Some("0.5.3")),
            Some(RemoteServerVersion::new(0, 5, 3))
        );
    }

    #[test]
    fn strips_v_prefix_and_prerelease() {
        assert_eq!(
            RemoteServerVersion::parse(Some("v0.10.0-beta.2")),
            Some(RemoteServerVersion::new(0, 10, 0))
        );
        assert_eq!(
            RemoteServerVersion::parse(Some("V1.2.3+build.5")),
            Some(RemoteServerVersion::new(1, 2, 3))
        );
    }

    #[test]
    fn pads_missing_components() {
        assert_eq!(
            RemoteServerVersion::parse(Some("1.2")),
            Some(RemoteServerVersion::new(1, 2, 0))
        );
        assert_eq!(
            RemoteServerVersion::parse(Some("1")),
            Some(RemoteServerVersion::new(1, 0, 0))
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(RemoteServerVersion::parse(None), None);
        assert_eq!(RemoteServerVersion::parse(Some("")), None);
        assert_eq!(RemoteServerVersion::parse(Some("1.2.3.4")), None);
        assert_eq!(RemoteServerVersion::parse(Some("a.b.c")), None);
        assert_eq!(RemoteServerVersion::parse(Some("1.-2.3")), None);
        assert_eq!(RemoteServerVersion::parse(Some("1..3")), None);
    }

    #[test]
    fn orders_semantically() {
        let v053 = RemoteServerVersion::new(0, 5, 3);
        let v090 = RemoteServerVersion::new(0, 9, 0);
        let v0100 = RemoteServerVersion::new(0, 10, 0);
        assert!(v053 < v090);
        assert!(v090 < v0100);
        assert!(v090 >= RemoteServerVersion::parse(Some("0.9.0")).unwrap());
    }

    // --- normalized_fingerprint ---

    #[test]
    fn normalizes_fingerprint() {
        assert_eq!(
            RemoteDirectTransportPolicy::normalized_fingerprint(Some("  ABC123  ")),
            Some("abc123".to_string())
        );
        assert_eq!(
            RemoteDirectTransportPolicy::normalized_fingerprint(Some("   ")),
            None
        );
        assert_eq!(
            RemoteDirectTransportPolicy::normalized_fingerprint(None),
            None
        );
    }

    // --- decision ---

    fn ad(
        fingerprint: Option<&str>,
        version: Option<&str>,
        caps: Option<Vec<&str>>,
    ) -> RemoteDirectTransportAdvertisement {
        RemoteDirectTransportAdvertisement::new(
            fingerprint.map(str::to_string),
            version.map(str::to_string),
            caps.map(|v| v.into_iter().map(str::to_string).collect()),
        )
    }

    #[test]
    fn capability_wins_with_fingerprint() {
        let d = RemoteDirectTransportPolicy::decision(&ad(
            Some("AA:BB"),
            Some("0.1.0"),
            Some(vec![MOBILE_TLS_CAPABILITY]),
        ));
        assert_eq!(
            d,
            RemoteDirectTransportDecision::Tls {
                fingerprint: "aa:bb".to_string()
            }
        );
    }

    #[test]
    fn capability_without_fingerprint_is_unpinnable() {
        let d = RemoteDirectTransportPolicy::decision(&ad(
            None,
            Some("0.1.0"),
            Some(vec![MOBILE_TLS_CAPABILITY]),
        ));
        assert_eq!(d, RemoteDirectTransportDecision::TlsUnpinnable);
    }

    #[test]
    fn old_version_means_plaintext() {
        let d = RemoteDirectTransportPolicy::decision(&ad(Some("aa"), Some("0.5.3"), None));
        assert_eq!(d, RemoteDirectTransportDecision::Plaintext);
    }

    #[test]
    fn new_version_with_fingerprint_means_tls() {
        let d = RemoteDirectTransportPolicy::decision(&ad(Some("AA"), Some("0.9.0"), None));
        assert_eq!(
            d,
            RemoteDirectTransportDecision::Tls {
                fingerprint: "aa".to_string()
            }
        );
    }

    #[test]
    fn new_version_without_fingerprint_is_unpinnable() {
        let d = RemoteDirectTransportPolicy::decision(&ad(None, Some("1.0.0"), None));
        assert_eq!(d, RemoteDirectTransportDecision::TlsUnpinnable);
    }

    #[test]
    fn no_signal_is_unknown() {
        let d = RemoteDirectTransportPolicy::decision(&ad(None, None, None));
        assert_eq!(d, RemoteDirectTransportDecision::Unknown);
        let d = RemoteDirectTransportPolicy::decision(&ad(Some("aa"), Some("bogus"), None));
        assert_eq!(d, RemoteDirectTransportDecision::Unknown);
    }

    // --- is_plaintext_refusal ---

    #[test]
    fn detects_plaintext_refusal() {
        assert!(RemoteDirectTransportPolicy::is_plaintext_refusal(426, None));
        assert!(RemoteDirectTransportPolicy::is_plaintext_refusal(
            401,
            Some("use https")
        ));
        assert!(RemoteDirectTransportPolicy::is_plaintext_refusal(
            401,
            Some("TLS required")
        ));
        assert!(!RemoteDirectTransportPolicy::is_plaintext_refusal(
            401,
            Some("bad token")
        ));
        assert!(!RemoteDirectTransportPolicy::is_plaintext_refusal(
            401, None
        ));
        assert!(!RemoteDirectTransportPolicy::is_plaintext_refusal(
            403,
            Some("https")
        ));
        assert!(!RemoteDirectTransportPolicy::is_plaintext_refusal(
            200, None
        ));
    }

    // --- canonical_stored_endpoint ---

    #[test]
    fn canonicalizes_https_to_http() {
        assert_eq!(
            RemoteDirectTransportPolicy::canonical_stored_endpoint("https://mac.local:8443/mobile"),
            "http://mac.local:8443/mobile"
        );
        assert_eq!(
            RemoteDirectTransportPolicy::canonical_stored_endpoint("HTTPS://mac.local/mobile"),
            "http://mac.local/mobile"
        );
        // Anything else is untouched.
        assert_eq!(
            RemoteDirectTransportPolicy::canonical_stored_endpoint("http://mac.local/mobile"),
            "http://mac.local/mobile"
        );
        assert_eq!(
            RemoteDirectTransportPolicy::canonical_stored_endpoint("not a url"),
            "not a url"
        );
    }

    // --- RemoteBootstrapDeadline ---

    #[test]
    fn bootstrap_deadlines() {
        assert_eq!(RemoteBootstrapDeadline::seconds(false, None), 4.0);
        assert_eq!(RemoteBootstrapDeadline::seconds(false, Some(1.0)), 4.0);
        // No measurement -> minimum.
        assert_eq!(RemoteBootstrapDeadline::seconds(true, None), 10.0);
        assert_eq!(RemoteBootstrapDeadline::seconds(true, Some(0.0)), 10.0);
        assert_eq!(
            RemoteBootstrapDeadline::seconds(true, Some(f64::INFINITY)),
            10.0
        );
        // 1s RTT * 5 = 5s -> clamped to minimum 10s.
        assert_eq!(RemoteBootstrapDeadline::seconds(true, Some(1.0)), 10.0);
        // 3s RTT * 5 = 15s -> within [10, 20].
        assert_eq!(RemoteBootstrapDeadline::seconds(true, Some(3.0)), 15.0);
        // 10s RTT * 5 = 50s -> clamped to maximum 20s.
        assert_eq!(RemoteBootstrapDeadline::seconds(true, Some(10.0)), 20.0);
    }

    // --- PushTokenRegistrationRoute::plan ---

    #[test]
    fn push_route_plans() {
        // Active Mac on the relay: relay only, skip the doomed LAN attempt.
        assert_eq!(
            PushTokenRegistrationRoute::plan(true, true, true),
            vec![PushTokenRegistrationRoute::ActiveRelayClient]
        );
        // Otherwise: direct first, transient relay when credentials exist.
        assert_eq!(
            PushTokenRegistrationRoute::plan(false, true, true),
            vec![
                PushTokenRegistrationRoute::Direct,
                PushTokenRegistrationRoute::TransientRelay
            ]
        );
        assert_eq!(
            PushTokenRegistrationRoute::plan(false, false, false),
            vec![PushTokenRegistrationRoute::Direct]
        );
        // Active Mac NOT on relay falls through to the normal plan.
        assert_eq!(
            PushTokenRegistrationRoute::plan(true, false, true),
            vec![
                PushTokenRegistrationRoute::Direct,
                PushTokenRegistrationRoute::TransientRelay
            ]
        );
    }
}
