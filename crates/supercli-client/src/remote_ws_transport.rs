//! Live WebSocket transport for the terminal output stream.
//!
//! Port of `RemoteTerminalWebSocket.swift` (SupercliIOS): the
//! certificate-pinned WebSocket connection
//! (`RemoteTerminalWebSocketConnection`), the ping watchdog, the input
//! router (`RemoteTerminalInputRouter`), and the endpoint discovery holder
//! (`RemoteServerDiscovery`).
//!
//! The portable decision logic (error enum, routing rules) lives in
//! [`crate::remote_terminal_websocket`]; the wire protocol (hello/error
//! frames, binary frames, input encoding) lives in
//! [`crate::terminal_stream`].
//!
//! Native only: needs OS sockets (tungstenite), threads (ping watchdog),
//! and rustls (certificate pinning). The Swift original uses
//! `URLSessionWebSocketTask`; the Rust port uses blocking tungstenite on a
//! dedicated thread model, mirroring `relay_conn.rs`.

use std::io;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::client::IntoClientRequest;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Connector, Message, WebSocket};

use crate::remote_terminal_websocket::{RemoteTerminalWebSocketError, WS_SEND_TIMEOUT_SECS};
use crate::terminal_stream::{
    terminal_server_endpoint, web_socket_output_url, RemoteServerEndpoint,
    RemoteTerminalWsClientMessage, RemoteTerminalWsHello, RemoteTerminalWsServerMessage,
};
use crate::tls::pinned_client_config;

/// Generous handshake bound: steady-state liveness is owned by the ping
/// watchdog (server pings keep healthy idle streams alive); this only
/// bounds a wedged TCP/TLS/WS handshake.
///
/// Port of `configuration.timeoutIntervalForRequest = 120` in
/// `RemoteTerminalWebSocketConnection.init`.
pub const WS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(120);

/// Replay bursts can exceed tungstenite's 1MB default.
///
/// Port of `task.maximumMessageSize = 16 * 1024 * 1024`.
pub const WS_MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// How long to wait for the server's hello frame after connect.
///
/// Port of `receiveHello(timeout: .seconds(10))`.
pub const WS_HELLO_TIMEOUT: Duration = Duration::from_secs(10);

/// Ping interval for the watchdog.
///
/// Port of `Task.sleep(for: .seconds(30))` in `startPingWatchdog`.
pub const WS_PING_INTERVAL: Duration = Duration::from_secs(30);

/// If no frame (or successful ping) has been seen for this long, the
/// connection is presumed silently dead (sleep, Wi-Fi drop) and aborted
/// so the read loop unblocks and falls back to HTTP.
///
/// The Swift aborts on a single failed pong; the Rust port also covers
/// the case where the watchdog thread cannot acquire the socket because
/// a reader is blocked on a black-holed TCP connection.
pub const WS_STALE_TIMEOUT: Duration = Duration::from_secs(90);

type WsStream = WebSocket<MaybeTlsStream<TcpStream>>;

/// Why a WebSocket connect attempt failed. The caller maps any of these
/// to "no WS available" and falls back to the HTTP long-poll, mirroring
/// the Swift renderer which stays on HTTP when the socket never comes up.
#[derive(Debug, thiserror::Error)]
pub enum WsTransportConnectError {
    /// The output URL could not be built (empty host/session).
    #[error("invalid WebSocket URL")]
    BadUrl,
    /// The fingerprint is malformed — pinning cannot proceed, and there
    /// is deliberately no bypass.
    #[error("invalid certificate fingerprint for pinning")]
    BadFingerprint,
    /// TCP dial failed.
    #[error("TCP connect failed: {0}")]
    Tcp(String),
    /// The TLS handshake failed (includes pin mismatch — the server's
    /// leaf SHA-256 did not equal the pinned fingerprint).
    #[error("TLS handshake failed: {0}")]
    Tls(String),
    /// The WebSocket upgrade handshake failed.
    #[error("WebSocket handshake failed: {0}")]
    Handshake(String),
}

/// Blocking TCP dial with a connect timeout.
///
/// Mirrors `relay_tcp_connect` in `relay_conn.rs`: DNS + dial + nodelay,
/// without the redirect support the client never uses.
fn ws_tcp_connect(host: &str, port: u16, timeout: Duration) -> io::Result<TcpStream> {
    let mut last_err = None;
    for addr in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => {
                stream.set_nodelay(true)?;
                return Ok(stream);
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("could not resolve WS host {host}"),
        )
    }))
}

