//! Native scrcpy client in Rust — speaks the scrcpy-server wire protocol
//! directly, with no `scrcpy` desktop app.
//!
//! Compiled only with the `device` cargo feature. The scrcpy-server jar is
//! **not vendored**: it is downloaded once from the official GitHub release,
//! SHA-256 verified against a pinned hash, then `adb push`ed to the device
//! and started via `app_process`. This keeps the crate LITE (std only, no
//! new dependencies).
//!
//! Pinned server: scrcpy-server **v2.7** (see [`SCRCPY_SERVER_VERSION`]).
//! Protocol reference: scrcpy `doc/develop.md` at the v2.7 tag plus the
//! independent protocol write-ups that document the 2.x wire format
//! (video: 64-byte device name, codec id / width / height as u32 BE, then
//! 12-byte packet headers of u64-BE pts + u32-BE size; control: big-endian
//! messages with a leading type byte). Keyframe detection is done by
//! scanning H.264 NAL units for IDR slices — v2.x packets carry no
//! keyframe flag (the flag bits packed into the PTS high bits are a
//! scrcpy-3.x protocol change and are NOT parsed here).
//!
//! Design: `docs/device.md` §9.2 (native client), §9.3 (unified wire format).

use super::{run_tool, run_tool_ok, tool_on_path, DeviceError, DeviceId};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Pinned scrcpy-server
// ---------------------------------------------------------------------------

/// Pinned scrcpy-server version. The version argument passed to
/// `app_process ... com.genymobile.scrcpy.Server <VERSION>` must match the
/// jar exactly or the server exits immediately.
pub const SCRCPY_SERVER_VERSION: &str = "2.7";

/// Official GitHub release asset for the pinned server.
pub const SCRCPY_SERVER_JAR_URL: &str =
    "https://github.com/Genymobile/scrcpy/releases/download/v2.7/scrcpy-server-v2.7";

/// SHA-256 of the pinned jar, computed 2026-09-26 by downloading
/// [`SCRCPY_SERVER_JAR_URL`] directly from the official Genymobile/scrcpy
/// GitHub release (71,200 bytes; independently re-verified with Python
/// hashlib). The jar is Apache-2.0; its notice is in THIRD_PARTY_NOTICES.txt
/// ("MANUAL NOTICE: scrcpy-server").
pub const SCRCPY_SERVER_SHA256: &str =
    "a23c5659f36c260f105c022d27bcb3eafffa26070e7baa9eda66d01377a1adba";

/// Where the jar lives on the device.
pub const SCRCPY_SERVER_DEVICE_PATH: &str = "/data/local/tmp/scrcpy-server.jar";

/// Abstract socket name prefix the server listens on (client connects via
/// `adb forward tcp:<port> localabstract:scrcpy_<scid>`). With no scid the
/// server uses the bare name "scrcpy"; see [`generate_scid`].
pub const SCRCPY_ABSTRACT_SOCKET: &str = "scrcpy";

/// Generate a random 31-bit session id, formatted as 8 lowercase hex
/// digits (e.g. "a1b2c3d4").
///
/// The scrcpy-server v2.7 `scid` option is parsed as hex
/// (`Integer.parseInt(value, 16)`, must be a 31-bit non-negative value)
/// and names the abstract socket `scrcpy_<hex8>`
/// (`DesktopConnection.getSocketName`). Without a scid every server
/// binds the same `scrcpy` socket, so a second concurrent session (or a
/// lingering server from a previous run with `cleanup=false`) makes the
/// new server fail to bind and exit — the client then sees EOF on the
/// header read. A per-session scid also lets /farm run concurrent
/// devices/sessions against one daemon.
///
/// Entropy comes from the OS (`/dev/urandom`); the fallback mixes
/// wall-clock nanos with the pid so a missing urandom still yields
/// distinct ids across processes.
pub fn generate_scid() -> String {
    let mut bytes = [0u8; 4];
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| {
            use std::io::Read as _;
            f.read_exact(&mut bytes)
        })
        .is_ok();
    if !ok {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let mixed = nanos ^ (std::process::id().wrapping_mul(0x9E37_79B9));
        bytes = mixed.to_le_bytes();
    }
    let v = u32::from_le_bytes(bytes) & 0x7FFF_FFFF; // 31 bits
    format!("{v:08x}")
}

/// Host path of the captured stdout/stderr of the `adb shell` session
/// that runs the scrcpy server for `scid`. The server's own log lines
/// (bind failures, encoder errors, version mismatches) go here so a
/// failed session leaves its own error in the CI artifacts instead of
/// vanishing with the dead `adb shell` process.
pub fn server_log_path(scid: &str, stream: &str) -> PathBuf {
    std::env::temp_dir().join(format!("scrcpy-server-{scid}.{stream}.log"))
}

/// Local cache directory for the downloaded jar:
/// `$XDG_CACHE_HOME/supercli/` or `$HOME/.cache/supercli/`.
pub fn server_jar_cache_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                let mut p = PathBuf::from(h);
                p.push(".cache");
                p
            })
        })?;
    Some(base.join("supercli"))
}

/// Full cache path of the pinned jar.
pub fn server_jar_cache_path() -> Option<PathBuf> {
    server_jar_cache_dir().map(|d| d.join("scrcpy-server-v2.7"))
}

// ---------------------------------------------------------------------------
// Minimal SHA-256 (FIPS 180-4), so the crate stays dependency-free.
// ---------------------------------------------------------------------------

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    let (blocks, _) = msg.as_chunks::<64>();
    for chunk in blocks {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn sha256_hex(data: &[u8]) -> String {
    sha256(data).iter().map(|b| format!("{b:02x}")).collect()
}

/// Verify the jar at `path` against the pinned SHA-256. Rejects a mismatch
/// loudly — never silently accept a different binary.
pub fn verify_jar_sha256(path: &Path) -> Result<(), DeviceError> {
    let data = std::fs::read(path).map_err(DeviceError::Io)?;
    let got = sha256_hex(&data);
    if got == SCRCPY_SERVER_SHA256 {
        Ok(())
    } else {
        Err(DeviceError::Parse(format!(
            "scrcpy-server.jar SHA-256 mismatch: expected {SCRCPY_SERVER_SHA256}, got {got} ({}); refusing to push",
            path.display()
        )))
    }
}

/// Ensure the pinned jar is in the local cache, downloading it if missing.
///
/// NOTE for CLI integrators: downloading hits the network and therefore
/// belongs behind the approval flow (`supercli device setup android`
/// performs the download with approval; this function is the mechanism).
pub fn ensure_server_jar() -> Result<PathBuf, DeviceError> {
    let path = server_jar_cache_path().ok_or_else(|| {
        DeviceError::Unsupported(
            "cannot determine a cache directory (no HOME or XDG_CACHE_HOME)".to_string(),
        )
    })?;
    if path.exists() {
        verify_jar_sha256(&path)?;
        return Ok(path);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(DeviceError::Io)?;
    }
    if !tool_on_path("curl") {
        return Err(DeviceError::ToolMissing(
            "curl (needed to download scrcpy-server; or download it manually to {})".to_string(),
        ));
    }
    let path_str = path
        .to_str()
        .ok_or_else(|| DeviceError::Unsupported("jar cache path is not valid UTF-8".to_string()))?;
    // `curl -fSL`: fail on HTTP errors, follow redirects.
    let out = run_tool(
        "curl",
        &[
            "-fSL",
            "--max-time",
            "300",
            "-o",
            path_str,
            SCRCPY_SERVER_JAR_URL,
        ],
    )?;
    if out.code != Some(0) {
        return Err(DeviceError::ToolFailed {
            tool: "curl".to_string(),
            code: out.code,
            stderr: out.stderr_lossy(),
        });
    }
    verify_jar_sha256(&path)?;
    Ok(path)
}

/// Push the verified jar to the device.
pub fn deploy_server_jar(serial: &DeviceId, jar: &Path) -> Result<(), DeviceError> {
    verify_jar_sha256(jar)?;
    let jar_str = jar
        .to_str()
        .ok_or_else(|| DeviceError::Unsupported("jar path is not valid UTF-8".to_string()))?;
    let args = [
        "-s",
        serial.as_str(),
        "push",
        jar_str,
        SCRCPY_SERVER_DEVICE_PATH,
    ];
    run_tool_ok("adb", &args)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Control message encoding (client -> server, big-endian, leading type byte)
// ---------------------------------------------------------------------------

/// Control message type bytes (scrcpy 2.x).
pub mod control_type {
    pub const INJECT_KEYCODE: u8 = 0;
    pub const INJECT_TEXT: u8 = 1;
    pub const INJECT_TOUCH_EVENT: u8 = 2;
    pub const INJECT_SCROLL_EVENT: u8 = 3;
    pub const BACK_OR_SCREEN_ON: u8 = 4;
    pub const EXPAND_NOTIFICATION_PANEL: u8 = 5;
    pub const EXPAND_SETTINGS_PANEL: u8 = 6;
    pub const COLLAPSE_PANELS: u8 = 7;
    pub const GET_CLIPBOARD: u8 = 8;
    pub const SET_CLIPBOARD: u8 = 9;
    pub const SET_DISPLAY_POWER: u8 = 10;
    pub const ROTATE_DEVICE: u8 = 11;
}

/// Touch actions (Android `MotionEvent` action constants).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TouchAction {
    Down = 0,
    Up = 1,
    Move = 2,
    Cancel = 3,
    PointerDown = 5,
    PointerUp = 6,
}

/// Key event actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyAction {
    Down = 0,
    Up = 1,
}

