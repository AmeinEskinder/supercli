//! End-to-end relay-transport test: [`HostClient::via_relay`] against a
//! mock relay uplink that speaks the Swift `RelayProtocol` handshake and
//! then serves tunneled `/mobile/*` requests like a real Host uplink.
//!
//! Proves the whole fallback data path — sealed E2E tunnel, URL →
//! (path, query) splitting, bearer auth inside the tunnel, JSON body
//! tunneling, and tunneled status/body mapping — without touching the
//! Direct transports at all.

use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use base64::Engine;
use tungstenite::http::{Request, Response};
use tungstenite::Message;

use unpeel_client::crypto::handshake::transcript_mac;
use unpeel_client::crypto::RelayCryptoSession;
use unpeel_client::relay::{
    EphemeralKeyPair, RelayClientHello, RelayCredentials, RelayHostHello, RelayTunnelRequest,
    RelayTunnelResponse, RELAY_PROTOCOL_VERSION,
};
use unpeel_client::{HostClient, RelayConnection, TransportKind};

mod common;

use common::B64;

/// Mock Host uplink (plaintext `ws://`): token-header check, relay
/// handshake, then serve tunneled requests with strict assertions on what
/// the client tunneled.
#[allow(clippy::result_large_err)]
fn mock_tunnel_host(
    listener: TcpListener,
    e2e_key: [u8; 32],
    device_id: &'static str,
    relay_token: &'static str,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("client connects");
        let mut ws = tungstenite::accept_hdr(stream, |req: &Request<()>, resp: Response<()>| {
            common::relay_handshake_response(req, resp, relay_token)
        })
        .expect("handshake");

        // Host side of the relay handshake (mirrors common::serve_relay_host
        // so this test controls the request assertions itself).
        let client_hello: RelayClientHello = match ws.read().expect("hello") {
            Message::Binary(b) => serde_json::from_slice(&b).expect("client hello JSON"),
            other => panic!("expected binary hello, got {other:?}"),
        };
        assert_eq!(client_hello.v, RELAY_PROTOCOL_VERSION);
        assert_eq!(client_hello.device_id, device_id);
        let client_salt: [u8; 16] = B64
            .decode(&client_hello.salt_b64)
            .unwrap()
            .try_into()
            .unwrap();
        let client_ephemeral = B64.decode(&client_hello.ephemeral_public_key_b64).unwrap();
        let mut host_salt = [0u8; 16];
        getrandom::getrandom(&mut host_salt).unwrap();
        let host_ephemeral = EphemeralKeyPair::generate();
        let mac = transcript_mac(
            &e2e_key,
            device_id,
            &client_salt,
            &host_salt,
            &client_ephemeral,
            &host_ephemeral.public,
        );
        let host_hello = RelayHostHello {
            v: RELAY_PROTOCOL_VERSION,
            salt_b64: B64.encode(host_salt),
            ephemeral_public_key_b64: B64.encode(host_ephemeral.public),
            mac_b64: B64.encode(mac),
        };
        ws.send(Message::Binary(
            serde_json::to_vec(&host_hello).unwrap().into(),
        ))
        .expect("host hello send");
        let shared_secret = host_ephemeral
            .shared_secret(&client_ephemeral)
            .expect("key agreement");
        let mut crypto =
            RelayCryptoSession::new(&e2e_key, &shared_secret, &client_salt, &host_salt, true)
                .expect("host session");

        // Serve requests until the client goes away. Each tunneled request
        // is asserted, then answered according to its path.
        loop {
            let plaintext = match ws.read() {
                Ok(Message::Binary(b)) => crypto.open(&b).expect("open request"),
                Ok(_) => continue,
                Err(_) => return,
            };
            let request: RelayTunnelRequest =
                serde_json::from_slice(&plaintext).expect("request JSON");
            // The bearer auth rides INSIDE the sealed tunnel.
            assert_eq!(request.auth.as_deref(), Some("Bearer test-auth"));
            let body_json: Option<serde_json::Value> = request
                .body_b64
                .as_deref()
                .map(|b| serde_json::from_slice(&B64.decode(b).unwrap()).unwrap());
            let (status, body) = match (request.method.as_str(), request.path.as_str()) {
                ("GET", "/mobile/bootstrap") => {
                    assert!(request.query.is_empty());
                    assert_eq!(request.content_type, None);
                    (200, serde_json::json!({"ok": true}).to_string())
                }
                ("GET", "/mobile/output") => {
                    // Query pairs arrive split and decoded.
                    assert_eq!(
                        request.query.get("session_id").map(String::as_str),
                        Some("sess-1")
                    );
                    assert_eq!(request.query.get("limit").map(String::as_str), Some("100"));
                    (
                        200,
                        serde_json::json!({
                            "offset": 0,
                            "nextOffset": 5,
                            "dataBase64": B64.encode("hello"),
                            "truncated": false,
                        })
                        .to_string(),
                    )
                }
                ("POST", "/mobile/write") => {
                    assert_eq!(request.content_type.as_deref(), Some("application/json"));
                    assert_eq!(
                        body_json
                            .as_ref()
                            .and_then(|b| b.get("data"))
                            .and_then(|d| d.as_str()),
                        Some("ls\n")
                    );
                    (200, serde_json::json!({"ok": true}).to_string())
                }
                (method, path) => panic!("unexpected tunneled request {method} {path}"),
            };
            let response = RelayTunnelResponse {
                id: request.id,
                status,
                body_b64: Some(B64.encode(body)),
            };
            let sealed = crypto
                .seal(&serde_json::to_vec(&response).unwrap())
                .expect("seal response");
            ws.send(Message::Binary(sealed.into()))
                .expect("send response");
        }
    })
}