/// One live terminal-output WebSocket connection.
///
/// Port of `RemoteTerminalWebSocketConnection`: owns the pinned TLS
/// socket, runs the ping watchdog on a background thread, and is
/// thread-safe (mutable socket behind a mutex) because the renderer's
/// read loop and the input router's sends run on different threads.
pub struct RemoteTerminalWsTransport {
    ws: Arc<Mutex<WsStream>>,
    /// Cloned TCP handle to the same socket: shutting it down unblocks a
    /// thread stuck in `read()`, and socket options (read/write timeouts)
    /// set on it apply to the shared socket.
    shutdown: TcpStream,
    closed: Arc<AtomicBool>,
    last_activity: Arc<Mutex<Instant>>,
    watchdog: Option<thread::JoinHandle<()>>,
}

impl std::fmt::Debug for RemoteTerminalWsTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteTerminalWsTransport")
            .field("closed", &self.closed.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

impl RemoteTerminalWsTransport {
    /// Connect to the terminal output WebSocket.
    ///
    /// `host` is the paired Host's address; `endpoint` carries the
    /// OS-assigned port and the pinned leaf SHA-256 from discovery;
    /// `token` is the paired Bearer <redacted> `offset` resumes a
    /// previous stream.
    ///
    /// TLS uses the pinned leaf certificate only — no WebPKI roots, no
    /// bypass. A pin mismatch fails the handshake.
    pub fn connect(
        host: &str,
        endpoint: &RemoteServerEndpoint,
        session_id: &str,
        token: &str,
        offset: Option<u64>,
    ) -> Result<Self, WsTransportConnectError> {
        let url = web_socket_output_url(host, endpoint.port, session_id, token, offset)
            .ok_or(WsTransportConnectError::BadUrl)?;

        // Exact pin, no bypass: a mismatch (or an unreadable chain)
        // fails the handshake, mirroring
        // `RemoteCertificatePinningDelegate`.
        let tls_config = pinned_client_config(&endpoint.certificate_fingerprint)
            .ok_or(WsTransportConnectError::BadFingerprint)?;
        let connector = Connector::Rustls(tls_config);

        let request = url
            .into_client_request()
            .map_err(|_| WsTransportConnectError::BadUrl)?;

        let host_for_dial = host.to_string();
        let port = endpoint.port;
        let (tx, rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let mut config = WebSocketConfig::default();
            config.max_message_size = Some(WS_MAX_MESSAGE_SIZE);
            let result = ws_tcp_connect(&host_for_dial, port, WS_HANDSHAKE_TIMEOUT)
                .map_err(|e| WsTransportConnectError::Tcp(e.to_string()))
                .and_then(|stream| {
                    // Clone before the handshake consumes the stream: the
                    // clone lets the watchdog abort a black-holed socket.
                    let shutdown = stream
                        .try_clone()
                        .map_err(|e| WsTransportConnectError::Tcp(e.to_string()))?;
                    tungstenite::client_tls_with_config(
                        request,
                        stream,
                        Some(config),
                        Some(connector),
                    )
                    .map(|(ws, _response)| (ws, shutdown))
                    .map_err(|e| match e {
                        tungstenite::handshake::HandshakeError::Failure(f) => {
                            // TLS and HTTP-upgrade failures surface here;
                            // pin mismatches arrive as TLS alerts.
                            WsTransportConnectError::Tls(f.to_string())
                        }
                        tungstenite::handshake::HandshakeError::Interrupted(_) => {
                            WsTransportConnectError::Handshake(
                                "handshake interrupted on blocking socket".to_string(),
                            )
                        }
                    })
                });
            let _ = tx.send(result);
        });
        let (ws, shutdown) = rx
            .recv_timeout(WS_HANDSHAKE_TIMEOUT)
            .map_err(|_| WsTransportConnectError::Tcp("WS connect timed out".to_string()))??;

        let closed = Arc::new(AtomicBool::new(false));
        let last_activity = Arc::new(Mutex::new(Instant::now()));
        let mut transport = Self {
            ws: Arc::new(Mutex::new(ws)),
            shutdown,
            closed: Arc::clone(&closed),
            last_activity: Arc::clone(&last_activity),
            watchdog: None,
        };
        transport.start_ping_watchdog();
        Ok(transport)
    }

    /// Whether the connection has been closed or aborted.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Read the next server message, auto-answering pings.
    ///
    /// Returns `Text` (hello/error frames) and `Binary` (output frames).
    /// `Ping` is answered with `Pong` internally; `Pong` only proves
    /// liveness. `Close` marks the transport closed.
    ///
    /// Port of `receive()`; the cancellation handler (which aborts the
    /// socket when the caller's task is cancelled) is replaced by
    /// [`Self::close`] / watchdog abort, which shut the TCP stream down
    /// and unblock this call with an error.
    pub fn receive(&self) -> Result<Message, RemoteTerminalWebSocketError> {
        loop {
            let msg = {
                let mut ws = self
                    .ws
                    .lock()
                    .map_err(|_| RemoteTerminalWebSocketError::Closed)?;
                ws.read()
                    .map_err(|_| RemoteTerminalWebSocketError::Closed)?
            };
            *self.last_activity.lock().unwrap() = Instant::now();
            match msg {
                Message::Ping(payload) => {
                    // Answer pings on read, like URLSession does
                    // internally; the pong proves the path without
                    // surfacing to the caller.
                    let mut ws = self
                        .ws
                        .lock()
                        .map_err(|_| RemoteTerminalWebSocketError::Closed)?;
                    let _ = ws.send(Message::Pong(payload));
                    let _ = ws.flush();
                    continue;
                }
                Message::Pong(_) => continue,
                Message::Close(_) => {
                    self.mark_closed();
                    return Err(RemoteTerminalWebSocketError::Closed);
                }
                Message::Text(_) | Message::Binary(_) => return Ok(msg),
                Message::Frame(_) => continue,
            }
        }
    }

    /// The server's first frame must be the hello text frame; anything
    /// else (or silence) means an incompatible/unhealthy server — the
    /// caller falls back to HTTP.
    ///
    /// Port of `receiveHello(timeout:)`.
    pub fn receive_hello(&self) -> Result<RemoteTerminalWsHello, RemoteTerminalWebSocketError> {
        self.receive_hello_with_timeout(WS_HELLO_TIMEOUT)
    }

    fn receive_hello_with_timeout(
        &self,
        timeout: Duration,
    ) -> Result<RemoteTerminalWsHello, RemoteTerminalWebSocketError> {
        // Bound the blocking read so a silent server cannot hang the
        // caller past the hello deadline.
        self.shutdown
            .set_read_timeout(Some(timeout))
            .map_err(|_| RemoteTerminalWebSocketError::Closed)?;
        let result = (|| {
            let msg = self.receive()?;
            let Message::Text(text) = msg else {
                return Err(RemoteTerminalWebSocketError::ProtocolViolation);
            };
            match RemoteTerminalWsServerMessage::parse(&text) {
                RemoteTerminalWsServerMessage::Hello(hello) => Ok(hello),
                _ => Err(RemoteTerminalWebSocketError::ProtocolViolation),
            }
        })();
        // Clear the timeout; a timed-out read surfaces as WouldBlock/
        // TimedOut, which maps to HelloTimeout below.
        let _ = self.shutdown.set_read_timeout(None);
        match result {
            Err(RemoteTerminalWebSocketError::Closed) => {
                // Distinguish a read timeout (server silent) from a real
                // close: if we are not marked closed, the deadline fired.
                if self.is_closed() {
                    Err(RemoteTerminalWebSocketError::Closed)
                } else {
                    Err(RemoteTerminalWebSocketError::HelloTimeout)
                }
            }
            other => other,
        }
    }

    /// Raw PTY input as ordered JSON text frames (no ack — the echo
    /// arrives via the output stream).
    ///
    /// Port of `sendInput(_:writeID:)`.
    pub fn send_input(
        &self,
        text: &str,
        write_id: Option<&str>,
    ) -> Result<(), RemoteTerminalWebSocketError> {
        if self.is_closed() {
            return Err(RemoteTerminalWebSocketError::Closed);
        }
        // The Swift router guarantees single-frame sends (large pastes
        // go over HTTP); the frame splitter stays as defense in depth.
        let frames = RemoteTerminalWsClientMessage::input_frames(
            text,
            write_id,
            RemoteTerminalWsClientMessage::MAX_INPUT_BYTES_PER_FRAME,
        );
        let mut ws = self
            .ws
            .lock()
            .map_err(|_| RemoteTerminalWebSocketError::Closed)?;
        for frame in frames {
            ws.send(Message::Text(frame.into()))
                .map_err(|_| RemoteTerminalWebSocketError::Closed)?;
        }
        ws.flush()
            .map_err(|_| RemoteTerminalWebSocketError::Closed)?;
        *self.last_activity.lock().unwrap() = Instant::now();
        Ok(())
    }

    /// Send input, bounding the whole attempt (lock acquisition + write)
    /// by `timeout`. A WS that reported "not closed" can still be
    /// half-dead; without this bound every queued keystroke would wait
    /// behind the stalled send.
    ///
    /// Port of the `withTimeout(seconds: 0.5)` wrapper in
    /// `RemoteTerminalInputRouter.send(_:)`.
    pub fn send_input_bounded(
        &self,
        text: &str,
        write_id: Option<&str>,
        timeout: Duration,
    ) -> Result<(), RemoteTerminalWebSocketError> {
        if self.is_closed() {
            return Err(RemoteTerminalWebSocketError::Closed);
        }
        let deadline = Instant::now() + timeout;
        // Poll the lock: a reader blocked in `read()` holds it, and a
        // black-holed socket would stall a blocking `lock()` forever.
        let mut ws_guard = loop {
            match self.ws.try_lock() {
                Ok(g) => break g,
                Err(_) => {
                    if Instant::now() >= deadline || self.is_closed() {
                        return Err(RemoteTerminalWebSocketError::Closed);
                    }
                    thread::sleep(Duration::from_millis(5));
                }
            }
        };
        // Bound the write itself via the socket's write timeout (shared
        // across the cloned handles).
        let remaining = deadline.saturating_duration_since(Instant::now());
        self.shutdown
            .set_write_timeout(Some(remaining))
            .map_err(|_| RemoteTerminalWebSocketError::Closed)?;
        let frames = RemoteTerminalWsClientMessage::input_frames(
            text,
            write_id,
            RemoteTerminalWsClientMessage::MAX_INPUT_BYTES_PER_FRAME,
        );
        let result = (|| {
            for frame in frames {
                ws_guard
                    .send(Message::Text(frame.into()))
                    .map_err(|_| RemoteTerminalWebSocketError::Closed)?;
            }
            ws_guard
                .flush()
                .map_err(|_| RemoteTerminalWebSocketError::Closed)
        })();
        let _ = self.shutdown.set_write_timeout(None);
        if result.is_ok() {
            *self.last_activity.lock().unwrap() = Instant::now();
        }
        result
    }

    /// Close the connection: stop the watchdog, mark closed, and shut
    /// the socket down (unblocks any thread in `receive()`).
    ///
    /// Port of `close()`.
    pub fn close(mut self) {
        self.mark_closed();
        let _ = self.shutdown.shutdown(Shutdown::Both);
        if let Some(handle) = self.watchdog.take() {
            let _ = handle.join();
        }
    }

    fn mark_closed(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    /// Detects a silently dead server (sleep, Wi-Fi drop): every
    /// [`WS_PING_INTERVAL`] the watchdog sends a ping; a failed ping —
    /// or no observed activity for [`WS_STALE_TIMEOUT`] — aborts the
    /// socket so the receive loop unblocks and the stream falls back to
    /// HTTP instead of hanging on a black-holed TCP connection.
    ///
    /// Port of `startPingWatchdog`.
    fn start_ping_watchdog(&mut self) {
        let ws = Arc::clone(&self.ws);
        let closed = Arc::clone(&self.closed);
        let last_activity = Arc::clone(&self.last_activity);
        let abort_closed = Arc::clone(&self.closed);
        let abort_shutdown = self
            .shutdown
            .try_clone()
            .expect("clone watchdog shutdown handle");
        self.watchdog = Some(thread::spawn(move || {
            let abort = || {
                abort_closed.store(true, Ordering::SeqCst);
                let _ = abort_shutdown.shutdown(Shutdown::Both);
            };
            loop {
                // Sleep in 1s increments so `close()` wakes the thread
                // promptly instead of blocking up to WS_PING_INTERVAL
                // on `join()`.
                for _ in 0..WS_PING_INTERVAL.as_secs() {
                    thread::sleep(Duration::from_secs(1));
                    if closed.load(Ordering::SeqCst) {
                        return;
                    }
                }
                if closed.load(Ordering::SeqCst) {
                    return;
                }
                // `try_lock`: a reader blocked in `read()` on a dead
                // socket holds the mutex; skipping the ping that round
                // is fine — the staleness check below still fires.
                let ping_ok = match ws.try_lock() {
                    Ok(mut ws) => ws.send(Message::Ping(Vec::new().into())).is_ok(),
                    Err(_) => true,
                };
                if !ping_ok {
                    abort();
                    return;
                }
                // A successful ping send counts as activity (the pong
                // will also refresh it on the read path).
                if let Ok(mut last) = last_activity.lock() {
                    *last = Instant::now();
                }
                let stale = last_activity
                    .lock()
                    .map(|last| last.elapsed() > WS_STALE_TIMEOUT)
                    .unwrap_or(false);
                if stale {
                    abort();
                    return;
                }
            }
        }));
    }
}

impl Drop for RemoteTerminalWsTransport {
    fn drop(&mut self) {
        self.mark_closed();
        let _ = self.shutdown.shutdown(Shutdown::Both);
        // Do not join here: `close()` joins explicitly; dropping while
        // the watchdog holds no locks is safe because every shared
        // handle is an Arc.
    }
}

/// Latest advertised `__remote__` endpoint for the connected Host.
///
/// Port of `RemoteServerDiscovery`: the Swift is a `@MainActor`
/// `ObservableObject` with a `@Published` endpoint; the actor/publisher
/// parts are UI-layer. This is the thread-safe holder the stream loop
/// polls: bootstrap polls refresh it (the port is OS-assigned per
/// server run, so the renderer reads the freshest value before every
/// (re)connect), pairing seeds it, unpairing clears it. `None` means
/// "no WS available" and the stream stays on HTTP.
#[derive(Debug, Default)]
pub struct RemoteServerDiscovery {
    endpoint: Mutex<Option<RemoteServerEndpoint>>,
}

impl RemoteServerDiscovery {
    pub fn new() -> Self {
        Self::default()
    }

    /// Port of `update(port:certificateFingerprint:)`: resolves through
    /// [`terminal_server_endpoint`] (nil port or bad fingerprint yields
    /// `None`) and only stores when the value changed.
    pub fn update(&self, port: Option<u16>, certificate_fingerprint: Option<&str>) {
        let next = terminal_server_endpoint(port, certificate_fingerprint);
        let mut guard = self.endpoint.lock().unwrap();
        if *guard != next {
            *guard = next;
        }
    }

    /// Port of `clear()`.
    pub fn clear(&self) {
        let mut guard = self.endpoint.lock().unwrap();
        if guard.is_some() {
            *guard = None;
        }
    }

    /// The latest endpoint, if the Host currently advertises one.
    pub fn endpoint(&self) -> Option<RemoteServerEndpoint> {
        self.endpoint.lock().unwrap().clone()
    }
}

/// Routes the renderer's ordered input queue through the active
/// transport: the WS text frame while a connection is healthy, the
/// proven HTTP write otherwise. A WS send failure still lands the bytes
/// over HTTP — failed keystrokes are input the Host never saw.
///
/// Port of `RemoteTerminalInputRouter`. The Swift is `@unchecked
/// Sendable` with an `NSLock`; the Rust port is `Send` via the mutex.
/// `H` is the HTTP fallback: it receives the input text and the
/// idempotency key for the same logical send first attempted over WS.
pub struct RemoteTerminalInputRouter<H> {
    transport: Mutex<Option<Arc<RemoteTerminalWsTransport>>>,
    http_send: H,
}

impl<H> RemoteTerminalInputRouter<H>
where
    H: Fn(&str, &str) -> Result<(), RouterHttpError> + Send + Sync,
{
    pub fn new(http_send: H) -> Self {
        Self {
            transport: Mutex::new(None),
            http_send,
        }
    }

    /// Port of `adoptWebSocket`.
    pub fn adopt(&self, transport: Arc<RemoteTerminalWsTransport>) {
        *self.transport.lock().unwrap() = Some(transport);
    }

    /// Port of `retireWebSocket`: clears only if `transport` is still
    /// the active one — a newer attempt may have installed itself first.
    pub fn retire(&self, transport: &Arc<RemoteTerminalWsTransport>) {
        let mut guard = self.transport.lock().unwrap();
        if let Some(active) = guard.as_ref() {
            if Arc::ptr_eq(active, transport) {
                *guard = None;
            }
        }
    }

    fn healthy_transport(&self) -> Option<Arc<RemoteTerminalWsTransport>> {
        let guard = self.transport.lock().unwrap();
        let t = guard.as_ref()?;
        if t.is_closed() {
            return None;
        }
        Some(Arc::clone(t))
    }

    /// Port of `send(_:)`:
    /// - one idempotency key per logical send, shared by both
    ///   transports (an ambiguous WS delivery reuses the key on the
    ///   HTTP fallback so the Host applies the keystroke once);
    /// - large pastes go directly over HTTP (a split WS send cannot be
    ///   made idempotent);
    /// - the WS attempt is bounded by [`WS_SEND_TIMEOUT_SECS`]; on
    ///   stall/failure the socket is retired and the bytes fall through
    ///   to HTTP.
    pub fn send(&self, text: &str) -> Result<(), RouterError> {
        let write_id = new_write_id();
        if text.len() > RemoteTerminalWsClientMessage::MAX_INPUT_BYTES_PER_FRAME {
            return (self.http_send)(text, &write_id).map_err(RouterError::Http);
        }
        if let Some(transport) = self.healthy_transport() {
            let timeout = Duration::from_secs_f64(WS_SEND_TIMEOUT_SECS);
            match transport.send_input_bounded(text, Some(&write_id), timeout) {
                Ok(()) => return Ok(()),
                Err(_) => self.retire(&transport),
            }
        }
        (self.http_send)(text, &write_id).map_err(RouterError::Http)
    }
}

/// HTTP fallback failure from the input router.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("HTTP input send failed: {0}")]
pub struct RouterHttpError(pub String);

/// Input router outcome.
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error(transparent)]
    Http(RouterHttpError),
    #[error("websocket failed and HTTP fallback failed: {0}")]
    HttpAfterWs(RouterHttpError),
}