/// Android keycodes we drive (plus a few common extras).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum AndroidKeycode {
    Home = 3,
    Back = 4,
    VolumeUp = 24,
    VolumeDown = 25,
    Power = 26,
    Menu = 82,
    AppSwitch = 187,
    Enter = 66,
    Del = 67,
    Tab = 61,
    Escape = 111,
    Space = 62,
    DpadCenter = 23,
}

fn put_u16_be(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_be_bytes());
}

fn put_i16_be(buf: &mut [u8], off: usize, v: i16) {
    buf[off..off + 2].copy_from_slice(&v.to_be_bytes());
}

fn put_u32_be(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_be_bytes());
}

fn put_u64_be(buf: &mut [u8], off: usize, v: u64) {
    buf[off..off + 8].copy_from_slice(&v.to_be_bytes());
}

/// Encode TYPE_INJECT_KEYCODE (14 bytes):
/// type(1) + action(1) + keycode(u32) + repeat(u32) + metastate(u32).
pub fn encode_inject_keycode(action: KeyAction, keycode: u32) -> [u8; 14] {
    let mut buf = [0u8; 14];
    buf[0] = control_type::INJECT_KEYCODE;
    buf[1] = action as u8;
    put_u32_be(&mut buf, 2, keycode);
    put_u32_be(&mut buf, 6, 0); // repeat
    put_u32_be(&mut buf, 10, 0); // metastate
    buf
}

/// Encode TYPE_INJECT_TEXT: type(1) + len(u32) + UTF-8 text.
pub fn encode_inject_text(text: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(5 + text.len());
    buf.push(control_type::INJECT_TEXT);
    buf.extend_from_slice(&(text.len() as u32).to_be_bytes());
    buf.extend_from_slice(text.as_bytes());
    buf
}

/// scrcpy pressure encoding: float 0.0..=1.0 scaled to u16
/// (`(u32)(0x10000 * f)` clamped to `0xffff`).
fn pressure_to_u16(pressure: f32) -> u16 {
    let p = pressure.clamp(0.0, 1.0);
    let u = (0x10000u32 as f32 * p) as u32;
    u.min(0xffff) as u16
}

/// Encode TYPE_INJECT_TOUCH_EVENT (32 bytes):
/// type(1) + action(1) + pointer_id(u64) + x(u32) + y(u32) +
/// screen_w(u16) + screen_h(u16) + pressure(u16 fixed) +
/// action_button(u32) + buttons(u32).
///
/// `screen_w`/`screen_h` are the dimensions the x/y coordinates were computed
/// against — the server maps them onto the real display, so they must be the
/// current video size (get them wrong and taps land in the wrong place after
/// a rotation).
pub fn encode_inject_touch(
    action: TouchAction,
    pointer_id: u64,
    x: u32,
    y: u32,
    screen_w: u16,
    screen_h: u16,
    pressure: f32,
) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[0] = control_type::INJECT_TOUCH_EVENT;
    buf[1] = action as u8;
    put_u64_be(&mut buf, 2, pointer_id);
    put_u32_be(&mut buf, 10, x);
    put_u32_be(&mut buf, 14, y);
    put_u16_be(&mut buf, 18, screen_w);
    put_u16_be(&mut buf, 20, screen_h);
    put_u16_be(&mut buf, 22, pressure_to_u16(pressure));
    put_u32_be(&mut buf, 24, 0); // action button
    put_u32_be(&mut buf, 28, 0); // buttons
    buf
}

/// Encode a scroll amount as the v2.7 server's i16 fixed-point
/// (`Binary.i16FixedPointToFloat`: `value == 0x7fff ? 1 : value / 2^15`).
/// `f` is in scroll ticks, clamped to [-1, 1]; 1.0 encodes as 0x7fff so
/// it decodes back to exactly 1.0.
fn scroll_to_i16(f: f32) -> i16 {
    let scaled = (f.clamp(-1.0, 1.0) * 32768.0).round() as i32;
    scaled.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// Encode TYPE_INJECT_SCROLL_EVENT (21 bytes):
/// type(1) + x(u32) + y(u32) + screen_w(u16) + screen_h(u16) +
/// hscroll(i16 fixed-point) + vscroll(i16 fixed-point) + buttons(u32).
///
/// Verified against the v2.7 server's `ControlMessageReader`:
/// scroll deltas are **i16** fixed-point here (older scrcpy used i32 —
/// sending 25 bytes would desync the server's message stream and corrupt
/// every control message after it).
pub fn encode_inject_scroll(
    x: u32,
    y: u32,
    screen_w: u16,
    screen_h: u16,
    hscroll: f32,
    vscroll: f32,
    buttons: u32,
) -> [u8; 21] {
    let mut buf = [0u8; 21];
    buf[0] = control_type::INJECT_SCROLL_EVENT;
    put_u32_be(&mut buf, 1, x);
    put_u32_be(&mut buf, 5, y);
    put_u16_be(&mut buf, 9, screen_w);
    put_u16_be(&mut buf, 11, screen_h);
    put_i16_be(&mut buf, 13, scroll_to_i16(hscroll));
    put_i16_be(&mut buf, 15, scroll_to_i16(vscroll));
    put_u32_be(&mut buf, 17, buttons);
    buf
}

/// Encode TYPE_BACK_OR_SCREEN_ON (2 bytes): type(1) + action(1).
pub fn encode_back_or_screen_on(action: KeyAction) -> [u8; 2] {
    [control_type::BACK_OR_SCREEN_ON, action as u8]
}

/// Encode TYPE_GET_CLIPBOARD (2 bytes): type(1) + copy(1).
pub fn encode_get_clipboard(copy: bool) -> [u8; 2] {
    [control_type::GET_CLIPBOARD, u8::from(copy)]
}

/// Encode TYPE_SET_CLIPBOARD: type(1) + sequence(u64) + paste(1) +
/// len(u32) + UTF-8 text.
pub fn encode_set_clipboard(sequence: u64, paste: bool, text: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(14 + text.len());
    buf.push(control_type::SET_CLIPBOARD);
    buf.extend_from_slice(&sequence.to_be_bytes());
    buf.push(u8::from(paste));
    buf.extend_from_slice(&(text.len() as u32).to_be_bytes());
    buf.extend_from_slice(text.as_bytes());
    buf
}

/// Encode TYPE_SET_DISPLAY_POWER (2 bytes): type(1) + on(1).
pub fn encode_set_display_power(on: bool) -> [u8; 2] {
    [control_type::SET_DISPLAY_POWER, u8::from(on)]
}

/// Encode TYPE_ROTATE_DEVICE (1 byte).
pub fn encode_rotate_device() -> [u8; 1] {
    [control_type::ROTATE_DEVICE]
}

/// Encode TYPE_EXPAND_NOTIFICATION_PANEL (1 byte).
pub fn encode_expand_notification_panel() -> [u8; 1] {
    [control_type::EXPAND_NOTIFICATION_PANEL]
}

/// Encode TYPE_COLLAPSE_PANELS (1 byte).
pub fn encode_collapse_panels() -> [u8; 1] {
    [control_type::COLLAPSE_PANELS]
}

// ---------------------------------------------------------------------------
// Video stream (server -> client)
// ---------------------------------------------------------------------------

/// Parsed video stream header (76 bytes on the wire: 64-byte device name +
/// codec id + width + height, all big-endian).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoHeader {
    pub device_name: String,
    pub codec: [u8; 4],
    pub width: u32,
    pub height: u32,
}

