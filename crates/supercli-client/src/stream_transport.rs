//! Pure protocol/selection logic for the terminal output WebSocket exposed
//! by the Mac's `supercli-host __remote__` server.
//!
//! Port of `RemoteTerminalStreamTransport.swift` (`ios/SupercliIOS`) plus the
//! pure helpers co-located with the view: `RemoteTerminalReconnectBackoff`
//! and `RemoteTerminalInputTracker`. Covers hello/error frame decoding,
//! binary frame parsing (8-byte big-endian output.bin offset prefix),
//! certificate fingerprint normalization, the transport-selection decision
//! (WS candidate vs. the HTTP long-poll fallback), client input framing, and
//! reconnect backoff.
//!
//! Deliberately socket-free and side-effect-free. The tiny base64 decode and
//! query-value escaping are hand-rolled so this module stays in the web-safe
//! subset (no platform crates).

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Discovery / transport selection
// ---------------------------------------------------------------------------

/// The advertised `supercli-host __remote__` endpoint, as discovered from the
/// latest bootstrap/pairing response. The port is OS-assigned per server run
/// (never cache it across reconnects — always read the freshest value); the
/// fingerprint is stable across restarts and already normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteServerEndpoint {
    pub port: u16,
    /// Normalized lowercase hex SHA-256 of the TLS leaf certificate DER.
    pub certificate_fingerprint: String,
}

/// Everything needed for one WS connect attempt: host from the paired mobile
/// endpoint, port + pin from discovery, the phone's paired bearer token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTerminalWebSocketCandidate {
    pub host: String,
    pub port: u16,
    pub certificate_fingerprint: String,
    pub token: String,
}

pub struct RemoteTerminalTransportSelector;

impl RemoteTerminalTransportSelector {
    /// Strict-but-liberal fingerprint normalization: strips an optional
    /// `sha256:` prefix, colons, and whitespace, lowercases, and requires
    /// exactly 64 hex characters. Anything else is unusable for pinning —
    /// and no pin means no WS (there is deliberately no bypass).
    pub fn normalized_fingerprint(raw: Option<&str>) -> Option<String> {
        let raw = raw?;
        let mut value: String = raw
            .trim()
            .to_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if let Some(stripped) = value.strip_prefix("sha256:") {
            value = stripped.to_string();
        }
        value.retain(|c| c != ':');
        if value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(value)
        } else {
            None
        }
    }

    pub fn fingerprints_match(a: Option<&str>, b: Option<&str>) -> bool {
        match (
            Self::normalized_fingerprint(a),
            Self::normalized_fingerprint(b),
        ) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    }

    /// Discovery: a usable remote-server endpoint requires both a live port
    /// and a valid fingerprint. The dev bridge advertises neither, so it
    /// always resolves `None` — the HTTP long-poll stays its only transport.
    pub fn endpoint(port: Option<u64>, fingerprint: Option<&str>) -> Option<RemoteServerEndpoint> {
        let port = port?;
        if port == 0 || port > 65_535 {
            return None;
        }
        let normalized = Self::normalized_fingerprint(fingerprint)?;
        Some(RemoteServerEndpoint {
            port: port as u16,
            certificate_fingerprint: normalized,
        })
    }

    /// The transport decision for one stream attempt: WS when the Mac
    /// advertises the remote server AND we hold a paired token to present;
    /// `None` means HTTP long-poll (dev bridge, server down, or pre-WS build).
    pub fn candidate(
        endpoint: Option<RemoteServerEndpoint>,
        base_url: &str,
        auth_token: Option<&str>,
    ) -> Option<RemoteTerminalWebSocketCandidate> {
        let endpoint = endpoint?;
        let host = host_from_url(base_url)?;
        if host.is_empty() {
            return None;
        }
        let token = auth_token?;
        if token.is_empty() {
            return None;
        }
        Some(RemoteTerminalWebSocketCandidate {
            host,
            port: endpoint.port,
            certificate_fingerprint: endpoint.certificate_fingerprint,
            token: token.to_string(),
        })
    }

    /// `wss://<host>:<port>/api/sessions/<id>/output?token=...[&offset=N]`
    pub fn web_socket_output_url(
        host: &str,
        port: u16,
        session_id: &str,
        token: &str,
        offset: Option<u64>,
    ) -> Option<String> {
        if host.is_empty() || session_id.is_empty() {
            return None;
        }
        let mut url = format!(
            "wss://{host}:{port}/api/sessions/{}/output?token={}",
            percent_encode_path_segment(session_id),
            percent_encode_query_value(token)
        );
        if let Some(offset) = offset {
            url.push_str(&format!("&offset={offset}"));
        }
        Some(url)
    }
}

