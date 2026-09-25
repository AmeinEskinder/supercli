//! Shared mock-relay helpers for the relay integration tests.
//!
//! The WebSocket accept is transport-specific (`ws://` vs `wss://`), but
//! everything after the handshake — the host side of the relay crypto
//! handshake, sealed request serving, push frames — is transport-agnostic
//! and lives here so both tests exercise the same host behavior.

use std::io::{Read, Write};
use std::thread;
use std::time::Duration;

use base64::Engine;
use tungstenite::http::{Request, Response};
use tungstenite::{Message, WebSocket};

use supercli_client::crypto::handshake::transcript_mac;
use supercli_client::crypto::RelayCryptoSession;
use supercli_client::relay::{
    EphemeralKeyPair, RelayClientHello, RelayHostHello, RelayStreamPush, RelayTunnelRequest,
    RelayTunnelResponse, RELAY_PROTOCOL_VERSION,
};

pub const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// Shared across the relay test targets; some targets use only one of
/// the helpers, so dead-code is allowed per-target here.
#[allow(dead_code)]
pub fn b64_decode(s: &str) -> Vec<u8> {
    B64.decode(s).expect("valid base64 in test")
}

/// Assert the client offered the relay subprotocol with its token in the
/// header (never the URL), and select `supercli-relay` in the 101 response —
/// what the real relay does.
// `accept_hdr`'s `Callback` trait fixes the closure's error type to
// `http::Response`, which trips `result_large_err`; the trait gives us
// no smaller option.
#[allow(clippy::result_large_err)]
pub fn relay_handshake_response(
    req: &Request<()>,
    mut resp: Response<()>,
    relay_token: &str,
) -> Result<Response<()>, Response<Option<String>>> {
    let proto = req
        .headers()
        .get("Sec-WebSocket-Protocol")
        .expect("subprotocol header present")
        .to_str()
        .unwrap();
    // The token rides the header, never the URL.
    assert!(proto.contains("supercli-relay"), "offers supercli-relay");
    assert!(
        proto.contains(&format!("supercli-relay-token.{relay_token}")),
        "carries the relay token"
    );
    resp.headers_mut()
        .insert("Sec-WebSocket-Protocol", "supercli-relay".parse().unwrap());
    Ok(resp)
}

/// Run the host side of the relay handshake over an accepted WebSocket:
/// verify the plaintext client hello, answer with the host hello +
/// transcript MAC, derive the crypto session, serve two sealed
/// `/mobile/bootstrap` requests, emit one push frame, then close.
#[allow(dead_code)]
pub fn serve_relay_host<S: Read + Write>(
    ws: &mut WebSocket<S>,
    e2e_key: [u8; 32],
    device_id: &str,
) {
    // Client hello (plaintext).
    let client_hello: RelayClientHello = match ws.read().expect("hello") {
        Message::Binary(b) => serde_json::from_slice(&b).expect("client hello JSON"),
        other => panic!("expected binary hello, got {other:?}"),
    };
    assert_eq!(client_hello.v, RELAY_PROTOCOL_VERSION);
    assert_eq!(client_hello.device_id, device_id);
    let client_salt: [u8; 16] = b64_decode(&client_hello.salt_b64).try_into().unwrap();
    let client_ephemeral = b64_decode(&client_hello.ephemeral_public_key_b64);

    // Host hello + transcript MAC.
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

    // Serve two sealed requests (fewer if the client goes away — e.g. it
    // rejected our transcript MAC). Control frames are skipped: the
    // client's keepalive ping must not kill the host.
    let mut served = 0;
    while served < 2 {
        let plaintext = match ws.read() {
            Ok(Message::Binary(b)) => crypto.open(&b).expect("open request"),
            Ok(_) => continue,
            Err(_) => return,
        };
        served += 1;
        let request: RelayTunnelRequest = serde_json::from_slice(&plaintext).expect("request JSON");
        assert_eq!(request.path, "/mobile/bootstrap");
        assert_eq!(request.auth.as_deref(), Some("Bearer tok"));
        let response = RelayTunnelResponse {
            id: request.id,
            status: 200,
            body_b64: Some(B64.encode(format!("ok-{}", request.id))),
        };
        let sealed = crypto
            .seal(&serde_json::to_vec(&response).unwrap())
            .expect("seal response");
        ws.send(Message::Binary(sealed.into()))
            .expect("send response");
    }

    // One host-push frame (same crypto session, same direction).
    let push = RelayStreamPush {
        stream: "sess-1".to_string(),
        subscription_id: None,
        offset: 128,
        data_b64: Some(B64.encode(b"terminal bytes")),
        rebased: None,
        cols: Some(80),
        rows: Some(24),
        event: None,
        message: None,
        bootstrap: None,
    };
    let sealed = crypto
        .seal(&serde_json::to_vec(&push).unwrap())
        .expect("seal push");
    ws.send(Message::Binary(sealed.into())).expect("send push");

    // Give the client a moment to read, then close.
    thread::sleep(Duration::from_millis(300));
    let _ = ws.close(None);
}

/// Run a test body on a worker thread and fail the test if it does not
/// finish within `timeout`.
///
/// The relay integration tests do blocking socket I/O on two threads and
/// join the mock server at the end. If a socket-teardown regression ever
/// reintroduces the hang this suite once had (the client holding the last
/// socket handle open, so the server's read never returned EOF and
/// `server.join()` blocked forever), the suite must report a failure
/// instead of hanging forever. Panics inside the body are propagated
/// unchanged; only a genuine overrun becomes the timeout failure.
pub fn run_with_timeout(
    name: &'static str,
    timeout: Duration,
    body: impl FnOnce() + Send + 'static,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        let _ = tx.send(result);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(())) => {}
        Ok(Err(payload)) => std::panic::resume_unwind(payload),
        Err(_) => panic!(
            "test '{name}' did not finish within {timeout:?}: probable relay socket/teardown hang"
        ),
    }
}
