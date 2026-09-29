//! Portable terminal-pane callback types from
//! `clients/legacy/native/SupercliNative/Sources/SupercliNative/GhosttyBridge.swift`.
//!
//! `GhosttyBridge.swift` is the one file allowed to touch the Ghostty terminal
//! APIs; everything Ghostty-shaped is translated into plain value types at
//! this boundary. The AppKit view classes (`GhosttyTerminalPane`,
//! `RemoteGhosttyTerminalPane`, `PathDraggableTerminalView`) are macOS-native
//! and stay Swift-side — they cannot be expressed in this crate. This module
//! ports the Ghostty-free types the bridge uses to talk to the remote
//! transport:
//!
//! - [`RemoteTerminalViewport`]: pane geometry delivered to the transport.
//! - [`RemoteTerminalCallbackEpoch`]: rebinding token; stale callbacks are discarded.
//! - [`RemoteTerminalCallbackRelay`]: thread-safe callback indirection for a retained pane.
//! - [`RemoteTerminalLocalFeed`]: byte sequences fed only to the local VT parser.
//! - [`GhosttyTerminalPaneDelegate`]: the pane event protocol, as a trait.
//!
//! The Swift relay hops every delivery onto the main actor (`deliverOnMain`)
//! before re-checking the epoch. Rust has no main actor, so the hop is
//! collapsed: [`RemoteTerminalCallbackRelay::send_input`] captures the epoch
//! and delivers synchronously through the same epoch check. The observable
//! invariant is unchanged — a callback enqueued before a rebinding never
//! reaches a handler after the rebinding.

use std::sync::{Arc, Mutex};

/// Ghostty-free viewport value delivered to the remote transport whenever
/// the Controller's pane changes size. The transport decides how and when to
/// send it to the Host; this bridge never reaches into local session state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoteTerminalViewport {
    pub columns: u32,
    pub rows: u32,
    pub width_pixels: u32,
    pub height_pixels: u32,
    pub cell_width_pixels: u32,
    pub cell_height_pixels: u32,
}

impl RemoteTerminalViewport {
    pub fn new(
        columns: u32,
        rows: u32,
        width_pixels: u32,
        height_pixels: u32,
        cell_width_pixels: u32,
        cell_height_pixels: u32,
    ) -> Self {
        Self {
            columns,
            rows,
            width_pixels,
            height_pixels,
            cell_width_pixels,
            cell_height_pixels,
        }
    }
}

/// Token captured when a terminal callback is enqueued. Rebinding or clearing
/// a pane advances the token, so already-queued input/resize is discarded
/// instead of crossing into the replacement transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct RemoteTerminalCallbackEpoch {
    revision: u64,
}

impl RemoteTerminalCallbackEpoch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Mirrors Swift's `revision &+= 1`: wrapping increment.
    pub fn advance(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// A queued callback is deliverable only if no rebinding happened since
    /// it was enqueued.
    pub fn accepts(&self, queued: RemoteTerminalCallbackEpoch) -> bool {
        *self == queued
    }
}

type InputHandler = Arc<dyn Fn(Vec<u8>) + Send + Sync>;
type ResizeHandler = Arc<dyn Fn(RemoteTerminalViewport) + Send + Sync>;

struct RelayState {
    input_handler: InputHandler,
    resize_handler: ResizeHandler,
    epoch: RemoteTerminalCallbackEpoch,
    resize_live: bool,
}

/// Mutable callback indirection for a retained pane. A reconnect may replace
/// the transport while the terminal surface (and its last rendered frame)
/// stays alive, so the in-memory session must not permanently capture the
/// connection that happened to create it.
///
/// All interior state is guarded by one mutex; handlers are cloned out from
/// under the lock and invoked after it is released, mirroring Swift's
/// fetch-under-`NSLock`-then-call discipline.
pub struct RemoteTerminalCallbackRelay {
    state: Mutex<RelayState>,
}

impl RemoteTerminalCallbackRelay {
    pub fn new(
        input: impl Fn(Vec<u8>) + Send + Sync + 'static,
        resize: impl Fn(RemoteTerminalViewport) + Send + Sync + 'static,
    ) -> Self {
        Self {
            state: Mutex::new(RelayState {
                input_handler: Arc::new(input),
                resize_handler: Arc::new(resize),
                epoch: RemoteTerminalCallbackEpoch::new(),
                resize_live: true,
            }),
        }
    }

