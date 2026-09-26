//! `supercli browser takeover` — attach to a running browser tab over CDP
//! and stream screenshots. The agent-drives/human-watches loop for the web.
//!
//! A dependency-free blocking WebSocket client (raw TCP + HTTP upgrade +
//! masked client frames), so the Host binary gains no new crates. It talks to
//! any Chrome DevTools Protocol endpoint: the agent-browser remote-cdp
//! binding (`~/.supercli/browser/remote-cdp.json`), or a plain
//! `chrome --remote-debugging-port=9222` (pass its `webSocketDebuggerUrl`).
//!
//! Only `ws://` (no TLS) is supported: CDP endpoints are loopback by design.
//! The client speaks the minimum CDP surface needed for takeover:
//! `Target.getTargets`, `Target.attachToTarget` (flattened), and
//! `Page.captureScreenshot`.
//!
//! Web UI / MCP wiring: call [`takeover_tool`] with a JSON object
//! (`{target_id|list, endpoint?, frames?, interval_ms?}`); it returns a JSON
//! summary. A future `browser_takeover` MCP tool is a 3-line dispatch
//! addition in `browser_mcp.rs` on top of it.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Default when `--endpoint` is omitted: a local debugging port. Chrome only
/// serves a bare port over HTTP (`/json/version`); pass the full
/// `webSocketDebuggerUrl` you get from there.
pub const DEFAULT_CDP_PORT: u16 = 9222;

const READ_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

/// One CDP target (tab, page, worker…).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetInfo {
    pub target_id: String,
    pub title: String,
    pub url: String,
    pub kind: String,
}

// ---------------------------------------------------------------------------
// Minimal blocking WebSocket client (RFC 6455, client side)
// ---------------------------------------------------------------------------

fn ws_key() -> String {
    // 16 pseudo-random bytes, base64-encoded. Unpredictability is only needed
    // against malicious proxies; localhost tooling just needs uniqueness.
    let mut x: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e3779b97f4a7c15)
        ^ (std::process::id() as u64).wrapping_mul(0xbf58476d1ce4e5b9);
    let mut bytes = [0u8; 16];
    for b in bytes.iter_mut() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = (x & 0xff) as u8;
    }
    base64_encode(&bytes)
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let mut n: u32 = 0;
        for (i, &b) in chunk.iter().enumerate() {
            n |= (b as u32) << (16 - 8 * i);
        }
        let pad = 3 - chunk.len();
        for i in 0..4 - pad {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
        for _ in 0..pad {
            out.push('=');
        }
    }
    out
}

/// Parsed `ws://host:port/path`. Only ws:// (no TLS): CDP is loopback.
fn parse_ws_url(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .strip_prefix("ws://")
        .ok_or_else(|| format!("only ws:// URLs are supported, got {url:?}"))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>()
                .map_err(|_| format!("bad port in {url:?}"))?,
        ),
        None => (authority.to_string(), 80),
    };
    if host.is_empty() {
        return Err(format!("empty host in {url:?}"));
    }
    Ok((host, port, path))
}

#[derive(Debug)]
pub struct WsClient {
    stream: TcpStream,
}

