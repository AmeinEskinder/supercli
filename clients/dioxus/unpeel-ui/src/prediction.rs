//! Mosh-style predictive terminal I/O for high-latency transports.
//!
//! Ported from the Swift client's `RemoteTerminalPredictionEngine`,
//! `RemoteTerminalScrollPredictionEngine`, and
//! `RemoteTerminalScrollShiftDetector`
//! (`clients/ios/SupercliIOS/Sources/SupercliIOS/RemoteTerminalPrediction.swift`
//! and `RemoteTerminalScrollPrediction.swift`). These are pure state
//! machines: no I/O, no platform APIs, fully unit-testable.
//!
//! Two deliberate differences from the Swift originals:
//! - Time is `std::time::Instant`, passed in by the caller (the Swift API
//!   takes `now: Date` the same way), so tests control the clock.
//! - Swift indexes rows by `Character` (grapheme clusters); this port
//!   indexes by Unicode scalar (`char`). For ASCII terminal grids the two
//!   agree; a wide glyph earlier in a row can misread a cell, which the
//!   engines treat as a contradiction — the overlay only ever hides.

use std::time::{Duration, Instant};

/// Mosh-style predictive local echo for high-latency transports.
///
/// Over the relay a keystroke's echo pays two WAN traversals, so typing
/// reads as laggy even when the link is healthy. The engine tracks
/// printable keystrokes at the cell the caret reported, renders them
/// immediately as a provisional overlay, and reconciles against the
/// authoritative viewport once server bytes arrive.
///
/// Safety comes from a confidence gate, not from understanding the remote
/// program: predictions are invisible until one of them is CONFIRMED by the
/// real grid (the predicted character appeared at the predicted cell). A
/// context that never echoes recognizably — password prompts, vim normal
/// mode, menus, TUIs that park the caret elsewhere — never earns display,
/// and a contradiction or expiry drops the gate again. Wrong predictions
/// are therefore at worst briefly visible, never destructive: the overlay
/// touches no terminal state.
#[derive(Debug, Clone, Default)]
pub struct KeystrokePredictionEngine {
    pending: Vec<Prediction>,
    is_confident: bool,
}

/// One provisional keystroke: the character and the 0-based viewport cell
/// where its echo should appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prediction {
    pub character: char,
    pub row: i32,
    pub column: i32,
    pub sent_at: Instant,
}

impl KeystrokePredictionEngine {
    /// A prediction unconfirmed this long means echo is not coming back in
    /// recognizable form; drop everything and close the display gate.
    pub const EXPIRY: Duration = Duration::from_secs(2);

    /// Beyond this many unconfirmed keystrokes something is off (key repeat
    /// into a stalled link) — stop predicting rather than paint a phantom
    /// line.
    pub const MAXIMUM_PENDING: usize = 24;

    pub fn new() -> Self {
        Self::default()
    }

    /// Register a printable keystroke. `cursor` is the current caret cell
    /// (used only when nothing is pending — later keystrokes chain off the
    /// previous prediction); `None` means the caret is unknown, which makes
    /// prediction impossible.
    pub fn keystroke(
        &mut self,
        character: char,
        cursor: Option<(i32, i32)>,
        columns: i32,
        now: Instant,
    ) {
        if self.pending.len() >= Self::MAXIMUM_PENDING {
            self.clear_pending();
            return;
        }
        let anchor: (i32, i32) = if let Some(last) = self.pending.last() {
            (last.row, last.column + 1)
        } else if let Some(cursor) = cursor {
            cursor
        } else {
            self.clear_pending();
            return;
        };
        // Wrapping is the remote program's call (soft wrap, composer
        // reflow) — stop predicting at the line edge instead of guessing.
        if anchor.0 < 0 || anchor.1 < 0 || anchor.1 >= columns - 1 {
            self.clear_pending();
            return;
        }
        self.pending.push(Prediction {
            character,
            row: anchor.0,
            column: anchor.1,
            sent_at: now,
        });
    }

    /// A backspace erases the most recent provisional keystroke.
    pub fn backspace(&mut self) {
        self.pending.pop();
    }

