/// ANSI/VT escape sequence parser for the P0-8 terminal pane.
///
/// Feeds raw terminal bytes (from the Host's `GET /api/sessions/{id}/output`)
/// into a [TerminalState], interpreting the VT sequences the Host's PTY
/// emits: printable characters, C0 controls, and CSI sequences (cursor
/// movement, erase, SGR colors).
///
/// This is the Dart-side equivalent of what Ghostty's `ghostty-vt` does on
/// the Rust Host for the local attach path. The Host streams raw PTY bytes;
/// the parser reconstructs the grid locally so the RLE fallback renderer
/// shows real colors, not stripped plaintext.
///
/// Supported:
/// - Printable characters (UTF-8 aware via [String] decoding by the caller;
///   this parser takes a [String] and handles code points)
/// - C0: `\n` (LF), `\r` (CR), `\b` (BS), `\t` (TAB), `\x07` (BEL, ignored)
/// - CSI `ESC [`:
///   - Cursor: `A` (up), `B` (down), `C` (forward), `D` (back),
///     `H`/`f` (position), `G` (column)
///   - Erase: `J` (display), `K` (line)
///   - SGR `m`: 0 (reset), 1 (bold), 22 (bold off), 30-37/90-97 (fg),
///     40-47/100-107 (bg), 39/49 (default fg/bg),
///     38;5;N / 48;5;N (256-color), 38;2;R;G;B / 48;2;R;G;B (truecolor)
/// - Unknown/unsupported sequences are skipped, never crash.
library;

import 'terminal_state.dart';
import 'terminal_types.dart';

/// Parses ANSI/VT byte streams into [TerminalState] updates.
///
/// Stateful: SGR attributes persist across [parse] calls, and a partial
/// escape sequence at the end of a chunk is buffered until the next chunk
/// completes it.
final class AnsiParser {
  AnsiParser();

  // -- SGR state ---------------------------------------------------------
  TerminalColor _fg = const PaletteColor(7);
  TerminalColor _bg = const PaletteColor(0);
  bool _bold = false;

  // -- Partial sequence buffer --------------------------------------------
  final StringBuffer _pending = StringBuffer();

  TerminalColor get currentFg => _fg;
  TerminalColor get currentBg => _bg;
  bool get currentBold => _bold;

  /// Parse [text] (a chunk of terminal output) into [state].
  ///
  /// The chunk may end mid-escape-sequence; the remainder is buffered and
  /// completed by the next [parse] call.
  void parse(String text, TerminalState state) {
    var s = text;
    if (_pending.isNotEmpty) {
      _pending.write(s);
      s = _pending.toString();
      _pending.clear();
    }

    var i = 0;
    while (i < s.length) {
      final ch = s[i];
      if (ch == '\x1b') {
        // Try to consume a full escape sequence starting here.
        final consumed = _consumeEscape(s, i, state);
        if (consumed < 0) {
          // Incomplete: buffer the rest for the next chunk.
          _pending.write(s.substring(i));
          break;
        }
        i += consumed;
        continue;
      }
      if (ch == '\n') {
        _lineFeed(state);
        i++;
        continue;
      }
      if (ch == '\r') {
        state.setCursor(0, state.cursor.row, state.cursor.style, state.cursor.visible);
        i++;
        continue;
      }
      if (ch == '\b') {
        final c = state.cursor;
        if (c.col > 0) {
          state.setCursor(c.col - 1, c.row, c.style, c.visible);
        }
        i++;
        continue;
      }
      if (ch == '\t') {
        final c = state.cursor;
        final next = ((c.col ~/ 8) + 1) * 8;
        state.setCursor(
            next.clamp(0, state.cols - 1), c.row, c.style, c.visible);
        i++;
        continue;
      }
      if (ch == '\x07') {
        // BEL: no visual effect in the grid.
        i++;
        continue;
      }
      // Printable: write at cursor with current SGR attributes.
      _writeChar(state, ch);
      i++;
    }
  }

  void _writeChar(TerminalState state, String ch) {
    var col = state.cursor.col;
    var row = state.cursor.row;
    if (col >= state.cols) {
      col = 0;
      row++;
    }
    if (row >= state.rows) {
      state.scrollUp(row - state.rows + 1);
      row = state.rows - 1;
    }
    state.updateCells([
      TerminalCellUpdate(
        row,
        col,
        TerminalCell(char: ch, fg: _fg, bg: _bg, bold: _bold),
      ),
    ]);
    final c = state.cursor;
    state.setCursor(col + 1, row, c.style, c.visible);
  }