/// 128-bit random hex idempotency key, like Swift's
/// `UUID().uuidString` (lowercased hex without dashes is fine — the
/// Host treats it as an opaque key).
fn new_write_id() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::getrandom(&mut bytes).is_err() {
        // Fallback: nanos timestamp folded into 16 bytes. Uniqueness
        // only needs to hold per connection.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (nanos >> ((i % 16) * 8)) as u8;
        }
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_stream::normalize_terminal_fingerprint;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use rustls::{ServerConfig, ServerConnection};
    use sha2::{Digest, Sha256};
    use std::net::TcpListener;
    use std::sync::Arc as StdArc;

    /// Generate a self-signed test certificate for 127.0.0.1 at runtime.
    /// Returns (cert_der, key_der, fingerprint_hex). Mirrors
    /// `tls_e2e.rs` so no key material lives in the repo.
    fn generate_test_cert() -> (Vec<u8>, Vec<u8>, String) {
        let certified =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).expect("cert");
        let cert_der = certified.cert.der().to_vec();
        let key_der = certified.key_pair.serialize_der();
        let fingerprint = Sha256::digest(&cert_der)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        (cert_der, key_der, fingerprint)
    }

    /// Mock WSS Host: TLS with the test cert, tungstenite server
    /// handshake, one hello text frame, then echoes input frames back
    /// as binary output and answers pings (tungstenite server does not
    /// auto-pong; the test driver does).
    fn mock_wss_host(
        listener: TcpListener,
        cert_der: Vec<u8>,
        key_der: Vec<u8>,
        hello_json: String,
    ) -> thread::JoinHandle<Vec<String>> {
        thread::spawn(move || {
            let (tcp, _) = listener.accept().expect("client connects");
            let cert = CertificateDer::from(cert_der);
            let key = PrivateKeyDer::Pkcs8(key_der.into());
            let config = ServerConfig::builder_with_provider(StdArc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_protocol_versions(rustls::ALL_VERSIONS)
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap();
            let tls = ServerConnection::new(StdArc::new(config)).unwrap();
            let stream = rustls::StreamOwned::new(tls, tcp);
            // A client that rejects the pin aborts the TLS handshake;
            // that is the expected outcome for negative tests.
            let mut ws = match tungstenite::accept(stream) {
                Ok(ws) => ws,
                Err(_) => return Vec::new(),
            };

            // First frame must be the hello.
            ws.send(Message::Text(hello_json.into())).expect("hello");

            let mut received_inputs = Vec::new();
            // Serve a few messages, then close.
            for _ in 0..8 {
                match ws.read() {
                    Ok(Message::Text(text)) => {
                        received_inputs.push(text.to_string());
                    }
                    Ok(Message::Ping(payload)) => {
                        ws.send(Message::Pong(payload)).expect("pong");
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            received_inputs
        })
    }

    fn hello_json() -> String {
        serde_json::json!({
            "type": "hello",
            "protocol": 1,
            "session_id": "sess-1",
            "state": "live",
            "output_size": 128,
            "requested_offset": null,
            "start_offset": 0,
            "rebased": false,
            "cols": 80,
            "rows": 24,
        })
        .to_string()
    }

    fn test_endpoint(port: u16, fingerprint: &str) -> RemoteServerEndpoint {
        RemoteServerEndpoint {
            port,
            certificate_fingerprint: fingerprint.to_string(),
        }
    }

    #[test]
    fn connect_and_hello_with_pinned_cert() {
        let (cert_der, key_der, fingerprint) = generate_test_cert();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let server = mock_wss_host(listener, cert_der, key_der, hello_json());

        let transport = RemoteTerminalWsTransport::connect(
            "127.0.0.1",
            &test_endpoint(port, &fingerprint),
            "sess-1",
            "tok",
            None,
        )
        .expect("pinned WS connect");
        let hello = transport.receive_hello().expect("hello frame");
        assert_eq!(hello.session_id, "sess-1");
        assert_eq!(hello.protocol_version, 1);
        assert!(!transport.is_closed());

        // Send input; the mock records the JSON text frames.
        transport
            .send_input("ls\n", Some("write-1"))
            .expect("send input");
        transport.close();
        let inputs = server.join().expect("server done");
        assert_eq!(inputs.len(), 1);
        let frame: serde_json::Value = serde_json::from_str(&inputs[0]).expect("json frame");
        assert_eq!(frame["type"], "input");
        assert_eq!(frame["data"], "ls\n");
        assert_eq!(frame["wid"], "write-1");
    }

    #[test]
    fn wrong_pin_fails_handshake() {
        let (cert_der, key_der, _fingerprint) = generate_test_cert();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let server = mock_wss_host(listener, cert_der, key_der, hello_json());

        let wrong = "00".repeat(32);
        let err = RemoteTerminalWsTransport::connect(
            "127.0.0.1",
            &test_endpoint(port, &wrong),
            "sess-1",
            "tok",
            None,
        )
        .expect_err("pin mismatch must fail");
        assert!(
            matches!(err, WsTransportConnectError::Tls(_)),
            "expected TLS pin failure, got {err:?}"
        );
        server.join().expect("server done");
    }

    #[test]
    fn malformed_fingerprint_is_rejected_before_dial() {
        let endpoint = test_endpoint(1, "not-hex");
        let err = RemoteTerminalWsTransport::connect("127.0.0.1", &endpoint, "s", "t", None)
            .expect_err("bad fingerprint");
        assert!(matches!(err, WsTransportConnectError::BadFingerprint));
    }

    #[test]
    fn hello_timeout_when_server_is_silent() {
        let (cert_der, key_der, fingerprint) = generate_test_cert();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        // Server completes TLS+WS but never sends the hello.
        let server = thread::spawn(move || {
            let (tcp, _) = listener.accept().expect("client connects");
            let cert = CertificateDer::from(cert_der);
            let key = PrivateKeyDer::Pkcs8(key_der.into());
            let config = ServerConfig::builder_with_provider(StdArc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_protocol_versions(rustls::ALL_VERSIONS)
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap();
            let tls = ServerConnection::new(StdArc::new(config)).unwrap();
            let stream = rustls::StreamOwned::new(tls, tcp);
            let mut ws = tungstenite::accept(stream).expect("WS handshake");
            // Stay silent past the (shortened) hello deadline.
            thread::sleep(Duration::from_millis(600));
            let _ = ws.close(None);
        });

        let transport = RemoteTerminalWsTransport::connect(
            "127.0.0.1",
            &test_endpoint(port, &fingerprint),
            "s",
            "t",
            None,
        )
        .expect("connect");
        // Use a short deadline so the test stays fast.
        let err = transport
            .receive_hello_with_timeout(Duration::from_millis(300))
            .expect_err("silent server");
        assert!(
            matches!(err, RemoteTerminalWebSocketError::HelloTimeout),
            "expected HelloTimeout, got {err:?}"
        );
        transport.close();
        server.join().expect("server done");
    }

    #[test]
    fn non_hello_first_frame_is_protocol_violation() {
        let (cert_der, key_der, fingerprint) = generate_test_cert();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let server = mock_wss_host(
            listener,
            cert_der,
            key_der,
            r#"{"type":"error","message":"busy"}"#.to_string(),
        );

        let transport = RemoteTerminalWsTransport::connect(
            "127.0.0.1",
            &test_endpoint(port, &fingerprint),
            "s",
            "t",
            None,
        )
        .expect("connect");
        let err = transport.receive_hello().expect_err("not a hello");
        assert!(matches!(
            err,
            RemoteTerminalWebSocketError::ProtocolViolation
        ));
        transport.close();
        server.join().expect("server done");
    }

    #[test]
    fn discovery_update_and_clear() {
        let discovery = RemoteServerDiscovery::new();
        assert!(discovery.endpoint().is_none());

        let fp = "ab".repeat(32);
        discovery.update(Some(1234), Some(&fp));
        let ep = discovery.endpoint().expect("endpoint set");
        assert_eq!(ep.port, 1234);
        assert_eq!(ep.certificate_fingerprint, fp);

        // Same values: no change (and no panic).
        discovery.update(Some(1234), Some(&fp));
        assert!(discovery.endpoint().is_some());

        // Bad fingerprint clears to None.
        discovery.update(Some(1234), Some("nope"));
        assert!(discovery.endpoint().is_none());

        // Zero port is unusable.
        discovery.update(Some(0), Some(&fp));
        assert!(discovery.endpoint().is_none());

        discovery.update(Some(9999), Some(&fp));
        discovery.clear();
        assert!(discovery.endpoint().is_none());
        // Clearing twice is fine.
        discovery.clear();
    }

    #[test]
    fn router_prefers_ws_then_falls_back_to_http() {
        use std::sync::atomic::{AtomicUsize, Ordering as AOrd};
        let http_calls = StdArc::new(AtomicUsize::new(0));
        let http_calls2 = StdArc::clone(&http_calls);
        let router = RemoteTerminalInputRouter::new(move |text: &str, wid: &str| {
            assert!(!text.is_empty() && !wid.is_empty());
            http_calls2.fetch_add(1, AOrd::SeqCst);
            Ok(())
        });

        // No WS adopted: straight to HTTP.
        router.send("a").expect("http fallback");
        assert_eq!(http_calls.load(AOrd::SeqCst), 1);

        // Large paste bypasses WS even when one is adopted.
        let (cert_der, key_der, fingerprint) = generate_test_cert();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let server = mock_wss_host(listener, cert_der, key_der, hello_json());
        let transport = Arc::new(
            RemoteTerminalWsTransport::connect(
                "127.0.0.1",
                &test_endpoint(port, &fingerprint),
                "s",
                "t",
                None,
            )
            .expect("connect"),
        );
        router.adopt(Arc::clone(&transport));
        let big = "x".repeat(RemoteTerminalWsClientMessage::MAX_INPUT_BYTES_PER_FRAME + 1);
        router.send(&big).expect("large paste over http");
        assert_eq!(http_calls.load(AOrd::SeqCst), 2);

        // Healthy WS: input goes over the socket, HTTP untouched.
        router.send("hi").expect("ws send");
        assert_eq!(http_calls.load(AOrd::SeqCst), 2);

        // Retire with a non-active transport: must NOT clear the
        // adopted one (identity check, port of `retireWebSocket`'s
        // `===` guard). Build a second transport against a fresh mock.
        let (cert_der2, key_der2, fingerprint2) = generate_test_cert();
        let listener2 = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port2 = listener2.local_addr().unwrap().port();
        let server2 = mock_wss_host(listener2, cert_der2, key_der2, hello_json());
        let other = Arc::new(
            RemoteTerminalWsTransport::connect(
                "127.0.0.1",
                &test_endpoint(port2, &fingerprint2),
                "s",
                "t",
                None,
            )
            .expect("second connect"),
        );
        router.retire(&other);
        // The adopted transport is still active: WS send, no HTTP.
        router.send("still-ws").expect("ws still adopted");
        assert_eq!(http_calls.load(AOrd::SeqCst), 2);

        // Retire the active one: next send falls back to HTTP.
        router.retire(&transport);
        router.send("after-retire").expect("http fallback");
        assert_eq!(http_calls.load(AOrd::SeqCst), 3);

        // Clean up: dropping the Arcs drops the transports (Drop
        // marks closed and shuts the sockets down); the watchdogs see
        // the flag within 1s and exit.
        drop(transport);
        drop(other);
        drop(router);
        server.join().expect("server done");
        server2.join().expect("server2 done");
    }

    #[test]
    fn normalize_terminal_fingerprint_vectors() {
        assert_eq!(
            normalize_terminal_fingerprint(Some("AB:CD")),
            None,
            "too short"
        );
        let fp = "ab".repeat(32);
        assert_eq!(
            normalize_terminal_fingerprint(Some(&format!("sha256:{fp}"))),
            Some(fp.clone())
        );
        assert_eq!(
            normalize_terminal_fingerprint(Some(&fp.to_uppercase())),
            Some(fp)
        );
    }
}
