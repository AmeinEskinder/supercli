//! Browser gallery transport: query encoding, chunked download/upload
//! against scripted mock Hosts over raw TCP (same pattern as
//! `output_e2e.rs`).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use unpeel_client::{HostClient, HostClientError};

struct RecordedRequest {
    method: String,
    target: String,
    body: Vec<u8>,
}

fn read_request(tcp: &mut TcpStream) -> RecordedRequest {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        let n = tcp.read(&mut byte).expect("request head");
        assert!(n > 0, "client sends a request");
        head.push(byte[0]);
    }
    let head_str = String::from_utf8_lossy(&head).into_owned();
    let mut lines = head_str.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    let mut content_length = 0usize;
    for line in lines {
        if let Some(value) = line
            .strip_prefix("Content-Length:")
            .or_else(|| line.strip_prefix("content-length:"))
        {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    tcp.read_exact(&mut body).expect("request body");
    RecordedRequest {
        method,
        target,
        body,
    }
}

/// Serve `connections` requests; `respond(i, request)` returns
/// `(status, body)` for the i-th connection. Returns the base URL, the
/// recorded requests, and the server thread.
fn serve<F>(
    connections: usize,
    respond: F,
) -> (
    String,
    Arc<Mutex<Vec<RecordedRequest>>>,
    thread::JoinHandle<()>,
)
where
    F: Fn(usize, &RecordedRequest) -> (u16, Vec<u8>) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let recorded: Arc<Mutex<Vec<RecordedRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded_thread = recorded.clone();
    let handle = thread::spawn(move || {
        for i in 0..connections {
            let (mut tcp, _) = listener.accept().expect("client connects");
            let request = read_request(&mut tcp);
            let (status, body) = respond(i, &request);
            recorded_thread.lock().expect("record").push(request);
            let reason = if status == 200 { "OK" } else { "Error" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            tcp.write_all(response.as_bytes()).expect("status line");
            tcp.write_all(&body).expect("body");
            tcp.flush().expect("flush");
        }
    });
    (format!("http://{addr}/mobile"), recorded, handle)
}

fn ok_json(body: &str) -> (u16, Vec<u8>) {
    (200, body.as_bytes().to_vec())
}

fn is_canonical_uuid_v4(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (i, b) in bytes.iter().enumerate() {
        let hyphen = matches!(i, 8 | 13 | 18 | 23);
        if hyphen {
            if *b != b'-' {
                return false;
            }
        } else if !b.is_ascii_hexdigit() || b.is_ascii_uppercase() {
            return false;
        }
    }
    bytes[14] == b'4' && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
}

fn query_param<'a>(target: &'a str, name: &str) -> Option<&'a str> {
    let query = target.split_once('?')?.1;
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == name {
                return Some(v);
            }
        }
    }
    None
}

#[test]
fn artifacts_list_parses_entries() {
    let (base, _recorded, handle) = serve(1, |_, _| {
        ok_json(
            r#"{"sessionID":"s-1","artifacts":[{"kind":"image","name":"a.png","size":12,"modified_at_unix_ms":7}],"capturedAtUnixMs":9}"#,
        )
    });
    let client = HostClient::new(&base, "t").expect("http client");
    let entries = client.browser_artifacts("s-1").expect("list parses");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, "image");
    assert_eq!(entries[0].name, "a.png");
    assert_eq!(entries[0].size, 12);
    handle.join().expect("server exits");
}

#[test]
fn artifact_name_is_percent_encoded() {
    let (base, recorded, handle) = serve(1, |_, _| {
        ok_json(
            r#"{"contentType":"image/png","offset":0,"nextOffset":3,"totalSize":3,"dataBase64":"eHl6"}"#,
        )
    });
    let client = HostClient::new(&base, "t").expect("http client");
    let (content_type, bytes) = client
        .artifact_bytes("s-1", "image", "a&b=c?.png")
        .expect("download");
    assert_eq!(content_type, "image/png");
    assert_eq!(bytes, b"xyz");
    let requests = recorded.lock().expect("recorded");
    assert_eq!(requests.len(), 1);
    // `&`, `=`, `?` must not survive raw into the query string.
    assert!(
        requests[0].target.contains("name=a%26b%3Dc%3F.png"),
        "encoded target: {}",
        requests[0].target
    );
    handle.join().expect("server exits");
}