  void _lineFeed(TerminalState state) {
    final c = state.cursor;
    var row = c.row + 1;
    if (row >= state.rows) {
      state.scrollUp(1);
      row = state.rows - 1;
    }
    state.setCursor(0, row, c.style, c.visible);
  }

  /// Consume an escape sequence starting at `s[i]` (which is ESC).
  /// Returns the number of chars consumed, or -1 if the sequence is
  /// incomplete (needs more input).
  int _consumeEscape(String s, int i, TerminalState state) {
    if (i + 1 >= s.length) return -1;
    final next = s[i + 1];
    if (next == '[') {
      return _consumeCsi(s, i, state);
    }
    if (next == '(' || next == ')' || next == '#') {
      // Charset selection / DEC line drawing: skip 3 chars (ESC ( X).
      if (i + 2 >= s.length) return -1;
      return 3;
    }
    if (next == 'M') {
      // RI: reverse index (cursor up, scroll down if at top).
      final c = state.cursor;
      if (c.row > 0) {
        state.setCursor(c.col, c.row - 1, c.style, c.visible);
      }
      return 2;
    }
    if (next == 'c') {
      // RIS: full reset — clear grid and SGR.
      _resetSgr();
      for (var r = 0; r < state.rows; r++) {
        state.updateCells([
          for (var col = 0; col < state.cols; col++)
            TerminalCellUpdate(r, col, const TerminalCell()),
        ]);
      }
      final c = state.cursor;
      state.setCursor(0, 0, c.style, c.visible);
      return 2;
    }
    // Unknown single-char escape: skip ESC + char.
    return 2;
  }

  /// Consume a CSI sequence starting at `s[i]` (ESC [ ...).
  /// Returns chars consumed, or -1 if incomplete.
  int _consumeCsi(String s, int i, TerminalState state) {
    // Find the final byte (0x40-0x7E).
    var j = i + 2;
    while (j < s.length) {
      final c = s.codeUnitAt(j);
      if (c >= 0x40 && c <= 0x7E) break;
      j++;
    }
    if (j >= s.length) return -1; // incomplete
    final params = s.substring(i + 2, j);
    final finalChar = s[j];
    _applyCsi(finalChar, params, state);
    return j - i + 1;
  }

  void _applyCsi(String finalChar, String params, TerminalState state) {
    final c = state.cursor;
    List<int> args() {
      if (params.isEmpty) return const [];
      return params.split(';').map((p) => int.tryParse(p) ?? 0).toList();
    }

    switch (finalChar) {
      case 'A': // CUU: cursor up
        final n = args().isEmpty ? 1 : args()[0];
        state.setCursor(c.col, (c.row - n).clamp(0, state.rows - 1),
            c.style, c.visible);
      case 'B': // CUD: cursor down
        final n = args().isEmpty ? 1 : args()[0];
        state.setCursor(c.col, (c.row + n).clamp(0, state.rows - 1),
            c.style, c.visible);
      case 'C': // CUF: cursor forward
        final n = args().isEmpty ? 1 : args()[0];
        state.setCursor((c.col + n).clamp(0, state.cols - 1), c.row,
            c.style, c.visible);
      case 'D': // CUB: cursor back
        final n = args().isEmpty ? 1 : args()[0];
        state.setCursor((c.col - n).clamp(0, state.cols - 1), c.row,
            c.style, c.visible);
      case 'H':
      case 'f': // CUP: cursor position (1-based)
        final a = args();
        final row = (a.isEmpty ? 1 : a[0]) - 1;
        final col = (a.length < 2 ? 1 : a[1]) - 1;
        state.setCursor(col.clamp(0, state.cols - 1),
            row.clamp(0, state.rows - 1), c.style, c.visible);
      case 'G': // CHA: cursor horizontal absolute (1-based)
        final a = args();
        final col = (a.isEmpty ? 1 : a[0]) - 1;
        state.setCursor(
            col.clamp(0, state.cols - 1), c.row, c.style, c.visible);
      case 'J': // ED: erase display
        final a = args();
        final mode = a.isEmpty ? 0 : a[0];
        _eraseDisplay(state, mode);
      case 'K': // EL: erase line
        final a = args();
        final mode = a.isEmpty ? 0 : a[0];
        _eraseLine(state, mode);
      case 'm': // SGR: select graphic rendition
        _applySgr(args());
      case 'h':
      case 'l':
        // DECSET/DECRST: show/hide cursor etc. Only honor 25 (cursor visible).
        if (params == '?25') {
          state.setCursor(c.col, c.row, c.style, finalChar == 'h');
        }
      // Explicitly ignored (no grid effect): device reports, scrolling
      // regions, alternate screen. Unknown finals fall through silently.
    }
  }