fn host_from_url(url: &str) -> Option<String> {
    let after_scheme = url.split("://").nth(1)?;
    let authority = after_scheme.split('/').next()?;
    let authority = authority.rsplit('@').next()?; // strip userinfo if present
    let host = authority.split(':').next()?;
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// Percent-encode everything outside the unreserved set, matching the
/// query-value escaping URLComponents applies to the token.
fn percent_encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn percent_encode_path_segment(value: &str) -> String {
    percent_encode_query_value(value)
}

// ---------------------------------------------------------------------------
// Server → client frames
// ---------------------------------------------------------------------------

/// The server's first text frame on a successful upgrade.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RemoteTerminalWsHello {
    #[serde(rename = "protocol")]
    pub protocol_version: u64,
    #[serde(rename = "session_id")]
    pub session_id: String,
    pub state: String,
    /// Total output.bin size at connect time.
    #[serde(rename = "output_size")]
    pub output_size: u64,
    /// The offset the client asked for (None on a fresh connect).
    #[serde(rename = "requested_offset")]
    pub requested_offset: Option<u64>,
    /// Where the binary stream actually begins.
    #[serde(rename = "start_offset")]
    pub start_offset: u64,
    /// True when the requested offset was unusable (beyond the file or too
    /// far behind the tail) and the server restarted from an aligned tail —
    /// the client must clear before feeding, like an HTTP rebase.
    pub rebased: bool,
    pub cols: Option<u64>,
    pub rows: Option<u64>,
    /// DEC-mode restore preamble (base64) the client feeds into its freshly
    /// reset VT before the replayed tail — the mouse-tracking / alt-screen
    /// sequences that scrolled out of the retained journal. Absent from
    /// older Hosts and at the session origin. Not journal bytes: it never
    /// moves `start_offset` or the resume cursor.
    #[serde(rename = "mode_preamble_base64")]
    pub mode_preamble_base64: Option<String>,
}

impl RemoteTerminalWsHello {
    pub fn mode_preamble(&self) -> Option<Vec<u8>> {
        let encoded = self.mode_preamble_base64.as_ref()?;
        let bytes = base64_standard_decode(encoded);
        if bytes.is_empty() {
            None
        } else {
            Some(bytes)
        }
    }
}

/// Server→client text frames: the hello, or non-fatal in-stream errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteTerminalWsServerMessage {
    Hello(RemoteTerminalWsHello),
    Error(String),
    Unknown,
}

impl RemoteTerminalWsServerMessage {
    pub fn parse(text: &str) -> Self {
        let value: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => return Self::Unknown,
        };
        let obj = match value.as_object() {
            Some(o) => o,
            None => return Self::Unknown,
        };
        let frame_type = match obj.get("type").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => return Self::Unknown,
        };
        match frame_type {
            "hello" => match serde_json::from_value::<RemoteTerminalWsHello>(value) {
                Ok(hello) => Self::Hello(hello),
                Err(_) => Self::Unknown,
            },
            "error" => {
                let message = obj
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                Self::Error(message.to_string())
            }
            _ => Self::Unknown,
        }
    }
}

