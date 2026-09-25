//! The Controller side of Supercli Remote: one WebSocket to the relay
//! carrying end-to-end encrypted `/mobile/*` request/response frames.
//!
//! Blocking twin of `RemoteRelayConnection.swift`:
//! - The relay authenticates the socket with the per-device relayToken
//!   (WS subprotocol header, never the URL query); content is AES-GCM
//!   sealed with the per-device e2eKey, so the relay can read none of it.
//! - Lazily connected; any failure tears the connection down and the next
//!   request reconnects with a fresh handshake.
//! - Delivery certainty is tracked so callers preserve at-most-once
//!   effects: a failed request is either proven not-sent or outcome-unknown.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use tungstenite::client::IntoClientRequest;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use crate::crypto::handshake::{constant_time_equal, transcript_mac};
use crate::crypto::RelayCryptoSession;
use crate::relay::{
    encode_tunnel_request, random_bytes, EphemeralKeyPair, RelayClientHello, RelayCredentials,
    RelayHostHello, RelayProtocolError, RelayStreamPush, RelayTunnelRequest, RelayTunnelResponse,
    RELAY_PROTOCOL_VERSION,
};

/// Whether a failed relay request is proven not to have entered the
/// encrypted channel, or may have reached the Host without a correlated
/// response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryState {
    NotSent,
    OutcomeUnknown,
}

/// Transport-level failures. Keeping delivery certainty here lets the
/// Dioxus backends preserve at-most-once effects without reimplementing
/// the connection or crypto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayError {
    /// An effect was bound to a connection generation that has since
    /// changed: fail closed, never send on a successor socket.
    GenerationChanged,
    Transport {
        delivery: DeliveryState,
        message: String,
    },
    TimedOut {
        delivery: DeliveryState,
    },
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelayError::GenerationChanged => {
                write!(
                    f,
                    "the relay connection changed before this request was sent"
                )
            }
            RelayError::Transport { message, .. } => write!(f, "{message}"),
            RelayError::TimedOut { .. } => write!(f, "the relay request timed out"),
        }
    }
}

impl std::error::Error for RelayError {}

fn transport_not_sent(message: impl Into<String>) -> RelayError {
    RelayError::Transport {
        delivery: DeliveryState::NotSent,
        message: message.into(),
    }
}

/// Parameters for [`RelayConnection::perform`].
pub struct PerformParams<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: HashMap<String, String>,
    pub auth: Option<&'a str>,
    pub content_type: Option<&'a str>,
    pub body: Option<&'a [u8]>,
    pub timeout: Duration,
}
/// Reads may open a fresh generation; effects bind to the generation whose
/// bootstrap was accepted and therefore fail before send if that
/// connection changed.
/// A response plus the exact E2E/WebSocket generation that carried it.
/// Reads may open a fresh generation; effects bind to the generation whose
/// bootstrap was accepted and therefore fail before send if that
/// connection changed.
#[derive(Debug, Clone)]
pub struct RelayTransportResponse {
    pub response: RelayTunnelResponse,
    pub connection_generation: u64,
}

/// Longest a live socket can go without inbound traffic: the keepalive
/// ping cadence (15 s) plus round-trip slack.
pub const SILENCE_LIMIT: Duration = Duration::from_secs(20);
/// Keepalive ping cadence. The relay answers WS pings at the edge, so a
/// pong proves the Controller→relay path and keeps NAT/carrier mappings
/// warm.
pub const KEEPALIVE_PING_INTERVAL: Duration = Duration::from_secs(15);
/// Hard silence deadline: with pings every 15 s, a live transport is never
/// quiet this long.
pub const KEEPALIVE_SILENCE_LIMIT: Duration = Duration::from_secs(40);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// How often the read loop polls the nonblocking socket. The `ws` mutex
/// is only ever held for a single nonblocking `read()`/`write()`, so a
/// parked read can never starve a writer (or vice versa).
const READ_POLL_INTERVAL: Duration = Duration::from_millis(20);
/// Bound for flushing one message on the nonblocking socket; the relay
/// path carries small control frames, so a full send buffer is transient.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_RETRY_POLL: Duration = Duration::from_millis(20);

