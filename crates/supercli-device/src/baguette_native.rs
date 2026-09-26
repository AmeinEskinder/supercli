//! Native baguette client in Rust — speaks baguette's `serve` WebSocket
//! stream protocol and the `baguette input` NDJSON input protocol directly,
//! with no per-command CLI round-trips.
//!
//! Compiled only with the `device` cargo feature. baguette itself is **not
//! vendored**: it must be installed by the user (`brew install baguette`).
//! This client shells out to it only to *spawn* the long-lived processes it
//! then drives natively: `baguette serve` (the WebSocket host) and
//! `baguette input` (the gesture pipe). This keeps the crate LITE (std
//! only, no new dependencies).
//!
//! ## Protocol reference
//!
//! - baguette README "Quick start": `baguette serve` hosts the web UI at
//!   `http://127.0.0.1:8421/simulators`; "For a long-lived session, pipe
//!   JSON gestures into `baguette input`". Coordinates are device points;
//!   `--width`/`--height` are the screen size in points.
//! - Wire protocol — `baguette input`: newline-delimited JSON on stdin →
//!   `{"ok":true}` / `{"ok":false,"error":…}` on stdout, one ack per line.
//!   Gestures: `tap`, `swipe`, `touch1-down`/`touch1-move`/`touch1-up`,
//!   `touch2-down`/`touch2-move`/`touch2-up` (the pinch path), `button`
//!   (`home`, `lock`, `power`, `volume-up`, `volume-down`, `action`,
//!   `app-switcher`, `swipe-to-home`, …), `key` (W3C `code`), `text`.
//! - Serve routes: `WS /devices/:udid/stream?format=` — `avcc` for H.264,
//!   `mjpeg` for MJPEG. Each WebSocket message is one unified wire frame
//!   (see [`crate::wire_format`]): `0x01` description, `0x02` keyframe,
//!   `0x03` delta, `0x04` JPEG seed. baguette already speaks this format,
//!   so the video path is a validated passthrough (byte-identical), exactly
//!   like [`crate::wire_format::wire_from_baguette`].
//!
//! ## Session shape (mirrors [`crate::scrcpy_native::ScrcpyNative`])
//!
//! [`BaguetteNative::connect`] gates (macOS → Apple Silicon → `baguette`
//! on PATH), ensures `baguette serve` is listening on 127.0.0.1:8421
//! (spawning it when needed), opens the `format=avcc` WebSocket stream,
//! reads the `0x01` description frame for the point/pixel geometry, then
//! spawns the persistent `baguette input` child. Dropping the session
//! kills the input child and — only if this session spawned it — the
//! serve process.
//!
//! The same Devices panel / `/farm` infrastructure serves both platforms:
//! the panel consumes [`crate::wire_format`] frames (0x01–0x04) and sends
//! [`crate::wire_format::DevicePoint`] input; platform is selected only by
//! the device id. On iOS the frames arrive via this client's WebSocket and
//! input goes through the NDJSON pipe — both in device points, baguette's
//! convention, so no rescaling is needed anywhere.
//!
//! Design: `docs/device.md` §9 (native client), §9.3 (unified wire format).

use super::{
    json::{parse_json, Json},
    tool_on_path,
    wire_format::{wire_from_baguette, DevicePoint, FRAME_DESCRIPTION},
    DeviceError, DeviceId,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Serve endpoint
// ---------------------------------------------------------------------------

/// Loopback host baguette `serve` binds to (README: 127.0.0.1:8421).
pub const BAGUETTE_SERVE_HOST: &str = "127.0.0.1";

/// Default `baguette serve` port (README: web UI at 127.0.0.1:8421).
pub const BAGUETTE_SERVE_PORT: u16 = 8421;

/// H.264 stream path on the serve WebSocket.
pub fn stream_path(session_id: &str) -> String {
    format!("/devices/{session_id}/stream?format=avcc")
}

/// How long `connect` waits for a freshly spawned `baguette serve` to
/// accept TCP connections.
const SERVE_START_TIMEOUT: Duration = Duration::from_secs(15);

/// How long `connect` waits for the `0x01` description frame once the
/// WebSocket is open.
const DESCRIPTION_TIMEOUT: Duration = Duration::from_secs(10);

/// Default video-socket read timeout after [`BaguetteNative::connect`].
/// Short timeouts let a caller poll for a frame with a deadline (a trial
/// with no frame in time is a "miss", not a fatal error).
const DEFAULT_VIDEO_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Ack deadline for one `baguette input` NDJSON line (protocol: one ack
/// per line).
const INPUT_ACK_TIMEOUT: Duration = Duration::from_secs(5);

/// Cap on a single reassembled WebSocket message (64 MiB): a runaway
/// frame must error, not OOM the host.
const MAX_WS_MESSAGE: usize = 64 * 1024 * 1024;

// ---------------------------------------------------------------------------
// SHA-1 (FIPS 180-4) + Base64, std-only, for the WebSocket handshake
// ---------------------------------------------------------------------------

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    let (blocks, rest) = msg.as_chunks::<64>();
    debug_assert!(rest.is_empty(), "SHA-1 padding guarantees full blocks");
    for chunk in blocks {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let mut n: u32 = 0;
        for (i, &b) in chunk.iter().enumerate() {
            n |= (b as u32) << (16 - 8 * i);
        }
        let pad = 3 - chunk.len();
        for i in 0..4 - pad {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 0x3F) as usize] as char);
        }
        for _ in 0..pad {
            out.push('=');
        }
    }
    out
}

/// xorshift64* — good enough for WebSocket nonces/masks (not for secrets).
/// Cell variant so [`WsClient`] methods can stay `&self`.
fn rand_u64_next(seed: &std::cell::Cell<u64>) -> u64 {
    let mut x = seed.get();
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    seed.set(x);
    x.wrapping_mul(0x2545F4914F6CDD1D)
}

fn nonce_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E3779B97F4A7C15);
    nanos
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(std::process::id() as u64)
}

/// `Sec-WebSocket-Accept` for a client key (RFC 6455 §1.3).
fn ws_accept_key(client_key: &str) -> String {
    const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    let mut material = String::with_capacity(client_key.len() + GUID.len());
    material.push_str(client_key.trim());
    material.push_str(GUID);
    base64_encode(&sha1(material.as_bytes()))
}

