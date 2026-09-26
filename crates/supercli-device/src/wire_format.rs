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
//! Touch input uses [`DevicePoint`]: coordinates normalized to 0.0–1.0 on
//! both axes, independent of device pixels.
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
    /// A point was normalized against a zero width or height.
    ZeroDimension,
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireError::Empty => f.write_str("wire frame is empty (missing type byte)"),
            WireError::UnknownFrameType(t) => {
                write!(f, "unknown wire frame type 0x{t:02x}")
            }
            WireError::ZeroDimension => {
                f.write_str("cannot normalize a point against a zero width or height")
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

/// Normalized device point: 0.0–1.0 on both axes, independent of pixels.
/// All touch input on the wire uses these coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DevicePoint {
    pub x: f32,
    pub y: f32,
}

impl DevicePoint {
    /// Convert device pixels to normalized coordinates. Values outside the
    /// screen clamp to [0.0, 1.0] rather than erroring.
    pub fn from_pixels(x: u32, y: u32, width: u32, height: u32) -> Result<Self, WireError> {
        if width == 0 || height == 0 {
            return Err(WireError::ZeroDimension);
        }
        Ok(DevicePoint {
            x: (x as f32 / width as f32).clamp(0.0, 1.0),
            y: (y as f32 / height as f32).clamp(0.0, 1.0),
        })
    }

    /// Convert back to device pixels (rounds to the nearest pixel).
    pub fn to_pixels(self, width: u32, height: u32) -> (u32, u32) {
        (
            (self.x * width as f32).round() as u32,
            (self.y * height as f32).round() as u32,
        )
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
/// this format, so this is a validated passthrough (byte-identical).
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
/// UTF-8 JSON: `{"device":"Pixel 7","platform":"android","width":1080,
/// "height":2400,"orientation":"portrait"}`. `platform` is `"android"` or
/// `"ios"`; `orientation` is `"portrait"` or `"landscape"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamDescription {
    pub device: String,
    pub platform: String,
    pub width: u32,
    pub height: u32,
    pub orientation: String,
}

impl StreamDescription {
    /// Encode as UTF-8 JSON (hand-rolled: this crate is `std`-only).
    pub fn encode(&self) -> Vec<u8> {
        let json = format!(
            "{{\"device\":{},\"platform\":{},\"width\":{},\"height\":{},\"orientation\":{}}}",
            json_string(&self.device),
            json_string(&self.platform),
            self.width,
            self.height,
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
    fn device_point_pixel_roundtrip() {
        for (x, y, w, h) in [
            (0u32, 0u32, 1080u32, 2400u32),
            (100, 200, 1080, 2400),
            (1079, 2399, 1080, 2400),
            (1080, 2400, 1080, 2400),
            (540, 1200, 1080, 2400),
            (10, 20, 1170, 2532),
        ] {
            let p = DevicePoint::from_pixels(x, y, w, h).unwrap();
            assert!(
                (0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y),
                "out of range for ({x},{y}) in {w}x{h}"
            );
            assert_eq!(p.to_pixels(w, h), (x, y), "roundtrip failed for ({x},{y})");
        }
    }

    #[test]
    fn device_point_clamps_out_of_bounds() {
        let p = DevicePoint::from_pixels(2000, 5000, 1080, 2400).unwrap();
        assert_eq!(p, DevicePoint { x: 1.0, y: 1.0 });
    }

    #[test]
    fn device_point_zero_dimension_is_error() {
        assert_eq!(
            DevicePoint::from_pixels(1, 1, 0, 2400),
            Err(WireError::ZeroDimension)
        );
        assert_eq!(
            DevicePoint::from_pixels(1, 1, 1080, 0),
            Err(WireError::ZeroDimension)
        );
    }

    #[test]
    fn stream_description_encodes_exact_schema() {
        let d = StreamDescription {
            device: "Pixel 7".to_string(),
            platform: "android".to_string(),
            width: 1080,
            height: 2400,
            orientation: "portrait".to_string(),
        };
        let json = String::from_utf8(d.encode()).unwrap();
        assert_eq!(
            json,
            r#"{"device":"Pixel 7","platform":"android","width":1080,"height":2400,"orientation":"portrait"}"#
        );
        let frame = d.wire_frame();
        assert_eq!(frame[0], FRAME_DESCRIPTION);
        assert_eq!(&frame[1..], json.as_bytes());
    }

    #[test]
    fn stream_description_escapes_strings() {
        let d = StreamDescription {
            device: "weird \"name\" \\ \u{1}".to_string(),
            platform: "ios".to_string(),
            width: 1170,
            height: 2532,
            orientation: "landscape".to_string(),
        };
        let json = String::from_utf8(d.encode()).unwrap();
        // The device name must survive as valid JSON: quotes and backslash
        // escaped, U+0001 as the six-character \u0001 escape.
        let expected = concat!(
            "{\"device\":\"weird \\\"name\\\" \\\\ \\u0001\",",
            "\"platform\":\"ios\",\"width\":1170,\"height\":2532,",
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
        assert!(WireError::ZeroDimension.to_string().contains("zero width"));
    }
}