/// When a relay request that missed its deadline should also retire the
/// socket it rode. Pure so the rule is testable without a relay.
pub fn should_retire_connection(sent_at_ms: u64, last_incoming_ms: u64, now_ms: u64) -> bool {
    // Inbound traffic after the send proves the transport outright.
    if last_incoming_ms >= sent_at_ms {
        return false;
    }
    now_ms.saturating_sub(last_incoming_ms) > SILENCE_LIMIT.as_millis() as u64
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

type Ws = WebSocket<MaybeTlsStream<TcpStream>>;
type PendingResult = Result<RelayTunnelResponse, RelayError>;

struct Shared {
    ws: Mutex<Ws>,
    crypto: Mutex<RelayCryptoSession>,
    pending: Mutex<HashMap<u64, mpsc::Sender<PendingResult>>>,
    push_tx: Mutex<Option<mpsc::Sender<RelayStreamPush>>>,
    /// TCP-level handle used to unblock the read loop on teardown.
    /// TLS has no read-timeout API; shutting the socket down works
    /// regardless of the framing above it.
    shutdown_sock: Mutex<Option<TcpStream>>,
    generation: AtomicU64,
    next_id: AtomicU64,
    last_incoming_ms: AtomicU64,
    closed: AtomicBool,
    /// Live `RelayConnection` handles sharing this socket. When the last
    /// handle drops, the connection tears itself down: without this, a
    /// dropped handle leaks the read/ping threads (and the socket) until
    /// the 40 s keepalive silence limit fires. Clones share the socket;
    /// only the last drop tears it down.
    handles: AtomicUsize,
}

impl Shared {
    fn note_incoming(&self) {
        self.last_incoming_ms.store(now_ms(), Ordering::SeqCst);
    }

    fn settle(&self, id: u64, result: PendingResult) {
        let tx = self.pending.lock().unwrap().remove(&id);
        if let Some(tx) = tx {
            let _ = tx.send(result);
        }
    }

    /// Close the current socket and fail every in-flight call. The same
    /// connection object may reconnect later for an unconstrained read.
    fn teardown(&self, _reason: &str) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        if !self.closed.swap(true, Ordering::SeqCst) {
            if let Some(sock) = self.shutdown_sock.lock().unwrap().take() {
                let _ = sock.shutdown(Shutdown::Both);
            }
        }
        let waiting = std::mem::take(&mut *self.pending.lock().unwrap());
        for (_, tx) in waiting {
            let _ = tx.send(Err(RelayError::Transport {
                delivery: DeliveryState::OutcomeUnknown,
                message: "relay connection torn down".to_string(),
            }));
        }
        // End the push-frame consumer so its loop exits and re-subscribes
        // (or falls back) through a fresh connection.
        *self.push_tx.lock().unwrap() = None;
    }
}

/// One relay connection. Clones share the underlying socket; `perform`
/// is safe to call from multiple threads. The socket (and its read/ping
/// threads) lives exactly as long as at least one handle is alive: the
/// last dropped handle tears the connection down, so a forgotten handle
/// can never leak threads or hold the relay socket open.
pub struct RelayConnection {
    shared: Arc<Shared>,
    credentials: RelayCredentials,
    device_id: String,
}

impl Clone for RelayConnection {
    fn clone(&self) -> Self {
        self.shared.handles.fetch_add(1, Ordering::SeqCst);
        Self {
            shared: self.shared.clone(),
            credentials: self.credentials.clone(),
            device_id: self.device_id.clone(),
        }
    }
}

impl Drop for RelayConnection {
    fn drop(&mut self) {
        // fetch_sub returns the previous count: 1 means this was the last
        // handle. teardown is idempotent (closed.swap), so racing an
        // explicit close() is harmless.
        if self.shared.handles.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.shared.teardown("last connection handle dropped");
        }
    }
}

/// Blocking TCP dial for the relay handshake thread, with a connect
/// timeout. Mirrors what `tungstenite::connect_with_config` did internally
/// (DNS + dial + nodelay), minus the redirect support this client never
/// uses — the thread + `recv_timeout` still bound the whole attempt.
fn relay_tcp_connect(
    request: &tungstenite::http::Request<()>,
    timeout: Duration,
) -> std::io::Result<TcpStream> {
    use std::net::ToSocketAddrs;

    let uri = request.uri();
    let host = uri.host().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "relay URL has no host")
    })?;
    let port = uri
        .port_u16()
        .unwrap_or(if uri.scheme_str() == Some("wss") {
            443
        } else {
            80
        });
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
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("could not resolve relay host {host}"),
        )
    }))
}

