//! Controller-side ephemeral LAN listener for pairing-proxy.
//!
//! Rust port of Swift `ControllerPairingProxy` (ControllerPairingProxy.swift).
//!
//! When the app acts as a pure Controller for an already-selected remote
//! Host, it exposes exactly one short-lived, opaque pairing exchange so a
//! phone can be added without the Controller owning any Host state: the
//! selected Host still validates the sealed request and mints every durable
//! credential. The listener binds an ephemeral port on 0.0.0.0 and serves
//! minimal HTTP/1.1 over `std::net::TcpListener` (one thread per
//! connection, mirroring the Swift accept loop).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Reservation expiry: the QR's five-minute lifetime plus the historical
/// 30-second proxy grace. Swift: `5 * 60 + 30`.
pub const RESERVATION_TTL_SECS: u64 = 5 * 60 + 30;
/// Provider invocation timeout. Swift: 20 seconds via DispatchSemaphore.
pub const PROVIDER_TIMEOUT: Duration = Duration::from_secs(20);
/// Per-socket read/write timeout. Swift: 5 seconds via SO_RCVTIMEO/SO_SNDTIMEO.
pub const SOCKET_TIMEOUT: Duration = Duration::from_secs(5);
/// Maximum accumulated request bytes before rejection.
/// Swift: `StrictHTTPContentLength.maximum + 64 * 1024`.
pub const MAX_REQUEST_BYTES: usize = strict_content_length::MAXIMUM + 64 * 1024;

/// Error with an HTTP status, mirroring Swift `MobileRemoteError`
/// (defined in MobilePairingStore.swift).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MobileRemoteError {
    pub status: u16,
    pub message: String,
}

impl MobileRemoteError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for MobileRemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {}: {}", self.status, self.message)
    }
}

impl std::error::Error for MobileRemoteError {}

/// Strict fixed-length HTTP body parsing. The native listeners support
/// fixed-length request bodies only; parse the framing header strictly so
/// signed, overflowed, empty, or comma-joined values cannot reach buffer
/// indexing with an invalid offset.
///
/// Rust port of Swift `StrictHTTPContentLength` (HookServer.swift).
pub mod strict_content_length {
    /// Maximum accepted body: 4 MiB.
    pub const MAXIMUM: usize = 4 * 1024 * 1024;

    /// Failures of [`parse`], mirroring Swift `StrictHTTPContentLengthError`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ParseError {
        /// Non-digit characters, empty string, or unparseable.
        Invalid,
        /// Value exceeds [`MAXIMUM`].
        TooLarge,
    }

    impl std::fmt::Display for ParseError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                ParseError::Invalid => write!(f, "invalid content-length"),
                ParseError::TooLarge => write!(f, "body too large"),
            }
        }
    }

    impl std::error::Error for ParseError {}

    /// Parse a `Content-Length` header value strictly: ASCII digits only,
    /// at most [`MAXIMUM`]. A missing header means an empty body (0).
    /// Swift: `StrictHTTPContentLength.parse`.
    pub fn parse(raw: Option<&str>) -> Result<usize, ParseError> {
        let raw = match raw {
            None => return Ok(0),
            Some(raw) => raw,
        };
        if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ParseError::Invalid);
        }
        // Digits-only and non-empty; a value that overflows usize is
        // necessarily larger than MAXIMUM.
        let value: usize = raw.parse().map_err(|_| ParseError::TooLarge)?;
        if value > MAXIMUM {
            return Err(ParseError::TooLarge);
        }
        Ok(value)
    }
}

/// A minted pairing invitation: the opaque id and the LAN URL handed to the
/// phone (e.g. via QR). Swift: `ControllerPairingProxy.Reservation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingProxyReservation {
    pub id: String,
    pub endpoint: String,
}

struct ActiveReservation {
    id: String,
    expires_at: SystemTime,
    /// The sealed-exchange provider. `Send`/`Sync` because it runs on a
    /// connection thread; Swift hops to `@MainActor` here, which has no Rust
    /// equivalent in this crate, so the provider runs inline on a worker
    /// thread.
    provider: Arc<dyn Fn(Vec<u8>) -> Result<Vec<u8>, MobileRemoteError> + Send + Sync + 'static>,
    forwarding: bool,
}

struct ProxyState {
    active: Option<ActiveReservation>,
    /// False once [`ControllerPairingProxy::stop`] ran; the accept loop
    /// exits when the listener is gone.
    listening: bool,
}