/// One H.264 packet from the video socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct H264Packet {
    /// Presentation timestamp in microseconds (scrcpy 2.x: plain u64 BE;
    /// the flag bits packed into the PTS high bits are a 3.x change and are
    /// not interpreted here).
    pub pts: u64,
    /// Raw H.264 Annex-B payload.
    pub data: Vec<u8>,
    /// True when the payload contains an IDR slice (NAL unit type 5).
    pub is_keyframe: bool,
}

/// Maximum sane packet size (8 MiB); anything larger is a corrupt stream.
const MAX_PACKET_SIZE: u32 = 8 * 1024 * 1024;

/// Scan H.264 Annex-B NAL units; true if any NAL unit is an IDR slice
/// (nal_unit_type == 5).
pub fn payload_is_keyframe(data: &[u8]) -> bool {
    let mut i = 0;
    while i + 4 < data.len() {
        // Find the next start code (0x000001 or 0x00000001).
        let start = if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            i + 3
        } else if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 0 && data[i + 3] == 1 {
            i + 4
        } else {
            i += 1;
            continue;
        };
        if start < data.len() && data[start] & 0x1f == 5 {
            return true;
        }
        i = start + 1;
    }
    false
}

/// Stateful reader for the scrcpy video socket. Generic over `Read` so tests
/// can feed synthetic bytes without a device.
pub struct VideoStreamReader<R> {
    inner: R,
}

impl<R: Read> VideoStreamReader<R> {
    pub fn new(inner: R) -> Self {
        VideoStreamReader { inner }
    }

    pub fn into_inner(self) -> R {
        self.inner
    }

    fn read_exact_vec(&mut self, n: usize) -> std::io::Result<Vec<u8>> {
        let mut buf = vec![0u8; n];
        self.inner.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Read the 76-byte stream header: 64-byte null-padded device name,
    /// 4-byte codec id (e.g. `b"h264"`), u32-BE width, u32-BE height.
    pub fn read_header(&mut self) -> std::io::Result<VideoHeader> {
        let raw = self.read_exact_vec(76)?;
        let name_end = raw[..64].iter().position(|&b| b == 0).unwrap_or(64);
        let device_name = String::from_utf8_lossy(&raw[..name_end]).into_owned();
        let codec = [raw[64], raw[65], raw[66], raw[67]];
        let width = u32::from_be_bytes([raw[68], raw[69], raw[70], raw[71]]);
        let height = u32::from_be_bytes([raw[72], raw[73], raw[74], raw[75]]);
        Ok(VideoHeader {
            device_name,
            codec,
            width,
            height,
        })
    }

    /// Read the next packet. Returns `Ok(None)` on clean EOF (server hung
    /// up); `Err` on a truncated header/payload.
    pub fn next_packet(&mut self) -> std::io::Result<Option<H264Packet>> {
        let mut header = [0u8; 12];
        match self.inner.read_exact(&mut header) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Distinguish clean EOF (0 bytes) from a torn header.
                return Ok(None);
            }
            Err(e) => return Err(e),
        }
        let pts = u64::from_be_bytes(header[0..8].try_into().unwrap());
        let size = u32::from_be_bytes(header[8..12].try_into().unwrap());
        if size > MAX_PACKET_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("scrcpy packet size {size} exceeds {MAX_PACKET_SIZE}"),
            ));
        }
        let data = self.read_exact_vec(size as usize)?;
        let is_keyframe = payload_is_keyframe(&data);
        Ok(Some(H264Packet {
            pts,
            data,
            is_keyframe,
        }))
    }
}

// ---------------------------------------------------------------------------
// Unified wire format (docs/device.md §9.3): baguette framing for both
// platforms. The web Devices panel and the gpuidart P0-6 surface decode one
// format; the platform is selected only by device id.
// ---------------------------------------------------------------------------

/// Frame types (verbatim from §9.3).
pub const WIRE_DESCRIPTION: u8 = 0x01;
pub const WIRE_KEYFRAME: u8 = 0x02;
pub const WIRE_DELTA: u8 = 0x03;
pub const WIRE_JPEG_SEED: u8 = 0x04;

/// Frame layout: `type (1 byte) | payload length (u32 BE) | payload`.
/// The length prefix makes the stream self-delimiting over a WebSocket.
pub fn wire_frame(frame_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + payload.len());
    out.push(frame_type);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Parse one frame from the front of `buf`.
/// Returns `(frame_type, payload, bytes_consumed)`, or `None` when `buf`
/// does not yet hold a complete frame.
pub fn parse_wire_frame(buf: &[u8]) -> Option<(u8, &[u8], usize)> {
    if buf.len() < 5 {
        return None;
    }
    let len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
    if buf.len() < 5 + len {
        return None;
    }
    Some((buf[0], &buf[5..5 + len], 5 + len))
}