    /// Replace both handlers. Advances the epoch, so callbacks enqueued before
    /// this call are discarded at delivery.
    pub fn update(
        &self,
        input: impl Fn(Vec<u8>) + Send + Sync + 'static,
        resize: impl Fn(RemoteTerminalViewport) + Send + Sync + 'static,
    ) {
        let mut state = self.state.lock().expect("relay state lock");
        state.epoch.advance();
        state.input_handler = Arc::new(input);
        state.resize_handler = Arc::new(resize);
    }

    /// False while the owning pane is detached from a window or has
    /// presentation disabled. Ghostty core emits garbage geometry in that
    /// state (default ~50x17 grids from the unhosted layer); resizes while
    /// not live are DROPPED — a live re-present always refits from real bounds.
    pub fn set_resize_live(&self, live: bool) {
        self.state.lock().expect("relay state lock").resize_live = live;
    }

    pub fn is_resize_live(&self) -> bool {
        self.state.lock().expect("relay state lock").resize_live
    }

    /// Break transport/runtime captures as soon as a pane is evicted. The
    /// teardown itself is deferred by one main-queue turn in Swift, so relying
    /// on drop alone would leave a brief window where input could still hit a
    /// retired connection; advancing the epoch here closes that window.
    pub fn clear(&self) {
        let mut state = self.state.lock().expect("relay state lock");
        state.epoch.advance();
        state.input_handler = Arc::new(|_| {});
        state.resize_handler = Arc::new(|_| {});
    }

    pub fn current_epoch(&self) -> RemoteTerminalCallbackEpoch {
        self.state.lock().expect("relay state lock").epoch
    }

    /// Enqueue input for delivery. Captures the current epoch first (mirrors
    /// Swift's pre-`deliverOnMain` capture); the synchronous delivery below
    /// re-checks it.
    pub fn send_input(&self, data: Vec<u8>) {
        let queued_epoch = self.current_epoch();
        self.deliver_input(data, queued_epoch);
    }

    /// Enqueue a resize for delivery. Resizes while not live are dropped
    /// before any epoch is captured, mirroring Swift's early return.
    pub fn send_resize(&self, viewport: RemoteTerminalViewport) {
        if !self.is_resize_live() {
            return;
        }
        let queued_epoch = self.current_epoch();
        let plain_viewport = viewport;
        self.deliver_resize(plain_viewport, queued_epoch);
    }

    /// Delivery half of [`Self::send_input`]: drops the callback when the
    /// epoch advanced since it was enqueued, otherwise invokes the current
    /// input handler.
    fn deliver_input(&self, data: Vec<u8>, queued_epoch: RemoteTerminalCallbackEpoch) {
        let handler = {
            let state = self.state.lock().expect("relay state lock");
            if !state.epoch.accepts(queued_epoch) {
                return;
            }
            Arc::clone(&state.input_handler)
        };
        handler(data);
    }

    /// Delivery half of [`Self::send_resize`].
    fn deliver_resize(
        &self,
        viewport: RemoteTerminalViewport,
        queued_epoch: RemoteTerminalCallbackEpoch,
    ) {
        let handler = {
            let state = self.state.lock().expect("relay state lock");
            if !state.epoch.accepts(queued_epoch) {
                return;
            }
            Arc::clone(&state.resize_handler)
        };
        handler(viewport);
    }
}

/// Bytes that are injected only into the Controller's local VT parser. They
/// are never routed through the session's `send_input`, so resetting a
/// retained frame cannot write escape sequences to the Host PTY.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteTerminalLocalFeed {
    pub bytes: Vec<u8>,
}

impl RemoteTerminalLocalFeed {
    /// CAN aborts an unterminated OSC/DCS before RIS. A retention rebase may
    /// deliberately cut such a pathological control string to keep an
    /// always-on journal bounded, so ESC c alone could be swallowed.
    const RESET_PREFIX: &'static [u8] = &[0x18, 0x1B, b'c'];
    const BEGIN_SYNCHRONIZED_OUTPUT: &'static [u8] = b"\x1B[?2026h";
    const CLEAR_DISPLAY_AND_SCROLLBACK: &'static [u8] = b"\x1B[3J\x1B[2J\x1B[H";
    const END_SYNCHRONIZED_OUTPUT: &'static [u8] = b"\x1B[?2026l";

    fn assembled(payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(
            Self::RESET_PREFIX.len()
                + Self::BEGIN_SYNCHRONIZED_OUTPUT.len()
                + Self::CLEAR_DISPLAY_AND_SCROLLBACK.len()
                + payload.len()
                + Self::END_SYNCHRONIZED_OUTPUT.len(),
        );
        bytes.extend_from_slice(Self::RESET_PREFIX);
        bytes.extend_from_slice(Self::BEGIN_SYNCHRONIZED_OUTPUT);
        bytes.extend_from_slice(Self::CLEAR_DISPLAY_AND_SCROLLBACK);
        bytes.extend_from_slice(payload);
        bytes.extend_from_slice(Self::END_SYNCHRONIZED_OUTPUT);
        bytes
    }

