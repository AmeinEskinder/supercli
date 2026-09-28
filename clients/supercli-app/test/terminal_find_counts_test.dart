/// Tests for [terminalFindCountLabel].
///
/// Port of the `updateCounts(total:selected:)` behaviour from
/// `TerminalFindBar.swift` in `native/SupercliNative/Sources/SupercliNative`.
library;

import 'package:test/test.dart';

import '../lib/terminal/terminal_find_counts.dart';

void main() {
  group('terminalFindCountLabel', () {
    test('empty query hides the label', () {
      expect(
        terminalFindCountLabel(query: '', total: 17, selected: 2),
        '',
      );
      expect(
        terminalFindCountLabel(query: '', total: null, selected: null),
        '',
      );
    });

    test('unknown total hides the label', () {
      expect(
        terminalFindCountLabel(query: 'foo', total: null, selected: 0),
        '',
      );
    });

    test('zero matches shows "No results"', () {
      expect(
        terminalFindCountLabel(query: 'foo', total: 0, selected: null),
        'No results',
      );
    });

    test('selected match shows 1-based "N of total"', () {
      expect(
        terminalFindCountLabel(query: 'foo', total: 17, selected: 2),
        '3 of 17',
      );
      expect(
        terminalFindCountLabel(query: 'foo', total: 1, selected: 0),
        '1 of 1',
      );
    });

    test('no selection shows the bare total', () {
      expect(
        terminalFindCountLabel(query: 'foo', total: 17, selected: null),
        '17',
      );
    });

    test('negative selection shows the bare total', () {
      expect(
        terminalFindCountLabel(query: 'foo', total: 17, selected: -1),
        '17',
      );
    });
  });

  group('TerminalFindNotifications', () {
    test('names match the Swift notification names', () {
      expect(TerminalFindNotifications.find, 'supercli.terminal.find');
      expect(TerminalFindNotifications.findNext, 'supercli.terminal.find-next');
      expect(
        TerminalFindNotifications.findPrevious,
        'supercli.terminal.find-previous',
      );
    });
  });
}
