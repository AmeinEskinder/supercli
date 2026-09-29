//! Pure decisions behind the phone's Direct (LAN) `/mobile` transport and
//! the relay's bootstrap budget.
//!
//! Ported from
//! `clients/legacy/ios/SupercliIOS/Sources/SupercliIOS/RemoteDirectTransport.swift`.
//!
//! Everything here is socket-free: which scheme a paired Host gets (pinned
//! HTTPS vs legacy plaintext) and how that decision is learned from
//! bootstrap/pairing and folded into the stored pin, the bootstrap deadline
//! per transport, and the push-token registration route per paired Host.
//! The connection store applies these decisions; the HTTP client executes
//! them.
//!
//! Deliberate gap: `RemotePinnedURLSessionCache` (one `URLSession` per
//! certificate fingerprint) is Apple networking machinery — not portable.
//! The Rust equivalent is [`crate::tls::pinned_client_config`], which
//! reuses one pinned `rustls` config per fingerprint.

use crate::protocol::capabilities;
use crate::tls::normalize_fingerprint;

/// Semantic server version (`"0.5.3"`, `"0.10.0-beta.2"`). Pre-release and
/// build suffixes are ignored: `0.5.3-beta.1` is 0.5.3 for gating purposes
/// because the feature ships with the release line, not the tag.
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

    /// Parse a version string. Returns `None` for empty input, non-numeric
    /// parts, negative parts, or more than three dot-separated parts.
    /// Mirrors Swift's failable `RemoteServerVersion(_:)`.
    pub fn parse(raw: Option<&str>) -> Option<Self> {
        let raw = raw?;
        let mut core = raw.trim();
        if let Some(stripped) = core.strip_prefix('v').or_else(|| core.strip_prefix('V')) {
            core = stripped;
        }
        if let Some(cut) = core.find(['-', '+']) {
            core = &core[..cut];
        }
        let parts: Vec<&str> = core.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return None;
        }
        let mut numbers = [0u64; 3];
        for (i, part) in parts.iter().enumerate() {
            numbers[i] = part.parse().ok()?;
        }
        Some(Self::new(numbers[0], numbers[1], numbers[2]))
    }
}

/// What a Host said about its Direct transport, extracted at the wire
/// boundary from a bootstrap snapshot or a sealed pairing response.
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteDirectTransportDecision {
    /// Pinned HTTPS. The bearer only ever rides this.
    Tls { fingerprint: String },
    /// The Host said it serves TLS but advertised no certificate to pin to.
    /// Stay on whatever the record already uses; never send the bearer to
    /// an unpinned TLS endpoint, and never assume plaintext is acceptable.
    TlsUnpinnable,
    /// Legacy plaintext transport. `direct_transport_decision` never produces
    /// this: supercli restarted versioning at 0.1.0 and every Host advertises
    /// `host.mobile.tls`, so a missing capability means `Unknown`, never
    /// plaintext. The variant is retained so `apply_direct_transport_decision`
    /// can still clear a stale pin from an authenticated source.
    Plaintext,
    /// The Host said nothing either way (no `host.mobile.tls` capability).
    /// Keep the record's current transport.
    Unknown,
}

/// Minimal pin record the policy folds decisions into: the pinned Direct
/// `/mobile` TLS fingerprint. Mirrors the `directTLSFingerprint` field of
/// Swift's `PairedMacRecord`; the full record lives in the platform layer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DirectTransportPin {
    pub direct_tls_fingerprint: Option<String>,
}

/// The capability flag is the only signal. Supercli restarted versioning at
/// 0.1.0 and every Host advertises `host.mobile.tls`, so the old
/// version-gated plaintext fallback is gone: capability present means TLS
/// (pinned when a fingerprint was advertised), a missing capability means
/// `Unknown` — never plaintext, no matter what the server version looks
/// like. No signal keeps the current transport.
pub fn direct_transport_decision(
    advertisement: &RemoteDirectTransportAdvertisement,
) -> RemoteDirectTransportDecision {
    let fingerprint = advertisement
        .certificate_fingerprint
        .as_deref()
        .and_then(normalize_fingerprint);
    let tls_or_unpinnable = || match fingerprint.clone() {
        Some(f) => RemoteDirectTransportDecision::Tls { fingerprint: f },
        None => RemoteDirectTransportDecision::TlsUnpinnable,
    };
    if advertisement
        .host_capabilities
        .as_deref()
        .unwrap_or_default()
        .iter()
        .any(|c| c == capabilities::HOST_MOBILE_TLS)
    {
        return tls_or_unpinnable();
    }
    RemoteDirectTransportDecision::Unknown
}

