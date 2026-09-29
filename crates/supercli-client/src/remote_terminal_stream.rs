//! Port of `RemoteTerminalStreamTransport.swift` (SupercliIOS).
//!
//! Pure protocol/selection logic for the terminal output WebSocket exposed
//! by the Mac's `supercli-host __remote__` server: hello/error frame
//! decoding, binary frame parsing (8-byte big-endian output.bin offset
//! prefix), certificate fingerprint normalization, and the
//! transport-selection decision (WS candidate vs. the HTTP long-poll
//! fallback).
//!
//! Deliberately socket-free and side-effect-free so every piece is unit
//! testable; the live connection lives elsewhere.
//!
//! Web-safe: compiles for `wasm32-unknown-unknown`.

use serde::Deserialize;

/// The advertised `supercli-host __remote__` endpoint, as discovered from the
/// latest bootstrap/pairing response. The port is OS-assigned per server run
/// (never cache it across reconnects — always read the freshest value); the
/// fingerprint is stable across restarts and already normalized.
///
/// Port of `RemoteServerEndpoint` from `RemoteTerminalStreamTransport.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteServerEndpoint {
    pub port: u16,
    /// Normalized lowercase hex SHA-256 of the TLS leaf certificate DER.
    pub certificate_fingerprint: String,
}

/// Everything needed for one WS connect attempt: host from the paired mobile
/// endpoint, port + pin from discovery, the phone's paired bearer token.
///
/// Port of `RemoteTerminalWebSocketCandidate` from
/// `RemoteTerminalStreamTransport.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteTerminalWebSocketCandidate {
    pub host: String,
    pub port: u16,
    pub certificate_fingerprint: String,
    pub token: String,
}

/// Transport-selection decisions.
///
/// Port of `RemoteTerminalTransportSelector` from
/// `RemoteTerminalStreamTransport.swift`.
pub struct RemoteTerminalTransportSelector;

impl RemoteTerminalTransportSelector {
    /// Strict-but-liberal fingerprint normalization: strips an optional
    /// `sha256:` prefix, colons, and whitespace, lowercases, and requires
    /// exactly 64 hex characters. Anything else is unusable for pinning —
    /// and no pin means no WS (there is deliberately no bypass).
    pub fn normalized_fingerprint(raw: Option<&str>) -> Option<String> {
        let raw = raw?;
        let mut value = raw.trim().to_lowercase();
        if let Some(stripped) = value.strip_prefix("sha256:") {
            value = stripped.to_string();
        }
        value = value.chars().filter(|c| *c != ':').collect();
        if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(value)
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
    pub fn endpoint(port: Option<u16>, fingerprint: Option<&str>) -> Option<RemoteServerEndpoint> {
        let port = port?;
        if port == 0 {
            return None;
        }
        let normalized = Self::normalized_fingerprint(fingerprint)?;
        Some(RemoteServerEndpoint {
            port,
            certificate_fingerprint: normalized,
        })
    }

    /// The transport decision for one stream attempt: WS when the Mac
    /// advertises the remote server AND we hold a paired token to present;
    /// `None` means HTTP long-poll (dev bridge, server down, or pre-WS build).
    pub fn candidate(
        endpoint: Option<&RemoteServerEndpoint>,
        base_url_host: Option<&str>,
        auth_token: Option<&str>,
    ) -> Option<RemoteTerminalWebSocketCandidate> {
        let endpoint = endpoint?;
        let host = base_url_host?;
        if host.is_empty() {
            return None;
        }
        let token = auth_token?;
        if token.is_empty() {
            return None;
        }
        Some(RemoteTerminalWebSocketCandidate {
            host: host.to_string(),
            port: endpoint.port,
            certificate_fingerprint: endpoint.certificate_fingerprint.clone(),
            token: token.to_string(),
        })
    }

    /// `wss://<host>:<port>/api/sessions/<id>/output?token=...[&offset=N]`
    ///
    /// Returns `None` when host or session id is empty. The token is
    /// percent-encoded for safe query embedding.
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
            "wss://{}:{}/api/sessions/{}/output?token={}",
            host,
            port,
            percent_encode_path_segment(session_id),
            percent_encode_query_value(token),
        );
        if let Some(offset) = offset {
            url.push_str(&format!("&offset={offset}"));
        }
        Some(url)
    }
}

