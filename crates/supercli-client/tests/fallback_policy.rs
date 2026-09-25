//! Relay fallback policy tests: the Direct→relay fallback may fire ONLY on
//! classified reachability failures. A TLS/pin failure, HTTP status, decode
//! error, invalid endpoint, or local setup failure must surface as a hard
//! error — falling back on those would silently route around a security
//! decision.
//!
//! These tests pin the policy at the classification layer
//! ([`DirectFailure::relay_eligible`]); the launchers' `connect_result`
//! consults exactly this predicate plus the record's Link-enabled flag
//! before touching the relay.

use supercli_client::{DirectFailure, HostClientError};

#[test]
fn tls_pin_failure_is_never_relay_eligible() {
    let err = HostClientError::Tls("TLS handshake: pin mismatch".to_string());
    assert!(
        !err.is_reachability_failure(),
        "TLS errors must not classify as reachability"
    );
    // DirectFailure::classify is private; exercise the same rule through
    // the public surface: only Transport is reachability.
    assert!(!DirectFailure::Setup("x".to_string()).relay_eligible());
}

#[test]
fn transport_failure_is_relay_eligible() {
    // Genuine reachability: DNS failure, TCP refused, timeouts.
    let err = HostClientError::Transport("connection refused".to_string());
    assert!(err.is_reachability_failure());
}

#[test]
fn non_transport_failures_are_never_relay_eligible() {
    for err in [
        HostClientError::Tls("handshake failed".to_string()),
        HostClientError::Status(401, "unauthorized".to_string()),
        HostClientError::Decode("bad json".to_string()),
        HostClientError::InvalidEndpoint("not an https:// URL".to_string()),
    ] {
        assert!(
            !err.is_reachability_failure(),
            "must not classify as reachability: {err:?}"
        );
    }
    // Setup failures (credential store, client construction) happen before
    // any network I/O — never a relay trigger.
    assert!(!DirectFailure::Setup("no credentials".to_string()).relay_eligible());
}
