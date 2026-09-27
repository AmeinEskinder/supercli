/// Tests for the predictive scrolling engine and shift detector (port of
/// RemoteTerminalScrollPrediction.swift).
library;

import 'package:supercli_app/ios/scroll_prediction.dart';
import 'package:test/test.dart';

RemoteTerminalScrollPredictionEngine engine() =>
    RemoteTerminalScrollPredictionEngine();

void main() {
  group('RemoteTerminalScrollPredictionEngine', () {
    test('beginGesture latches display decision', () {
      final e = engine();
      // No confidence yet: no display.
      e.beginGesture();
      expect(e.offsetRows, 0);

      // Earn confidence with an observed wheel→shift response.
      e.wheelSent(3, now: 0.0);
      e.contentShifted(3, now: 1.0); // slow path: latency 1.0s
      expect(e.isConfident, isTrue);

      e.beginGesture();
      e.wheelSent(2, now: 2.0);
      // Display latched on: offset follows the finger.
      expect(e.offsetRows, -2);
    });

    test('display gated by latency', () {
      final e = engine();
      e.wheelSent(3, now: 0.0);
      // Fast path: 0.05s round trip, below the display threshold.
      e.contentShifted(3, now: 0.05);
      expect(e.isConfident, isTrue);
      expect(
        e.responseLatency,
        lessThan(RemoteTerminalScrollPredictionEngine.displayLatencyThreshold),
      );

      e.beginGesture();
      e.wheelSent(2, now: 1.0);
      // Tracking continues, but no translation on a fast link.
      expect(e.offsetRows, 0);
      expect(e.pendingRows, 2);
    });

    test('wheelSent caps pending rows', () {
      final e = engine();
      e.wheelSent(
        RemoteTerminalScrollPredictionEngine.maximumPendingRows + 10,
        now: 0.0,
      );
      expect(e.pendingRows, 0); // over the cap: not tracked
      e.wheelSent(10, now: 0.1);
      e.wheelSent(10, now: 0.2);
      expect(e.pendingRows, 20);
      e.wheelSent(1, now: 0.3); // would exceed the cap
      expect(e.pendingRows, 20);
    });

    test('wheelSent ignores zero rows', () {
      final e = engine();
      e.wheelSent(0, now: 0.0);
      expect(e.pendingRows, 0);
    });

    test('contentShifted drains oldest first', () {
      final e = engine();
      e.wheelSent(3, now: 0.0);
      e.wheelSent(5, now: 0.1);
      e.contentShifted(4, now: 1.0);
      // First batch (3) fully drained, second split: 5 - 1 = 4 left.
      expect(e.pendingRows, 4);
      expect(e.pending.length, 1);
      expect(e.pending.single.rows, 4);
    });

    test('opposite-direction movement drains nothing', () {
      final e = engine();
      e.wheelSent(3, now: 0.0);
      e.contentShifted(-2, now: 1.0); // autoscroll, not an answer
      expect(e.pendingRows, 3);
      expect(e.isConfident, isFalse);
    });

    test('overshoot clamps at zero', () {
      final e = engine();
      e.wheelSent(3, now: 0.0);
      e.contentShifted(10, now: 1.0); // TUI moved further than predicted
      expect(e.pendingRows, 0);
    });

    test('expireIfUnanswered closes gate only when gesture unacked', () {
      final e = engine();
      e.wheelSent(3, now: 0.0);
      // No answer within the timeout: gate closes.
      expect(
        e.expireIfUnanswered(
          now: RemoteTerminalScrollPredictionEngine.responseTimeout + 0.1,
        ),
        isTrue,
      );
      expect(e.isConfident, isFalse);
      expect(e.pendingRows, 0);
    });

    test('expireIfUnanswered keeps confidence when gesture was acked', () {
      final e = engine();
      e.wheelSent(3, now: 0.0);
      e.contentShifted(3, now: 0.1); // answered…
      e.wheelSent(2, now: 0.2); // …then a trailing batch goes unanswered
      expect(
        e.expireIfUnanswered(
          now: 0.2 + RemoteTerminalScrollPredictionEngine.responseTimeout + 0.1,
        ),
        isTrue,
      );
      // Coalesced trailing frames must not punish a responsive TUI.
      expect(e.isConfident, isTrue);
    });

    test('cancel keeps confidence', () {
      final e = engine();
      e.wheelSent(3, now: 0.0);
      e.contentShifted(3, now: 1.0);
      expect(e.isConfident, isTrue);
      e.cancel();
      expect(e.pendingRows, 0);
      expect(e.isConfident, isTrue);
    });

    test('resetConfidence clears latency estimate', () {
      final e = engine();
      e.wheelSent(3, now: 0.0);
      e.contentShifted(3, now: 1.0);
      expect(e.responseLatency, isNotNull);
      e.resetConfidence();
      expect(e.responseLatency, isNull);
      expect(e.isConfident, isFalse);
      expect(e.pendingRows, 0);
    });

    test('probe batch samples path latency once', () {
      final e = engine();
      e.wheelSent(5, now: 0.0); // probe: queue was empty
      expect(e.pending.single.probe, isTrue);
      // Partial answer splits the probe; it must not re-sample older.
      e.contentShifted(2, now: 2.0);
      expect(e.responseLatency, 2.0);
      expect(e.pending.single.probe, isFalse);
      expect(e.pending.single.rows, 3);
    });
  });

  group('RemoteTerminalScrollShiftDetector', () {
    test('detects upward content shift', () {
      const before = 'one\ntwo\nthree\nfour';
      const after = 'two\nthree\nfour\nfive';
      expect(
        RemoteTerminalScrollShiftDetector.shift(
          before: before,
          after: after,
          maxShift: 3,
        ),
        1,
      );
    });

    test('detects downward content shift', () {
      const before = 'two\nthree\nfour\nfive';
      const after = 'one\ntwo\nthree\nfour';
      expect(
        RemoteTerminalScrollShiftDetector.shift(
          before: before,
          after: after,
          maxShift: 3,
        ),
        -1,
      );
    });

    test('stationary screen reads as zero', () {
      const text = 'one\ntwo\nthree';
      expect(
        RemoteTerminalScrollShiftDetector.shift(
          before: text,
          after: text,
          maxShift: 3,
        ),
        0,
      );
    });

    test('repainted-beyond-recognition reads as zero', () {
      const before = 'aaaaaaaa\nbbbbbbbb\ncccccccc';
      const after = 'xxxxxxxx\nyyyyyyyy\nzzzzzzzz';
      expect(
        RemoteTerminalScrollShiftDetector.shift(
          before: before,
          after: after,
          maxShift: 3,
        ),
        0,
      );
    });

    test('trailing spaces do not break alignment', () {
      const before = 'one   \ntwo  \nthree ';
      const after = 'two\nthree\nfour';
      expect(
        RemoteTerminalScrollShiftDetector.shift(
          before: before,
          after: after,
          maxShift: 3,
        ),
        1,
      );
    });

    test('zero maxShift returns zero', () {
      expect(
        RemoteTerminalScrollShiftDetector.shift(
          before: 'a\nb',
          after: 'b\nc',
          maxShift: 0,
        ),
        0,
      );
    });

    test('ties resolve to the smallest movement', () {
      // 'b' appears in both rows; the smallest shift (0) must win.
      const before = 'a\nb';
      const after = 'b\nb';
      expect(
        RemoteTerminalScrollShiftDetector.shift(
          before: before,
          after: after,
          maxShift: 2,
        ),
        0,
      );
    });
  });
}