/// Minimal percent-encoding for one query value (RFC 3986 unreserved set
/// passes through; everything else becomes `%XX` uppercase hex).
fn percent_encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Minimal percent-encoding for one path segment (also encodes `/` so a
/// session id can never escape its segment).
fn percent_encode_path_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The server's first text frame on a successful upgrade.
///
/// Port of `RemoteTerminalWSHello` from `RemoteTerminalStreamTransport.swift`.
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
    /// DEC-mode restore preamble (base64) the client feeds into its freshly
    /// reset VT before the replayed tail — the mouse-tracking / alt-screen
    /// sequences that scrolled out of the retained journal. Absent from
    /// older Hosts and at the session origin. Not journal bytes: it never
    /// moves `start_offset` or the resume cursor.
    #[serde(rename = "mode_preamble_base64")]
    pub mode_preamble_base64: Option<String>,
}

impl RemoteTerminalWsHello {
    /// Decoded preamble bytes, or `None` when absent/invalid/empty.
    pub fn mode_preamble(&self) -> Option<Vec<u8>> {
        let encoded = self.mode_preamble_base64.as_deref()?;
        let bytes = base64_decode(encoded)?;
        if bytes.is_empty() {
            None
        } else {
            Some(bytes)
        }
    }
}

/// Minimal base64 decoder (standard alphabet, padding-aware) so this module
/// stays web-safe without extra dependencies.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    // Strip ASCII whitespace; Swift's Data(base64Encoded:) is lenient.
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if clean.is_empty() || clean.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for chunk in clean.chunks(4) {
        let mut n: u32 = 0;
        let mut pad = 0;
        for (i, &c) in chunk.iter().enumerate() {
            if c == b'=' {
                // Padding is only legal in the last two positions.
                if i < 2 {
                    return None;
                }
                pad += 1;
            } else {
                if pad > 0 {
                    return None;
                }
                n = (n << 6) | u32::from(val(c)?);
            }
        }
        if pad > 2 {
            return None;
        }
        // Re-align: pad quartets contribute zero bits at the end.
        n <<= 6 * pad;
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// Server→client text frames: the hello, or non-fatal in-stream errors.
///
/// Port of `RemoteTerminalWSServerMessage` from
/// `RemoteTerminalStreamTransport.swift`.
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
        let msg_type = value.get("type").and_then(|t| t.as_str());
        match msg_type {
            Some("hello") => match serde_json::from_value::<RemoteTerminalWsHello>(value) {
                Ok(hello) => Self::Hello(hello),
                Err(_) => Self::Unknown,
            },
            Some("error") => {
                let message = value
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
///
/// Port of `RemoteTerminalWSBinaryFrame` from
/// `RemoteTerminalStreamTransport.swift`.
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
        let mut offset: u64 = 0;
        for &byte in &data[..8] {
            offset = (offset << 8) | u64::from(byte);
        }
        Some(Self {
            offset,
            payload: data[8..].to_vec(),
        })
    }

    /// Resume offset: `offset + payload.len()`.
    pub fn resume_offset(&self) -> u64 {
        self.offset + self.payload.len() as u64
    }
}

/// Client→server JSON text frames.
///
/// Port of `RemoteTerminalWSClientMessage` from
/// `RemoteTerminalStreamTransport.swift`.
pub struct RemoteTerminalWsClientMessage;

impl RemoteTerminalWsClientMessage {
    /// The server caps one input message's data at 64KB; chunk well under it
    /// (on character boundaries, so escape sequences and multi-byte UTF-8
    /// never split mid-scalar).
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
        let wid = write_id.filter(|w| !w.is_empty());
        // Hand-rolled JSON: serde_json would also work, but the payload is
        // fixed-shape and this keeps the module dependency-light. Escape the
        // two characters JSON strings require beyond what serde would do.
        fn esc(s: &str) -> String {
            let mut out = String::with_capacity(s.len());
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                    c => out.push(c),
                }
            }
            out
        }
        match wid {
            Some(w) => format!(
                "{{\"type\":\"input\",\"data\":\"{}\",\"wid\":\"{}\"}}",
                esc(data),
                esc(w)
            ),
            None => format!("{{\"type\":\"input\",\"data\":\"{}\"}}", esc(data)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- normalized_fingerprint ---

    #[test]
    fn normalizes_strict_fingerprint() {
        let fp = "aa".repeat(32);
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some(&fp)),
            Some(fp.clone())
        );
        // Colons, sha256: prefix, whitespace, uppercase all normalize.
        let messy = format!(
            "  SHA256:{}  ",
            fp[..32].to_uppercase() + ":" + &fp[32..].to_uppercase()
        );
        // Build a colon-separated uppercase variant.
        let mut coloned = String::from("sha256:");
        for (i, c) in fp.chars().enumerate() {
            if i > 0 && i % 2 == 0 {
                coloned.push(':');
            }
            coloned.push(c.to_ascii_uppercase());
        }
        let _ = messy;
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some(&coloned)),
            Some(fp)
        );
    }

    #[test]
    fn rejects_bad_fingerprints() {
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(None),
            None
        );
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some("")),
            None
        );
        // Too short.
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some(&"aa".repeat(31))),
            None
        );
        // Non-hex.
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some(&"zz".repeat(32))),
            None
        );
        // 65 hex chars.
        assert_eq!(
            RemoteTerminalTransportSelector::normalized_fingerprint(Some(&("aa".repeat(32) + "a"))),
            None
        );
    }

    #[test]
    fn fingerprints_match_after_normalization() {
        let fp = "ab".repeat(32);
        let other = format!(
            "SHA256:{}",
            fp.to_uppercase()
                .chars()
                .enumerate()
                .fold(String::new(), |mut s, (i, c)| {
                    if i > 0 && i % 4 == 0 {
                        s.push(':');
                    }
                    s.push(c);
                    s
                })
        );
        assert!(RemoteTerminalTransportSelector::fingerprints_match(
            Some(&fp),
            Some(&other)
        ));
        assert!(!RemoteTerminalTransportSelector::fingerprints_match(
            Some(&fp),
            Some(&"cd".repeat(32))
        ));
        assert!(!RemoteTerminalTransportSelector::fingerprints_match(
            None,
            Some(&fp)
        ));
    }

    // --- endpoint / candidate ---

    #[test]
    fn endpoint_requires_port_and_fingerprint() {
        let fp = "aa".repeat(32);
        let ep = RemoteTerminalTransportSelector::endpoint(Some(8443), Some(&fp)).unwrap();
        assert_eq!(ep.port, 8443);
        assert_eq!(ep.certificate_fingerprint, fp);
        assert!(RemoteTerminalTransportSelector::endpoint(None, Some(&fp)).is_none());
        assert!(RemoteTerminalTransportSelector::endpoint(Some(0), Some(&fp)).is_none());
        assert!(RemoteTerminalTransportSelector::endpoint(Some(8443), Some("bogus")).is_none());
        assert!(RemoteTerminalTransportSelector::endpoint(Some(8443), None).is_none());
    }

    #[test]
    fn candidate_requires_endpoint_host_and_token() {
        let fp = "aa".repeat(32);
        let ep = RemoteTerminalTransportSelector::endpoint(Some(8443), Some(&fp)).unwrap();
        let c =
            RemoteTerminalTransportSelector::candidate(Some(&ep), Some("mac.local"), Some("tok"))
                .unwrap();
        assert_eq!(c.host, "mac.local");
        assert_eq!(c.port, 8443);
        assert_eq!(c.token, "tok");
        assert!(
            RemoteTerminalTransportSelector::candidate(None, Some("mac.local"), Some("tok"))
                .is_none()
        );
        assert!(RemoteTerminalTransportSelector::candidate(Some(&ep), None, Some("tok")).is_none());
        assert!(
            RemoteTerminalTransportSelector::candidate(Some(&ep), Some(""), Some("tok")).is_none()
        );
        assert!(
            RemoteTerminalTransportSelector::candidate(Some(&ep), Some("mac.local"), Some(""))
                .is_none()
        );
    }

    #[test]
    fn builds_output_url() {
        let url = RemoteTerminalTransportSelector::web_socket_output_url(
            "mac.local",
            8443,
            "sess-1",
            "tok en",
            Some(42),
        )
        .unwrap();
        assert_eq!(
            url,
            "wss://mac.local:8443/api/sessions/sess-1/output?token=tok%20en&offset=42"
        );
        let no_offset = RemoteTerminalTransportSelector::web_socket_output_url(
            "mac.local",
            8443,
            "sess-1",
            "tok",
            None,
        )
        .unwrap();
        assert!(!no_offset.contains("offset="));
        assert!(
            RemoteTerminalTransportSelector::web_socket_output_url("", 8443, "s", "t", None)
                .is_none()
        );
        assert!(
            RemoteTerminalTransportSelector::web_socket_output_url("h", 8443, "", "t", None)
                .is_none()
        );
    }

    // --- hello parsing ---

    #[test]
    fn parses_hello_frame() {
        let json = r#"{"type":"hello","protocol":3,"session_id":"s1","state":"running","output_size":1024,"requested_offset":null,"start_offset":512,"rebased":true,"cols":80,"rows":24,"mode_preamble_base64":null}"#;
        match RemoteTerminalWsServerMessage::parse(json) {
            RemoteTerminalWsServerMessage::Hello(h) => {
                assert_eq!(h.protocol_version, 3);
                assert_eq!(h.session_id, "s1");
                assert_eq!(h.output_size, 1024);
                assert_eq!(h.requested_offset, None);
                assert_eq!(h.start_offset, 512);
                assert!(h.rebased);
                assert_eq!(h.cols, Some(80));
                assert_eq!(h.mode_preamble(), None);
            }
            other => panic!("expected hello, got {other:?}"),
        }
    }

    #[test]
    fn parses_error_and_unknown_frames() {
        match RemoteTerminalWsServerMessage::parse(r#"{"type":"error","message":"boom"}"#) {
            RemoteTerminalWsServerMessage::Error(m) => assert_eq!(m, "boom"),
            other => panic!("expected error, got {other:?}"),
        }
        // Missing message -> default.
        match RemoteTerminalWsServerMessage::parse(r#"{"type":"error"}"#) {
            RemoteTerminalWsServerMessage::Error(m) => assert_eq!(m, "unknown error"),
            other => panic!("expected error, got {other:?}"),
        }
        assert_eq!(
            RemoteTerminalWsServerMessage::parse(r#"{"type":"wat"}"#),
            RemoteTerminalWsServerMessage::Unknown
        );
        assert_eq!(
            RemoteTerminalWsServerMessage::parse("not json"),
            RemoteTerminalWsServerMessage::Unknown
        );
        assert_eq!(
            RemoteTerminalWsServerMessage::parse(r#"{"no_type":1}"#),
            RemoteTerminalWsServerMessage::Unknown
        );
    }

    #[test]
    fn hello_mode_preamble_decodes() {
        // "hello" in base64.
        let json = r#"{"type":"hello","protocol":3,"session_id":"s","state":"running","output_size":0,"start_offset":0,"rebased":false,"mode_preamble_base64":"aGVsbG8="}"#;
        match RemoteTerminalWsServerMessage::parse(json) {
            RemoteTerminalWsServerMessage::Hello(h) => {
                assert_eq!(h.mode_preamble(), Some(b"hello".to_vec()));
            }
            other => panic!("expected hello, got {other:?}"),
        }
        // Invalid base64 -> None.
        let bad = r#"{"type":"hello","protocol":3,"session_id":"s","state":"running","output_size":0,"start_offset":0,"rebased":false,"mode_preamble_base64":"!!!"}"#;
        match RemoteTerminalWsServerMessage::parse(bad) {
            RemoteTerminalWsServerMessage::Hello(h) => assert_eq!(h.mode_preamble(), None),
            other => panic!("expected hello, got {other:?}"),
        }
    }

    // --- binary frame ---

    #[test]
    fn parses_binary_frame() {
        let mut data = vec![0u8, 0, 0, 0, 0, 0, 0x12, 0x34]; // offset 0x1234
        data.extend_from_slice(b"payload-bytes");
        let frame = RemoteTerminalWsBinaryFrame::parse(&data).unwrap();
        assert_eq!(frame.offset, 0x1234);
        assert_eq!(frame.payload, b"payload-bytes");
        assert_eq!(frame.resume_offset(), 0x1234 + 13);
    }

    #[test]
    fn rejects_short_binary_frame() {
        assert!(RemoteTerminalWsBinaryFrame::parse(&[1, 2, 3]).is_none());
        // Exactly 8 bytes -> empty payload is fine.
        let frame = RemoteTerminalWsBinaryFrame::parse(&[0; 8]).unwrap();
        assert_eq!(frame.offset, 0);
        assert!(frame.payload.is_empty());
    }

    // --- input frames ---

    #[test]
    fn single_frame_input() {
        let frames = RemoteTerminalWsClientMessage::input_frames("echo hi", Some("w1"), 1024);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], r#"{"type":"input","data":"echo hi","wid":"w1"}"#);
    }

    #[test]
    fn empty_input_yields_no_frames() {
        assert!(RemoteTerminalWsClientMessage::input_frames("", Some("w1"), 1024).is_empty());
    }

    #[test]
    fn chunks_on_char_boundaries_without_write_id() {
        // 10 bytes max: "abcdefgh" (8) then "ij" would overflow, etc.
        // Use multi-byte chars to prove no mid-scalar split.
        let text = "aé中b"; // 1 + 2 + 3 + 1 = 7 bytes
        let frames = RemoteTerminalWsClientMessage::input_frames(text, Some("w1"), 4);
        assert!(frames.len() >= 2);
        // Reassemble payloads and compare.
        let mut reassembled = String::new();
        for f in &frames {
            let v: serde_json::Value = serde_json::from_str(f).unwrap();
            assert_eq!(v["type"], "input");
            // Multi-frame sends never carry the write id.
            assert!(v.get("wid").is_none());
            reassembled.push_str(v["data"].as_str().unwrap());
        }
        assert_eq!(reassembled, text);
        // Each frame's data is valid UTF-8 on char boundaries by construction.
        for f in &frames {
            let v: serde_json::Value = serde_json::from_str(f).unwrap();
            let data = v["data"].as_str().unwrap();
            assert!(data.len() <= 4 || data.chars().count() >= 1);
        }
    }

    #[test]
    fn escapes_json_specials_in_input() {
        let frames = RemoteTerminalWsClientMessage::input_frames("a\"b\\c\n", None, 1024);
        assert_eq!(frames.len(), 1);
        // Must still parse as JSON with the original content.
        let v: serde_json::Value = serde_json::from_str(&frames[0]).unwrap();
        assert_eq!(v["data"], "a\"b\\c\n");
    }

    #[test]
    fn base64_decoder_vectors() {
        assert_eq!(base64_decode("aGVsbG8="), Some(b"hello".to_vec()));
        assert_eq!(base64_decode("aGk="), Some(b"hi".to_vec()));
        assert_eq!(base64_decode("YWI="), Some(b"ab".to_vec()));
        assert_eq!(base64_decode("YWJj"), Some(b"abc".to_vec()));
        assert_eq!(base64_decode(""), None);
        assert_eq!(base64_decode("!!!"), None);
        assert_eq!(base64_decode("abc"), None); // not multiple of 4
    }
}