impl RelayConnection {
    /// Connect and run the forward-secret handshake: plaintext salts +
    /// ephemeral X25519 public keys both ways (worthless to the relay
    /// without the device key), then the host proves it holds the device
    /// key via a transcript MAC. After that, everything is sealed under
    /// keys bound to the ephemeral secret.
    ///
    /// TLS uses the default WebPKI roots — right for the public relay.
    pub fn connect(credentials: RelayCredentials, device_id: &str) -> Result<Self, RelayError> {
        Self::connect_with_tls(credentials, device_id, None)
    }

    /// [`RelayConnection::connect`] with an explicit TLS
    /// [`tungstenite::Connector`].
    ///
    /// `None` is identical to [`connect`]. `Some` exists for tests against
    /// mock relays with a private CA, and for private relay deployments
    /// whose certificates the public roots don't sign.
    pub fn connect_with_tls(
        credentials: RelayCredentials,
        device_id: &str,
        connector: Option<tungstenite::Connector>,
    ) -> Result<Self, RelayError> {
        let e2e_key = credentials
            .e2e_key()
            .ok_or_else(|| transport_not_sent("invalid relay e2e key"))?;
        let base = credentials.relay_url.trim_end_matches('/');
        let mut ws_url = format!("{base}/v1/client/{}", credentials.mac_id);
        if let Some(rest) = ws_url.strip_prefix("https://") {
            ws_url = format!("wss://{rest}");
        } else if let Some(rest) = ws_url.strip_prefix("http://") {
            ws_url = format!("ws://{rest}");
        }

        // The relayToken rides a WS subprotocol header, never the URL
        // query, so it can't leak into relay/proxy access logs.
        let mut request = ws_url
            .as_str()
            .into_client_request()
            .map_err(|e| transport_not_sent(format!("bad relay URL: {e}")))?;
        let protocol_value = format!(
            "supercli-relay, supercli-relay-token.{}",
            credentials.relay_token
        );
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            protocol_value
                .parse()
                .map_err(|_| transport_not_sent("bad relay token"))?,
        );

