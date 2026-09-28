//! Mosh-style predictive local echo for high-latency transports.
//!
//! Port of `RemoteTerminalPredictionEngine` in `RemoteTerminalPrediction.swift`
//! (`ios/SupercliIOS`). The SwiftUI overlay views are not ported — only the
//! engine, which is UI-agnostic.
//!
//! Over the relay a keystroke's echo pays two WAN traversals, so typing
//! reads as laggy even when the link is healthy. The engine tracks printable
//! keystrokes at the cell the local surface's IME caret reported, renders
//! them immediately as a provisional overlay, and reconciles against the
//! authoritative viewport once server bytes arrive.
//!
//! Safety comes from a confidence gate, not from understanding the remote
//! program: predictions are invisible until one of them is CONFIRMED by the
//! real grid (the predicted character appeared at the predicted cell). A
//! context that never echoes recognizably — password prompts, vim normal
//! mode, menus, TUIs that park the caret elsewhere — never earns display,
//! and a contradiction or expiry drops the gate again.

/// One tracked keystroke. Timestamps are seconds on an arbitrary monotonic
/// clock (matches Swift's `Date.timeIntervalSince` usage in tests).
#[derive(Debug, Clone, PartialEq)]
pub struct Prediction {
    pub character: char,
    /// 0-based viewport cell where the echo should appear.
    pub row: usize,
    pub column: usize,
    pub sent_at: f64,
}

#[derive(Debug, Clone, Default)]
pub struct RemoteTerminalPredictionEngine {
    pending: Vec<Prediction>,
    /// Display gate: earned by the first confirmed prediction, lost on
    /// contradiction or expiry. Tracking continues while the gate is closed
    /// so ordinary echo re-earns it with no user-visible risk.
    is_confident: bool,
}

impl RemoteTerminalPredictionEngine {
    /// A prediction unconfirmed this long means echo is not coming back in
    /// recognizable form; drop everything and close the display gate.
    pub const EXPIRY: f64 = 2.0;

    /// Beyond this many unconfirmed keystrokes something is off (key repeat
    /// into a stalled link) — stop predicting rather than paint a phantom
    /// line.
    pub const MAXIMUM_PENDING: usize = 24;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending(&self) -> &[Prediction] {
        &self.pending
    }

    pub fn is_confident(&self) -> bool {
        self.is_confident
    }

