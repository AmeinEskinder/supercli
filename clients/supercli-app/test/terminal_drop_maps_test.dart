/// Wiring test: TerminalPaneView -> TerminalDropMaps -> Rust FFI.
///
/// The decision logic (hit tests, TTL gates, wire codecs) is tested in Rust
/// (`crates/supercli-core/src/terminal_drop_maps.rs`, 6 tests) and at the
/// C ABI boundary (`crates/supercli-client-ffi`). These tests cover the
/// Dart side only: session-dir resolution, fail-closed file reads, and the
/// TerminalPaneView wiring.
///
/// FFI-dependent tests require the `SUPERCLI_FFI_LIB` environment variable
/// pointing at the built cdylib (same convention as `ffi_smoke_test.dart`);
/// they skip with a clear message when it is absent. Fail-closed tests need
/// no native library and always run.
library;

import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';

import 'package:supercli_app/screens/terminalpaneview.dart';

void main() {
  final ffiLib = Platform.environment['SUPERCLI_FFI_LIB'];
  final hasFfi = ffiLib != null && ffiLib.isNotEmpty;

  group('TerminalDropMaps', () {
    test('sessionDir joins app-sessions dir and session id', () {
      expect(
        TerminalDropMaps.sessionDir('/h/.supercli/app-sessions', 'abc'),
        '/h/.supercli/app-sessions/abc',
      );
    });

    test('missing map files fail closed without FFI', () {
      final dir = Directory.systemTemp.createTempSync('dropmaps-missing-');
      try {
        expect(
          TerminalDropMaps.acceptsDropAt(
            sessionDir: dir.path,
            row: 0,
            column: 0,
          ),
          isFalse,
        );
        expect(
          TerminalDropMaps.dragPathAt(sessionDir: dir.path, row: 0, column: 0),
          isNull,
        );
      } finally {
        dir.deleteSync(recursive: true);
      }
    });

    test('oversize map files fail closed without FFI', () {
      final dir = Directory.systemTemp.createTempSync('dropmaps-big-');
      try {
        final big = List.filled(TerminalDropMaps.maximumBytes + 1, 0x78);
        File('${dir.path}/${TerminalDropMaps.dropTargetMapFilename}')
            .writeAsBytesSync(big);
        File('${dir.path}/${TerminalDropMaps.dragMapFilename}')
            .writeAsBytesSync(big);
        expect(
          TerminalDropMaps.acceptsDropAt(
            sessionDir: dir.path,
            row: 0,
            column: 0,
          ),
          isFalse,
        );
        expect(
          TerminalDropMaps.dragPathAt(sessionDir: dir.path, row: 0, column: 0),
          isNull,
        );
      } finally {
        dir.deleteSync(recursive: true);
      }
    });

    test('drop-target map round-trips through the Rust FFI', () {
      if (!hasFfi) {
        markTestSkipped('SUPERCLI_FFI_LIB not set; build the cdylib first');
        return;
      }
      final dir = Directory.systemTemp.createTempSync('dropmaps-ffi-');
      try {
        final now = DateTime.now().millisecondsSinceEpoch;
        final map = {
          'version': 1,
          'pid': 42,
          'updated_at': now,
          'regions': [
            {
              'screen_row': 2,
              'start_column': 4,
              'end_row': 5,
              'end_column': 10,
            },
          ],
        };
        File('${dir.path}/${TerminalDropMaps.dropTargetMapFilename}')
            .writeAsStringSync(jsonEncode(map));
        expect(
          TerminalDropMaps.acceptsDropAt(
            sessionDir: dir.path,
            row: 3,
            column: 5,
            nowMs: now,
          ),
          isTrue,
        );
        expect(
          TerminalDropMaps.acceptsDropAt(
            sessionDir: dir.path,
            row: 0,
            column: 0,
            nowMs: now,
          ),
          isFalse,
          reason: 'cell outside every region',
        );
        expect(
          TerminalDropMaps.acceptsDropAt(
            sessionDir: dir.path,
            row: 3,
            column: 5,
            nowMs: now + 6000,
          ),
          isFalse,
          reason: 'stale map fails closed',
        );
      } finally {
        dir.deleteSync(recursive: true);
      }
    });

    test('path-drag map round-trips through the Rust FFI', () {
      if (!hasFfi) {
        markTestSkipped('SUPERCLI_FFI_LIB not set; build the cdylib first');
        return;
      }
      final dir = Directory.systemTemp.createTempSync('dragmap-ffi-');
      try {
        final now = DateTime.now().millisecondsSinceEpoch;
        final map = {
          'version': 1,
          'pid': 7,
          'updated_at': now,
          'rows': [
            {
              'screen_row': 3,
              'start_column': 0,
              'end_column': 20,
              'path': '/tmp/dragged.txt',
            },
          ],
        };
        File('${dir.path}/${TerminalDropMaps.dragMapFilename}')
            .writeAsStringSync(jsonEncode(map));
        expect(
          TerminalDropMaps.dragPathAt(
            sessionDir: dir.path,
            row: 3,
            column: 5,
            nowMs: now,
          ),
          '/tmp/dragged.txt',
        );
        expect(
          TerminalDropMaps.dragPathAt(
            sessionDir: dir.path,
            row: 9,
            column: 5,
            nowMs: now,
          ),
          isNull,
          reason: 'unmapped row',
        );
      } finally {
        dir.deleteSync(recursive: true);
      }
    });
  });

  group('TerminalPaneView drop-map wiring', () {
    test('static pane has no maps and fails closed', () {
      final view = TerminalPaneView(paneId: 'p1', title: 't');
      expect(view.hasDropMaps, isFalse);
      expect(view.sessionDirectory, isNull);
      expect(view.acceptsDropAt(0, 0), isFalse);
      expect(view.dragPathAt(0, 0), isNull);
    });

    test('sessionDirectory is exposed when provided', () {
      final view = TerminalPaneView(
        paneId: 'p1',
        title: 't',
        sessionDirectory: '/h/.supercli/app-sessions/s1',
      );
      expect(view.hasDropMaps, isTrue);
      expect(view.sessionDirectory, '/h/.supercli/app-sessions/s1');
      // No map files on disk: fail closed without needing the FFI.
      expect(view.acceptsDropAt(3, 5), isFalse);
      expect(view.dragPathAt(3, 5), isNull);
    });
  });
}