// ---------------------------------------------------------------------------
// Minimal RFC 6455 WebSocket client (std only)
// ---------------------------------------------------------------------------

/// One decoded server→client message.
#[derive(Debug, PartialEq, Eq)]
pub enum WsMessage {
    /// Binary or text payload (reassembled across fragments).
    Data(Vec<u8>),
    /// The server sent a close frame (we already replied with our own).
    Closed,
}

/// Opcodes we care about.
const OP_CONT: u8 = 0x0;
const OP_TEXT: u8 = 0x1;
const OP_BINARY: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xA;

/// A client-side WebSocket over a plain TCP stream. Only `ws://` (no TLS):
/// baguette `serve` is loopback-only by design.
///
/// All I/O methods take `&self`: `std` implements `Read`/`Write` for
/// `&TcpStream`, so a caller can hold the video reader and the input
/// channel disjointly (see [`BaguetteNative::split`]).
pub struct WsClient {
    stream: TcpStream,
    seed: std::cell::Cell<u64>,
    /// Bytes read past the HTTP response head during the handshake; a
    /// single TCP segment can carry the `101` head *and* the first
    /// WebSocket frames, so `read_message` drains these first.
    pending: std::cell::RefCell<Vec<u8>>,
}

impl WsClient {
    /// Open a WebSocket: TCP connect, HTTP upgrade, verify `101` and the
    /// `Sec-WebSocket-Accept` key. `path` is e.g. [`stream_path`].
    pub fn connect(host: &str, port: u16, path: &str) -> Result<Self, DeviceError> {
        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e| DeviceError::Parse(format!("bad serve address {host}:{port}: {e}")))?;
        let stream =
            TcpStream::connect_timeout(&addr, Duration::from_secs(5)).map_err(DeviceError::Io)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(DeviceError::Io)?;
        let seed = std::cell::Cell::new(nonce_seed());
        let key_bytes: [u8; 16] = {
            let mut b = [0u8; 16];
            for slot in b.chunks_mut(8) {
                let r = rand_u64_next(&seed);
                slot.copy_from_slice(&r.to_le_bytes());
            }
            b
        };
        let key = base64_encode(&key_bytes);
        let request = format!(
            "GET {path} HTTP/1.1\r\n\
             Host: {host}:{port}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: {key}\r\n\
             Sec-WebSocket-Version: 13\r\n\
             \r\n"
        );
        (&stream)
            .write_all(request.as_bytes())
            .map_err(DeviceError::Io)?;

        // Read the HTTP response head (bounded: 64 KiB of headers is plenty).
        let mut head = Vec::with_capacity(1024);
        let mut buf = [0u8; 1024];
        let end = loop {
            let n = (&stream).read(&mut buf).map_err(DeviceError::Io)?;
            if n == 0 {
                return Err(DeviceError::Parse(
                    "baguette serve: EOF before HTTP response head".to_string(),
                ));
            }
            head.extend_from_slice(&buf[..n]);
            if head.len() > 65536 {
                return Err(DeviceError::Parse(
                    "baguette serve: HTTP response head exceeds 64 KiB".to_string(),
                ));
            }
            if let Some(i) = find_crlf_crlf(&head) {
                break i;
            }
        };
        let head_str = String::from_utf8_lossy(&head[..end]);
        let mut lines = head_str.lines();
        let status = lines.next().unwrap_or("");
        if !status.starts_with("HTTP/1.1 101") && !status.starts_with("HTTP/1.0 101") {
            return Err(DeviceError::Parse(format!(
                "baguette serve: expected 101 Switching Protocols, got {status:?}"
            )));
        }
        let expected = ws_accept_key(&key);
        let mut accept_ok = false;
        for line in lines {
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.trim().eq_ignore_ascii_case("sec-websocket-accept")
                    && value.trim() == expected
                {
                    accept_ok = true;
                }
            }
        }
        if !accept_ok {
            return Err(DeviceError::Parse(
                "baguette serve: Sec-WebSocket-Accept mismatch (wrong server?)".to_string(),
            ));
        }
        Ok(WsClient {
            stream,
            seed,
            pending: std::cell::RefCell::new(head[end..].to_vec()),
        })
    }

    /// Send one client→server frame (always masked, per RFC 6455 §5.3).
    fn send_frame(&self, opcode: u8, payload: &[u8]) -> Result<(), DeviceError> {
        let mut hdr = Vec::with_capacity(14);
        hdr.push(0x80 | opcode); // FIN, no RSV
        let mut mask = [0u8; 4];
        mask.copy_from_slice(&rand_u64_next(&self.seed).to_le_bytes()[..4]);
        let len = payload.len();
        if len < 126 {
            hdr.push(0x80 | len as u8);
        } else if len < 65536 {
            hdr.push(0x80 | 126);
            hdr.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            hdr.push(0x80 | 127);
            hdr.extend_from_slice(&(len as u64).to_be_bytes());
        }
        hdr.extend_from_slice(&mask);
        (&self.stream).write_all(&hdr).map_err(DeviceError::Io)?;
        // Mask in chunks to avoid a second full-size allocation.
        let mut chunk = [0u8; 4096];
        for (i, piece) in payload.chunks(4096).enumerate() {
            for (j, &b) in piece.iter().enumerate() {
                chunk[j] = b ^ mask[(i * 4096 + j) % 4];
            }
            (&self.stream)
                .write_all(&chunk[..piece.len()])
                .map_err(DeviceError::Io)?;
        }
        (&self.stream).flush().map_err(DeviceError::Io)?;
        Ok(())
    }

    /// Fill `out` from the pending handshake-overflow buffer first, then
    /// from the socket.
    fn read_exact_into(&self, out: &mut [u8]) -> Result<(), DeviceError> {
        let n = {
            let pending = self.pending.borrow();
            let n = pending.len().min(out.len());
            out[..n].copy_from_slice(&pending[..n]);
            n
        };
        // Method call on the temporary: no `mut` binding needed.
        self.pending.borrow_mut().drain(..n);
        if n < out.len() {
            (&self.stream)
                .read_exact(&mut out[n..])
                .map_err(DeviceError::Io)?;
        }
        Ok(())
    }

    /// Read one server→client message, reassembling fragments. Answers
    /// pings with pongs; on a close frame replies close and returns
    /// [`WsMessage::Closed`].
    pub fn read_message(&self) -> Result<WsMessage, DeviceError> {
        let mut message: Option<Vec<u8>> = None;
        loop {
            let mut hdr = [0u8; 2];
            self.read_exact_into(&mut hdr)?;
            let fin = hdr[0] & 0x80 != 0;
            let opcode = hdr[0] & 0x0F;
            let masked = hdr[1] & 0x80 != 0;
            let mut len = (hdr[1] & 0x7F) as u64;
            if opcode >= OP_CLOSE {
                // Control frames must not be fragmented and carry ≤ 125 bytes.
                if !fin || len > 125 {
                    return Err(DeviceError::Parse(format!(
                        "baguette serve: malformed control frame (opcode {opcode:#x})"
                    )));
                }
            }
            if len == 126 {
                let mut ext = [0u8; 2];
                self.read_exact_into(&mut ext)?;
                len = u16::from_be_bytes(ext) as u64;
            } else if len == 127 {
                let mut ext = [0u8; 8];
                self.read_exact_into(&mut ext)?;
                len = u64::from_be_bytes(ext);
                if len >> 63 != 0 {
                    return Err(DeviceError::Parse(
                        "baguette serve: absurd frame length".to_string(),
                    ));
                }
            }
            if masked {
                // Servers MUST NOT mask (RFC 6455 §5.1): fail, don't guess.
                return Err(DeviceError::Parse(
                    "baguette serve: masked server frame (protocol violation)".to_string(),
                ));
            }
            let total = message.as_ref().map(|m| m.len()).unwrap_or(0) as u64 + len;
            if total > MAX_WS_MESSAGE as u64 {
                return Err(DeviceError::Parse(format!(
                    "baguette serve: message exceeds {MAX_WS_MESSAGE} bytes"
                )));
            }
            let mut payload = vec![0u8; len as usize];
            self.read_exact_into(&mut payload)?;
            match opcode {
                OP_CLOSE => {
                    let _ = self.send_frame(OP_CLOSE, &[]);
                    return Ok(WsMessage::Closed);
                }
                OP_PING => {
                    self.send_frame(OP_PONG, &payload)?;
                }
                OP_PONG => { /* keep-alive noise; ignore */ }
                OP_CONT => {
                    let m = message.get_or_insert_with(Vec::new);
                    m.extend_from_slice(&payload);
                    if fin {
                        return Ok(WsMessage::Data(message.take().unwrap()));
                    }
                }
                OP_TEXT | OP_BINARY => {
                    if message.is_some() {
                        return Err(DeviceError::Parse(
                            "baguette serve: new data frame before previous finished".to_string(),
                        ));
                    }
                    if fin {
                        return Ok(WsMessage::Data(payload));
                    }
                    message = Some(payload);
                }
                other => {
                    return Err(DeviceError::Parse(format!(
                        "baguette serve: unknown opcode {other:#x}"
                    )));
                }
            }
        }
    }

    /// Adjust the underlying socket read timeout (see
    /// [`BaguetteNative::set_video_read_timeout`]).
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> Result<(), DeviceError> {
        self.stream
            .set_read_timeout(timeout)
            .map_err(DeviceError::Io)
    }
}