/// 0x01 description frame: JSON stream metadata.
pub fn description_frame(codec: &str, width: u32, height: u32) -> Vec<u8> {
    let payload = format!(r#"{{"codec":"{codec}","width":{width},"height":{height}}}"#);
    wire_frame(WIRE_DESCRIPTION, payload.as_bytes())
}

/// 0x04 JPEG seed frame (recovery on packet loss). The JPEG bytes come from
/// the caller — e.g. an `adb exec-out screencap` JPEG or a decoded frame;
/// H.264 decoding itself needs an external decoder (WebCodecs in the
/// browser, platform decoder in gpuidart).
pub fn jpeg_seed_frame(jpeg: &[u8]) -> Vec<u8> {
    wire_frame(WIRE_JPEG_SEED, jpeg)
}

impl H264Packet {
    /// Wrap this packet in the unified wire format: 0x02 for keyframes,
    /// 0x03 for deltas. The payload is the raw H.264 Annex-B packet.
    pub fn to_wire_format(&self) -> Vec<u8> {
        wire_frame(
            if self.is_keyframe {
                WIRE_KEYFRAME
            } else {
                WIRE_DELTA
            },
            &self.data,
        )
    }
}

// ---------------------------------------------------------------------------
// Control channel (client -> server), generic over the sink for tests.
// ---------------------------------------------------------------------------

/// Write side of the scrcpy control socket. `screen_w`/`screen_h` are the
/// current video dimensions the touch coordinates are computed against.
pub struct ControlChannel<W> {
    inner: W,
    screen_w: u16,
    screen_h: u16,
    clipboard_sequence: u64,
}

impl<W: Write> ControlChannel<W> {
    pub fn new(inner: W, screen_w: u16, screen_h: u16) -> Self {
        ControlChannel {
            inner,
            screen_w,
            screen_h,
            clipboard_sequence: 0,
        }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }

    /// Update the screen dimensions (e.g. after a rotation changes the
    /// video size). Values are clamped to u16::MAX for the wire format.
    pub fn set_screen_size(&mut self, w: u32, h: u32) {
        self.screen_w = w.min(u16::MAX as u32) as u16;
        self.screen_h = h.min(u16::MAX as u32) as u16;
    }

    fn send(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.inner.write_all(bytes)?;
        self.inner.flush()
    }

    /// Multi-touch capable touch injection. Use distinct `pointer_id`s for
    /// concurrent pointers (pinch = two pointers with Down/Move/Up).
    pub fn inject_touch(
        &mut self,
        action: TouchAction,
        pointer_id: u64,
        x: u32,
        y: u32,
    ) -> std::io::Result<()> {
        let msg = encode_inject_touch(action, pointer_id, x, y, self.screen_w, self.screen_h, 1.0);
        self.send(&msg)
    }

    /// Inject a scroll event (mouse-wheel style): `hscroll`/`vscroll` are
    /// in scroll ticks, each clamped to [-1, 1] by the wire encoding.
    /// Positive vscroll scrolls up, negative scrolls down.
    pub fn inject_scroll(
        &mut self,
        x: u32,
        y: u32,
        hscroll: f32,
        vscroll: f32,
    ) -> std::io::Result<()> {
        let msg = encode_inject_scroll(x, y, self.screen_w, self.screen_h, hscroll, vscroll, 0);
        self.send(&msg)
    }

    /// Press and release a key (DOWN then UP).
    pub fn press_key(&mut self, keycode: AndroidKeycode) -> std::io::Result<()> {
        self.send(&encode_inject_keycode(KeyAction::Down, keycode as u32))?;
        self.send(&encode_inject_keycode(KeyAction::Up, keycode as u32))
    }

    pub fn inject_text(&mut self, text: &str) -> std::io::Result<()> {
        self.send(&encode_inject_text(text))
    }

    /// Set the device clipboard (does not paste; agents paste explicitly).
    pub fn set_clipboard(&mut self, text: &str) -> std::io::Result<()> {
        self.clipboard_sequence += 1;
        self.send(&encode_set_clipboard(self.clipboard_sequence, false, text))
    }

    pub fn rotate_device(&mut self) -> std::io::Result<()> {
        self.send(&encode_rotate_device())
    }

    pub fn back_or_screen_on(&mut self, action: KeyAction) -> std::io::Result<()> {
        self.send(&encode_back_or_screen_on(action))
    }

    pub fn set_display_power(&mut self, on: bool) -> std::io::Result<()> {
        self.send(&encode_set_display_power(on))
    }
}

/// Swipe gesture on a control channel: Down → interpolated Moves → Up on
/// pointer 0. `duration` is spread evenly across the move steps; short
/// durations (≈150–200 ms) read as flings on the device.
///
/// Free function (generic over the sink) so callers holding a split
/// borrow of [`ScrcpyNative`] can drive the control channel while reading
/// the video socket; [`ScrcpyNative::swipe`] is the sequential convenience
/// wrapper.
pub fn swipe_control<W: Write>(
    control: &mut ControlChannel<W>,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    duration: Duration,
) -> std::io::Result<()> {
    const STEPS: u32 = 12;
    control.inject_touch(TouchAction::Down, 0, x0, y0)?;
    for s in 1..=STEPS {
        let t = s as f32 / STEPS as f32;
        let x = (x0 as f32 + (x1 as f32 - x0 as f32) * t) as u32;
        let y = (y0 as f32 + (y1 as f32 - y0 as f32) * t) as u32;
        control.inject_touch(TouchAction::Move, 0, x, y)?;
        std::thread::sleep(duration / STEPS);
    }
    control.inject_touch(TouchAction::Up, 0, x1, y1)
}

// ---------------------------------------------------------------------------
// Last-resort fallback: adb shell input
// ---------------------------------------------------------------------------

/// Exact warning logged when the native control channel is unavailable.
/// Kept as a constant so the wording is stable and testable.
pub const ADB_FALLBACK_WARNING: &str =
    "scrcpy control unavailable, falling back to adb shell input (200-500ms latency, no multi-touch)";

/// Last-resort input path: `adb -s <serial> shell input <args>`.
///
/// Only call this when the native scrcpy control channel cannot be
/// established. Logs [`ADB_FALLBACK_WARNING`] to stderr so the degraded
/// path is visible (no multi-touch, no pinch, 200–500 ms per call).
pub fn fallback_adb_input(serial: &DeviceId, input_args: &[&str]) -> Result<(), DeviceError> {
    eprintln!("supercli-device: {ADB_FALLBACK_WARNING}");
    if !tool_on_path("adb") {
        return Err(DeviceError::ToolMissing("adb".to_string()));
    }
    let mut full: Vec<&str> = Vec::with_capacity(input_args.len() + 4);
    full.extend_from_slice(&["-s", serial.as_str(), "shell", "input"]);
    full.extend_from_slice(input_args);
    run_tool_ok("adb", &full)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// High-level client
// ---------------------------------------------------------------------------

/// How long to wait for each TCP connect to the forwarded scrcpy sockets.
const SOCKET_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

fn adb_forward(serial: &DeviceId, local_port: u16, scid: &str) -> Result<(), DeviceError> {
    let spec = format!("tcp:{local_port}");
    run_tool_ok(
        "adb",
        &[
            "-s",
            serial.as_str(),
            "forward",
            &spec,
            &format!("localabstract:{SCRCPY_ABSTRACT_SOCKET}_{scid}"),
        ],
    )?;
    Ok(())
}

fn adb_forward_remove(local_port: u16) -> Result<(), DeviceError> {
    let spec = format!("tcp:{local_port}");
    run_tool_ok("adb", &["forward", "--remove", &spec])?;
    Ok(())
}

/// Pick a free loopback TCP port (bind :0, read the port, release it).
/// There is an inherent TOCTOU race; the connect that follows retries, so a
/// stolen port surfaces as a connection error, not silent misrouting.
fn pick_free_port() -> Result<u16, DeviceError> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(DeviceError::Io)?;
    listener
        .local_addr()
        .map(|a| a.port())
        .map_err(DeviceError::Io)
}

/// Wait for any previous scrcpy server (app_process) to be fully gone.
///
/// Amein's fix (run #23): the 720 CI run starts right after the full-res
/// session tears down ("Terminated" in its stderr). If the old app_process
/// is still dying, the new server crashes with NPE in
/// DisplayManager.getDisplayInfo (DisplayManager.java:89) from
/// Device.<init>. Poll `pidof app_process` until empty, then settle 2s.
fn wait_for_old_server_gone(serial: &DeviceId) -> Result<(), DeviceError> {
    for _ in 0..20 {
        let out = std::process::Command::new("adb")
            .args(["-s", serial.as_str(), "shell", "pidof", "app_process"])
            .output()
            .map_err(DeviceError::Io)?;
        let pids = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if pids.is_empty() {
            eprintln!("scrcpy: no old app_process running; settling 2s");
            std::thread::sleep(Duration::from_secs(2));
            return Ok(());
        }
        eprintln!("scrcpy: waiting for old app_process to exit (pids: {pids})");
        std::thread::sleep(Duration::from_millis(500));
    }
    eprintln!("scrcpy: warning: old app_process still present after 10s; proceeding anyway");
    Ok(())
}

/// Spawn the scrcpy server on the device, detached (it runs until the video
/// socket closes or the device reboots; `cleanup` defaults to true so the
/// jar is removed from /data/local/tmp on exit).
fn spawn_server(serial: &DeviceId, scid: &str) -> Result<(), DeviceError> {
    // v2.x server args are `key=value` pairs; the version argument must
    // match the jar exactly ("2.7", not "2.7.0" or "v2.7").
    //
    // Amein's proven command (2026-09-26): tunnel_forward + cleanup=false
    // are required. tunnel_forward=true makes the server listen on the
    // abstract socket (client does adb forward and connects); without it
    // the server expects adb reverse and never listens.
    //
    // Note: video_encoder is NOT forced here. Amein suggested
    // c2.android.avc.encoder for emulators, but if that specific encoder
    // is absent the server hangs during MediaCodec init (45s timeout).
    // Let scrcpy pick the default encoder.
    //
    // SCRCPY_MAX_SIZE (env): when set, appends `max_size=N` to cap the
    // video resolution (e.g. 720 for a second CI run at lower res). Unset
    // means full device resolution.
    //
    // scid (per-session, hex): the v2.7 server parses it as hex and binds
    // the abstract socket `scrcpy_<hex8>` (DesktopConnection.getSocketName),
    // so concurrent sessions never collide on the bare `scrcpy` name.
    // Amein's fix (run #23): wait for the previous session's server to be
    // fully gone before starting a new one, otherwise the new server can
    // crash with NPE in DisplayManager.getDisplayInfo.
    wait_for_old_server_gone(serial)?;

    let max_size_arg = std::env::var("SCRCPY_MAX_SIZE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(|v| format!(" max_size={}", v.trim()))
        .unwrap_or_default();
    let server_cmd = format!(
        "CLASSPATH={} app_process / com.genymobile.scrcpy.Server {} \
         tunnel_forward=true audio=false control=true cleanup=false scid={scid} \
         video_codec=h264 max_fps=60 display_id=0{max_size_arg}",
        SCRCPY_SERVER_DEVICE_PATH, SCRCPY_SERVER_VERSION
    );
    eprintln!("scrcpy: server_cmd={server_cmd}");
    // Capture the server's own stdout/stderr to files: if the server dies
    // (bind failure, bad args, encoder error) its log lines survive in the
    // CI artifacts instead of vanishing with the adb shell session.
    // Amein's fix (run #23): retry the server start up to 3 times with
    // backoff. The 720 run can hit a transient NPE in the server's
    // Device.<init> if the emulator is still settling; a retry usually
    // succeeds.
    let mut last_err: Option<DeviceError> = None;
    for attempt in 1..=3 {
        if attempt > 1 {
            let backoff = Duration::from_secs(2 * attempt as u64);
            eprintln!("scrcpy: retrying server start in {backoff:?} (attempt {attempt}/3)");
            std::thread::sleep(backoff);
            // The failed attempt may have left a dying app_process behind;
            // wait for it to clear before retrying.
            let _ = wait_for_old_server_gone(serial);
        }
        let stdout_log = server_log_path(scid, "stdout");
        let stderr_log = server_log_path(scid, "stderr");
        let stdout_file = std::fs::File::create(&stdout_log).map_err(DeviceError::Io)?;
        let stderr_file = std::fs::File::create(&stderr_log).map_err(DeviceError::Io)?;
        eprintln!(
            "scrcpy: server logs: {} {}",
            stdout_log.display(),
            stderr_log.display()
        );
        let mut child = std::process::Command::new("adb")
            .args(["-s", serial.as_str(), "shell", &server_cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(stdout_file))
            .stderr(std::process::Stdio::from(stderr_file))
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    DeviceError::ToolMissing("adb".to_string())
                } else {
                    DeviceError::Io(e)
                }
            })?;
        // Give the server a moment to start, then verify it's actually running.
        // If app_process exited immediately (wrong args, jar not found, version
        // mismatch), the adb client exits too — catch that here with a clear
        // error instead of a mysterious 45s socket timeout.
        std::thread::sleep(Duration::from_secs(3));
        match child.try_wait() {
            Ok(Some(status)) => {
                // The adb client exited => the server command failed fast.
                // The server's own output is in the log files; surface it.
                let stdout = std::fs::read_to_string(&stdout_log).unwrap_or_default();
                let stderr = std::fs::read_to_string(&stderr_log).unwrap_or_default();
                eprintln!(
                    "scrcpy: stage=server_died_immediately status={status} attempt={attempt}/3"
                );
                eprintln!("scrcpy: server stdout: {stdout}");
                eprintln!("scrcpy: server stderr: {stderr}");
                last_err = Some(DeviceError::Parse(format!(
                    "scrcpy server died immediately (status {status}); stdout: {stdout}; stderr: {stderr}"
                )));
                // Fall through to the retry loop.
            }
            Ok(None) => {
                // Still running — the server is up (or at least the adb shell
                // session is alive). The socket poll will confirm.
                eprintln!("scrcpy: stage=server_process_alive");
                // Intentionally leak the child handle: the adb shell session
                // must stay alive for the server's lifetime. Dropping the
                // Child without waiting detaches it (the OS reaps it on exit).
                std::mem::forget(child);
                return Ok(());
            }
            Err(e) => {
                eprintln!("scrcpy: warning: try_wait failed: {e}");
                std::mem::forget(child);
                return Ok(());
            }
        }
    }
    Err(last_err.unwrap_or(DeviceError::Parse(
        "scrcpy server failed to start after 3 attempts".to_string(),
    )))
}