/// Fold a decision into the stored pin. Returns `true` when something
/// changed (`None` in Swift).
///
/// Upgrading to TLS is accepted from any transport: the phone then stops
/// sending the bearer in plaintext from the next request on. Downgrading
/// (clearing TLS) is accepted only from an `authenticated` source — an E2E
/// Relay bootstrap or a pinned-TLS bootstrap — so a plaintext reply from a
/// LAN impostor can never strip the pin from a Host that has one.
pub fn apply_direct_transport_decision(
    decision: &RemoteDirectTransportDecision,
    pin: &mut DirectTransportPin,
    authenticated: bool,
) -> bool {
    match decision {
        RemoteDirectTransportDecision::Tls { fingerprint } => {
            if pin.direct_tls_fingerprint.as_deref() == Some(fingerprint.as_str()) {
                return false;
            }
            pin.direct_tls_fingerprint = Some(fingerprint.clone());
            true
        }
        RemoteDirectTransportDecision::Plaintext => {
            if !authenticated || pin.direct_tls_fingerprint.is_none() {
                return false;
            }
            pin.direct_tls_fingerprint = None;
            true
        }
        RemoteDirectTransportDecision::TlsUnpinnable | RemoteDirectTransportDecision::Unknown => {
            false
        }
    }
}

/// Whether a plaintext `/mobile` reply is the Host refusing the bearer over
/// plaintext (the transition-era `426 Upgrade Required`, or a `401` whose
/// message points at HTTPS/TLS). Any other 4xx keeps its meaning.
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
/// endpoint-equality checks in the generation guards stable across a Host
/// that starts advertising `https://`.
pub fn canonical_stored_endpoint(endpoint: &str) -> String {
    const PREFIX: &str = "https://";
    if endpoint.len() >= PREFIX.len() && endpoint[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        format!("http://{}", &endpoint[PREFIX.len()..])
    } else {
        endpoint.to_string()
    }
}

/// Bootstrap is the connection health signal, so its deadline is short on
/// the LAN. Over the relay a bootstrap crosses the tunnel twice plus the
/// Host's own work; on cellular that legitimately misses 4 s, and each miss
/// used to be read as "connection lost". Budget it from the measured path.
pub mod bootstrap_deadline {
    /// LAN bootstrap deadline, seconds.
    pub const DIRECT_SECS: f64 = 4.0;
    /// Relay bootstrap deadline floor, seconds.
    pub const RELAY_MINIMUM_SECS: f64 = 10.0;
    /// Relay bootstrap deadline ceiling, seconds.
    pub const RELAY_MAXIMUM_SECS: f64 = 20.0;
    /// Multiplier on the last measured relay round-trip. A bootstrap is one
    /// request; giving it several RTTs of headroom absorbs jitter without
    /// letting a genuinely dead path linger past the keepalive limit.
    pub const RELAY_ROUND_TRIP_MULTIPLIER: f64 = 5.0;

    /// Bootstrap deadline in seconds for the measured path.
    pub fn seconds(is_relay: bool, measured_round_trip_secs: Option<f64>) -> f64 {
        if !is_relay {
            return DIRECT_SECS;
        }
        match measured_round_trip_secs {
            Some(rtt) if rtt.is_finite() && rtt > 0.0 => {
                (rtt * RELAY_ROUND_TRIP_MULTIPLIER).clamp(RELAY_MINIMUM_SECS, RELAY_MAXIMUM_SECS)
            }
            _ => RELAY_MINIMUM_SECS,
        }
    }
}

