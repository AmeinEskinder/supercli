/// Mosh-style predictive scrolling for remote-rendered TUIs.
///
/// Port of `RemoteTerminalScrollPredictionEngine` and
/// `RemoteTerminalScrollShiftDetector` in `RemoteTerminalScrollPrediction.swift`
/// (`clients/legacy/ios/SupercliIOS`). The SwiftUI state/modifier
/// (`RemoteTerminalScrollPredictionState`,
/// `RemoteTerminalScrollPredictionOffsetModifier`) are UI glue and out of
/// scope — only the engine and the shift detector are ported.
///
/// Alternate-screen TUIs own their scrolling: a flick becomes wheel events,
/// the Mac redraws, frames ride back. Over the relay every visible movement
/// therefore lags a full WAN round trip. This engine closes that gap by
/// predicting the one thing a wheel almost always means: the content shifts
/// by the rows sent. The view translates the already-rendered canvas in sync
/// with the finger and lets the incoming edge show background until truth
/// arrives.
///
/// Safety mirrors [RemoteTerminalPredictionEngine]: the prediction is a
/// *view-layer translation* that touches no terminal state, it displays only
/// behind a confidence gate earned by an observed wheel→redraw response, and
/// an unanswered gesture eases home and closes the gate.
///
/// Reconciliation is by OBSERVED CONTENT SHIFT, not per-chunk counting: the
/// renderer measures how many rows the viewport actually moved when a chunk
/// committed and drains exactly that many predicted rows from the front of
/// the queue.
///
/// Timestamps are seconds on an arbitrary monotonic clock (mirrors the
/// Rust port's `f64` convention and keeps the engine deterministically
/// testable).
library;

/// One batch of wheel rows sent to the host.
final class PendingScrollBatch {
  PendingScrollBatch({
    required this.rows,
    required this.sentAt,
    this.probe = false,
  });

  /// Signed rows: positive = wheel down (content moves up on screen).
  int rows;
  final double sentAt;

  /// Sent while the queue was empty: its send→answered time is pure path
  /// latency. Batches queued behind others measure queue wait too.
  bool probe;

  @override
  bool operator ==(Object other) =>
      other is PendingScrollBatch &&
      other.rows == rows &&
      other.sentAt == sentAt &&
      other.probe == probe;

  @override
  int get hashCode => Object.hash(rows, sentAt, probe);
}

final class RemoteTerminalScrollPredictionEngine {
  /// No redraw this long after the oldest unacked send means the TUI is
  /// not answering wheels for this gesture.
  static const double responseTimeout = 0.45;

  /// Hard cap on how far prediction may run ahead of truth; beyond this
  /// the placeholder region dominates the viewport.
  static const int maximumPendingRows = 20;

  /// The translation displays only when wheels take at least this long to
  /// come back as pixels. On a fast link the whole-canvas translation
  /// visibly fights a TUI's pinned chrome for no felt benefit.
  static const double displayLatencyThreshold = 0.18;

  final List<PendingScrollBatch> _pending = <PendingScrollBatch>[];

  /// Earned by any observed wheel→shift response, lost when an entire
  /// gesture goes unanswered. Tracking continues while closed so the
  /// next responsive gesture re-earns display with no visible risk.
  bool _isConfident = false;
  bool _ackedThisGesture = false;

  /// EWMA of send→answered-on-screen time, sampled at each drain from the
  /// oldest pending batch. Describes the path (LAN vs relay), so it
  /// survives gestures and resets with confidence.
  double? _responseLatency;

  /// Display decision latched at gesture start so the offset can never
  /// pop in or out mid-drag when confidence or the EWMA crosses over.
  bool _displaysThisGesture = false;

  List<PendingScrollBatch> get pending =>
      List<PendingScrollBatch>.unmodifiable(_pending);
  bool get isConfident => _isConfident;
  double? get responseLatency => _responseLatency;

  int get pendingRows => _pending.fold(0, (sum, b) => sum + b.rows);

  /// Rows the canvas should translate by right now (negative y per down
  /// row — content follows the finger). Zero until the TUI has proven it
  /// answers wheels AND the path is slow enough for prediction to beat
  /// the real frames.
  int get offsetRows => _displaysThisGesture ? -pendingRows : 0;

  void beginGesture() {
    _ackedThisGesture = false;
    _displaysThisGesture =
        _isConfident && (_responseLatency ?? 0) >= displayLatencyThreshold;
  }

  void wheelSent(int rows, {required double now}) {
    if (rows == 0) return;
    // Stop growing at the cap: the steps still went to the host, so
    // later frames simply drain the tracked portion.
    if ((pendingRows + rows).abs() > maximumPendingRows) return;
    _pending.add(
      PendingScrollBatch(rows: rows, sentAt: now, probe: _pending.isEmpty),
    );
  }

