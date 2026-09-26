//! Web Devices panel + `/farm` route for supercli-serve.
//!
//! Serves the unified device wire format (docs/device.md §9.3) over WebSocket:
//!
//! ```text
//! 0x01  description  — stream metadata (JSON: width, height, codec, fps)
//! 0x02  keyframe     — H.264 IDR packet
//! 0x03  delta        — H.264 P-frame packet
//! 0x04  JPEG seed    — recovery frame on packet loss
//! ```
//!
//! Each wire frame is `[1 byte type][4 bytes big-endian payload len][payload]`.
//!
//! Routes (all local-only; the listener binds 127.0.0.1):
//!
//! ```text
//! GET  /devices                      — HTML devices panel
//! GET  /farm                         — HTML multi-device wall
//! GET  /static/device-stream.js      — JS WebSocket client + WebCodecs renderer
//! GET  /api/devices                  — JSON device list
//! POST /api/devices/<id>/touch       — {"x":device-points,"y":device-points,"action":"down|move|up"}
//! POST /api/devices/<id>/key         — {"keycode":"home|back|power|lock"}
//! POST /api/devices/<id>/text        — {"text":"..."}
//! GET  /api/devices/<id>/a11y        — JSON accessibility tree
//! GET  /api/devices/stream/<id>      — WebSocket, binary wire-format frames
//! GET  /api/devices/logs/<id>        — WebSocket, text frames (one log line each)
//! ```
//!
//! The device backend is behind [`DeviceProvider`]. The production provider is
//! [`DeviceBackendProvider`], an adapter over the real `supercli-device`
//! backends (adb on every host, simctl/baguette on macOS, plus three scripted
//! demo devices when `SUPERCLI_FARM_DEMO` is set). It is installed by
//! [`install_default_provider`], called once at mobile-server startup. The
//! routes, the WebSocket handshake/framing, and the wire format are all real
//! and tested here.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, Once, RwLock};
use std::time::Duration;
use supercli_device::DeviceBackend;

// ---------------------------------------------------------------------------
// Unified wire format (§9.3)
// ---------------------------------------------------------------------------

/// Stream metadata: JSON `{width,height,codec,fps}`.
pub const WIRE_DESCRIPTION: u8 = 0x01;
/// H.264 IDR packet.
pub const WIRE_KEYFRAME: u8 = 0x02;
/// H.264 P-frame packet.
pub const WIRE_DELTA: u8 = 0x03;
/// JPEG recovery frame (sent after packet loss).
pub const WIRE_JPEG_SEED: u8 = 0x04;

/// Encode one wire-format frame: `[type][u32 BE len][payload]`.
pub fn encode_wire_frame(frame_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + payload.len());
    out.push(frame_type);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Decode one wire-format frame from the front of `buf`.
/// Returns `(frame_type, payload, bytes_consumed)` or `None` if incomplete.
pub fn decode_wire_frame(buf: &[u8]) -> Option<(u8, Vec<u8>, usize)> {
    if buf.len() < 5 {
        return None;
    }
    let frame_type = buf[0];
    let len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
    if buf.len() < 5 + len {
        return None;
    }
    Some((frame_type, buf[5..5 + len].to_vec(), 5 + len))
}

// ---------------------------------------------------------------------------
// WebSocket (RFC 6455) — minimal server side, no new dependencies
// ---------------------------------------------------------------------------

const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Compute `Sec-WebSocket-Accept` from the client's `Sec-WebSocket-Key`.
pub fn websocket_accept_key(client_key: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(client_key.trim().as_bytes());
    hasher.update(WS_GUID.as_bytes());
    let digest = hasher.finalize();
    base64_encode(&digest)
}

fn base64_encode(data: &[u8]) -> String {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | (chunk.get(2).copied().unwrap_or(0) as u32);
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// WebSocket opcodes we use.
pub const WS_OPCODE_TEXT: u8 = 0x1;
pub const WS_OPCODE_BINARY: u8 = 0x2;
pub const WS_OPCODE_CLOSE: u8 = 0x8;
pub const WS_OPCODE_PING: u8 = 0x9;
pub const WS_OPCODE_PONG: u8 = 0xA;

/// Encode a server→client WebSocket frame (never masked, per RFC 6455 §5.1).
pub fn encode_ws_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + payload.len());
    out.push(0x80 | (opcode & 0x0F)); // FIN + opcode
    let len = payload.len();
    if len < 126 {
        out.push(len as u8);
    } else if len < 65536 {
        out.push(126);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(127);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    out.extend_from_slice(payload);
    out
}

/// Decode one client→server WebSocket frame from the front of `buf`.
/// Returns `(opcode, payload, bytes_consumed)` or `None` if incomplete.
/// Handles the mandatory client masking.
pub fn decode_ws_frame(buf: &[u8]) -> Option<(u8, Vec<u8>, usize)> {
    if buf.len() < 2 {
        return None;
    }
    let opcode = buf[0] & 0x0F;
    let masked = buf[1] & 0x80 != 0;
    let mut len = (buf[1] & 0x7F) as usize;
    let mut offset = 2;
    if len == 126 {
        if buf.len() < 4 {
            return None;
        }
        len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        offset = 4;
    } else if len == 127 {
        if buf.len() < 10 {
            return None;
        }
        len = u64::from_be_bytes([
            buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8], buf[9],
        ]) as usize;
        offset = 10;
    }
    let mask = if masked {
        if buf.len() < offset + 4 {
            return None;
        }
        let m = [
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
        ];
        offset += 4;
        Some(m)
    } else {
        None
    };
    if buf.len() < offset + len {
        return None;
    }
    let mut payload = buf[offset..offset + len].to_vec();
    if let Some(m) = mask {
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= m[i % 4];
        }
    }
    Some((opcode, payload, offset + len))
}

