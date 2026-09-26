//! Standalone Devices-panel / /farm demo server.
//!
//! Starts a loopback HTTP server serving ONLY the device routes from
//! `supercli_serve::devices` (no pairing, no TLS, no full host), with the
//! production [`supercli_serve::devices::DeviceBackendProvider`] installed
//! and `SUPERCLI_FARM_DEMO` forced on so the three scripted demo devices are
//! present.
//!
//! Usage: `cargo run -p supercli-serve --example farm_demo` — prints the
//! bound `127.0.0.1:PORT`. Then:
//!
//! ```sh
//! curl http://127.0.0.1:$PORT/api/devices
//! # open http://127.0.0.1:$PORT/devices and /farm in a browser
//! ```
//!
//! This exists to prove the panel end-to-end (list / touch in device points /
//! stream handshake) without attached hardware.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;
use supercli_serve::devices;

fn main() {
    // Force the three scripted demo devices on before the provider installs.
    std::env::set_var("SUPERCLI_FARM_DEMO", "1");
    devices::install_default_provider();

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    listener
        .set_nonblocking(false)
        .expect("blocking listener");
    let port = listener.local_addr().expect("local addr").port();
    println!("farm_demo listening on 127.0.0.1:{port}");
    println!("  devices: http://127.0.0.1:{port}/devices");
    println!("  farm:    http://127.0.0.1:{port}/farm");
    println!("  api:     http://127.0.0.1:{port}/api/devices");

    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("accept: {e}");
                continue;
            }
        };
        std::thread::spawn(move || handle(stream));
    }
}

fn handle(mut stream: std::net::TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut buf = vec![0u8; 65536];
    let n = match stream.read(&mut buf) {
        Ok(0) | Err(_) => return,
        Ok(n) => n,
    };
    let (method, path, headers, body) = match parse_request(&buf[..n]) {
        Some(r) => r,
        None => {
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            return;
        }
    };
    // Only device routes are served here.
    if !devices::is_device_route(&path) {
        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    }
    // WebSocket upgrade: run the same connection handler as the real server.
    if headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("upgrade") && v.eq_ignore_ascii_case("websocket"))
    {
        // Reconstruct a stream the handler can read the request from: it
        // re-reads the request itself, so replay the bytes we consumed.
        let mut headers_map = std::collections::HashMap::new();
        for (k, v) in &headers {
            headers_map.insert(k.to_lowercase(), v.clone());
        }
        let mut replay = ReplayStream::new(stream, buf[..n].to_vec());
        devices::handle_device_connection(
            &mut replay,
            &method,
            &path,
            &headers_map,
            &body,
            false,
        );
        return;
    }
    let (status, resp_body, content_type) = devices::handle_device_http(&method, &path, &body);
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let head = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        resp_body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(resp_body.as_bytes());
}

/// Split an HTTP request head into method, path, headers, body.
fn parse_request(
    raw: &[u8],
) -> Option<(String, String, Vec<(String, String)>, Vec<u8>)> {
    let head_end = raw.windows(4).position(|w| w == b"\r\n\r\n")? + 4;
    let head = std::str::from_utf8(&raw[..head_end]).ok()?;
    let mut lines = head.lines();
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let content_len = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let body = raw[head_end..head_end.saturating_add(content_len).min(raw.len())].to_vec();
    Some((method, path, headers, body))
}

/// A TcpStream wrapper that replays already-read bytes first, so
/// `handle_device_connection` can re-read the request head itself.
struct ReplayStream {
    inner: std::net::TcpStream,
    replay: std::io::Cursor<Vec<u8>>,
}

impl ReplayStream {
    fn new(inner: std::net::TcpStream, prefix: Vec<u8>) -> Self {
        ReplayStream {
            inner,
            replay: std::io::Cursor::new(prefix),
        }
    }
}

impl Read for ReplayStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if (self.replay.position() as usize) < self.replay.get_ref().len() {
            self.replay.read(buf)
        } else {
            self.inner.read(buf)
        }
    }
}

impl Write for ReplayStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