/// Controller-owned pairing-proxy listener. Swift: `ControllerPairingProxy`.
pub struct ControllerPairingProxy {
    state: Arc<Mutex<ProxyState>>,
    port: u16,
    advertised_host: Option<String>,
}

impl ControllerPairingProxy {
    /// Bind one ephemeral LAN listener on 0.0.0.0. `advertised_host`
    /// overrides the auto-detected LAN address in minted URLs (tests
    /// advertise loopback while still exercising the real socket and HTTP
    /// exchange). Returns `None` when the socket cannot be bound.
    pub fn new(advertised_host: Option<String>) -> Option<Self> {
        let listener = TcpListener::bind("0.0.0.0:0").ok()?;
        let port = listener.local_addr().ok()?.port();
        let state = Arc::new(Mutex::new(ProxyState {
            active: None,
            listening: true,
        }));
        let accept_state = Arc::clone(&state);
        std::thread::Builder::new()
            .name("supercli.controller-pairing-proxy".to_string())
            .spawn(move || accept_loop(listener, accept_state))
            .ok()?;
        Some(Self {
            state,
            port,
            advertised_host,
        })
    }

    /// The bound port, for tests and diagnostics.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Stop the listener and drop any active reservation. Idempotent.
    pub fn stop(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.active = None;
        state.listening = false;
        // Connect to ourselves to unblock the accept loop, then let the
        // listener drop when the accept thread exits.
        let _ = TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", self.port)
                .to_socket_addrs()
                .ok()
                .and_then(|mut a| a.next())
                .unwrap_or_else(|| "127.0.0.1:1".parse().unwrap()),
            Duration::from_millis(200),
        );
    }

    /// Replace any prior invitation with one random, short-lived URL.
    /// Returns `None` after [`stop`](Self::stop).
    pub fn reserve(
        &self,
        provider: impl Fn(Vec<u8>) -> Result<Vec<u8>, MobileRemoteError> + Send + Sync + 'static,
    ) -> Option<PairingProxyReservation> {
        // 128-bit random id, uppercased hex like Swift's UUID().uuidString.
        let mut id_bytes = [0u8; 16];
        getrandom_fill(&mut id_bytes);
        let id = id_bytes
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<String>();
        let host = self
            .advertised_host
            .clone()
            .unwrap_or_else(LocalNetworkAddress::preferred_ipv4);
        let endpoint = format!("http://{}:{}/mobile/pairing-proxy/{}", host, self.port, id);
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if !state.listening {
            return None;
        }
        let expires_at = SystemTime::now() + Duration::from_secs(RESERVATION_TTL_SECS);
        state.active = Some(ActiveReservation {
            id: id.clone(),
            expires_at,
            provider: Arc::new(provider),
            forwarding: false,
        });
        Some(PairingProxyReservation { id, endpoint })
    }

    /// Drop the reservation with `id`, if it is the active one.
    pub fn cancel(&self, id: &str) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.active.as_ref().is_some_and(|a| a.id == id) {
            state.active = None;
        }
    }
}

impl Drop for ControllerPairingProxy {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Fill `buf` with random bytes; falls back to a time-seeded counter when
/// the OS RNG is unavailable (reservation ids only need uniqueness).
fn getrandom_fill(buf: &mut [u8]) {
    // Use /dev/urandom directly to avoid adding a dependency.
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(buf).is_ok() {
            return;
        }
    }
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for (i, b) in buf.iter_mut().enumerate() {
        *b = ((seed >> ((i % 8) * 8)) as u8).wrapping_add(i as u8);
    }
}

