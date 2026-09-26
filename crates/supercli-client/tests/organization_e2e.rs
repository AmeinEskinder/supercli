//! End-to-end test for [`HostClient::update_session_organization`] against
//! a plain-HTTP mock Host.
//!
//! Proves archive/restore ride the session-organization patch
//! (`POST /mobile/session-organization`) with the exact body shape the
//! native clients send — mirroring `RemoteMacClient.updateSessionOrganization`
//! — and that `None` fields are omitted rather than sent as null.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;

use supercli_client::HostClient;

const TOKEN: &str = "auth-token-1";

/// Serve one organization-patch request, asserting the exact wire shape the
/// native clients send. `expected` is the full expected JSON body.
fn mock_organization_host(
    listener: TcpListener,
    expected: serde_json::Value,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("client connects");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut request_line = String::new();
        reader.read_line(&mut request_line).expect("request line");
        assert!(
            request_line.starts_with("POST /mobile/session-organization HTTP/1.1"),
            "organization path, got: {request_line}"
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
        assert!(content_length > 0, "JSON body present");
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).expect("body");
        let request: serde_json::Value = serde_json::from_slice(&body).expect("body JSON");
        assert_eq!(request, expected, "organization patch body");

        let response_bytes = serde_json::to_vec(&serde_json::json!({})).unwrap();
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
fn archive_rides_the_organization_patch() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let handle = mock_organization_host(
        listener,
        serde_json::json!({"sessionID": "sess-1", "archived": true}),
    );

    let client = HostClient::new(format!("http://127.0.0.1:{port}/mobile"), TOKEN).expect("client");
    client
        .update_session_organization("sess-1", None, None, Some(true))
        .expect("archive patch");
    handle.join().expect("mock host");
}

#[test]
fn full_patch_sends_every_set_field() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let handle = mock_organization_host(
        listener,
        serde_json::json!({
            "sessionID": "sess-2",
            "title": "New title",
            "pinned": true,
            "archived": false,
        }),
    );

    let client = HostClient::new(format!("http://127.0.0.1:{port}/mobile"), TOKEN).expect("client");
    client
        .update_session_organization("sess-2", Some("New title"), Some(true), Some(false))
        .expect("full patch");
    handle.join().expect("mock host");
}

#[test]
fn full_patch_includes_notify_when_done() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    // `pinned: Some(false)` must be encoded, not dropped: false means
    // "unpin", only None means "leave untouched".
    let handle = mock_organization_host(
        listener,
        serde_json::json!({
            "sessionID": "sess-3",
            "title": "Renamed",
            "pinned": false,
            "notifyWhenDone": true,
        }),
    );

    let client = HostClient::new(format!("http://127.0.0.1:{port}/mobile"), TOKEN).expect("client");
    client
        .update_session_organization_full("sess-3", Some("Renamed"), Some(false), None, Some(true))
        .expect("full patch with notifyWhenDone");
    handle.join().expect("mock host");
}

/// Serve one project-organization PATCH, asserting the exact JSON body.
fn mock_project_organization_host(
    listener: TcpListener,
    expected: serde_json::Value,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("client connects");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut request_line = String::new();
        reader.read_line(&mut request_line).expect("request line");
        assert!(
            request_line.starts_with("POST /mobile/project-organization HTTP/1.1"),
            "project organization path, got: {request_line}"
        );
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("header");
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                content_length = value.trim().parse().expect("content length");
            }
        }
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).expect("body");
        let request: serde_json::Value = serde_json::from_slice(&body).expect("body JSON");
        assert_eq!(request, expected, "project organization patch body");

        let response_bytes = serde_json::to_vec(&serde_json::json!({})).unwrap();
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
fn project_organization_patch_body() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    // `pinned: None` is omitted; every set field keeps the Host's key
    // shape (projectID/displayName/colorID/dateSorted/sortOrder).
    let handle = mock_project_organization_host(
        listener,
        serde_json::json!({
            "projectID": "proj-1",
            "displayName": "Backend",
            "colorID": "sky",
            "dateSorted": true,
            "sortOrder": 2,
        }),
    );

    let client = HostClient::new(format!("http://127.0.0.1:{port}/mobile"), TOKEN).expect("client");
    client
        .update_project_organization(
            "proj-1",
            Some("Backend"),
            Some("sky"),
            Some(true),
            None,
            Some(2),
        )
        .expect("project patch");
    handle.join().expect("mock host");
}

