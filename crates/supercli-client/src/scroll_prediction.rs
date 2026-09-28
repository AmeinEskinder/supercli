//! Mosh-style predictive scrolling for remote-rendered TUIs.
//!
//! Port of `RemoteTerminalScrollPredictionEngine` and
//! `RemoteTerminalScrollShiftDetector` in `RemoteTerminalScrollPrediction.swift`
//! (`ios/SupercliIOS`). The SwiftUI state/modifier types are not ported —
//! only the engines, which are UI-agnostic.
//!
//! Alternate-screen TUIs own their scrolling: a flick becomes wheel events,
//! the host redraws, frames ride back. Over the relay every visible movement
//! therefore lags a full WAN round trip. This engine closes that gap by
//! predicting the one thing a wheel almost always means: the content shifts
//! by the rows sent.
//!
//! Safety: the prediction is a *view-layer translation* that touches no
//! terminal state, it displays only behind a confidence gate earned by an
//! observed wheel→redraw response, and an unanswered gesture eases home and
//! closes the gate. Display is additionally latency-gated per gesture: on a
//! fast path the whole-canvas translation visibly fights a TUI's pinned
//! chrome for no felt benefit.
//!
//! Reconciliation is by OBSERVED CONTENT SHIFT, not per-chunk counting.

/// One batch of wheel rows sent to the host. Timestamps are seconds on an
/// arbitrary monotonic clock.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingBatch {
    /// Signed rows: positive = wheel down (content moves up on screen).
    pub rows: i32,
    pub sent_at: f64,
    /// Sent while the queue was empty: its send→answered time is pure path
    /// latency. Batches queued behind others measure queue wait too.
    pub probe: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RemoteTerminalScrollPredictionEngine {
    pending: Vec<PendingBatch>,
    /// Earned by any observed wheel→shift response, lost when an entire
    /// gesture goes unanswered.
    is_confident: bool,
    acked_this_gesture: bool,
    /// EWMA of send→answered-on-screen time, sampled at each drain from the
    /// oldest pending batch. Describes the path (LAN vs relay).
    response_latency: Option<f64>,
    /// Display decision latched at gesture start so the offset can never
    /// pop in or out mid-drag.
    displays_this_gesture: bool,
}

impl RemoteTerminalScrollPredictionEngine {
    /// No redraw this long after the oldest unacked send means the TUI is
    /// not answering wheels for this gesture.
    pub const RESPONSE_TIMEOUT: f64 = 0.45;

    /// Hard cap on how far prediction may run ahead of truth.
    pub const MAXIMUM_PENDING_ROWS: i32 = 20;

    /// The translation displays only when wheels take at least this long to
    /// come back as pixels.
    pub const DISPLAY_LATENCY_THRESHOLD: f64 = 0.18;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending(&self) -> &[PendingBatch] {
        &self.pending
    }

    pub fn is_confident(&self) -> bool {
        self.is_confident
    }

    pub fn response_latency(&self) -> Option<f64> {
        self.response_latency
    }

    pub fn pending_rows(&self) -> i32 {
        self.pending.iter().map(|b| b.rows).sum()
    }

