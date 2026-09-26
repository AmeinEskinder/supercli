//! Unified stream wire format for device screens (docs/device.md §9.3).
//!
//! Adopts baguette's framing verbatim so the web Devices panel and the
//! gpuidart P0-6 surface stay platform-agnostic: one decoder path, one input
//! path, platform selected only by the device id.
//!
//! Frame layout on the wire: one type byte followed by the payload.
//! Transport framing (message boundaries) is the caller's job — on a
//! WebSocket each message is one frame; on a raw TCP socket the proxy must
//! length-prefix frames itself.
//!
//! Touch input uses [`DevicePoint`]: coordinates in the device's point
//! coordinate system (baguette's convention) — the same units as the
//! screen size in points, so callers can pipe the 0x01 description's
//! `width_points`/`height_points` straight back as input.
//!
//! No feature gate: pure `std`, always compiled.

use std::error::Error;
use std::fmt;

/// Frame carrying stream metadata (device name, OS, resolution,
/// orientation). Payload schema: [`StreamDescription::encode`].
pub const FRAME_DESCRIPTION: u8 = 0x01;
/// Full frame: H.264 IDR or JPEG.
pub const FRAME_KEYFRAME: u8 = 0x02;
/// Incremental frame: H.264 P-frame.
pub const FRAME_DELTA: u8 = 0x03;
/// JPEG seed: initial frame for fast start / recovery on packet loss.
pub const FRAME_JPEG_SEED: u8 = 0x04;

/// True for the four defined frame types (0x01–0x04).
pub fn is_known_frame_type(frame_type: u8) -> bool {
    matches!(
        frame_type,
        FRAME_DESCRIPTION | FRAME_KEYFRAME | FRAME_DELTA | FRAME_JPEG_SEED
    )
}

/// Errors from wire-format encode/decode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    /// Tried to decode an empty buffer (no type byte).
    Empty,
    /// A frame arrived with a type byte outside 0x01–0x04.
    UnknownFrameType(u8),
    /// A pixel↔point conversion was attempted with a zero display density.
    ZeroDensity,
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireError::Empty => f.write_str("wire frame is empty (missing type byte)"),
            WireError::UnknownFrameType(t) => {
                write!(f, "unknown wire frame type 0x{t:02x}")
            }
            WireError::ZeroDensity => {
                f.write_str("cannot map pixels to device points with a zero density_dpi")
            }
        }
    }
}

impl Error for WireError {}

/// A single wire frame: one type byte followed by the payload.
pub struct WireFrame;

impl WireFrame {
    /// Encode one frame. `frame_type` is one of the `FRAME_*` constants;
    /// unknown values pass through untouched (forward compatibility).
    pub fn encode(frame_type: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(1 + payload.len());
        out.push(frame_type);
        out.extend_from_slice(payload);
        out
    }

    /// Split one frame into `(frame_type, payload)`. Accepts unknown type
    /// bytes — use [`is_known_frame_type`] when strictness matters.
    pub fn decode(data: &[u8]) -> Result<(u8, &[u8]), WireError> {
        match data.split_first() {
            Some((&frame_type, payload)) => Ok((frame_type, payload)),
            None => Err(WireError::Empty),
        }
    }
}

/// A point in the device's point coordinate system (baguette's convention).
///
/// Device points are the same units as the screen size in points, so a
/// caller can pipe the 0x01 description's `width_points`/`height_points`
/// straight back as touch input without rescaling. iOS points are
/// pixels / scale; Android points are pixels * 160 / density_dpi.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DevicePoint {
    pub x: f32,
    pub y: f32,
}

impl DevicePoint {
    /// Map Android device pixels to device points via the display density:
    /// points = pixels * 160 / density_dpi.
    pub fn from_android_pixels(px: u32, py: u32, density_dpi: u32) -> Result<Self, WireError> {
        if density_dpi == 0 {
            return Err(WireError::ZeroDensity);
        }
        Ok(DevicePoint {
            x: px as f32 * 160.0 / density_dpi as f32,
            y: py as f32 * 160.0 / density_dpi as f32,
        })
    }

