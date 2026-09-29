//! Pure protocol/selection logic for the terminal output WebSocket exposed
//! by the Host's `__remote__` server: hello/error frame decoding, binary
//! frame parsing (8-byte big-endian output.bin offset prefix), certificate
//! fingerprint normalization, and the transport-selection decision (WS
//! candidate vs. the HTTP long-poll fallback).
//!
//! Ported from
//! `clients/legacy/ios/SupercliIOS/Sources/SupercliIOS/RemoteTerminalStreamTransport.swift`.
//!
//! Deliberately socket-free and side-effect-free so every piece is unit
//! testable; the live connection lives in the platform layer.

use serde::{Deserialize, Serialize};

/// The advertised `__remote__` endpoint, as discovered from the latest
/// bootstrap/pairing response. The port is OS-assigned per server run
/// (never cache it across reconnects — always read the freshest value);
/// the fingerprint is stable across restarts and already normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteServerEndpoint {
    pub port: u16,
    /// Normalized lowercase hex SHA-256 of the TLS leaf certificate DER.
    pub certificate_fingerprint: String,
}

/// Everything needed for one WS connect attempt: host from the paired
/// mobile endpoint, port + pin from discovery, the paired bearer token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTerminalWebSocketCandidate {
    pub host: String,
    pub port: u16,
    pub certificate_fingerprint: String,
    pub token: String,
}

/// Strict-but-liberal fingerprint normalization: strips an optional
/// `sha256:` prefix, colons, and whitespace, lowercases, and requires
/// exactly 64 hex characters. Anything else is unusable for pinning —
/// and no pin means no WS (there is deliberately no bypass).
///
/// This is stricter than [`crate::tls::normalize_fingerprint`] (which only
/// trims and lowercases for the Direct `/mobile` pin): the WS handshake
/// needs a full SHA-256 to compare against.
pub fn normalize_terminal_fingerprint(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let mut value = raw.trim().to_lowercase();
    if let Some(stripped) = value.strip_prefix("sha256:") {
        value = stripped.to_string();
    }
    value = value.replace(':', "");
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(value)
}

/// Case-insensitive fingerprint equality on the normalized forms.
pub fn terminal_fingerprints_match(a: Option<&str>, b: Option<&str>) -> bool {
    match (
        normalize_terminal_fingerprint(a),
        normalize_terminal_fingerprint(b),
    ) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// Discovery: a usable remote-server endpoint requires both a live port
/// and a valid fingerprint. The dev bridge advertises neither, so it
/// always resolves `None` — the HTTP long-poll stays its only transport.
pub fn terminal_server_endpoint(
    port: Option<u16>,
    fingerprint: Option<&str>,
) -> Option<RemoteServerEndpoint> {
    let port = port.filter(|p| *p != 0)?;
    let certificate_fingerprint = normalize_terminal_fingerprint(fingerprint)?;
    Some(RemoteServerEndpoint {
        port,
        certificate_fingerprint,
    })
}

/// Extract the host from an `http(s)://host[:port][/...]` URL string.
fn url_host(url: &str) -> Option<&str> {
    let after_scheme = url.split("://").nth(1)?;
    let authority = after_scheme.split('/').next()?;
    let host_port = authority.rsplit('@').next()?;
    if let Some(rest) = host_port.strip_prefix('[') {
        // IPv6 literal: [::1]:8080.
        let end = rest.find(']')?;
        let host = &rest[..end];
        return if host.is_empty() { None } else { Some(host) };
    }
    let host = host_port.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// The transport decision for one stream attempt: WS when the Host
/// advertises the remote server AND we hold a paired token to present;
/// `None` means HTTP long-poll (dev bridge, server down, or pre-WS build).
pub fn terminal_stream_candidate(
    endpoint: Option<&RemoteServerEndpoint>,
    base_url: &str,
    auth_token: Option<&str>,
) -> Option<RemoteTerminalWebSocketCandidate> {
    let endpoint = endpoint?;
    let host = url_host(base_url)?;
    let token = auth_token.filter(|t| !t.is_empty())?;
    Some(RemoteTerminalWebSocketCandidate {
        host: host.to_string(),
        port: endpoint.port,
        certificate_fingerprint: endpoint.certificate_fingerprint.clone(),
        token: token.to_string(),
    })
}

/// Percent-encode a query-string value (RFC 3986). Mirrors what Swift's
/// `URLComponents` does to `URLQueryItem` values.
fn query_escape(value: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
    const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    utf8_percent_encode(value, UNRESERVED).to_string()
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
        query_escape(session_id),
        query_escape(token),
    );
    if let Some(offset) = offset {
        use std::fmt::Write;
        write!(url, "&offset={offset}").expect("writing to String cannot fail");
    }
    Some(url)
}

/// The server's first text frame on a successful upgrade.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RemoteTerminalWsHello {
    #[serde(rename = "protocol")]
    pub protocol_version: i64,
    #[serde(rename = "session_id")]
    pub session_id: String,
    pub state: String,
    /// Total output.bin size at connect time.
    #[serde(rename = "output_size")]
    pub output_size: u64,
    /// The offset the client asked for (`None` on a fresh connect).
    #[serde(rename = "requested_offset")]
    pub requested_offset: Option<u64>,
    /// Where the binary stream actually begins.
    #[serde(rename = "start_offset")]
    pub start_offset: u64,
    /// True when the requested offset was unusable (beyond the file or too
    /// far behind the tail) and the server restarted from an aligned tail —
    /// the client must clear before feeding, like an HTTP rebase.
    pub rebased: bool,
    pub cols: Option<i64>,
    pub rows: Option<i64>,
    #[serde(rename = "mode_preamble_base64")]
    pub mode_preamble_base64: Option<String>,
}

