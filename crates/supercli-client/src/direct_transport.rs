//! Pure decisions behind the phone's Direct (LAN) `/mobile` transport and
//! the relay's request budget.
//!
//! Port of `RemoteDirectTransport.swift` (`clients/legacy/ios/SupercliIOS`):
//! - which scheme a paired Host gets (pinned HTTPS vs legacy plaintext) and
//!   how that decision is learned from bootstrap / pairing;
//! - the bootstrap deadline per transport (4 s health poll on the LAN, a
//!   wider budget over the relay so one slow cellular round-trip cannot
//!   feed the reconnect loop);
//! - the push-token registration route for each paired Mac.
//!
//! Everything here is socket-free and unit-tested. The connection store
//! applies these decisions; the client executes them.

use super::protocol::capabilities;

/// Minimum server version that serves TLS on `/mobile`
/// (`RemoteControlProtocol.mobileTLSMinimumServerVersion`).
pub const MOBILE_TLS_MINIMUM_SERVER_VERSION: &str = "0.5.3";

/// Semantic server version (`"0.5.3"`, `"0.10.0-beta.2"`). Pre-release and
/// build suffixes are ignored: `0.5.3-beta.1` is 0.5.3 for gating purposes
/// because the feature ships with the release line, not the tag.
///
/// Port of `RemoteServerVersion`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RemoteServerVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl RemoteServerVersion {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parse `"0.5.3"`, `"v0.5.3"`, `"0.5.3-beta.1"`, `"1"`. Returns `None`
    /// for anything else (empty, non-numeric parts, more than 3 components).
    pub fn parse(raw: Option<&str>) -> Option<Self> {
        let raw = raw?;
        let mut core = raw.trim();
        if core.is_empty() {
            return None;
        }
        if let Some(stripped) = core.strip_prefix('v').or_else(|| core.strip_prefix('V')) {
            core = stripped;
        }
        // Cut pre-release (`-`) and build (`+`) suffixes.
        if let Some(cut) = core.find(['-', '+']) {
            core = &core[..cut];
        }
        let parts: Vec<&str> = core.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return None;
        }
        let mut numbers = [0u64; 3];
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                return None;
            }
            numbers[i] = part.parse::<u64>().ok()?;
        }
        Some(Self::new(numbers[0], numbers[1], numbers[2]))
    }
}

/// What a Host said about its Direct transport, extracted at the wire
/// boundary from a bootstrap snapshot or a sealed pairing response.
///
/// Port of `RemoteDirectTransportAdvertisement`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteDirectTransportAdvertisement {
    /// Lowercase hex SHA-256 of the Host's self-signed TLS leaf.
    pub certificate_fingerprint: Option<String>,
    pub server_version: Option<String>,
    /// `hostProtocol.capabilities`; None on a pre-ledger Host.
    pub host_capabilities: Option<Vec<String>>,
}

/// The scheme a paired Host's Direct `/mobile` requests use.
///
/// Port of `RemoteDirectTransportDecision`.
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

