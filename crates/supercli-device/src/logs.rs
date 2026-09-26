//! Live device logs: `adb logcat` / `log stream` parsing, filtering, and
//! WebSocket streaming.
//!
//! [`LogStream`] spawns the platform log tool and yields parsed, filtered
//! [`LogEntry`]s. [`ws_serve_logs`] performs a server-side WebSocket
//! handshake on an accepted TCP stream and pushes each entry as a JSON
//! text frame. Pure std: SHA-1 and base64 are hand-rolled (this crate has
//! zero mandatory dependencies).
//!
//! Compiled only with the `device` cargo feature.

use super::{spawn_stream, DeviceError, DeviceId, DeviceStream};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

/// Android log priority, ordered so filters can use `>=`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    #[default]
    Verbose = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Fatal = 5,
}

impl LogLevel {
    pub fn from_char(c: char) -> Option<LogLevel> {
        match c {
            'V' => Some(LogLevel::Verbose),
            'D' => Some(LogLevel::Debug),
            'I' => Some(LogLevel::Info),
            'W' => Some(LogLevel::Warn),
            'E' => Some(LogLevel::Error),
            'F' | 'A' => Some(LogLevel::Fatal),
            _ => None,
        }
    }

    pub fn as_char(self) -> char {
        match self {
            LogLevel::Verbose => 'V',
            LogLevel::Debug => 'D',
            LogLevel::Info => 'I',
            LogLevel::Warn => 'W',
            LogLevel::Error => 'E',
            LogLevel::Fatal => 'F',
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LogLevel::Verbose => "verbose",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
            LogLevel::Fatal => "fatal",
        }
    }
}

/// One parsed log line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    /// Raw timestamp as the tool printed it (`MM-DD HH:MM:SS.mmm`).
    pub timestamp: String,
    pub pid: u32,
    pub tid: u32,
    pub level: LogLevel,
    pub tag: String,
    pub message: String,
}

impl LogEntry {
    /// Compact JSON line for the WebSocket stream / MCP.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"ts\":\"{}\",\"pid\":{},\"tid\":{},\"level\":\"{}\",\"tag\":\"{}\",\"msg\":\"{}\"}}",
            super::ui::json_escape(&self.timestamp),
            self.pid,
            self.tid,
            self.level.as_char(),
            super::ui::json_escape(&self.tag),
            super::ui::json_escape(&self.message),
        )
    }
}

/// Filter applied to each entry before it is yielded.
#[derive(Clone, Debug, Default)]
pub struct LogFilter {
    /// Exact tag match (case-sensitive), e.g. `Some("scrcpy")`.
    pub tag: Option<String>,
    /// Minimum level; entries below are dropped.
    pub min_level: LogLevel,
    /// Substring that must appear in tag or message (case-insensitive).
    pub contains: Option<String>,
}

impl LogFilter {
    pub fn matches(&self, e: &LogEntry) -> bool {
        if e.level < self.min_level {
            return false;
        }
        if let Some(tag) = &self.tag {
            if &e.tag != tag {
                return false;
            }
        }
        if let Some(needle) = &self.contains {
            let n = needle.to_lowercase();
            if !e.tag.to_lowercase().contains(&n) && !e.message.to_lowercase().contains(&n) {
                return false;
            }
        }
        true
    }
}

/// Parse one `adb logcat -v threadtime` line:
/// `09-26 04:11:02.123  1234  5678 I MyTag: hello world`
/// Returns None for blank lines, logcat banners (`--------- beginning of …`),
/// and lines that do not match the format.
pub fn parse_logcat_threadtime(line: &str) -> Option<LogEntry> {
    let line = line.trim_end();
    if line.is_empty() || line.starts_with("---------") {
        return None;
    }
    // Date `MM-DD`, time `HH:MM:SS.mmm`, pid, tid, level, `Tag: msg`.
    let mut parts = line.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    if date.len() != 5 || !date.chars().nth(2).map(|c| c == '-').unwrap_or(false) {
        return None;
    }
    let pid: u32 = parts.next()?.parse().ok()?;
    let tid: u32 = parts.next()?.parse().ok()?;
    let level_c = parts.next()?.chars().next()?;
    let level = LogLevel::from_char(level_c)?;
    // The rest is `Tag: message`. Find the tag/message split in the
    // original line to preserve inner spacing.
    let rest_start = line.find(level_c)?;
    let rest = line[rest_start + 1..].trim_start();
    let colon = rest.find(':')?;
    let tag = rest[..colon].trim().to_string();
    let message = rest[colon + 1..].trim_start().to_string();
    if tag.is_empty() {
        return None;
    }
    Some(LogEntry {
        timestamp: format!("{date} {time}"),
        pid,
        tid,
        level,
        tag,
        message,
    })
}