fn accept_loop(listener: TcpListener, state: Arc<Mutex<ProxyState>>) {
    for client in listener.incoming() {
        {
            let state = state.lock().unwrap_or_else(|p| p.into_inner());
            if !state.listening {
                return;
            }
        }
        let Ok(stream) = client else { continue };
        let _ = stream.set_nodelay(true);
        let _ = stream.set_read_timeout(Some(SOCKET_TIMEOUT));
        let _ = stream.set_write_timeout(Some(SOCKET_TIMEOUT));
        let conn_state = Arc::clone(&state);
        std::thread::spawn(move || handle_connection(stream, conn_state));
    }
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

/// Read one minimal HTTP/1.1 request: request line, headers, fixed-length
/// body. Rejects duplicate `Content-Length`, any `Transfer-Encoding`, and
/// oversized requests, mirroring the Swift parser.
fn read_request(stream: &mut TcpStream) -> Result<HttpRequest, MobileRemoteError> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let mut header_end: Option<usize> = None;
    let mut content_length: usize = 0;
    let mut request_line = String::new();

    loop {
        if header_end.is_none() {
            if let Some(pos) = find_crlf_crlf(&buffer) {
                let header_bytes = &buffer[..pos];
                let header = std::str::from_utf8(header_bytes)
                    .map_err(|_| MobileRemoteError::new(400, "bad request"))?;
                let mut lines = header.split("\r\n");
                request_line = lines.next().unwrap_or("").to_string();
                let mut content_length_header: Option<&str> = None;
                for line in lines {
                    let Some(colon) = line.find(':') else {
                        continue;
                    };
                    let name = line[..colon].trim().to_ascii_lowercase();
                    let value = line[colon + 1..].trim();
                    if name == "content-length" {
                        if content_length_header.is_some() {
                            return Err(MobileRemoteError::new(400, "duplicate content-length"));
                        }
                        content_length_header = Some(value);
                    } else if name == "transfer-encoding" {
                        return Err(MobileRemoteError::new(400, "unsupported transfer-encoding"));
                    }
                }
                content_length = match strict_content_length::parse(content_length_header) {
                    Ok(n) => n,
                    Err(strict_content_length::ParseError::TooLarge) => {
                        return Err(MobileRemoteError::new(400, "body too large"))
                    }
                    Err(strict_content_length::ParseError::Invalid) => {
                        return Err(MobileRemoteError::new(400, "invalid content-length"))
                    }
                };
                header_end = Some(pos + 4);
            }
        }

        if let Some(end) = header_end {
            if buffer.len() - end >= content_length {
                let parts: Vec<&str> = request_line.split(' ').collect();
                if parts.len() < 2 {
                    return Err(MobileRemoteError::new(400, "bad request"));
                }
                let raw_path = parts[1];
                // Reject query strings and fragments, mirroring
                // `components.query == nil && components.fragment == nil`.
                if raw_path.contains(['?', '#']) {
                    return Err(MobileRemoteError::new(400, "bad request"));
                }
                // Percent-decode the path the way URLComponents would.
                let path = percent_decode(raw_path);
                let body = buffer[end..end + content_length].to_vec();
                return Ok(HttpRequest {
                    method: parts[0].to_string(),
                    path,
                    body,
                });
            }
        }

        if buffer.len() > MAX_REQUEST_BYTES {
            return Err(MobileRemoteError::new(400, "request too large"));
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(MobileRemoteError::new(400, "bad request")),
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    }
}

fn find_crlf_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Minimal percent-decoding for request paths (`%XX` → byte).
fn percent_decode(raw: &str) -> String {
    let mut out = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Validate the exchange and run the provider: path must be
/// `/mobile/pairing-proxy/<id>/pair`, method POST, the reservation must be
/// live and not already forwarding (single-flight → 410 otherwise).
fn forward(
    state: &Arc<Mutex<ProxyState>>,
    method: &str,
    path: &str,
    body: Vec<u8>,
) -> Result<Vec<u8>, MobileRemoteError> {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() != 4 || parts[0] != "mobile" || parts[1] != "pairing-proxy" || parts[3] != "pair"
    {
        return Err(MobileRemoteError::new(404, "not found"));
    }
    if method != "POST" {
        return Err(MobileRemoteError::new(405, "method not allowed"));
    }
    let id = parts[2];

    // Claim the single-flight forwarding slot; expire stale reservations.
    let provider: Arc<dyn Fn(Vec<u8>) -> Result<Vec<u8>, MobileRemoteError> + Send + Sync>;
    {
        let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
        let now = SystemTime::now();
        let claimable = match state.active.as_mut() {
            Some(a) if a.id == id && a.expires_at > now && !a.forwarding => true,
            _ => false,
        };
        if !claimable {
            if state.active.as_ref().is_some_and(|a| a.expires_at <= now) {
                state.active = None;
            }
            return Err(MobileRemoteError::new(410, "pairing invitation expired"));
        }
        state.active.as_mut().unwrap().forwarding = true;
        provider = Arc::clone(&state.active.as_ref().unwrap().provider);
    }

    // Run the provider with a timeout, mirroring the 20s semaphore wait.
    // Swift hops to @MainActor; here the provider runs on a worker thread.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(provider(body));
    });
    let response = match rx.recv_timeout(PROVIDER_TIMEOUT) {
        Ok(r) => r,
        Err(_) => {
            release_forwarding_claim(state, id);
            return Err(MobileRemoteError::new(504, "pairing proxy timed out"));
        }
    };
    match response {
        Ok(bytes) => {
            if std::str::from_utf8(&bytes).is_err() {
                release_forwarding_claim(state, id);
                return Err(MobileRemoteError::new(
                    502,
                    "Host returned an invalid pairing response",
                ));
            }
            // Success consumes the one-shot reservation.
            let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
            if state.active.as_ref().is_some_and(|a| a.id == id) {
                state.active = None;
            }
            Ok(bytes)
        }
        Err(e) => {
            // MobileRemoteError from the provider keeps its status;
            // anything else would already be wrapped by the provider.
            release_forwarding_claim(state, id);
            Err(e)
        }
    }
}