  void _eraseDisplay(TerminalState state, int mode) {
    final c = state.cursor;
    final blank = const TerminalCell();
    switch (mode) {
      case 0: // cursor to end
        _eraseLine(state, 0);
        for (var r = c.row + 1; r < state.rows; r++) {
          state.updateCells([
            for (var col = 0; col < state.cols; col++)
              TerminalCellUpdate(r, col, blank),
          ]);
        }
      case 1: // start to cursor
        for (var r = 0; r < c.row; r++) {
          state.updateCells([
            for (var col = 0; col < state.cols; col++)
              TerminalCellUpdate(r, col, blank),
          ]);
        }
        _eraseLine(state, 1);
      case 2: // entire display
      case 3: // entire display + scrollback
        for (var r = 0; r < state.rows; r++) {
          state.updateCells([
            for (var col = 0; col < state.cols; col++)
              TerminalCellUpdate(r, col, blank),
          ]);
        }
        if (mode == 3) state.scrollback.clear();
    }
  }

  void _eraseLine(TerminalState state, int mode) {
    final c = state.cursor;
    final blank = const TerminalCell();
    final row = c.row;
    int from, to;
    switch (mode) {
      case 0: // cursor to end
        from = c.col;
        to = state.cols;
      case 1: // start to cursor
        from = 0;
        to = c.col + 1;
      case 2: // entire line
        from = 0;
        to = state.cols;
      default:
        return;
    }
    state.updateCells([
      for (var col = from; col < to; col++)
        TerminalCellUpdate(row, col, blank),
    ]);
  }

  void _applySgr(List<int> args) {
    if (args.isEmpty) {
      _resetSgr();
      return;
    }
    var i = 0;
    while (i < args.length) {
      final p = args[i];
      if (p == 0) {
        _resetSgr();
      } else if (p == 1) {
        _bold = true;
      } else if (p == 22) {
        _bold = false;
      } else if (p >= 30 && p <= 37) {
        _fg = PaletteColor(p - 30);
      } else if (p >= 90 && p <= 97) {
        _fg = PaletteColor(p - 90 + 8);
      } else if (p == 39) {
        _fg = const PaletteColor(7);
      } else if (p >= 40 && p <= 47) {
        _bg = PaletteColor(p - 40);
      } else if (p >= 100 && p <= 107) {
        _bg = PaletteColor(p - 100 + 8);
      } else if (p == 49) {
        _bg = const PaletteColor(0);
      } else if (p == 38 || p == 48) {
        // Extended color: 38;5;N / 38;2;R;G;B (fg), 48;... (bg).
        final isFg = p == 38;
        if (i + 1 < args.length) {
          final mode = args[i + 1];
          if (mode == 5 && i + 2 < args.length) {
            final color = PaletteColor(args[i + 2].clamp(0, 255));
            if (isFg) {
              _fg = color;
            } else {
              _bg = color;
            }
            i += 2;
          } else if (mode == 2 && i + 4 < args.length) {
            final color = RgbColor(
              args[i + 2].clamp(0, 255),
              args[i + 3].clamp(0, 255),
              args[i + 4].clamp(0, 255),
            );
            if (isFg) {
              _fg = color;
            } else {
              _bg = color;
            }
            i += 4;
          }
        }
      }
      // 2 (dim), 3/23 (italic), 4/24 (underline), 7/27 (reverse),
      // 9/29 (strikethrough) intentionally unmodeled: TerminalCell has no
      // slot for them, and they must not disturb fg/bg/bold.
      i++;
    }
  }

  void _resetSgr() {
    _fg = const PaletteColor(7);
    _bg = const PaletteColor(0);
    _bold = false;
  }
}