fn connect_socket(port: u16) -> Result<TcpStream, DeviceError> {
    let addr: SocketAddr = format!("127.0.0.1:{port}")
        .parse()
        .map_err(|_| DeviceError::Parse(format!("bad loopback port: {port}")))?;
    // Simple connect with timeout. NOTE: with `adb forward` this succeeds
    // as soon as the forward accepts locally — even if the server isn't
    // listening on the device yet. Do NOT treat a successful connect as
    // proof the server is up; use the dummy-byte probe below for that.
    eprintln!("scrcpy: stage=socket_connect_wait port={port}");
    match TcpStream::connect_timeout(&addr, SOCKET_CONNECT_TIMEOUT) {
        Ok(s) => {
            eprintln!("scrcpy: stage=socket_connected port={port}");
            Ok(s)
        }
        Err(e) => Err(DeviceError::Io(e)),
    }
}

/// Connect the video (first) socket.
///
/// NOTE: with `adb forward` the TCP connect succeeds as soon as the
/// forward accepts locally — even if the server isn't listening on the
/// device yet. A successful connect is NOT proof the server is up; the
/// dummy-byte read (after the control socket is connected) is the real
/// liveness check.
fn connect_video_socket(port: u16) -> Result<TcpStream, DeviceError> {
    connect_socket(port)
}