fn release_forwarding_claim(state: &Arc<Mutex<ProxyState>>, id: &str) {
    let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(a) = state.active.as_mut() {
        if a.id == id {
            a.forwarding = false;
        }
    }
}

fn error_json(message: &str) -> String {
    // Minimal JSON string escaping for the error envelope.
    let escaped = message
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("{{\"error\":\"{escaped}\"}}")
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        410 => "Gone",
        502 => "Bad Gateway",
        504 => "Gateway Timeout",
        _ => "Error",
    }
}

fn handle_connection(mut stream: TcpStream, state: Arc<Mutex<ProxyState>>) {
    let (status, body) = match read_request(&mut stream) {
        Ok(req) => match forward(&state, &req.method, &req.path, req.body) {
            Ok(body) => (200, String::from_utf8_lossy(&body).into_owned()),
            Err(e) => (e.status, error_json(&e.message)),
        },
        Err(e) => (e.status, error_json(&e.message)),
    };
    let payload = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        reason_phrase(status),
        body.len(),
        body
    );
    let bytes = payload.as_bytes();
    let mut sent = 0;
    while sent < bytes.len() {
        match stream.write(&bytes[sent..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => sent += n,
        }
    }
}

/// LAN address selection. Swift: `LocalNetworkAddress`.
pub struct LocalNetworkAddress;

impl LocalNetworkAddress {
    /// Best IPv4 address to advertise for the pairing-proxy URL: prefer
    /// `en0`, then other `en*`, then anything up and running that is not
    /// loopback; fall back to 127.0.0.1. Swift:
    /// `LocalNetworkAddress.preferredIPv4`.
    ///
    /// Interface enumeration needs platform APIs; this pure selection over
    /// `(interface_name, address)` candidates is the testable core, and the
    /// live lookup below shells out to the OS.
    pub fn select_preferred_ipv4(candidates: &[(&str, &str)]) -> Option<String> {
        candidates
            .iter()
            .min_by(|a, b| {
                let ra = interface_rank(a.0);
                let rb = interface_rank(b.0);
                ra.cmp(&rb)
                    .then_with(|| a.0.cmp(b.0))
                    .then_with(|| a.1.cmp(b.1))
            })
            .map(|(_, addr)| addr.to_string())
    }

    /// Live lookup of the preferred IPv4 address. Parses `ifconfig`
    /// output; mirrors the getifaddrs walk (skip lo/utun/awdl/llw/bridge,
    /// require UP+RUNNING, skip 127.x).
    pub fn preferred_ipv4() -> String {
        let candidates = list_ipv4_candidates();
        Self::select_preferred_ipv4(
            &candidates
                .iter()
                .map(|(n, a)| (n.as_str(), a.as_str()))
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|| "127.0.0.1".to_string())
    }
}

fn interface_rank(name: &str) -> u8 {
    if name == "en0" {
        0
    } else if name.starts_with("en") {
        1
    } else {
        2
    }
}

