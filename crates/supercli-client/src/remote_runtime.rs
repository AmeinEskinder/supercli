//! Remote Host connection state machine and Direct→Link fallback.
//!
//! Ported from `clients/legacy/native/SupercliNative/Sources/SupercliNative/RemoteHostRuntime.swift`.
//! Only the portable state types and the connection-plan logic live here:
//! the `@MainActor ObservableObject` UI runtime (session verbs, pane state)
//! stays in the launcher, and the verbs themselves ride
//! [`crate::transport::HostClient`], which is already Rust.

use serde::{Deserialize, Serialize};

/// Connection lifecycle for one Host. Mirrors Swift
/// `RemoteHostConnectionState`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteHostConnectionState {
    Idle,
    Connecting,
    Connected {
        name: String,
    },
    /// The last valid sidebar/terminal remains visible while reconnection runs.
    Reconnecting {
        message: String,
    },
    RepairRequired {
        message: String,
    },
    Incompatible {
        message: String,
    },
    Failed {
        message: String,
    },
}

impl RemoteHostConnectionState {
    /// True while a connection attempt (or re-attempt) is in flight.
    pub fn is_transitional(&self) -> bool {
        matches!(
            self,
            RemoteHostConnectionState::Connecting | RemoteHostConnectionState::Reconnecting { .. }
        )
    }

    /// True once the transport is up.
    pub fn is_connected(&self) -> bool {
        matches!(self, RemoteHostConnectionState::Connected { .. })
    }
}

/// User-facing route for one Host connection. The UI deliberately exposes
/// only the useful distinction (local network or Supercli Link), never relay
/// endpoints, tokens, or a manual transport picker. Mirrors Swift
/// `RemoteHostConnectionRoute`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteHostConnectionRoute {
    Ssh,
    LocalGateway,
    Direct,
    Link,
}

impl RemoteHostConnectionRoute {
    pub fn short_label(&self) -> &'static str {
        match self {
            RemoteHostConnectionRoute::Ssh => "Connected",
            RemoteHostConnectionRoute::LocalGateway => "This Mac",
            RemoteHostConnectionRoute::Direct => "Direct",
            RemoteHostConnectionRoute::Link => "Via Link",
        }
    }
}

/// Which transport to open for a paired host. Mirrors the portable subset of
/// Swift `RemoteHostTransport` (the SSH/local variants carry launcher-side
/// state and stay out of this crate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairedTransport {
    Direct {
        endpoint: String,
        certificate_fingerprint: String,
    },
    Link {
        controller_device_id: String,
    },
}

impl PairedTransport {
    pub fn route(&self) -> RemoteHostConnectionRoute {
        match self {
            PairedTransport::Direct { .. } => RemoteHostConnectionRoute::Direct,
            PairedTransport::Link { .. } => RemoteHostConnectionRoute::Link,
        }
    }
}

/// Direct→Link fallback plan for one paired host. Mirrors Swift
/// `PairedHostConnectionPlan`.
///
/// A Direct-only host (removed from the Link enrollment list) gets no Link
/// transport at all: reachability failures then report Direct-only
/// reachability instead of silently riding the relay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedHostConnectionPlan {
    pub host_id: String,
    pub direct: PairedTransport,
    /// `None` when the user scoped this host to Direct-only.
    pub link: Option<PairedTransport>,
}

/// Failure building a connection plan. Mirrors the Swift
/// `requirePairingRepair` branch of `connectPairedHost`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionPlanError {
    /// No usable certificate pin: legacy records can still use their
    /// authenticated E2E Link, but a Direct-only host must re-pair.
    /// Never send a saved bearer to an unverified LAN endpoint.
    PairingRepairRequired { message: String },
}