  /// A committed feed moved the viewport content by [rows] (positive =
  /// content moved up, the wheel-down direction). That movement is the TUI
  /// answering the oldest predicted wheels: drain exactly that many rows
  /// from the front of the queue, splitting a partially-answered batch.
  /// Movement opposite the queued direction is not an answer and drains
  /// nothing. If the TUI moved further than predicted, the drain clamps at
  /// zero. [now] samples the send→on-screen latency that decides whether
  /// future gestures display at all.
  void contentShifted(int rows, {required double now}) {
    if (rows == 0 || _pending.isEmpty) return;
    final oldest = _pending.first;
    var remaining = rows;
    while (remaining != 0 && _pending.isNotEmpty) {
      final first = _pending.first;
      if ((first.rows > 0) != (remaining > 0)) break;
      if (first.rows.abs() <= remaining.abs()) {
        remaining -= first.rows;
        _pending.removeAt(0);
      } else {
        first.rows -= remaining;
        remaining = 0;
      }
    }
    if (remaining == rows) return;
    if (oldest.probe) {
      final sample = (now - oldest.sentAt).clamp(0.0, double.infinity);
      _responseLatency = _responseLatency == null
          ? sample
          : _responseLatency! * 0.7 + sample * 0.3;
      // A split probe stays queued; it has been sampled and must not
      // report an ever-older age on its next partial answer.
      if (_pending.isNotEmpty && _pending.first.sentAt == oldest.sentAt) {
        _pending.first.probe = false;
      }
    }
    _ackedThisGesture = true;
    _isConfident = true;
  }

  /// True when the oldest prediction expired unanswered — the caller
  /// eases the translation home. The gate closes only if the whole
  /// gesture produced no ack (coalesced trailing frames must not punish
  /// a TUI that demonstrably responded).
  bool expireIfUnanswered({required double now}) {
    final oldest = _pending.isEmpty ? null : _pending.first;
    if (oldest == null || now - oldest.sentAt < responseTimeout) {
      return false;
    }
    _pending.clear();
    if (!_ackedThisGesture) {
      _isConfident = false;
    }
    return true;
  }

  /// Gesture cancelled / session detached: drop the translation but keep
  /// earned confidence (it describes the TUI, not the gesture).
  void cancel() {
    _pending.clear();
  }

  /// Full reset (new session / screen replaced): confidence and the
  /// path-latency estimate must be re-earned against whatever now owns
  /// the terminal.
  void resetConfidence() {
    _pending.clear();
    _isConfident = false;
    _ackedThisGesture = false;
    _responseLatency = null;
    _displaysThisGesture = false;
  }
}

/// Measures how many rows the rendered viewport moved between two reads —
/// the reconciliation signal for [RemoteTerminalScrollPredictionEngine.contentShifted].
/// Pure text alignment over the two viewport snapshots: for each candidate
/// shift the score is the number of identical non-blank rows, and the
/// smallest shift wins ties, so a stationary screen (or one repainted beyond
/// recognition) reads as zero rather than guessing. Positive = content moved
/// up on screen (the wheel-down direction), matching the engine's
/// convention.
abstract final class RemoteTerminalScrollShiftDetector {
  /// Fewer than this many agreeing non-blank rows means the screen
  /// changed too much to trust any alignment, including zero.
  static const int minimumMatches = 2;

  static int shift({
    required String before,
    required String after,
    required int maxShift,
  }) {
    if (maxShift <= 0) return 0;
    final beforeRows = _rows(before);
    final afterRows = _rows(after);
    final count = beforeRows.length < afterRows.length
        ? beforeRows.length
        : afterRows.length;
    if (count == 0) return 0;

    var bestShift = 0;
    var bestScore = -1;
    for (var magnitude = 0; magnitude <= maxShift; magnitude++) {
      final candidates = magnitude == 0 ? const [0] : [magnitude, -magnitude];
      for (final candidate in candidates) {
        var score = 0;
        for (var index = 0; index < count; index++) {
          final source = index + candidate;
          if (source < 0 || source >= beforeRows.length) continue;
          final row = afterRows[index];
          if (row.isEmpty || row != beforeRows[source]) continue;
          score += 1;
        }
        // Strictly greater: |candidate| grows through the loop, so
        // ties resolve to the smallest movement.
        if (score > bestScore) {
          bestScore = score;
          bestShift = candidate;
        }
      }
    }
    if (bestScore < minimumMatches) return 0;
    return bestShift;
  }

  /// Viewport text split into rows with trailing spaces dropped, so
  /// ghostty's cell padding never breaks an otherwise identical row.
  static List<String> _rows(String text) {
    return text.split('\n').map((line) {
      var trimmed = line;
      while (trimmed.endsWith(' ')) {
        trimmed = trimmed.substring(0, trimmed.length - 1);
      }
      return trimmed;
    }).toList();
  }
}