        // tungstenite's connect has no timeout of its own; bound it. The
        // TCP connect runs inside the thread so DNS + dial are covered.
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut config = WebSocketConfig::default();
            config.max_message_size = Some(crate::crypto::MAX_FRAME_BYTES + 64);
            let result = relay_tcp_connect(&request, CONNECT_TIMEOUT)
                .map_err(|e| transport_not_sent(format!("relay TCP connect failed: {e}")))
                .and_then(|stream| {
                    tungstenite::client_tls_with_config(request, stream, Some(config), connector)
                        .map_err(|e| match e {
                            tungstenite::handshake::HandshakeError::Failure(f) => {
                                transport_not_sent(format!("relay WS handshake failed: {f}"))
                            }
                            tungstenite::handshake::HandshakeError::Interrupted(_) => {
                                panic!("relay WS handshake runs on a blocking socket")
                            }
                        })
                });
            let _ = tx.send(result);
        });
        let (mut ws, _response) = rx
            .recv_timeout(CONNECT_TIMEOUT)
            .map_err(|_| transport_not_sent("relay connect timed out"))??;

        set_socket_nonblocking(&mut ws)
            .map_err(|e| transport_not_sent(format!("failed to configure relay socket: {e}")))?;
        let shutdown_sock = clone_tcp(&mut ws);

        let client_salt = {
            let mut salt = [0u8; 16];
            random_bytes(&mut salt)
                .map_err(|_| transport_not_sent("RNG failure during handshake"))?;
            salt
        };
        let ephemeral = EphemeralKeyPair::generate();
        let hello = RelayClientHello {
            v: RELAY_PROTOCOL_VERSION,
            device_id: device_id.to_string(),
            salt_b64: base64::engine::general_purpose::STANDARD.encode(client_salt),
            ephemeral_public_key_b64: base64::engine::general_purpose::STANDARD
                .encode(ephemeral.public),
        };
        let hello_json =
            serde_json::to_vec(&hello).map_err(|e| transport_not_sent(e.to_string()))?;
        ws_send(&mut ws, Message::Binary(hello_json.into()))
            .map_err(|e| transport_not_sent(format!("relay hello send failed: {e}")))?;

        let reply = read_binary_with_deadline(&mut ws, HANDSHAKE_TIMEOUT)
            .map_err(|e| transport_not_sent(format!("relay handshake failed: {e}")))?;
        let host_hello: RelayHostHello = serde_json::from_slice(&reply)
            .map_err(|_| transport_not_sent("bad relay host hello"))?;
        if host_hello.v != RELAY_PROTOCOL_VERSION {
            return Err(transport_not_sent("relay protocol version mismatch"));
        }
        let host_salt: [u8; 16] = base64::engine::general_purpose::STANDARD
            .decode(&host_hello.salt_b64)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| transport_not_sent("bad relay host salt"))?;
        let host_ephemeral = base64::engine::general_purpose::STANDARD
            .decode(&host_hello.ephemeral_public_key_b64)
            .map_err(|_| transport_not_sent("bad relay host key"))?;
        let host_mac = base64::engine::general_purpose::STANDARD
            .decode(&host_hello.mac_b64)
            .map_err(|_| transport_not_sent("bad relay host MAC"))?;

        // Verify the host's transcript MAC BEFORE deriving/using any key:
        // proves the peer holds the device key and that the relay did not
        // swap either ephemeral key or downgrade the version.
        let expected_mac = transcript_mac(
            &e2e_key,
            device_id,
            &client_salt,
            &host_salt,
            &ephemeral.public,
            &host_ephemeral,
        );
        if !constant_time_equal(&host_mac, &expected_mac) {
            let _ = ws.close(None);
            return Err(transport_not_sent("relay host authentication failed"));
        }
        let shared_secret = ephemeral
            .shared_secret(&host_ephemeral)
            .map_err(|_| transport_not_sent("relay key agreement failed"))?;
        let crypto =
            RelayCryptoSession::new(&e2e_key, &shared_secret, &client_salt, &host_salt, false)
                .map_err(|e| transport_not_sent(format!("relay session failed: {e}")))?;

        let shared = Arc::new(Shared {
            ws: Mutex::new(ws),
            crypto: Mutex::new(crypto),
            pending: Mutex::new(HashMap::new()),
            push_tx: Mutex::new(None),
            shutdown_sock: Mutex::new(shutdown_sock),
            generation: AtomicU64::new(1),
            next_id: AtomicU64::new(0),
            last_incoming_ms: AtomicU64::new(now_ms()),
            closed: AtomicBool::new(false),
            handles: AtomicUsize::new(1),
        });
        // The handshake just exchanged frames — this connection's first
        // proof of life.
        shared.note_incoming();
        spawn_read_loop(shared.clone());
        spawn_ping_loop(shared.clone());

        Ok(Self {
            shared,
            credentials,
            device_id: device_id.to_string(),
        })
    }

    /// Current connection generation. Effects bind to the generation whose
    /// bootstrap was accepted.
    pub fn connection_generation(&self) -> u64 {
        self.shared.generation.load(Ordering::SeqCst)
    }

    /// Register (replacing any previous) the consumer for pushed output
    /// frames. Frames for sessions the consumer no longer cares about are
    /// its to ignore.
    pub fn output_push_frames(&self) -> mpsc::Receiver<RelayStreamPush> {
        let (tx, rx) = mpsc::channel();
        *self.shared.push_tx.lock().unwrap() = Some(tx);
        rx
    }

    /// Perform one tunneled `/mobile/*` request. A `None` generation may
    /// open a fresh connection; a `Some` generation is fail-closed and
    /// never reconnects.
    pub fn perform(&self, params: PerformParams<'_>) -> Result<RelayTunnelResponse, RelayError> {
        let id = self.shared.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let request = RelayTunnelRequest {
            id,
            method: params.method.to_string(),
            path: params.path.to_string(),
            query: params.query,
            auth: params.auth.map(str::to_string),
            content_type: params.content_type.map(str::to_string),
            body_b64: params
                .body
                .map(|b| base64::engine::general_purpose::STANDARD.encode(b)),
        };
        self.perform_request(&request, None, params.timeout)
            .map(|r| r.response)
    }

    /// Perform an already-numbered request, optionally bound to a
    /// connection generation.
    pub fn perform_request(
        &self,
        request: &RelayTunnelRequest,
        required_generation: Option<u64>,
        timeout: Duration,
    ) -> Result<RelayTransportResponse, RelayError> {
        self.shared.next_id.fetch_max(request.id, Ordering::SeqCst);
        // Measure the complete JSON/base64 envelope before sealing. An
        // oversized local request must not burn a crypto counter or tear
        // down an otherwise healthy relay connection.
        let plaintext =
            encode_tunnel_request(request).map_err(|e| transport_not_sent(e.to_string()))?;

        if let Some(generation) = required_generation {
            if self.shared.closed.load(Ordering::SeqCst)
                || self.shared.generation.load(Ordering::SeqCst) != generation
            {
                return Err(RelayError::GenerationChanged);
            }
        } else if self.shared.closed.load(Ordering::SeqCst) {
            return Err(transport_not_sent("relay not connected"));
        }

        let sealed = {
            let mut crypto = self.shared.crypto.lock().unwrap();
            crypto.seal(&plaintext).map_err(|e| {
                self.shared.teardown("seal failed");
                transport_not_sent(format!("relay seal failed: {e}"))
            })?
        };
        let generation = self.shared.generation.load(Ordering::SeqCst);
        let sent_at_ms = now_ms();

        let (tx, rx) = mpsc::channel();
        self.shared.pending.lock().unwrap().insert(request.id, tx);
        let send_result = {
            let mut ws = self.shared.ws.lock().unwrap();
            ws_send(&mut ws, Message::Binary(sealed.into()))
        };
        if let Err(e) = send_result {
            self.shared.pending.lock().unwrap().remove(&request.id);
            self.shared.teardown("send failed");
            return Err(RelayError::Transport {
                delivery: DeliveryState::OutcomeUnknown,
                message: format!("relay send failed: {e}"),
            });
        }

        match rx.recv_timeout(timeout) {
            Ok(result) => result.map(|response| RelayTransportResponse {
                response,
                connection_generation: generation,
            }),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.shared.pending.lock().unwrap().remove(&request.id);
                // A request that missed its deadline fails on its own. The
                // connection is retired only when the socket has ALSO been
                // silent past the keepalive cadence: pongs answer every
                // 15 s on a live path, so that long a silence is a
                // black-holed socket, while a slow Host behind a live
                // socket is just a slow request.
                if generation == self.shared.generation.load(Ordering::SeqCst)
                    && should_retire_connection(
                        sent_at_ms,
                        self.shared.last_incoming_ms.load(Ordering::SeqCst),
                        now_ms(),
                    )
                {
                    self.shared
                        .teardown("request timed out on silent connection");
                }
                Err(RelayError::TimedOut {
                    delivery: DeliveryState::OutcomeUnknown,
                })
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(RelayError::Transport {
                delivery: DeliveryState::OutcomeUnknown,
                message: "relay connection torn down".to_string(),
            }),
        }
    }

    /// Close the current socket and fail every in-flight call. The same
    /// object may reconnect later via [`RelayConnection::connect`].
    pub fn close(&self) {
        self.shared.teardown("closed");
    }

    /// Reconnect with a fresh handshake after a failure. Returns a new
    /// connection object; the old one stays torn down.
    pub fn reconnect(&self) -> Result<Self, RelayError> {
        Self::connect(self.credentials.clone(), &self.device_id)
    }
}

