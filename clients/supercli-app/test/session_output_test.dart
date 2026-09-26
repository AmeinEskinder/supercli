/// Tests for the session output journal reader (attached pane live tail).
library;

import 'dart:io';

import 'package:supercli_app/platform_keys.dart';
import 'package:supercli_app/session_output.dart';
import 'package:test/test.dart';

void main() {
  group('stripAnsi', () {
    test('removes CSI color sequences', () {
      expect(stripAnsi('\x1B[31mred\x1B[0m plain'), 'red plain');
    });

    test('removes CSI cursor movement', () {
      expect(stripAnsi('a\x1B[2Kb'), 'ab');
    });

    test('removes OSC hyperlinks terminated by BEL', () {
      expect(
        stripAnsi('\x1B]8;;https://example.com\x07link\x1B]8;;\x07'),
        'link',
      );
    });

    test('removes OSC terminated by ESC backslash', () {
      expect(stripAnsi('\x1B]0;title\x1B\\text'), 'text');
    });

    test('removes single-character escapes', () {
      expect(stripAnsi('a\x1BMb'), 'ab');
    });

    test('drops BEL', () {
      expect(stripAnsi('a\x07b'), 'ab');
    });

    test('normalizes CRLF and lone CR to LF', () {
      expect(stripAnsi('a\r\nb\rc'), 'a\nb\nc');
    });

    test('expands tabs', () {
      expect(stripAnsi('a\tb'), 'a        b');
    });

    test('keeps plain text untouched', () {
      expect(
        stripAnsi('Hello from live session 2\n\$ '),
        'Hello from live session 2\n\$ ',
      );
    });

    test('handles truncated escape at end of input', () {
      expect(stripAnsi('text\x1B['), 'text');
    });
  });

  group('alignStart', () {
    test('mid-UTF-8 desiredStart falls back to the last safe boundary', () {
      // 'é' is 0xC3 0xA9; desiredStart=2 lands mid-char, so the last safe
      // boundary at or before it is 0 (matches supercli-attach semantics:
      // re-reading a few bytes is harmless).
      final window = [0x41, 0xC3, 0xA9, 0x42];
      expect(alignStart(window, 0, 2), 0);
    });

    test('desiredStart inside CSI falls back to the ESC start', () {
      final window = [0x0A, 0x1B, 0x5B, 0x33, 0x31, 0x6D, 0x58];
      // desiredStart=3 is inside `ESC[31m`; last safe boundary is 1 (the
      // ESC itself), so the whole sequence is re-read and stripped.
      expect(alignStart(window, 0, 3), 1);
    });

    test('keeps a clean newline boundary', () {
      final window = [0x61, 0x0A, 0x62, 0x63];
      expect(alignStart(window, 0, 2), 2);
    });

    test('desiredStart after a complete CSI stays at the sequence end', () {
      final window = [0x1B, 0x5B, 0x33, 0x31, 0x6D, 0x58];
      // The complete CSI `ESC[31m` ends at offset 5; 'X' adds no boundary,
      // so the last safe boundary at or before 6 is 5.
      expect(alignStart(window, 0, 6), 5);
    });
  });

  group('journal reads', () {
    late Directory tmp;

    setUp(() {
      tmp = Directory.systemTemp.createTempSync('session-output-test');
    });

    tearDown(() {
      tmp.deleteSync(recursive: true);
    });

    String writeJournal(String name, List<int> bytes) {
      final path = '${tmp.path}/$name/$outputBinName';
      Directory('${tmp.path}/$name').createSync();
      File(path).writeAsBytesSync(bytes);
      return path;
    }

    test('snapshot returns tail and end offset', () {
      final path = writeJournal('s1', 'line1\nline2\nline3\n'.codeUnits);
      final read = readSnapshotTailAt(path, maxBytes: 6);
      expect(read, isNotNull);
      expect(String.fromCharCodes(read!.bytes), 'line3\n');
      expect(read.nextOffset, 'line1\nline2\nline3\n'.length);
    });

    test('snapshot of empty journal', () {
      final path = writeJournal('s2', []);
      final read = readSnapshotTailAt(path);
      expect(read, isNotNull);
      expect(read!.bytes, isEmpty);
      expect(read.nextOffset, 0);
    });

    test('snapshot clamps to retained_from', () {
      final dir = Directory('${tmp.path}/s3')..createSync();
      final path = '${dir.path}/$outputBinName';
      File(path).writeAsBytesSync('0123456789'.codeUnits);
      File('${dir.path}/$outputRetentionName')
          .writeAsStringSync('{"version":4,"retained_from":8}');
      final read = readSnapshotTailAt(path, maxBytes: 100);
      expect(read, isNotNull);
      expect(String.fromCharCodes(read!.bytes), '89');
    });

    test('tail reads only new bytes', () {
      final path = writeJournal('s4', 'hello\n'.codeUnits);
      final first = readSnapshotTailAt(path)!;
      File(path).writeAsBytesSync('hello\nworld\n'.codeUnits);
      final tail = readTailAt(path, first.nextOffset);
      expect(tail, isNotNull);
      expect(String.fromCharCodes(tail!.bytes), 'world\n');
      expect(tail.nextOffset, 'hello\nworld\n'.length);
    });

    test('tail with no new bytes is empty', () {
      final path = writeJournal('s5', 'hello\n'.codeUnits);
      final first = readSnapshotTailAt(path)!;
      final tail = readTailAt(path, first.nextOffset);
      expect(tail, isNotNull);
      expect(tail!.bytes, isEmpty);
    });

    test('tail after shrink re-snapshots', () {
      final path = writeJournal('s6', 'hello world\n'.codeUnits);
      File(path).writeAsBytesSync('new\n'.codeUnits);
      final tail = readTailAt(path, 999);
      expect(tail, isNotNull);
      expect(String.fromCharCodes(tail!.bytes), 'new\n');
    });

    test('missing journal returns null', () {
      expect(readSnapshotTailAt('${tmp.path}/nope/$outputBinName'), isNull);
      expect(readTailAt('${tmp.path}/nope/$outputBinName', 0), isNull);
    });

    test('decodeOutputText strips ANSI from journal bytes', () {
      final bytes = '\x1B[32mHello from live session 2\x1B[0m\n'.codeUnits;
      expect(decodeOutputText(bytes), 'Hello from live session 2\n');
    });
  });

  group('platform_keys', () {
    test('primaryModifier maps macOS to meta, others to ctrl', () {
      expect(primaryModifier(isMacOS: true), 'meta');
      expect(primaryModifier(isMacOS: false), 'ctrl');
    });
  });
}