    /// Rows the canvas should translate by right now (negative y per down
    /// row — content follows the finger). Zero until the TUI has proven it
    /// answers wheels AND the path is slow enough.
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
            && self.response_latency.unwrap_or(0.0) >= Self::DISPLAY_LATENCY_THRESHOLD;
    }

    pub fn wheel_sent(&mut self, rows: i32, now: f64) {
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
    /// content moved up, the wheel-down direction). Drains exactly that many
    /// rows from the front of the queue, splitting a partially-answered
    /// batch. Movement opposite the queued direction drains nothing. If the
    /// TUI moved further than predicted, the drain clamps at zero.
    pub fn content_shifted(&mut self, rows: i32, now: f64) {
        if rows == 0 || self.pending.is_empty() {
            return;
        }
        let oldest = self.pending[0].clone();
        let mut remaining = rows;
        while remaining != 0 {
            let first = match self.pending.first() {
                Some(b) => b.clone(),
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
            let sample = (now - oldest.sent_at).max(0.0);
            self.response_latency = Some(match self.response_latency {
                Some(prev) => prev * 0.7 + sample * 0.3,
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
    /// gesture produced no ack.
    pub fn expire_if_unanswered(&mut self, now: f64) -> bool {
        let oldest = match self.pending.first() {
            Some(b) => b.clone(),
            None => return false,
        };
        if now - oldest.sent_at < Self::RESPONSE_TIMEOUT {
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
    /// path-latency estimate must be re-earned.
    pub fn reset_confidence(&mut self) {
        self.pending.clear();
        self.is_confident = false;
        self.acked_this_gesture = false;
        self.response_latency = None;
        self.displays_this_gesture = false;
    }
}

/// Measures how many rows the rendered viewport moved between two reads —
/// the reconciliation signal for `content_shifted`. Pure text alignment over
/// the two viewport snapshots: for each candidate shift the score is the
/// number of identical non-blank rows, and the smallest shift wins ties.
pub struct RemoteTerminalScrollShiftDetector;

impl RemoteTerminalScrollShiftDetector {
    /// Fewer than this many agreeing non-blank rows means the screen
    /// changed too much to trust any alignment, including zero.
    pub const MINIMUM_MATCHES: i32 = 2;

    /// Positive = content moved up on screen (the wheel-down direction).
    pub fn shift(before: &str, after: &str, max_shift: usize) -> i32 {
        if max_shift == 0 {
            return 0;
        }
        let before_rows = Self::rows(before);
        let after_rows = Self::rows(after);
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
                for (index, row) in after_rows.iter().enumerate().take(count) {
                    let source = index as i32 + candidate;
                    if source < 0 || source as usize >= before_rows.len() {
                        continue;
                    }
                    if !row.is_empty() && *row == before_rows[source as usize] {
                        score += 1;
                    }
                }
                // Strictly greater: |candidate| grows through the loop, so
                // ties resolve to the smallest movement.
                if score > best_score {
                    best_score = score;
                    best_shift = candidate;
                }
            }
        }
        if best_score < Self::MINIMUM_MATCHES {
            return 0;
        }
        best_shift
    }

    /// Viewport text split into rows with trailing spaces dropped, so
    /// cell padding never breaks an otherwise identical row.
    fn rows(text: &str) -> Vec<String> {
        text.split('\n')
            .map(|line| line.trim_end_matches(' ').to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: f64 = 1_000_000.0;

    /// One answered slow-path gesture: confident, EWMA well above the
    /// display threshold, queue drained.
    fn earn_slow_confidence(engine: &mut RemoteTerminalScrollPredictionEngine, at: f64) {
        engine.begin_gesture();
        engine.wheel_sent(1, at);
        engine.content_shifted(1, at + 0.3);
    }

    #[test]
    fn fast_path_tracks_but_never_displays() {
        let mut engine = RemoteTerminalScrollPredictionEngine::new();
        engine.begin_gesture();
        engine.wheel_sent(2, START);
        engine.content_shifted(2, START + 0.05);
        assert!(engine.is_confident());
        engine.begin_gesture();
        engine.wheel_sent(4, START + 1.0);
        assert_eq!(engine.pending_rows(), 4);
        assert_eq!(engine.offset_rows(), 0);
    }

    #[test]
    fn slow_path_displays_from_the_next_gesture() {
        let mut engine = RemoteTerminalScrollPredictionEngine::new();
        earn_slow_confidence(&mut engine, START);
        engine.begin_gesture();
        engine.wheel_sent(3, START + 1.0);
        assert_eq!(engine.offset_rows(), -3);
    }

    #[test]
    fn mid_gesture_confidence_does_not_display_this_gesture() {
        let mut engine = RemoteTerminalScrollPredictionEngine::new();
        engine.begin_gesture();
        engine.wheel_sent(5, START);
        engine.content_shifted(2, START + 0.3);
        assert!(engine.is_confident());
        assert_eq!(engine.pending_rows(), 3);
        assert_eq!(engine.offset_rows(), 0);
    }

    #[test]
    fn queue_wait_does_not_inflate_the_latency_estimate() {
        let mut engine = RemoteTerminalScrollPredictionEngine::new();
        engine.begin_gesture();
        engine.wheel_sent(1, START); // probe: queue was empty
        engine.wheel_sent(1, START + 0.02); // queued
        engine.content_shifted(1, START + 0.05);
        engine.content_shifted(1, START + 0.6);
        let latency = engine.response_latency().unwrap_or(-1.0);
        assert!(
            (latency - 0.05).abs() < 0.001,
            "latency was {latency}, expected ~0.05"
        );
        engine.begin_gesture();
        engine.wheel_sent(4, START + 1.0);
        assert_eq!(
            engine.offset_rows(),
            0,
            "queue-wait noise must never flip the gate on"
        );
    }

    #[test]
    fn shift_drains_rows_across_batches_with_partial_split() {
        let mut engine = RemoteTerminalScrollPredictionEngine::new();
        earn_slow_confidence(&mut engine, START);
        engine.begin_gesture();
        engine.wheel_sent(3, START + 1.0);
        engine.wheel_sent(2, START + 1.1);
        assert_eq!(engine.offset_rows(), -5);
        engine.content_shifted(4, START + 1.4);
        assert_eq!(engine.offset_rows(), -1);
        engine.content_shifted(1, START + 1.5);
        assert_eq!(engine.offset_rows(), 0);
    }

    #[test]
    fn unshifted_chunk_drains_nothing() {
        let mut engine = RemoteTerminalScrollPredictionEngine::new();
        earn_slow_confidence(&mut engine, START);
        engine.begin_gesture();
        engine.wheel_sent(4, START + 1.0);
        engine.content_shifted(0, START + 1.2);
        assert_eq!(engine.offset_rows(), -4);
    }

    #[test]
    fn opposite_direction_shift_is_not_an_answer() {
        let mut engine = RemoteTerminalScrollPredictionEngine::new();
        earn_slow_confidence(&mut engine, START);
        engine.begin_gesture();
        engine.wheel_sent(4, START + 1.0);
        engine.content_shifted(-3, START + 1.2);
        assert_eq!(
            engine.offset_rows(),
            -4,
            "autoscroll opposite the wheels drains nothing"
        );
        let late = START + 1.3 + RemoteTerminalScrollPredictionEngine::RESPONSE_TIMEOUT;
        assert!(engine.expire_if_unanswered(late));
        assert!(
            !engine.is_confident(),
            "an unanswered gesture still closes the gate"
        );
    }

    #[test]
    fn over_shift_clamps_at_zero_instead_of_flipping_sign() {
        let mut engine = RemoteTerminalScrollPredictionEngine::new();
        earn_slow_confidence(&mut engine, START);
        engine.begin_gesture();
        engine.wheel_sent(2, START + 1.0);
        engine.content_shifted(6, START + 1.3);
        assert_eq!(engine.offset_rows(), 0);
    }

    #[test]
    fn pending_rows_are_capped_not_unbounded() {
        let mut engine = RemoteTerminalScrollPredictionEngine::new();
        engine.begin_gesture();
        engine.wheel_sent(
            RemoteTerminalScrollPredictionEngine::MAXIMUM_PENDING_ROWS,
            START,
        );
        engine.wheel_sent(5, START);
        assert_eq!(
            engine.pending_rows(),
            RemoteTerminalScrollPredictionEngine::MAXIMUM_PENDING_ROWS,
            "steps beyond the cap are sent but not predicted"
        );
    }

    #[test]
    fn unanswered_gesture_expires_and_closes_the_gate() {
        let mut engine = RemoteTerminalScrollPredictionEngine::new();
        earn_slow_confidence(&mut engine, START);
        engine.begin_gesture();
        engine.wheel_sent(3, START + 1.0);
        let late = START + 1.1 + RemoteTerminalScrollPredictionEngine::RESPONSE_TIMEOUT;
        assert!(engine.expire_if_unanswered(late));
        assert_eq!(engine.pending_rows(), 0);
        assert!(
            !engine.is_confident(),
            "a fully unanswered gesture closes the gate"
        );
    }

    #[test]
    fn expiry_after_an_ack_keeps_the_gate() {
        let mut engine = RemoteTerminalScrollPredictionEngine::new();
        earn_slow_confidence(&mut engine, START);
        engine.begin_gesture();
        engine.wheel_sent(2, START + 1.0);
        engine.wheel_sent(2, START + 1.05);
        engine.content_shifted(2, START + 1.3);
        let late = START + 1.1 + RemoteTerminalScrollPredictionEngine::RESPONSE_TIMEOUT;
        assert!(engine.expire_if_unanswered(late));
        assert_eq!(engine.pending_rows(), 0);
        assert!(
            engine.is_confident(),
            "coalesced trailing frames must not punish a responsive TUI"
        );
    }

    #[test]
    fn early_expiry_probe_does_nothing() {
        let mut engine = RemoteTerminalScrollPredictionEngine::new();
        engine.begin_gesture();
        engine.wheel_sent(2, START);
        assert!(!engine.expire_if_unanswered(START + 0.2));
        assert_eq!(engine.pending_rows(), 2);
    }

    #[test]
    fn cancel_keeps_confidence_reset_drops_everything() {
        let mut engine = RemoteTerminalScrollPredictionEngine::new();
        earn_slow_confidence(&mut engine, START);
        engine.begin_gesture();
        engine.wheel_sent(2, START + 1.0);
        engine.cancel();
        assert_eq!(engine.pending_rows(), 0);
        assert!(engine.is_confident());
        assert!(
            engine.response_latency().is_some(),
            "cancel keeps the path estimate"
        );
        engine.reset_confidence();
        assert!(!engine.is_confident());
        assert!(
            engine.response_latency().is_none(),
            "a replaced screen re-earns the path estimate"
        );
        engine.begin_gesture();
        engine.wheel_sent(2, START + 2.0);
        assert_eq!(engine.offset_rows(), 0);
    }

    // --- Shift detector tests ---

    fn screen(lines: &[String]) -> String {
        lines.join("\n")
    }

    fn transcript() -> Vec<String> {
        (0..20)
            .map(|i| format!("line {i} of the transcript body"))
            .collect()
    }

    #[test]
    fn detects_downward_scroll_shift() {
        let t = transcript();
        let before = screen(&t);
        let mut after_lines = t[3..].to_vec();
        after_lines.extend([
            "new 0".to_string(),
            "new 1".to_string(),
            "new 2".to_string(),
        ]);
        let after = screen(&after_lines);
        assert_eq!(
            RemoteTerminalScrollShiftDetector::shift(&before, &after, 10),
            3
        );
    }

    #[test]
    fn detects_upward_scroll_shift() {
        let t = transcript();
        let before = screen(&t[4..].to_vec());
        let after = screen(&t);
        assert_eq!(
            RemoteTerminalScrollShiftDetector::shift(&before, &after, 10),
            -4
        );
    }

    #[test]
    fn stationary_screen_with_changed_tail_is_zero() {
        let t = transcript();
        let mut lines = t.clone();
        lines[18] = "streamed replacement A".to_string();
        lines[19] = "streamed replacement B".to_string();
        assert_eq!(
            RemoteTerminalScrollShiftDetector::shift(&screen(&t), &screen(&lines), 10),
            0
        );
    }

    #[test]
    fn pinned_chrome_does_not_mask_the_scrolled_region() {
        let t = transcript();
        let chrome = [
            "╭── composer ──╮".to_string(),
            "│ >            │".to_string(),
            "╰──────────────╯".to_string(),
            "? for shortcuts".to_string(),
        ];
        let before = screen(&[t.clone(), chrome.to_vec()].concat());
        let mut after_tail = t[2..].to_vec();
        after_tail.extend(["tail 0".to_string(), "tail 1".to_string()]);
        after_tail.extend(chrome.to_vec());
        let after = screen(&after_tail);
        assert_eq!(
            RemoteTerminalScrollShiftDetector::shift(&before, &after, 10),
            2
        );
    }

    #[test]
    fn unrecognizable_repaint_is_zero() {
        let t = transcript();
        let before = screen(&t);
        let after_lines: Vec<String> = (0..20)
            .map(|i| format!("totally different row {i}"))
            .collect();
        let after = screen(&after_lines);
        assert_eq!(
            RemoteTerminalScrollShiftDetector::shift(&before, &after, 10),
            0
        );
    }

    #[test]
    fn blank_rows_never_vote_and_trailing_spaces_are_ignored() {
        let before = screen(&[
            "".to_string(),
            "alpha   ".to_string(),
            "".to_string(),
            "beta".to_string(),
            "".to_string(),
        ]);
        let after = screen(&[
            "".to_string(),
            "alpha".to_string(),
            "".to_string(),
            "beta  ".to_string(),
            "".to_string(),
        ]);
        assert_eq!(
            RemoteTerminalScrollShiftDetector::shift(&before, &after, 4),
            0
        );
    }
}