fn find_crlf_crlf(haystack: &[u8]) -> Option<usize> {
    haystack
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
}

// ---------------------------------------------------------------------------
// `baguette input` NDJSON protocol
// ---------------------------------------------------------------------------

/// Touch phase for the streamed one- and two-finger gestures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Down,
    Move,
    Up,
}

impl TouchPhase {
    fn verb(self) -> &'static str {
        match self {
            TouchPhase::Down => "down",
            TouchPhase::Move => "move",
            TouchPhase::Up => "up",
        }
    }
}

/// Minimal JSON string escaper (quotes, backslash, control characters).
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Format an f64 with the shortest round-trippable representation
/// (`0.05`, not `0.050000000000000003`).
fn fmt_num(v: f64) -> String {
    // `{}` on f64 already prints the shortest representation that
    // round-trips (Rust uses Grisu/Ryū-style formatting).
    format!("{v}")
}

/// Device-point coordinate as baguette expects it: rounded to the nearest
/// integer point, matching the documented examples (`"x":219`).
fn pt(p: f32) -> i64 {
    p.round() as i64
}

/// `{"type":"tap","x":219,"y":478,"width":438,"height":954,"duration":0.05}`
/// — the exact documented shape. `width`/`height` are the screen size in
/// device points; `p` is a [`DevicePoint`].
pub fn encode_tap(p: DevicePoint, width_points: f32, height_points: f32) -> String {
    format!(
        "{{\"type\":\"tap\",\"x\":{},\"y\":{},\"width\":{},\"height\":{},\"duration\":0.05}}",
        pt(p.x),
        pt(p.y),
        pt(width_points),
        pt(height_points)
    )
}

/// `{"type":"swipe","startX":…,"startY":…,"endX":…,"endY":…,
/// "width":…,"height":…,"duration":0.3}`.
pub fn encode_swipe(
    from: DevicePoint,
    to: DevicePoint,
    width_points: f32,
    height_points: f32,
    duration_secs: f64,
) -> String {
    format!(
        "{{\"type\":\"swipe\",\"startX\":{},\"startY\":{},\"endX\":{},\"endY\":{},\"width\":{},\"height\":{},\"duration\":{}}}",
        pt(from.x),
        pt(from.y),
        pt(to.x),
        pt(to.y),
        pt(width_points),
        pt(height_points),
        fmt_num(duration_secs)
    )
}

/// `{"type":"touch1-down","x":219,"y":478,"width":438,"height":954}` —
/// the phase-driven one-finger stream (also the edge-gesture path; pass
/// `edge: "bottom"|"top"|"left"|"right"` for screen-edge system gestures).
pub fn encode_touch1(
    phase: TouchPhase,
    p: DevicePoint,
    width_points: f32,
    height_points: f32,
    edge: Option<&str>,
) -> String {
    let edge = match edge {
        Some(e) => format!(",\"edge\":{}", json_string(e)),
        None => String::new(),
    };
    format!(
        "{{\"type\":\"touch1-{}\",\"x\":{},\"y\":{},\"width\":{},\"height\":{}{}}}",
        phase.verb(),
        pt(p.x),
        pt(p.y),
        pt(width_points),
        pt(height_points),
        edge
    )
}

