/// End-to-end color proof for the TerminalPaneView port (row 184).
///
/// Feeds raw PTY bytes through [AnsiParser] into [TerminalState], then
/// through the RLE fallback renderer ([TerminalPane.buildFallback]) and
/// asserts the colors land on styled [UiText] runs — i.e. agent TUI colors
/// survive to the screen instead of being stripped to plaintext.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/terminal/ansi_parser.dart';
import 'package:supercli_app/terminal/terminal_pane.dart';
import 'package:supercli_app/terminal/terminal_state.dart';
import 'package:supercli_app/terminal/terminal_types.dart';
import 'package:test/test.dart';

TerminalState makeState({int cols = 10, int rows = 1}) =>
    TerminalState(cols: cols, rows: rows);

void main() {
  group('AnsiParser + fallback: colors survive to styled runs (row 184)', () {
    test(
      '256-color SGR word renders as a colored UiText run, not stripped text',
      () {
        final s = makeState();
        // Raw PTY bytes exactly as an agent TUI would emit them.
        AnsiParser().parse('\x1b[38;5;196mERROR\x1b[0m ok', s);
        // Hide the cursor: block inversion would repaint the cursor cell.
        s.setCursor(0, 0, CursorStyle.block, false);
        final pane = TerminalPane(paneId: 'p1', title: 'zsh', state: s);
        final fb = pane.buildFallback();
        final row = fb.children[0] as UiRow;
        final theme = TerminalTheme.dark;
        // 'ERROR' red + ' ok  ' default: exactly two styled runs.
        expect(row.children.length, 2);
        final red = row.children[0] as UiText;
        expect(red.text, 'ERROR');
        // Palette 196 resolves to #ff0000 in the dark theme — the color is
        // carried on the run, not stripped to plaintext.
        expect((red.style!.foreground as dynamic).toJson(), '#ff0000');
        final plain = row.children[1] as UiText;
        expect(plain.text, ' ok  ');
        // Default cells carry PaletteColor(7), which resolves to the theme's
        // white — still a styled run, not stripped.
        expect(
          (plain.style!.foreground as dynamic).toJson(),
          theme.resolve(const PaletteColor(7)),
        );
      },
    );

    test('truecolor SGR renders as a styled run with the RGB hex', () {
      final s = makeState(cols: 5);
      AnsiParser().parse('\x1b[38;2;255;0;0mRED\x1b[0m', s);
      s.setCursor(0, 0, CursorStyle.block, false);
      final pane = TerminalPane(paneId: 'p1', title: 'zsh', state: s);
      final fb = pane.buildFallback();
      final row = fb.children[0] as UiRow;
      expect(row.children.length, 2);
      final red = row.children[0] as UiText;
      expect(red.text, 'RED');
      expect((red.style!.foreground as dynamic).toJson(), '#ff0000');
    });
  });
}