#[test]
fn artifact_bytes_assembles_chunks() {
    // 300_000 bytes -> two 262144-bounded chunks.
    let fixture: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let total = fixture.len() as u64;
    let fixture = Arc::new(fixture);
    let fixture_for_assert = fixture.clone();
    let (base, recorded, handle) = serve(2, move |_, req| {
        let query = req.target.split_once('?').map(|(_, q)| q).unwrap_or("");
        let mut offset = 0u64;
        let mut limit = 262_144u64;
        for pair in query.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                if k == "offset" {
                    offset = v.parse().unwrap_or(0);
                } else if k == "limit" {
                    limit = v.parse().unwrap_or(262_144);
                }
            }
        }
        let end = (offset + limit).min(total) as usize;
        let chunk = &fixture[offset as usize..end];
        let body = format!(
            "{{\"contentType\":\"image/png\",\"offset\":{offset},\"nextOffset\":{end},\"totalSize\":{total},\"dataBase64\":\"{}\"}}",
            base64_encode(chunk)
        );
        ok_json(&body)
    });
    let client = HostClient::new(&base, "t").expect("http client");
    let (content_type, bytes) = client
        .artifact_bytes("s-1", "image", "big.png")
        .expect("assembles");
    assert_eq!(content_type, "image/png");
    assert_eq!(bytes.len(), fixture_for_assert.len());
    assert_eq!(bytes, fixture_for_assert.as_slice());
    assert_eq!(recorded.lock().expect("recorded").len(), 2);
    handle.join().expect("server exits");
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[test]
fn artifact_bytes_rejects_stalled_offset() {
    // nextOffset == offset with a non-empty chunk: without the stall guard
    // this would loop forever.
    let (base, _recorded, handle) = serve(1, |_, _| {
        ok_json(
            r#"{"contentType":"image/png","offset":0,"nextOffset":0,"totalSize":4,"dataBase64":"eHl6"}"#,
        )
    });
    let client = HostClient::new(&base, "t").expect("http client");
    let err = client
        .artifact_bytes("s-1", "image", "stuck.png")
        .expect_err("stall is an error");
    assert!(matches!(err, HostClientError::Decode(_)), "{err:?}");
    assert!(err.to_string().contains("stalled"), "{err}");
    handle.join().expect("server exits");
}

#[test]
fn artifact_bytes_rejects_size_mismatch() {
    // Host claims totalSize 100 but only ever delivers 10 bytes.
    let (base, _recorded, handle) = serve(1, |_, _| {
        ok_json(
            r#"{"contentType":"image/png","offset":0,"nextOffset":100,"totalSize":100,"dataBase64":"AAAAAAAAAAAAAA=="}"#,
        )
    });
    let client = HostClient::new(&base, "t").expect("http client");
    let err = client
        .artifact_bytes("s-1", "image", "short.png")
        .expect_err("short read is an error");
    assert!(matches!(err, HostClientError::Decode(_)), "{err:?}");
    assert!(err.to_string().contains("size mismatch"), "{err}");
    handle.join().expect("server exits");
}

#[test]
fn artifact_bytes_rejects_midstream_size_change() {
    let (base, _recorded, handle) = serve(2, |i, _| {
        let total = if i == 0 { 6 } else { 7 };
        ok_json(&format!(
            r#"{{"contentType":"image/png","offset":0,"nextOffset":3,"totalSize":{total},"dataBase64":"eHl6"}}"#
        ))
    });
    let client = HostClient::new(&base, "t").expect("http client");
    let err = client
        .artifact_bytes("s-1", "image", "shifty.png")
        .expect_err("size change is an error");
    assert!(err.to_string().contains("changed size"), "{err}");
    handle.join().expect("server exits");
}

#[test]
fn delete_artifact_encodes_name() {
    let (base, recorded, handle) = serve(1, |_, req| {
        assert_eq!(req.method, "POST");
        ok_json("{}")
    });
    let client = HostClient::new(&base, "t").expect("http client");
    client
        .delete_artifact("s-1", "image", "x y+z.png")
        .expect("delete ok");
    let requests = recorded.lock().expect("recorded");
    assert!(
        requests[0].target.contains("name=x%20y%2Bz.png"),
        "encoded target: {}",
        requests[0].target
    );
    handle.join().expect("server exits");
}