/// `{"type":"touch2-down","x1":175,"y1":478,"x2":263,"y2":478,
/// "width":438,"height":954}` — the two-finger stream; the primary
/// pinch/pan path for real-time gestures.
pub fn encode_touch2(
    phase: TouchPhase,
    p1: DevicePoint,
    p2: DevicePoint,
    width_points: f32,
    height_points: f32,
) -> String {
    format!(
        "{{\"type\":\"touch2-{}\",\"x1\":{},\"y1\":{},\"x2\":{},\"y2\":{},\"width\":{},\"height\":{}}}",
        phase.verb(),
        pt(p1.x),
        pt(p1.y),
        pt(p2.x),
        pt(p2.y),
        pt(width_points),
        pt(height_points)
    )
}

/// Hardware + virtual buttons: `home`, `lock`, `power`, `volume-up`,
/// `volume-down`, `action`, `app-switcher`, `swipe-to-home`,
/// `swipe-to-app-switcher`, `pull-down-to-lock-screen`,
/// `pull-down-to-notification-center`.
/// `{"type":"button","button":"home"}` — `duration` (seconds) is optional.
pub fn encode_button(button: &str, duration_secs: Option<f64>) -> String {
    let duration = match duration_secs {
        Some(d) => format!(",\"duration\":{}", fmt_num(d)),
        None => String::new(),
    };
    format!(
        "{{\"type\":\"button\",\"button\":{}{}}}",
        json_string(button),
        duration
    )
}

/// Keyboard: `code` is a W3C `KeyboardEvent.code`.
pub fn encode_key(code: &str) -> String {
    format!("{{\"type\":\"key\",\"code\":{}}}", json_string(code))
}

/// Typed text into the focused field.
pub fn encode_text(text: &str) -> String {
    format!("{{\"type\":\"text\",\"text\":{}}}", json_string(text))
}

/// Parse one `baguette input` ack line: `{"ok":true}` → `Ok(())`,
/// `{"ok":false,"error":"…"}` → `Err(reason)`.
pub fn parse_ack(line: &str) -> Result<(), String> {
    let v = parse_json(line).map_err(|e| format!("baguette input: ack is not JSON: {e}"))?;
    match v.get("ok") {
        Some(Json::Bool(true)) => Ok(()),
        Some(Json::Bool(false)) => {
            let reason = v
                .get("error")
                .and_then(Json::as_str)
                .unwrap_or("unknown error");
            Err(format!("baguette input rejected gesture: {reason}"))
        }
        _ => Err("baguette input: ack missing boolean \"ok\"".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Persistent `baguette input` child
// ---------------------------------------------------------------------------

/// A long-lived `baguette input` session: NDJSON gestures go in on stdin,
/// one `{"ok":…}` ack per line comes back on stdout. Constructed via the
/// crate-private `InputChannel::spawn`; dropping it kills the child.
///
/// A dedicated reader thread pumps ack lines into a channel so [`send`]
/// can enforce the ack deadline instead of blocking forever on a wedged
/// child.
///
/// [`send`]: InputChannel::send
pub struct InputChannel {
    child: Child,
    stdin: ChildStdin,
    acks: mpsc::Receiver<Result<(), String>>,
}

impl InputChannel {
    /// Spawn the input child. `program`/`args` are split out (instead of
    /// hard-coding `baguette input …`) so tests can drive a stub.
    pub(crate) fn spawn(program: &str, args: &[&str]) -> Result<Self, DeviceError> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    DeviceError::ToolMissing(program.to_string())
                } else {
                    DeviceError::Io(e)
                }
            })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            DeviceError::Io(std::io::Error::other("baguette input: stdin was not piped"))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            DeviceError::Io(std::io::Error::other(
                "baguette input: stdout was not piped",
            ))
        })?;
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("baguette-input-acks".to_string())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => {
                            let _ = tx.send(Err(
                                "baguette input: stdout closed (child exited?)".to_string()
                            ));
                            break;
                        }
                        Ok(_) => {
                            if tx.send(parse_ack(&line)).is_err() {
                                break; // receiver gone; child teardown in progress
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(format!("baguette input: ack read failed: {e}")));
                            break;
                        }
                    }
                }
            })
            .map_err(DeviceError::Io)?;
        Ok(InputChannel {
            child,
            stdin,
            acks: rx,
        })
    }

    /// Write one NDJSON gesture line and wait for its ack.
    pub fn send(&mut self, json_line: &str) -> Result<(), DeviceError> {
        self.stdin
            .write_all(json_line.as_bytes())
            .map_err(DeviceError::Io)?;
        self.stdin.write_all(b"\n").map_err(DeviceError::Io)?;
        self.stdin.flush().map_err(DeviceError::Io)?;
        match self.acks.recv_timeout(INPUT_ACK_TIMEOUT) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(reason)) => Err(DeviceError::ToolFailed {
                tool: "baguette input".to_string(),
                code: None,
                stderr: reason,
            }),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(DeviceError::Timeout {
                tool: "baguette input".to_string(),
            }),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(DeviceError::Io(
                std::io::Error::other("baguette input: ack channel closed"),
            )),
        }
    }
}

