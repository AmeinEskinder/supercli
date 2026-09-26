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

/// CDP grants full browser control (arbitrary script evaluation,
/// navigation, input injection, credential-bearing pages). Refuse any
/// endpoint whose host is not loopback.
///
/// `localhost` is allowlisted by name; IP literals must parse as loopback
/// (covers the whole 127.0.0.0/8 and ::1); any other hostname is resolved
/// and refused unless EVERY resolved address is loopback. Unresolvable
/// hosts are refused (fail closed) rather than trusted.
fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    // Strip brackets from IPv6 literals ("[::1]" -> "::1").
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = bare.parse::<std::net::IpAddr>() {
        return ip.is_loopback();
    }
    // Non-IP hostname: resolve; all addresses must be loopback.
    match (host, 0).to_socket_addrs() {
        Ok(mut addrs) => {
            let mut any = false;
            for addr in &mut addrs {
                any = true;
                if !addr.ip().is_loopback() {
                    return false;
                }
            }
            any // false when resolution returned zero addresses
        }
        Err(_) => false, // fail closed: do not trust what we cannot resolve
    }
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
        // Security gate: CDP is full browser control. Refuse non-loopback
        // endpoints BEFORE any socket is opened (parse_ws_url also rejects
        // non-ws:// schemes).
        let (host, _, _) = parse_ws_url(endpoint)?;
        if !is_loopback_host(&host) {
            return Err(format!(
                "CDP endpoint refused: {host:?} is not loopback (CDP grants full browser control)"
            ));
        }
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

    /// Forward one input event via `Input.dispatchMouseEvent`.
    /// `mouse_type`: "mousePressed" | "mouseReleased" | "mouseMoved".
    /// `button`: "none" | "left" | "middle" | "right".
    fn dispatch_mouse_event(
        &mut self,
        session_id: &str,
        mouse_type: &str,
        x: f64,
        y: f64,
        button: &str,
        click_count: u32,
    ) -> Result<(), String> {
        match mouse_type {
            "mousePressed" | "mouseReleased" | "mouseMoved" => {}
            _ => return Err(format!("bad mouse event type: {mouse_type:?}")),
        }
        match button {
            "none" | "left" | "middle" | "right" => {}
            _ => return Err(format!("bad mouse button: {button:?}")),
        }
        if !x.is_finite() || !y.is_finite() {
            return Err("mouse coordinates must be finite".to_string());
        }
        let id = self.send_command(
            "Input.dispatchMouseEvent",
            serde_json::json!({
                "type": mouse_type, "x": x, "y": y,
                "button": button, "clickCount": click_count,
            }),
            Some(session_id),
        )?;
        self.recv_response(id).map(|_| ())
    }

    /// Forward one input event via `Input.dispatchKeyEvent`.
    /// `key_type`: "keyDown" | "keyUp" | "rawKeyDown" | "char".
    fn dispatch_key_event(
        &mut self,
        session_id: &str,
        key_type: &str,
        key: &str,
        code: &str,
        text: Option<&str>,
    ) -> Result<(), String> {
        match key_type {
            "keyDown" | "keyUp" | "rawKeyDown" | "char" => {}
            _ => return Err(format!("bad key event type: {key_type:?}")),
        }
        let mut params = serde_json::json!({ "type": key_type, "key": key, "code": code });
        if let Some(t) = text {
            params["text"] = serde_json::Value::String(t.to_string());
        }
        let id = self.send_command("Input.dispatchKeyEvent", params, Some(session_id))?;
        self.recv_response(id).map(|_| ())
    }
}

// ---------------------------------------------------------------------------
// Live takeover session: human takes control, then hands it back
// ---------------------------------------------------------------------------

/// Who currently drives the browser tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakeoverState {
    /// The agent drives; human input is refused.
    Agent,
    /// The human drives (agent's browser actions are paused); agent input
    /// paths must not inject events.
    Human,
}

/// Audit file for takeover transitions, under the host home dir.
pub fn takeover_audit_path(home: &std::path::Path) -> std::path::PathBuf {
    home.join("browser-takeover-audit.jsonl")
}

/// Hash recorded as `prev_hash` for the first entry in a takeover audit log.
const TAKEOVER_GENESIS_PREV_HASH: &str = "genesis";

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    // Local hex encoding: avoids a new dependency for one call.
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        out.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    out
}

