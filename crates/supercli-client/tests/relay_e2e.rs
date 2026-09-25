//! End-to-end relay test: `RelayConnection` against a mock Host that
//! speaks the Swift `RelayProtocol` handshake from the other side.
//!
//! Exercises the real wire path — WS connect with the token subprotocol
//! header, plaintext hello exchange, transcript-MAC verification, X25519
//! key agreement, sealed request/response correlation, generation-bound
//! fail-closed requests, and push-frame demultiplexing.

use std::collections::HashMap;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use base64::Engine;
use tungstenite::http::{Request, Response};
use supercli_client::relay::{RelayCredentials, RelayTunnelRequest};
use supercli_client::{PerformParams, RelayConnection, RelayError};

mod common;

use common::B64;

/// Mock Host uplink (plaintext `ws://`): verifies the token header, then
/// runs the shared host-side handshake/serve logic.
// `accept_hdr`'s `Callback` trait fixes the closure's error type to
// `http::Response`, which trips `result_large_err`; the trait gives us
// no smaller option.
#[allow(clippy::result_large_err)]
fn mock_host(
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
        common::serve_relay_host(&mut ws, e2e_key, device_id);
    })
}

#[test]
fn relay_e2e_handshake_request_response_and_push() {
    // Per-test timeout: fail instead of hanging the suite if a
    // socket-teardown regression ever reintroduces the old hang.
    common::run_with_timeout(
        "relay_e2e_handshake_request_response_and_push",
        Duration::from_secs(30),
        || {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().unwrap().port();
            let e2e_key = [7u8; 32];
            let server = mock_host(listener, e2e_key, "test-device", "test-token");

            let creds = RelayCredentials {
                relay_url: format!("http://127.0.0.1:{port}"),
                mac_id: "mac-1".to_string(),
                relay_token: "test-token".to_string(),
                e2e_key_b64: B64.encode(e2e_key),
            };
            let conn = RelayConnection::connect(creds, "test-device").expect("connect");
            let generation = conn.connection_generation();
            let pushes = conn.output_push_frames();

            // Plain request/response round trip.
            let response = conn
                .perform(PerformParams {
                    method: "GET",
                    path: "/mobile/bootstrap",
                    query: HashMap::new(),
                    auth: Some("Bearer tok"),
                    content_type: None,
                    body: None,
                    timeout: Duration::from_secs(5),
                })
                .expect("perform");
            assert_eq!(response.status, 200);
            assert_eq!(response.body(), b"ok-1");

            // Generation-bound request on the live generation succeeds.
            let request = RelayTunnelRequest {
                id: 42,
                method: "GET".to_string(),
                path: "/mobile/bootstrap".to_string(),
                query: HashMap::new(),
                auth: Some("Bearer tok".to_string()),
                content_type: None,
                body_b64: None,
            };
            let bound = conn
                .perform_request(&request, Some(generation), Duration::from_secs(5))
                .expect("generation-bound perform");
            assert_eq!(bound.connection_generation, generation);
            assert_eq!(bound.response.id, 42);
            assert_eq!(bound.response.body(), b"ok-42");

            // A stale generation fails closed without touching the socket.
            let stale =
                conn.perform_request(&request, Some(generation + 1), Duration::from_secs(1));
            assert!(
                matches!(stale, Err(RelayError::GenerationChanged)),
                "stale generation must fail closed, got {stale:?}"
            );

            // The host push frame arrives on the push channel, demuxed from
            // responses by shape.
            let push = pushes
                .recv_timeout(Duration::from_secs(5))
                .expect("push frame");
            assert_eq!(push.stream, "sess-1");
            assert_eq!(push.offset, 128);
            assert_eq!(push.data(), b"terminal bytes");
            assert_eq!(push.cols, Some(80));

            conn.close();
            server.join().expect("mock host finishes");
        },
    );
}

#[test]
fn relay_connect_rejects_wrong_device_key() {
    common::run_with_timeout(
        "relay_connect_rejects_wrong_device_key",
        Duration::from_secs(30),
        || {
            // The mock host computes its transcript MAC with the real key; a
            // client with the wrong static key must fail the handshake — the
            // relay cannot MITM without the device key, and neither can a guesser.
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().unwrap().port();
            let server = mock_host(listener, [7u8; 32], "test-device", "test-token");

            let creds = RelayCredentials {
                relay_url: format!("http://127.0.0.1:{port}"),
                mac_id: "mac-1".to_string(),
                relay_token: "test-token".to_string(),
                e2e_key_b64: B64.encode([9u8; 32]), // wrong key
            };
            let err = match RelayConnection::connect(creds, "test-device") {
                Ok(_) => panic!("connect with wrong key must fail"),
                Err(e) => e,
            };
            assert!(
                matches!(
                    err,
                    RelayError::Transport {
                        delivery: supercli_client::DeliveryState::NotSent,
                        ..
                    }
                ),
                "wrong key fails as not-sent, got {err:?}"
            );
            server.join().expect("mock host finishes");
        },
    );
}