impl Drop for InputChannel {
    fn drop(&mut self) {
        // Best effort: a leaked `baguette input` child would hold the
        // simulator's input pipe open.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------------------------------------------------------------------------
// Native baguette session
// ---------------------------------------------------------------------------

/// Geometry parsed from the `0x01` stream description frame.
#[derive(Debug, Clone, PartialEq)]
pub struct BaguetteDescription {
    pub device_name: String,
    pub width_points: f32,
    pub height_points: f32,
    pub width_pixels: u32,
    pub height_pixels: u32,
}

fn description_from_json(payload: &[u8]) -> Result<BaguetteDescription, DeviceError> {
    let text = std::str::from_utf8(payload)
        .map_err(|e| DeviceError::Parse(format!("0x01 not UTF-8: {e}")))?;
    let v = parse_json(text).map_err(|e| DeviceError::Parse(format!("0x01 not JSON: {e}")))?;
    let num = |key: &str| -> Result<f64, DeviceError> {
        match v.get(key) {
            Some(Json::Num(n)) => Ok(*n),
            _ => Err(DeviceError::Parse(format!(
                "0x01 description missing number {key:?}"
            ))),
        }
    };
    let name = v
        .get("device")
        .and_then(Json::as_str)
        .unwrap_or("iPhone Simulator")
        .to_string();
    Ok(BaguetteDescription {
        device_name: name,
        width_points: num("width_points")? as f32,
        height_points: num("height_points")? as f32,
        width_pixels: num("width_pixels")? as u32,
        height_pixels: num("height_pixels")? as u32,
    })
}

/// A native baguette session: WebSocket video + persistent NDJSON input.
///
/// Constructed by [`BaguetteNative::connect`]. Like
/// [`crate::scrcpy_native::ScrcpyNative`], video and control are separate
/// channels; [`split`](BaguetteNative::split) hands out disjoint borrows so
/// one thread can pump frames while another sends input.
pub struct BaguetteNative {
    session_id: DeviceId,
    /// The `baguette serve` child we spawned, if we spawned one. `None`
    /// when serve was already listening.
    serve_child: Option<Child>,
    video: WsClient,
    input: InputChannel,
    description: BaguetteDescription,
    description_frame: Vec<u8>,
}

impl BaguetteNative {
    /// Connect to baguette's `serve` stream for `session_id` (the simulator
    /// UDID) and open the persistent input pipe.
    ///
    /// Gates: macOS host, Apple Silicon, `baguette` on PATH. Ensures
    /// `baguette serve` is listening on 127.0.0.1:8421 (spawning it when it
    /// isn't), opens `WS /devices/<udid>/stream?format=avcc`, reads the
    /// `0x01` description frame for the point/pixel geometry, then spawns
    /// the long-lived `baguette input` child.
    pub fn connect(session_id: &str) -> Result<Self, DeviceError> {
        Self::gates()?;
        let serve_child = Self::ensure_serve()?;

        let video = WsClient::connect(
            BAGUETTE_SERVE_HOST,
            BAGUETTE_SERVE_PORT,
            &stream_path(session_id),
        )
        .map_err(|e| match e {
            DeviceError::Parse(msg) => {
                DeviceError::Parse(format!("baguette serve stream {session_id}: {msg}"))
            }
            other => other,
        })?;
        video.set_read_timeout(Some(DESCRIPTION_TIMEOUT))?;

        // The stream's first unified-wire frame is the 0x01 description.
        let (description, description_frame) = loop {
            match video.read_message() {
                Ok(WsMessage::Data(frame)) => {
                    let validated = wire_from_baguette(&frame).map_err(|e| {
                        DeviceError::Parse(format!(
                            "baguette serve stream {session_id}: bad wire frame: {e}"
                        ))
                    })?;
                    if validated[0] == FRAME_DESCRIPTION {
                        let desc = description_from_json(&validated[1..])?;
                        break (desc, frame);
                    }
                    // Non-description frames before the description are
                    // dropped; the stream restarts each session.
                }
                Ok(WsMessage::Closed) => {
                    return Err(DeviceError::Parse(format!(
                        "baguette serve stream {session_id}: closed before description frame"
                    )));
                }
                Err(e) => return Err(e),
            }
        };
        video.set_read_timeout(Some(DEFAULT_VIDEO_READ_TIMEOUT))?;

        let input = InputChannel::spawn("baguette", &["input", "--udid", session_id])?;

        Ok(BaguetteNative {
            session_id: DeviceId(session_id.to_string()),
            serve_child,
            video,
            input,
            description,
            description_frame,
        })
    }

    fn gates() -> Result<(), DeviceError> {
        if !cfg!(target_os = "macos") {
            return Err(DeviceError::NotMacOSHost);
        }
        if !cfg!(target_arch = "aarch64") {
            return Err(DeviceError::Unsupported(
                "baguette requires Apple Silicon (arm64)".to_string(),
            ));
        }
        if !tool_on_path("baguette") {
            return Err(DeviceError::ToolMissing("baguette".to_string()));
        }
        Ok(())
    }

    /// If `baguette serve` is not listening on 127.0.0.1:8421, spawn it and
    /// wait for the port to accept connections. Returns the child when we
    /// spawned it (so `Drop` can tear it back down).
    fn ensure_serve() -> Result<Option<Child>, DeviceError> {
        let addr: SocketAddr = format!("{BAGUETTE_SERVE_HOST}:{BAGUETTE_SERVE_PORT}")
            .parse()
            .expect("loopback serve address parses");
        if TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok() {
            return Ok(None);
        }
        let mut child = Command::new("baguette")
            .arg("serve")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    DeviceError::ToolMissing("baguette".to_string())
                } else {
                    DeviceError::Io(e)
                }
            })?;
        let deadline = Instant::now() + SERVE_START_TIMEOUT;
        while Instant::now() < deadline {
            if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
                return Ok(Some(child));
            }
            if let Ok(Some(status)) = child.try_wait() {
                return Err(DeviceError::ToolFailed {
                    tool: "baguette serve".to_string(),
                    code: status.code(),
                    stderr: "serve exited before the port opened".to_string(),
                });
            }
            thread::sleep(Duration::from_millis(200));
        }
        let _ = child.kill();
        let _ = child.wait();
        Err(DeviceError::Timeout {
            tool: "baguette serve".to_string(),
        })
    }

    /// The simulator UDID this session is attached to.
    pub fn session_id(&self) -> &DeviceId {
        &self.session_id
    }

    /// Device name from the stream description (e.g. "iPhone 17 Pro").
    pub fn device_name(&self) -> &str {
        &self.description.device_name
    }

    /// Screen size in device points — the coordinate space for all input.
    pub fn video_size_points(&self) -> (f32, f32) {
        (
            self.description.width_points,
            self.description.height_points,
        )
    }

    /// Encoded video size in pixels.
    pub fn video_size_pixels(&self) -> (u32, u32) {
        (
            self.description.width_pixels,
            self.description.height_pixels,
        )
    }

    /// The raw `0x01` description wire frame, byte-identical to what the
    /// stream sent. Hand it to the Devices panel / `/farm` so iOS sessions
    /// bootstrap exactly like Android ones.
    pub fn description_frame(&self) -> &[u8] {
        &self.description_frame
    }

    /// Borrow the video client and the input channel disjointly: one thread
    /// can pump [`start_video_stream`](BaguetteNative::start_video_stream)
    /// frames while another sends touch.
    pub fn split(&mut self) -> (&WsClient, &mut InputChannel) {
        (&self.video, &mut self.input)
    }

    /// Adjust the video socket read timeout. Short timeouts let a caller
    /// poll for a frame with a deadline; a trial with no frame in time is
    /// a "miss", not a fatal error (same pattern as the Android path).
    pub fn set_video_read_timeout(&self, timeout: Option<Duration>) -> Result<(), DeviceError> {
        self.video.set_read_timeout(timeout)
    }

    /// Iterate validated unified-wire frames from the stream. Each payload
    /// is byte-identical to what baguette sent (validated passthrough via
    /// [`wire_from_baguette`]); iteration ends when the server closes.
    pub fn start_video_stream(
        &self,
    ) -> impl Iterator<Item = Result<BaguetteFrame, DeviceError>> + '_ {
        std::iter::from_fn(move || match self.video.read_message() {
            Ok(WsMessage::Data(frame)) => Some(
                wire_from_baguette(&frame)
                    .map(|validated| BaguetteFrame {
                        frame_type: validated[0],
                        payload: validated[1..].to_vec(),
                    })
                    .map_err(|e| {
                        DeviceError::Parse(format!("baguette serve: bad wire frame: {e}"))
                    }),
            ),
            Ok(WsMessage::Closed) => None,
            Err(e) => Some(Err(e)),
        })
    }

    fn dims(&self) -> (f32, f32) {
        self.video_size_points()
    }

    /// Tap at a device-point coordinate.
    pub fn tap(&mut self, p: DevicePoint) -> Result<(), DeviceError> {
        let (w, h) = self.dims();
        self.input.send(&encode_tap(p, w, h))
    }

    /// Swipe from one device-point coordinate to another.
    pub fn swipe(
        &mut self,
        from: DevicePoint,
        to: DevicePoint,
        duration_secs: f64,
    ) -> Result<(), DeviceError> {
        let (w, h) = self.dims();
        self.input
            .send(&encode_swipe(from, to, w, h, duration_secs))
    }

    /// Pinch around `center`: two fingers move from `start_span` apart to
    /// `end_span` apart over `steps` moves, then lift. `end_span` <
    /// `start_span` pinches in.
    pub fn pinch(
        &mut self,
        center: DevicePoint,
        start_span: f32,
        end_span: f32,
        steps: u32,
        duration_secs: f64,
    ) -> Result<(), DeviceError> {
        let (w, h) = self.dims();
        let at = |span: f32| -> (DevicePoint, DevicePoint) {
            let half = span / 2.0;
            (
                DevicePoint {
                    x: center.x - half,
                    y: center.y,
                },
                DevicePoint {
                    x: center.x + half,
                    y: center.y,
                },
            )
        };
        let (p1, p2) = at(start_span);
        self.input
            .send(&encode_touch2(TouchPhase::Down, p1, p2, w, h))?;
        let steps = steps.max(1);
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let span = start_span + (end_span - start_span) * t;
            let (p1, p2) = at(span);
            self.input
                .send(&encode_touch2(TouchPhase::Move, p1, p2, w, h))?;
        }
        let (p1, p2) = at(end_span);
        self.input
            .send(&encode_touch2(TouchPhase::Up, p1, p2, w, h))?;
        // Pacing is inherent: every pipe line waits for its ack, so the
        // pinch already unfolds in real time; `duration_secs` names the
        // intended gesture length for the caller.
        let _ = duration_secs;
        Ok(())
    }

    /// Press the HOME button.
    pub fn home(&mut self) -> Result<(), DeviceError> {
        self.input.send(&encode_button("home", None))
    }
}