/// Serve one GET /mobile/archive request: assert the project_id query is
/// percent-encoded, then reply with (status, body).
fn mock_archive_host(
    listener: TcpListener,
    expected_encoded_project_id: &str,
    status: u16,
    body: Vec<u8>,
) -> thread::JoinHandle<()> {
    let expected = expected_encoded_project_id.to_string();
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("client connects");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut request_line = String::new();
        reader.read_line(&mut request_line).expect("request line");
        assert!(
            request_line.starts_with(&format!("GET /mobile/archive?project_id={expected} ")),
            "archive path with encoded project_id, got: {request_line}"
        );
        // Drain headers.
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("header");
            if line.trim().is_empty() {
                break;
            }
        }
        let reason = if status == 200 { "OK" } else { "Error" };
        let mut stream = reader.into_inner();
        let http = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(http.as_bytes()).expect("headers");
        stream.write_all(&body).expect("body");
        stream.flush().expect("flush");
    })
}

#[test]
fn archived_sessions_encodes_project_id_and_parses_rows() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let rows = serde_json::json!({
        "sessions": [
            {
                "id": "s-arch-1",
                "projectID": "my proj/1",
                "title": "Old work",
                "archived": true,
                "capabilities": {"restart": true, "resume_agent": false, "archive": true, "notifyWhenDone": true}
            },
            {"id": "s-arch-2", "projectID": "my proj/1", "title": "Older", "archived": true}
        ]
    });
    let handle = mock_archive_host(
        listener,
        "my%20proj%2F1",
        200,
        serde_json::to_vec(&rows).unwrap(),
    );

    let client = HostClient::new(format!("http://127.0.0.1:{port}/mobile"), TOKEN).expect("client");
    let sessions = client
        .archived_sessions("my proj/1")
        .expect("archive rows parse");
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].id, "s-arch-1");
    assert!(sessions[0].archived);
    assert!(sessions[0].capabilities.restart);
    assert!(!sessions[0].capabilities.resume_agent);
    assert!(sessions[0].capabilities.notify_when_done);
    // Rows without a capabilities block default to all-false.
    assert!(!sessions[1].capabilities.restart);
    handle.join().expect("mock host");
}

#[test]
fn archived_sessions_non_2xx_is_status_not_reachability() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let handle = mock_archive_host(listener, "p1", 500, b"boom".to_vec());

    let client = HostClient::new(format!("http://127.0.0.1:{port}/mobile"), TOKEN).expect("client");
    match client.archived_sessions("p1") {
        Err(supercli_client::HostClientError::Status(500, _)) => {}
        other => panic!("expected Status(500, _), got: {other:?}"),
    }
    handle.join().expect("mock host");
}

#[test]
fn archived_sessions_bad_json_is_decode() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let handle = mock_archive_host(listener, "p1", 200, b"not json".to_vec());

    let client = HostClient::new(format!("http://127.0.0.1:{port}/mobile"), TOKEN).expect("client");
    match client.archived_sessions("p1") {
        Err(supercli_client::HostClientError::Decode(_)) => {}
        other => panic!("expected Decode, got: {other:?}"),
    }
    handle.join().expect("mock host");
}

#[test]
fn move_session_to_project_sends_project_id() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let handle = mock_organization_host(
        listener,
        serde_json::json!({"sessionID": "sess-1", "projectID": "proj-2"}),
    );

    let client = HostClient::new(format!("http://127.0.0.1:{port}/mobile"), TOKEN).expect("client");
    client
        .move_session_to_project("sess-1", "proj-2")
        .expect("move patch");
    handle.join().expect("mock host");
}