/// Enumerate `(interface, ipv4)` candidates via `ifconfig`.
fn list_ipv4_candidates() -> Vec<(String, String)> {
    let skipped = ["lo", "utun", "awdl", "llw", "bridge"];
    let output = std::process::Command::new("ifconfig")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let mut candidates = Vec::new();
    let mut current: Option<(String, bool)> = None; // (name, up_and_running)
    for line in output.lines() {
        if !line.starts_with('\t') && !line.starts_with(' ') {
            // Interface header, e.g. "en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500"
            if let Some(colon) = line.find(':') {
                let name = line[..colon].to_string();
                let flags = line.contains("<UP,") || line.contains(",UP,");
                let running = line.contains(",RUNNING,") || line.contains("<RUNNING,");
                current = Some((name, flags && running));
            }
        } else if let Some((name, up)) = current.as_ref() {
            let trimmed = line.trim();
            if trimmed.starts_with("inet ") {
                let addr = trimmed.split_whitespace().nth(1).unwrap_or("").to_string();
                if !addr.starts_with("127.") && *up && !skipped.iter().any(|p| name.starts_with(p))
                {
                    candidates.push((name.clone(), addr));
                }
            }
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::strict_content_length::{parse, ParseError, MAXIMUM};
    use super::*;

    #[test]
    fn strict_content_length_accepts_digits_within_maximum() {
        assert_eq!(parse(None), Ok(0));
        assert_eq!(parse(Some("0")), Ok(0));
        assert_eq!(parse(Some("123")), Ok(123));
        assert_eq!(parse(Some(&MAXIMUM.to_string())), Ok(MAXIMUM));
    }

    #[test]
    fn strict_content_length_rejects_non_digits() {
        assert_eq!(parse(Some("")), Err(ParseError::Invalid));
        assert_eq!(parse(Some("-5")), Err(ParseError::Invalid));
        assert_eq!(parse(Some("+5")), Err(ParseError::Invalid));
        assert_eq!(parse(Some("1,000")), Err(ParseError::Invalid));
        assert_eq!(parse(Some("12 34")), Err(ParseError::Invalid));
        assert_eq!(parse(Some("0x10")), Err(ParseError::Invalid));
        assert_eq!(parse(Some("  12")), Err(ParseError::Invalid));
    }

    #[test]
    fn strict_content_length_rejects_too_large() {
        assert_eq!(
            parse(Some(&(MAXIMUM + 1).to_string())),
            Err(ParseError::TooLarge)
        );
        assert_eq!(
            parse(Some("99999999999999999999")),
            Err(ParseError::TooLarge)
        );
    }

    #[test]
    fn interface_rank_prefers_en0_then_en_then_other() {
        assert_eq!(
            LocalNetworkAddress::select_preferred_ipv4(&[
                ("eth0", "192.168.1.5"),
                ("en1", "192.168.1.6"),
                ("en0", "192.168.1.4"),
            ]),
            Some("192.168.1.4".to_string())
        );
        assert_eq!(
            LocalNetworkAddress::select_preferred_ipv4(&[
                ("eth0", "10.0.0.2"),
                ("en3", "10.0.0.3")
            ]),
            Some("10.0.0.3".to_string())
        );
    }

    #[test]
    fn interface_rank_breaks_ties_by_name_then_address() {
        assert_eq!(
            LocalNetworkAddress::select_preferred_ipv4(&[
                ("en1", "192.168.1.9"),
                ("en1", "192.168.1.2"),
            ]),
            Some("192.168.1.2".to_string())
        );
        assert_eq!(LocalNetworkAddress::select_preferred_ipv4(&[]), None);
    }

    #[test]
    fn proxy_reserve_mints_pairing_proxy_url() {
        let proxy = ControllerPairingProxy::new(Some("127.0.0.1".to_string())).unwrap();
        let r = proxy
            .reserve(|body| Ok(body))
            .expect("reserve must succeed while listening");
        assert!(r.endpoint.starts_with("http://127.0.0.1:"));
        assert!(r.endpoint.contains("/mobile/pairing-proxy/"));
        assert!(r.endpoint.ends_with(&r.id));
        assert_eq!(r.id.len(), 32);
        assert!(r.id.chars().all(|c| c.is_ascii_hexdigit()));
        proxy.stop();
    }

    #[test]
    fn proxy_rejects_unknown_paths_with_404() {
        let proxy = ControllerPairingProxy::new(Some("127.0.0.1".to_string())).unwrap();
        let r = proxy.reserve(|_| Ok(b"{}".to_vec())).unwrap();
        let url = r.endpoint.replace(
            &format!("/mobile/pairing-proxy/{}", r.id),
            "/mobile/bootstrap",
        );
        let body = post(&url, b"{}");
        assert!(body.contains("\"error\":\"not found\""), "{body}");
        proxy.stop();
    }

    #[test]
    fn proxy_rejects_wrong_method_with_405() {
        let proxy = ControllerPairingProxy::new(Some("127.0.0.1".to_string())).unwrap();
        let r = proxy.reserve(|_| Ok(b"{}".to_vec())).unwrap();
        let (status, _) = raw_request(&format!("{}/pair", r.endpoint), "GET", b"");
        assert_eq!(status, 405);
        proxy.stop();
    }

    #[test]
    fn proxy_forwards_once_then_expires() {
        let proxy = ControllerPairingProxy::new(Some("127.0.0.1".to_string())).unwrap();
        let r = proxy
            .reserve(|body| {
                assert_eq!(body, b"ping");
                Ok(b"{\"ok\":true}".to_vec())
            })
            .unwrap();
        let pair_url = format!("{}/pair", r.endpoint);
        let first = post(&pair_url, b"ping");
        assert!(first.contains("\"ok\":true"), "{first}");
        // Second exchange hits the consumed reservation: 410.
        let (status, body) = raw_request(&pair_url, "POST", b"ping");
        assert_eq!(status, 410);
        assert!(body.contains("pairing invitation expired"), "{body}");
        proxy.stop();
    }

    #[test]
    fn proxy_cancel_drops_reservation() {
        let proxy = ControllerPairingProxy::new(Some("127.0.0.1".to_string())).unwrap();
        let r = proxy.reserve(|_| Ok(b"{}".to_vec())).unwrap();
        proxy.cancel(&r.id);
        let pair_url = format!("{}/pair", r.endpoint);
        let (status, _) = raw_request(&pair_url, "POST", b"{}");
        assert_eq!(status, 410);
        proxy.stop();
    }

    #[test]
    fn proxy_rejects_duplicate_content_length() {
        let proxy = ControllerPairingProxy::new(Some("127.0.0.1".to_string())).unwrap();
        let r = proxy.reserve(|_| Ok(b"{}".to_vec())).unwrap();
        let pair_url = format!("{}/pair", r.endpoint);
        let (status, _) = raw_request_with_headers(
            &pair_url,
            "POST",
            &[("Content-Length", "2"), ("Content-Length", "2")],
            b"{}",
        );
        assert_eq!(status, 400);
        proxy.stop();
    }

    #[test]
    fn proxy_rejects_transfer_encoding() {
        let proxy = ControllerPairingProxy::new(Some("127.0.0.1".to_string())).unwrap();
        let r = proxy.reserve(|_| Ok(b"{}".to_vec())).unwrap();
        let pair_url = format!("{}/pair", r.endpoint);
        let (status, _) = raw_request_with_headers(
            &pair_url,
            "POST",
            &[("Transfer-Encoding", "chunked")],
            b"{}",
        );
        assert_eq!(status, 400);
        proxy.stop();
    }

    #[test]
    fn proxy_maps_provider_error_status() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_clone = Arc::clone(&calls);
        let proxy = ControllerPairingProxy::new(Some("127.0.0.1".to_string())).unwrap();
        let r = proxy
            .reserve(move |_| {
                let n = calls_clone.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err(MobileRemoteError::new(400, "bad envelope"))
                } else {
                    Ok(b"{\"ok\":true}".to_vec())
                }
            })
            .unwrap();
        let pair_url = format!("{}/pair", r.endpoint);
        let (status, body) = raw_request(&pair_url, "POST", b"{}");
        assert_eq!(status, 400);
        assert!(body.contains("bad envelope"), "{body}");
        // A failed exchange releases the forwarding claim without consuming
        // the one-shot reservation: the next attempt retries the provider.
        let (status2, body2) = raw_request(&pair_url, "POST", b"{}");
        assert_eq!(status2, 200);
        assert!(body2.contains("\"ok\":true"), "{body2}");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        proxy.stop();
    }

    // --- test helpers: minimal blocking HTTP client ---

    fn post(url: &str, body: &[u8]) -> String {
        let (_, text) = raw_request(url, "POST", body);
        text
    }

    fn raw_request(url: &str, method: &str, body: &[u8]) -> (u16, String) {
        raw_request_with_headers(
            url,
            method,
            &[("Content-Length", &body.len().to_string())],
            body,
        )
    }

    fn raw_request_with_headers(
        url: &str,
        method: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> (u16, String) {
        let url = url.strip_prefix("http://").unwrap();
        let (host_port, path) = url.split_once('/').unwrap();
        let path = format!("/{path}");
        let mut stream = TcpStream::connect(host_port).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {host_port}\r\n");
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str("\r\n");
        stream.write_all(req.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut resp = Vec::new();
        stream.read_to_end(&mut resp).unwrap();
        let text = String::from_utf8_lossy(&resp).into_owned();
        let status: u16 = text
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        (status, text)
    }
}