impl RemoteTerminalWsHello {
    /// DEC-mode restore preamble (base64) the client feeds into its freshly
    /// reset VT before the replayed tail — the mouse-tracking / alt-screen
    /// sequences that scrolled out of the retained journal. Absent from
    /// older Hosts and at the session origin. Not journal bytes: it never
    /// moves `startOffset` or the resume cursor.
    pub fn mode_preamble(&self) -> Option<Vec<u8>> {
        let encoded = self.mode_preamble_base64.as_deref()?;
        let bytes = base64_decode(encoded).ok()?;
        if bytes.is_empty() {
            None
        } else {
            Some(bytes)
        }
    }
}

/// Simple base64 decoder (web-safe subset avoids the `base64` crate, which
/// is native-only in this crate).
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.chars() {
        if c == '=' {
            break;
        }
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            _ => return Err(format!("invalid base64 char: {c}")),
        };
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
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
        let object = match value.as_object() {
            Some(o) => o,
            None => return Self::Unknown,
        };
        let type_str = match object.get("type").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => return Self::Unknown,
        };
        match type_str {
            "hello" => match serde_json::from_value::<RemoteTerminalWsHello>(value) {
                Ok(hello) => Self::Hello(hello),
                // A hello missing required fields must not decode into a
                // bogus hello.
                Err(_) => Self::Unknown,
            },
            "error" => {
                let message = object
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error")
                    .to_string();
                Self::Error(message)
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
pub struct RemoteTerminalWsBinaryFrame {
    pub offset: u64,
    pub payload: Vec<u8>,
}

impl RemoteTerminalWsBinaryFrame {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }
        let mut header = [0u8; 8];
        header.copy_from_slice(&data[..8]);
        Some(Self {
            offset: u64::from_be_bytes(header),
            payload: data[8..].to_vec(),
        })
    }
}

#[derive(Serialize)]
struct WsInputPayload<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    data: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    wid: Option<&'a str>,
}

/// Client→server JSON text frames.
pub struct RemoteTerminalWsClientMessage;

