/// Tests for [TerminalDropTargetMap] and [TerminalPathDragMap].
///
/// Ports of `TerminalDropTargetMapTests.swift` and
/// `TerminalPathDragMapTests.swift` from
/// `native/SupercliNative/Tests/SupercliNativeTests`.
library;

import 'package:test/test.dart';

import '../lib/terminal/terminal_drop_target_map.dart';
import '../lib/terminal/terminal_path_drag_map.dart';

void main() {
  group('TerminalDropTargetMap', () {
    test('target hit testing is fresh, bounded, and half-open', () {
      const now = 10_000;
      final map = TerminalDropTargetMap.fromJson({
        'version': 1,
        'pid': 42,
        'updated_at': now,
        'regions': [
          {
            'screen_row': 3,
            'start_column': 2,
            'end_row': 8,
            'end_column': 20,
          },
        ],
      });
      expect(map.accepts(row: 3, column: 2, nowMilliseconds: now), isTrue);
      expect(map.accepts(row: 7, column: 19, nowMilliseconds: now), isTrue);
      // Half-open: endRow/endColumn excluded.
      expect(map.accepts(row: 8, column: 19, nowMilliseconds: now), isFalse);
      expect(map.accepts(row: 7, column: 20, nowMilliseconds: now), isFalse);
      // Stale.
      expect(
        map.accepts(
          row: 4,
          column: 4,
          nowMilliseconds: now + TerminalDropTargetMap.maximumAgeMilliseconds + 1,
        ),
        isFalse,
      );
    });

    test('event JSON uses the shared wire contract', () {
      final event = TerminalDropTargetEvent(
        version: 1,
        eventID: 'evt-1',
        updatedAt: 10_000,
        kind: TerminalDropTargetEventKind.drop,
        screenRow: 5,
        column: 9,
        text: 'notes/file.md',
        references: ['/tmp/notes/file.md'],
      );
      final json = event.toJson();
      expect(json['version'], 1);
      expect(json['kind'], 'drop');
      expect(json['screen_row'], 5);
      expect(json['column'], 9);
      expect(json['text'], 'notes/file.md');
    });

    test('rejects wrong version and non-positive pid', () {
      final badVersion = TerminalDropTargetMap(
        version: 2,
        processID: 42,
        updatedAt: 10_000,
        regions: const [
          TerminalDropTargetRegion(
            screenRow: 0,
            startColumn: 0,
            endRow: 10,
            endColumn: 10,
          ),
        ],
      );
      expect(
        badVersion.accepts(row: 1, column: 1, nowMilliseconds: 10_000),
        isFalse,
      );
      final badPid = TerminalDropTargetMap(
        version: 1,
        processID: 0,
        updatedAt: 10_000,
        regions: const [
          TerminalDropTargetRegion(
            screenRow: 0,
            startColumn: 0,
            endRow: 10,
            endColumn: 10,
          ),
        ],
      );
      expect(
        badPid.accepts(row: 1, column: 1, nowMilliseconds: 10_000),
        isFalse,
      );
    });
  });

  group('TerminalPathDragMap', () {
    test('fresh mapped cell resolves an absolute path', () {
      const map = TerminalPathDragMap(
        version: 1,
        processID: 42,
        updatedAt: 10_000,
        rows: [
          TerminalPathDragRow(
            screenRow: 4,
            startColumn: 0,
            endColumn: 18,
            path: '/tmp/a folder',
          ),
        ],
      );
      expect(
        map.pathAt(row: 4, column: 7, nowMilliseconds: 12_000),
        '/tmp/a folder',
      );
      expect(map.pathAt(row: 4, column: 18, nowMilliseconds: 12_000), isNull);
      expect(map.pathAt(row: 3, column: 7, nowMilliseconds: 12_000), isNull);
    });

    test('stale, future, and relative maps fail closed', () {
      const row = TerminalPathDragRow(
        screenRow: 1,
        startColumn: 0,
        endColumn: 8,
        path: '/tmp/item',
      );
      // Stale.
      expect(
        const TerminalPathDragMap(
          version: 1,
          processID: 42,
          updatedAt: 10_000,
          rows: [row],
        ).pathAt(row: 1, column: 2, nowMilliseconds: 16_000),
        isNull,
      );
      // From the future.
      expect(
        const TerminalPathDragMap(
          version: 1,
          processID: 42,
          updatedAt: 20_000,
          rows: [row],
        ).pathAt(row: 1, column: 2, nowMilliseconds: 10_000),
        isNull,
      );
      // Relative path.
      expect(
        const TerminalPathDragMap(
          version: 1,
          processID: 42,
          updatedAt: 10_000,
          rows: [
            TerminalPathDragRow(
              screenRow: 1,
              startColumn: 0,
              endColumn: 8,
              path: 'relative/item',
            ),
          ],
        ).pathAt(row: 1, column: 2, nowMilliseconds: 10_000),
        isNull,
      );
    });
  });
}