#[test]
fn request_screenshot_posts_session_id() {
    let (base, recorded, handle) = serve(1, |_, req| {
        assert_eq!(req.method, "POST");
        ok_json("{}")
    });
    let client = HostClient::new(&base, "t").expect("http client");
    client.request_screenshot("s-1").expect("screenshot ok");
    let requests = recorded.lock().expect("recorded");
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(body.contains(r#""sessionID":"s-1""#), "body: {body}");
    handle.join().expect("server exits");
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn upload_artifact_sends_uuid_and_sha256() {
    let fixture: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let total = fixture.len() as u64;
    let expected_sha = sha256_hex(&fixture);
    let (base, recorded, handle) = serve(2, move |i, req| {
        assert_eq!(req.method, "POST");
        assert!(req.target.starts_with("/mobile/upload-chunk?"));
        let upload_id = query_param(&req.target, "upload_id").expect("upload_id param");
        assert!(is_canonical_uuid_v4(upload_id), "uuid v4: {upload_id}");
        let sha = query_param(&req.target, "sha256").expect("sha256 param");
        assert_eq!(sha, expected_sha);
        let offset: u64 = query_param(&req.target, "offset").unwrap().parse().unwrap();
        let total_size: u64 = query_param(&req.target, "total_size")
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(total_size, total);
        let expect_offset = if i == 0 { 0 } else { 262_144 };
        assert_eq!(offset, expect_offset);
        assert_eq!(req.body.len() as u64, (total - expect_offset).min(262_144));
        let next = (expect_offset + req.body.len() as u64).min(total);
        let complete = next == total;
        if complete {
            ok_json(&format!(
                "{{\"nextOffset\":{next},\"complete\":true,\
                 \"path\":\"/sessions/s-1/artifacts/uploads/abc.png\",\
                 \"name\":\"abc.png\",\"kind\":\"uploads\"}}"
            ))
        } else {
            ok_json(&format!(
                "{{\"nextOffset\":{next},\"complete\":{complete}}}"
            ))
        }
    });
    let client = HostClient::new(&base, "t").expect("http client");
    let path = client
        .upload_artifact("s-1", "image/png", &fixture)
        .expect("upload ok");
    // The completion path is the Host-side absolute path — the value
    // clients quote into the agent's composer (Swift `uploadImage` parity).
    assert_eq!(path, "/sessions/s-1/artifacts/uploads/abc.png");
    let requests = recorded.lock().expect("recorded");
    assert_eq!(requests.len(), 2);
    // Both chunks carry the same upload id (one resumable upload).
    let id0 = query_param(&requests[0].target, "upload_id")
        .unwrap()
        .to_string();
    let id1 = query_param(&requests[1].target, "upload_id")
        .unwrap()
        .to_string();
    assert_eq!(id0, id1);
    handle.join().expect("server exits");
}

#[test]
fn upload_artifact_requires_completion_path() {
    // Host sets complete but omits the path: the client must not claim a
    // path it never received (the path is what gets pasted to the agent).
    let (base, _recorded, handle) =
        serve(1, |_, _| ok_json(r#"{"nextOffset":10,"complete":true}"#));
    let client = HostClient::new(&base, "t").expect("http client");
    let err = client
        .upload_artifact("s-1", "image/png", b"0123456789")
        .expect_err("missing completion path is an error");
    assert!(err.to_string().contains("completion path"), "{err}");
    handle.join().expect("server exits");
}

#[test]
fn upload_artifact_refuses_empty() {
    // Zero connections served: the refusal must happen before any I/O.
    let (base, recorded, handle) = serve(0, |_, _| ok_json("{}"));
    let client = HostClient::new(&base, "t").expect("http client");
    let err = client
        .upload_artifact("s-1", "image/png", &[])
        .expect_err("empty upload refused");
    assert!(err.to_string().contains("empty"), "{err}");
    assert!(recorded.lock().expect("recorded").is_empty());
    handle.join().expect("server exits");
}

#[test]
fn upload_artifact_requires_completion_flag() {
    // Host advances the offset to total_size but never sets complete:
    // the old code returned Ok here.
    let (base, _recorded, handle) =
        serve(1, |_, _| ok_json(r#"{"nextOffset":10,"complete":false}"#));
    let client = HostClient::new(&base, "t").expect("http client");
    let err = client
        .upload_artifact("s-1", "image/png", b"0123456789")
        .expect_err("missing complete flag is an error");
    assert!(err.to_string().contains("completion"), "{err}");
    handle.join().expect("server exits");
}

#[test]
fn upload_artifact_rejects_stalled_upload() {
    let (base, _recorded, handle) =
        serve(1, |_, _| ok_json(r#"{"nextOffset":0,"complete":false}"#));
    let client = HostClient::new(&base, "t").expect("http client");
    let err = client
        .upload_artifact("s-1", "image/png", b"0123456789")
        .expect_err("stalled upload is an error");
    assert!(err.to_string().contains("stalled"), "{err}");
    handle.join().expect("server exits");
}

#[test]
fn upload_chunk_surfaces_host_error() {
    let (base, _recorded, handle) = serve(1, |_, _| {
        (
            400,
            br#"{"error":"upload_id must be a canonical lowercase UUIDv4"}"#.to_vec(),
        )
    });
    let client = HostClient::new(&base, "t").expect("http client");
    let err = client
        .upload_artifact_chunk(&unpeel_client::UploadChunkParams {
            session_id: "s-1",
            upload_id: "not-a-uuid",
            offset: 0,
            total_size: 10,
            sha256_hex: "0",
            content_type: "image/png",
            bytes: b"0123456789",
        })
        .expect_err("host 400 surfaces");
    assert!(matches!(err, HostClientError::Status(400, _)), "{err:?}");
    handle.join().expect("server exits");
}

#[test]
fn canonical_uuid_v4_checker() {
    assert!(is_canonical_uuid_v4("123e4567-e89b-42d3-a456-426614174000"));
    assert!(!is_canonical_uuid_v4(
        "123e4567-e89b-12d3-a456-426614174000"
    )); // v1
    assert!(!is_canonical_uuid_v4(
        "123E4567-e89b-42d3-a456-426614174000"
    )); // uppercase
    assert!(!is_canonical_uuid_v4("123e4567e89b42d3a456426614174000")); // no hyphens
    assert!(!is_canonical_uuid_v4("short"));
}

#[test]
fn artifact_thumbnail_bytes_sends_max_dim() {
    let (base, recorded, handle) = serve(1, |_, _| {
        ok_json(
            r#"{"contentType":"image/jpeg","offset":0,"nextOffset":2,"totalSize":2,"dataBase64":"/9g="}"#,
        )
    });
    let client = HostClient::new(&base, "t").expect("http client");
    let (content_type, bytes) = client
        .artifact_thumbnail_bytes("s-1", "image", "shot.png", 512)
        .expect("thumbnail download");
    assert_eq!(content_type, "image/jpeg");
    assert_eq!(bytes, vec![0xff, 0xd8]);
    let requests = recorded.lock().expect("recorded");
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].target.contains("max_dim=512"),
        "max_dim on the wire: {}",
        requests[0].target
    );
    handle.join().expect("server exits");
}

#[test]
fn artifact_bytes_omits_max_dim() {
    // The full-bytes path must not send max_dim: a Host without a thumbnail
    // adapter falls back to original bytes anyway, but the plain route
    // keeps its exact historical contract.
    let (base, recorded, handle) = serve(1, |_, _| {
        ok_json(
            r#"{"contentType":"image/png","offset":0,"nextOffset":1,"totalSize":1,"dataBase64":"AA=="}"#,
        )
    });
    let client = HostClient::new(&base, "t").expect("http client");
    let _ = client
        .artifact_bytes("s-1", "image", "a.png")
        .expect("download");
    let requests = recorded.lock().expect("recorded");
    assert!(
        !requests[0].target.contains("max_dim"),
        "no max_dim: {}",
        requests[0].target
    );
    handle.join().expect("server exits");
}
