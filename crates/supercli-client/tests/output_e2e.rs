//! `output_chunk` parsing: strict on required fields, tolerant of new ones.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use supercli_client::{HostClient, HostClientError};

/// Serve one canned `/mobile/output` body, then exit.
fn mock_output_host(body: &'static [u8]) -> (thread::JoinHandle<()>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let handle = thread::spawn(move || {
        let (mut tcp, _) = listener.accept().expect("client connects");
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            let n = tcp.read(&mut byte).expect("request head");
            assert!(n > 0, "client sends a request");
            head.push(byte[0]);
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        tcp.write_all(response.as_bytes()).expect("status");
        tcp.write_all(body).expect("body");
        tcp.flush().expect("flush");
    });
    (handle, format!("http://{addr}/mobile"))
}

fn chunk(client: &HostClient) -> Result<supercli_client::OutputChunk, HostClientError> {
    client.output_chunk("session-1", None, 65536, 0)
}

#[test]
fn output_chunk_parses_valid_body() {
    let (_h, base) = mock_output_host(
        br#"{"offset":0,"nextOffset":4,"dataBase64":"aGVsbA==","truncated":false}"#,
    );
    let client = HostClient::new(&base, "t").expect("http client");
    let c = chunk(&client).expect("parses");
    assert_eq!(c.offset, 0);
    assert_eq!(c.next_offset, 4);
    assert!(!c.truncated);
    assert_eq!(c.data, b"hell");
}

#[test]
fn output_chunk_ignores_unknown_fields() {
    let (_h, base) = mock_output_host(
        br#"{"offset":1,"nextOffset":2,"dataBase64":"","truncated":true,"futureField":"x"}"#,
    );
    let client = HostClient::new(&base, "t").expect("http client");
    let c = chunk(&client).expect("future fields tolerated");
    assert!(c.truncated);
    assert_eq!(c.next_offset, 2);
}

#[test]
fn output_chunk_rejects_missing_next_offset() {
    let (_h, base) = mock_output_host(br#"{"offset":0,"dataBase64":"aGVsbA==","truncated":false}"#);
    let client = HostClient::new(&base, "t").expect("http client");
    match chunk(&client) {
        Err(HostClientError::Decode(_)) => {}
        other => panic!("expected Decode, got {other:?}"),
    }
}

#[test]
fn output_chunk_rejects_mistyped_truncated() {
    let (_h, base) = mock_output_host(
        br#"{"offset":0,"nextOffset":4,"dataBase64":"aGVsbA==","truncated":"yes"}"#,
    );
    let client = HostClient::new(&base, "t").expect("http client");
    match chunk(&client) {
        Err(HostClientError::Decode(_)) => {}
        other => panic!("expected Decode, got {other:?}"),
    }
}

#[test]
fn output_chunk_rejects_bad_base64() {
    let (_h, base) =
        mock_output_host(br#"{"offset":0,"nextOffset":4,"dataBase64":"!!!","truncated":false}"#);
    let client = HostClient::new(&base, "t").expect("http client");
    match chunk(&client) {
        Err(HostClientError::Decode(_)) => {}
        other => panic!("expected Decode, got {other:?}"),
    }
}

#[test]
fn output_compat_wrapper_fetches_from_start() {
    let (_h, base) =
        mock_output_host(br#"{"offset":0,"nextOffset":2,"dataBase64":"aGk=","truncated":false}"#);
    let client = HostClient::new(&base, "t").expect("http client");
    let c = client.output("session-1").expect("compat wrapper works");
    assert_eq!(c.data, b"hi");
}

#[test]
fn new_rejects_non_http_scheme() {
    match HostClient::new("ftp://example.com/mobile", "t") {
        Err(HostClientError::InvalidEndpoint(_)) => {}
        other => panic!("expected InvalidEndpoint, got {other:?}"),
    }
}