/// Perform the v2.x forward-tunnel socket handshake against an already-
/// forwarded `port`.
///
/// Returns `(video_stream, control_stream, header)`.
///
/// Ordering (critical — any other order deadlocks):
/// 1. Connect socket #1 (video).
/// 2. Connect socket #2 (control) IMMEDIATELY. The server accepts ALL
///    sockets first and only then writes anything; waiting for the video
///    header before opening control deadlocks.
/// 3. Read the 1-byte dummy on the video socket. If EOF/reset here, the
///    server isn't listening yet (adb forward accepts locally before the
///    device-side listener exists): close both sockets and retry the whole
///    handshake every 100ms for up to 10s.
/// 4. Read the 76-byte header on the video socket (64-byte NUL-padded
///    device name + u32-BE codec id + u32-BE width + u32-BE height).
///
/// `pub(crate)` so the unit test can drive it against a scripted fake
/// server without adb or a device.
pub(crate) fn handshake_video_control(
    port: u16,
) -> Result<(TcpStream, TcpStream, VideoHeader), DeviceError> {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut attempts = 0u32;
    loop {
        attempts += 1;
        // 1. Video socket.
        let mut video = match connect_video_socket(port) {
            Ok(s) => s,
            Err(e) => {
                if std::time::Instant::now() >= deadline {
                    return Err(e);
                }
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        // 2. Control socket IMMEDIATELY.
        let control_stream = match connect_socket(port) {
            Ok(s) => s,
            Err(e) => {
                drop(video);
                if std::time::Instant::now() >= deadline {
                    return Err(e);
                }
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        eprintln!("scrcpy: stage=control_connected attempts={attempts}");

        // 3. Dummy byte: the server writes it once both sockets are
        //    accepted. EOF/reset here means the server isn't listening yet.
        video
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(DeviceError::Io)?;
        let mut dummy = [0u8; 1];
        match video.read_exact(&mut dummy) {
            Ok(()) => {
                eprintln!(
                    "scrcpy: stage=video_dummy_ok attempts={attempts} byte=0x{:02x}",
                    dummy[0]
                );
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::UnexpectedEof
                    || e.kind() == std::io::ErrorKind::ConnectionReset =>
            {
                eprintln!(
                    "scrcpy: stage=video_dummy_eof attempts={attempts} (server not listening yet)"
                );
                drop(video);
                drop(control_stream);
                if std::time::Instant::now() >= deadline {
                    return Err(DeviceError::Io(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "scrcpy video dummy byte not received within 10s; server never listened",
                    )));
                }
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => {
                drop(video);
                drop(control_stream);
                return Err(DeviceError::Io(e));
            }
        }

        // 4. Header: 64-byte device name + codec meta.
        video
            .set_read_timeout(Some(SOCKET_CONNECT_TIMEOUT))
            .map_err(DeviceError::Io)?;
        let mut reader = VideoStreamReader::new(video);
        let header = match reader.read_header() {
            Ok(h) => h,
            Err(e) => {
                return Err(DeviceError::Parse(format!(
                    "scrcpy video header unreadable: {e}"
                )))
            }
        };
        if header.codec != *b"h264" {
            return Err(DeviceError::Parse(format!(
                "unexpected scrcpy video codec: {:?} (only h264 is supported)",
                header.codec
            )));
        }
        eprintln!(
            "scrcpy: stage=header_ok codec=h264 size={}x{} device='{}'",
            header.width, header.height, header.device_name
        );
        // Get the video stream back from the reader.
        let video = reader.into_inner();
        return Ok((video, control_stream, header));
    }
}

/// Native scrcpy client: video + control sockets to a scrcpy-server running
/// on the device. Obtain via [`ScrcpyNative::connect`]; dropping the client
/// removes the `adb forward` (closing the video socket stops the server).
pub struct ScrcpyNative {
    serial: DeviceId,
    local_port: u16,
    scid: String,
    video: TcpStream,
    control: ControlChannel<TcpStream>,
    device_name: String,
    video_width: u32,
    video_height: u32,
}

impl ScrcpyNative {
    /// Connect to the device: verify/deploy the server jar, forward the
    /// port, start the server, then perform the v2.x forward-tunnel
    /// handshake:
    ///
    /// 1. Connect socket #1 (video); read the 1-byte dummy (with retry:
    ///    adb forward accepts locally before the server listens).
    /// 2. Connect socket #2 (control) IMMEDIATELY. The server accepts ALL
    ///    sockets first and only then writes anything; waiting for the
    ///    video header before opening control deadlocks.
    /// 3. Read the 76-byte header on the video socket (64-byte device name
    ///    + u32 codec + u32 width + u32 height, big-endian).
    pub fn connect(device_serial: &str) -> Result<Self, DeviceError> {
        if !tool_on_path("adb") {
            return Err(DeviceError::ToolMissing("adb".to_string()));
        }
        let serial = DeviceId::new(device_serial);
        eprintln!("scrcpy: stage=connect_start serial={device_serial}");
        // Per-session scid: the server binds `scrcpy_<scid>` and we forward
        // to it, so a lingering server from another session (cleanup=false)
        // can never steal this session's socket.
        let scid = generate_scid();
        eprintln!("scrcpy: stage=scid scid={scid}");
        let jar = ensure_server_jar()?;
        eprintln!("scrcpy: stage=jar_ok path={}", jar.display());
        deploy_server_jar(&serial, &jar)?;
        eprintln!("scrcpy: stage=jar_pushed dest={SCRCPY_SERVER_DEVICE_PATH}");
        let port = pick_free_port()?;
        adb_forward(&serial, port, &scid)?;
        eprintln!("scrcpy: stage=forward_ok port={port} scid={scid}");
        spawn_server(&serial, &scid)?;
        eprintln!("scrcpy: stage=server_spawned version={SCRCPY_SERVER_VERSION}");

        // v2.x forward-tunnel handshake: video dummy probe, control
        // immediately, then the 76-byte header. See handshake_video_control
        // for why this exact order is required (deadlock otherwise).
        let (video, control_stream, header) = handshake_video_control(port)?;

        let control = ControlChannel::new(
            control_stream,
            header.width.min(u16::MAX as u32) as u16,
            header.height.min(u16::MAX as u32) as u16,
        );

        Ok(ScrcpyNative {
            serial,
            local_port: port,
            scid,
            video,
            control,
            device_name: header.device_name,
            video_width: header.width,
            video_height: header.height,
        })
    }

    /// This session's scid (8 hex chars). The server binds the abstract
    /// socket `scrcpy_<scid>`; its captured stdout/stderr live at
    /// [`server_log_path`] for this scid.
    pub fn scid(&self) -> &str {
        &self.scid
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Current video dimensions; touch coordinates are computed against these.
    pub fn video_size(&self) -> (u32, u32) {
        (self.video_width, self.video_height)
    }

    /// Iterate H.264 packets from the video socket. The iterator ends
    /// (`None`) when the server hangs up; I/O errors surface as
    /// `Err(DeviceError)` items.
    pub fn start_video_stream(&mut self) -> VideoPacketIter<'_> {
        VideoPacketIter {
            reader: VideoStreamReader::new(&self.video),
        }
    }

    /// Borrow the video socket and the control channel disjointly, so a
    /// caller can drive input on the control channel while simultaneously
    /// reading frames from the video socket (e.g. measuring fps while the
    /// screen animates). The two sockets are independent TCP streams, so
    /// holding both borrows at once is safe.
    pub fn split(&mut self) -> (&TcpStream, &mut ControlChannel<TcpStream>) {
        (&self.video, &mut self.control)
    }

    /// Adjust the video socket read timeout (10 s after [`ScrcpyNative::connect`]).
    /// Short timeouts let a caller poll for a frame with a deadline (a
    /// trial with no frame in time is a "miss", not a fatal error); restore
    /// the default afterwards with `set_video_read_timeout(Some(10s))`.
    pub fn set_video_read_timeout(&self, timeout: Option<Duration>) -> Result<(), DeviceError> {
        self.video
            .set_read_timeout(timeout)
            .map_err(DeviceError::Io)
    }

    /// Swipe gesture: Down → interpolated Moves → Up on pointer 0.
    /// See [`swipe_control`].
    pub fn swipe(
        &mut self,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
        duration: Duration,
    ) -> Result<(), DeviceError> {
        swipe_control(&mut self.control, x0, y0, x1, y1, duration).map_err(DeviceError::Io)
    }

    /// Multi-touch injection. `pointer_id` distinguishes concurrent
    /// pointers: a pinch is two pointers each doing Down → Move* → Up.
    pub fn inject_touch(
        &mut self,
        pointer_id: u64,
        x: u32,
        y: u32,
        action: TouchAction,
    ) -> Result<(), DeviceError> {
        self.control
            .inject_touch(action, pointer_id, x, y)
            .map_err(DeviceError::Io)
    }

    /// Scroll injection (mouse-wheel style): `hscroll`/`vscroll` in ticks,
    /// each clamped to [-1, 1]. Positive vscroll scrolls up.
    pub fn inject_scroll(
        &mut self,
        x: u32,
        y: u32,
        hscroll: f32,
        vscroll: f32,
    ) -> Result<(), DeviceError> {
        self.control
            .inject_scroll(x, y, hscroll, vscroll)
            .map_err(DeviceError::Io)
    }

    /// Convenience: tap = Down + Up on pointer 0.
    pub fn tap(&mut self, x: u32, y: u32) -> Result<(), DeviceError> {
        self.inject_touch(0, x, y, TouchAction::Down)?;
        self.inject_touch(0, x, y, TouchAction::Up)
    }

    /// Press and release a key (DOWN then UP). `Power` locks/unlocks.
    pub fn press_key(&mut self, keycode: AndroidKeycode) -> Result<(), DeviceError> {
        self.control.press_key(keycode).map_err(DeviceError::Io)
    }

    pub fn inject_text(&mut self, text: &str) -> Result<(), DeviceError> {
        self.control.inject_text(text).map_err(DeviceError::Io)
    }

    pub fn set_clipboard(&mut self, text: &str) -> Result<(), DeviceError> {
        self.control.set_clipboard(text).map_err(DeviceError::Io)
    }

    pub fn rotate_device(&mut self) -> Result<(), DeviceError> {
        self.control.rotate_device().map_err(DeviceError::Io)
    }

    /// Update the touch coordinate space (call when the video size changes,
    /// e.g. after rotation).
    pub fn set_video_size(&mut self, w: u32, h: u32) {
        self.video_width = w;
        self.video_height = h;
        self.control.set_screen_size(w, h);
    }
}

impl Drop for ScrcpyNative {
    fn drop(&mut self) {
        // Best effort: remove the forward. Closing `video` (field drop
        // order) stops the server on the device.
        let _ = adb_forward_remove(self.local_port);
        let _ = &self.serial;
    }
}

/// Iterator over [`H264Packet`]s from a borrowed video socket.
pub struct VideoPacketIter<'a> {
    reader: VideoStreamReader<&'a TcpStream>,
}

impl Iterator for VideoPacketIter<'_> {
    type Item = Result<H264Packet, DeviceError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.next_packet() {
            Ok(Some(pkt)) => Some(Ok(pkt)),
            Ok(None) => None,
            Err(e) => Some(Err(DeviceError::Io(e))),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests — all run without a device (sockets are mocked with Vec/Cursor).
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "device"))]
mod tests {
    use super::*;
    use std::io::Cursor;

    // -- SHA-256 known vectors (FIPS 180-4) --------------------------------

    #[test]
    fn sha256_empty_string_vector() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_abc_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_multiblock_vector() {
        // 56-byte FIPS vector: exercises multi-block padding.
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn jar_pin_is_self_consistent() {
        // The URL must name the pinned version, and the hash must be a
        // 64-char lowercase hex string.
        assert!(SCRCPY_SERVER_JAR_URL.ends_with("scrcpy-server-v2.7"));
        assert!(SCRCPY_SERVER_JAR_URL.contains(SCRCPY_SERVER_VERSION));
        assert_eq!(SCRCPY_SERVER_SHA256.len(), 64);
        assert!(SCRCPY_SERVER_SHA256.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // -- scid (per-session socket identity) ---------------------------------

    #[test]
    fn scid_is_31bit_8hex() {
        // The v2.7 server parses scid as hex (Integer.parseInt(value, 16))
        // and requires a 31-bit non-negative value; it then binds
        // `scrcpy_<hex8>`. Check the format contract (not the randomness).
        for _ in 0..100 {
            let scid = generate_scid();
            assert_eq!(scid.len(), 8, "scid must be 8 hex chars: {scid}");
            assert!(
                scid.chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "scid must be lowercase hex: {scid}"
            );
            let v = u32::from_str_radix(&scid, 16).expect("scid parses as hex");
            assert!(v <= 0x7FFF_FFFF, "scid must be 31-bit: {scid}");
        }
    }

    #[test]
    fn server_log_path_names_scid_and_stream() {
        let (out, err) = (
            server_log_path("a1b2c3d4", "stdout"),
            server_log_path("a1b2c3d4", "stderr"),
        );
        assert!(out.ends_with("scrcpy-server-a1b2c3d4.stdout.log"));
        assert!(err.ends_with("scrcpy-server-a1b2c3d4.stderr.log"));
        assert_ne!(out, err);
    }

    // -- control message encoding (byte-exact) ------------------------------

    #[test]
    fn touch_encoding_is_byte_exact() {
        let got = encode_inject_touch(
            TouchAction::Down,
            0x0102030405060708,
            100,
            200,
            1080,
            2400,
            1.0,
        );
        let expected: [u8; 32] = [
            0x02, // type = INJECT_TOUCH_EVENT
            0x00, // action = DOWN
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // pointer id
            0x00, 0x00, 0x00, 0x64, // x = 100
            0x00, 0x00, 0x00, 0xC8, // y = 200
            0x04, 0x38, // screen width = 1080
            0x09, 0x60, // screen height = 2400
            0xFF, 0xFF, // pressure 1.0 -> 0xffff
            0x00, 0x00, 0x00, 0x00, // action button
            0x00, 0x00, 0x00, 0x00, // buttons
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn touch_pointer_up_uses_action_6() {
        let got = encode_inject_touch(TouchAction::PointerUp, 7, 10, 20, 1080, 2400, 0.5);
        assert_eq!(got[0], 0x02);
        assert_eq!(got[1], 6); // POINTER_UP
        assert_eq!(&got[2..10], &[0, 0, 0, 0, 0, 0, 0, 7]);
        // pressure 0.5 -> (0x10000 * 0.5) = 0x8000
        assert_eq!(&got[22..24], &[0x80, 0x00]);
    }

    #[test]
    fn scroll_encoding_is_byte_exact() {
        // v2.7 wire format: scroll deltas are i16 FIXED-POINT (not i32).
        // Sending 25 bytes would desync the server's message stream.
        let got = encode_inject_scroll(100, 200, 1080, 2400, 0.0, 1.0, 0);
        let expected: [u8; 21] = [
            0x03, // type = INJECT_SCROLL_EVENT
            0x00, 0x00, 0x00, 0x64, // x = 100
            0x00, 0x00, 0x00, 0xC8, // y = 200
            0x04, 0x38, // screen width = 1080
            0x09, 0x60, // screen height = 2400
            0x00, 0x00, // hscroll 0.0
            0x7F, 0xFF, // vscroll 1.0 -> 0x7fff (decodes to exactly 1.0)
            0x00, 0x00, 0x00, 0x00, // buttons
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn scroll_negative_one_tick_encodes() {
        let got = encode_inject_scroll(0, 0, 1080, 2400, 0.0, -1.0, 0);
        assert_eq!(got[0], 0x03);
        assert_eq!(got.len(), 21);
        // -1.0 -> -32768 = 0x8000
        assert_eq!(&got[15..17], &[0x80, 0x00]);
    }

    #[test]
    fn scroll_zero_is_21_bytes() {
        let got = encode_inject_scroll(540, 1200, 1080, 2400, 0.0, 0.0, 0);
        assert_eq!(got.len(), 21);
        assert_eq!(got[0], control_type::INJECT_SCROLL_EVENT);
    }

    #[test]
    fn keycode_home_down_is_byte_exact() {
        let got = encode_inject_keycode(KeyAction::Down, AndroidKeycode::Home as u32);
        let expected: [u8; 14] = [
            0x00, // type = INJECT_KEYCODE
            0x00, // action = DOWN
            0x00, 0x00, 0x00, 0x03, // keycode HOME = 3
            0x00, 0x00, 0x00, 0x00, // repeat
            0x00, 0x00, 0x00, 0x00, // metastate
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn keycode_power_up_is_byte_exact() {
        let got = encode_inject_keycode(KeyAction::Up, AndroidKeycode::Power as u32);
        assert_eq!(got[0], 0x00);
        assert_eq!(got[1], 0x01); // UP
        assert_eq!(&got[2..6], &[0x00, 0x00, 0x00, 0x1A]); // POWER = 26
    }

    #[test]
    fn text_encoding_is_byte_exact() {
        let got = encode_inject_text("hi");
        assert_eq!(got, vec![0x01, 0x00, 0x00, 0x00, 0x02, b'h', b'i']);
    }

    #[test]
    fn set_clipboard_encoding_is_byte_exact() {
        let got = encode_set_clipboard(1, false, "x");
        let expected: Vec<u8> = vec![
            0x09, // type = SET_CLIPBOARD
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // sequence = 1
            0x00, // paste = false
            0x00, 0x00, 0x00, 0x01, // len = 1
            b'x',
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn rotate_device_is_single_byte_11() {
        assert_eq!(encode_rotate_device(), [11]);
    }

    #[test]
    fn back_or_screen_on_is_byte_exact() {
        assert_eq!(encode_back_or_screen_on(KeyAction::Down), [4, 0]);
        assert_eq!(encode_back_or_screen_on(KeyAction::Up), [4, 1]);
    }

    #[test]
    fn set_display_power_is_byte_exact() {
        assert_eq!(encode_set_display_power(true), [10, 1]);
        assert_eq!(encode_set_display_power(false), [10, 0]);
    }

    // -- control channel over a mock writer --------------------------------

    #[test]
    fn control_channel_writes_to_mock_sink() {
        let mut ch = ControlChannel::new(Vec::new(), 1080, 2400);
        ch.inject_touch(TouchAction::Down, 0, 50, 60).unwrap();
        ch.press_key(AndroidKeycode::Back).unwrap();
        ch.inject_text("ok").unwrap();
        ch.set_clipboard("clip").unwrap();
        ch.rotate_device().unwrap();
        let out = ch.into_inner();
        // 32 (touch) + 14*2 (key down/up) + 7 (text "ok") + 18 (clipboard "clip") + 1 (rotate)
        assert_eq!(out.len(), 32 + 28 + 7 + 18 + 1);
        assert_eq!(out[0], 0x02); // touch
        assert_eq!(out[32], 0x00); // keycode down
        assert_eq!(out[32 + 14], 0x00); // keycode up
        assert_eq!(out[32 + 28], 0x01); // text
        assert_eq!(out[32 + 28 + 7], 0x09); // set clipboard
        assert_eq!(out[out.len() - 1], 0x0B); // rotate
                                              // clipboard sequence starts at 1
        assert_eq!(
            &out[32 + 28 + 7 + 1..32 + 28 + 7 + 9],
            &[0, 0, 0, 0, 0, 0, 0, 1]
        );
    }

    // -- video parsing with synthetic data ----------------------------------

    fn synthetic_header(name: &str, w: u32, h: u32) -> Vec<u8> {
        let mut v = vec![0u8; 64];
        let bytes = name.as_bytes();
        v[..bytes.len()].copy_from_slice(bytes);
        v.extend_from_slice(b"h264");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v
    }

    fn synthetic_packet(pts: u64, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&pts.to_be_bytes());
        v.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn video_header_parsing() {
        let data = synthetic_header("emulator-5554", 1080, 2400);
        let mut r = VideoStreamReader::new(Cursor::new(data));
        let h = r.read_header().unwrap();
        assert_eq!(h.device_name, "emulator-5554");
        assert_eq!(h.codec, *b"h264");
        assert_eq!((h.width, h.height), (1080, 2400));
    }

    #[test]
    fn video_packets_parse_with_keyframe_detection() {
        // IDR NAL (type 5) -> keyframe; non-IDR (type 1) -> delta.
        let idr = vec![0x00, 0x00, 0x00, 0x01, 0x65, 0xAA, 0xBB];
        let pframe = vec![0x00, 0x00, 0x01, 0x41, 0xCC];
        let mut data = synthetic_header("dev", 100, 200);
        data.extend_from_slice(&synthetic_packet(1_000, &idr));
        data.extend_from_slice(&synthetic_packet(2_000, &pframe));
        let mut r = VideoStreamReader::new(Cursor::new(data));
        r.read_header().unwrap();

        let p1 = r.next_packet().unwrap().unwrap();
        assert_eq!(p1.pts, 1_000);
        assert_eq!(p1.data, idr);
        assert!(p1.is_keyframe);

        let p2 = r.next_packet().unwrap().unwrap();
        assert_eq!(p2.pts, 2_000);
        assert!(!p2.is_keyframe);

        assert!(r.next_packet().unwrap().is_none());
    }

    #[test]
    fn video_eof_returns_none() {
        let mut r = VideoStreamReader::new(Cursor::new(Vec::new()));
        assert!(r.next_packet().unwrap().is_none());
    }

    #[test]
    fn video_truncated_payload_is_error() {
        let mut data = Vec::new();
        data.extend_from_slice(&1u64.to_be_bytes());
        data.extend_from_slice(&100u32.to_be_bytes()); // claims 100 bytes
        data.extend_from_slice(&[0xAA; 10]); // only 10 present
        let mut r = VideoStreamReader::new(Cursor::new(data));
        let err = r.next_packet().unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn video_oversize_packet_is_rejected() {
        let mut data = Vec::new();
        data.extend_from_slice(&1u64.to_be_bytes());
        data.extend_from_slice(&u32::MAX.to_be_bytes());
        let mut r = VideoStreamReader::new(Cursor::new(data));
        let err = r.next_packet().unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    // -- unified wire format (§9.3) ------------------------------------------

    #[test]
    fn wire_frame_roundtrip() {
        for (ftype, payload) in [
            (WIRE_DESCRIPTION, b"meta".as_slice()),
            (WIRE_KEYFRAME, b"\x00\x00\x00\x01\x65".as_slice()),
            (WIRE_DELTA, b"\x00\x00\x00\x01\x41".as_slice()),
            (WIRE_JPEG_SEED, b"\xff\xd8\xff".as_slice()),
        ] {
            let framed = wire_frame(ftype, payload);
            assert_eq!(framed[0], ftype);
            let (t, p, consumed) = parse_wire_frame(&framed).unwrap();
            assert_eq!(t, ftype);
            assert_eq!(p, payload);
            assert_eq!(consumed, framed.len());
        }
    }

    #[test]
    fn wire_frame_partial_buffer_returns_none() {
        let framed = wire_frame(WIRE_DELTA, b"12345");
        assert!(parse_wire_frame(&framed[..3]).is_none()); // no length yet
        assert!(parse_wire_frame(&framed[..7]).is_none()); // length but no payload
        assert!(parse_wire_frame(&framed).is_some());
    }

    #[test]
    fn h264_packet_maps_to_keyframe_or_delta() {
        let kf = H264Packet {
            pts: 1,
            data: vec![0, 0, 0, 1, 0x65],
            is_keyframe: true,
        };
        let (t, _, _) = parse_wire_frame(&kf.to_wire_format()).unwrap();
        assert_eq!(t, WIRE_KEYFRAME);

        let delta = H264Packet {
            pts: 2,
            data: vec![0, 0, 0, 1, 0x41],
            is_keyframe: false,
        };
        let (t, _, _) = parse_wire_frame(&delta.to_wire_format()).unwrap();
        assert_eq!(t, WIRE_DELTA);
    }

    #[test]
    fn description_frame_carries_codec_and_size() {
        let f = description_frame("h264", 1080, 2400);
        let (t, p, _) = parse_wire_frame(&f).unwrap();
        assert_eq!(t, WIRE_DESCRIPTION);
        assert_eq!(p, br#"{"codec":"h264","width":1080,"height":2400}"#);
    }

    // -- fallback warning -----------------------------------------------------

    #[test]
    fn fallback_warning_string_is_exact() {
        assert_eq!(
            ADB_FALLBACK_WARNING,
            "scrcpy control unavailable, falling back to adb shell input (200-500ms latency, no multi-touch)"
        );
    }

    #[test]
    fn fallback_without_adb_fails_honestly() {
        if tool_on_path("adb") {
            return; // adb exists here; can't test the fallback without a device.
        }
        let err =
            fallback_adb_input(&DeviceId::new("emulator-5554"), &["tap", "1", "1"]).unwrap_err();
        assert!(matches!(err, DeviceError::ToolMissing(_)));
    }

    // -- v2.x forward-tunnel handshake ---------------------------------------
    //
    // Regression test for the CI run #8 deadlock (2026-09-26): with
    // tunnel_forward=true the server accepts ALL sockets first (video,
    // then control) and only then writes anything. A client that waits
    // for the video header before opening the control socket deadlocks.
    //
    // The fake server below reproduces the exact server ordering:
    // accept video, accept control, THEN write (dummy + header). If the
    // handshake ever regresses to header-before-control, this test hangs
    // and fails (the 10s dummy-probe deadline keeps it bounded).

    #[test]
    fn handshake_accepts_both_sockets_before_writing() {
        use std::io::Write;
        use std::net::TcpListener;
        use std::sync::mpsc;
        use std::time::Duration;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        // Record the order the server accepted the sockets.
        let (order_tx, order_rx) = mpsc::channel::<&'static str>();

        let server = std::thread::spawn(move || {
            // Accept video first...
            let (mut video, _) = listener.accept().unwrap();
            order_tx.send("video").unwrap();
            // ...then control, BEFORE writing anything (the real server's
            // ordering with tunnel_forward=true).
            let (mut control, _) = listener.accept().unwrap();
            order_tx.send("control").unwrap();

            // Now write to video: 1 dummy byte, 64-byte device name,
            // 12-byte codec meta (u32 codec, u32 width, u32 height).
            video.write_all(&[0x00]).unwrap();
            let mut name = [0u8; 64];
            name[..9].copy_from_slice(b"fake-dev\x00");
            video.write_all(&name).unwrap();
            video.write_all(b"h264").unwrap();
            video.write_all(&1080u32.to_be_bytes()).unwrap();
            video.write_all(&2400u32.to_be_bytes()).unwrap();
            video.flush().unwrap();

            // Hold the control socket open briefly so the client sees a
            // live peer, then close both.
            std::thread::sleep(Duration::from_millis(200));
            drop(control);
            drop(video);
        });

        // The handshake must complete without deadlock.
        let (_video, _control, header) =
            handshake_video_control(port).expect("handshake against fake server must succeed");

        assert_eq!(header.device_name, "fake-dev");
        assert_eq!(header.codec, *b"h264");
        assert_eq!(header.width, 1080);
        assert_eq!(header.height, 2400);

        // The server must have accepted video before control.
        assert_eq!(
            order_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            "video"
        );
        assert_eq!(
            order_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            "control"
        );

        server.join().unwrap();
    }

    #[test]
    fn handshake_rejects_wrong_codec() {
        use std::io::Write;
        use std::net::TcpListener;
        use std::time::Duration;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = std::thread::spawn(move || {
            let (mut video, _) = listener.accept().unwrap();
            let (_control, _) = listener.accept().unwrap();
            video.write_all(&[0x00]).unwrap();
            video.write_all(&[0u8; 64]).unwrap();
            video.write_all(b"h265").unwrap(); // wrong codec
            video.write_all(&1080u32.to_be_bytes()).unwrap();
            video.write_all(&2400u32.to_be_bytes()).unwrap();
            video.flush().unwrap();
            std::thread::sleep(Duration::from_millis(200));
        });

        let err = handshake_video_control(port).unwrap_err();
        assert!(
            matches!(err, DeviceError::Parse(ref m) if m.contains("unexpected scrcpy video codec")),
            "expected codec parse error, got: {err:?}"
        );
        server.join().unwrap();
    }

    #[test]
    fn swipe_control_sends_down_moves_up() {
        use std::time::Duration;

        let mut ch = ControlChannel::new(Vec::new(), 1080, 2400);
        swipe_control(&mut ch, 540, 1800, 540, 600, Duration::from_millis(120)).unwrap();
        let bytes = ch.into_inner();
        // Down + 12 interpolated Moves + Up, 32 bytes each.
        assert_eq!(bytes.len(), 14 * 32, "expected 14 touch messages");
        let msg = |i: usize| &bytes[i * 32..(i + 1) * 32];
        // Every message is INJECT_TOUCH_EVENT.
        for i in 0..14 {
            assert_eq!(msg(i)[0], control_type::INJECT_TOUCH_EVENT, "msg {i} type");
        }
        assert_eq!(msg(0)[1], TouchAction::Down as u8, "first msg action");
        for i in 1..13 {
            assert_eq!(msg(i)[1], TouchAction::Move as u8, "msg {i} action");
        }
        assert_eq!(msg(13)[1], TouchAction::Up as u8, "last msg action");
        // Y interpolates monotonically from 1800 down to 600.
        let y = |i: usize| u32::from_be_bytes(msg(i)[14..18].try_into().unwrap());
        assert_eq!(y(0), 1800);
        assert_eq!(y(13), 600);
        let mut prev = y(0);
        for i in 1..14 {
            assert!(y(i) <= prev, "y must not increase during an upward swipe");
            prev = y(i);
        }
    }
}
