/// Behavior tests for the P0-8 terminal pane.
///
/// These exercise the terminal state model, not just tree shape:
/// damage application, scrollback, cursor styles, selection/copy,
/// ANSI 256 + truecolor resolution, run-length-encoded rendering,
/// and the keyboard input map.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/terminal/terminal_pane.dart';
import 'package:supercli_app/terminal/terminal_state.dart';
import 'package:test/test.dart';

TerminalState makeState({int cols = 10, int rows = 4}) =>
    TerminalState(cols: cols, rows: rows);

void main() {
  group('TerminalTheme 256-color palette', () {
    final theme = TerminalTheme.dark;

    test('ANSI 0-7 resolve from theme', () {
      expect(theme.resolve(TerminalColor.palette(1)), '#cc0000');
      expect(theme.resolve(TerminalColor.palette(7)), '#d3d7cf');
    });

    test('bright ANSI 8-15 resolve from theme', () {
      expect(theme.resolve(TerminalColor.palette(9)), '#ef2929');
    });

    test('6x6x6 cube: 16=black, 231=white, 196=red', () {
      expect(theme.resolve(TerminalColor.palette(16)), '#000000');
      expect(theme.resolve(TerminalColor.palette(231)), '#ffffff');
      expect(theme.resolve(TerminalColor.palette(196)), '#ff0000');
    });

    test('grayscale ramp: 232 near-black, 255 near-white', () {
      expect(theme.resolve(TerminalColor.palette(232)), '#080808');
      expect(theme.resolve(TerminalColor.palette(255)), '#eeeeee');
    });

    test('truecolor passes through', () {
      expect(
        theme.resolve(TerminalColor.rgb(0x12, 0x34, 0x56)),
        '#123456',
      );
    });

    test('palette index out of range throws', () {
      expect(() => TerminalColor.palette(256), throwsArgumentError);
      expect(() => TerminalColor.palette(-1), throwsArgumentError);
    });
  });

  group('TerminalState.updateCells', () {
    test('applies dirty cells and returns damage ranges', () {
      final s = makeState();
      final damage = s.updateCells([
        TerminalCellUpdate(1, 2, const TerminalCell(char: 'x')),
        TerminalCellUpdate(1, 3, const TerminalCell(char: 'y')),
        TerminalCellUpdate(2, 0, const TerminalCell(char: 'z')),
      ]);
      expect(s.grid[1][2].char, 'x');
      expect(s.grid[1][3].char, 'y');
      expect(s.grid[2][0].char, 'z');
      // Untouched cells keep the default blank.
      expect(s.grid[0][0].char, ' ');
      // Damage merged per row: row 1 cols [2,4), row 2 cols [0,1).
      expect(damage.ranges, [(1, 2, 4), (2, 0, 1)]);
    });

    test('out-of-bounds updates are ignored', () {
      final s = makeState();
      final damage = s.updateCells([
        TerminalCellUpdate(99, 0, const TerminalCell(char: 'x')),
        TerminalCellUpdate(0, 99, const TerminalCell(char: 'y')),
      ]);
      expect(damage.ranges, isEmpty);
    });

    test('new output snaps the viewport back to live', () {
      final s = makeState();
      s.scrollback.add(
          List.filled(10, const TerminalCell(char: 's'), growable: false));
      s.scrollTo(1);
      expect(s.scrollOffset, 1);
      s.updateCells([TerminalCellUpdate(0, 0, const TerminalCell(char: 'n'))]);
      expect(s.scrollOffset, 0);
    });
  });

  group('TerminalState scrollback', () {
    test('scrollUp pushes lines into scrollback, capped', () {
      final s = TerminalState(cols: 4, rows: 2, scrollbackLines: 3);
      s.writeString('aaaa\nbbbb\ncccc\ndddd\n');
      // 4 lines + trailing newline into a 2-row grid -> 3 lines scrolled.
      expect(s.scrollback.length, 3);
      expect(s.scrollback[0].map((c) => c.char).join(), 'aaaa');
      expect(s.grid[0].map((c) => c.char).join(), 'dddd');
      expect(s.grid[1].map((c) => c.char).join(), '    ');
    });

    test('scrollback cap drops oldest lines', () {
      final s = TerminalState(cols: 2, rows: 1, scrollbackLines: 2);
      s.writeString('a\nb\nc\nd\n');
      expect(s.scrollback.length, 2);
      expect(s.scrollback[0].map((c) => c.char).join().trim(), 'c');
      expect(s.scrollback[1].map((c) => c.char).join().trim(), 'd');
    });

    test('scrollTo clamps to available scrollback', () {
      final s = makeState();
      s.scrollTo(99);
      expect(s.scrollOffset, 0);
      s.scrollback.add(
          List.filled(10, const TerminalCell(), growable: false));
      s.scrollTo(99);
      expect(s.scrollOffset, 1);
    });
  });

  group('TerminalState cursor', () {
    test('setCursor clamps to grid', () {
      final s = makeState();
      s.setCursor(999, 999, CursorStyle.bar, true);
      expect(s.cursor.col, 9);
      expect(s.cursor.row, 3);
      expect(s.cursor.style, CursorStyle.bar);
    });

    test('cursor style round-trips through the node', () {
      final s = makeState();
      s.setCursor(2, 1, CursorStyle.underline, true);
      final node = s.buildNode('t1');
      expect(node.cursor.col, 2);
      expect(node.cursor.row, 1);
      expect(node.cursor.style, CursorStyle.underline);
      final json = node.toJson();
      expect((json['cursor'] as Map)['style'], 'underline');
    });
  });

  group('TerminalState selection and copy', () {
    test('copySelection returns selected text', () {
      final s = makeState();
      s.writeString('hello\nworld\n');
      s.select(1, 0, 3, 0);
      expect(s.copySelection(), 'ell');
    });

    test('multi-line selection joins lines', () {
      final s = makeState();
      s.writeString('hello\nworld\n');
      s.select(3, 0, 1, 1);
      expect(s.copySelection(), 'lo\nwo');
    });

    test('no selection copies nothing', () {
      final s = makeState();
      expect(s.copySelection(), isEmpty);
    });
  });

  group('UiTerminal node', () {
    test('buildNode carries grid, theme, and dimensions', () {
      final s = makeState(cols: 80, rows: 24);
      final node = s.buildNode('term-1');
      expect(node, isA<UiTerminal>());
      expect(node.cols, 80);
      expect(node.rows, 24);
      expect(node.cells.length, 24);
      expect(node.cells[0].length, 80);
      expect(node.fontFamily, isNotEmpty);
      expect(node.scrollbackLines, 10000);
    });

    test('node JSON has the terminal kind and cell data', () {
      final s = makeState();
      s.writeString('hi', fg: TerminalColor.palette(196));
      final json = s.buildNode('t1').toJson();
      expect(json['kind'], 'terminal');
      expect(json['cols'], 10);
      final cells = json['cells'] as List;
      final first = (cells[0] as List)[0] as Map;
      expect(first['char'], 'h');
      expect((first['fg'] as Map)['palette'], 196);
    });

    test('selection serializes when present', () {
      final s = makeState();
      s.select(0, 0, 2, 0);
      final json = s.buildNode('t1').toJson();
      expect(json.containsKey('selection'), isTrue);
    });
  });

  group('TerminalPane fallback rendering', () {
    test('run-length encoding merges same-attribute cells', () {
      final s = makeState(cols: 6, rows: 1);
      // 'aaa' red, 'bbb' default: two runs expected.
      s.updateCells([
        for (var c = 0; c < 3; c++)
          TerminalCellUpdate(
              0, c, TerminalCell(char: 'a', fg: TerminalColor.palette(1))),
        for (var c = 3; c < 6; c++)
          TerminalCellUpdate(0, c, const TerminalCell(char: 'b')),
      ]);
      final pane = TerminalPane(paneId: 'p1', title: 'zsh', state: s);
      final fb = pane.buildFallback();
      final row = fb.children[0] as UiRow;
      expect(row.children.length, 2);
      final first = row.children[0] as UiText;
      expect(first.text, 'aaa');
      expect(first.style!.foreground, isNotNull);
    });

    test('truecolor cell resolves to hex in the fallback', () {
      final s = makeState(cols: 2, rows: 1);
      s.updateCells([
        TerminalCellUpdate(
            0, 0, TerminalCell(char: 'x', fg: TerminalColor.rgb(1, 2, 3))),
      ]);
      // Park the cursor off the tested cell so its inversion doesn't interfere.
      s.setCursor(1, 0, CursorStyle.block, true);
      final pane = TerminalPane(paneId: 'p1', title: 'zsh', state: s);
      final fb = pane.buildFallback();
      final row = fb.children[0] as UiRow;
      final first = row.children[0] as UiText;
      // '#010203' hex for the truecolor fg.
      expect(
        (first.style!.foreground as dynamic).toJson(),
        '#010203',
      );
    });

    test('block cursor inverts the cursor cell', () {
      final s = makeState(cols: 3, rows: 1);
      s.setCursor(1, 0, CursorStyle.block, true);
      final pane = TerminalPane(paneId: 'p1', title: 'zsh', state: s);
      final fb = pane.buildFallback();
      final row = fb.children[0] as UiRow;
      // All cells default attr -> one run; cursor cell inverted inside it.
      // The run keeps one node; the inversion is per-cell style, so with
      // RLE the cursor run splits out.
      expect(row.children.length, greaterThanOrEqualTo(1));
      // Find the run containing the inverted cursor cell: its background
      // must differ from the plain background.
      final theme = TerminalTheme.dark;
      var foundInverted = false;
      for (final child in row.children) {
        final t = child as UiText;
        final bg = (t.style!.background as dynamic).toJson() as String;
        if (bg != theme.background) foundInverted = true;
      }
      expect(foundInverted, isTrue);
    });

    test('selection highlighting overrides cell colors', () {
      final s = makeState(cols: 4, rows: 1);
      s.writeString('abcd');
      s.select(1, 0, 2, 0);
      // Hide the cursor: its block inversion would repaint the cursor cell
      // over the selection highlight.
      s.setCursor(0, 0, CursorStyle.block, false);
      final pane = TerminalPane(paneId: 'p1', title: 'zsh', state: s);
      final fb = pane.buildFallback();
      final row = fb.children[0] as UiRow;
      final theme = TerminalTheme.dark;
      final selected = row.children.where((c) {
        final t = c as UiText;
        return (t.style!.background as dynamic).toJson() ==
            theme.selectionBackground;
      });
      expect(selected, isNotEmpty);
    });

    test('build() composes header + terminal node + fallback', () {
      final s = makeState();
      final pane = TerminalPane(paneId: 'p1', title: 'zsh', state: s);
      final node = pane.build() as UiColumn;
      expect(node.children[0], isA<UiRow>()); // header
      expect(node.children[1], isA<UiTerminal>()); // P0-8 node
      expect(node.children[2], isA<UiColumn>()); // fallback grid
    });
  });

  group('TerminalKeymap', () {
    const keymap = TerminalKeymap();

    test('arrows produce CSI sequences', () {
      expect(keymap.sequenceFor('up', {}), '\x1b[A');
      expect(keymap.sequenceFor('down', {}), '\x1b[B');
      expect(keymap.sequenceFor('left', {}), '\x1b[D');
    });

    test('function keys produce correct sequences', () {
      expect(keymap.sequenceFor('f1', {}), '\x1bOP');
      expect(keymap.sequenceFor('f5', {}), '\x1b[15~');
    });

    test('enter/tab/backspace/escape', () {
      expect(keymap.sequenceFor('enter', {}), '\r');
      expect(keymap.sequenceFor('tab', {}), '\t');
      expect(keymap.sequenceFor('backspace', {}), '\x7f');
      expect(keymap.sequenceFor('escape', {}), '\x1b');
    });

    test('ctrl+letter produces control character', () {
      expect(keymap.sequenceFor('c', {'ctrl'}), '\x03');
      expect(keymap.sequenceFor('d', {'ctrl'}), '\x04');
    });

    test('unknown key returns null', () {
      expect(keymap.sequenceFor('f13', {}), isNull);
    });

    test('actionsFor declares bindings scoped to the terminal node', () {
      final actions = keymap.actionsFor('terminal-p1');
      expect(actions, isNotEmpty);
      expect(actions.every((a) => a.name.startsWith('terminal.key.')), isTrue);
    });
  });

  group('TerminalPane input handling', () {
    test('handleKeyAction forwards bytes to onInput', () {
      final s = makeState();
      final received = <List<int>>[];
      final pane = TerminalPane(
        paneId: 'p1',
        title: 'zsh',
        state: s,
        onInput: received.add,
      );
      expect(pane.handleKeyAction('terminal.key.up', {}), isTrue);
      expect(received.single, '\x1b[A'.codeUnits);
    });

    test('unknown action is not consumed', () {
      final s = makeState();
      var called = false;
      final pane = TerminalPane(
        paneId: 'p1',
        title: 'zsh',
        state: s,
        onInput: (_) => called = true,
      );
      expect(pane.handleKeyAction('sidebar.focus', {}), isFalse);
      expect(called, isFalse);
    });

    test('copySelection fires onCopy with selected text', () {
      final s = makeState();
      s.writeString('hello');
      s.select(0, 0, 4, 0);
      String? copied;
      final pane = TerminalPane(
        paneId: 'p1',
        title: 'zsh',
        state: s,
        onCopy: (t) => copied = t,
      );
      pane.copySelection();
      expect(copied, 'hello');
    });
  });
}