    /// Map device points back to Android device pixels (rounds to nearest).
    pub fn to_android_pixels(self, density_dpi: u32) -> Result<(u32, u32), WireError> {
        if density_dpi == 0 {
            return Err(WireError::ZeroDensity);
        }
        Ok((
            (self.x * density_dpi as f32 / 160.0).round() as u32,
            (self.y * density_dpi as f32 / 160.0).round() as u32,
        ))
    }
}

/// One H.264 access unit from the native scrcpy client (video socket).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct H264Packet {
    /// True for an IDR (keyframe), false for a P-frame.
    pub keyframe: bool,
    pub data: Vec<u8>,
}

/// Wrap a baguette frame for the unified stream: baguette already speaks
/// this format — and already uses device points for coordinates — so this
/// is a validated passthrough (byte-identical).
pub fn wire_from_baguette(frame: &[u8]) -> Result<Vec<u8>, WireError> {
    let (frame_type, _) = WireFrame::decode(frame)?;
    if !is_known_frame_type(frame_type) {
        return Err(WireError::UnknownFrameType(frame_type));
    }
    Ok(frame.to_vec())
}

/// Wrap an H.264 packet from the native scrcpy client: IDR → 0x02
/// keyframe, P-frame → 0x03 delta.
pub fn wire_from_h264(packet: &H264Packet) -> Vec<u8> {
    let frame_type = if packet.keyframe {
        FRAME_KEYFRAME
    } else {
        FRAME_DELTA
    };
    WireFrame::encode(frame_type, &packet.data)
}

/// Stream metadata carried in a [`FRAME_DESCRIPTION`] payload, encoded as
/// UTF-8 JSON. `platform` is `"android"` or `"ios"`; `orientation` is
/// `"portrait"` or `"landscape"`. `width_points`/`height_points` are the
/// screen size in device points — the same units as [`DevicePoint`], so
/// clients can scale their UI and pipe coordinates straight back as touch
/// input. `density_dpi` is present for Android only (needed for the
/// pixel↔point mapping); on iOS points = pixels / scale, so it is omitted.
#[derive(Clone, Debug, PartialEq)]
pub struct StreamDescription {
    pub device: String,
    pub platform: String,
    pub width_points: f32,
    pub height_points: f32,
    pub width_pixels: u32,
    pub height_pixels: u32,
    pub density_dpi: Option<u32>,
    pub orientation: String,
}

impl StreamDescription {
    /// Encode as UTF-8 JSON (hand-rolled: this crate is `std`-only).
    /// `density_dpi` is emitted only when `Some` (Android); iOS descriptions
    /// omit the key entirely.
    pub fn encode(&self) -> Vec<u8> {
        let density = match self.density_dpi {
            Some(d) => format!(",\"density_dpi\":{d}"),
            None => String::new(),
        };
        let json = format!(
            "{{\"device\":{},\"platform\":{},\"width_points\":{},\"height_points\":{},\"width_pixels\":{},\"height_pixels\":{}{},\"orientation\":{}}}",
            json_string(&self.device),
            json_string(&self.platform),
            self.width_points,
            self.height_points,
            self.width_pixels,
            self.height_pixels,
            density,
            json_string(&self.orientation),
        );
        json.into_bytes()
    }

    /// Encode as a complete [`FRAME_DESCRIPTION`] wire frame.
    pub fn wire_frame(&self) -> Vec<u8> {
        WireFrame::encode(FRAME_DESCRIPTION, &self.encode())
    }
}