/// Strict-but-liberal fingerprint normalization for the transport decision:
/// trims whitespace, lowercases; empty becomes None. (The WS pinning path in
/// `stream_transport` additionally requires 64 hex chars; the transport
/// decision only needs a non-empty pin to attempt TLS.)
pub fn normalized_fingerprint(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim().to_lowercase();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// The transport decision for one Host: capability flag wins outright;
/// otherwise a reported server version at/after the minimum means TLS and a
/// lower one means plaintext. No signal keeps the current transport.
///
/// Port of `RemoteDirectTransportPolicy.decision`.
pub fn transport_decision(
    advertisement: &RemoteDirectTransportAdvertisement,
) -> RemoteDirectTransportDecision {
    let fingerprint = normalized_fingerprint(advertisement.certificate_fingerprint.as_deref());
    if advertisement
        .host_capabilities
        .as_deref()
        .is_some_and(|caps| caps.iter().any(|c| c == capabilities::HOST_MOBILE_TLS))
    {
        return fingerprint.map_or(RemoteDirectTransportDecision::TlsUnpinnable, |f| {
            RemoteDirectTransportDecision::Tls { fingerprint: f }
        });
    }
    let version = RemoteServerVersion::parse(advertisement.server_version.as_deref());
    let minimum = RemoteServerVersion::parse(Some(MOBILE_TLS_MINIMUM_SERVER_VERSION));
    match (version, minimum) {
        (Some(v), Some(m)) if v >= m => fingerprint
            .map_or(RemoteDirectTransportDecision::TlsUnpinnable, |f| {
                RemoteDirectTransportDecision::Tls { fingerprint: f }
            }),
        (Some(_), Some(_)) => RemoteDirectTransportDecision::Plaintext,
        _ => RemoteDirectTransportDecision::Unknown,
    }
}

/// Whether a plaintext `/mobile` reply is the Host refusing the bearer
/// over plaintext (the transition-era `426 Upgrade Required`, or a `401`
/// whose message points at HTTPS/TLS). Any other 4xx keeps its meaning.
///
/// Port of `RemoteDirectTransportPolicy.isPlaintextRefusal`.
pub fn is_plaintext_refusal(status_code: u16, server_message: Option<&str>) -> bool {
    if status_code == 426 {
        return true;
    }
    if status_code == 401 {
        if let Some(message) = server_message {
            let lowered = message.to_lowercase();
            return lowered.contains("https") || lowered.contains("tls");
        }
    }
    false
}

/// Persisted `/mobile` endpoints are always spelled `http://` — the pin,
/// not the stored scheme, decides the wire. Normalizing here keeps the
/// endpoint-equality checks in the generation guards stable across a
/// Host that starts advertising `https://`.
///
/// Port of `RemoteDirectTransportPolicy.canonicalStoredEndpoint`.
/// Pure string transform (no `url` crate in this crate): rewrites a
/// leading `https://` scheme to `http://`, case-insensitively.
pub fn canonical_stored_endpoint(endpoint: &str) -> String {
    if endpoint.len() >= 8 && endpoint[..8].eq_ignore_ascii_case("https://") {
        format!("http://{}", &endpoint[8..])
    } else {
        endpoint.to_string()
    }
}

/// Bootstrap is the connection health signal, so its deadline is short on
/// the LAN. Over the relay a bootstrap crosses the tunnel twice plus the
/// Host's own work; on cellular that legitimately misses 4 s, and each miss
/// used to be read as "connection lost". Budget it from the measured path.
///
/// Port of `RemoteBootstrapDeadline`.
pub struct RemoteBootstrapDeadline;

impl RemoteBootstrapDeadline {
    pub const DIRECT: f64 = 4.0;
    pub const RELAY_MINIMUM: f64 = 10.0;
    pub const RELAY_MAXIMUM: f64 = 20.0;
    /// Multiplier on the last measured relay round-trip.
    pub const RELAY_ROUND_TRIP_MULTIPLIER: f64 = 5.0;

    pub fn seconds(is_relay: bool, measured_round_trip: Option<f64>) -> f64 {
        if !is_relay {
            return Self::DIRECT;
        }
        match measured_round_trip {
            Some(rtt) if rtt.is_finite() && rtt > 0.0 => {
                let scaled = rtt * Self::RELAY_ROUND_TRIP_MULTIPLIER;
                scaled.clamp(Self::RELAY_MINIMUM, Self::RELAY_MAXIMUM)
            }
            _ => Self::RELAY_MINIMUM,
        }
    }
}

/// How an APNs token reaches one paired Mac.
///
/// Port of `PushTokenRegistrationRoute`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    ///
    /// Port of `PushTokenRegistrationRoute.plan`.
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

    const FINGERPRINT: &str = "9f2c4a1b5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f70819";

    fn advertisement(
        fingerprint: Option<&str>,
        version: Option<&str>,
        capabilities: Option<Vec<&str>>,
    ) -> RemoteDirectTransportAdvertisement {
        RemoteDirectTransportAdvertisement {
            certificate_fingerprint: fingerprint.map(str::to_string),
            server_version: version.map(str::to_string),
            host_capabilities: capabilities.map(|c| c.into_iter().map(str::to_string).collect()),
        }
    }

    // Port of testCapabilityFlagSelectsTLS.
    #[test]
    fn capability_flag_selects_tls() {
        let decision = transport_decision(&advertisement(
            Some(&FINGERPRINT.to_uppercase()),
            None,
            Some(vec!["host.bootstrap", capabilities::HOST_MOBILE_TLS]),
        ));
        assert_eq!(
            decision,
            RemoteDirectTransportDecision::Tls {
                fingerprint: FINGERPRINT.to_string()
            }
        );
    }

    // Port of testServerVersionAtOrAfterMinimumSelectsTLSWithoutCapability.
    #[test]
    fn server_version_at_or_after_minimum_selects_tls_without_capability() {
        for version in [
            "0.5.3",
            "0.5.10",
            "0.6.0",
            "1.0.0",
            "v0.5.3",
            "0.5.3-beta.1",
        ] {
            let decision = transport_decision(&advertisement(
                Some(FINGERPRINT),
                Some(version),
                Some(vec!["host.bootstrap"]),
            ));
            assert_eq!(
                decision,
                RemoteDirectTransportDecision::Tls {
                    fingerprint: FINGERPRINT.to_string()
                },
                "version {version}"
            );
        }
    }

    // Port of testOlderServerVersionSelectsPlaintext.
    #[test]
    fn older_server_version_selects_plaintext() {
        for version in ["0.5.2", "0.4.9", "0.5"] {
            let decision =
                transport_decision(&advertisement(Some(FINGERPRINT), Some(version), None));
            assert_eq!(
                decision,
                RemoteDirectTransportDecision::Plaintext,
                "version {version}"
            );
        }
    }

    // Port of testNoSignalKeepsCurrentTransport.
    #[test]
    fn no_signal_keeps_current_transport() {
        let decision = transport_decision(&advertisement(
            Some(FINGERPRINT),
            None,
            Some(vec!["host.bootstrap"]),
        ));
        assert_eq!(decision, RemoteDirectTransportDecision::Unknown);
        let decision = transport_decision(&advertisement(None, Some("garbage"), None));
        assert_eq!(decision, RemoteDirectTransportDecision::Unknown);
    }

    // Port of testTLSSignalWithoutFingerprintIsUnpinnableAndChangesNothing.
    #[test]
    fn tls_signal_without_fingerprint_is_unpinnable() {
        let decision = transport_decision(&advertisement(Some("  "), Some("0.5.3"), None));
        assert_eq!(decision, RemoteDirectTransportDecision::TlsUnpinnable);
    }

    // Port of testPlaintextRefusalIsRecognizedFrom426AndFromA401ThatNamesHTTPS.
    #[test]
    fn plaintext_refusal_recognized_from_426_and_401_naming_https() {
        assert!(is_plaintext_refusal(426, None));
        assert!(is_plaintext_refusal(426, Some("Upgrade Required")));
        assert!(is_plaintext_refusal(401, Some("use https")));
        assert!(is_plaintext_refusal(
            401,
            Some("bearer tokens are only accepted over TLS")
        ));
        assert!(!is_plaintext_refusal(401, Some("invalid token")));
        assert!(!is_plaintext_refusal(401, None));
        assert!(!is_plaintext_refusal(403, Some("https")));
    }

    // Port of testHTTPSAdvertisedEndpointsAreStoredCanonicallyAsHTTP.
    #[test]
    fn https_endpoints_stored_canonically_as_http() {
        assert_eq!(
            canonical_stored_endpoint("https://192.168.1.10:4485/mobile"),
            "http://192.168.1.10:4485/mobile"
        );
        assert_eq!(
            canonical_stored_endpoint("HTTPS://192.168.1.10:4485/mobile"),
            "http://192.168.1.10:4485/mobile"
        );
        assert_eq!(
            canonical_stored_endpoint("http://192.168.1.10:4485/mobile"),
            "http://192.168.1.10:4485/mobile"
        );
    }

    // Port of testBootstrapDeadlineIsFourSecondsOnTheLANAndTenOverTheRelay.
    #[test]
    fn bootstrap_deadline_four_seconds_lan_ten_relay() {
        assert_eq!(RemoteBootstrapDeadline::seconds(false, None), 4.0);
        assert_eq!(RemoteBootstrapDeadline::seconds(false, Some(3.0)), 4.0);
        assert_eq!(RemoteBootstrapDeadline::seconds(true, None), 10.0);
    }

    // Port of testRelayBootstrapDeadlineScalesWithMeasuredRoundTripWithinBounds.
    #[test]
    fn relay_bootstrap_deadline_scales_with_round_trip_within_bounds() {
        assert_eq!(RemoteBootstrapDeadline::seconds(true, Some(0.3)), 10.0);
        assert_eq!(RemoteBootstrapDeadline::seconds(true, Some(2.5)), 12.5);
        assert_eq!(RemoteBootstrapDeadline::seconds(true, Some(9.0)), 20.0);
        assert_eq!(RemoteBootstrapDeadline::seconds(true, Some(0.0)), 10.0);
        assert_eq!(RemoteBootstrapDeadline::seconds(true, Some(f64::NAN)), 10.0);
    }

    // Port of testActiveMacOnRelayRegistersOverTheLiveConnectionOnly.
    #[test]
    fn active_mac_on_relay_registers_over_live_connection_only() {
        assert_eq!(
            PushTokenRegistrationRoute::plan(true, true, true),
            vec![PushTokenRegistrationRoute::ActiveRelayClient]
        );
    }

    // Port of testDirectMacsTryTheLANThenALinkConnection.
    #[test]
    fn direct_macs_try_lan_then_link_connection() {
        assert_eq!(
            PushTokenRegistrationRoute::plan(true, false, true),
            vec![
                PushTokenRegistrationRoute::Direct,
                PushTokenRegistrationRoute::TransientRelay
            ]
        );
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
    }

    // Port of testServerVersionParsingAndOrdering.
    #[test]
    fn server_version_parsing_and_ordering() {
        assert_eq!(
            RemoteServerVersion::parse(Some("0.5.3")),
            Some(RemoteServerVersion::new(0, 5, 3))
        );
        assert_eq!(
            RemoteServerVersion::parse(Some("0.10.0-beta.2+build.7")),
            Some(RemoteServerVersion::new(0, 10, 0))
        );
        assert_eq!(
            RemoteServerVersion::parse(Some("1")),
            Some(RemoteServerVersion::new(1, 0, 0))
        );
        assert_eq!(RemoteServerVersion::parse(None), None);
        assert_eq!(RemoteServerVersion::parse(Some("")), None);
        assert_eq!(RemoteServerVersion::parse(Some("0.5.x")), None);
        assert_eq!(RemoteServerVersion::parse(Some("0.5.3.1")), None);
        assert!(
            RemoteServerVersion::parse(Some("0.5.10")).unwrap()
                > RemoteServerVersion::parse(Some("0.5.3")).unwrap()
        );
        assert!(
            RemoteServerVersion::parse(Some("0.6.0")).unwrap()
                > RemoteServerVersion::parse(Some("0.5.99")).unwrap()
        );
        assert!(
            RemoteServerVersion::parse(Some("0.5.2")).unwrap()
                < RemoteServerVersion::parse(Some("0.5.3")).unwrap()
        );
    }
}