/// Server→client binary frame: bytes 0-7 are the big-endian u64 output.bin
/// offset of the first payload byte, the rest is raw terminal bytes (no
/// base64). `offset + payload.len()` is the resume offset — the same offset
/// space the HTTP long-poll uses, so the two transports are interchangeable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTerminalWsBinaryFrame<'a> {
    pub offset: u64,
    pub payload: &'a [u8],
}

impl<'a> RemoteTerminalWsBinaryFrame<'a> {
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }
        let mut offset: u64 = 0;
        for &byte in &data[..8] {
            offset = (offset << 8) | u64::from(byte);
        }
        Some(Self {
            offset,
            payload: &data[8..],
        })
    }
}

// ---------------------------------------------------------------------------
// Client → server frames
// ---------------------------------------------------------------------------

pub struct RemoteTerminalWsClientMessage;

impl RemoteTerminalWsClientMessage {
    /// The server caps one input message's data at 64KB; chunk well under it
    /// (on character boundaries, so escape sequences and multi-byte UTF-8
    /// never split mid-scalar — a str iterates whole characters).
    pub const MAX_INPUT_BYTES_PER_FRAME: usize = 32 * 1024;

    /// One raw-PTY input as one or more `{"type":"input","data":...}` frames,
    /// in order. No ack is expected — the echo arrives via output.
    ///
    /// `write_id` is the idempotency key the caller also sends on the HTTP
    /// fallback for this same logical send. It is attached only when the send
    /// fits in one frame. The live input router sends larger values directly
    /// over HTTP, avoiding an ambiguous partial multi-frame WS delivery.
    pub fn input_frames(text: &str, write_id: Option<&str>, max_bytes: usize) -> Vec<String> {
        if text.is_empty() {
            return Vec::new();
        }
        if text.len() <= max_bytes {
            return vec![Self::encode_input(text, write_id)];
        }
        let mut frames = Vec::new();
        let mut chunk = String::new();
        let mut chunk_bytes = 0usize;
        for character in text.chars() {
            let size = character.len_utf8();
            if chunk_bytes + size > max_bytes && !chunk.is_empty() {
                frames.push(Self::encode_input(&chunk, None));
                chunk.clear();
                chunk_bytes = 0;
            }
            chunk.push(character);
            chunk_bytes += size;
        }
        if !chunk.is_empty() {
            frames.push(Self::encode_input(&chunk, None));
        }
        frames
    }

    fn encode_input(data: &str, write_id: Option<&str>) -> String {
        let wid = write_id.filter(|w| !w.is_empty());
        let payload = serde_json::json!({
            "type": "input",
            "data": data,
            "wid": wid,
        });
        payload.to_string()
    }
}

// ---------------------------------------------------------------------------
// Reconnect backoff
// ---------------------------------------------------------------------------

/// Exponential reconnect backoff (nanoseconds): 500ms → 1s → 2s … capped at
/// 8s. A successful paint (new `healthy_serial`) resets the streak, so a
/// stream that ran normally for minutes never inherits a stale delay.
#[derive(Debug, Clone)]
pub struct RemoteTerminalReconnectBackoff {
    next_delay_ns: u64,
    last_healthy_serial: u64,
}

impl RemoteTerminalReconnectBackoff {
    pub const INITIAL_DELAY_NS: u64 = 500_000_000;
    pub const MAXIMUM_DELAY_NS: u64 = 8_000_000_000;

    pub fn new(healthy_serial: u64) -> Self {
        Self {
            next_delay_ns: Self::INITIAL_DELAY_NS,
            last_healthy_serial: healthy_serial,
        }
    }

