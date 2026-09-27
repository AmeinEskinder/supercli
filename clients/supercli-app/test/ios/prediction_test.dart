/// Tests for the predictive local echo engine (port of
/// RemoteTerminalPrediction.swift).
library;

import 'package:supercli_app/ios/prediction.dart';
import 'package:test/test.dart';

const _cols = 80;

RemoteTerminalPredictionEngine engine() => RemoteTerminalPredictionEngine();

void main() {
  group('RemoteTerminalPredictionEngine', () {
    test('keystroke chains off previous prediction', () {
      final e = engine();
      e.keystroke('h', cursor: (row: 0, column: 0), columns: _cols, now: 0.0);
      e.keystroke('i', cursor: (row: 5, column: 5), columns: _cols, now: 0.1);
      final pending = e.pending;
      expect(pending.length, 2);
      expect(pending[0].character, 'h');
      expect(pending[0].row, 0);
      expect(pending[0].column, 0);
      // Second keystroke chains off the first, ignoring the moved cursor.
      expect(pending[1].character, 'i');
      expect(pending[1].row, 0);
      expect(pending[1].column, 1);
    });

    test('keystroke without cursor or pending clears', () {
      final e = engine();
      e.keystroke('h', cursor: (row: 0, column: 0), columns: _cols, now: 0.0);
      // No pending and no cursor: prediction impossible.
      e.clearPending();
      e.keystroke('x', columns: _cols, now: 0.1);
      expect(e.pending, isEmpty);
    });

    test('keystroke stops at the line edge', () {
      final e = engine();
      // Cursor at the last column: wrapping is the remote program's call.
      e.keystroke(
        'x',
        cursor: (row: 0, column: _cols - 1),
        columns: _cols,
        now: 0.0,
      );
      expect(e.pending, isEmpty);
    });

    test('backspace drops last pending', () {
      final e = engine();
      e.keystroke('h', cursor: (row: 0, column: 0), columns: _cols, now: 0.0);
      e.keystroke('i', cursor: (row: 0, column: 0), columns: _cols, now: 0.1);
      e.backspace();
      expect(e.pending.length, 1);
      expect(e.pending.single.character, 'h');
      e.backspace();
      expect(e.pending, isEmpty);
      // Backspace on empty is a no-op.
      e.backspace();
      expect(e.pending, isEmpty);
    });

    test('clearPending keeps confidence', () {
      final e = engine();
      e.keystroke('h', cursor: (row: 0, column: 0), columns: _cols, now: 0.0);
      e.reconcile(['h'], now: 0.1);
      expect(e.isConfident, isTrue);
      e.keystroke('i', cursor: (row: 9, column: 9), columns: _cols, now: 0.2);
      e.clearPending();
      expect(e.pending, isEmpty);
      expect(e.isConfident, isTrue);
    });

    test('reset clears confidence', () {
      final e = engine();
      e.keystroke('h', cursor: (row: 0, column: 0), columns: _cols, now: 0.0);
      e.reconcile(['h'], now: 0.1);
      expect(e.isConfident, isTrue);
      e.reset();
      expect(e.pending, isEmpty);
      expect(e.isConfident, isFalse);
    });

    test('reconcile confirms and earns gate', () {
      final e = engine();
      e.keystroke('h', cursor: (row: 0, column: 0), columns: _cols, now: 0.0);
      e.keystroke('i', cursor: (row: 0, column: 0), columns: _cols, now: 0.1);
      expect(e.displayedText, isNull); // gate closed until confirmed
      e.reconcile(['hi'], now: 0.2);
      expect(e.isConfident, isTrue);
      expect(e.pending, isEmpty);
    });

    test('displayedText hidden until confident', () {
      final e = engine();
      e.keystroke('h', cursor: (row: 0, column: 0), columns: _cols, now: 0.0);
      expect(e.displayedText, isNull);
      // Blank cell: echo not painted yet, keep waiting.
      e.reconcile([' '], now: 0.5);
      expect(e.displayedText, isNull);
      expect(e.isConfident, isFalse);
      // Confirmation earns the gate.
      e.reconcile(['h'], now: 0.6);
      expect(e.isConfident, isTrue);
      e.keystroke('i', cursor: (row: 0, column: 1), columns: _cols, now: 0.7);
      expect(e.displayedText, ['i']);
    });

    test('contradiction closes the gate', () {
      final e = engine();
      e.keystroke('h', cursor: (row: 0, column: 0), columns: _cols, now: 0.0);
      e.reconcile(['h'], now: 0.1);
      expect(e.isConfident, isTrue);
      e.keystroke('x', cursor: (row: 0, column: 1), columns: _cols, now: 0.2);
      // Something else landed where we predicted: wrong context.
      e.reconcile(['hy'], now: 0.3);
      expect(e.pending, isEmpty);
      expect(e.isConfident, isFalse);
    });

    test('expiry drops gate', () {
      final e = engine();
      e.keystroke('h', cursor: (row: 0, column: 0), columns: _cols, now: 0.0);
      e.reconcile([' '], now: RemoteTerminalPredictionEngine.expiry + 0.1);
      expect(e.pending, isEmpty);
      expect(e.isConfident, isFalse);
    });

    test('maximumPending stops predicting', () {
      final e = engine();
      for (
        var i = 0;
        i < RemoteTerminalPredictionEngine.maximumPending + 5;
        i++
      ) {
        e.keystroke(
          'a',
          cursor: (row: 0, column: 0),
          columns: _cols,
          now: i * 0.01,
        );
      }
      // Overflow clears rather than painting a phantom line.
      expect(
        e.pending.length,
        lessThan(RemoteTerminalPredictionEngine.maximumPending),
      );
    });

    test('cellCharacter bounds', () {
      expect(RemoteTerminalPredictionEngine.cellCharacter('hello', 0), 'h');
      expect(RemoteTerminalPredictionEngine.cellCharacter('hello', 4), 'o');
      expect(RemoteTerminalPredictionEngine.cellCharacter('hello', 5), isNull);
      expect(RemoteTerminalPredictionEngine.cellCharacter('hello', -1), isNull);
      expect(RemoteTerminalPredictionEngine.cellCharacter('', 0), isNull);
    });

    test('anchor is the first pending prediction', () {
      final e = engine();
      expect(e.anchor, isNull);
      e.keystroke('h', cursor: (row: 2, column: 3), columns: _cols, now: 0.0);
      expect(e.anchor?.character, 'h');
      expect(e.anchor?.row, 2);
      expect(e.anchor?.column, 3);
    });
  });
}
