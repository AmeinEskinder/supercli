/// Tests for the terminal query filter (port of TerminalQueryFilter.swift).
library;

import 'package:supercli_app/ios/query_filter.dart';
import 'package:test/test.dart';

List<int> bytes(String s) => s.codeUnits;
String text(List<int> b) => String.fromCharCodes(b);

void main() {
  group('TerminalQueryFilter', () {
    test('passes plain text through untouched', () {
      final filter = TerminalQueryFilter();
      const plain = 'hello world\r\n\$ ls -la\r\n';
      expect(text(filter.stripRequests(bytes(plain))), plain);
    });

    test('strips device attribute queries (CSI c)', () {
      final filter = TerminalQueryFilter();
      final out = filter.stripRequests(
        bytes('before\x1b[c\x1b[>c\x1b[?c after'),
      );
      expect(text(out), 'before after');
    });

    test('strips device status report queries (CSI n)', () {
      final filter = TerminalQueryFilter();
      final out = filter.stripRequests(bytes('a\x1b[6nb'));
      expect(text(out), 'ab');
    });

    test('strips XTVERSION but preserves DECSCUSR', () {
      final filter = TerminalQueryFilter();
      // XTVERSION: CSI > … q — stripped.
      expect(text(filter.stripRequests(bytes('x\x1b[>0qy'))), 'xy');
      // DECSCUSR: CSI … SP q (cursor style) — preserved.
      const decscusr = '\x1b[2 q';
      expect(
        text(filter.stripRequests(bytes('x${decscusr}y'))),
        'x${decscusr}y',
      );
    });

    test('strips DECRQM but preserves DECSTR', () {
      final filter = TerminalQueryFilter();
      // DECRQM: CSI … $ p — stripped.
      expect(text(filter.stripRequests(bytes('x\x1b[?2026\$py'))), 'xy');
      // DECSTR: CSI ! p — preserved.
      const decstr = '\x1b[!p';
      expect(text(filter.stripRequests(bytes('x${decstr}y'))), 'x${decstr}y');
    });

    test('preserves ESC c (RIS)', () {
      final filter = TerminalQueryFilter();
      const ris = '\x1bc';
      expect(text(filter.stripRequests(bytes('x${ris}y'))), 'x${ris}y');
    });

    test('strips DCS XTGETTCAP / DECRQSS queries', () {
      final filter = TerminalQueryFilter();
      // XTGETTCAP: ESC P +q … ST
      final out = filter.stripRequests(bytes('a\x1bP+q544e\x1b\\b'));
      expect(text(out), 'ab');
      // DECRQSS: ESC P $q … BEL
      final out2 = filter.stripRequests(bytes('a\x1bP\$q0;1\x07b'));
      expect(text(out2), 'ab');
    });

    test('withholds a query split across chunks', () {
      final filter = TerminalQueryFilter();
      // First chunk ends mid-query: nothing query-shaped may leak, and the
      // withheld prefix must not be emitted yet.
      final first = filter.stripRequests(bytes('hello\x1b[>'));
      expect(text(first), 'hello');
      // Second chunk completes the XTVERSION query: the whole thing is gone.
      final second = filter.stripRequests(bytes('0qworld'));
      expect(text(second), 'world');
    });

    test('withholds a lone trailing ESC', () {
      final filter = TerminalQueryFilter();
      expect(text(filter.stripRequests(bytes('abc\x1b'))), 'abc');
      // The lone ESC turns out to start RIS ('ESC c'): ESC + other is
      // preserved, not swallowed.
      expect(text(filter.stripRequests(bytes('cdef'))), '\x1bcdef');
    });

    test('drops oversized withheld CSI runs instead of emitting them', () {
      final filter = TerminalQueryFilter();
      // Parameter bytes ('1;') are not final bytes, so this CSI run is
      // unterminated. Longer than the carry cap, it is not a real query —
      // it must be dropped, not re-emitted as a hazard.
      final huge = '1;' * 128;
      final out = filter.stripRequests(bytes('\x1b[$huge'));
      expect(text(out), isEmpty);
      // Stream continues cleanly afterwards.
      expect(text(filter.stripRequests(bytes('ok'))), 'ok');
    });

    test('discards unterminated DCS query payload chunk by chunk', () {
      final filter = TerminalQueryFilter();
      // DCS XTGETTCAP with no terminator in this chunk: payload swallowed.
      expect(text(filter.stripRequests(bytes('a\x1bP+q544epayload'))), 'a');
      // More payload, still unterminated: still swallowed.
      expect(text(filter.stripRequests(bytes('morepayload'))), isEmpty);
      // Terminator arrives: discard ends, following text passes through.
      expect(text(filter.stripRequests(bytes('end\x07tail'))), 'tail');
    });

    test('split ST across chunks ends the DCS discard', () {
      final filter = TerminalQueryFilter();
      expect(text(filter.stripRequests(bytes('a\x1bP+q544epay\x1b'))), 'a');
      // Second chunk completes the split ST (ESC \); discard ends there.
      expect(text(filter.stripRequests(bytes('\\tail'))), 'tail');
    });

    test('reset clears carried state', () {
      final filter = TerminalQueryFilter();
      filter.stripRequests(bytes('abc\x1b[>'));
      filter.reset();
      // After reset the stream restarts from scratch: the withheld prefix
      // is gone and plain text flows.
      expect(text(filter.stripRequests(bytes('0q'))), '0q');
    });

    test('non-query CSI sequences pass through', () {
      final filter = TerminalQueryFilter();
      // Cursor movement, colors, erase — all preserved byte-for-byte.
      const seqs = '\x1b[2J\x1b[H\x1b[31m\x1b[0K';
      expect(text(filter.stripRequests(bytes('x${seqs}y'))), 'x${seqs}y');
    });
  });
}
