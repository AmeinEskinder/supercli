//! Unix domain socket client transport for the `unpeel.ui/1` protocol.
//!
//! Port of the Foundation-only transport logic from
//! `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIUnixSessionClient.swift`.
//!
//! This module contains the UI-independent pieces:
//! - [`UIProjectionDelivery`] — snapshot vs delta delivery signal
//! - [`ConnectionState`] — connection lifecycle states
//! - [`FrameDecoder`] — newline-delimited JSON frame decoding with size limits
//! - [`ReconnectBackoff`] — exponential backoff for reconnect attempts
//! - [`ClientConfiguration`] — socket path and client identity
//!
//! The UI message types (`UIMessage`, `UISnapshot`, `UIAction`, etc.) and the
//! full `NWConnection`-based client remain Dart-side per the sidecar
//! (`Dart: appkit_widgets`). This module provides the portable framing and
//! connection logic that any Rust consumer of the Unix socket needs.

/// The wire representation used for the most recent accepted projection.
///
/// The client always exposes a complete snapshot to renderers, including after
/// it applies a delta. This separate signal lets test rigs verify whether the
/// host sent a snapshot or a compact delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UIProjectionDelivery {
    /// A full snapshot was delivered.
    Snapshot { revision: i64 },
    /// A delta was applied on top of a base revision.
    Delta {
        base_revision: i64,
        revision: i64,
        operation_count: usize,
    },
}

/// Lifecycle states of the Unix socket connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not running.
    Stopped,
    /// Attempting to connect.
    Connecting,
    /// Attached to an app instance.
    Attached {
        app_instance_id: String,
        resumed: bool,
    },
    /// Waiting before the next reconnect attempt.
    WaitingToReconnect,
}

/// Maximum frame size: 16 MiB, matching Swift's `maximumFrameBytes`.
pub const MAXIMUM_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Maximum bytes to read in a single receive call: 64 KiB.
pub const RECEIVE_CHUNK_BYTES: usize = 64 * 1024;

/// Decodes newline-delimited frames from a byte stream.
///
/// Frames are split on `\n` (0x0A), with an optional trailing `\r` (0x0D)
/// stripped. Empty frames are skipped. Frames larger than
/// [`MAXIMUM_FRAME_BYTES`] cause the decoder to signal overflow.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    /// Create a new empty decoder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append received bytes to the internal buffer.
    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Number of bytes currently buffered.
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    /// Clear the buffer, keeping allocated capacity.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Decode all complete frames currently in the buffer.
    ///
    /// Returns the frames and whether an overflow was detected. On overflow,
    /// the caller should drop the connection (matching Swift's
    /// `connection?.cancel()` behavior).
    pub fn decode_available(&mut self) -> (Vec<Vec<u8>>, bool) {
        let mut frames = Vec::new();
        let mut overflow = false;

        while let Some(nl_pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let mut frame = self.buffer[..nl_pos].to_vec();
            self.buffer.drain(..=nl_pos);
            // Strip trailing CR (CRLF line endings).
            if frame.last() == Some(&b'\r') {
                frame.pop();
            }
            if frame.is_empty() {
                continue;
            }
            if frame.len() > MAXIMUM_FRAME_BYTES {
                overflow = true;
                break;
            }
            frames.push(frame);
        }

        if !overflow && self.buffer.len() > MAXIMUM_FRAME_BYTES {
            overflow = true;
        }

        (frames, overflow)
    }
}

/// Exponential backoff for reconnect attempts.
///
/// Matches Swift's `scheduleReconnect`: `delay = min(2^(attempt-1) * 0.1, 5.0)`
/// seconds, with attempts capped at 8.
#[derive(Debug, Clone)]
pub struct ReconnectBackoff {
    attempt: u32,
}

impl ReconnectBackoff {
    /// Maximum reconnect attempt before the backoff saturates.
    pub const MAX_ATTEMPT: u32 = 8;
    /// Maximum delay in seconds.
    pub const MAX_DELAY_SECS: f64 = 5.0;
    /// Base delay in seconds.
    pub const BASE_DELAY_SECS: f64 = 0.1;