/// Canonical bytes of a takeover audit entry for hashing: JSON object;
/// serde_json is built WITHOUT the preserve_order feature, so keys
/// serialize in alphabetical order regardless of insertion order. `entry_hash`
/// is excluded (it is what we are computing).
fn takeover_audit_canonical_bytes(
    ts_ms: u64,
    event: &str,
    target_id: &str,
    actor: &str,
    reason: &str,
    prev_hash: &str,
) -> Vec<u8> {
    serde_json::json!({
        "actor": actor,
        "event": event,
        "prev_hash": prev_hash,
        "reason": reason,
        "target_id": target_id,
        "ts_ms": ts_ms,
    })
    .to_string()
    .into_bytes()
}

/// Read the `entry_hash` of the last line in the takeover audit log, if any.
fn takeover_audit_last_hash(home: &std::path::Path) -> Result<Option<String>, String> {
    let path = takeover_audit_path(home);
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("audit open: {e}")),
    };
    use std::io::BufRead;
    let reader = std::io::BufReader::new(file);
    let mut last: Option<String> = None;
    for line in reader.lines() {
        let line = line.map_err(|e| format!("audit read: {e}"))?;
        let v: serde_json::Value =
            serde_json::from_str(&line).map_err(|e| format!("audit parse: {e}"))?;
        if let Some(h) = v.get("entry_hash").and_then(|h| h.as_str()) {
            last = Some(h.to_string());
        }
    }
    Ok(last)
}

/// Verify the hash chain of the takeover audit log. Returns the number of
/// entries. Any tampering (flipped byte), reordering, fork, or truncation
/// is an error.
pub fn verify_takeover_audit_chain(home: &std::path::Path) -> Result<usize, String> {
    let path = takeover_audit_path(home);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("audit open: {e}"))?;
    let mut prev = TAKEOVER_GENESIS_PREV_HASH.to_string();
    let mut count = 0usize;
    for (i, line) in text.lines().enumerate() {
        let v: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("audit line {i}: parse: {e}"))?;
        let field = |k: &str| {
            v.get(k)
                .and_then(|x| x.as_str())
                .ok_or_else(|| format!("audit line {i}: missing {k}"))
        };
        let ts_ms = v
            .get("ts_ms")
            .and_then(|x| x.as_u64())
            .ok_or_else(|| format!("audit line {i}: missing ts_ms"))?;
        let (event, target_id, actor, reason, file_prev, file_hash) = (
            field("event")?,
            field("target_id")?,
            field("actor")?,
            field("reason")?,
            field("prev_hash")?,
            field("entry_hash")?,
        );
        if file_prev != prev {
            return Err(format!(
                "audit line {i}: prev_hash mismatch (reorder or fork)"
            ));
        }
        let recomputed = sha256_hex(&takeover_audit_canonical_bytes(
            ts_ms, event, target_id, actor, reason, file_prev,
        ));
        if recomputed != file_hash {
            return Err(format!("audit line {i}: entry_hash mismatch (tampered)"));
        }
        prev = file_hash.to_string();
        count += 1;
    }
    Ok(count)
}

fn audit_transition(
    home: &std::path::Path,
    event: &str,
    target_id: &str,
    actor: &str,
    reason: &str,
) -> Result<(), String> {
    let path = takeover_audit_path(home);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("audit dir: {e}"))?;
    }
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let prev_hash =
        takeover_audit_last_hash(home)?.unwrap_or_else(|| TAKEOVER_GENESIS_PREV_HASH.to_string());
    let entry_hash = sha256_hex(&takeover_audit_canonical_bytes(
        ts_ms, event, target_id, actor, reason, &prev_hash,
    ));
    let line = serde_json::json!({
        "ts_ms": ts_ms,
        "event": event,
        "target_id": target_id,
        "actor": actor,
        "reason": reason,
        "prev_hash": prev_hash,
        "entry_hash": entry_hash,
    })
    .to_string();
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("audit open: {e}"))?;
    f.write_all(line.as_bytes())
        .map_err(|e| format!("audit write: {e}"))?;
    f.write_all(b"\n")
        .map_err(|e| format!("audit write: {e}"))?;
    f.sync_all().map_err(|e| format!("audit fsync: {e}"))?;
    Ok(())
}