/// A live, filtered log iterator. Owns the tool child: dropping the stream
/// kills `logcat` (see [`DeviceStream`]).
pub struct LogStream {
    lines: std::io::Lines<BufReader<DeviceStream>>,
    filter: LogFilter,
}

impl LogStream {
    fn new(stream: DeviceStream, filter: LogFilter) -> Self {
        LogStream {
            lines: BufReader::new(stream).lines(),
            filter,
        }
    }

    /// Build a stream from pre-captured text (tests, offline replay).
    pub fn from_text(text: &str, filter: LogFilter) -> Vec<LogEntry> {
        text.lines()
            .filter_map(parse_logcat_threadtime)
            .filter(|e| filter.matches(e))
            .collect()
    }
}

impl Iterator for LogStream {
    type Item = Result<LogEntry, DeviceError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let line = match self.lines.next()? {
                Ok(l) => l,
                Err(e) => {
                    return Some(Err(DeviceError::Io(e)));
                }
            };
            match parse_logcat_threadtime(&line) {
                Some(e) if self.filter.matches(&e) => return Some(Ok(e)),
                Some(_) => continue, // filtered out
                None => continue,    // unparsable line
            }
        }
    }
}

/// Spawn `adb -s <serial> logcat -v threadtime` and stream filtered entries.
pub fn stream_logcat(serial: &DeviceId, filter: LogFilter) -> Result<LogStream, DeviceError> {
    if !super::tool_on_path("adb") {
        return Err(DeviceError::ToolMissing("adb".to_string()));
    }
    let stream = spawn_stream(
        "adb",
        &["-s", serial.as_str(), "logcat", "-v", "threadtime"],
    )?;
    Ok(LogStream::new(stream, filter))
}

/// Stream iOS simulator logs: `xcrun simctl spawn <udid> log stream`.
/// Output lines look like `2026-09-26 04:11:02.123 Df MyTag[1234] <Info>: msg`;
/// only lines carrying a `<Level>:` marker are parsed, the rest are skipped.
pub fn stream_simctl_log(udid: &DeviceId, filter: LogFilter) -> Result<LogStream, DeviceError> {
    if !super::tool_on_path("xcrun") {
        return Err(DeviceError::ToolMissing("xcrun".to_string()));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (udid, filter);
        Err(DeviceError::NotMacOSHost)
    }
    #[cfg(target_os = "macos")]
    {
        let stream = spawn_stream(
            "xcrun",
            &[
                "simctl",
                "spawn",
                udid.as_str(),
                "log",
                "stream",
                "--style",
                "compact",
            ],
        )?;
        Ok(LogStream::new(stream, filter))
    }
}

// ---------------------------------------------------------------------------
// Minimal WebSocket server (RFC 6455, server side, text frames only)
// ---------------------------------------------------------------------------

/// Errors from the log WebSocket server.
#[derive(Debug)]
pub enum WsError {
    Io(std::io::Error),
    /// The HTTP upgrade request was not a WebSocket handshake.
    BadHandshake(String),
    /// The client closed the connection.
    ClientClosed,
}

impl std::fmt::Display for WsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WsError::Io(e) => write!(f, "websocket I/O: {e}"),
            WsError::BadHandshake(m) => write!(f, "bad websocket handshake: {m}"),
            WsError::ClientClosed => write!(f, "client closed the connection"),
        }
    }
}