    pub fn delay_after_failure(&mut self, healthy_serial: u64) -> u64 {
        if healthy_serial != self.last_healthy_serial {
            self.last_healthy_serial = healthy_serial;
            self.next_delay_ns = Self::INITIAL_DELAY_NS;
        }
        let delay = self.next_delay_ns;
        self.next_delay_ns = self
            .next_delay_ns
            .saturating_mul(2)
            .min(Self::MAXIMUM_DELAY_NS);
        delay
    }
}

// ---------------------------------------------------------------------------
// Input follow tracker
// ---------------------------------------------------------------------------

/// Estimates the caret column from raw typed bytes so the composer can be
/// kept visible while typing a long line. The follow hint is coarse: after
/// the composer is long, one event every eight columns avoids per-byte work.
pub struct RemoteTerminalInputTracker {
    pub on_follow: Option<Box<dyn Fn(i32) + Send + Sync>>,
    estimated_column: i32,
    last_notified_column: i32,
}

impl RemoteTerminalInputTracker {
    const LONG_LINE_THRESHOLD: i32 = 48;
    const FOLLOW_STEP: i32 = 8;

    pub fn new() -> Self {
        Self {
            on_follow: None,
            estimated_column: 0,
            last_notified_column: 0,
        }
    }

    pub fn record(&mut self, data: &[u8]) {
        let mut ended_line = false;
        for &byte in data {
            match byte {
                10 | 13 => {
                    self.estimated_column = 0;
                    ended_line = true;
                }
                8 | 127 => {
                    self.estimated_column = self.estimated_column.saturating_sub(1);
                }
                0..32 => {}
                _ => {
                    self.estimated_column = self.estimated_column.saturating_add(1);
                }
            }
        }
        let column = self.estimated_column;
        let crossed_long_line_threshold = column >= Self::LONG_LINE_THRESHOLD
            && self.last_notified_column < Self::LONG_LINE_THRESHOLD;
        let moved_one_follow_step = column >= Self::LONG_LINE_THRESHOLD
            && (column - self.last_notified_column).abs() >= Self::FOLLOW_STEP;
        let should_notify = ended_line || crossed_long_line_threshold || moved_one_follow_step;
        if should_notify {
            self.last_notified_column = column;
            if let Some(on_follow) = &self.on_follow {
                on_follow(column);
            }
        }
    }
}

impl Default for RemoteTerminalInputTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Minimal standard-base64 decode (web-safe subset; no platform crates)
// ---------------------------------------------------------------------------