/// A live takeover session: the human takes control of a browser tab (the
/// agent's browser actions pause), drives it via CDP input injection, then
/// hands control back to the agent.
///
/// State machine (fail closed):
/// - `begin` → Agent. `pause_for_human` → Human. `resume_agent` → Agent.
/// - Human input (`human_mouse`/`human_key`) is refused unless the session
///   is in Human state; pausing twice is an idempotent no-op, resuming
///   without a pause errors.
/// - `resume_agent` re-verifies the tab is still the same one (target id +
///   URL); a navigated-away or vanished tab refuses the handoff and control
///   stays with the human.
/// - Every transition appends a durable, hash-chained audit entry (fsync)
///   recording who moved control and why; [`verify_takeover_audit_chain`]
///   detects tampering.
pub struct TakeoverSession {
    client: CdpClient,
    session_id: String,
    target_id: String,
    /// URL of the tab as seen at `begin`; resume refuses if it changed.
    target_url: String,
    home: std::path::PathBuf,
    actor: String,
    state: TakeoverState,
}

impl TakeoverSession {
    /// Attach to a target and start in Agent state. Audit: `takeover_begin`.
    pub fn begin(
        home: &std::path::Path,
        endpoint: &str,
        target_id: &str,
        actor: &str,
    ) -> Result<Self, String> {
        let mut client = CdpClient::connect(endpoint)?;
        let session_id = client.attach(target_id)?;
        // Record the tab's URL now so resume can verify the human did not
        // navigate it away (or the target was replaced) while paused.
        let target_url = client
            .list_targets()?
            .into_iter()
            .find(|t| t.target_id == target_id)
            .map(|t| t.url)
            .ok_or_else(|| format!("takeover begin: target {target_id:?} not listed"))?;
        audit_transition(home, "takeover_begin", target_id, actor, "session opened")?;
        Ok(TakeoverSession {
            client,
            session_id,
            target_id: target_id.to_string(),
            target_url,
            home: home.to_path_buf(),
            actor: actor.to_string(),
            state: TakeoverState::Agent,
        })
    }

    pub fn state(&self) -> TakeoverState {
        self.state
    }

    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    /// URL recorded at `begin`, used to verify the tab on resume.
    pub fn target_url(&self) -> &str {
        &self.target_url
    }

    /// Re-list targets and confirm `target_id` still resolves to the URL
    /// recorded at `begin`. Fail closed: any mismatch or lookup failure
    /// refuses the handoff (control stays with the human).
    fn verify_target_unchanged(&mut self) -> Result<(), String> {
        let targets = self.client.list_targets()?;
        match targets.into_iter().find(|t| t.target_id == self.target_id) {
            Some(t) if t.url == self.target_url => Ok(()),
            Some(t) => Err(format!(
                "refused: target {} navigated away (was {:?}, now {:?}); control stays with the human",
                self.target_id, self.target_url, t.url
            )),
            None => Err(format!(
                "refused: target {} is no longer listed; control stays with the human",
                self.target_id
            )),
        }
    }

    /// Pause the agent's browser actions and hand control to the human.
    /// The CDP session stays attached (it is NOT closed); only the state
    /// gate changes, so in-flight agent input paths are refused from here
    /// on. Audit: `takeover_pause` (fsynced; on audit failure the state
    /// rolls back so we never claim an unrecorded handoff).
    /// Pausing twice is idempotent: a no-op success with no duplicate audit.
    pub fn pause_for_human(&mut self, reason: &str) -> Result<(), String> {
        if self.state == TakeoverState::Human {
            // Idempotent: already handed off; no state change, no duplicate
            // audit entry.
            return Ok(());
        }
        self.state = TakeoverState::Human;
        // Audit the transition; if the audit write fails, roll the state
        // back so we never claim a handoff that is not recorded.
        if let Err(e) = audit_transition(
            &self.home,
            "takeover_pause",
            &self.target_id,
            &self.actor,
            reason,
        ) {
            self.state = TakeoverState::Agent;
            return Err(e);
        }
        Ok(())
    }

    /// Forward a human mouse event. Refused unless the human holds control.
    pub fn human_mouse(
        &mut self,
        mouse_type: &str,
        x: f64,
        y: f64,
        button: &str,
        click_count: u32,
    ) -> Result<(), String> {
        if self.state != TakeoverState::Human {
            return Err("refused: the agent holds control; call pause_for_human first".to_string());
        }
        self.client.dispatch_mouse_event(
            &self.session_id.clone(),
            mouse_type,
            x,
            y,
            button,
            click_count,
        )
    }

    /// Forward a human key event. Refused unless the human holds control.
    pub fn human_key(
        &mut self,
        key_type: &str,
        key: &str,
        code: &str,
        text: Option<&str>,
    ) -> Result<(), String> {
        if self.state != TakeoverState::Human {
            return Err("refused: the agent holds control; call pause_for_human first".to_string());
        }
        self.client
            .dispatch_key_event(&self.session_id.clone(), key_type, key, code, text)
    }

