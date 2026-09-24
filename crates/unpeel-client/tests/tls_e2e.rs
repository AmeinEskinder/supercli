//! End-to-end pinned-TLS test: a mock HTTPS Host serves a self-signed
//! certificate; the client only connects when the leaf SHA-256 fingerprint
//! matches the pinned value. A wrong fingerprint fails the handshake —
//! there is no silent fallback.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;

use base64::Engine;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection};
use unpeel_client::{HostClient, HostClientError};

// Self-signed test certificate for CN=127.0.0.1 (SAN IP:127.0.0.1),
// valid 2026-09-19..2026-09-21. Leaf SHA-256:
// 62b048e38726d0dc6f22bd639be9310aa44651be9b726b924250baba46e558de
const TEST_CERT_PEM: &str = "\
-----BEGIN CERTIFICATE-----\n\
MIIDGjCCAgKgAwIBAgIUW8w7iZwyFnIxww0+/IXD3K+bv7kwDQYJKoZIhvcNAQEL\n\
BQAwFDESMBAGA1UEAwwJMTI3LjAuMC4xMB4XDTI2MDkxOTA0NTQ0M1oXDTI2MDky\n\
MTA0NTQ0M1owFDESMBAGA1UEAwwJMTI3LjAuMC4xMIIBIjANBgkqhkiG9w0BAQEF\n\
AAOCAQ8AMIIBCgKCAQEAttLcDFHLiL8Hjpidjawa02qhYALIfMmOHNSc/125RCDI\n\
jptGMY0xv8k4gENAB58XC/V5DLXHUgPfiEuvhMEFJ0b+FszADnABIDn+O/tcK1hO\n\
hi13IrRfbwO+t7L841gmfSzAu+kDk/xabbGL8O2VhiODShTFT3XwSuhkNvrXeXhp\n\
jld4tqAeDulQG48XEVQkKY2DuLwVjBUHEDG/yCHv4FQ09vxpFuNUs+/ZW5blKXkx\n\
A2xxx8H70NjDpn6iUxtx/A5XCmUV4Mt0EjhnJci4xHvLaRvJ4UEvFpQAhqMvi/1q\n\
ZoIzHTl4vRXK/2MX8CrLCp3tvvYP7wMlV7JObw4kNwIDAQABo2QwYjAdBgNVHQ4E\n\
FgQUz39Qa0gAShsgW4Aw/r1t3/KlVLswHwYDVR0jBBgwFoAUz39Qa0gAShsgW4Aw\n\
/r1t3/KlVLswDwYDVR0TAQH/BAUwAwEB/zAPBgNVHREECDAGhwR/AAABMA0GCSqG\n\
SIb3DQEBCwUAA4IBAQCivqa83+UzqUnDbjiLXsOyJooQatIk8VOWQQ5OpCHL+PWs\n\
Lyg/YDCL9wBB2kT0OVSnDlihKHGdsXzZm2dbQKHjpan0/Sf7hHm6bW1/aU4GDI/2\n\
RpR1JCZ/SWKZ7oyFRnhMvBg4QFkp5SWrrRTC91DuHX5Bl6CPMBIo3r4jWLimdNP5\n\
ceuGKDP3ZiOmblc/3ctJVH+pYqUFii8YNhKEuFlJJvOm+oVComCJ6HN8+ciDwPT0\n\
ICc3SBRxfc7M+aLXNR3aLnno4+v7otRQaAqerok2/rljymjsSVXoP1y+rq6JqzHw\n\
cfq+S659VxZcA1mQozVjzIy5nqT90+Yyhsc4NNMl\n\
-----END CERTIFICATE-----\n";