impl WsClient {
    pub fn connect(url: &str) -> Result<Self, String> {
        let (host, port, path) = parse_ws_url(url)?;
        let addr = format!("{host}:{port}");
        let socket_addr = addr
            .to_socket_addrs()
            .map_err(|e| format!("resolve {addr}: {e}"))?
            .next()
            .ok_or_else(|| format!("no address for {addr}"))?;
        let stream = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(10))
            .map_err(|e| format!("connect {addr}: {e}"))?;
        stream
            .set_read_timeout(Some(READ_TIMEOUT))
            .map_err(|e| format!("set read timeout: {e}"))?;
        let mut client = WsClient { stream };
        let key = ws_key();
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        client
            .stream
            .write_all(request.as_bytes())
            .map_err(|e| format!("write upgrade: {e}"))?;
        let mut head = Vec::new();
        let mut buf = [0u8; 1];
        loop {
            let n = client
                .stream
                .read(&mut buf)
                .map_err(|e| format!("read upgrade response: {e}"))?;
            if n == 0 {
                return Err("server closed during WebSocket handshake".to_string());
            }
            head.extend_from_slice(&buf[..n]);
            if head.ends_with(b"\r\n\r\n") {
                break;
            }
            if head.len() > 8192 {
                return Err("handshake response too large".to_string());
            }
        }
        let head_str = String::from_utf8_lossy(&head);
        let status = head_str.lines().next().unwrap_or("");
        if !status.contains(" 101 ") {
            return Err(format!("WebSocket handshake rejected: {status}"));
        }
        Ok(client)
    }

    /// Send one masked text frame.
    pub fn send_text(&mut self, text: &str) -> Result<(), String> {
        let payload = text.as_bytes();
        let len = payload.len();
        let mut frame = vec![0x81u8]; // FIN + text
        if len < 126 {
            frame.push(0x80 | len as u8);
        } else if len < 65536 {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(len as u64).to_be_bytes());
        }
        let mask: [u8; 4] = [0x12, 0x34, 0x56, 0x78];
        frame.extend_from_slice(&mask);
        for (i, b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        self.stream
            .write_all(&frame)
            .map_err(|e| format!("ws send: {e}"))
    }

    /// Read one application message. Answers pings, skips binary frames,
    /// errors on close or protocol violations.
    pub fn recv_text(&mut self) -> Result<String, String> {
        loop {
            let (opcode, payload) = read_frame(&mut self.stream)?;
            match opcode {
                0x1 => {
                    return String::from_utf8(payload)
                        .map_err(|_| "non-UTF8 text frame".to_string())
                }
                0x2 => continue, // binary: not used by CDP; skip
                0x8 => return Err("server closed the WebSocket".to_string()),
                0x9 => {
                    // ping -> pong with identical payload
                    let mut frame = vec![0x8Au8, 0x80 | payload.len() as u8];
                    let mask: [u8; 4] = [0x12, 0x34, 0x56, 0x78];
                    frame.extend_from_slice(&mask);
                    for (i, b) in payload.iter().enumerate() {
                        frame.push(b ^ mask[i % 4]);
                    }
                    self.stream
                        .write_all(&frame)
                        .map_err(|e| format!("ws pong: {e}"))?;
                }
                0xA => continue, // unsolicited pong
                other => return Err(format!("unexpected ws opcode {other:#x}")),
            }
        }
    }

    pub fn close(&mut self) {
        let _ = self.stream.write_all(&[0x88, 0x80, 0, 0, 0, 0]);
    }
}

/// Read one raw frame; returns (opcode, payload). Shared with tests.
pub(crate) fn read_frame(stream: &mut TcpStream) -> Result<(u8, Vec<u8>), String> {
    let mut hdr = [0u8; 2];
    stream
        .read_exact(&mut hdr)
        .map_err(|e| format!("ws read header: {e}"))?;
    let opcode = hdr[0] & 0x0f;
    let masked = hdr[1] & 0x80 != 0;
    let mut len = (hdr[1] & 0x7f) as u64;
    if len == 126 {
        let mut ext = [0u8; 2];
        stream
            .read_exact(&mut ext)
            .map_err(|e| format!("ws read ext16: {e}"))?;
        len = u16::from_be_bytes(ext) as u64;
    } else if len == 127 {
        let mut ext = [0u8; 8];
        stream
            .read_exact(&mut ext)
            .map_err(|e| format!("ws read ext64: {e}"))?;
        len = u64::from_be_bytes(ext);
    }
    if len > MAX_MESSAGE_BYTES as u64 {
        return Err("ws message too large".to_string());
    }
    let mask = if masked {
        let mut m = [0u8; 4];
        stream
            .read_exact(&mut m)
            .map_err(|e| format!("ws read mask: {e}"))?;
        Some(m)
    } else {
        None
    };
    let mut payload = vec![0u8; len as usize];
    stream
        .read_exact(&mut payload)
        .map_err(|e| format!("ws read payload: {e}"))?;
    if let Some(m) = mask {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= m[i % 4];
        }
    }
    Ok((opcode, payload))
}