    /// Hand control back to the agent. Audit: `takeover_resume`.
    /// Fail closed: the tab is re-verified (same target id AND same URL as
    /// at `begin`) BEFORE the state flips. If the human navigated away, the
    /// target vanished, or the check itself fails, control stays with the
    /// human and no resume is audited.
    pub fn resume_agent(&mut self, reason: &str) -> Result<(), String> {
        if self.state != TakeoverState::Human {
            return Err("refused: the agent already holds control".to_string());
        }
        self.verify_target_unchanged()?;
        self.state = TakeoverState::Agent;
        if let Err(e) = audit_transition(
            &self.home,
            "takeover_resume",
            &self.target_id,
            &self.actor,
            reason,
        ) {
            self.state = TakeoverState::Human;
            return Err(e);
        }
        Ok(())
    }

    pub fn close(&mut self) {
        self.client.close();
    }
}

// ---------------------------------------------------------------------------
// Process-global live-takeover session registry
//
// The JSON tool boundary is stateless, but a live takeover spans many
// calls (begin → pause → mouse/key… → resume → close). Sessions live here,
// keyed by an unguessable token returned at begin time. Both the MCP tool
// and the Host HTTP endpoint go through this registry.
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static TAKEOVER_SESSIONS: OnceLock<Mutex<HashMap<String, TakeoverSession>>> = OnceLock::new();

fn takeover_sessions() -> &'static Mutex<HashMap<String, TakeoverSession>> {
    TAKEOVER_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn new_session_token() -> String {
    static CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let n = CTR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("ts-{t:x}-{n:x}")
}

fn with_takeover_session<T>(
    token: &str,
    f: impl FnOnce(&mut TakeoverSession) -> Result<T, String>,
) -> Result<T, String> {
    let mut map = takeover_sessions()
        .lock()
        .map_err(|e| format!("takeover session lock: {e}"))?;
    let sess = map
        .get_mut(token)
        .ok_or_else(|| format!("unknown takeover session: {token:?}"))?;
    f(sess)
}