impl PairedHostConnectionPlan {
    /// Build the plan for a paired host record.
    ///
    /// - `link_enabled`: `false` scopes the host to Direct-only (no Link
    ///   transport, no grace race, no background probe may open the relay).
    /// - `certificate_fingerprint`: 64-hex-char pin for the Direct endpoint.
    ///   Legacy records without a pin fall back to Link-only; a Direct-only
    ///   host without a pin must re-pair.
    pub fn build(
        host_id: String,
        endpoint: String,
        controller_device_id: String,
        link_enabled: bool,
        certificate_fingerprint: Option<String>,
    ) -> Result<Self, ConnectionPlanError> {
        let link = link_enabled.then_some(PairedTransport::Link {
            controller_device_id,
        });
        let pin = certificate_fingerprint.filter(|p| is_valid_pin(p));
        match (pin, link) {
            (Some(pin), link) => Ok(Self {
                host_id,
                direct: PairedTransport::Direct {
                    endpoint,
                    certificate_fingerprint: pin,
                },
                link,
            }),
            // Legacy records can still use their authenticated E2E Link.
            // Never send the saved bearer to an unverified LAN endpoint:
            // the Direct transport is a placeholder the launcher must not open.
            (None, Some(link_transport)) => Ok(Self {
                host_id,
                direct: PairedTransport::Direct {
                    endpoint,
                    certificate_fingerprint: String::new(),
                },
                link: Some(link_transport),
            }),
            (None, None) => Err(ConnectionPlanError::PairingRepairRequired {
                message: "This Host was paired before Direct connections were certificate-pinned. Pair it again to reach it directly.".to_string(),
            }),
        }
    }

    /// The transport to try first: Direct when pinned, else Link.
    pub fn initial(&self) -> &PairedTransport {
        match &self.direct {
            PairedTransport::Direct {
                certificate_fingerprint,
                ..
            } if !certificate_fingerprint.is_empty() => &self.direct,
            _ => self.link.as_ref().unwrap_or(&self.direct),
        }
    }

    /// The fallback transport after the initial one fails, if any.
    pub fn fallback(&self) -> Option<&PairedTransport> {
        let initial_is_direct = matches!(
            self.initial(),
            PairedTransport::Direct {
                certificate_fingerprint,
                ..
            } if !certificate_fingerprint.is_empty()
        );
        if initial_is_direct {
            self.link.as_ref()
        } else {
            None
        }
    }
}

/// A 64-hex-char certificate pin, as stored on the pairing record.
fn is_valid_pin(pin: &str) -> bool {
    pin.len() == 64 && pin.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_state_variants() {
        assert!(RemoteHostConnectionState::Connecting.is_transitional());
        assert!(RemoteHostConnectionState::Reconnecting {
            message: "x".into()
        }
        .is_transitional());
        assert!(!RemoteHostConnectionState::Idle.is_transitional());
        assert!(RemoteHostConnectionState::Connected { name: "Mac".into() }.is_connected());
        assert!(!RemoteHostConnectionState::Failed {
            message: "x".into()
        }
        .is_connected());
    }

    #[test]
    fn route_short_labels() {
        assert_eq!(RemoteHostConnectionRoute::Ssh.short_label(), "Connected");
        assert_eq!(
            RemoteHostConnectionRoute::LocalGateway.short_label(),
            "This Mac"
        );
        assert_eq!(RemoteHostConnectionRoute::Direct.short_label(), "Direct");
        assert_eq!(RemoteHostConnectionRoute::Link.short_label(), "Via Link");
    }

    #[test]
    fn link_disabled_means_no_fallback() {
        let plan = PairedHostConnectionPlan::build(
            "host-1".to_string(),
            "https://192.168.1.5:443".to_string(),
            "controller-1".to_string(),
            false, // Direct-only
            Some("a".repeat(64)),
        )
        .unwrap();
        assert!(plan.link.is_none());
        assert_eq!(plan.initial().route(), RemoteHostConnectionRoute::Direct);
        assert!(plan.fallback().is_none());
    }

    #[test]
    fn direct_first_then_link_fallback() {
        let plan = PairedHostConnectionPlan::build(
            "host-1".to_string(),
            "https://192.168.1.5:443".to_string(),
            "controller-1".to_string(),
            true,
            Some("b".repeat(64)),
        )
        .unwrap();
        assert_eq!(plan.initial().route(), RemoteHostConnectionRoute::Direct);
        assert_eq!(
            plan.fallback().map(|t| t.route()),
            Some(RemoteHostConnectionRoute::Link)
        );
    }

    #[test]
    fn legacy_record_without_pin_needs_repair_when_direct_only() {
        let err = PairedHostConnectionPlan::build(
            "host-1".to_string(),
            "https://192.168.1.5:443".to_string(),
            "controller-1".to_string(),
            false, // Direct-only, no pin
            None,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ConnectionPlanError::PairingRepairRequired { .. }
        ));
    }

    #[test]
    fn invalid_pin_rejected() {
        assert!(!is_valid_pin("short"));
        assert!(!is_valid_pin(&"z".repeat(64)));
        assert!(is_valid_pin(&"A".repeat(64)));
        assert!(is_valid_pin(&"0123456789abcdef".repeat(4)));
    }
}