/// Minimal JSON string escaper: quotes, backslash, and control characters.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_type_constants_match_baguette_framing() {
        assert_eq!(FRAME_DESCRIPTION, 0x01);
        assert_eq!(FRAME_KEYFRAME, 0x02);
        assert_eq!(FRAME_DELTA, 0x03);
        assert_eq!(FRAME_JPEG_SEED, 0x04);
        for t in [0x01u8, 0x02, 0x03, 0x04] {
            assert!(is_known_frame_type(t), "0x{t:02x} must be known");
        }
        assert!(!is_known_frame_type(0x00));
        assert!(!is_known_frame_type(0x05));
        assert!(!is_known_frame_type(0xff));
    }

    #[test]
    fn encode_decode_roundtrip_all_frame_types() {
        let payload = b"\x00\x01\x02binary-\xff-payload";
        for frame_type in [
            FRAME_DESCRIPTION,
            FRAME_KEYFRAME,
            FRAME_DELTA,
            FRAME_JPEG_SEED,
        ] {
            let encoded = WireFrame::encode(frame_type, payload);
            assert_eq!(encoded[0], frame_type);
            assert_eq!(&encoded[1..], payload);
            let (decoded_type, decoded_payload) = WireFrame::decode(&encoded).unwrap();
            assert_eq!(decoded_type, frame_type);
            assert_eq!(decoded_payload, payload);
        }
    }

    #[test]
    fn encode_decode_empty_payload() {
        let encoded = WireFrame::encode(FRAME_DELTA, &[]);
        assert_eq!(encoded, vec![FRAME_DELTA]);
        let (t, p) = WireFrame::decode(&encoded).unwrap();
        assert_eq!(t, FRAME_DELTA);
        assert!(p.is_empty());
    }

    #[test]
    fn decode_empty_is_error() {
        assert_eq!(WireFrame::decode(&[]), Err(WireError::Empty));
    }

    #[test]
    fn decode_unknown_type_passes_through() {
        // Forward compatibility: decode does not reject, callers decide.
        let (t, p) = WireFrame::decode(&[0x09, 0xaa]).unwrap();
        assert_eq!(t, 0x09);
        assert_eq!(p, &[0xaa]);
    }

    #[test]
    fn wire_from_baguette_is_byte_identical_passthrough() {
        let frame = WireFrame::encode(FRAME_KEYFRAME, b"h264-bytes");
        let out = wire_from_baguette(&frame).unwrap();
        assert_eq!(out, frame);
    }

    #[test]
    fn wire_from_baguette_rejects_unknown_type() {
        let bad = WireFrame::encode(0x7f, b"nope");
        assert_eq!(
            wire_from_baguette(&bad),
            Err(WireError::UnknownFrameType(0x7f))
        );
        assert_eq!(wire_from_baguette(&[]), Err(WireError::Empty));
    }

    #[test]
    fn wire_from_h264_maps_keyframe_and_delta() {
        let idr = H264Packet {
            keyframe: true,
            data: b"idr-data".to_vec(),
        };
        let p = H264Packet {
            keyframe: false,
            data: b"p-data".to_vec(),
        };
        let kf = wire_from_h264(&idr);
        let delta = wire_from_h264(&p);
        assert_eq!(kf[0], FRAME_KEYFRAME);
        assert_eq!(&kf[1..], b"idr-data");
        assert_eq!(delta[0], FRAME_DELTA);
        assert_eq!(&delta[1..], b"p-data");
    }

    #[test]
    fn device_point_android_pixel_roundtrip() {
        // density 160: points == pixels exactly.
        for (px, py, dpi) in [
            (0u32, 0u32, 160u32),
            (100, 200, 160),
            (1080, 2400, 160),
            // density 320: 1080px -> 540pt, exact round-trip.
            (0, 0, 320),
            (540, 1200, 320),
            (1080, 2400, 320),
            // density 420: non-integral points still round-trip.
            (0, 0, 420),
            (420, 840, 420),
            (840, 1680, 420),
        ] {
            let p = DevicePoint::from_android_pixels(px, py, dpi).unwrap();
            assert_eq!(
                p.to_android_pixels(dpi).unwrap(),
                (px, py),
                "roundtrip failed for ({px},{py}) @ {dpi}dpi"
            );
        }
    }

    #[test]
    fn device_point_android_conversion_uses_density() {
        // points = pixels * 160 / density_dpi (baguette's device-point units).
        let p = DevicePoint::from_android_pixels(1080, 2400, 320).unwrap();
        assert_eq!(
            p,
            DevicePoint {
                x: 540.0,
                y: 1200.0
            }
        );
        let p = DevicePoint::from_android_pixels(1080, 2400, 420).unwrap();
        let expected_x = 1080.0f32 * 160.0 / 420.0;
        let expected_y = 2400.0f32 * 160.0 / 420.0;
        assert!(
            (p.x - expected_x).abs() < 1e-3 && (p.y - expected_y).abs() < 1e-3,
            "got ({}, {}), want ({}, {})",
            p.x,
            p.y,
            expected_x,
            expected_y
        );
        // Back to pixels through the same density.
        assert_eq!(p.to_android_pixels(420).unwrap(), (1080, 2400));
    }

    #[test]
    fn device_point_zero_density_is_error() {
        assert_eq!(
            DevicePoint::from_android_pixels(1, 1, 0),
            Err(WireError::ZeroDensity)
        );
        assert_eq!(
            DevicePoint { x: 1.0, y: 1.0 }.to_android_pixels(0),
            Err(WireError::ZeroDensity)
        );
    }

    #[test]
    fn stream_description_android_includes_point_size_and_density() {
        let d = StreamDescription {
            device: "Pixel 7".to_string(),
            platform: "android".to_string(),
            width_points: 540.0,
            height_points: 1200.0,
            width_pixels: 1080,
            height_pixels: 2400,
            density_dpi: Some(320),
            orientation: "portrait".to_string(),
        };
        let json = String::from_utf8(d.encode()).unwrap();
        assert_eq!(
            json,
            r#"{"device":"Pixel 7","platform":"android","width_points":540,"height_points":1200,"width_pixels":1080,"height_pixels":2400,"density_dpi":320,"orientation":"portrait"}"#
        );
        let frame = d.wire_frame();
        assert_eq!(frame[0], FRAME_DESCRIPTION);
        assert_eq!(&frame[1..], json.as_bytes());
    }

    #[test]
    fn stream_description_ios_omits_density_dpi() {
        let d = StreamDescription {
            device: "iPhone 15".to_string(),
            platform: "ios".to_string(),
            width_points: 390.0,
            height_points: 844.0,
            width_pixels: 1170,
            height_pixels: 2532,
            density_dpi: None,
            orientation: "portrait".to_string(),
        };
        let json = String::from_utf8(d.encode()).unwrap();
        assert_eq!(
            json,
            r#"{"device":"iPhone 15","platform":"ios","width_points":390,"height_points":844,"width_pixels":1170,"height_pixels":2532,"orientation":"portrait"}"#
        );
        assert!(!json.contains("density_dpi"));
    }

    #[test]
    fn stream_description_escapes_strings() {
        let d = StreamDescription {
            device: "weird \"name\" \\ \u{1}".to_string(),
            platform: "ios".to_string(),
            width_points: 390.0,
            height_points: 844.0,
            width_pixels: 1170,
            height_pixels: 2532,
            density_dpi: None,
            orientation: "landscape".to_string(),
        };
        let json = String::from_utf8(d.encode()).unwrap();
        // The device name must survive as valid JSON: quotes and backslash
        // escaped, U+0001 as the six-character \u0001 escape.
        let expected = concat!(
            "{\"device\":\"weird \\\"name\\\" \\\\ \\u0001\",",
            "\"platform\":\"ios\",\"width_points\":390,\"height_points\":844,",
            "\"width_pixels\":1170,\"height_pixels\":2532,",
            "\"orientation\":\"landscape\"}"
        );
        assert_eq!(json, expected);
    }

    #[test]
    fn wire_error_display_strings() {
        assert_eq!(
            WireError::Empty.to_string(),
            "wire frame is empty (missing type byte)"
        );
        assert_eq!(
            WireError::UnknownFrameType(0x7f).to_string(),
            "unknown wire frame type 0x7f"
        );
        assert!(WireError::ZeroDensity.to_string().contains("zero density"));
    }
}
