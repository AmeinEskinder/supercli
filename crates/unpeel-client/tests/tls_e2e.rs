//! End-to-end pinned-TLS test: a mock HTTPS Host serves a self-signed
//! certificate; the client only connects when the leaf SHA-256 fingerprint
//! matches the pinned value. A wrong fingerprint fails the handshake —
//! there is no silent fallback.
//!
//! The test certificate is generated at runtime with rcgen (not embedded),
//! so no private key material lives in the repository.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection};
use sha2::{Digest, Sha256};
use unpeel_client::{HostClient, HostClientError};

/// Generate a self-signed test certificate for 127.0.0.1 at runtime.
/// Returns (cert_der, key_der, fingerprint_hex).
fn generate_test_cert() -> (Vec<u8>, Vec<u8>, String) {
    let certified =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).expect("generate cert");
    let cert_der = certified.cert.der().to_vec();
    let key_der = certified.key_pair.serialize_der();
    let fingerprint = Sha256::digest(&cert_der)
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    (cert_der, key_der, fingerprint)
}

/// A mock HTTPS Host: TLS with the test cert, one JSON response, and an
/// assertion that the auth token arrived (so we prove the pinned channel
/// carried the credential).
fn mock_https_host(
    listener: TcpListener,
    cert_der: Vec<u8>,
    key_der: Vec<u8>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let (tcp, _) = listener.accept().expect("client connects");
        let cert = CertificateDer::from(cert_der);
        let key = PrivateKeyDer::Pkcs8(key_der.into());
        let config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(rustls::ALL_VERSIONS)
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(vec![cert], key)
                .unwrap();
        let tls = ServerConnection::new(Arc::new(config)).unwrap();
        let mut stream = rustls::StreamOwned::new(tls, tcp);

        // Read the HTTP head.
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            let n = stream.read(&mut byte).expect("request head");
            assert!(n > 0, "client sends a request");
            head.push(byte[0]);
        }
        let head_text = String::from_utf8(head).expect("head utf8");
        assert!(
            head_text.starts_with("GET /mobile/output?"),
            "output path, got: {}",
            head_text.lines().next().unwrap_or("")
        );
        assert!(
            head_text.contains("Authorization: Bearer test-token"),
            "auth token sent over the pinned channel"
        );

        let body = br#"{"sessionID":"session-1","offset":0,"nextOffset":4,"dataBase64":"aGVsbA==","truncated":false}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(response.as_bytes()).expect("status");
        stream.write_all(body).expect("body");
        stream.flush().expect("flush");
        // Clean TLS close so the client's read terminates.
        stream.conn.send_close_notify();
        stream.flush().ok();
    })
}

#[test]
fn pinned_tls_accepts_matching_fingerprint() {
    let (cert_der, key_der, fingerprint) = generate_test_cert();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let server = mock_https_host(listener, cert_der, key_der);

    let client = HostClient::with_pinned_tls(
        format!("https://127.0.0.1:{port}/mobile"),
        "test-token",
        &fingerprint,
    )
    .expect("client builds");
    // `output_chunk` exercises the GET path through the pinned transport.
    let chunk = client
        .output_chunk("session-1", None, 10, 0)
        .expect("pinned GET");
    assert_eq!(chunk.data, b"hell");
    assert_eq!(chunk.next_offset, 4);
    assert!(!chunk.truncated);
    server.join().expect("mock host finishes");
}

#[test]
fn pinned_tls_rejects_wrong_fingerprint() {
    let (cert_der, key_der, _fingerprint) = generate_test_cert();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let server = mock_https_host(listener, cert_der, key_der);

    let wrong = "00".repeat(32);
    let client = HostClient::with_pinned_tls(
        format!("https://127.0.0.1:{port}/mobile"),
        "test-token",
        &wrong,
    )
    .expect("client builds");
    let err = client.output_chunk("session-1", None, 10, 0).unwrap_err();
    match err {
        HostClientError::Tls(_) => {}
        other => panic!("expected a TLS pin failure, got: {other}"),
    }
    // A pin failure is a security boundary, not reachability: the relay
    // fallback policy must never treat it as a relay trigger.
    assert!(
        !err.is_reachability_failure(),
        "pin failure must not classify as reachability"
    );
    // The server thread may have errored on the failed handshake; join to
    // avoid a detached panic killing the test binary.
    let _ = server.join();
}

#[test]
fn pinned_tls_rejects_malformed_fingerprint() {
    let err =
        HostClient::with_pinned_tls("https://127.0.0.1:1/mobile", "t", "not-hex").unwrap_err();
    assert!(matches!(err, HostClientError::InvalidEndpoint(_)));
}

#[test]
fn plaintext_client_refuses_https_urls() {
    let err =
        HostClient::with_pinned_tls("http://127.0.0.1:1/mobile", "t", "00".repeat(32).as_str())
            .unwrap_err();
    assert!(matches!(err, HostClientError::InvalidEndpoint(_)));
}