/// How an APNs token reaches one paired Host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushTokenRegistrationRoute {
    /// POST over the paired Direct endpoint (pinned HTTPS or legacy HTTP).
    Direct,
    /// POST through the connection store's live Link connection for the
    /// active Host — no second socket, no LAN wait.
    ActiveRelayClient,
    /// POST through a short-lived Link connection built for this Host.
    TransientRelay,
}

impl PushTokenRegistrationRoute {
    /// Order of attempts for one Host. The active Host already on the relay
    /// skips the LAN entirely: that attempt is known to fail and used to
    /// hold the registration for the full 10 s POST timeout before opening
    /// a throwaway relay socket next to the live one.
    pub fn plan(is_active_host: bool, using_relay: bool, has_relay_credentials: bool) -> Vec<Self> {
        if is_active_host && using_relay {
            return vec![Self::ActiveRelayClient];
        }
        let mut routes = vec![Self::Direct];
        if has_relay_credentials {
            routes.push(Self::TransientRelay);
        }
        routes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::capabilities;

    fn fingerprint() -> String {
        format!("ab{}", "cd".repeat(31))
    }

    fn advertisement(
        certificate_fingerprint: Option<&str>,
        server_version: Option<&str>,
        host_capabilities: Option<Vec<&str>>,
    ) -> RemoteDirectTransportAdvertisement {
        RemoteDirectTransportAdvertisement::new(
            certificate_fingerprint.map(str::to_string),
            server_version.map(str::to_string),
            host_capabilities.map(|caps| caps.into_iter().map(str::to_string).collect()),
        )
    }

    // MARK: - Server version parsing

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

    // MARK: - Transport decision

    #[test]
    fn capability_flag_selects_tls() {
        let decision = direct_transport_decision(&advertisement(
            Some(&fingerprint().to_uppercase()),
            None,
            Some(vec!["host.bootstrap", capabilities::HOST_MOBILE_TLS]),
        ));
        assert_eq!(
            decision,
            RemoteDirectTransportDecision::Tls {
                fingerprint: fingerprint()
            }
        );
    }

    #[test]
    fn server_version_without_capability_selects_unknown_never_plaintext() {
        // The version-gated plaintext fallback is gone: without the
        // host.mobile.tls capability the decision is Unknown no matter what
        // the server version looks like (old, new, or garbage).
        for version in [
            "0.5.2",
            "0.4.9",
            "0.5",
            "0.5.3",
            "0.5.10",
            "0.6.0",
            "1.0.0",
            "v0.5.3",
            "0.5.3-beta.1",
            "garbage",
        ] {
            let decision = direct_transport_decision(&advertisement(
                Some(&fingerprint()),
                Some(version),
                Some(vec!["host.bootstrap"]),
            ));
            assert_eq!(
                decision,
                RemoteDirectTransportDecision::Unknown,
                "version {version}"
            );
        }
    }

    #[test]
    fn no_signal_keeps_current_transport() {
        let decision = direct_transport_decision(&advertisement(
            Some(&fingerprint()),
            None,
            Some(vec!["host.bootstrap"]),
        ));
        assert_eq!(decision, RemoteDirectTransportDecision::Unknown);
        assert_eq!(
            direct_transport_decision(&advertisement(None, Some("garbage"), None)),
            RemoteDirectTransportDecision::Unknown
        );
    }

    #[test]
    fn tls_signal_without_fingerprint_is_unpinnable_and_changes_nothing() {
        let decision = direct_transport_decision(&advertisement(
            Some("  "),
            Some("0.5.3"),
            Some(vec![capabilities::HOST_MOBILE_TLS]),
        ));
        assert_eq!(decision, RemoteDirectTransportDecision::TlsUnpinnable);
        let mut pin = DirectTransportPin::default();
        assert!(!apply_direct_transport_decision(&decision, &mut pin, true));
        let mut pinned = DirectTransportPin {
            direct_tls_fingerprint: Some(fingerprint()),
        };
        assert!(!apply_direct_transport_decision(
            &decision,
            &mut pinned,
            true
        ));
        assert_eq!(pinned.direct_tls_fingerprint, Some(fingerprint()));
    }

    #[test]
    fn upgrade_to_tls_is_accepted_from_plaintext_bootstrap() {
        let mut pin = DirectTransportPin::default();
        assert!(apply_direct_transport_decision(
            &RemoteDirectTransportDecision::Tls {
                fingerprint: fingerprint()
            },
            &mut pin,
            false,
        ));
        assert_eq!(pin.direct_tls_fingerprint, Some(fingerprint()));
        // Re-applying the same pin is a no-op.
        assert!(!apply_direct_transport_decision(
            &RemoteDirectTransportDecision::Tls {
                fingerprint: fingerprint()
            },
            &mut pin,
            false,
        ));
    }

    #[test]
    fn downgrade_to_plaintext_requires_an_authenticated_source() {
        let mut pinned = DirectTransportPin {
            direct_tls_fingerprint: Some(fingerprint()),
        };
        // A plaintext LAN reply must never strip the pin.
        assert!(!apply_direct_transport_decision(
            &RemoteDirectTransportDecision::Plaintext,
            &mut pinned,
            false,
        ));
        assert_eq!(pinned.direct_tls_fingerprint, Some(fingerprint()));
        assert!(apply_direct_transport_decision(
            &RemoteDirectTransportDecision::Plaintext,
            &mut pinned,
            true,
        ));
        assert_eq!(pinned.direct_tls_fingerprint, None);
        // Already plaintext.
        let mut plain = DirectTransportPin::default();
        assert!(!apply_direct_transport_decision(
            &RemoteDirectTransportDecision::Plaintext,
            &mut plain,
            true,
        ));
    }

    // MARK: - Plaintext refusal

    #[test]
    fn plaintext_refusal_is_recognized_from_426_and_from_a_401_that_names_https() {
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
        assert!(!is_plaintext_refusal(500, None));
    }

    // MARK: - Endpoint canonicalization

    #[test]
    fn https_advertised_endpoints_are_stored_canonically_as_http() {
        assert_eq!(
            canonical_stored_endpoint("https://192.168.1.10:4485/mobile"),
            "http://192.168.1.10:4485/mobile"
        );
        assert_eq!(
            canonical_stored_endpoint("http://192.168.1.10:4485/mobile"),
            "http://192.168.1.10:4485/mobile"
        );
    }

    // MARK: - Bootstrap deadline

    #[test]
    fn bootstrap_deadline_is_four_seconds_on_the_lan_and_ten_over_the_relay() {
        use bootstrap_deadline::seconds;
        assert_eq!(seconds(false, None), 4.0);
        assert_eq!(seconds(false, Some(3.0)), 4.0);
        assert_eq!(seconds(true, None), 10.0);
    }

    #[test]
    fn relay_bootstrap_deadline_scales_with_measured_round_trip_within_bounds() {
        use bootstrap_deadline::seconds;
        assert_eq!(seconds(true, Some(0.3)), 10.0);
        assert_eq!(seconds(true, Some(2.5)), 12.5);
        assert_eq!(seconds(true, Some(9.0)), 20.0);
        assert_eq!(seconds(true, Some(0.0)), 10.0);
        assert_eq!(seconds(true, Some(f64::NAN)), 10.0);
    }

    // MARK: - Push token routing

    #[test]
    fn active_host_on_relay_registers_over_the_live_connection_only() {
        assert_eq!(
            PushTokenRegistrationRoute::plan(true, true, true),
            vec![PushTokenRegistrationRoute::ActiveRelayClient]
        );
    }

    #[test]
    fn direct_hosts_try_the_lan_then_a_link_connection() {
        use PushTokenRegistrationRoute::{Direct, TransientRelay};
        assert_eq!(
            PushTokenRegistrationRoute::plan(true, false, true),
            vec![Direct, TransientRelay]
        );
        assert_eq!(
            PushTokenRegistrationRoute::plan(false, true, true),
            vec![Direct, TransientRelay],
            "the active Host's relay state says nothing about another Host"
        );
        assert_eq!(
            PushTokenRegistrationRoute::plan(false, false, false),
            vec![Direct]
        );
    }
}