    /// Anything non-printable (submit, arrows, escape sequences) moves the
    /// cursor in ways only the server knows; keep the earned confidence.
    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    /// Full reset for replays/rebase/session teardown.
    pub fn reset(&mut self) {
        self.pending.clear();
        self.is_confident = false;
    }

    /// Reconcile against the authoritative viewport after server bytes.
    /// Confirms predictions in order; a foreign character at a predicted
    /// cell is a contradiction and closes the gate; blank cells wait until
    /// [`Self::EXPIRY`].
    pub fn reconcile<S: AsRef<str>>(&mut self, rows: &[S], now: Instant) {
        loop {
            let first = match self.pending.first() {
                Some(p) => *p,
                None => return,
            };
            if now.saturating_duration_since(first.sent_at) > Self::EXPIRY {
                self.pending.clear();
                self.is_confident = false;
                return;
            }
            let cell = rows
                .get(first.row as usize)
                .and_then(|row| cell_character(row.as_ref(), first.column));
            match cell {
                None => return, // beyond current content: still blank, keep waiting
                Some(c) if c == first.character => {
                    self.pending.remove(0);
                    self.is_confident = true;
                }
                Some(' ') => return, // echo not painted yet, keep waiting
                Some(_) => {
                    // Something else landed where we predicted: wrong context.
                    self.pending.clear();
                    self.is_confident = false;
                    return;
                }
            }
        }
    }

    /// The provisional characters to draw, only while the gate is open.
    pub fn displayed_text(&self) -> Option<Vec<char>> {
        if !self.is_confident || self.pending.is_empty() {
            return None;
        }
        Some(self.pending.iter().map(|p| p.character).collect())
    }

    /// The oldest unconfirmed prediction (the overlay's anchor cell).
    pub fn anchor(&self) -> Option<Prediction> {
        self.pending.first().copied()
    }