    /// Register a printable keystroke. `cursor` is the current caret cell
    /// (used only when nothing is pending — later keystrokes chain off the
    /// previous prediction); `None` means the caret is unknown, which makes
    /// prediction impossible.
    pub fn keystroke(
        &mut self,
        character: char,
        cursor: Option<(usize, usize)>,
        columns: usize,
        now: f64,
    ) {
        if self.pending.len() >= Self::MAXIMUM_PENDING {
            self.clear_pending();
            return;
        }
        let anchor: (usize, usize) = if let Some(last) = self.pending.last() {
            (last.row, last.column + 1)
        } else if let Some(cursor) = cursor {
            cursor
        } else {
            self.clear_pending();
            return;
        };
        // Wrapping is the remote program's call (soft wrap, composer
        // reflow) — stop predicting at the line edge instead of guessing.
        if anchor.1 + 1 >= columns {
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
    /// `expiry`.
    pub fn reconcile(&mut self, rows: &[&str], now: f64) {
        while let Some(first) = self.pending.first() {
            if now - first.sent_at > Self::EXPIRY {
                self.pending.clear();
                self.is_confident = false;
                return;
            }
            let cell = first
                .row
                .checked_sub(0)
                .and_then(|r| rows.get(r))
                .and_then(|row| Self::cell_character(row, first.column));
            match cell {
                None => return,      // beyond current content: still blank, keep waiting
                Some(' ') => return, // echo not painted yet, keep waiting
                Some(c) if c == first.character => {
                    self.pending.remove(0);
                    self.is_confident = true;
                }
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
        if self.is_confident && !self.pending.is_empty() {
            Some(self.pending.iter().map(|p| p.character).collect())
        } else {
            None
        }
    }

    pub fn anchor(&self) -> Option<&Prediction> {
        self.pending.first()
    }

    /// Character at a display column, assuming one column per char.
    /// Wide glyphs (CJK, emoji) earlier in the row shift this mapping; the
    /// resulting misread at worst reads as a contradiction, which only
    /// hides the overlay.
    pub fn cell_character(row: &str, column: usize) -> Option<char> {
        row.chars().nth(column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: f64 = 1_000.0;

    fn rows(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    fn reconcile_strs(engine: &mut RemoteTerminalPredictionEngine, lines: &[&str], now: f64) {
        let owned = rows(lines);
        let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        engine.reconcile(&refs, now);
    }

    #[test]
    fn predictions_stay_hidden_until_first_confirmation() {
        let mut engine = RemoteTerminalPredictionEngine::new();
        engine.keystroke('h', Some((1, 4)), 80, START);
        assert!(
            engine.displayed_text().is_none(),
            "an unconfirmed context (password prompt, menu) must never show a prediction"
        );

        reconcile_strs(&mut engine, &["", "  > h"], START + 0.3);
        assert!(engine.is_confident());
        assert!(engine.pending().is_empty());

        engine.keystroke('i', Some((1, 5)), 80, START);
        assert_eq!(
            engine.displayed_text(),
            Some(vec!['i']),
            "after one confirmed echo the gate is open"
        );
    }

    #[test]
    fn rapid_keystrokes_chain_off_the_last_prediction() {
        let mut engine = RemoteTerminalPredictionEngine::new();
        engine.keystroke('a', Some((0, 2)), 80, START);
        engine.keystroke('b', Some((0, 2)), 80, START);
        engine.keystroke('c', Some((0, 2)), 80, START);
        let cols: Vec<usize> = engine.pending().iter().map(|p| p.column).collect();
        let rows_: Vec<usize> = engine.pending().iter().map(|p| p.row).collect();
        assert_eq!(cols, vec![2, 3, 4]);
        assert_eq!(rows_, vec![0, 0, 0]);
    }

    #[test]
    fn partial_echo_confirms_prefix_and_keeps_the_rest() {
        let mut engine = RemoteTerminalPredictionEngine::new();
        engine.keystroke('a', Some((0, 0)), 80, START);
        engine.keystroke('b', Some((0, 0)), 80, START);
        reconcile_strs(&mut engine, &["a"], START + 0.2);
        assert!(engine.is_confident());
        let chars: Vec<char> = engine.pending().iter().map(|p| p.character).collect();
        assert_eq!(chars, vec!['b'], "unechoed suffix keeps waiting");
    }

    #[test]
    fn foreign_character_at_predicted_cell_closes_the_gate() {
        let mut engine = RemoteTerminalPredictionEngine::new();
        engine.keystroke('j', Some((0, 3)), 80, START);
        reconcile_strs(&mut engine, &["absX"], START + 0.2);
        assert!(engine.pending().is_empty());
        assert!(
            !engine.is_confident(),
            "vim-normal-mode style contradiction closes display"
        );
    }

    #[test]
    fn expiry_drops_everything_and_closes_the_gate() {
        let mut engine = RemoteTerminalPredictionEngine::new();
        engine.keystroke('s', Some((0, 0)), 80, START);
        reconcile_strs(&mut engine, &[""], START + 0.5);
        assert!(
            !engine.pending().is_empty(),
            "blank cell inside the expiry window waits"
        );
        reconcile_strs(
            &mut engine,
            &[""],
            START + RemoteTerminalPredictionEngine::EXPIRY + 0.1,
        );
        assert!(engine.pending().is_empty());
        assert!(!engine.is_confident());
    }

    #[test]
    fn backspace_removes_the_last_prediction() {
        let mut engine = RemoteTerminalPredictionEngine::new();
        engine.keystroke('a', Some((0, 0)), 80, START);
        engine.keystroke('b', Some((0, 0)), 80, START);
        engine.backspace();
        let chars: Vec<char> = engine.pending().iter().map(|p| p.character).collect();
        assert_eq!(chars, vec!['a']);
        engine.backspace();
        engine.backspace();
        assert!(
            engine.pending().is_empty(),
            "extra backspaces with nothing pending are no-ops"
        );
    }

    #[test]
    fn line_edge_and_unknown_cursor_suppress_prediction() {
        let mut engine = RemoteTerminalPredictionEngine::new();
        engine.keystroke('x', Some((0, 79)), 80, START);
        assert!(
            engine.pending().is_empty(),
            "wrap is the remote program's call"
        );
        engine.keystroke('x', None, 80, START);
        assert!(engine.pending().is_empty());
    }

    #[test]
    fn overflow_clears_instead_of_painting_a_phantom_line() {
        let mut engine = RemoteTerminalPredictionEngine::new();
        for _ in 0..RemoteTerminalPredictionEngine::MAXIMUM_PENDING {
            engine.keystroke('x', Some((0, 0)), 200, START);
        }
        assert_eq!(
            engine.pending().len(),
            RemoteTerminalPredictionEngine::MAXIMUM_PENDING
        );
        engine.keystroke('x', Some((0, 0)), 200, START);
        assert!(engine.pending().is_empty());
    }

    #[test]
    fn confidence_survives_non_printable_clear_but_not_reset() {
        let mut engine = RemoteTerminalPredictionEngine::new();
        engine.keystroke('a', Some((0, 0)), 80, START);
        reconcile_strs(&mut engine, &["a"], START + 0.1);
        assert!(engine.is_confident());
        engine.clear_pending();
        assert!(
            engine.is_confident(),
            "an arrow key doesn't invalidate earned trust"
        );
        engine.reset();
        assert!(!engine.is_confident(), "a replay/rebase does");
    }

    #[test]
    fn cell_character_maps_columns_and_blanks() {
        let row = "ab cd";
        assert_eq!(
            RemoteTerminalPredictionEngine::cell_character(row, 0),
            Some('a')
        );
        assert_eq!(
            RemoteTerminalPredictionEngine::cell_character(row, 2),
            Some(' ')
        );
        assert_eq!(
            RemoteTerminalPredictionEngine::cell_character(row, 4),
            Some('d')
        );
        assert_eq!(
            RemoteTerminalPredictionEngine::cell_character(row, 5),
            None,
            "beyond the painted row reads as blank (waiting), not contradiction"
        );
    }
}