// ---------------------------------------------------------------------------
// CDP client
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct CdpClient {
    ws: WsClient,
    next_id: u64,
}

impl CdpClient {
    pub fn connect(endpoint: &str) -> Result<Self, String> {
        Ok(CdpClient {
            ws: WsClient::connect(endpoint)?,
            next_id: 1,
        })
    }

    fn send_command(
        &mut self,
        method: &str,
        params: serde_json::Value,
        session_id: Option<&str>,
    ) -> Result<u64, String> {
        let id = self.next_id;
        self.next_id += 1;
        let mut msg = serde_json::json!({"id": id, "method": method, "params": params});
        if let Some(s) = session_id {
            msg["sessionId"] = serde_json::Value::String(s.to_string());
        }
        self.ws.send_text(&msg.to_string()).map(|()| id)
    }

    /// Read messages until the response with `id` arrives (at most 200
    /// unrelated messages — events — are skipped).
    fn recv_response(&mut self, id: u64) -> Result<serde_json::Value, String> {
        for _ in 0..200 {
            let text = self.ws.recv_text()?;
            let v: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| format!("bad CDP JSON: {e}"))?;
            if v.get("id").and_then(|i| i.as_u64()) == Some(id) {
                if let Some(err) = v.get("error") {
                    return Err(format!("CDP error: {err}"));
                }
                return Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null));
            }
            // else: an event or another session's message — keep waiting.
        }
        Err("CDP response not received (too many unrelated messages)".to_string())
    }

    pub fn list_targets(&mut self) -> Result<Vec<TargetInfo>, String> {
        let id = self.send_command("Target.getTargets", serde_json::json!({}), None)?;
        let result = self.recv_response(id)?;
        let infos = result
            .get("targetInfos")
            .and_then(|v| v.as_array())
            .ok_or("Target.getTargets: no targetInfos")?;
        Ok(infos
            .iter()
            .map(|t| TargetInfo {
                target_id: t
                    .get("targetId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .into(),
                title: t.get("title").and_then(|v| v.as_str()).unwrap_or("").into(),
                url: t.get("url").and_then(|v| v.as_str()).unwrap_or("").into(),
                kind: t.get("type").and_then(|v| v.as_str()).unwrap_or("").into(),
            })
            .collect())
    }

    fn attach(&mut self, target_id: &str) -> Result<String, String> {
        let id = self.send_command(
            "Target.attachToTarget",
            serde_json::json!({"targetId": target_id, "flatten": true}),
            None,
        )?;
        let result = self.recv_response(id)?;
        result
            .get("sessionId")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| "Target.attachToTarget: no sessionId".to_string())
    }

    /// Capture one screenshot of a target; returns raw PNG bytes.
    pub fn capture_screenshot(&mut self, target_id: &str) -> Result<Vec<u8>, String> {
        let session = self.attach(target_id)?;
        let id = self.send_command(
            "Page.captureScreenshot",
            serde_json::json!({"format": "png"}),
            Some(&session),
        )?;
        let result = self.recv_response(id)?;
        let data = result
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or("Page.captureScreenshot: no data")?;
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| format!("screenshot base64: {e}"))
    }

    /// Capture `frames` screenshots spaced `interval` apart (>= 5fps means
    /// interval <= 200ms; the caller chooses).
    pub fn takeover_stream(
        &mut self,
        target_id: &str,
        frames: usize,
        interval: Duration,
    ) -> Result<Vec<Vec<u8>>, String> {
        let mut out = Vec::with_capacity(frames);
        for _ in 0..frames {
            out.push(self.capture_screenshot(target_id)?);
            if interval > Duration::ZERO {
                std::thread::sleep(interval);
            }
        }
        Ok(out)
    }

    pub fn close(&mut self) {
        self.ws.close();
    }
}