impl std::error::Error for WsError {}

impl From<std::io::Error> for WsError {
    fn from(e: std::io::Error) -> Self {
        WsError::Io(e)
    }
}

/// Base64 encode (RFC 4648, no line breaks). Hand-rolled: no dependency.
pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    const ALPH: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut n: u32 = 0;
        for (i, &b) in chunk.iter().enumerate() {
            n |= (b as u32) << (16 - 8 * i);
        }
        let pad = 3 - chunk.len();
        for i in 0..4 - pad {
            out.push(ALPH[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
        for _ in 0..pad {
            out.push('=');
        }
    }
    out
}

/// SHA-1 digest (FIPS 180-4). Hand-rolled for the WebSocket accept key.
fn sha1(message: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = message.to_vec();
    let bit_len = (msg.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // Padding guarantees `msg.len() % 64 == 0`, so there is no remainder.
    let (blocks, _) = msg.as_chunks::<64>();
    for block in blocks {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[4 * i],
                block[4 * i + 1],
                block[4 * i + 2],
                block[4 * i + 3],
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
    for (i, &v) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// `Sec-WebSocket-Accept` for a client key (RFC 6455 §1.3).
fn ws_accept_key(client_key: &str) -> String {
    const MAGIC: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    let combined = format!("{client_key}{MAGIC}");
    base64_encode(&sha1(combined.as_bytes()))
}

/// Read the HTTP upgrade request and validate it is a WebSocket handshake.
/// Returns the client's `Sec-WebSocket-Key`.
fn ws_read_handshake(stream: &mut TcpStream) -> Result<String, WsError> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    if !request_line.starts_with("GET ") {
        return Err(WsError::BadHandshake(format!(
            "expected GET, got {request_line:?}"
        )));
    }
    let mut key: Option<String> = None;
    let mut upgrade_ws = false;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let lower = line.to_lowercase();
        if let Some(v) = lower.strip_prefix("sec-websocket-key:") {
            key = Some(v.trim().to_string());
        }
        if lower.starts_with("upgrade:") && lower.contains("websocket") {
            upgrade_ws = true;
        }
        if line.len() > 8192 {
            return Err(WsError::BadHandshake("header too long".to_string()));
        }
    }
    match (key, upgrade_ws) {
        (Some(k), true) => Ok(k),
        _ => Err(WsError::BadHandshake(
            "missing Sec-WebSocket-Key or Upgrade: websocket".to_string(),
        )),
    }
}

/// Encode one server-side text frame (never masked, per RFC 6455 §5.1).
fn ws_text_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(2 + payload.len() + 8);
    frame.push(0x81); // FIN + text opcode
    let len = payload.len();
    if len < 126 {
        frame.push(len as u8);
    } else if len < 65536 {
        frame.push(126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    frame.extend_from_slice(payload);
    frame
}

/// Read one client frame header to detect a close frame. Returns true when
/// the client sent a close (opcode 0x8) or EOF.
fn ws_client_closed(stream: &mut TcpStream) -> bool {
    let mut hdr = [0u8; 2];
    // Non-blocking peek: no data yet is fine, we only care about close.
    let _ = stream.set_nonblocking(true);
    let r = stream.peek(&mut hdr);
    let _ = stream.set_nonblocking(false);
    match r {
        Ok(0) => true, // EOF
        Ok(_) => {
            // Read and parse the frame; only close matters here.
            let mut buf = [0u8; 2];
            if stream.read_exact(&mut buf).is_err() {
                return true;
            }
            let opcode = buf[0] & 0x0F;
            if opcode == 0x8 {
                return true;
            }
            // Drain the rest of this frame so the stream stays aligned.
            let mut len = (buf[1] & 0x7F) as u64;
            if len == 126 {
                let mut b = [0u8; 2];
                if stream.read_exact(&mut b).is_ok() {
                    len = u16::from_be_bytes(b) as u64;
                }
            } else if len == 127 {
                let mut b = [0u8; 8];
                if stream.read_exact(&mut b).is_ok() {
                    len = u64::from_be_bytes(b);
                }
            }
            let masked = buf[1] & 0x80 != 0;
            if masked {
                let mut mask = [0u8; 4];
                let _ = stream.read_exact(&mut mask);
            }
            let mut drain = vec![0u8; len.min(65536) as usize];
            let mut left = len;
            while left > 0 {
                let n = left.min(drain.len() as u64) as usize;
                match stream.read(&mut drain[..n]) {
                    Ok(0) => break,
                    Ok(k) => left -= k as u64,
                    Err(_) => break,
                }
            }
            false
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => false,
        Err(_) => true,
    }
}

/// Accept one TCP connection, complete the WebSocket handshake, then push
/// each [`LogEntry`] from `entries` as a JSON text frame. Returns the number
/// of frames sent. Stops early when the client closes the connection.
///
/// Typical use: `let l = TcpListener::bind("127.0.0.1:0")?;` then
/// `ws_serve_logs(l, filter, LogStream::new(...))` on a thread.
pub fn ws_serve_logs(
    listener: TcpListener,
    entries: impl Iterator<Item = Result<LogEntry, DeviceError>>,
    read_timeout: Duration,
) -> Result<usize, WsError> {
    let (mut stream, _) = listener.accept()?;
    stream.set_read_timeout(Some(read_timeout))?;
    let key = ws_read_handshake(&mut stream)?;
    let accept = ws_accept_key(&key);
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;

    let mut sent = 0usize;
    for entry in entries {
        if ws_client_closed(&mut stream) {
            return Err(WsError::ClientClosed);
        }
        let entry = entry.map_err(|e| WsError::Io(std::io::Error::other(e.to_string())))?;
        let frame = ws_text_frame(entry.to_json().as_bytes());
        stream.write_all(&frame)?;
        sent += 1;
    }
    // Clean close: server close frame (unmasked, no payload).
    let _ = stream.write_all(&[0x88, 0x00]);
    let _ = stream.flush();
    Ok(sent)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGCAT_FIXTURE: &str = "\
--------- beginning of main
09-26 04:11:02.123  1234  5678 I MyTag: hello world
09-26 04:11:02.124  1234  5678 D MyTag: debug detail
09-26 04:11:03.001   999   1000 E scrcpy: encoder failed
09-26 04:11:03.002   999   1000 W scrcpy: retrying
not a log line
09-26 04:11:04.000  1234  5678 V MyTag: verbose noise
";

    #[test]
    fn logcat_threadtime_parses() {
        let e = parse_logcat_threadtime("09-26 04:11:02.123  1234  5678 I MyTag: hello world")
            .expect("parses");
        assert_eq!(e.timestamp, "09-26 04:11:02.123");
        assert_eq!((e.pid, e.tid), (1234, 5678));
        assert_eq!(e.level, LogLevel::Info);
        assert_eq!(e.tag, "MyTag");
        assert_eq!(e.message, "hello world");
    }

    #[test]
    fn logcat_skips_banners_and_garbage() {
        assert!(parse_logcat_threadtime("--------- beginning of main").is_none());
        assert!(parse_logcat_threadtime("").is_none());
        assert!(parse_logcat_threadtime("not a log line").is_none());
        // Message containing a colon keeps its tail.
        let e = parse_logcat_threadtime("09-26 04:11:02.123  1  2 W Tag: a: b: c").expect("parses");
        assert_eq!(e.message, "a: b: c");
    }

    #[test]
    fn log_level_ordering_and_chars() {
        assert!(LogLevel::Verbose < LogLevel::Fatal);
        assert_eq!(LogLevel::from_char('E'), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_char('A'), Some(LogLevel::Fatal));
        assert_eq!(LogLevel::from_char('X'), None);
        assert_eq!(LogLevel::Warn.as_char(), 'W');
    }

    #[test]
    fn filter_matches_tag_level_contains() {
        let filter = LogFilter {
            tag: Some("scrcpy".to_string()),
            min_level: LogLevel::Warn,
            contains: None,
        };
        let entries = LogStream::from_text(LOGCAT_FIXTURE, filter);
        assert_eq!(entries.len(), 2); // E scrcpy + W scrcpy
        assert!(entries.iter().all(|e| e.tag == "scrcpy"));

        let filter = LogFilter {
            tag: None,
            min_level: LogLevel::Error,
            contains: Some("encoder".to_string()),
        };
        let entries = LogStream::from_text(LOGCAT_FIXTURE, filter);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, LogLevel::Error);
    }

    #[test]
    fn base64_encode_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn sha1_test_vector() {
        // FIPS 180-4 "abc".
        let d = sha1(b"abc");
        let hex: String = d.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn ws_accept_key_rfc6455_vector() {
        // RFC 6455 §1.3 example.
        assert_eq!(
            ws_accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn ws_text_frame_encoding() {
        let f = ws_text_frame(b"Hi");
        assert_eq!(f, vec![0x81, 0x02, b'H', b'i']);
        // 126-byte extended length path.
        let big = vec![b'x'; 200];
        let f = ws_text_frame(&big);
        assert_eq!(&f[..4], &[0x81, 126, 0, 200]);
        assert_eq!(f.len(), 4 + 200);
    }

    #[test]
    fn ws_handshake_rejects_non_websocket() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().expect("accept");
            // A short read timeout keeps the test fast.
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            ws_read_handshake(&mut s)
        });
        let mut client = TcpStream::connect(addr).expect("connect");
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
            .expect("write");
        let err = server.join().expect("join").unwrap_err();
        assert!(matches!(err, WsError::BadHandshake(_)));
    }

    #[test]
    fn ws_serve_logs_streams_json_frames() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().unwrap();
        let entries: Vec<Result<LogEntry, DeviceError>> = LogStream::from_text(
            LOGCAT_FIXTURE,
            LogFilter {
                tag: Some("MyTag".to_string()),
                ..Default::default()
            },
        )
        .into_iter()
        .map(Ok)
        .collect();
        let expected = entries.len();
        assert!(expected > 0);

        let server = std::thread::spawn(move || {
            ws_serve_logs(listener, entries.into_iter(), Duration::from_secs(5)).expect("serve")
        });

        // Minimal WS client: handshake, then read text frames.
        let mut client = TcpStream::connect(addr).expect("connect");
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        client
            .write_all(
                format!(
                    "GET /logs HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\n\
                     Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\n\
                     Sec-WebSocket-Version: 13\r\n\r\n"
                )
                .as_bytes(),
            )
            .expect("handshake write");
        let mut reader = BufReader::new(client.try_clone().expect("clone"));
        let mut status = String::new();
        reader.read_line(&mut status).expect("status");
        assert!(status.contains("101"), "got {status}");
        // Consume the rest of the HTTP response headers (until blank line)
        // before reading WebSocket frames.
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("header");
            if line.trim().is_empty() {
                break;
            }
        }

        let mut frames = 0usize;
        loop {
            let mut hdr = [0u8; 2];
            reader.read_exact(&mut hdr).expect("frame header");
            let opcode = hdr[0] & 0x0F;
            if opcode == 0x8 {
                break; // server close frame
            }
            assert_eq!(opcode, 0x1, "text frame");
            let mut len = (hdr[1] & 0x7F) as usize;
            if len == 126 {
                let mut b = [0u8; 2];
                reader.read_exact(&mut b).unwrap();
                len = u16::from_be_bytes(b) as usize;
            }
            let mut payload = vec![0u8; len];
            reader.read_exact(&mut payload).expect("payload");
            let text = String::from_utf8(payload).expect("utf-8");
            assert!(text.contains("\"tag\":\"MyTag\""), "got {text}");
            frames += 1;
        }
        let sent = server.join().expect("server join");
        assert_eq!(frames, expected);
        assert_eq!(sent, expected);
    }

    #[test]
    fn log_entry_to_json_is_valid() {
        let e = parse_logcat_threadtime("09-26 04:11:02.123  1  2 I T: m\"q").expect("parses");
        let j = e.to_json();
        assert!(j.contains("\\\"q"));
        assert!(j.contains("\"level\":\"I\""));
    }
}