fn spawn_read_loop(shared: Arc<Shared>) {
    thread::spawn(move || loop {
        if shared.closed.load(Ordering::SeqCst) {
            break;
        }
        // The socket is nonblocking: this `read()` returns immediately
        // and the mutex is held only for its duration, so a writer can
        // never be parked behind a read (or vice versa).
        let message = { shared.ws.lock().unwrap().read() };
        match message {
            Err(tungstenite::Error::Io(e))
                if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut =>
            {
                // Nothing to read. Sleep OFF the mutex so writers proceed.
                thread::sleep(READ_POLL_INTERVAL);
                continue;
            }
            Ok(Message::Binary(data)) => {
                shared.note_incoming();
                let opened = {
                    let mut crypto = shared.crypto.lock().unwrap();
                    crypto.open(&data)
                };
                match opened {
                    Ok(plaintext) => {
                        // Responses always carry id+status; push frames
                        // always carry stream+offset — the failed first
                        // decode falls through cleanly.
                        if let Ok(response) =
                            serde_json::from_slice::<RelayTunnelResponse>(&plaintext)
                        {
                            shared.settle(response.id, Ok(response));
                        } else if let Ok(push) =
                            serde_json::from_slice::<RelayStreamPush>(&plaintext)
                        {
                            if let Some(tx) = shared.push_tx.lock().unwrap().as_ref() {
                                let _ = tx.send(push);
                            }
                        }
                    }
                    Err(_) => {
                        // AEAD/replay failure is terminal — never skip a frame.
                        shared.teardown("crypto failure");
                        break;
                    }
                }
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {
                // tungstenite answers pings on read; either proves the path.
                shared.note_incoming();
            }
            Ok(Message::Text(_)) => {
                shared.note_incoming();
            }
            Ok(Message::Frame(_)) => {
                // Raw frames never surface from read() in practice; count
                // as proof of life all the same.
                shared.note_incoming();
            }
            Ok(Message::Close(_)) => {
                shared.teardown("peer closed");
                break;
            }
            Err(_) => {
                shared.teardown("read error");
                break;
            }
        }
    });
}

fn spawn_ping_loop(shared: Arc<Shared>) {
    thread::spawn(move || loop {
        thread::sleep(KEEPALIVE_PING_INTERVAL);
        if shared.closed.load(Ordering::SeqCst) {
            break;
        }
        if now_ms().saturating_sub(shared.last_incoming_ms.load(Ordering::SeqCst))
            > KEEPALIVE_SILENCE_LIMIT.as_millis() as u64
        {
            shared.teardown("keepalive silence limit");
            break;
        }
        let ping = {
            let mut ws = shared.ws.lock().unwrap();
            ws_send(&mut ws, Message::Ping(Vec::new().into()))
        };
        if ping.is_err() {
            shared.teardown("ping failed");
            break;
        }
    });
}

/// Write one WS message, mapping tungstenite's large error type to a
/// string at the boundary (keeps `Result` Err variants small).
///
/// The socket is nonblocking: `write()` buffers the frame inside
/// tungstenite, and `flush()` is retried on `WouldBlock` up to
/// `WRITE_TIMEOUT`. The `ws` mutex is held for the whole send, but only
/// for microseconds per attempt plus short sleeps — never a parked
/// blocking call.
fn ws_send(ws: &mut Ws, message: Message) -> Result<(), String> {
    ws.write(message)
        .map_err(|e| format!("relay write failed: {e}"))?;
    let start = Instant::now();
    loop {
        match ws.flush() {
            Ok(()) => return Ok(()),
            Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => {
                if start.elapsed() > WRITE_TIMEOUT {
                    return Err("relay write timed out (send buffer full)".to_string());
                }
                thread::sleep(WRITE_RETRY_POLL);
            }
            Err(e) => return Err(format!("relay write flush failed: {e}")),
        }
    }
}

/// Read one binary WS message, waiting at most `deadline`. Text and
/// control frames are skipped; used for the plaintext handshake reply.
fn read_binary_with_deadline(ws: &mut Ws, deadline: Duration) -> Result<Vec<u8>, String> {
    let start = Instant::now();
    loop {
        if start.elapsed() > deadline {
            return Err("timed out waiting for handshake reply".to_string());
        }
        match ws.read() {
            Ok(Message::Binary(data)) => return Ok(data.to_vec()),
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e))
                if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut =>
            {
                continue
            }
            Err(e) => return Err(format!("handshake read failed: {e}")),
        }
    }
}