    pub fn is_confident(&self) -> bool {
        self.is_confident
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

/// Character at a display column, assuming one column per `char`.
/// Wide glyphs (CJK, emoji) earlier in the row shift this mapping; the
/// resulting misread at worst reads as a contradiction, which only
/// hides the overlay.
fn cell_character(row: &str, column: i32) -> Option<char> {
    if column < 0 {
        return None;
    }
    row.chars().nth(column as usize)
}

/// Mosh-style predictive scrolling for remote-rendered TUIs.
///
/// Alternate-screen TUIs (Claude & co) own their scrolling: a flick becomes
/// wheel events, the Mac redraws, frames ride back. Over the relay every
/// visible movement therefore lags a full WAN round trip — the finger moves,
/// the content doesn't. This engine closes that gap by predicting the one
/// thing a wheel almost always means: the content shifts by the rows sent.
/// The view translates the already-rendered canvas in sync with the finger
/// and lets the incoming edge show background until truth arrives.
///
/// Safety mirrors [`KeystrokePredictionEngine`]: the prediction is a
/// *view-layer translation* that touches no terminal state, it displays only
/// behind a confidence gate earned by an observed wheel→redraw response, and
/// an unanswered gesture eases home and closes the gate — a TUI that
/// ignores wheels is never worse than today, just unimproved. Display is
/// additionally latency-gated per gesture: on a fast path (LAN Wi-Fi) the
/// whole-canvas translation visibly fights a TUI's pinned chrome for no
/// felt benefit, so the engine only translates when measured wheel→pixels
/// time says the user would otherwise be waiting on the network.
///
/// Reconciliation is by OBSERVED CONTENT SHIFT, not per-chunk counting: the
/// renderer measures how many rows the viewport actually moved when a chunk
/// committed and drains exactly that many predicted rows from the front of
/// the queue.
#[derive(Debug, Clone, Default)]
pub struct ScrollPredictionEngine {
    pending: Vec<PendingBatch>,
    /// Earned by any observed wheel→shift response, lost when an entire
    /// gesture goes unanswered. Tracking continues while closed so the
    /// next responsive gesture re-earns display with no visible risk.
    is_confident: bool,
    acked_this_gesture: bool,
    /// EWMA of send→answered-on-screen time, sampled at each drain from the
    /// oldest pending batch. Describes the path (LAN vs relay), so it
    /// survives gestures and resets with confidence.
    response_latency: Option<Duration>,
    /// Display decision latched at gesture start so the offset can never
    /// pop in or out mid-drag when confidence or the EWMA crosses over.
    displays_this_gesture: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingBatch {
    /// Signed rows: positive = wheel down (content moves up on screen).
    rows: i32,
    sent_at: Instant,
    /// Sent while the queue was empty: its send→answered time is pure
    /// path latency. Batches queued behind others measure queue wait
    /// too, which inflated the estimate on fast links whenever the
    /// shift detector missed a chunk.
    probe: bool,
}

impl ScrollPredictionEngine {
    /// No redraw this long after the oldest unacked send means the TUI is
    /// not answering wheels for this gesture.
    pub const RESPONSE_TIMEOUT: Duration = Duration::from_millis(450);

    /// Hard cap on how far prediction may run ahead of truth; beyond this
    /// the placeholder region dominates the viewport.
    pub const MAXIMUM_PENDING_ROWS: i32 = 20;

    /// The translation displays only when wheels take at least this long to
    /// come back as pixels.
    pub const DISPLAY_LATENCY_THRESHOLD: Duration = Duration::from_millis(180);

    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_confident(&self) -> bool {
        self.is_confident
    }

    pub fn response_latency(&self) -> Option<Duration> {
        self.response_latency
    }

    pub fn pending_rows(&self) -> i32 {
        self.pending.iter().map(|b| b.rows).sum()
    }

    /// Rows the canvas should translate by right now (negative y per down
    /// row — content follows the finger). Zero until the TUI has proven it
    /// answers wheels AND the path is slow enough for prediction to beat
    /// the real frames.
    pub fn offset_rows(&self) -> i32 {
        if self.displays_this_gesture {
            -self.pending_rows()
        } else {
            0
        }
    }

    pub fn begin_gesture(&mut self) {
        self.acked_this_gesture = false;
        self.displays_this_gesture = self.is_confident
            && self.response_latency.unwrap_or_default() >= Self::DISPLAY_LATENCY_THRESHOLD;
    }

    pub fn wheel_sent(&mut self, rows: i32, now: Instant) {
        if rows == 0 {
            return;
        }
        // Stop growing at the cap: the steps still went to the host, so
        // later frames simply drain the tracked portion.
        if (self.pending_rows() + rows).abs() > Self::MAXIMUM_PENDING_ROWS {
            return;
        }
        let probe = self.pending.is_empty();
        self.pending.push(PendingBatch {
            rows,
            sent_at: now,
            probe,
        });
    }

    /// A committed feed moved the viewport content by `rows` (positive =
    /// content moved up, the wheel-down direction). That movement is the TUI
    /// answering the oldest predicted wheels: drain exactly that many rows
    /// from the front of the queue, splitting a partially-answered batch.
    /// Movement opposite the queued direction is not an answer (autoscroll
    /// or a repaint the finger didn't cause) and drains nothing. If the TUI
    /// moved further than predicted, the drain clamps at zero — real frames
    /// already carry the extra motion, so translating past truth would
    /// overshoot. `now` samples the send→on-screen latency that decides
    /// whether future gestures display at all.
    pub fn content_shifted(&mut self, rows: i32, now: Instant) {
        let oldest = match self.pending.first() {
            Some(b) => *b,
            None => return,
        };
        if rows == 0 {
            return;
        }
        let mut remaining = rows;
        while remaining != 0 {
            let first = match self.pending.first() {
                Some(b) => b,
                None => break,
            };
            if (first.rows > 0) != (remaining > 0) {
                break;
            }
            if first.rows.abs() <= remaining.abs() {
                remaining -= first.rows;
                self.pending.remove(0);
            } else {
                self.pending[0].rows -= remaining;
                remaining = 0;
            }
        }
        if remaining == rows {
            return;
        }
        if oldest.probe {
            let sample = now.saturating_duration_since(oldest.sent_at);
            self.response_latency = Some(match self.response_latency {
                Some(latency) => latency.mul_f64(0.7) + sample.mul_f64(0.3),
                None => sample,
            });
            // A split probe stays queued; it has been sampled and must not
            // report an ever-older age on its next partial answer.
            if self.pending.first().map(|b| b.sent_at) == Some(oldest.sent_at) {
                self.pending[0].probe = false;
            }
        }
        self.acked_this_gesture = true;
        self.is_confident = true;
    }

    /// True when the oldest prediction expired unanswered — the caller
    /// eases the translation home. The gate closes only if the whole
    /// gesture produced no ack (coalesced trailing frames must not punish
    /// a TUI that demonstrably responded).
    pub fn expire_if_unanswered(&mut self, now: Instant) -> bool {
        let oldest = match self.pending.first() {
            Some(b) => *b,
            None => return false,
        };
        if now.saturating_duration_since(oldest.sent_at) < Self::RESPONSE_TIMEOUT {
            return false;
        }
        self.pending.clear();
        if !self.acked_this_gesture {
            self.is_confident = false;
        }
        true
    }

    /// Gesture cancelled / session detached: drop the translation but keep
    /// earned confidence (it describes the TUI, not the gesture).
    pub fn cancel(&mut self) {
        self.pending.clear();
    }

    /// Full reset (new session / screen replaced): confidence and the
    /// path-latency estimate must be re-earned against whatever now owns
    /// the terminal.
    pub fn reset_confidence(&mut self) {
        self.pending.clear();
        self.is_confident = false;
        self.acked_this_gesture = false;
        self.response_latency = None;
        self.displays_this_gesture = false;
    }
}

/// Measures how many rows the rendered viewport moved between two reads —
/// the reconciliation signal for
/// [`ScrollPredictionEngine::content_shifted`]. Pure text alignment over the
/// two viewport snapshots: for each candidate shift the score is the number
/// of identical non-blank rows, and the smallest shift wins ties, so a
/// stationary screen (or one repainted beyond recognition) reads as zero
/// rather than guessing. Positive = content moved up on screen (the
/// wheel-down direction), matching the engine's convention.
///
/// TUIs with fixed chrome (a pinned composer and footer while the transcript
/// scrolls) still resolve correctly: the scrolled region outnumbers the
/// pinned rows, so the true shift outscores zero. When it doesn't,
/// under-reporting is safe — the leftover translation drains via the
/// expiry ease-home instead of a wrong jump.
pub fn detect_scroll_shift(before: &str, after: &str, max_shift: usize) -> i32 {
    /// Fewer than this many agreeing non-blank rows means the screen
    /// changed too much to trust any alignment, including zero.
    const MINIMUM_MATCHES: i32 = 2;

    if max_shift == 0 {
        return 0;
    }
    // Viewport text split into rows with trailing spaces dropped, so cell
    // padding never breaks an otherwise identical row.
    let before_rows: Vec<&str> = before
        .split('\n')
        .map(|l| l.trim_end_matches(' '))
        .collect();
    let after_rows: Vec<&str> = after.split('\n').map(|l| l.trim_end_matches(' ')).collect();
    let count = before_rows.len().min(after_rows.len());
    if count == 0 {
        return 0;
    }

    let mut best_shift: i32 = 0;
    let mut best_score: i32 = -1;
    for magnitude in 0..=max_shift {
        let candidates: &[i32] = if magnitude == 0 {
            &[0]
        } else {
            &[magnitude as i32, -(magnitude as i32)]
        };
        for &candidate in candidates {
            let mut score: i32 = 0;
            for (index, &row) in after_rows.iter().take(count).enumerate() {
                let source = index as i32 + candidate;
                if source < 0 || source as usize >= before_rows.len() {
                    continue;
                }
                if row.is_empty() || row != before_rows[source as usize] {
                    continue;
                }
                score += 1;
            }
            // Strictly greater: |candidate| grows through the loop, so
            // ties resolve to the smallest movement.
            if score > best_score {
                best_score = score;
                best_shift = candidate;
            }
        }
    }
    if best_score < MINIMUM_MATCHES {
        0
    } else {
        best_shift
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    // --- Keystroke engine ---

    #[test]
    fn predictions_stay_hidden_until_confirmed() {
        let start = t0();
        let mut engine = KeystrokePredictionEngine::new();
        engine.keystroke('h', Some((0, 0)), 80, start);
        // Gate closed: nothing displayed even though a prediction is pending.
        assert_eq!(engine.displayed_text(), None);
        assert!(!engine.is_confident());

        // The echo arrives: gate opens.
        engine.reconcile(&["h"], start + Duration::from_millis(50));
        assert!(engine.is_confident());
        assert_eq!(engine.pending_count(), 0);

        // Now predictions display.
        engine.keystroke('i', Some((0, 1)), 80, start + Duration::from_millis(60));
        assert_eq!(engine.displayed_text(), Some(vec!['i']));
        assert_eq!(
            engine.anchor(),
            Some(Prediction {
                character: 'i',
                row: 0,
                column: 1,
                sent_at: start + Duration::from_millis(60),
            })
        );
    }

    #[test]
    fn contradiction_closes_the_gate() {
        let start = t0();
        let mut engine = KeystrokePredictionEngine::new();
        engine.keystroke('h', Some((0, 0)), 80, start);
        engine.reconcile(&["h"], start + Duration::from_millis(50));
        assert!(engine.is_confident());

        // Predicted 'x' at (0,1), but 'y' landed there: wrong context.
        engine.keystroke('x', Some((0, 1)), 80, start + Duration::from_millis(60));
        engine.reconcile(&["hy"], start + Duration::from_millis(100));
        assert!(!engine.is_confident());
        assert_eq!(engine.pending_count(), 0);
        assert_eq!(engine.displayed_text(), None);
    }

    #[test]
    fn blank_cells_wait_until_expiry() {
        let start = t0();
        let mut engine = KeystrokePredictionEngine::new();
        engine.keystroke('h', Some((0, 0)), 80, start);
        // Echo not painted yet: blank cell waits, nothing confirmed.
        engine.reconcile(&[" "], start + Duration::from_millis(100));
        assert_eq!(engine.pending_count(), 1);
        assert!(!engine.is_confident());
        // Past expiry: everything drops, gate closes.
        engine.reconcile(&[" "], start + Duration::from_secs(3));
        assert_eq!(engine.pending_count(), 0);
        assert!(!engine.is_confident());
    }

    #[test]
    fn backspace_drops_last_prediction() {
        let start = t0();
        let mut engine = KeystrokePredictionEngine::new();
        engine.keystroke('h', Some((0, 0)), 80, start);
        engine.keystroke('i', None, 80, start); // chains off the previous prediction
        assert_eq!(engine.pending_count(), 2);
        engine.backspace();
        assert_eq!(engine.pending_count(), 1);
        assert_eq!(engine.anchor().unwrap().character, 'h');
        engine.backspace();
        engine.backspace(); // empty: no-op, not a panic
        assert_eq!(engine.pending_count(), 0);
    }

    #[test]
    fn clear_pending_keeps_earned_confidence() {
        let start = t0();
        let mut engine = KeystrokePredictionEngine::new();
        engine.keystroke('h', Some((0, 0)), 80, start);
        engine.reconcile(&["h"], start + Duration::from_millis(50));
        assert!(engine.is_confident());
        // An arrow key: drop the provisional text, keep the gate.
        engine.keystroke('i', Some((0, 1)), 80, start + Duration::from_millis(60));
        engine.clear_pending();
        assert_eq!(engine.pending_count(), 0);
        assert!(engine.is_confident());
    }

    #[test]
    fn unknown_cursor_and_line_edge_predict_nothing() {
        let start = t0();
        let mut engine = KeystrokePredictionEngine::new();
        engine.keystroke('h', None, 80, start);
        assert_eq!(engine.pending_count(), 0);
        // At the last column the remote program owns wrapping.
        engine.keystroke('h', Some((0, 79)), 80, start);
        assert_eq!(engine.pending_count(), 0);
        assert_eq!(engine.displayed_text(), None);
    }

    #[test]
    fn maximum_pending_stops_predicting() {
        let start = t0();
        let mut engine = KeystrokePredictionEngine::new();
        for _ in 0..KeystrokePredictionEngine::MAXIMUM_PENDING {
            engine.keystroke('a', Some((0, 0)), 10_000, start);
        }
        assert_eq!(
            engine.pending_count(),
            KeystrokePredictionEngine::MAXIMUM_PENDING
        );
        // One more: stop rather than paint a phantom line.
        engine.keystroke('a', Some((0, 0)), 10_000, start);
        assert_eq!(engine.pending_count(), 0);
    }

    #[test]
    fn reset_clears_everything() {
        let start = t0();
        let mut engine = KeystrokePredictionEngine::new();
        engine.keystroke('h', Some((0, 0)), 80, start);
        engine.reconcile(&["h"], start + Duration::from_millis(50));
        engine.reset();
        assert!(!engine.is_confident());
        assert_eq!(engine.pending_count(), 0);
    }

    // --- Scroll engine ---

    /// Drive the engine to confidence with a wheel answered after `latency`.
    fn earn_confidence(engine: &mut ScrollPredictionEngine, start: Instant, latency: Duration) {
        engine.begin_gesture();
        engine.wheel_sent(3, start);
        engine.content_shifted(3, start + latency);
        assert!(engine.is_confident());
    }

    #[test]
    fn no_offset_until_proven() {
        let start = t0();
        let mut engine = ScrollPredictionEngine::new();
        engine.begin_gesture();
        engine.wheel_sent(3, start);
        // Tracked, but the TUI never proved it answers wheels.
        assert_eq!(engine.pending_rows(), 3);
        assert_eq!(engine.offset_rows(), 0);
    }

    #[test]
    fn slow_path_translates_after_answered_wheel() {
        let start = t0();
        let mut engine = ScrollPredictionEngine::new();
        earn_confidence(&mut engine, start, Duration::from_millis(300));
        assert!(engine.response_latency().unwrap() >= Duration::from_millis(180));

        // A new gesture latches display on.
        engine.begin_gesture();
        engine.wheel_sent(2, start + Duration::from_secs(1));
        assert_eq!(engine.offset_rows(), -2);
    }

    #[test]
    fn fast_path_tracks_but_never_translates() {
        let start = t0();
        let mut engine = ScrollPredictionEngine::new();
        earn_confidence(&mut engine, start, Duration::from_millis(50));
        assert!(engine.is_confident());

        engine.begin_gesture();
        engine.wheel_sent(2, start + Duration::from_secs(1));
        // Confident about the TUI, but the link is fast: no translation.
        assert_eq!(engine.offset_rows(), 0);
        assert_eq!(engine.pending_rows(), 2);
    }

    #[test]
    fn unanswered_gesture_expires_and_closes_gate() {
        let start = t0();
        let mut engine = ScrollPredictionEngine::new();
        earn_confidence(&mut engine, start, Duration::from_millis(300));

        engine.begin_gesture();
        engine.wheel_sent(2, start + Duration::from_secs(1));
        assert!(engine.expire_if_unanswered(start + Duration::from_secs(2)));
        assert!(!engine.is_confident());
        assert_eq!(engine.pending_rows(), 0);
        assert_eq!(engine.offset_rows(), 0);
    }

    #[test]
    fn answered_gesture_keeps_confidence_through_expiry() {
        let start = t0();
        let mut engine = ScrollPredictionEngine::new();
        earn_confidence(&mut engine, start, Duration::from_millis(300));

        engine.begin_gesture();
        let g = start + Duration::from_secs(1);
        engine.wheel_sent(4, g);
        // Partially answered: the gesture proved responsive.
        engine.content_shifted(1, g + Duration::from_millis(300));
        assert!(engine.expire_if_unanswered(g + Duration::from_secs(1)));
        assert!(engine.is_confident(), "an acked gesture keeps the gate");
        assert_eq!(engine.pending_rows(), 0);
    }

    #[test]
    fn opposite_shift_drains_nothing() {
        let start = t0();
        let mut engine = ScrollPredictionEngine::new();
        engine.begin_gesture();
        engine.wheel_sent(3, start);
        // Content moved the other way: autoscroll, not an answer.
        engine.content_shifted(-2, start + Duration::from_millis(300));
        assert_eq!(engine.pending_rows(), 3);
        assert!(!engine.is_confident());
    }

    #[test]
    fn drain_clamps_at_zero_and_splits_batches() {
        let start = t0();
        let mut engine = ScrollPredictionEngine::new();
        engine.begin_gesture();
        engine.wheel_sent(2, start);
        // The TUI moved further than predicted: clamp, don't overshoot.
        engine.content_shifted(5, start + Duration::from_millis(300));
        assert_eq!(engine.pending_rows(), 0);
        assert!(engine.is_confident());

        engine.begin_gesture();
        let g = start + Duration::from_secs(1);
        engine.wheel_sent(5, g);
        engine.content_shifted(2, g + Duration::from_millis(300));
        assert_eq!(engine.pending_rows(), 3);
    }

    #[test]
    fn wheel_cap_stops_growing() {
        let start = t0();
        let mut engine = ScrollPredictionEngine::new();
        engine.begin_gesture();
        for _ in 0..10 {
            engine.wheel_sent(3, start);
        }
        assert!(engine.pending_rows() <= ScrollPredictionEngine::MAXIMUM_PENDING_ROWS);
    }

    #[test]
    fn cancel_keeps_confidence_but_drops_translation() {
        let start = t0();
        let mut engine = ScrollPredictionEngine::new();
        earn_confidence(&mut engine, start, Duration::from_millis(300));
        engine.begin_gesture();
        engine.wheel_sent(2, start + Duration::from_secs(1));
        engine.cancel();
        assert_eq!(engine.pending_rows(), 0);
        assert!(engine.is_confident());
    }

    #[test]
    fn reset_confidence_forgets_path_and_tui() {
        let start = t0();
        let mut engine = ScrollPredictionEngine::new();
        earn_confidence(&mut engine, start, Duration::from_millis(300));
        engine.reset_confidence();
        assert!(!engine.is_confident());
        assert_eq!(engine.response_latency(), None);
        engine.begin_gesture();
        engine.wheel_sent(2, start + Duration::from_secs(1));
        assert_eq!(engine.offset_rows(), 0);
    }

    #[test]
    fn latency_ewma_blends_samples() {
        let start = t0();
        let mut engine = ScrollPredictionEngine::new();
        engine.begin_gesture();
        engine.wheel_sent(3, start);
        engine.content_shifted(3, start + Duration::from_millis(100));
        // Second sample on a fresh probe batch: EWMA 0.7*100 + 0.3*400.
        engine.begin_gesture();
        let g = start + Duration::from_secs(1);
        engine.wheel_sent(3, g);
        engine.content_shifted(3, g + Duration::from_millis(400));
        let latency = engine.response_latency().unwrap();
        let expected =
            Duration::from_millis(100).mul_f64(0.7) + Duration::from_millis(400).mul_f64(0.3);
        assert!(
            latency.abs_diff(expected) < Duration::from_millis(1),
            "latency {latency:?} != expected {expected:?}"
        );
    }

    // --- Shift detector ---

    #[test]
    fn detects_upward_shift() {
        let before = "aaa\nbbb\nccc";
        let after = "bbb\nccc\nddd";
        assert_eq!(detect_scroll_shift(before, after, 4), 1);
    }

    #[test]
    fn stationary_screen_reads_zero() {
        let text = "aaa\nbbb\nccc";
        assert_eq!(detect_scroll_shift(text, text, 4), 0);
    }

    #[test]
    fn repainted_beyond_recognition_reads_zero() {
        let before = "aaa\nbbb\nccc";
        let after = "xxx\nyyy\nzzz";
        assert_eq!(detect_scroll_shift(before, after, 4), 0);
    }

    #[test]
    fn scrolled_region_outscores_pinned_chrome() {
        // Claude-style: pinned header/footer, scrolling transcript. The
        // scrolled rows must outnumber the chrome, else the tie-break
        // (smallest shift wins) legitimately reads zero.
        let before = "HDR\n111\n222\n333\n444\nFTR";
        let after = "HDR\n222\n333\n444\n555\nFTR";
        assert_eq!(detect_scroll_shift(before, after, 4), 1);
    }

    #[test]
    fn trailing_cell_padding_is_ignored() {
        let before = "aaa   \nbbb  \nccc ";
        let after = "bbb\nccc\nddd";
        assert_eq!(detect_scroll_shift(before, after, 4), 1);
    }

    #[test]
    fn blank_rows_do_not_score() {
        let before = "\n\n\n";
        let after = "\n\n\n";
        assert_eq!(detect_scroll_shift(before, after, 4), 0);
    }

    #[test]
    fn zero_max_shift_returns_zero() {
        assert_eq!(detect_scroll_shift("a\nb", "b\na", 0), 0);
    }
}
