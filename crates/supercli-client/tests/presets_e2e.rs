//! End-to-end test for [`HostClient::create_session_with_preset`] against
//! a plain-HTTP mock Host.
//!
//! Proves the preset drawer launches with the exact body shape the native
//! clients send — `RemoteCreateSessionRequest` with `projectID` + `presetID`
//! on `POST /mobile/sessions` — and that the new session id is extracted
//! from the Host's `{"sessionID": …}` response.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;

use supercli_client::HostClient;

const TOKEN: &str = "auth-token-1";

/// Serve one `POST /mobile/sessions`, asserting the exact create body the
/// preset drawer sends, then reply with `{"sessionID": "sess-new-1"}`.
fn mock_create_host(listener: TcpListener, expected: serde_json::Value) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("client connects");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut request_line = String::new();
        reader.read_line(&mut request_line).expect("request line");
        assert!(
            request_line.starts_with("POST /mobile/sessions HTTP/1.1"),
            "sessions path, got: {request_line}"
        );
        let mut content_length = 0usize;
        let mut auth_ok = false;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("header");
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            let lower = line.to_lowercase();
            if let Some(value) = lower.strip_prefix("content-length:") {
                content_length = value.trim().parse().expect("content length");
            }
            if lower == format!("authorization: bearer {}", TOKEN.to_lowercase()) {
                auth_ok = true;
            }
        }
        assert!(auth_ok, "Bearer <redacted> header present");
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).expect("body");
        let request: serde_json::Value = serde_json::from_slice(&body).expect("body JSON");
        assert_eq!(request, expected, "create-session body");

        let response_bytes =
            serde_json::to_vec(&serde_json::json!({"sessionID": "sess-new-1"})).unwrap();
        let mut stream = reader.into_inner();
        let http = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response_bytes.len()
        );
        stream.write_all(http.as_bytes()).expect("headers");
        stream.write_all(&response_bytes).expect("body");
        stream.flush().expect("flush");
    })
}

#[test]
fn create_with_preset_sends_project_id_and_preset_id() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let handle = mock_create_host(
        listener,
        serde_json::json!({"projectID": "proj-1", "presetID": "preset-9"}),
    );

    let client = HostClient::new(format!("http://127.0.0.1:{port}/mobile"), TOKEN).expect("client");
    let response = client
        .create_session_with_preset("proj-1", "preset-9")
        .expect("create with preset");
    assert_eq!(
        HostClient::created_session_id(&response).as_deref(),
        Some("sess-new-1")
    );
    handle.join().expect("mock host");
}

#[test]
fn created_session_id_none_when_absent() {
    assert_eq!(HostClient::created_session_id(&serde_json::json!({})), None);
    assert_eq!(
        HostClient::created_session_id(&serde_json::json!({"sessionID": 42})),
        None
    );
}