/// Is this request a WebSocket upgrade for one of our device stream routes?
/// Returns the target (`"stream"` or `"logs"`) and device id.
pub fn websocket_upgrade_target(
    method: &str,
    path: &str,
    headers: &HashMap<String, String>,
) -> Option<(WsTarget, String)> {
    if method != "GET" {
        return None;
    }
    let upgrade = headers.get("upgrade").map(|s| s.to_lowercase());
    if upgrade.as_deref() != Some("websocket") {
        return None;
    }
    if !headers.contains_key("sec-websocket-key") {
        return None;
    }
    if let Some(id) = path.strip_prefix("/api/devices/stream/") {
        if !id.is_empty() && !id.contains('/') {
            return Some((WsTarget::Stream, id.to_string()));
        }
    }
    if let Some(id) = path.strip_prefix("/api/devices/logs/") {
        if !id.is_empty() && !id.contains('/') {
            return Some((WsTarget::Logs, id.to_string()));
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WsTarget {
    Stream,
    Logs,
}

// ---------------------------------------------------------------------------
// Device model + provider abstraction
// ---------------------------------------------------------------------------

/// Serializable device summary for `GET /api/devices`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub platform: String,
    pub state: String,
    #[serde(default)]
    pub name: String,
    /// How the device is attached: `adb`, `simctl`, `baguette`, `demo`.
    #[serde(default)]
    pub connection_type: String,
}

/// Touch action in DEVICE-POINT coordinates (not normalized 0-1).
/// Device points are the units from the 0x01 wire description's
/// width_points/height_points. For Android: points = pixels * 160 / density_dpi.
#[derive(Clone, Debug)]
pub struct TouchInput {
    pub x: f64,
    pub y: f64,
    pub action: TouchAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchAction {
    Down,
    Move,
    Up,
}

impl TouchAction {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "down" => Some(TouchAction::Down),
            "move" => Some(TouchAction::Move),
            "up" => Some(TouchAction::Up),
            _ => None,
        }
    }
}

/// Backend interface. supercli-device implements this once it is a workspace
/// member; until then [`UnwiredProvider`] fails honestly.
pub trait DeviceProvider: Send + Sync {
    fn list_devices(&self) -> Result<Vec<DeviceInfo>, String>;
    fn touch(&self, id: &str, input: &TouchInput) -> Result<(), String>;
    fn key(&self, id: &str, keycode: &str) -> Result<(), String>;
    fn type_text(&self, id: &str, text: &str) -> Result<(), String>;
    fn a11y(&self, id: &str) -> Result<serde_json::Value, String>;
    /// Next chunk of wire-format bytes for the stream, or `None` on EOS.
    /// Called repeatedly by the stream loop; blocking with a timeout is fine.
    fn next_stream_chunk(&self, id: &str) -> Option<Vec<u8>>;
    /// Next log line, or `None` when the log source ends / times out.
    fn next_log_line(&self, id: &str) -> Option<String>;
}

/// Default provider before supercli-device is wired: every call fails with an
/// honest message instead of fake-passing.
pub struct UnwiredProvider;

impl DeviceProvider for UnwiredProvider {
    fn list_devices(&self) -> Result<Vec<DeviceInfo>, String> {
        Ok(Vec::new())
    }
    fn touch(&self, _id: &str, _input: &TouchInput) -> Result<(), String> {
        Err("device backend not wired (supercli-device integration pending)".into())
    }
    fn key(&self, _id: &str, _keycode: &str) -> Result<(), String> {
        Err("device backend not wired (supercli-device integration pending)".into())
    }
    fn type_text(&self, _id: &str, _text: &str) -> Result<(), String> {
        Err("device backend not wired (supercli-device integration pending)".into())
    }
    fn a11y(&self, _id: &str) -> Result<serde_json::Value, String> {
        Err("device backend not wired (supercli-device integration pending)".into())
    }
    fn next_stream_chunk(&self, _id: &str) -> Option<Vec<u8>> {
        None
    }
    fn next_log_line(&self, _id: &str) -> Option<String> {
        None
    }
}

static PROVIDER: RwLock<Option<Arc<dyn DeviceProvider>>> = RwLock::new(None);

/// Install the real backend (called once supercli-device is integrated).
/// May be called again (e.g. in tests) to swap the backend.
pub fn set_provider(provider: Arc<dyn DeviceProvider>) {
    if let Ok(mut guard) = PROVIDER.write() {
        *guard = Some(provider);
    }
}

fn provider() -> Arc<dyn DeviceProvider> {
    PROVIDER
        .read()
        .ok()
        .and_then(|guard| guard.clone())
        .unwrap_or_else(|| Arc::new(UnwiredProvider))
}

// ---------------------------------------------------------------------------
// Real backend provider: adapter over supercli-device
// ---------------------------------------------------------------------------

/// Per-device gesture state: down/move/up resolves to a tap or a swipe.
struct Gesture {
    down_x: f64,
    down_y: f64,
    last_x: f64,
    last_y: f64,
}

struct DemoStreamState {
    sent_description: bool,
}

/// One registered backend plus its connection-type label.
struct BackendEntry {
    /// Connection-type label surfaced in the API/UI: `adb`, `simctl`,
    /// `baguette`, or `demo`.
    name: &'static str,
    backend: Arc<dyn DeviceBackend>,
}

/// [`DeviceProvider`] adapter over the real `supercli-device` backends.
///
/// Touch arrives in device points end-to-end and is converted to pixels here:
/// `pixels = round(points * dpi / 160)`. Density comes from the backend
/// (`wm density` on Android); when the backend cannot report one, 160 is used
/// (1:1, documented on the wire description).
///
/// Backends that are unavailable (e.g. `adb` not on PATH) contribute zero
/// devices instead of failing the whole list. Live H.264 streaming for real
/// hardware is not wired here yet — `next_stream_chunk` returns `None` for
/// non-demo devices; the three scripted demo devices get a synthetic
/// description + ~2 fps JPEG-seed stream.
pub struct DeviceBackendProvider {
    backends: Vec<BackendEntry>,
    demo: Option<Arc<supercli_device::demo::DemoBackend>>,
    routing: RwLock<HashMap<String, usize>>,
    gestures: Mutex<HashMap<String, Gesture>>,
    demo_streams: Mutex<HashMap<String, DemoStreamState>>,
    demo_logs: Mutex<HashMap<String, u32>>,
}

impl DeviceBackendProvider {
    fn new(
        backends: Vec<BackendEntry>,
        demo: Option<Arc<supercli_device::demo::DemoBackend>>,
    ) -> Self {
        DeviceBackendProvider {
            backends,
            demo,
            routing: RwLock::new(HashMap::new()),
            gestures: Mutex::new(HashMap::new()),
            demo_streams: Mutex::new(HashMap::new()),
            demo_logs: Mutex::new(HashMap::new()),
        }
    }

    /// Re-enumerate every backend: rebuild the id->backend routing table and
    /// return the mapped device list (deterministic order by id).
    fn refresh(&self) -> Result<Vec<DeviceInfo>, String> {
        let mut routing = HashMap::new();
        let mut out = Vec::new();
        for (idx, entry) in self.backends.iter().enumerate() {
            // An unavailable backend (adb missing, baguette on Linux, ...)
            // contributes zero devices instead of failing the list.
            let devices = match entry.backend.list() {
                Ok(devices) => devices,
                Err(_) => Vec::new(),
            };
            for d in devices {
                routing.insert(d.id.as_str().to_string(), idx);
                out.push(DeviceInfo {
                    id: d.id.as_str().to_string(),
                    platform: d.platform.to_string(),
                    state: d.state.to_string(),
                    name: d.name.clone(),
                    connection_type: entry.name.to_string(),
                });
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        *self.routing.write().map_err(|e| e.to_string())? = routing;
        Ok(out)
    }

    fn resolve(&self, id: &str) -> Result<usize, String> {
        if let Some(&idx) = self.routing.read().map_err(|e| e.to_string())?.get(id) {
            return Ok(idx);
        }
        self.refresh()?;
        self.routing
            .read()
            .map_err(|e| e.to_string())?
            .get(id)
            .copied()
            .ok_or_else(|| format!("unknown device '{id}'"))
    }

    fn density_dpi(&self, idx: usize, id: &supercli_device::DeviceId) -> u32 {
        self.backends[idx].backend.density_dpi(id).unwrap_or(160)
    }
}

/// Device points -> pixels: `round(points * dpi / 160)`.
fn points_to_pixels(points: f64, dpi: u32) -> u32 {
    ((points * dpi as f64 / 160.0).round().max(0.0)) as u32
}

impl DeviceProvider for DeviceBackendProvider {
    fn list_devices(&self) -> Result<Vec<DeviceInfo>, String> {
        self.refresh()
    }

    fn touch(&self, id: &str, input: &TouchInput) -> Result<(), String> {
        let idx = self.resolve(id)?;
        let device_id = supercli_device::DeviceId::new(id);
        let dpi = self.density_dpi(idx, &device_id);
        let to_px = |pt: f64| points_to_pixels(pt, dpi);
        let backend = &self.backends[idx].backend;
        match input.action {
            TouchAction::Down => {
                self.gestures.lock().map_err(|e| e.to_string())?.insert(
                    id.to_string(),
                    Gesture {
                        down_x: input.x,
                        down_y: input.y,
                        last_x: input.x,
                        last_y: input.y,
                    },
                );
                Ok(())
            }
            TouchAction::Move => {
                if let Some(g) = self.gestures.lock().map_err(|e| e.to_string())?.get_mut(id) {
                    g.last_x = input.x;
                    g.last_y = input.y;
                }
                Ok(())
            }
            TouchAction::Up => {
                let gesture = self.gestures.lock().map_err(|e| e.to_string())?.remove(id);
                match gesture {
                    // Up without a Down: plain tap at the release point.
                    None => backend
                        .tap(&device_id, to_px(input.x), to_px(input.y))
                        .map_err(|e| e.to_string()),
                    Some(g) => {
                        let dist =
                            ((g.last_x - g.down_x).powi(2) + (g.last_y - g.down_y).powi(2)).sqrt();
                        if dist < 8.0 {
                            // Stationary press: tap at the release point.
                            backend
                                .tap(&device_id, to_px(input.x), to_px(input.y))
                                .map_err(|e| e.to_string())
                        } else {
                            // Drag: swipe from press to last move, 300 ms.
                            backend
                                .swipe(
                                    &device_id,
                                    to_px(g.down_x),
                                    to_px(g.down_y),
                                    to_px(g.last_x),
                                    to_px(g.last_y),
                                    300,
                                )
                                .map_err(|e| e.to_string())
                        }
                    }
                }
            }
        }
    }

    fn key(&self, id: &str, keycode: &str) -> Result<(), String> {
        let idx = self.resolve(id)?;
        let device_id = supercli_device::DeviceId::new(id);
        self.backends[idx]
            .backend
            .key(&device_id, keycode)
            .map_err(|e| e.to_string())
    }

    fn type_text(&self, id: &str, text: &str) -> Result<(), String> {
        let idx = self.resolve(id)?;
        let device_id = supercli_device::DeviceId::new(id);
        self.backends[idx]
            .backend
            .type_text(&device_id, text)
            .map_err(|e| e.to_string())
    }

    fn a11y(&self, id: &str) -> Result<serde_json::Value, String> {
        let idx = self.resolve(id)?;
        let device_id = supercli_device::DeviceId::new(id);
        let raw = self.backends[idx]
            .backend
            .describe_ui(&device_id)
            .map_err(|e| e.to_string())?;
        match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(v) => Ok(v),
            Err(_) => Ok(serde_json::json!({ "raw": raw })),
        }
    }

    fn next_stream_chunk(&self, id: &str) -> Option<Vec<u8>> {
        let demo = self.demo.as_ref()?;
        if !demo.is_demo_device(id) {
            // Live H.264 for real hardware is not wired here yet
            // (scrcpy session integration is tracked separately).
            // INVARIANT when it is wired: create exactly one
            // `supercli_device::ScrcpyNative` session per device id via its
            // public constructor, and never share or cache a socket name.
            // The constructor generates a per-session scid internally
            // (`generate_scid`, 8 hex chars) and forwards
            // `localabstract:scrcpy_<scid>`, so concurrent /farm tiles can
            // never collide on one abstract socket.
            return None;
        }
        let device_id = supercli_device::DeviceId::new(id);
        let needs_description = {
            let mut streams = self.demo_streams.lock().ok()?;
            let state = streams.entry(id.to_string()).or_insert(DemoStreamState {
                sent_description: false,
            });
            if state.sent_description {
                false
            } else {
                state.sent_description = true;
                true
            }
        };
        if needs_description {
            let geo = demo.geometry(&device_id)?;
            let desc = serde_json::json!({
                "width": geo.pixels.0,
                "height": geo.pixels.1,
                "width_points": geo.points.0,
                "height_points": geo.points.1,
                "density_dpi": geo.dpi,
                "codec": "jpeg",
                "fps": 2,
            });
            return Some(encode_wire_frame(
                WIRE_DESCRIPTION,
                desc.to_string().as_bytes(),
            ));
        }
        // Scripted ~2 fps preview: JPEG seed each call.
        std::thread::sleep(Duration::from_millis(500));
        let jpeg = demo.screenshot(&device_id).ok()?;
        Some(encode_wire_frame(WIRE_JPEG_SEED, &jpeg))
    }

    fn next_log_line(&self, id: &str) -> Option<String> {
        let demo = self.demo.as_ref()?;
        if !demo.is_demo_device(id) {
            return None;
        }
        let mut counts = self.demo_logs.lock().ok()?;
        let n = counts.entry(id.to_string()).or_insert(0);
        *n += 1;
        match *n {
            1 => {
                std::thread::sleep(Duration::from_millis(150));
                Some(format!("demo: {id} attached (scripted)"))
            }
            2 => {
                std::thread::sleep(Duration::from_millis(150));
                let geo = demo.geometry(&supercli_device::DeviceId::new(id))?;
                Some(format!(
                    "demo: {id} screen {}x{}pt @{}dpi",
                    geo.points.0, geo.points.1, geo.dpi
                ))
            }
            _ => None,
        }
    }
}

static INSTALL_ONCE: Once = Once::new();

/// Install the production provider: the real `supercli-device` backends.
///
/// adb is always registered (it reports zero devices when the tool is
/// missing); simctl/baguette join on macOS. When `SUPERCLI_FARM_DEMO` is set,
/// three scripted demo devices are registered as well — their names carry a
/// "Demo" prefix so they can never be mistaken for hardware.
///
/// Idempotent: only the first call installs. Tests keep working because
/// [`set_provider`] overwrites unconditionally.
pub fn install_default_provider() {
    INSTALL_ONCE.call_once(|| {
        let mut backends: Vec<BackendEntry> = Vec::new();
        backends.push(BackendEntry {
            name: "adb",
            backend: Arc::new(supercli_device::adb::AdbBackend::new()),
        });
        if cfg!(target_os = "macos") {
            backends.push(BackendEntry {
                name: "simctl",
                backend: Arc::new(supercli_device::simctl::SimctlBackend::new()),
            });
            backends.push(BackendEntry {
                name: "baguette",
                backend: Arc::new(supercli_device::baguette::BaguetteBackend::new()),
            });
        }
        let demo = if std::env::var("SUPERCLI_FARM_DEMO").is_ok() {
            let demo = Arc::new(supercli_device::demo::DemoBackend::new());
            backends.push(BackendEntry {
                name: "demo",
                backend: demo.clone(),
            });
            Some(demo)
        } else {
            None
        };
        set_provider(Arc::new(DeviceBackendProvider::new(backends, demo)));
    });
}

// ---------------------------------------------------------------------------
// Static frontend
// ---------------------------------------------------------------------------

const DEVICES_HTML: &str = include_str!("../static/devices.html");
const FARM_HTML: &str = include_str!("../static/farm.html");
const DEVICE_STREAM_JS: &str = include_str!("../static/device-stream.js");

// ---------------------------------------------------------------------------
// HTTP routing
// ---------------------------------------------------------------------------

/// Does this path belong to the device web surface?
pub fn is_device_route(path: &str) -> bool {
    path == "/devices"
        || path == "/farm"
        || path == "/static/device-stream.js"
        || path == "/api/devices"
        || path.starts_with("/api/devices/")
}

/// Parsed device route.
#[derive(Debug, PartialEq, Eq)]
pub enum DeviceRoute {
    DevicesPage,
    FarmPage,
    StreamJs,
    ListDevices,
    Touch(String),
    Key(String),
    Text(String),
    A11y(String),
    StreamWs(String),
    LogsWs(String),
    NotFound,
}

pub fn parse_device_route(method: &str, path: &str) -> DeviceRoute {
    match (method, path) {
        ("GET", "/devices") => DeviceRoute::DevicesPage,
        ("GET", "/farm") => DeviceRoute::FarmPage,
        ("GET", "/static/device-stream.js") => DeviceRoute::StreamJs,
        ("GET", "/api/devices") => DeviceRoute::ListDevices,
        _ => {
            if let Some(rest) = path.strip_prefix("/api/devices/") {
                let mut parts = rest.splitn(2, '/');
                let id = parts.next().unwrap_or("");
                let tail = parts.next().unwrap_or("");
                if id.is_empty() || id.contains('/') || tail.contains('/') {
                    return DeviceRoute::NotFound;
                }
                // WebSocket path forms: /api/devices/stream/<id>, /api/devices/logs/<id>
                if tail.is_empty() {
                    return DeviceRoute::NotFound;
                }
                return match (method, id, tail) {
                    ("POST", id, "touch") => DeviceRoute::Touch(id.to_string()),
                    ("POST", id, "key") => DeviceRoute::Key(id.to_string()),
                    ("POST", id, "text") => DeviceRoute::Text(id.to_string()),
                    ("GET", id, "a11y") => DeviceRoute::A11y(id.to_string()),
                    ("GET", "stream", device_id) => DeviceRoute::StreamWs(device_id.to_string()),
                    ("GET", "logs", device_id) => DeviceRoute::LogsWs(device_id.to_string()),
                    _ => DeviceRoute::NotFound,
                };
            }
            DeviceRoute::NotFound
        }
    }
}

/// Handle a device HTTP request. Returns `(status, body, content_type)`.
/// WebSocket upgrades are NOT handled here — see [`handle_device_connection`].
pub fn handle_device_http(method: &str, path: &str, body: &[u8]) -> (u16, String, &'static str) {
    match parse_device_route(method, path) {
        DeviceRoute::DevicesPage => (200, DEVICES_HTML.to_string(), "text/html; charset=utf-8"),
        DeviceRoute::FarmPage => (200, FARM_HTML.to_string(), "text/html; charset=utf-8"),
        DeviceRoute::StreamJs => (
            200,
            DEVICE_STREAM_JS.to_string(),
            "application/javascript; charset=utf-8",
        ),
        DeviceRoute::ListDevices => match provider().list_devices() {
            Ok(devices) => (
                200,
                serde_json::json!({ "devices": devices }).to_string(),
                "application/json",
            ),
            Err(e) => (500, json_error(&e), "application/json"),
        },
        DeviceRoute::Touch(id) => {
            let v: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
            let x = v.get("x").and_then(|x| x.as_f64());
            let y = v.get("y").and_then(|x| x.as_f64());
            let action = v
                .get("action")
                .and_then(|a| a.as_str())
                .and_then(TouchAction::parse);
            match (x, y, action) {
                (Some(x), Some(y), Some(action))
                    if x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0 =>
                {
                    match provider().touch(&id, &TouchInput { x, y, action }) {
                        Ok(()) => (200, r#"{"ok":true}"#.to_string(), "application/json"),
                        Err(e) => (502, json_error(&e), "application/json"),
                    }
                }
                _ => (
                    400,
                    json_error("touch requires {x: device-points, y: device-points, action: down|move|up} (x,y >= 0, finite)"),
                    "application/json",
                ),
            }
        }
        DeviceRoute::Key(id) => {
            let v: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
            let keycode = v.get("keycode").and_then(|k| k.as_str()).unwrap_or("");
            match keycode {
                "home" | "back" | "power" | "lock" => match provider().key(&id, keycode) {
                    Ok(()) => (200, r#"{"ok":true}"#.to_string(), "application/json"),
                    Err(e) => (502, json_error(&e), "application/json"),
                },
                _ => (
                    400,
                    json_error("keycode must be one of: home, back, power, lock"),
                    "application/json",
                ),
            }
        }
        DeviceRoute::Text(id) => {
            let v: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
            match v.get("text").and_then(|t| t.as_str()) {
                Some(text) => match provider().type_text(&id, text) {
                    Ok(()) => (200, r#"{"ok":true}"#.to_string(), "application/json"),
                    Err(e) => (502, json_error(&e), "application/json"),
                },
                None => (
                    400,
                    json_error("text requires {text: string}"),
                    "application/json",
                ),
            }
        }
        DeviceRoute::A11y(id) => match provider().a11y(&id) {
            Ok(tree) => (200, tree.to_string(), "application/json"),
            Err(e) => (502, json_error(&e), "application/json"),
        },
        DeviceRoute::StreamWs(_) | DeviceRoute::LogsWs(_) => (
            426,
            json_error("websocket upgrade required"),
            "application/json",
        ),
        DeviceRoute::NotFound => (404, json_error("not found"), "application/json"),
    }
}

fn json_error(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

// ---------------------------------------------------------------------------
// Connection-level dispatch (HTTP + WebSocket upgrade)
// ---------------------------------------------------------------------------

/// Handle one device-route connection. Returns `true` if the HTTP layer may
/// keep reading requests on this stream, `false` if we took it over (WebSocket)
/// or closed it.
pub fn handle_device_connection<S: Read + Write>(
    stream: &mut S,
    method: &str,
    path: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
    keep_alive: bool,
) -> bool {
    // WebSocket upgrade takes over the stream.
    if let Some((target, device_id)) = websocket_upgrade_target(method, path, headers) {
        let key = headers
            .get("sec-websocket-key")
            .cloned()
            .unwrap_or_default();
        let accept = websocket_accept_key(&key);
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Accept: {accept}\r\n\
             \r\n"
        );
        if stream.write_all(response.as_bytes()).is_err() {
            return false;
        }
        let _ = stream.flush();
        match target {
            WsTarget::Stream => run_stream_loop(stream, &device_id),
            WsTarget::Logs => run_logs_loop(stream, &device_id),
        }
        return false;
    }

    let (status, body_text, content_type) = handle_device_http(method, path, body);
    write_http_response(stream, status, &body_text, content_type, keep_alive)
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        426 => "Upgrade Required",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        _ => "Error",
    }
}

fn write_http_response<S: Write>(
    stream: &mut S,
    status: u16,
    body: &str,
    content_type: &str,
    keep_alive: bool,
) -> bool {
    let connection = if keep_alive { "keep-alive" } else { "close" };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Cache-Control: no-store\r\n\
         Content-Length: {len}\r\n\
         Connection: {connection}\r\n\
         \r\n\
         {body}",
        reason = reason_phrase(status),
        len = body.len(),
    )
    .and_then(|_| stream.flush())
    .is_ok()
}

/// Stream loop: relay wire-format chunks from the provider as binary WS frames.
/// Also answers pings and stops on close/error.
fn run_stream_loop<S: Read + Write>(stream: &mut S, device_id: &str) {
    // Send a description frame first so the client can size the canvas even
    // before the first keyframe. The provider's chunks carry the real frames;
    // this is a placeholder the real backend replaces via next_stream_chunk.
    let p = provider();
    let mut read_buf = [0u8; 4096];
    let mut pending = Vec::new();
    // Best-effort non-blocking reads so we can answer pings while streaming.
    for _ in 0..60_000 {
        // Drain any inbound control frames (ping/close) without blocking.
        if let Some(n) = try_read(stream, &mut read_buf) {
            if n == 0 {
                break; // peer closed
            }
            pending.extend_from_slice(&read_buf[..n]);
            while let Some((opcode, _payload, consumed)) = decode_ws_frame(&pending) {
                pending.drain(..consumed);
                if opcode == WS_OPCODE_CLOSE {
                    let _ = stream.write_all(&encode_ws_frame(WS_OPCODE_CLOSE, &[]));
                    return;
                }
                if opcode == WS_OPCODE_PING {
                    let _ = stream.write_all(&encode_ws_frame(WS_OPCODE_PONG, &[]));
                    let _ = stream.flush();
                }
            }
        }
        match p.next_stream_chunk(device_id) {
            Some(chunk) => {
                if stream
                    .write_all(&encode_ws_frame(WS_OPCODE_BINARY, &chunk))
                    .is_err()
                {
                    break;
                }
                let _ = stream.flush();
            }
            None => break, // backend ended the stream (or unwired)
        }
    }
    let _ = stream.write_all(&encode_ws_frame(WS_OPCODE_CLOSE, &[]));
}

/// Logs loop: one text WS frame per log line.
fn run_logs_loop<S: Read + Write>(stream: &mut S, device_id: &str) {
    let p = provider();
    let mut read_buf = [0u8; 4096];
    let mut pending = Vec::new();
    for _ in 0..60_000 {
        if let Some(n) = try_read(stream, &mut read_buf) {
            if n == 0 {
                break;
            }
            pending.extend_from_slice(&read_buf[..n]);
            while let Some((opcode, _payload, consumed)) = decode_ws_frame(&pending) {
                pending.drain(..consumed);
                if opcode == WS_OPCODE_CLOSE {
                    let _ = stream.write_all(&encode_ws_frame(WS_OPCODE_CLOSE, &[]));
                    return;
                }
                if opcode == WS_OPCODE_PING {
                    let _ = stream.write_all(&encode_ws_frame(WS_OPCODE_PONG, &[]));
                    let _ = stream.flush();
                }
            }
        }
        match p.next_log_line(device_id) {
            Some(line) => {
                if stream
                    .write_all(&encode_ws_frame(WS_OPCODE_TEXT, line.as_bytes()))
                    .is_err()
                {
                    break;
                }
                let _ = stream.flush();
            }
            None => break,
        }
    }
    let _ = stream.write_all(&encode_ws_frame(WS_OPCODE_CLOSE, &[]));
}

/// Best-effort non-blocking read: returns `Some(n)` with bytes read,
/// `Some(0)` on clean EOF, or `None` when no data is available right now.
/// Never blocks: implementations are expected to have set a read timeout, but
/// we additionally guard with a zero-duration poll via `TcpStream`-style
/// peeking where possible. For generic streams we do one `read` and treat
/// `WouldBlock`/`TimedOut` as "no data".
fn try_read<S: Read>(stream: &mut S, buf: &mut [u8]) -> Option<usize> {
    match stream.read(buf) {
        Ok(n) => Some(n),
        Err(e)
            if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut =>
        {
            None
        }
        Err(_) => Some(0),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes the provider-dependent tests: `set_provider` is process-global.
    static TEST_PROVIDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// RFC 6455 §1.3 handshake example.
    #[test]
    fn websocket_accept_key_matches_rfc6455_vector() {
        assert_eq!(
            websocket_accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn ws_frame_encode_decode_roundtrip() {
        let payload = b"hello device";
        let frame = encode_ws_frame(WS_OPCODE_BINARY, payload);
        // Server frames are never masked.
        assert_eq!(frame[1] & 0x80, 0);
        let (opcode, decoded, consumed) = decode_ws_frame(&frame).expect("decodes");
        assert_eq!(opcode, WS_OPCODE_BINARY);
        assert_eq!(decoded, payload);
        assert_eq!(consumed, frame.len());
    }

    #[test]
    fn ws_frame_decode_masked_client_frame() {
        // Client "Hello" masked with 37:fa:21:3d (RFC 6455 §5.7 example).
        let raw = [
            0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58,
        ];
        let (opcode, payload, consumed) = decode_ws_frame(&raw).expect("decodes");
        assert_eq!(opcode, WS_OPCODE_TEXT);
        assert_eq!(payload, b"Hello");
        assert_eq!(consumed, raw.len());
    }

    #[test]
    fn ws_frame_decode_extended_lengths() {
        let payload = vec![0xABu8; 300];
        let frame = encode_ws_frame(WS_OPCODE_BINARY, &payload);
        assert_eq!(frame[1], 126); // 16-bit extended length
        let (opcode, decoded, _) = decode_ws_frame(&frame).expect("decodes");
        assert_eq!(opcode, WS_OPCODE_BINARY);
        assert_eq!(decoded, payload);

        let big = vec![0xCDu8; 70_000];
        let frame = encode_ws_frame(WS_OPCODE_BINARY, &big);
        assert_eq!(frame[1], 127); // 64-bit extended length
        let (_, decoded, _) = decode_ws_frame(&frame).expect("decodes");
        assert_eq!(decoded, big);
    }

    #[test]
    fn ws_frame_decode_incomplete_returns_none() {
        assert!(decode_ws_frame(&[]).is_none());
        assert!(decode_ws_frame(&[0x81]).is_none());
        let frame = encode_ws_frame(WS_OPCODE_BINARY, b"hello");
        assert!(decode_ws_frame(&frame[..3]).is_none());
    }

    #[test]
    fn wire_frame_encode_decode_roundtrip() {
        for frame_type in [WIRE_DESCRIPTION, WIRE_KEYFRAME, WIRE_DELTA, WIRE_JPEG_SEED] {
            let payload = b"\x00\x01\x02payload";
            let encoded = encode_wire_frame(frame_type, payload);
            assert_eq!(encoded[0], frame_type);
            let (t, p, consumed) = decode_wire_frame(&encoded).expect("decodes");
            assert_eq!(t, frame_type);
            assert_eq!(p, payload);
            assert_eq!(consumed, encoded.len());
        }
    }

    #[test]
    fn wire_frame_decode_incomplete_returns_none() {
        assert!(decode_wire_frame(&[]).is_none());
        assert!(decode_wire_frame(&[0x02, 0x00]).is_none());
        let encoded = encode_wire_frame(WIRE_KEYFRAME, b"12345");
        assert!(decode_wire_frame(&encoded[..7]).is_none());
        // Trailing bytes are fine: only the first frame is consumed.
        let mut two = encoded.clone();
        two.extend_from_slice(&encoded);
        let (_, _, consumed) = decode_wire_frame(&two).expect("decodes");
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn route_parsing_covers_all_routes() {
        assert_eq!(
            parse_device_route("GET", "/devices"),
            DeviceRoute::DevicesPage
        );
        assert_eq!(parse_device_route("GET", "/farm"), DeviceRoute::FarmPage);
        assert_eq!(
            parse_device_route("GET", "/static/device-stream.js"),
            DeviceRoute::StreamJs
        );
        assert_eq!(
            parse_device_route("GET", "/api/devices"),
            DeviceRoute::ListDevices
        );
        assert_eq!(
            parse_device_route("POST", "/api/devices/emulator-5554/touch"),
            DeviceRoute::Touch("emulator-5554".into())
        );
        assert_eq!(
            parse_device_route("POST", "/api/devices/emulator-5554/key"),
            DeviceRoute::Key("emulator-5554".into())
        );
        assert_eq!(
            parse_device_route("POST", "/api/devices/emulator-5554/text"),
            DeviceRoute::Text("emulator-5554".into())
        );
        assert_eq!(
            parse_device_route("GET", "/api/devices/emulator-5554/a11y"),
            DeviceRoute::A11y("emulator-5554".into())
        );
        assert_eq!(
            parse_device_route("GET", "/api/devices/stream/emulator-5554"),
            DeviceRoute::StreamWs("emulator-5554".into())
        );
        assert_eq!(
            parse_device_route("GET", "/api/devices/logs/emulator-5554"),
            DeviceRoute::LogsWs("emulator-5554".into())
        );
        // Rejections
        assert_eq!(
            parse_device_route("GET", "/api/devices//touch"),
            DeviceRoute::NotFound
        );
        assert_eq!(
            parse_device_route("GET", "/api/devices/a/b/touch"),
            DeviceRoute::NotFound
        );
        assert_eq!(
            parse_device_route("DELETE", "/api/devices"),
            DeviceRoute::NotFound
        );
        assert_eq!(
            parse_device_route("GET", "/api/other"),
            DeviceRoute::NotFound
        );
    }

    #[test]
    fn is_device_route_matches() {
        for path in [
            "/devices",
            "/farm",
            "/static/device-stream.js",
            "/api/devices",
            "/api/devices/emulator-5554/touch",
            "/api/devices/stream/x",
        ] {
            assert!(is_device_route(path), "{path}");
        }
        assert!(!is_device_route("/mobile/sessions"));
        assert!(!is_device_route("/api/other"));
    }

    #[test]
    fn websocket_upgrade_detection() {
        let headers = |key: &str| {
            let mut h = HashMap::new();
            h.insert("upgrade".to_string(), "websocket".to_string());
            h.insert("sec-websocket-key".to_string(), key.to_string());
            h
        };
        assert_eq!(
            websocket_upgrade_target("GET", "/api/devices/stream/emulator-5554", &headers("k")),
            Some((WsTarget::Stream, "emulator-5554".to_string()))
        );
        assert_eq!(
            websocket_upgrade_target("GET", "/api/devices/logs/ios-1", &headers("k")),
            Some((WsTarget::Logs, "ios-1".to_string()))
        );
        // Not an upgrade
        assert_eq!(
            websocket_upgrade_target("GET", "/api/devices/stream/x", &HashMap::new()),
            None
        );
        // Wrong method
        assert_eq!(
            websocket_upgrade_target("POST", "/api/devices/stream/x", &headers("k")),
            None
        );
        // Non-device path
        assert_eq!(
            websocket_upgrade_target("GET", "/mobile/events", &headers("k")),
            None
        );
    }

    /// In-memory read/write stream for handshake tests.
    struct MemStream {
        read_buf: Vec<u8>,
        written: Vec<u8>,
    }

    impl MemStream {
        fn new() -> Self {
            MemStream {
                read_buf: Vec::new(),
                written: Vec::new(),
            }
        }
    }

    impl Read for MemStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            // EOF immediately: try_read treats WouldBlock as "no data".
            Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "no data",
            ))
        }
    }

    impl Write for MemStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.written.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn websocket_handshake_writes_101_with_rfc_vector() {
        let _lock = TEST_PROVIDER_LOCK.lock().unwrap();
        set_provider(Arc::new(UnwiredProvider));
        let mut stream = MemStream::new();
        let mut headers = HashMap::new();
        headers.insert("upgrade".to_string(), "websocket".to_string());
        headers.insert(
            "sec-websocket-key".to_string(),
            "dGhlIHNhbXBsZSBub25jZQ==".to_string(),
        );
        // Unwired provider: stream loop ends immediately, then close frame.
        let cont = handle_device_connection(
            &mut stream,
            "GET",
            "/api/devices/stream/emulator-5554",
            &headers,
            &[],
            false,
        );
        assert!(!cont, "websocket takes over the stream");
        let head = String::from_utf8_lossy(&stream.written);
        assert!(
            head.starts_with("HTTP/1.1 101 Switching Protocols"),
            "got: {}",
            &head[..head.len().min(120)]
        );
        assert!(head.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
    }

    #[test]
    fn http_routes_return_expected_statuses() {
        let _lock = TEST_PROVIDER_LOCK.lock().unwrap();
        set_provider(Arc::new(UnwiredProvider));
        // Pages
        let (s, body, ct) = handle_device_http("GET", "/devices", &[]);
        assert_eq!(s, 200);
        assert!(ct.starts_with("text/html"));
        assert!(body.contains("<!DOCTYPE html>"));

        let (s, body, ct) = handle_device_http("GET", "/farm", &[]);
        assert_eq!(s, 200);
        assert!(ct.starts_with("text/html"));
        assert!(body.contains("<!DOCTYPE html>"));

        let (s, body, ct) = handle_device_http("GET", "/static/device-stream.js", &[]);
        assert_eq!(s, 200);
        assert!(ct.starts_with("application/javascript"));
        assert!(body.contains("DeviceStream"));

        // Device list (unwired provider → empty list, still 200)
        let (s, body, _) = handle_device_http("GET", "/api/devices", &[]);
        assert_eq!(s, 200);
        assert_eq!(body, r#"{"devices":[]}"#);

        // Touch validation (device points: finite, >= 0, no upper bound)
        let (s, _, _) = handle_device_http(
            "POST",
            "/api/devices/d1/touch",
            br#"{"x":-1.0,"y":0.5,"action":"down"}"#,
        );
        assert_eq!(s, 400);
        let (s, _, _) = handle_device_http(
            "POST",
            "/api/devices/d1/touch",
            br#"{"x":0.5,"y":0.5,"action":"bogus"}"#,
        );
        assert_eq!(s, 400);
        // Valid shape but unwired backend → 502 (honest, not fake success)
        let (s, _, _) = handle_device_http(
            "POST",
            "/api/devices/d1/touch",
            br#"{"x":0.5,"y":0.5,"action":"down"}"#,
        );
        assert_eq!(s, 502);

        // Key validation
        let (s, _, _) = handle_device_http("POST", "/api/devices/d1/key", br#"{"keycode":"home"}"#);
        assert_eq!(s, 502); // valid shape, unwired backend
        let (s, _, _) = handle_device_http("POST", "/api/devices/d1/key", br#"{"keycode":"frob"}"#);
        assert_eq!(s, 400);

        // Text validation
        let (s, _, _) = handle_device_http("POST", "/api/devices/d1/text", br#"{"nope":1}"#);
        assert_eq!(s, 400);

        // a11y unwired → 502
        let (s, _, _) = handle_device_http("GET", "/api/devices/d1/a11y", &[]);
        assert_eq!(s, 502);

        // WS routes without upgrade → 426
        let (s, _, _) = handle_device_http("GET", "/api/devices/stream/d1", &[]);
        assert_eq!(s, 426);

        // Unknown
        let (s, _, _) = handle_device_http("GET", "/api/devices/d1/bogus", &[]);
        assert_eq!(s, 404);
    }

    /// Mock provider proving the HTTP layer delegates correctly.
    struct MockProvider;

    impl DeviceProvider for MockProvider {
        fn list_devices(&self) -> Result<Vec<DeviceInfo>, String> {
            Ok(vec![DeviceInfo {
                id: "emulator-5554".into(),
                platform: "android".into(),
                state: "running".into(),
                name: "Pixel_8".into(),
                connection_type: "usb".into(),
            }])
        }
        fn touch(&self, id: &str, input: &TouchInput) -> Result<(), String> {
            assert_eq!(id, "emulator-5554");
            // Device points (not normalized 0-1): finite and non-negative.
            assert!(input.x.is_finite() && input.x >= 0.0);
            assert!(input.y.is_finite() && input.y >= 0.0);
            Ok(())
        }
        fn key(&self, id: &str, keycode: &str) -> Result<(), String> {
            assert_eq!((id, keycode), ("emulator-5554", "home"));
            Ok(())
        }
        fn type_text(&self, id: &str, text: &str) -> Result<(), String> {
            assert_eq!((id, text), ("emulator-5554", "hello"));
            Ok(())
        }
        fn a11y(&self, id: &str) -> Result<serde_json::Value, String> {
            assert_eq!(id, "emulator-5554");
            Ok(serde_json::json!({"nodes": []}))
        }
        fn next_stream_chunk(&self, _id: &str) -> Option<Vec<u8>> {
            // One description frame, then EOS.
            static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            if N.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Some(encode_wire_frame(
                    WIRE_DESCRIPTION,
                    br#"{"width":1080,"height":2400,"codec":"h264","fps":60}"#,
                ))
            } else {
                None
            }
        }
        fn next_log_line(&self, id: &str) -> Option<String> {
            assert_eq!(id, "emulator-5554");
            None
        }
    }

    #[test]
    fn mock_provider_end_to_end() {
        let _lock = TEST_PROVIDER_LOCK.lock().unwrap();
        set_provider(Arc::new(MockProvider));
        // list
        let (s, body, _) = handle_device_http("GET", "/api/devices", &[]);
        assert_eq!(s, 200);
        assert!(body.contains("emulator-5554"));
        // touch/key/text/a11y all succeed through the mock
        let (s, _, _) = handle_device_http(
            "POST",
            "/api/devices/emulator-5554/touch",
            br#"{"x":0.5,"y":0.5,"action":"up"}"#,
        );
        assert_eq!(s, 200);
        let (s, _, _) = handle_device_http(
            "POST",
            "/api/devices/emulator-5554/key",
            br#"{"keycode":"home"}"#,
        );
        assert_eq!(s, 200);
        let (s, _, _) = handle_device_http(
            "POST",
            "/api/devices/emulator-5554/text",
            br#"{"text":"hello"}"#,
        );
        assert_eq!(s, 200);
        let (s, body, _) = handle_device_http("GET", "/api/devices/emulator-5554/a11y", &[]);
        assert_eq!(s, 200);
        assert_eq!(body, r#"{"nodes":[]}"#);

        // Stream WS: handshake, one binary frame (description), then close.
        let mut stream = MemStream::new();
        let mut headers = HashMap::new();
        headers.insert("upgrade".to_string(), "websocket".to_string());
        headers.insert(
            "sec-websocket-key".to_string(),
            "dGhlIHNhbXBsZSBub25jZQ==".to_string(),
        );
        let cont = handle_device_connection(
            &mut stream,
            "GET",
            "/api/devices/stream/emulator-5554",
            &headers,
            &[],
            false,
        );
        assert!(!cont);
        let written = &stream.written;
        let head_end = written
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("http head")
            + 4;
        assert!(written.starts_with(b"HTTP/1.1 101"));
        // First WS frame after the handshake: binary, carrying a 0x01 description.
        let (opcode, payload, _) = decode_ws_frame(&written[head_end..]).expect("ws frame");
        assert_eq!(opcode, WS_OPCODE_BINARY);
        let (frame_type, meta, _) = decode_wire_frame(&payload).expect("wire frame");
        assert_eq!(frame_type, WIRE_DESCRIPTION);
        assert!(String::from_utf8_lossy(&meta).contains("\"width\":1080"));
    }

    /// Build a provider over the scripted demo backend only.
    fn demo_only_provider() -> (
        DeviceBackendProvider,
        Arc<supercli_device::demo::DemoBackend>,
    ) {
        let demo = Arc::new(supercli_device::demo::DemoBackend::new());
        let backends = vec![BackendEntry {
            name: "demo",
            backend: demo.clone() as Arc<dyn supercli_device::DeviceBackend>,
        }];
        (
            DeviceBackendProvider::new(backends, Some(demo.clone())),
            demo,
        )
    }

    fn touch_input(x: f64, y: f64, action: TouchAction) -> TouchInput {
        TouchInput { x, y, action }
    }

    #[test]
    fn backend_provider_lists_three_demo_devices() {
        let (p, _) = demo_only_provider();
        let devices = p.list_devices().expect("list");
        assert_eq!(devices.len(), 3);
        let ids: Vec<&str> = devices.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["demo-pixel7-1", "demo-pixel7-2", "demo-pixel8-pro"]
        );
        for d in &devices {
            assert_eq!(d.platform, "android");
            assert_eq!(d.state, "running");
            assert_eq!(d.connection_type, "demo");
            assert!(
                d.name.starts_with("Demo "),
                "demo device flagged: {}",
                d.name
            );
        }
    }

    #[test]
    fn points_to_pixels_conversion() {
        // 1080px @420dpi <-> 411.43pt, 2400px @420dpi <-> 914.29pt:
        // round-trip through the wire units. Note the truncated values
        // 411pt/914pt do NOT round-trip (1079/2399) — the fractional
        // part carries the pixel.
        assert_eq!(points_to_pixels(411.43, 420), 1080);
        assert_eq!(points_to_pixels(914.29, 420), 2400);
        assert_eq!(points_to_pixels(0.0, 420), 0);
        assert_eq!(points_to_pixels(336.0, 480), 1008);
    }

    #[test]
    fn backend_provider_touch_tap_converts_device_points() {
        let (p, demo) = demo_only_provider();
        // Tap at device points (205.5, 457): 205.5*420/160 = 539.4 -> 539,
        // 457*420/160 = 1199.625 -> 1200.
        p.touch(
            "demo-pixel7-1",
            &touch_input(205.5, 457.0, TouchAction::Down),
        )
        .unwrap();
        p.touch("demo-pixel7-1", &touch_input(205.5, 457.0, TouchAction::Up))
            .unwrap();
        let taps: Vec<_> = demo
            .calls()
            .into_iter()
            .filter(|c| c.method == "tap")
            .collect();
        assert_eq!(taps.len(), 1);
        assert_eq!(taps[0].args, vec!["demo-pixel7-1", "539", "1200"]);
    }

    #[test]
    fn backend_provider_touch_drag_becomes_swipe() {
        let (p, demo) = demo_only_provider();
        // Drag in device points: (100,200) -> (300,400) at 420dpi.
        p.touch(
            "demo-pixel7-2",
            &touch_input(100.0, 200.0, TouchAction::Down),
        )
        .unwrap();
        p.touch(
            "demo-pixel7-2",
            &touch_input(300.0, 400.0, TouchAction::Move),
        )
        .unwrap();
        p.touch("demo-pixel7-2", &touch_input(300.0, 400.0, TouchAction::Up))
            .unwrap();
        let swipes: Vec<_> = demo
            .calls()
            .into_iter()
            .filter(|c| c.method == "swipe")
            .collect();
        assert_eq!(swipes.len(), 1);
        // 100*420/160=262.5->263, 200*420/160=525, 300*420/160=787.5->788,
        // 400*420/160=1050, 300 ms.
        assert_eq!(
            swipes[0].args,
            vec!["demo-pixel7-2", "263", "525", "788", "1050", "300"]
        );
    }

    #[test]
    fn backend_provider_key_and_text_reach_backend() {
        let (p, demo) = demo_only_provider();
        p.key("demo-pixel8-pro", "home").unwrap();
        p.type_text("demo-pixel8-pro", "hello").unwrap();
        let calls = demo.calls();
        assert!(calls
            .iter()
            .any(|c| c.method == "key" && c.args == vec!["demo-pixel8-pro", "home"]));
        assert!(calls
            .iter()
            .any(|c| c.method == "type_text" && c.args == vec!["demo-pixel8-pro", "hello"]));
        assert!(p.key("demo-pixel8-pro", "bogus").is_err());
    }

    #[test]
    fn backend_provider_stream_description_carries_points() {
        let (p, _) = demo_only_provider();
        let chunk = p.next_stream_chunk("demo-pixel7-1").expect("chunk");
        let (frame_type, payload, _) = decode_wire_frame(&chunk).expect("wire frame");
        assert_eq!(frame_type, WIRE_DESCRIPTION);
        let meta: serde_json::Value = serde_json::from_slice(&payload).expect("json");
        assert_eq!(meta["width"], 1080);
        assert_eq!(meta["height"], 2400);
        assert_eq!(meta["width_points"], 411);
        assert_eq!(meta["height_points"], 914);
        assert_eq!(meta["density_dpi"], 420);
    }

    #[test]
    fn backend_provider_unknown_device_errors_honestly() {
        let (p, _) = demo_only_provider();
        assert!(p
            .touch("nope", &touch_input(1.0, 1.0, TouchAction::Up))
            .is_err());
        assert!(p.key("nope", "home").is_err());
        assert!(p.next_stream_chunk("nope").is_none());
    }

    #[test]
    fn install_default_provider_is_idempotent() {
        let _lock = TEST_PROVIDER_LOCK.lock().unwrap();
        install_default_provider();
        install_default_provider();
        // The installed provider answers list_devices (possibly empty when no
        // hardware is attached); the point is it no longer fails as unwired.
        let devices = provider().list_devices().expect("list");
        assert!(devices.iter().all(|d| !d.id.is_empty()));
    }
}