fn base64_standard_decode(input: &str) -> Vec<u8> {
    fn value(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut sextets = Vec::with_capacity(input.len());
    let mut padding = 0usize;
    for &byte in input.as_bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            padding += 1;
            continue;
        }
        match value(byte) {
            Some(v) => sextets.push(v),
            None => return Vec::new(),
        }
    }
    let mut out = Vec::with_capacity(sextets.len() * 3 / 4);
    let (chunks, remainder) = sextets.as_chunks::<4>();
    for chunk in chunks {
        let n = (u32::from(chunk[0]) << 18)
            | (u32::from(chunk[1]) << 12)
            | (u32::from(chunk[2]) << 6)
            | u32::from(chunk[3]);
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
    }
    match remainder {
        [] => {}
        [a, b] if padding >= 2 => {
            let n = (u32::from(*a) << 18) | (u32::from(*b) << 12);
            out.push((n >> 16) as u8);
        }
        [a, b, c] if padding >= 1 => {
            let n = (u32::from(*a) << 18) | (u32::from(*b) << 12) | (u32::from(*c) << 6);
            out.push((n >> 16) as u8);
            out.push((n >> 8) as u8);
        }
        _ => return Vec::new(),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // --- Hello frame ---

    #[test]
    fn hello_decodes_full_spec_frame() {
        let text = r#"{"type":"hello","protocol":1,"session_id":"sess-1","state":"running","output_size":123456,"requested_offset":1024,"start_offset":1024,"rebased":false,"cols":204,"rows":58}"#;
        let message = RemoteTerminalWsServerMessage::parse(text);
        let RemoteTerminalWsServerMessage::Hello(hello) = message else {
            panic!("expected hello");
        };
        assert_eq!(hello.protocol_version, 1);
        assert_eq!(hello.session_id, "sess-1");
        assert_eq!(hello.state, "running");
        assert_eq!(hello.output_size, 123_456);
        assert_eq!(hello.requested_offset, Some(1024));
        assert_eq!(hello.start_offset, 1024);
        assert!(!hello.rebased);
        assert_eq!(hello.cols, Some(204));
        assert_eq!(hello.rows, Some(58));
    }

    #[test]
    fn hello_carries_optional_mode_preamble() {
        // base64 of "\u{1B}[?1049h\u{1B}[?1000h\u{1B}[?1006h"
        let restore = "G1s/MTA0OWgbWz8xMDAwaBtbPzEwMDZo";
        let text = format!(
            r#"{{"type":"hello","protocol":1,"session_id":"sess-1","state":"running","output_size":9000,"requested_offset":null,"start_offset":4096,"rebased":false,"cols":80,"rows":24,"mode_preamble_base64":"{restore}"}}"#
        );
        let message = RemoteTerminalWsServerMessage::parse(&text);
        let RemoteTerminalWsServerMessage::Hello(hello) = message else {
            panic!("expected hello");
        };
        assert_eq!(
            hello.mode_preamble().as_deref(),
            Some(b"\x1B[?1049h\x1B[?1000h\x1B[?1006h".as_slice())
        );
        // The preamble is not journal bytes: the stream still starts where
        // the Host said it does.
        assert_eq!(hello.start_offset, 4096);
    }

    #[test]
    fn hello_without_preamble_field_decodes_as_none() {
        let text = r#"{"type":"hello","protocol":1,"session_id":"sess-1","state":"running","output_size":10,"requested_offset":null,"start_offset":0,"rebased":false,"cols":null,"rows":null}"#;
        let message = RemoteTerminalWsServerMessage::parse(text);
        let RemoteTerminalWsServerMessage::Hello(hello) = message else {
            panic!("expected hello");
        };
        assert_eq!(hello.mode_preamble(), None);
    }

    #[test]
    fn hello_decodes_null_offset_and_missing_grid() {
        let text = r#"{"type":"hello","protocol":1,"session_id":"sess-2","state":"running","output_size":9000,"requested_offset":null,"start_offset":8704,"rebased":true,"cols":null,"rows":null}"#;
        let message = RemoteTerminalWsServerMessage::parse(text);
        let RemoteTerminalWsServerMessage::Hello(hello) = message else {
            panic!("expected hello");
        };
        assert_eq!(hello.requested_offset, None);
        assert_eq!(hello.start_offset, 8704);
        assert!(hello.rebased);
        assert_eq!(hello.cols, None);
        assert_eq!(hello.rows, None);
    }

    #[test]
    fn parse_error_frame() {
        assert_eq!(
            RemoteTerminalWsServerMessage::parse(r#"{"type":"error","message":"host went away"}"#),
            RemoteTerminalWsServerMessage::Error("host went away".to_string())
        );
    }

    #[test]
    fn parse_error_frame_without_message_field() {
        assert_eq!(
            RemoteTerminalWsServerMessage::parse(r#"{"type":"error"}"#),
            RemoteTerminalWsServerMessage::Error("unknown error".to_string())
        );
    }

    #[test]
    fn parse_unknown_type_and_garbage() {
        assert_eq!(
            RemoteTerminalWsServerMessage::parse(r#"{"type":"future-thing"}"#),
            RemoteTerminalWsServerMessage::Unknown
        );
        assert_eq!(
            RemoteTerminalWsServerMessage::parse("not json"),
            RemoteTerminalWsServerMessage::Unknown
        );
        assert_eq!(
            RemoteTerminalWsServerMessage::parse("[1,2,3]"),
            RemoteTerminalWsServerMessage::Unknown
        );
        // A hello missing required fields must not decode into a bogus hello.
        assert_eq!(
            RemoteTerminalWsServerMessage::parse(r#"{"type":"hello"}"#),
            RemoteTerminalWsServerMessage::Unknown
        );
    }

    // --- Binary frames ---

    #[test]
    fn binary_frame_parses_big_endian_offset_and_payload() {
        let mut data = vec![0, 0, 0, 0, 0, 0, 0x01, 0x02]; // 258
        data.extend_from_slice(b"hello");
        let frame = RemoteTerminalWsBinaryFrame::parse(&data).expect("frame");
        assert_eq!(frame.offset, 258);
        assert_eq!(frame.payload, b"hello");
    }

    #[test]
    fn binary_frame_parses_large_offset() {
        // 0x0000_0001_0000_0000 = 4 GiB — past the u32 boundary.
        let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0];
        data.push(0x41);
        let frame = RemoteTerminalWsBinaryFrame::parse(&data).expect("frame");
        assert_eq!(frame.offset, 4_294_967_296);
        assert_eq!(frame.payload.len(), 1);
    }

    #[test]
    fn binary_frame_with_empty_payload_parses() {
        let frame = RemoteTerminalWsBinaryFrame::parse(&[0, 0, 0, 0, 0, 0, 0, 42]).expect("frame");
        assert_eq!(frame.offset, 42);
        assert_eq!(frame.payload.len(), 0);
    }

    #[test]
    fn binary_frame_shorter_than_header_is_rejected() {
        assert!(RemoteTerminalWsBinaryFrame::parse(&[1, 2, 3]).is_none());
        assert!(RemoteTerminalWsBinaryFrame::parse(&[]).is_none());
    }

    #[test]
    fn binary_frame_parses_from_nonzero_based_slice() {
        let mut padded = vec![0xFF, 0xFF];
        padded.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 7]);
        padded.extend_from_slice(b"x");
        let frame = RemoteTerminalWsBinaryFrame::parse(&padded[2..]).expect("frame");
        assert_eq!(frame.offset, 7);
        assert_eq!(frame.payload, b"x");
    }

    // --- Fingerprint normalization ---

    #[test]
    fn fingerprint_normalization_lowercases_and_strips_decoration() {
        let plain: String = "ab12".repeat(16); // 64 hex chars
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some(&plain.to_uppercase())),
            Some(plain.clone())
        );
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some(&format!("  {plain}\n"))),
            Some(plain.clone())
        );
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some(&format!(
                "sha256:{plain}"
            ))),
            Some(plain.clone())
        );
        // Colon-separated hex pairs (openssl-style) normalize too.
        let colonized = plain
            .as_bytes()
            .chunks(2)
            .map(|pair| std::str::from_utf8(pair).unwrap())
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some(&colonized)),
            Some(plain)
        );
    }

    #[test]
    fn fingerprint_normalization_rejects_invalid_input() {
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(None),
            None
        );
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some("")),
            None
        );
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some("abcd")),
            None
        ); // too short
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some(&"g".repeat(64))),
            None
        ); // not hex
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some(&"a".repeat(65))),
            None
        ); // wrong length
    }

    #[test]
    fn fingerprints_match_is_case_insensitive() {
        let plain = "0f".repeat(32);
        assert!(RemoteTerminalTransportSelector::fingerprints_match(
            Some(&plain),
            Some(&plain.to_uppercase())
        ));
        assert!(!RemoteTerminalTransportSelector::fingerprints_match(
            Some(&plain),
            Some(&"0e".repeat(32))
        ));
        assert!(!RemoteTerminalTransportSelector::fingerprints_match(
            Some(&plain),
            None
        ));
    }

    // --- Transport selection ---

    #[test]
    fn endpoint_requires_port_and_valid_fingerprint() {
        let valid_fingerprint = "ab".repeat(32);
        assert!(
            RemoteTerminalTransportSelector::endpoint(None, Some(&valid_fingerprint)).is_none()
        );
        assert!(RemoteTerminalTransportSelector::endpoint(Some(50123), None).is_none());
        assert!(RemoteTerminalTransportSelector::endpoint(Some(50123), Some("bogus")).is_none());
        assert!(
            RemoteTerminalTransportSelector::endpoint(Some(0), Some(&valid_fingerprint)).is_none()
        );
        let endpoint = RemoteTerminalTransportSelector::endpoint(
            Some(50123),
            Some(&valid_fingerprint.to_uppercase()),
        )
        .expect("endpoint");
        assert_eq!(endpoint.port, 50123);
        assert_eq!(endpoint.certificate_fingerprint, valid_fingerprint);
    }

    #[test]
    fn candidate_requires_endpoint_host_and_token() {
        let valid_fingerprint = "ab".repeat(32);
        let endpoint = RemoteServerEndpoint {
            port: 50123,
            certificate_fingerprint: valid_fingerprint.clone(),
        };
        let base_url = "http://192.168.1.20:17661/mobile";

        // No endpoint (dev bridge / server down / pre-WS Mac) → HTTP only.
        assert!(RemoteTerminalTransportSelector::candidate(None, base_url, Some("tok")).is_none());
        // No token → HTTP only.
        assert!(
            RemoteTerminalTransportSelector::candidate(Some(endpoint.clone()), base_url, None)
                .is_none()
        );
        assert!(RemoteTerminalTransportSelector::candidate(
            Some(endpoint.clone()),
            base_url,
            Some("")
        )
        .is_none());

        let candidate =
            RemoteTerminalTransportSelector::candidate(Some(endpoint), base_url, Some("tok"))
                .expect("candidate");
        assert_eq!(candidate.host, "192.168.1.20");
        assert_eq!(candidate.port, 50123);
        assert_eq!(candidate.certificate_fingerprint, valid_fingerprint);
        assert_eq!(candidate.token, "tok");
    }

    #[test]
    fn web_socket_output_url_with_resume_offset() {
        let url = RemoteTerminalTransportSelector::web_socket_output_url(
            "192.168.1.20",
            50123,
            "sess-abc",
            "tok",
            Some(42),
        )
        .expect("url");
        assert_eq!(
            url,
            "wss://192.168.1.20:50123/api/sessions/sess-abc/output?token=tok&offset=42"
        );
    }

    #[test]
    fn web_socket_output_url_without_offset() {
        let url = RemoteTerminalTransportSelector::web_socket_output_url(
            "mac.local",
            1234,
            "s1",
            "tok",
            None,
        )
        .expect("url");
        assert_eq!(url, "wss://mac.local:1234/api/sessions/s1/output?token=tok");
    }

    #[test]
    fn web_socket_output_url_rejects_empty_host_or_session() {
        assert!(
            RemoteTerminalTransportSelector::web_socket_output_url("", 1, "s", "t", None).is_none()
        );
        assert!(
            RemoteTerminalTransportSelector::web_socket_output_url("h", 1, "", "t", None).is_none()
        );
    }

    // --- Client input frames ---

    fn decode_frame(frame: &str) -> serde_json::Value {
        serde_json::from_str(frame).expect("frame JSON")
    }

    #[test]
    fn input_frame_encodes_single_small_message() {
        let frames = RemoteTerminalWsClientMessage::input_frames(
            "ls -la\r",
            Some("write-123"),
            RemoteTerminalWsClientMessage::MAX_INPUT_BYTES_PER_FRAME,
        );
        assert_eq!(frames.len(), 1);
        let decoded = decode_frame(&frames[0]);
        assert_eq!(decoded["type"], "input");
        assert_eq!(decoded["data"], "ls -la\r");
        assert_eq!(decoded["wid"], "write-123");
    }

    #[test]
    fn input_frame_preserves_control_and_escape_bytes() {
        let text = "\u{1B}[200~pasted\u{1B}[201~\u{3}";
        let frames = RemoteTerminalWsClientMessage::input_frames(
            text,
            None,
            RemoteTerminalWsClientMessage::MAX_INPUT_BYTES_PER_FRAME,
        );
        assert_eq!(frames.len(), 1);
        assert_eq!(decode_frame(&frames[0])["data"], text);
    }

    #[test]
    fn input_frames_chunk_large_input_on_character_boundaries() {
        // 3-byte character: a naive byte split would shear it mid-scalar.
        let text = "€".repeat(100);
        let frames = RemoteTerminalWsClientMessage::input_frames(text.as_str(), None, 32);
        assert!(frames.len() > 1);
        let mut joined = String::new();
        for frame in &frames {
            let decoded = decode_frame(frame);
            assert_eq!(decoded["type"], "input");
            let data = decoded["data"].as_str().unwrap();
            assert!(data.len() <= 32);
            assert!(decoded.get("wid").map_or(true, |w| w.is_null()));
            joined.push_str(data);
        }
        assert_eq!(joined, text);
    }

    #[test]
    fn input_frames_empty_input_produces_no_frames() {
        assert!(RemoteTerminalWsClientMessage::input_frames(
            "",
            None,
            RemoteTerminalWsClientMessage::MAX_INPUT_BYTES_PER_FRAME
        )
        .is_empty());
    }

    // --- Reconnect backoff ---

    #[test]
    fn reconnect_backoff_resets_after_a_frame_paints() {
        let mut backoff = RemoteTerminalReconnectBackoff::new(0);
        assert_eq!(backoff.delay_after_failure(0), 500_000_000);
        assert_eq!(backoff.delay_after_failure(0), 1_000_000_000);
        assert_eq!(backoff.delay_after_failure(0), 2_000_000_000);

        // A later stream painted successfully, so its first reconnect must
        // be prompt rather than inheriting the old failure streak.
        assert_eq!(backoff.delay_after_failure(1), 500_000_000);
        assert_eq!(backoff.delay_after_failure(1), 1_000_000_000);
    }

    #[test]
    fn reconnect_backoff_caps_without_healthy_output() {
        let mut backoff = RemoteTerminalReconnectBackoff::new(0);
        let delays: Vec<u64> = (0..8).map(|_| backoff.delay_after_failure(0)).collect();
        assert_eq!(
            delays,
            [
                500_000_000,
                1_000_000_000,
                2_000_000_000,
                4_000_000_000,
                8_000_000_000,
                8_000_000_000,
                8_000_000_000,
                8_000_000_000,
            ]
        );
    }

    // --- Input tracker ---

    #[test]
    fn input_follow_hints_are_bounded_after_long_line_threshold() {
        let followed: Arc<Mutex<Vec<i32>>> = Arc::new(Mutex::new(Vec::new()));
        let followed_clone = followed.clone();
        let mut tracker = RemoteTerminalInputTracker::new();
        tracker.on_follow = Some(Box::new(move |column| {
            followed_clone.lock().unwrap().push(column);
        }));

        tracker.record(&vec![b'a'; 47]);
        assert!(followed.lock().unwrap().is_empty());

        tracker.record(b"a");
        assert_eq!(*followed.lock().unwrap(), vec![48]);

        tracker.record(&vec![b'a'; 7]);
        assert_eq!(*followed.lock().unwrap(), vec![48]);
        tracker.record(b"a");
        assert_eq!(*followed.lock().unwrap(), vec![48, 56]);

        tracker.record(&vec![127; 8]);
        assert_eq!(*followed.lock().unwrap(), vec![48, 56, 48]);
        tracker.record(&[13]);
        assert_eq!(*followed.lock().unwrap(), vec![48, 56, 48, 0]);
    }
}
