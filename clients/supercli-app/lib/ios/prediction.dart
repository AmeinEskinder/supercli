/// Mosh-style predictive local echo for high-latency transports.
///
/// Port of `RemoteTerminalPredictionEngine` in `RemoteTerminalPrediction.swift`
/// (`clients/legacy/ios/SupercliIOS`). Only the engine is ported — the
/// SwiftUI overlay views (`RemoteTerminalPredictionOverlayView`,
/// `RemoteTerminalPredictionOverlayState`, `RemoteTerminalPredictionOverlayModel`,
/// `RemoteTerminalPredictionInputTap`) are UI glue and out of scope.
///
/// Over the relay a keystroke's echo pays two WAN traversals, so typing
/// reads as laggy even when the link is healthy. The engine tracks printable
/// keystrokes at the cell the local surface's IME caret reported, renders
/// them immediately as a provisional overlay, and reconciles against the
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
///
/// Timestamps are seconds on an arbitrary monotonic clock (mirrors the
/// Rust port's `f64` convention and keeps the engine deterministically
/// testable without wall-clock access).
library;

/// One tracked keystroke.
final class Prediction {
  const Prediction({
    required this.character,
    required this.row,
    required this.column,
    required this.sentAt,
  });

  /// Single grapheme cluster (Swift `Character`).
  final String character;

  /// 0-based viewport cell where the echo should appear.
  final int row;
  final int column;
  final double sentAt;

  @override
  bool operator ==(Object other) =>
      other is Prediction &&
      other.character == character &&
      other.row == row &&
      other.column == column &&
      other.sentAt == sentAt;

  @override
  int get hashCode => Object.hash(character, row, column, sentAt);

  @override
  String toString() =>
      'Prediction($character at $row:$column, sentAt: $sentAt)';
}

final class RemoteTerminalPredictionEngine {
  /// A prediction unconfirmed this long means echo is not coming back in
  /// recognizable form; drop everything and close the display gate.
  static const double expiry = 2.0;

  /// Beyond this many unconfirmed keystrokes something is off (key repeat
  /// into a stalled link) — stop predicting rather than paint a phantom
  /// line.
  static const int maximumPending = 24;

  final List<Prediction> _pending = <Prediction>[];

  /// Display gate: earned by the first confirmed prediction, lost on
  /// contradiction or expiry. Tracking continues while the gate is closed
  /// so ordinary echo re-earns it with no user-visible risk.
  bool _isConfident = false;

  List<Prediction> get pending => List<Prediction>.unmodifiable(_pending);
  bool get isConfident => _isConfident;
  Prediction? get anchor => _pending.isEmpty ? null : _pending.first;

  /// Register a printable keystroke. [cursor] is the current caret cell
  /// (used only when nothing is pending — later keystrokes chain off the
  /// previous prediction); null means the caret is unknown, which makes
  /// prediction impossible.
  void keystroke(
    String character, {
    ({int row, int column})? cursor,
    required int columns,
    required double now,
  }) {
    if (_pending.length >= maximumPending) {
      clearPending();
      return;
    }
    final ({int row, int column}) anchor;
    if (_pending.isNotEmpty) {
      final last = _pending.last;
      anchor = (row: last.row, column: last.column + 1);
    } else if (cursor != null) {
      anchor = cursor;
    } else {
      clearPending();
      return;
    }
    // Wrapping is the remote program's call (soft wrap, composer
    // reflow) — stop predicting at the line edge instead of guessing.
    if (anchor.row < 0 || anchor.column < 0 || anchor.column >= columns - 1) {
      clearPending();
      return;
    }
    _pending.add(
      Prediction(
        character: character,
        row: anchor.row,
        column: anchor.column,
        sentAt: now,
      ),
    );
  }

  void backspace() {
    if (_pending.isNotEmpty) _pending.removeLast();
  }

  /// Anything non-printable (submit, arrows, escape sequences) moves the
  /// cursor in ways only the server knows; keep the earned confidence.
  void clearPending() {
    _pending.clear();
  }

  /// Full reset for replays/rebase/session teardown.
  void reset() {
    _pending.clear();
    _isConfident = false;
  }

  /// Reconcile against the authoritative viewport after server bytes.
  /// Confirms predictions in order; a foreign character at a predicted
  /// cell is a contradiction and closes the gate; blank cells wait until
  /// [expiry].
  void reconcile(List<String> rows, {required double now}) {
    while (_pending.isNotEmpty) {
      final first = _pending.first;
      if (now - first.sentAt > expiry) {
        _pending.clear();
        _isConfident = false;
        return;
      }
      final String? cell = (first.row >= 0 && first.row < rows.length)
          ? cellCharacter(rows[first.row], first.column)
          : null;
      if (cell == null) {
        return; // beyond current content: still blank, keep waiting
      }
      if (cell == first.character) {
        _pending.removeAt(0);
        _isConfident = true;
        continue;
      }
      if (cell == ' ') return; // echo not painted yet, keep waiting
      // Something else landed where we predicted: wrong context.
      _pending.clear();
      _isConfident = false;
      return;
    }
  }

  /// The provisional characters to draw, only while the gate is open.
  List<String>? get displayedText {
    if (!_isConfident || _pending.isEmpty) return null;
    return _pending.map((p) => p.character).toList();
  }

  /// Character at a display column, assuming one column per character.
  /// Wide glyphs (CJK, emoji) earlier in the row shift this mapping; the
  /// resulting misread at worst reads as a contradiction, which only
  /// hides the overlay.
  static String? cellCharacter(String row, int column) {
    final runes = row.runes.toList();
    if (column < 0 || column >= runes.length) return null;
    return String.fromCharCode(runes[column]);
  }
}
