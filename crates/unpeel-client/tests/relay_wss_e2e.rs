//! Relay over TLS (`wss://`): [`RelayConnection::connect_with_tls`] against
//! a mock relay serving a private-CA certificate.
//!
//! Proves the full relay handshake — `https://` → `wss://` URL
//! translation, TLS verification against a custom CA, token subprotocol,
//! transcript-MAC verification, X25519 agreement, sealed
//! request/response — works over an encrypted WebSocket. A companion
//! test proves the default WebPKI roots reject the mock's certificate,
//! so TLS verification is genuinely enforced, not skipped.

use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use base64::Engine;
use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair, SanType};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig, ServerConnection};
use tungstenite::http::{Request, Response};

use unpeel_client::relay::RelayCredentials;
use unpeel_client::{PerformParams, RelayConnection};

mod common;

use common::B64;

/// Private CA + a leaf certificate for 127.0.0.1, generated fresh per run.
fn test_ca_and_leaf() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut ca_params = CertificateParams::new(vec!["test relay ca".to_string()]).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca_key = KeyPair::generate().unwrap();
    let ca_cert = ca_params.self_signed(&ca_key).unwrap();

    let mut leaf_params = CertificateParams::new(vec![]).unwrap();
    leaf_params.subject_alt_names = vec![SanType::IpAddress("127.0.0.1".parse().unwrap())];
    let leaf_key = KeyPair::generate().unwrap();
    let leaf_cert = leaf_params.signed_by(&leaf_key, &ca_cert, &ca_key).unwrap();

    (
        ca_cert.der().to_vec(),
        leaf_cert.der().to_vec(),
        leaf_key.serialize_der(),
    )
}

/// Mock relay uplink: TLS with the test leaf, WebSocket accept with the
/// token-header check, then the shared host-side handshake/serve logic.
// `accept_hdr`'s `Callback` trait fixes the closure's error type to
// `http::Response`, which trips `result_large_err`; the trait gives us
// no smaller option.
#[allow(clippy::result_large_err)]
fn mock_wss_host(
    listener: TcpListener,
    leaf_der: Vec<u8>,
    leaf_key_der: Vec<u8>,
    e2e_key: [u8; 32],
    device_id: &'static str,
    relay_token: &'static str,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let (tcp, _) = listener.accept().expect("client connects");
        let cert = CertificateDer::from(leaf_der);
        let key = PrivateKeyDer::Pkcs8(leaf_key_der.into());
        let config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(rustls::ALL_VERSIONS)
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(vec![cert], key)
                .unwrap();
        let tls = ServerConnection::new(Arc::new(config)).unwrap();
        let tls_stream = rustls::StreamOwned::new(tls, tcp);
        let mut ws =
            tungstenite::accept_hdr(tls_stream, |req: &Request<()>, resp: Response<()>| {
                common::relay_handshake_response(req, resp, relay_token)
            })
            .expect("wss handshake");
        common::serve_relay_host(&mut ws, e2e_key, device_id);
    })
}

fn private_ca_connector(ca_der: &[u8]) -> tungstenite::Connector {
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(ca_der.to_vec()))
        .expect("test CA parses");
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(rustls::ALL_VERSIONS)
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
    tungstenite::Connector::Rustls(Arc::new(config))
}

fn test_creds(port: u16, e2e_key: [u8; 32]) -> RelayCredentials {
    RelayCredentials {
        relay_url: format!("https://127.0.0.1:{port}"),
        mac_id: "mac-1".to_string(),
        relay_token: "test-token".to_string(),
        e2e_key_b64: B64.encode(e2e_key),
    }
}

#[test]
fn relay_wss_handshake_request_response() {
    // Per-test timeout: fail instead of hanging the suite if a
    // socket-teardown regression ever reintroduces the old hang.
    common::run_with_timeout(
        "relay_wss_handshake_request_response",
        Duration::from_secs(30),
        || {
            let (ca_der, leaf_der, leaf_key_der) = test_ca_and_leaf();
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().unwrap().port();
            let e2e_key = [9u8; 32];
            let server = mock_wss_host(
                listener,
                leaf_der,
                leaf_key_der,
                e2e_key,
                "test-device",
                "test-token",
            );

            // The https:// URL must become wss://, the TLS handshake must verify
            // against the private CA, and then the relay handshake runs on top.
            let connector = private_ca_connector(&ca_der);
            let conn = RelayConnection::connect_with_tls(
                test_creds(port, e2e_key),
                "test-device",
                Some(connector),
            )
            .expect("wss connect");

            let response = conn
                .perform(PerformParams {
                    method: "GET",
                    path: "/mobile/bootstrap",
                    query: HashMap::new(),
                    auth: Some("Bearer tok"),
                    content_type: None,
                    body: None,
                    timeout: Duration::from_secs(10),
                })
                .expect("perform over wss");
            assert_eq!(response.status, 200);
            assert_eq!(response.body(), b"ok-1");

            conn.close();
            server.join().expect("mock host finishes");
        },
    );
}

#[test]
fn relay_wss_rejects_untrusted_certificate() {
    let (_ca_der, leaf_der, leaf_key_der) = test_ca_and_leaf();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let e2e_key = [9u8; 32];
    let server = mock_wss_host(
        listener,
        leaf_der,
        leaf_key_der,
        e2e_key,
        "test-device",
        "test-token",
    );

    // Default WebPKI roots must reject the mock's private-CA certificate
    // — TLS verification is enforced, not skipped. (The mock host sees
    // the failed handshake and exits; detach it.)
    let err = match RelayConnection::connect(test_creds(port, e2e_key), "test-device") {
        Ok(_) => panic!("untrusted cert must fail"),
        Err(e) => e,
    };
    let msg = format!("{err:?}");
    assert!(
        msg.contains("failed") || msg.contains("connect"),
        "transport-level failure, got: {msg}"
    );
    drop(server);
}