impl RemoteTerminalWsClientMessage {
    /// The server caps one input message's data at 64KB; chunk well under
    /// it (on character boundaries, so escape sequences and multi-byte
    /// UTF-8 never split mid-scalar — a `str` iterates whole characters).
    pub const MAX_INPUT_BYTES_PER_FRAME: usize = 32 * 1024;

    /// One raw-PTY input as one or more `{"type":"input","data":...}`
    /// frames, in order. No ack is expected — the echo arrives via output.
    ///
    /// `write_id` is the idempotency key the caller also sends on the HTTP
    /// fallback for this same logical send. It is attached only when the
    /// send fits in one frame. The live input router sends larger values
    /// directly over HTTP, avoiding an ambiguous partial multi-frame WS
    /// delivery.
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
        for ch in text.chars() {
            let size = ch.len_utf8();
            if chunk_bytes + size > max_bytes && !chunk.is_empty() {
                frames.push(Self::encode_input(&chunk, None));
                chunk.clear();
                chunk_bytes = 0;
            }
            chunk.push(ch);
            chunk_bytes += size;
        }
        if !chunk.is_empty() {
            frames.push(Self::encode_input(&chunk, None));
        }
        frames
    }

    fn encode_input(data: &str, write_id: Option<&str>) -> String {
        let payload = WsInputPayload {
            kind: "input",
            data,
            wid: write_id.filter(|w| !w.is_empty()),
        };
        serde_json::to_string(&payload).expect("encoding a string map cannot fail")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_fingerprint() -> String {
        "ab".repeat(32)
    }

    // MARK: - Hello frame

    #[test]
    fn hello_decodes_full_spec_frame() {
        let text = r#"{"type":"hello","protocol":1,"session_id":"sess-1","state":"running","output_size":123456,"requested_offset":1024,"start_offset":1024,"rebased":false,"cols":204,"rows":58}"#;
        let message = RemoteTerminalWsServerMessage::parse(text);
        let RemoteTerminalWsServerMessage::Hello(hello) = message else {
            panic!("expected hello, got {message:?}");
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
        // Mirrors the Swift test's escape-sequence payload.
        let restore = "\u{1b}[?1049h\u{1b}[?1000h\u{1b}[?1006h";
        let encoded = base64_encode(restore.as_bytes());
        let text = format!(
            r#"{{"type":"hello","protocol":1,"session_id":"sess-1","state":"running","output_size":9000,"requested_offset":null,"start_offset":4096,"rebased":false,"cols":80,"rows":24,"mode_preamble_base64":"{encoded}"}}"#
        );
        let message = RemoteTerminalWsServerMessage::parse(&text);
        let RemoteTerminalWsServerMessage::Hello(hello) = message else {
            panic!("expected hello, got {message:?}");
        };
        assert_eq!(hello.mode_preamble(), Some(restore.as_bytes().to_vec()));
        // The preamble is not journal bytes: the stream still starts where
        // the Host said it does.
        assert_eq!(hello.start_offset, 4096);
    }

    #[test]
    fn hello_without_preamble_field_decodes_as_none() {
        let text = r#"{"type":"hello","protocol":1,"session_id":"sess-1","state":"running","output_size":10,"requested_offset":null,"start_offset":0,"rebased":false,"cols":null,"rows":null}"#;
        let message = RemoteTerminalWsServerMessage::parse(text);
        let RemoteTerminalWsServerMessage::Hello(hello) = message else {
            panic!("expected hello, got {message:?}");
        };
        assert_eq!(hello.mode_preamble(), None);
    }

    #[test]
    fn hello_decodes_null_offset_and_missing_grid() {
        let text = r#"{"type":"hello","protocol":1,"session_id":"sess-2","state":"running","output_size":9000,"requested_offset":null,"start_offset":8704,"rebased":true,"cols":null,"rows":null}"#;
        let message = RemoteTerminalWsServerMessage::parse(text);
        let RemoteTerminalWsServerMessage::Hello(hello) = message else {
            panic!("expected hello, got {message:?}");
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

    // MARK: - Binary frames

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
        assert_eq!(RemoteTerminalWsBinaryFrame::parse(&[1, 2, 3]), None);
        assert_eq!(RemoteTerminalWsBinaryFrame::parse(&[]), None);
    }

    #[test]
    fn binary_frame_parses_from_non_zero_based_slice() {
        // The Swift test guards against absolute indexing into a Data slice
        // that keeps its parent's indices; Rust slices are always
        // zero-based, so a subslice is the equivalent case.
        let mut padded = vec![0xFF, 0xFF];
        padded.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 7]);
        padded.extend_from_slice(b"x");
        let frame = RemoteTerminalWsBinaryFrame::parse(&padded[2..]).expect("frame");
        assert_eq!(frame.offset, 7);
        assert_eq!(frame.payload, b"x");
    }

    // MARK: - Fingerprint normalization

    #[test]
    fn fingerprint_normalization_lowercases_and_strips_decoration() {
        let plain = "ab12".repeat(16); // 64 hex chars
        assert_eq!(
            normalize_terminal_fingerprint(Some(&plain.to_uppercase())),
            Some(plain.clone())
        );
        assert_eq!(
            normalize_terminal_fingerprint(Some(&format!("  {plain}\n"))),
            Some(plain.clone())
        );
        assert_eq!(
            normalize_terminal_fingerprint(Some(&format!("sha256:{plain}"))),
            Some(plain.clone())
        );
        // Colon-separated hex pairs (openssl-style) normalize too.
        let colonized = plain
            .as_bytes()
            .chunks(2)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(
            normalize_terminal_fingerprint(Some(&colonized)),
            Some(plain)
        );
    }

    #[test]
    fn fingerprint_normalization_rejects_invalid_input() {
        assert_eq!(normalize_terminal_fingerprint(None), None);
        assert_eq!(normalize_terminal_fingerprint(Some("")), None);
        assert_eq!(normalize_terminal_fingerprint(Some("abcd")), None); // too short
        assert_eq!(
            normalize_terminal_fingerprint(Some(&"g".repeat(64))), // not hex
            None
        );
        assert_eq!(
            normalize_terminal_fingerprint(Some(&"a".repeat(65))), // wrong length
            None
        );
    }

    #[test]
    fn fingerprints_match_is_case_insensitive() {
        let plain = "0f".repeat(32);
        assert!(terminal_fingerprints_match(
            Some(&plain),
            Some(&plain.to_uppercase())
        ));
        assert!(!terminal_fingerprints_match(
            Some(&plain),
            Some(&"0e".repeat(32))
        ));
        assert!(!terminal_fingerprints_match(Some(&plain), None));
    }

    // MARK: - Transport selection

    #[test]
    fn endpoint_requires_port_and_valid_fingerprint() {
        assert_eq!(
            terminal_server_endpoint(None, Some(&valid_fingerprint())),
            None
        );
        assert_eq!(terminal_server_endpoint(Some(50123), None), None);
        assert_eq!(terminal_server_endpoint(Some(50123), Some("bogus")), None);
        assert_eq!(
            terminal_server_endpoint(Some(0), Some(&valid_fingerprint())),
            None
        );
        let endpoint =
            terminal_server_endpoint(Some(50123), Some(&valid_fingerprint().to_uppercase()))
                .expect("endpoint");
        assert_eq!(endpoint.port, 50123);
        assert_eq!(endpoint.certificate_fingerprint, valid_fingerprint());
    }

    #[test]
    fn candidate_requires_endpoint_host_and_token() {
        let endpoint = RemoteServerEndpoint {
            port: 50123,
            certificate_fingerprint: valid_fingerprint(),
        };
        let base_url = "http://192.168.1.20:17661/mobile";

        // No endpoint (dev bridge / server down / pre-WS Host) → HTTP only.
        assert_eq!(terminal_stream_candidate(None, base_url, Some("tok")), None);
        // No token → HTTP only.
        assert_eq!(
            terminal_stream_candidate(Some(&endpoint), base_url, None),
            None
        );
        assert_eq!(
            terminal_stream_candidate(Some(&endpoint), base_url, Some("")),
            None
        );

        let candidate =
            terminal_stream_candidate(Some(&endpoint), base_url, Some("tok")).expect("candidate");
        assert_eq!(candidate.host, "192.168.1.20");
        assert_eq!(candidate.port, 50123);
        assert_eq!(candidate.certificate_fingerprint, valid_fingerprint());
        assert_eq!(candidate.token, "tok");
    }

    #[test]
    fn web_socket_output_url_with_resume_offset() {
        assert_eq!(
            web_socket_output_url("192.168.1.20", 50123, "sess-abc", "tok", Some(42)),
            Some(
                "wss://192.168.1.20:50123/api/sessions/sess-abc/output?token=tok&offset=42"
                    .to_string()
            )
        );
    }

    #[test]
    fn web_socket_output_url_without_offset() {
        assert_eq!(
            web_socket_output_url("mac.local", 1234, "s1", "tok", None),
            Some("wss://mac.local:1234/api/sessions/s1/output?token=tok".to_string())
        );
    }

    #[test]
    fn web_socket_output_url_rejects_empty_host_or_session() {
        assert_eq!(web_socket_output_url("", 1, "s", "t", None), None);
        assert_eq!(web_socket_output_url("h", 1, "", "t", None), None);
    }

    // MARK: - Client input frames

    #[derive(Deserialize)]
    struct DecodedInput {
        #[serde(rename = "type")]
        kind: String,
        data: String,
        wid: Option<String>,
    }

    fn decode_frames(frames: &[String]) -> Vec<DecodedInput> {
        frames
            .iter()
            .map(|f| serde_json::from_str(f).expect("frame is valid JSON"))
            .collect()
    }

    #[test]
    fn input_frame_encodes_single_small_message() {
        let frames = RemoteTerminalWsClientMessage::input_frames(
            "ls -la\r",
            Some("write-123"),
            RemoteTerminalWsClientMessage::MAX_INPUT_BYTES_PER_FRAME,
        );
        assert_eq!(frames.len(), 1);
        let decoded = decode_frames(&frames);
        assert_eq!(decoded[0].kind, "input");
        assert_eq!(decoded[0].data, "ls -la\r");
        assert_eq!(decoded[0].wid.as_deref(), Some("write-123"));
    }

    #[test]
    fn input_frame_preserves_control_and_escape_bytes() {
        let text = "\u{1b}[200~pasted\u{1b}[201~\u{3}";
        let decoded = decode_frames(&RemoteTerminalWsClientMessage::input_frames(
            text,
            None,
            RemoteTerminalWsClientMessage::MAX_INPUT_BYTES_PER_FRAME,
        ));
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].data, text);
    }

    #[test]
    fn input_frames_chunk_large_input_on_character_boundaries() {
        // 3-byte character: a naive byte split would shear it mid-scalar.
        let text = "€".repeat(100);
        let frames = RemoteTerminalWsClientMessage::input_frames(text.as_str(), None, 32);
        assert!(frames.len() > 1);
        let decoded = decode_frames(&frames);
        for part in &decoded {
            assert_eq!(part.kind, "input");
            assert!(part.data.len() <= 32);
            assert_eq!(part.wid, None);
        }
        let joined: String = decoded.iter().map(|p| p.data.as_str()).collect();
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

    // Test-only base64 encoder for the mode-preamble fixture.
    fn base64_encode(bytes: &[u8]) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let mut buf = [0u8; 3];
            buf[..chunk.len()].copy_from_slice(chunk);
            let n = (buf[0] as u32) << 16 | (buf[1] as u32) << 8 | buf[2] as u32;
            out.push(CHARS[((n >> 18) & 63) as usize] as char);
            out.push(CHARS[((n >> 12) & 63) as usize] as char);
            out.push(if chunk.len() > 1 {
                CHARS[((n >> 6) & 63) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                CHARS[(n & 63) as usize] as char
            } else {
                '='
            });
        }
        out
    }
}