    /// Create a new backoff starting at attempt 0.
    pub fn new() -> Self {
        Self { attempt: 0 }
    }

    /// Reset the backoff after a successful connection.
    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    /// Current attempt number (0 before any failure).
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Advance to the next attempt and return the delay in seconds.
    pub fn next_delay_secs(&mut self) -> f64 {
        self.attempt = (self.attempt + 1).min(Self::MAX_ATTEMPT);
        let exp = 2f64.powi(self.attempt as i32 - 1);
        (exp * Self::BASE_DELAY_SECS).min(Self::MAX_DELAY_SECS)
    }
}

impl Default for ReconnectBackoff {
    fn default() -> Self {
        Self::new()
    }
}

/// Client identity and socket configuration.
///
/// Mirrors Swift's `UIUnixSessionClient.Configuration`, minus the
/// UI-specific renderer metadata types (those remain Dart-side).
#[derive(Debug, Clone)]
pub struct ClientConfiguration {
    /// Path to the Unix domain socket.
    pub socket_path: String,
    /// Stable client identifier.
    pub client_id: String,
    /// View identifier.
    pub view_id: String,
}

impl ClientConfiguration {
    /// Create a new configuration.
    pub fn new(
        socket_path: impl Into<String>,
        client_id: impl Into<String>,
        view_id: impl Into<String>,
    ) -> Self {
        Self {
            socket_path: socket_path.into(),
            client_id: client_id.into(),
            view_id: view_id.into(),
        }
    }
}