impl Drop for BaguetteNative {
    fn drop(&mut self) {
        // InputChannel's own Drop kills the `baguette input` child.
        if let Some(child) = self.serve_child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// One validated video frame from [`BaguetteNative::start_video_stream`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaguetteFrame {
    /// Unified wire frame type (0x02 keyframe, 0x03 delta, 0x04 JPEG seed).
    pub frame_type: u8,
    /// Frame payload, byte-identical to what baguette sent.
    pub payload: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Tests: scripted fake `baguette serve` + stub `baguette input` children.
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "device"))]
mod tests {
    use super::*;
    use std::net::TcpListener;

    // -- hashing / handshake primitives ------------------------------------

    #[test]
    fn sha1_matches_known_vector() {
        let digest = sha1(b"abc");
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn ws_accept_key_matches_rfc6455_example() {
        // RFC 6455 §1.3 worked example.
        assert_eq!(
            ws_accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    // -- input protocol encoding --------------------------------------------

    #[test]
    fn encode_tap_matches_documented_wire_shape() {
        // The documented example, character for character.
        let line = encode_tap(DevicePoint { x: 219.0, y: 478.0 }, 438.0, 954.0);
        assert_eq!(
            line,
            r#"{"type":"tap","x":219,"y":478,"width":438,"height":954,"duration":0.05}"#
        );
    }

    #[test]
    fn device_point_coords_round_to_integer_points() {
        let line = encode_tap(DevicePoint { x: 219.4, y: 477.6 }, 438.0, 954.0);
        assert!(line.contains(r#""x":219,"y":478"#), "got {line}");
    }

    #[test]
    fn encode_home_button_shape() {
        assert_eq!(
            encode_button("home", None),
            r#"{"type":"button","button":"home"}"#
        );
        assert_eq!(
            encode_button("lock", Some(1.5)),
            r#"{"type":"button","button":"lock","duration":1.5}"#
        );
    }

    #[test]
    fn encode_swipe_shape() {
        let line = encode_swipe(
            DevicePoint { x: 200.0, y: 800.0 },
            DevicePoint { x: 200.0, y: 200.0 },
            438.0,
            954.0,
            0.3,
        );
        assert_eq!(
            line,
            r#"{"type":"swipe","startX":200,"startY":800,"endX":200,"endY":200,"width":438,"height":954,"duration":0.3}"#
        );
    }

    #[test]
    fn encode_touch_streams_use_phase_verbs_and_points() {
        let p = DevicePoint { x: 219.0, y: 478.0 };
        assert_eq!(
            encode_touch1(TouchPhase::Down, p, 438.0, 954.0, None),
            r#"{"type":"touch1-down","x":219,"y":478,"width":438,"height":954}"#
        );
        assert_eq!(
            encode_touch1(TouchPhase::Up, p, 438.0, 954.0, Some("bottom")),
            r#"{"type":"touch1-up","x":219,"y":478,"width":438,"height":954,"edge":"bottom"}"#
        );
        let p2 = DevicePoint { x: 263.0, y: 478.0 };
        assert_eq!(
            encode_touch2(TouchPhase::Move, p, p2, 438.0, 954.0),
            r#"{"type":"touch2-move","x1":219,"y1":478,"x2":263,"y2":478,"width":438,"height":954}"#
        );
    }

    #[test]
    fn encode_key_and_text_escape() {
        assert_eq!(encode_key("Enter"), r#"{"type":"key","code":"Enter"}"#);
        assert_eq!(
            encode_text("hi \"there\" \\ ok"),
            r#"{"type":"text","text":"hi \"there\" \\ ok"}"#
        );
    }

    // -- ack parsing ----------------------------------------------------------

    #[test]
    fn parse_ack_ok_true() {
        assert!(parse_ack("{\"ok\":true}\n").is_ok());
    }

    #[test]
    fn parse_ack_false_surfaces_error_text() {
        let err = parse_ack("{\"ok\":false,\"error\":\"no such gesture\"}\n").unwrap_err();
        assert!(err.contains("no such gesture"), "got {err}");
    }

    #[test]
    fn parse_ack_rejects_garbage() {
        assert!(parse_ack("not json\n").is_err());
        assert!(parse_ack("{\"ok\":\"yes\"}\n").is_err());
        assert!(parse_ack("{}\n").is_err());
    }

    #[test]
    fn description_from_json_extracts_points_and_pixels() {
        let payload = br#"{"device":"iPhone 17 Pro","width_points":402.0,"height_points":874.0,"width_pixels":1206,"height_pixels":2622}"#;
        let d = description_from_json(payload).expect("parses");
        assert_eq!(d.device_name, "iPhone 17 Pro");
        assert_eq!(d.width_points, 402.0);
        assert_eq!(d.height_points, 874.0);
        assert_eq!(d.width_pixels, 1206);
        assert_eq!(d.height_pixels, 2622);
    }

    // -- fake `baguette serve` --------------------------------------------------

    /// Build a server→client (unmasked) WebSocket frame.
    fn server_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
        let mut f = vec![0x80 | opcode];
        if payload.len() < 126 {
            f.push(payload.len() as u8);
        } else {
            f.push(126);
            f.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        f.extend_from_slice(payload);
        f
    }

    /// Read one client→server frame (masked) from the fake serve side and
    /// return `(opcode, unmasked_payload)`.
    fn read_client_frame(stream: &TcpStream) -> (u8, Vec<u8>) {
        let mut hdr = [0u8; 2];
        (&*stream)
            .read_exact(&mut hdr)
            .expect("client frame header");
        let opcode = hdr[0] & 0x0F;
        assert!(hdr[1] & 0x80 != 0, "client frames must be masked");
        let mut len = (hdr[1] & 0x7F) as usize;
        if len == 126 {
            let mut ext = [0u8; 2];
            (&*stream).read_exact(&mut ext).expect("ext len");
            len = u16::from_be_bytes(ext) as usize;
        }
        let mut mask = [0u8; 4];
        (&*stream).read_exact(&mut mask).expect("mask");
        let mut payload = vec![0u8; len];
        (&*stream).read_exact(&mut payload).expect("payload");
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= mask[i % 4];
        }
        (opcode, payload)
    }

    fn read_http_head(stream: &TcpStream) -> String {
        let mut head = Vec::new();
        let mut buf = [0u8; 512];
        loop {
            let n = (&*stream).read(&mut buf).expect("http head");
            assert!(n > 0, "EOF before HTTP head");
            head.extend_from_slice(&buf[..n]);
            if find_crlf_crlf(&head).is_some() {
                break;
            }
            assert!(head.len() < 65536, "head too large");
        }
        String::from_utf8_lossy(&head).into_owned()
    }

    struct FakeServe {
        port: u16,
        handle: thread::JoinHandle<()>,
    }

    /// Spawn a fake `baguette serve`: verifies the upgrade request path,
    /// computes the accept key from the client's nonce, answers a ping
    /// (asserting the client's pong is masked and echoes the payload), then
    /// serves scripted frames: a `0x01` description, a `0x02` keyframe,
    /// then close. Returns the port and a join handle.
    fn spawn_fake_serve() -> FakeServe {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .expect("timeout");
            let head = read_http_head(&stream);
            assert!(
                head.starts_with("GET /devices/sess-1/stream?format=avcc HTTP/1.1\r\n"),
                "unexpected request line: {}",
                head.lines().next().unwrap_or("")
            );
            let key = head
                .lines()
                .find_map(|l| {
                    l.split_once(':').and_then(|(n, v)| {
                        (n.trim().eq_ignore_ascii_case("sec-websocket-key"))
                            .then(|| v.trim().to_string())
                    })
                })
                .expect("Sec-WebSocket-Key present");
            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\n\
                 Upgrade: websocket\r\n\
                 Connection: Upgrade\r\n\
                 Sec-WebSocket-Accept: {}\r\n\
                 \r\n",
                ws_accept_key(&key)
            );
            (&stream).write_all(response.as_bytes()).expect("101");

            // Ping/pong: the client must answer with a masked pong echo.
            (&stream)
                .write_all(&server_frame(OP_PING, b"hb"))
                .expect("ping");
            let (opcode, payload) = read_client_frame(&stream);
            assert_eq!(opcode, OP_PONG, "client must pong a ping");
            assert_eq!(payload, b"hb", "pong must echo the ping payload");

            // 0x01 description then a 0x02 keyframe, then close.
            let desc_payload =
                br#"{"device":"iPhone 17 Pro","width_points":402.0,"height_points":874.0,"width_pixels":1206,"height_pixels":2622}"#;
            let mut desc_frame = vec![FRAME_DESCRIPTION];
            desc_frame.extend_from_slice(desc_payload);
            (&stream)
                .write_all(&server_frame(OP_BINARY, &desc_frame))
                .expect("desc");
            let keyframe: Vec<u8> = std::iter::once(0x02)
                .chain(b"\x00\x00\x00\x01FAKEKEY".iter().cloned())
                .collect();
            (&stream)
                .write_all(&server_frame(OP_BINARY, &keyframe))
                .expect("keyframe");
            (&stream)
                .write_all(&server_frame(OP_CLOSE, &[]))
                .expect("close");
            // Give the client a beat to read the close, then drop.
            thread::sleep(Duration::from_millis(500));
        });
        FakeServe { port, handle }
    }

    #[test]
    fn ws_client_upgrades_reads_frames_and_closes() {
        let serve = spawn_fake_serve();
        let client = WsClient::connect(BAGUETTE_SERVE_HOST, serve.port, &stream_path("sess-1"))
            .expect("handshake");

        // Description frame: validated passthrough, byte-identical.
        let desc = match client.read_message().expect("desc frame") {
            WsMessage::Data(f) => f,
            WsMessage::Closed => panic!("closed before description"),
        };
        let validated = wire_from_baguette(&desc).expect("valid wire frame");
        assert_eq!(validated[0], FRAME_DESCRIPTION);
        let d = description_from_json(&validated[1..]).expect("desc parses");
        assert_eq!(d.device_name, "iPhone 17 Pro");
        assert_eq!((d.width_points, d.height_points), (402.0, 874.0));

        // Keyframe: byte-identical payload.
        let kf = match client.read_message().expect("keyframe") {
            WsMessage::Data(f) => f,
            WsMessage::Closed => panic!("closed before keyframe"),
        };
        assert_eq!(kf[0], 0x02);
        assert_eq!(&kf[1..], b"\x00\x00\x00\x01FAKEKEY");

        // Then the server's close.
        assert_eq!(client.read_message().expect("close"), WsMessage::Closed);
        serve.handle.join().expect("fake serve thread");
    }

    #[test]
    fn ws_client_rejects_wrong_accept_key() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            read_http_head(&stream);
            (&stream)
                .write_all(
                    b"HTTP/1.1 101 Switching Protocols\r\n\
                      Upgrade: websocket\r\n\
                      Connection: Upgrade\r\n\
                      Sec-WebSocket-Accept: bogus\r\n\
                      \r\n",
                )
                .expect("101");
            thread::sleep(Duration::from_millis(300));
        });
        match WsClient::connect(BAGUETTE_SERVE_HOST, port, &stream_path("sess-1")) {
            Err(DeviceError::Parse(_)) => {}
            other => panic!("expected Parse error, got {:?}", other.is_ok()),
        }
        handle.join().expect("thread");
    }

    #[test]
    fn ws_client_rejects_masked_server_frame() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let head = read_http_head(&stream);
            let key = head
                .lines()
                .find_map(|l| {
                    l.split_once(':').and_then(|(n, v)| {
                        (n.trim().eq_ignore_ascii_case("sec-websocket-key"))
                            .then(|| v.trim().to_string())
                    })
                })
                .expect("key");
            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                ws_accept_key(&key)
            );
            (&stream).write_all(response.as_bytes()).expect("101");
            // Masked server frame: protocol violation.
            (&stream)
                .write_all(&[0x82, 0x80 | 3, 1, 2, 3, 4, 0xAA, 0xBB, 0xCC])
                .expect("masked frame");
            thread::sleep(Duration::from_millis(300));
        });
        let client = WsClient::connect(BAGUETTE_SERVE_HOST, port, &stream_path("sess-1"))
            .expect("handshake");
        let err = client.read_message().expect_err("masked frame must fail");
        assert!(
            matches!(err, DeviceError::Parse(_)),
            "expected Parse, got {err}"
        );
        handle.join().expect("thread");
    }

    // -- stub `baguette input` children ----------------------------------------

    /// A stub `sh` child that acks every input line with `{"ok":true}`.
    fn ack_ok_stub() -> InputChannel {
        InputChannel::spawn(
            "sh",
            &[
                "-c",
                "while IFS= read -r l; do printf '%s\\n' '{\"ok\":true}'; done",
            ],
        )
        .expect("stub spawns")
    }

    #[test]
    fn input_channel_roundtrip_with_stub() {
        let mut input = ack_ok_stub();
        input
            .send(&encode_tap(
                DevicePoint { x: 219.0, y: 478.0 },
                438.0,
                954.0,
            ))
            .expect("tap acked");
        let mut pinch = ack_ok_stub();
        pinch
            .send(&encode_button("home", None))
            .expect("home acked");
    }

    #[test]
    fn input_channel_surfaces_rejection() {
        let mut input = InputChannel::spawn(
            "sh",
            &[
                "-c",
                "while IFS= read -r l; do printf '%s\\n' '{\"ok\":false,\"error\":\"no such gesture\"}'; done",
            ],
        )
        .expect("stub spawns");
        let err = input
            .send(&encode_tap(DevicePoint { x: 1.0, y: 1.0 }, 438.0, 954.0))
            .expect_err("rejection must surface");
        match err {
            DeviceError::ToolFailed { tool, stderr, .. } => {
                assert_eq!(tool, "baguette input");
                assert!(stderr.contains("no such gesture"), "got {stderr}");
            }
            other => panic!("expected ToolFailed, got {other}"),
        }
    }

    #[test]
    fn input_channel_child_exiting_is_an_error_not_a_hang() {
        // `true` exits immediately: stdout EOF must surface as an error.
        // Which error is a race (EPIPE on the write vs EOF on the ack
        // read); the contract is only that it errors instead of hanging.
        let mut input = InputChannel::spawn("true", &[]).expect("stub spawns");
        assert!(
            input.send(&encode_button("home", None)).is_err(),
            "exited child must not ack"
        );
    }

    // -- gates -------------------------------------------------------------------

    #[test]
    fn connect_on_non_macos_reports_not_macos_host() {
        if cfg!(target_os = "macos") {
            return; // real path needs a simulator; covered by the fake-serve tests
        }
        assert!(
            matches!(
                BaguetteNative::connect("sess-1"),
                Err(DeviceError::NotMacOSHost)
            ),
            "expected NotMacOSHost"
        );
    }
}