    /// Standalone reset: RIS clears terminal modes; CSI 3J/2J/H clears the
    /// retained screen, scrollback, and cursor. The synchronized-output pair
    /// prevents an intermediate blank frame from presenting.
    pub fn reset_retained_state() -> Self {
        Self {
            bytes: Self::assembled(&[]),
        }
    }

    /// Atomic reset + replacement output. RIS must precede DEC 2026 because
    /// RIS itself resets synchronized-output mode.
    pub fn resetting_before_feeding(payload: &[u8]) -> Self {
        Self {
            bytes: Self::assembled(payload),
        }
    }
}

/// Plain events the rest of the app may care about. No Ghostty types.
/// Mirrors Swift's `GhosttyTerminalPaneDelegate` protocol.
pub trait GhosttyTerminalPaneDelegate {
    /// The surface's title changed.
    fn terminal_pane_did_change_title(&mut self, title: &str);
    /// The surface's child process exited (or the surface closed).
    fn terminal_pane_did_close(&mut self, process_alive: bool);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn viewport_fields_and_equality() {
        let a = RemoteTerminalViewport::new(80, 24, 640, 384, 8, 16);
        assert_eq!(a.columns, 80);
        assert_eq!(a.rows, 24);
        assert_eq!(a.width_pixels, 640);
        assert_eq!(a.height_pixels, 384);
        assert_eq!(a.cell_width_pixels, 8);
        assert_eq!(a.cell_height_pixels, 16);
        let b = RemoteTerminalViewport::new(80, 24, 640, 384, 8, 16);
        assert_eq!(a, b);
        let c = RemoteTerminalViewport::new(81, 24, 640, 384, 8, 16);
        assert_ne!(a, c);
    }

    #[test]
    fn epoch_starts_at_zero_and_advances() {
        let mut epoch = RemoteTerminalCallbackEpoch::new();
        assert_eq!(epoch.revision(), 0);
        epoch.advance();
        assert_eq!(epoch.revision(), 1);
        epoch.advance();
        assert_eq!(epoch.revision(), 2);
    }

    #[test]
    fn epoch_wraps_on_overflow() {
        // Mirrors Swift's `&+=`: wrapping, never trapping.
        let mut epoch = RemoteTerminalCallbackEpoch { revision: u64::MAX };
        epoch.advance();
        assert_eq!(epoch.revision(), 0);
    }

    #[test]
    fn epoch_accepts_only_current_revision() {
        let mut epoch = RemoteTerminalCallbackEpoch::new();
        let queued = epoch;
        assert!(epoch.accepts(queued));
        epoch.advance();
        assert!(!epoch.accepts(queued));
        assert!(epoch.accepts(epoch));
    }

    #[test]
    fn relay_delivers_input_and_resize() {
        let got_input: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
        let got_resize: Arc<Mutex<Vec<RemoteTerminalViewport>>> = Arc::new(Mutex::new(Vec::new()));
        let relay = RemoteTerminalCallbackRelay::new(
            {
                let got_input = Arc::clone(&got_input);
                move |data: Vec<u8>| got_input.lock().unwrap().push(data)
            },
            {
                let got_resize = Arc::clone(&got_resize);
                move |vp: RemoteTerminalViewport| got_resize.lock().unwrap().push(vp)
            },
        );
        relay.send_input(b"ls\r".to_vec());
        let vp = RemoteTerminalViewport::new(80, 24, 640, 384, 8, 16);
        relay.send_resize(vp);
        assert_eq!(got_input.lock().unwrap().as_slice(), &[b"ls\r".to_vec()]);
        assert_eq!(got_resize.lock().unwrap().as_slice(), &[vp]);
    }