/// Extract the command head from a launch command for protocol matching.
///
/// (Re-exported here for convenience; the canonical implementation lives in
/// `supercli-core::provider_theme::command_head`.)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_delivery_snapshot_eq() {
        let a = UIProjectionDelivery::Snapshot { revision: 42 };
        let b = UIProjectionDelivery::Snapshot { revision: 42 };
        assert_eq!(a, b);
        assert_ne!(a, UIProjectionDelivery::Snapshot { revision: 43 });
    }

    #[test]
    fn projection_delivery_delta_fields() {
        let d = UIProjectionDelivery::Delta {
            base_revision: 10,
            revision: 12,
            operation_count: 3,
        };
        match d {
            UIProjectionDelivery::Delta {
                base_revision,
                revision,
                operation_count,
            } => {
                assert_eq!(base_revision, 10);
                assert_eq!(revision, 12);
                assert_eq!(operation_count, 3);
            }
            _ => panic!("expected delta"),
        }
    }

    #[test]
    fn connection_state_attached_eq() {
        let a = ConnectionState::Attached {
            app_instance_id: "abc".to_string(),
            resumed: true,
        };
        let b = ConnectionState::Attached {
            app_instance_id: "abc".to_string(),
            resumed: true,
        };
        assert_eq!(a, b);
        assert_ne!(a, ConnectionState::Stopped);
    }

    #[test]
    fn frame_decoder_single_frame() {
        let mut dec = FrameDecoder::new();
        dec.feed(b"{\"a\":1}\n");
        let (frames, overflow) = dec.decode_available();
        assert!(!overflow);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"{\"a\":1}");
        assert_eq!(dec.buffered_len(), 0);
    }

    #[test]
    fn frame_decoder_multiple_frames() {
        let mut dec = FrameDecoder::new();
        dec.feed(b"one\ntwo\nthree\n");
        let (frames, overflow) = dec.decode_available();
        assert!(!overflow);
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0], b"one");
        assert_eq!(frames[2], b"three");
    }

    #[test]
    fn frame_decoder_crlf_stripped() {
        let mut dec = FrameDecoder::new();
        dec.feed(b"frame\r\n");
        let (frames, overflow) = dec.decode_available();
        assert!(!overflow);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"frame");
    }

    #[test]
    fn frame_decoder_empty_frames_skipped() {
        let mut dec = FrameDecoder::new();
        dec.feed(b"\n\nactual\n\n");
        let (frames, overflow) = dec.decode_available();
        assert!(!overflow);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"actual");
    }

    #[test]
    fn frame_decoder_partial_frame_waits() {
        let mut dec = FrameDecoder::new();
        dec.feed(b"partial");
        let (frames, overflow) = dec.decode_available();
        assert!(!overflow);
        assert!(frames.is_empty());
        assert_eq!(dec.buffered_len(), 7);
        // Complete it.
        dec.feed(b"-done\n");
        let (frames, overflow) = dec.decode_available();
        assert!(!overflow);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"partial-done");
    }

    #[test]
    fn frame_decoder_split_feed() {
        let mut dec = FrameDecoder::new();
        dec.feed(b"hel");
        dec.feed(b"lo\nwor");
        dec.feed(b"ld\n");
        let (frames, overflow) = dec.decode_available();
        assert!(!overflow);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], b"hello");
        assert_eq!(frames[1], b"world");
    }

    #[test]
    fn frame_decoder_oversize_frame_overflow() {
        let mut dec = FrameDecoder::new();
        // Feed a frame larger than the max without a newline, then terminate it.
        let big = vec![b'x'; MAXIMUM_FRAME_BYTES + 1];
        dec.feed(&big);
        dec.feed(b"\n");
        let (_frames, overflow) = dec.decode_available();
        assert!(overflow, "oversize frame must signal overflow");
    }

    #[test]
    fn frame_decoder_buffer_overflow_without_newline() {
        let mut dec = FrameDecoder::new();
        dec.feed(&vec![b'y'; MAXIMUM_FRAME_BYTES + 100]);
        let (_frames, overflow) = dec.decode_available();
        assert!(
            overflow,
            "buffer over max without newline must signal overflow"
        );
    }

    #[test]
    fn frame_decoder_clear() {
        let mut dec = FrameDecoder::new();
        dec.feed(b"data");
        assert_eq!(dec.buffered_len(), 4);
        dec.clear();
        assert_eq!(dec.buffered_len(), 0);
    }

    #[test]
    fn reconnect_backoff_sequence() {
        let mut b = ReconnectBackoff::new();
        assert_eq!(b.attempt(), 0);
        // Swift: min(2^(n-1) * 0.1, 5.0)
        let d1 = b.next_delay_secs();
        assert!((d1 - 0.1).abs() < 1e-9, "attempt 1: 0.1s, got {d1}");
        let d2 = b.next_delay_secs();
        assert!((d2 - 0.2).abs() < 1e-9, "attempt 2: 0.2s, got {d2}");
        let d3 = b.next_delay_secs();
        assert!((d3 - 0.4).abs() < 1e-9, "attempt 3: 0.4s, got {d3}");
        assert_eq!(b.attempt(), 3);
    }

    #[test]
    fn reconnect_backoff_caps_at_5s() {
        let mut b = ReconnectBackoff::new();
        let mut last = 0.0;
        for _ in 0..12 {
            last = b.next_delay_secs();
        }
        assert!((last - 5.0).abs() < 1e-9, "capped at 5.0s, got {last}");
        assert_eq!(b.attempt(), ReconnectBackoff::MAX_ATTEMPT);
    }

    #[test]
    fn reconnect_backoff_reset() {
        let mut b = ReconnectBackoff::new();
        b.next_delay_secs();
        b.next_delay_secs();
        assert_eq!(b.attempt(), 2);
        b.reset();
        assert_eq!(b.attempt(), 0);
        let d = b.next_delay_secs();
        assert!((d - 0.1).abs() < 1e-9);
    }

    #[test]
    fn client_configuration_fields() {
        let c = ClientConfiguration::new("/tmp/sock", "client-1", "view-9");
        assert_eq!(c.socket_path, "/tmp/sock");
        assert_eq!(c.client_id, "client-1");
        assert_eq!(c.view_id, "view-9");
    }
}