/// Put the socket into nonblocking mode after the (blocking) handshake.
/// The steady-state read loop polls on `READ_POLL_INTERVAL` and never
/// holds the `ws` mutex across a blocking call, so readers and writers
/// cannot starve each other. For TLS this is set on the TCP stream
/// underneath rustls, whose `Stream` impl propagates `WouldBlock`
/// correctly.
fn set_socket_nonblocking(ws: &mut Ws) -> std::io::Result<()> {
    match ws.get_mut() {
        MaybeTlsStream::Plain(s) => s.set_nonblocking(true),
        MaybeTlsStream::Rustls(s) => s.get_mut().set_nonblocking(true),
        _ => Ok(()),
    }
}

/// Clone the raw TCP socket as a teardown handle: shutting it down
/// unblocks a thread parked in a (possibly TLS-wrapped) blocking read.
fn clone_tcp(ws: &mut Ws) -> Option<TcpStream> {
    match ws.get_mut() {
        MaybeTlsStream::Plain(s) => s.try_clone().ok(),
        MaybeTlsStream::Rustls(s) => s.get_ref().try_clone().ok(),
        _ => None,
    }
}

impl From<RelayProtocolError> for RelayError {
    fn from(e: RelayProtocolError) -> Self {
        transport_not_sent(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_policy_keeps_live_slow_requests() {
        // Inbound traffic after the send proves the transport: never retire.
        assert!(!should_retire_connection(1000, 1500, 60_000));
        // Recent silence within the limit: keep the socket.
        assert!(!should_retire_connection(1000, 900, 1000 + 19_000));
        // Silent past the limit: retire.
        assert!(should_retire_connection(1000, 900, 1000 + 21_000));
    }

    #[test]
    fn error_display_is_human_readable() {
        let e = RelayError::TimedOut {
            delivery: DeliveryState::OutcomeUnknown,
        };
        assert_eq!(e.to_string(), "the relay request timed out");
        assert_eq!(
            RelayError::GenerationChanged.to_string(),
            "the relay connection changed before this request was sent"
        );
    }
}