    #[test]
    fn relay_drops_resize_while_not_live() {
        let calls = Arc::new(AtomicUsize::new(0));
        let relay = RemoteTerminalCallbackRelay::new(|_| {}, {
            let calls = Arc::clone(&calls);
            move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
            }
        });
        relay.set_resize_live(false);
        assert!(!relay.is_resize_live());
        relay.send_resize(RemoteTerminalViewport::new(80, 24, 640, 384, 8, 16));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        // A live re-present refits from real bounds: the next resize goes through.
        relay.set_resize_live(true);
        relay.send_resize(RemoteTerminalViewport::new(80, 24, 640, 384, 8, 16));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn relay_discards_input_enqueued_before_update() {
        let old_calls = Arc::new(AtomicUsize::new(0));
        let new_calls = Arc::new(AtomicUsize::new(0));
        let relay = RemoteTerminalCallbackRelay::new(
            {
                let old_calls = Arc::clone(&old_calls);
                move |_| {
                    old_calls.fetch_add(1, Ordering::SeqCst);
                }
            },
            |_| {},
        );
        // Capture the epoch the way send_input does before its delivery hop.
        let stale_epoch = relay.current_epoch();
        relay.update(
            {
                let new_calls = Arc::clone(&new_calls);
                move |_| {
                    new_calls.fetch_add(1, Ordering::SeqCst);
                }
            },
            |_| {},
        );
        // The stale delivery must not reach either handler.
        relay.deliver_input(b"stale".to_vec(), stale_epoch);
        assert_eq!(old_calls.load(Ordering::SeqCst), 0);
        assert_eq!(new_calls.load(Ordering::SeqCst), 0);
        // Fresh input goes to the new handler.
        relay.send_input(b"fresh".to_vec());
        assert_eq!(new_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn relay_discards_resize_enqueued_before_clear() {
        let calls = Arc::new(AtomicUsize::new(0));
        let relay = RemoteTerminalCallbackRelay::new(|_| {}, {
            let calls = Arc::clone(&calls);
            move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
            }
        });
        let stale_epoch = relay.current_epoch();
        relay.clear();
        relay.deliver_resize(
            RemoteTerminalViewport::new(80, 24, 640, 384, 8, 16),
            stale_epoch,
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        // After clear, sends hit the no-op handlers without panicking.
        relay.send_input(b"x".to_vec());
        relay.send_resize(RemoteTerminalViewport::new(80, 24, 640, 384, 8, 16));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn relay_update_advances_epoch() {
        let relay = RemoteTerminalCallbackRelay::new(|_| {}, |_| {});
        let before = relay.current_epoch();
        relay.update(|_| {}, |_| {});
        assert!(!before.accepts(relay.current_epoch()));
        assert_ne!(before.revision(), relay.current_epoch().revision());
    }

    #[test]
    fn local_feed_reset_bytes_are_exact() {
        let feed = RemoteTerminalLocalFeed::reset_retained_state();
        // CAN ESC c | ESC [ ? 2026 h | ESC [ 3 J ESC [ 2 J ESC [ H | ESC [ ? 2026 l
        let expected: Vec<u8> = [
            0x18, 0x1B, b'c', 0x1B, b'[', b'?', b'2', b'0', b'2', b'6', b'h', 0x1B, b'[', b'3',
            b'J', 0x1B, b'[', b'2', b'J', 0x1B, b'[', b'H', 0x1B, b'[', b'?', b'2', b'0', b'2',
            b'6', b'l',
        ]
        .to_vec();
        assert_eq!(feed.bytes, expected);
    }

    #[test]
    fn local_feed_reset_before_payload_ordering() {
        let payload = b"hello";
        let feed = RemoteTerminalLocalFeed::resetting_before_feeding(payload);
        let reset = RemoteTerminalLocalFeed::reset_retained_state();
        // Payload sits between the clear sequence and the end of synchronized output.
        let end_marker = b"\x1B[?2026l";
        let reset_end = reset.bytes.len() - end_marker.len();
        assert_eq!(&feed.bytes[..reset_end], &reset.bytes[..reset_end]);
        assert_eq!(&feed.bytes[reset_end..reset_end + payload.len()], payload);
        assert_eq!(&feed.bytes[reset_end + payload.len()..], end_marker);
        // RIS (ESC c) precedes the DEC 2026 synchronized-output begin marker.
        let ris_pos = feed
            .bytes
            .windows(2)
            .position(|w| w == [0x1B, b'c'])
            .expect("RIS present");
        let begin_pos = feed
            .bytes
            .windows(8)
            .position(|w| w == b"\x1B[?2026h")
            .expect("sync begin present");
        assert!(ris_pos < begin_pos);
    }

    #[test]
    fn delegate_trait_is_object_usable() {
        struct Recorder {
            titles: Vec<String>,
            closes: Vec<bool>,
        }
        impl GhosttyTerminalPaneDelegate for Recorder {
            fn terminal_pane_did_change_title(&mut self, title: &str) {
                self.titles.push(title.to_string());
            }
            fn terminal_pane_did_close(&mut self, process_alive: bool) {
                self.closes.push(process_alive);
            }
        }
        let mut r = Recorder {
            titles: Vec::new(),
            closes: Vec::new(),
        };
        r.terminal_pane_did_change_title("zsh");
        r.terminal_pane_did_close(false);
        assert_eq!(r.titles, ["zsh"]);
        assert_eq!(r.closes, [false]);
    }
}