/// Live-takeover JSON actions on top of the stateless tool boundary.
///
/// Legacy calls (no `action`) keep the old behavior: `{"list": true}` or
/// a screenshot stream for `target_id`.
///
/// Live-takeover calls set `action`:
/// - `begin`: attach to `target_id` (needs `endpoint`, optional `actor`,
///   optional `home`). Returns `{"session": token, "state": "agent"}`.
/// - `pause`: hand control to the human (`reason`). Returns new state.
/// - `mouse`: forward a click/move (`type`, `x`, `y`, `button`,
///   `click_count`). Refused unless the human holds control.
/// - `key`: forward a key (`type`, `key`, `code`, optional `text`).
///   Refused unless the human holds control.
/// - `resume`: hand control back to the agent (`reason`).
/// - `status`: current `state` / `target_id` of a session.
/// - `close`: detach and drop the session.
///
/// Every begin/pause/resume is appended to the durable audit log
/// (`browser-takeover-audit.jsonl` under the host home dir).
pub fn takeover_session_tool(args: &serde_json::Value) -> Result<String, String> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or("takeover_session_tool: need action")?;
    let out = match action {
        "begin" => {
            let endpoint = args
                .get("endpoint")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| format!("ws://127.0.0.1:{DEFAULT_CDP_PORT}"));
            let target_id = args
                .get("target_id")
                .and_then(|v| v.as_str())
                .ok_or("begin: need target_id")?;
            let actor = args
                .get("actor")
                .and_then(|v| v.as_str())
                .unwrap_or("human:unknown-device");
            let home = args
                .get("home")
                .and_then(|v| v.as_str())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(crate::app_paths::supercli_home);
            let sess = TakeoverSession::begin(&home, &endpoint, target_id, actor)?;
            let token = new_session_token();
            let state = format!("{:?}", sess.state()).to_lowercase();
            let tid = sess.target_id().to_string();
            takeover_sessions()
                .lock()
                .map_err(|e| format!("takeover session lock: {e}"))?
                .insert(token.clone(), sess);
            serde_json::json!({ "session": token, "target_id": tid, "state": state })
        }
        "pause" | "resume" => {
            let token = args
                .get("session")
                .and_then(|v| v.as_str())
                .ok_or(format!("{action}: need session token"))?;
            let reason = args.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            let state = with_takeover_session(token, |s| {
                if action == "pause" {
                    s.pause_for_human(reason)
                } else {
                    s.resume_agent(reason)
                }?;
                Ok(format!("{:?}", s.state()).to_lowercase())
            })?;
            serde_json::json!({ "session": token, "state": state })
        }
        "mouse" => {
            let token = args
                .get("session")
                .and_then(|v| v.as_str())
                .ok_or("mouse: need session token")?;
            let mouse_type = args
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("mousePressed");
            let x = args.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = args.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let button = args
                .get("button")
                .and_then(|v| v.as_str())
                .unwrap_or("left");
            let click_count = args
                .get("click_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as u32;
            with_takeover_session(token, |s| {
                s.human_mouse(mouse_type, x, y, button, click_count)
            })?;
            serde_json::json!({ "ok": true })
        }
        "key" => {
            let token = args
                .get("session")
                .and_then(|v| v.as_str())
                .ok_or("key: need session token")?;
            let key_type = args
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("keyDown");
            let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("");
            let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let text = args.get("text").and_then(|v| v.as_str());
            with_takeover_session(token, |s| s.human_key(key_type, key, code, text))?;
            serde_json::json!({ "ok": true })
        }
        "status" => {
            let token = args
                .get("session")
                .and_then(|v| v.as_str())
                .ok_or("status: need session token")?;
            with_takeover_session(token, |s| {
                Ok(serde_json::json!({
                    "session": token,
                    "target_id": s.target_id(),
                    "state": format!("{:?}", s.state()).to_lowercase(),
                }))
            })?
        }
        "close" => {
            let token = args
                .get("session")
                .and_then(|v| v.as_str())
                .ok_or("close: need session token")?;
            let mut map = takeover_sessions()
                .lock()
                .map_err(|e| format!("takeover session lock: {e}"))?;
            let mut sess = map
                .remove(token)
                .ok_or_else(|| format!("unknown takeover session: {token:?}"))?;
            sess.close();
            serde_json::json!({ "ok": true })
        }
        _ => return Err(format!("unknown takeover action: {action:?}")),
    };
    serde_json::to_string(&out).map_err(|e| e.to_string())
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
    // Live-takeover session actions (begin/pause/mouse/key/resume/status/
    // close) go through the session registry; everything else keeps the
    // legacy list/stream behavior.
    if args.get("action").and_then(|v| v.as_str()).is_some() {
        return takeover_session_tool(args);
    }
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
        fake_cdp_server_with_url_fn(reject, None)
    }

    /// Fake CDP server whose `Target.getTargets` reports
    /// `https://example.com` on the first call and
    /// `https://navigated-away.example` afterwards, simulating the human
    /// navigating the tab away while the agent is paused.
    fn fake_cdp_server_navigating() -> String {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls2 = calls.clone();
        fake_cdp_server_with_url_fn(
            false,
            Some(std::sync::Arc::new(move || {
                if calls2.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    "https://example.com".to_string()
                } else {
                    "https://navigated-away.example".to_string()
                }
            })),
        )
    }

    fn fake_cdp_server_with_url_fn(
        reject: bool,
        url_fn: Option<std::sync::Arc<dyn Fn() -> String + Send + Sync>>,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(mut stream) = conn else { break };
                let url_fn = url_fn.clone();
                std::thread::spawn(move || {
                    if let Err(e) = handle_fake_conn(&mut stream, reject, url_fn) {
                        let _ = e;
                    }
                });
            }
        });
        // Give the accept loop a moment to start.
        std::thread::sleep(Duration::from_millis(20));
        format!("ws://127.0.0.1:{port}/devtools/browser/x")
    }

    fn handle_fake_conn(
        stream: &mut TcpStream,
        reject: bool,
        url_fn: Option<std::sync::Arc<dyn Fn() -> String + Send + Sync>>,
    ) -> Result<(), String> {
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
        while let Ok((opcode, payload)) = read_frame(stream) {
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
                "Target.getTargets" => {
                    let url = url_fn
                        .as_ref()
                        .map(|f| f())
                        .unwrap_or_else(|| "https://example.com".to_string());
                    serde_json::json!({
                        "id": id, "result": {"targetInfos": [
                            {"targetId": "tab-1", "title": "Example", "url": url, "type": "page"}
                        ]}
                    })
                }
                "Target.attachToTarget" => serde_json::json!({
                    "id": id, "result": {"sessionId": "sess-1"}
                }),
                "Page.captureScreenshot" => serde_json::json!({
                    "id": id, "result": {"data": png_b64}
                }),
                "Input.dispatchMouseEvent" | "Input.dispatchKeyEvent" => {
                    serde_json::json!({ "id": id, "result": {} })
                }
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
    fn takeover_refuses_non_loopback_endpoint() {
        // CDP grants full browser control: any non-loopback host must be
        // refused BEFORE a socket is opened. IP literals first (no DNS
        // involved, deterministic).
        for url in [
            "ws://192.168.1.100:9222/devtools/page/1",
            "ws://10.0.0.5:9222/devtools/page/1",
            "ws://8.8.8.8:9222/devtools/page/1",
            "ws://[2001:db8::1]:9222/devtools/page/1",
        ] {
            let err = CdpClient::connect(url).unwrap_err();
            assert!(
                err.contains("not loopback"),
                "non-loopback {url} must be refused, got: {err}"
            );
        }
        // A public hostname resolves to non-loopback (or fails to resolve
        // in a sandbox — either way it must be refused, fail closed).
        let err = CdpClient::connect("ws://example.com:9222/x").unwrap_err();
        assert!(err.contains("not loopback"), "got: {err}");

        // Loopback hosts pass the gate (they fail later at TCP connect,
        // which proves the refusal is about loopback, not connectivity).
        for url in [
            "ws://127.0.0.1:1/nope",
            "ws://127.0.0.2:1/nope",
            "ws://localhost:1/nope",
            "ws://[::1]:1/nope",
        ] {
            let err = CdpClient::connect(url).unwrap_err();
            assert!(
                !err.contains("not loopback"),
                "loopback {url} must pass the gate, got: {err}"
            );
        }
    }

    #[test]
    fn is_loopback_host_unit_cases() {
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("LOCALHOST"));
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("127.0.0.2"));
        assert!(is_loopback_host("::1"));
        assert!(!is_loopback_host("192.168.1.1"));
        assert!(!is_loopback_host("10.0.0.1"));
        assert!(!is_loopback_host("0.0.0.0"));
        // Unresolvable names fail closed.
        assert!(!is_loopback_host("no-such-host.invalid"));
    }

    fn takeover_test_home() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "supercli-takeover-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn read_takeover_audit(home: &std::path::Path) -> Vec<serde_json::Value> {
        let text = std::fs::read_to_string(takeover_audit_path(home)).unwrap();
        text.lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn takeover_pause_handoff_resume_is_audited() {
        let url = fake_cdp_server(false);
        let home = takeover_test_home();

        // begin -> Agent state, audited.
        let mut sess = TakeoverSession::begin(&home, &url, "tab-1", "human:test-device").unwrap();
        assert_eq!(sess.state(), TakeoverState::Agent);

        // Human input while the agent holds control is refused (fail closed).
        let err = sess
            .human_mouse("mousePressed", 100.0, 200.0, "left", 1)
            .unwrap_err();
        assert!(err.contains("agent holds control"), "got: {err}");
        let err = sess
            .human_key("keyDown", "Enter", "Enter", None)
            .unwrap_err();
        assert!(err.contains("agent holds control"), "got: {err}");

        // Pause -> Human state, audited. The CDP session stays attached:
        // human input forwards over the same connection (proves pause did
        // not tear the session down).
        sess.pause_for_human("user clicked Take Over").unwrap();
        assert_eq!(sess.state(), TakeoverState::Human);
        // Pausing twice is idempotent: no-op success, still Human, and no
        // duplicate audit entry.
        sess.pause_for_human("again").unwrap();
        assert_eq!(sess.state(), TakeoverState::Human);

        // Human input now forwards via CDP Input.* (fake server acks).
        sess.human_mouse("mousePressed", 100.0, 200.0, "left", 1)
            .unwrap();
        sess.human_mouse("mouseReleased", 100.0, 200.0, "left", 1)
            .unwrap();
        sess.human_key("keyDown", "Enter", "Enter", None).unwrap();
        sess.human_key("char", "a", "KeyA", Some("a")).unwrap();
        // Bad event types are rejected without touching the wire.
        let err = sess.human_mouse("nope", 0.0, 0.0, "left", 1).unwrap_err();
        assert!(err.contains("bad mouse event type"), "got: {err}");

        // Resume -> Agent state, audited.
        sess.resume_agent("user clicked Hand Back").unwrap();
        assert_eq!(sess.state(), TakeoverState::Agent);
        // Resuming without a pause is refused.
        let err = sess.resume_agent("again").unwrap_err();
        assert!(err.contains("already holds control"), "got: {err}");
        // Human input is refused again.
        let err = sess
            .human_mouse("mousePressed", 1.0, 1.0, "left", 1)
            .unwrap_err();
        assert!(err.contains("agent holds control"), "got: {err}");
        sess.close();

        // Audit log: exactly the three transitions, in order, with actor
        // and reasons. Nothing else was recorded.
        let entries = read_takeover_audit(&home);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0]["event"], "takeover_begin");
        assert_eq!(entries[1]["event"], "takeover_pause");
        assert_eq!(entries[2]["event"], "takeover_resume");
        for e in &entries {
            assert_eq!(e["target_id"], "tab-1");
            assert_eq!(e["actor"], "human:test-device");
            assert!(e["ts_ms"].as_u64().unwrap() > 0);
        }
        assert_eq!(entries[1]["reason"], "user clicked Take Over");
        assert_eq!(entries[2]["reason"], "user clicked Hand Back");
        // Timestamps are non-decreasing.
        assert!(entries[0]["ts_ms"].as_u64().unwrap() <= entries[1]["ts_ms"].as_u64().unwrap());
        assert!(entries[1]["ts_ms"].as_u64().unwrap() <= entries[2]["ts_ms"].as_u64().unwrap());
        // Hash chain is intact over the three transitions.
        assert_eq!(verify_takeover_audit_chain(&home).unwrap(), 3);

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn takeover_double_pause_is_idempotent_no_duplicate_audit() {
        let url = fake_cdp_server(false);
        let home = takeover_test_home();
        let mut sess = TakeoverSession::begin(&home, &url, "tab-1", "human:x").unwrap();

        sess.pause_for_human("first").unwrap();
        // Repeated pauses are no-op successes: state stays Human.
        sess.pause_for_human("second").unwrap();
        sess.pause_for_human("third").unwrap();
        assert_eq!(sess.state(), TakeoverState::Human);

        // Exactly one pause entry: idempotent pauses are not re-audited.
        let entries = read_takeover_audit(&home);
        assert_eq!(
            entries
                .iter()
                .filter(|e| e["event"] == "takeover_pause")
                .count(),
            1
        );

        // The session still round-trips after the idempotent pauses.
        sess.resume_agent("back").unwrap();
        assert_eq!(sess.state(), TakeoverState::Agent);
        sess.close();
        assert_eq!(verify_takeover_audit_chain(&home).unwrap(), 3);

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn takeover_resume_fails_closed_when_tab_navigated() {
        let url = fake_cdp_server_navigating();
        let home = takeover_test_home();
        let mut sess = TakeoverSession::begin(&home, &url, "tab-1", "human:x").unwrap();
        // begin recorded the first URL.
        assert_eq!(sess.target_url(), "https://example.com");

        sess.pause_for_human("handoff").unwrap();
        assert_eq!(sess.state(), TakeoverState::Human);

        // The tab navigated while paused: resume must fail closed, control
        // stays with the human, and no resume is audited.
        let err = sess.resume_agent("hand back").unwrap_err();
        assert!(err.contains("navigated away"), "got: {err}");
        assert_eq!(sess.state(), TakeoverState::Human);

        // The CDP session is still alive: the human can keep driving.
        sess.human_mouse("mousePressed", 1.0, 1.0, "left", 1)
            .unwrap();

        // Audit log has begin + pause only; the chain still verifies.
        let entries = read_takeover_audit(&home);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["event"], "takeover_begin");
        assert_eq!(entries[1]["event"], "takeover_pause");
        assert_eq!(verify_takeover_audit_chain(&home).unwrap(), 2);
        sess.close();

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn takeover_audit_hash_chain_detects_tampering() {
        let url = fake_cdp_server(false);
        let home = takeover_test_home();
        let mut sess = TakeoverSession::begin(&home, &url, "tab-1", "human:x").unwrap();
        sess.pause_for_human("p").unwrap();
        sess.resume_agent("r").unwrap();
        sess.close();

        // Three linked entries verify.
        assert_eq!(verify_takeover_audit_chain(&home).unwrap(), 3);

        // Every entry carries prev_hash/entry_hash, linked head to tail.
        let entries = read_takeover_audit(&home);
        assert_eq!(entries[0]["prev_hash"], "genesis");
        for w in entries.windows(2) {
            assert_eq!(w[1]["prev_hash"], w[0]["entry_hash"]);
        }

        // Flip one byte in the middle entry (valid JSON, wrong hash):
        // verification must fail.
        let path = takeover_audit_path(&home);
        let text = std::fs::read_to_string(&path).unwrap();
        let tampered = text.replacen("takeover_pause", "takeover_pausf", 1);
        assert_ne!(text, tampered);
        std::fs::write(&path, tampered).unwrap();
        let err = verify_takeover_audit_chain(&home).unwrap_err();
        assert!(err.contains("entry_hash mismatch"), "got: {err}");

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn takeover_session_begin_audits_even_before_any_input() {
        // A session that is opened and closed without pause/resume still
        // records its lifetime start.
        let url = fake_cdp_server(false);
        let home = takeover_test_home();
        let mut sess = TakeoverSession::begin(&home, &url, "tab-1", "human:x").unwrap();
        sess.close();
        let entries = read_takeover_audit(&home);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["event"], "takeover_begin");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn takeover_session_tool_full_lifecycle() {
        // The JSON boundary (used by the MCP tool and the Host HTTP
        // endpoint) drives a whole pause/handoff/resume cycle.
        let url = fake_cdp_server(false);
        let home = takeover_test_home();
        let home_str = home.to_string_lossy().to_string();

        // begin
        let out: serde_json::Value = serde_json::from_str(
            &takeover_tool(&serde_json::json!({
                "action": "begin", "endpoint": url, "target_id": "tab-1",
                "actor": "human:gpuidart", "home": home_str,
            }))
            .unwrap(),
        )
        .unwrap();
        let token = out["session"].as_str().unwrap().to_string();
        assert_eq!(out["state"], "agent");

        // status
        let out: serde_json::Value = serde_json::from_str(
            &takeover_tool(&serde_json::json!({ "action": "status", "session": token })).unwrap(),
        )
        .unwrap();
        assert_eq!(out["state"], "agent");
        assert_eq!(out["target_id"], "tab-1");

        // mouse before pause is refused (agent holds control).
        let err = takeover_tool(&serde_json::json!({
            "action": "mouse", "session": token,
            "type": "mousePressed", "x": 10.0, "y": 20.0,
        }))
        .unwrap_err();
        assert!(err.contains("agent holds control"), "got: {err}");

        // pause -> human
        let out: serde_json::Value = serde_json::from_str(
            &takeover_tool(
                &serde_json::json!({ "action": "pause", "session": token, "reason": "take over" }),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(out["state"], "human");

        // mouse + key forward through the registry.
        let out: serde_json::Value = serde_json::from_str(
            &takeover_tool(&serde_json::json!({
                "action": "mouse", "session": token,
                "type": "mousePressed", "x": 10.0, "y": 20.0,
                "button": "left", "click_count": 1,
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(out["ok"], true);
        let out: serde_json::Value = serde_json::from_str(
            &takeover_tool(&serde_json::json!({
                "action": "key", "session": token,
                "type": "keyDown", "key": "Enter", "code": "Enter",
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(out["ok"], true);

        // resume -> agent; input refused again.
        let out: serde_json::Value = serde_json::from_str(
            &takeover_tool(
                &serde_json::json!({ "action": "resume", "session": token, "reason": "hand back" }),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(out["state"], "agent");
        let err = takeover_tool(&serde_json::json!({
            "action": "key", "session": token, "type": "keyDown", "key": "a", "code": "KeyA",
        }))
        .unwrap_err();
        assert!(err.contains("agent holds control"), "got: {err}");

        // unknown token is refused.
        let err = takeover_tool(&serde_json::json!({ "action": "status", "session": "ts-nope" }))
            .unwrap_err();
        assert!(err.contains("unknown takeover session"), "got: {err}");

        // close drops the session.
        let out: serde_json::Value = serde_json::from_str(
            &takeover_tool(&serde_json::json!({ "action": "close", "session": token })).unwrap(),
        )
        .unwrap();
        assert_eq!(out["ok"], true);
        let err = takeover_tool(&serde_json::json!({ "action": "status", "session": token }))
            .unwrap_err();
        assert!(err.contains("unknown takeover session"), "got: {err}");

        // The audit log recorded begin, pause, resume in order.
        let entries = read_takeover_audit(&home);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0]["event"], "takeover_begin");
        assert_eq!(entries[1]["event"], "takeover_pause");
        assert_eq!(entries[2]["event"], "takeover_resume");
        assert_eq!(entries[1]["actor"], "human:gpuidart");

        let _ = std::fs::remove_dir_all(&home);
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