const TEST_KEY_PEM: &str = "\
-----BEGIN PRIVATE KEY-----\n\
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC20twMUcuIvweO\n\
mJ2NrBrTaqFgAsh8yY4c1Jz/XblEIMiOm0YxjTG/yTiAQ0AHnxcL9XkMtcdSA9+I\n\
S6+EwQUnRv4WzMAOcAEgOf47+1wrWE6GLXcitF9vA763svzjWCZ9LMC76QOT/Fpt\n\
sYvw7ZWGI4NKFMVPdfBK6GQ2+td5eGmOV3i2oB4O6VAbjxcRVCQpjYO4vBWMFQcQ\n\
Mb/IIe/gVDT2/GkW41Sz79lbluUpeTEDbHHHwfvQ2MOmfqJTG3H8DlcKZRXgy3QS\n\
OGclyLjEe8tpG8nhQS8WlACGoy+L/WpmgjMdOXi9Fcr/YxfwKssKne2+9g/vAyVX\n\
sk5vDiQ3AgMBAAECggEABZRuU0RUnvBqbaEdmRftaN7Y3NJMH8R5cH3SUNghpTR1\n\
V1mvDPVPO5g+zJz0B+3q4sDAmHFP2Dv2bY8WmBZzO51zq2rfJ1GmmliXkgMVvA9U\n\
58ypby2FubpDVURcNyAXBKKboa1dIGSHHTcnAaWh3T2ZbmbJFg5c1Ok+bcMk7zfl\n\
RlM8pykYFXcuO9W3zy3hmVPs4fJ1JrYZL2msT4RhmAGog0iHpHrfd2a/0JBQZgkQ\n\
hsW2snWfuI1Ub3p/DCZ5FN9f17UoqpVuuXNiqlmcO6fSj+jObdzf3lOzQ7DuoMjq\n\
7zHsR0onHXbHA3SjqSeCA9v46qRVtQ+dErgLiQkCiQKBgQD3Ex85hd3PGNdnzdR7\n\
GBfMhmCCYGeHFjipVqtRKEo50fFVwI/zAniWMvL3VITtRNBfkQ4IvKrDYOoFfJtF\n\
qNvKG8kMUpZSItu1ld//cd54aXlKUAPFb2zzD6dCEugCbTbE1k/m8cEuwM8Goxua\n\
7wxxJfOlttdaAZ37jzqq3FEXBQKBgQC9bY/eoyKZr3A4puqmZ6bHfz8nCnHnd3eT\n\
nmJmlon7rsx4wOZ+Zc1TppAXfHgbYLJR1rsxMXIelj7WvHE09RsSrPDOOiZPqja8\n\
z60xzYdErEzWRPjL3Y4u3qn8p6s2utm6J5+j8RqM8jr7d6/2T7t43s4R1xGV7lo3\n\
SBS/xXQ7CwKBgQCYcpcAkiWTI9u889ZYATyl+H/R0hPu2PorGgvornhmBrDm3UK0\n\
iWDIJDWf/+lo0N3VKZZEM80VclXS/th7eb3rjYtWbBrOIS35c7lbTvIaz9GD1a0O\n\
BDtwGcd00F+RJ7v0Zdu1PpOY+mSCzt84kKjXo7gbFp73rSnCTtS/GeN61QKBgQCX\n\
u/tSSzqcAHvT5WBwivZ9NuBPVX6H+po+FNCdfFhq0knXMC1xcfpW10T0iy9qPrqX\n\
83lenkzbU15ig+/qi6tz+jOp/cpSZfYSqAgDrK59w8aInBbutjn+MT3YtDXhUSq2\ngqbNm4O5Aw9b/zNkCdoRp+dJQw1DG/oH7iUwt+myoQKBgGZ7eKBTfrLO3zORiKQb\n\
0xXigwXCnDf/DbLTGIcW9Zz+srCsZxcdctKWGo3C2zJRgkO2jMcWVNRnU8rLyBkK\n\
QxRCE5U3VUTNn+84Kcu0OhSO9VAWj+MZYkr/S/tUgnXpgFTLWp/Dsw5Azp0fl8u8\n\
jw5DpITU1cRu7hPFmfXfwRWB\n\
-----END PRIVATE KEY-----\n";

const PINNED_FINGERPRINT: &str = "62b048e38726d0dc6f22bd639be9310aa44651be9b726b924250baba46e558de";

fn pem_der(pem: &str) -> Vec<u8> {
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    base64::engine::general_purpose::STANDARD
        .decode(body)
        .expect("valid PEM")
}

/// A mock HTTPS Host: TLS with the test cert, one JSON response, and an
/// assertion that the auth token arrived (so we prove the pinned channel
/// carried the credential).
fn mock_https_host(listener: TcpListener) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let (tcp, _) = listener.accept().expect("client connects");
        let cert = CertificateDer::from(pem_der(TEST_CERT_PEM));
        let key = PrivateKeyDer::Pkcs8(pem_der(TEST_KEY_PEM).into());
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
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let server = mock_https_host(listener);

    let client = HostClient::with_pinned_tls(
        format!("https://127.0.0.1:{port}/mobile"),
        "test-token",
        PINNED_FINGERPRINT,
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
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let server = mock_https_host(listener);

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
    let err = HostClient::with_pinned_tls("http://127.0.0.1:1/mobile", "t", PINNED_FINGERPRINT)
        .unwrap_err();
    assert!(matches!(err, HostClientError::InvalidEndpoint(_)));
}