// ---------------------------------------------------------------------------
// MCP / web-UI entry point
// ---------------------------------------------------------------------------

/// JSON in / JSON out, for a future `browser_takeover` MCP tool and the web
/// UI "Take over" button.
///
/// Input: `{"list": true}` or `{"target_id": "...", "frames": N,
/// "interval_ms": M, "endpoint": "ws://…"}`. Output: `{"targets": [...]}` or
/// `{"target_id":…, "frames": N, "bytes": B, "png_magic_ok": true}`.
pub fn takeover_tool(args: &serde_json::Value) -> Result<String, String> {
    let endpoint = args
        .get("endpoint")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| format!("ws://127.0.0.1:{DEFAULT_CDP_PORT}"));
    let mut client = CdpClient::connect(&endpoint)?;
    if args.get("list").and_then(|v| v.as_bool()).unwrap_or(false) {
        let targets = client.list_targets()?;
        client.close();
        return serde_json::to_string(
            &serde_json::json!({ "targets": targets.iter().map(|t| serde_json::json!({
            "target_id": t.target_id, "title": t.title, "url": t.url, "type": t.kind
        })).collect::<Vec<_>>() }),
        )
        .map_err(|e| e.to_string());
    }
    let target_id = args
        .get("target_id")
        .and_then(|v| v.as_str())
        .ok_or("takeover_tool: need target_id or list=true")?;
    let frames = args.get("frames").and_then(|v| v.as_u64()).unwrap_or(25) as usize;
    let interval_ms = args
        .get("interval_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(200);
    let shots = client.takeover_stream(target_id, frames, Duration::from_millis(interval_ms))?;
    client.close();
    let bytes: usize = shots.iter().map(|s| s.len()).sum();
    let png_magic_ok = shots
        .iter()
        .all(|s| s.starts_with(&[0x89, b'P', b'N', b'G']));
    serde_json::to_string(&serde_json::json!({
        "target_id": target_id, "frames": shots.len(), "bytes": bytes, "png_magic_ok": png_magic_ok
    }))
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Minimal PNG (1x1 RGBA) for the fake CDP server to serve.
    const FAKE_PNG: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89,
    ];

    /// Write one unmasked server->client text frame.
    fn server_send(stream: &mut TcpStream, text: &str) {
        let p = text.as_bytes();
        let mut frame = vec![0x81u8];
        if p.len() < 126 {
            frame.push(p.len() as u8);
        } else {
            frame.push(126);
            frame.extend_from_slice(&(p.len() as u16).to_be_bytes());
        }
        frame.extend_from_slice(p);
        stream.write_all(&frame).unwrap();
    }

    /// Spawn a fake CDP server accepting connections in a loop (one handler
    /// thread per connection). Returns the ws:// URL. If `reject` is true
    /// every handshake is answered with 400 instead of 101.
    fn fake_cdp_server(reject: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(mut stream) = conn else { break };
                std::thread::spawn(move || {
                    if let Err(e) = handle_fake_conn(&mut stream, reject) {
                        let _ = e;
                    }
                });
            }
        });
        // Give the accept loop a moment to start.
        std::thread::sleep(Duration::from_millis(20));
        format!("ws://127.0.0.1:{port}/devtools/browser/x")
    }

    fn handle_fake_conn(stream: &mut TcpStream, reject: bool) -> Result<(), String> {
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        // Read HTTP upgrade headers.
        let mut head = Vec::new();
        let mut b = [0u8; 1];
        loop {
            let n = stream.read(&mut b).map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("eof during handshake".to_string());
            }
            head.extend_from_slice(&b);
            if head.ends_with(b"\r\n\r\n") || head.len() > 8192 {
                break;
            }
        }
        if reject {
            stream
                .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        stream
            .write_all(
                b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: dummy\r\n\r\n",
            )
            .map_err(|e| e.to_string())?;
        let png_b64 = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(FAKE_PNG)
        };
        loop {
            let Ok((opcode, payload)) = read_frame(stream) else {
                break;
            };
            if opcode == 0x8 {
                break;
            }
            if opcode != 0x1 {
                continue;
            }
            let msg: serde_json::Value =
                serde_json::from_str(&String::from_utf8_lossy(&payload)).unwrap();
            let id = msg["id"].clone();
            let method = msg["method"].as_str().unwrap_or("").to_string();
            let reply = match method.as_str() {
                "Target.getTargets" => serde_json::json!({
                    "id": id, "result": {"targetInfos": [
                        {"targetId": "tab-1", "title": "Example", "url": "https://example.com", "type": "page"}
                    ]}
                }),
                "Target.attachToTarget" => serde_json::json!({
                    "id": id, "result": {"sessionId": "sess-1"}
                }),
                "Page.captureScreenshot" => serde_json::json!({
                    "id": id, "result": {"data": png_b64}
                }),
                _ => serde_json::json!({"id": id, "error": {"message": "unknown"}}),
            };
            server_send(stream, &reply.to_string());
        }
        Ok(())
    }

    #[test]
    fn takeover_lists_targets_from_fake_cdp() {
        let url = fake_cdp_server(false);
        let mut client = CdpClient::connect(&url).unwrap();
        let targets = client.list_targets().unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_id, "tab-1");
        assert_eq!(targets[0].url, "https://example.com");
        client.close();
    }

    #[test]
    fn takeover_capture_screenshot_returns_png() {
        let url = fake_cdp_server(false);
        let mut client = CdpClient::connect(&url).unwrap();
        let png = client.capture_screenshot("tab-1").unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert_eq!(png, FAKE_PNG);
        client.close();
    }

    #[test]
    fn takeover_stream_captures_n_frames() {
        let url = fake_cdp_server(false);
        let mut client = CdpClient::connect(&url).unwrap();
        let frames = client
            .takeover_stream("tab-1", 3, Duration::from_millis(0))
            .unwrap();
        assert_eq!(frames.len(), 3);
        assert!(frames.iter().all(|f| f == FAKE_PNG));
        client.close();
    }

    #[test]
    fn takeover_handshake_rejected_errors() {
        let url = fake_cdp_server(true);
        let err = CdpClient::connect(&url).unwrap_err();
        assert!(err.contains("handshake rejected"), "got: {err}");
    }

    #[test]
    fn takeover_bad_endpoint_errors() {
        // Nothing listens here.
        let err = CdpClient::connect("ws://127.0.0.1:1/nope").unwrap_err();
        assert!(err.contains("connect"), "got: {err}");
        // Non-ws scheme is refused without touching the network.
        let err = CdpClient::connect("http://127.0.0.1:9222/x").unwrap_err();
        assert!(err.contains("only ws://"), "got: {err}");
    }

    #[test]
    fn takeover_tool_lists_and_captures() {
        let url = fake_cdp_server(false);
        let out = takeover_tool(&serde_json::json!({"list": true, "endpoint": url})).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["targets"].as_array().unwrap().len(), 1);

        let out = takeover_tool(
            &serde_json::json!({"target_id": "tab-1", "frames": 2, "interval_ms": 0, "endpoint": url}),
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["frames"], 2);
        assert_eq!(v["png_magic_ok"], true);
    }

    #[test]
    fn ws_url_parsing_rejects_non_ws() {
        assert!(parse_ws_url("ws://127.0.0.1:9222/devtools/page/1").is_ok());
        assert!(parse_ws_url("wss://example.com/x").is_err());
        assert!(parse_ws_url("ws://:9222/x").is_err());
    }

    #[test]
    fn base64_encode_roundtrip_spot_check() {
        // "Hello" -> "SGVsbG8="
        assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
    }
}