fn relay_client(port: u16) -> HostClient {
    let e2e_key = [7u8; 32];
    let creds = RelayCredentials {
        relay_url: format!("http://127.0.0.1:{port}"),
        mac_id: "mac-1".to_string(),
        relay_token: "test-token".to_string(),
        e2e_key_b64: B64.encode(e2e_key),
    };
    let conn = RelayConnection::connect(creds, "test-device").expect("connect");
    let client = HostClient::via_relay(conn, "test-auth");
    assert_eq!(client.transport_kind(), TransportKind::Relay);
    client
}

#[test]
fn relay_transport_tunnels_bootstrap_get_and_write_post() {
    // Per-test timeout: if a socket-teardown regression ever reintroduces
    // the hang this suite once had, fail instead of hanging the suite.
    common::run_with_timeout(
        "relay_transport_tunnels_bootstrap_get_and_write_post",
        Duration::from_secs(30),
        || {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().unwrap().port();
            let server = mock_tunnel_host(listener, [7u8; 32], "test-device", "test-token");
            let client = relay_client(port);

            // GET with no query: bootstrap shape passes through.
            let text = client
                .bootstrap()
                .expect("tunneled bootstrap")
                .sessions
                .is_empty();
            assert!(text);

            // GET with query pairs: output_chunk's session_id/limit survive the
            // tunnel split.
            let chunk = client
                .output_chunk("sess-1", None, 100, 0)
                .expect("tunneled output");
            assert_eq!(chunk.data, b"hello");
            assert_eq!(chunk.next_offset, 5);

            // POST with a JSON body: content type + body survive.
            client.write("sess-1", "ls\n").expect("tunneled write");

            drop(client);
            server.join().expect("server thread");
        },
    );
}

/// A relay client reports its kind so the UI can show "Via Link".
#[test]
fn relay_client_reports_transport_kind() {
    common::run_with_timeout(
        "relay_client_reports_transport_kind",
        Duration::from_secs(30),
        || {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().unwrap().port();
            let server = mock_tunnel_host(listener, [7u8; 32], "test-device", "test-token");
            let client = relay_client(port);
            assert_eq!(client.transport_kind(), TransportKind::Relay);
            assert_eq!(TransportKind::Relay.to_string(), "Via Link");
            assert_eq!(TransportKind::Direct.to_string(), "Direct");
            drop(client);
            server.join().expect("server thread");
        },
    );
}

/// `relay_credentials_for_host` rebuilds relay credentials from stored
/// secrets, and refuses when the relay URL is missing or not `wss://`.
#[test]
fn relay_credentials_rebuilt_from_stored_secrets() {
    use unpeel_client::{relay_credentials_for_host, HostSecrets, PairedHostRecord};

    let record = PairedHostRecord {
        host_id: "mac-9".to_string(),
        name: "Mac".to_string(),
        endpoint: "http://10.0.0.2:8321/mobile".to_string(),
        controller_device_id: "dev".to_string(),
        paired_at_unix_ms: 0,
        certificate_fingerprint: None,
        remote_server_port: None,
        remote_server_certificate_fingerprint: None,
        link_enabled: None,
    };
    let secrets = HostSecrets {
        auth_token: "auth".to_string(),
        relay_token: "relay-tok".to_string(),
        e2e_key_b64: B64.encode([9u8; 32]),
        relay_url: Some("wss://relay.example.com".to_string()),
    };
    let creds = relay_credentials_for_host(&record, &secrets).expect("credentials");
    assert_eq!(creds.relay_url, "wss://relay.example.com");
    assert_eq!(creds.mac_id, "mac-9");
    assert_eq!(creds.relay_token, "relay-tok");

    // No stored URL (paired before the fallback existed): no relay.
    let mut old = secrets.clone();
    old.relay_url = None;
    assert!(relay_credentials_for_host(&record, &old).is_none());

    // Non-wss URL: refused.
    let mut bad = secrets.clone();
    bad.relay_url = Some("ws://relay.example.com".to_string());
    assert!(relay_credentials_for_host(&record, &bad).is_none());

    // Bad E2E key: refused.
    let mut bad_key = secrets;
    bad_key.e2e_key_b64 = "not-base64!!".to_string();
    assert!(relay_credentials_for_host(&record, &bad_key).is_none());
}
