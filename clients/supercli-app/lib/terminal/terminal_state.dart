/// Mutable terminal pane state (P0-8).
///
/// The Rust host owns the ghostty-vt screen state, diffs it, and pushes dirty
/// cells. This class applies those updates to a local grid, maintains the
/// scrollback buffer, cursor, and selection, and rebuilds an immutable
/// `UiTerminal` snapshot node for the native renderer.
///
/// Data flow:
///   host damage (list of `TerminalCellUpdate`)
///     -> `updateCells` -> `TerminalDamage` ranges
///     -> `buildNode` -> UiTerminal -> host.publish()
library;

import 'package:gpuidart/gpuidart.dart';

/// One dirty cell pushed by the host's ghostty-vt diff.
final class TerminalCellUpdate {
  const TerminalCellUpdate(this.row, this.col, this.cell);
  final int row;
  final int col;
  final TerminalCell cell;
}

/// Mutable state for one terminal pane.
class TerminalState {
  TerminalState({
    required this.cols,
    required this.rows,
    this.scrollbackLines = 10000,
    TerminalTheme? theme,
    this.fontFamily = 'JetBrains Mono',
    this.fontSize = 13.0,
    this.lineHeight = 1.2,
  }) : theme = theme ?? TerminalTheme.dark {
    _grid = List.generate(
      rows,
      (_) => List.filled(cols, const TerminalCell(), growable: false),
    );
  }

  final int cols;
  final int rows;
  final int scrollbackLines;
  final TerminalTheme theme;
  final String fontFamily;
  final double fontSize;
  final double lineHeight;

  late final List<List<TerminalCell>> _grid;

  /// Lines scrolled off the top of the grid, oldest first.
  /// Capped at [scrollbackLines].
  final List<List<TerminalCell>> scrollback = [];

  /// Viewport offset into the scrollback: 0 = live grid, n = n lines up.
  int scrollOffset = 0;

  TerminalCursor cursor = const TerminalCursor(col: 0, row: 0);
  TerminalSelection? selection;

  /// Read-only view of the live grid.
  List<List<TerminalCell>> get grid => _grid;

  /// Apply host cell updates. Returns the damage ranges so the native
  /// renderer can do damage-only redraws. Out-of-bounds updates are ignored.
  TerminalDamage updateCells(List<TerminalCellUpdate> updates) {
    final dirty = <int, ({int min, int max})>{};
    for (final u in updates) {
      if (u.row < 0 || u.row >= rows || u.col < 0 || u.col >= cols) continue;
      _grid[u.row][u.col] = u.cell;
      final r = dirty[u.row];
      if (r == null) {
        dirty[u.row] = (min: u.col, max: u.col + 1);
      } else {
        dirty[u.row] = (
          min: u.col < r.min ? u.col : r.min,
          max: u.col + 1 > r.max ? u.col + 1 : r.max,
        );
      }
    }
    // Leaving scrollback viewing on new output would strand the user;
    // snap back to live like every terminal emulator does.
    if (dirty.isNotEmpty) scrollOffset = 0;
    final ranges = dirty.entries
        .map((e) => (e.key, e.value.min, e.value.max))
        .toList()
      ..sort((a, b) => a.$1.compareTo(b.$1));
    return TerminalDamage(ranges);
  }

  /// Scroll the grid up by [lines], pushing the top lines into scrollback.
  void scrollUp(int lines) {
    for (var i = 0; i < lines; i++) {
      scrollback.add(_grid.removeAt(0));
      _grid.add(List.filled(cols, const TerminalCell(), growable: false));
    }
    while (scrollback.length > scrollbackLines) {
      scrollback.removeAt(0);
    }
  }

  void setCursor(int col, int row, CursorStyle style, bool visible) {
    cursor = TerminalCursor(
      col: col.clamp(0, cols - 1),
      row: row.clamp(0, rows - 1),
      style: style,
      visible: visible,
    );
  }

  /// Scroll the viewport: 0 = live grid, positive = lines into scrollback.
  void scrollTo(int lineOffset) {
    scrollOffset = lineOffset.clamp(0, scrollback.length);
  }

  void select(int startCol, int startRow, int endCol, int endRow) {
    selection = TerminalSelection(
      startCol: startCol,
      startRow: startRow,
      endCol: endCol,
      endRow: endRow,
    );
  }

  void clearSelection() => selection = null;

  /// Text covered by the current selection, for [onCopy].
  String copySelection() {
    final s = selection;
    if (s == null) return '';
    final (r1, r2) = s.startRow <= s.endRow
        ? (s.startRow, s.endRow)
        : (s.endRow, s.startRow);
    final buf = StringBuffer();
    for (var r = r1; r <= r2; r++) {
      if (r < 0 || r >= rows) continue;
      final c1 = r == r1 ? s.startCol : 0;
      final c2 = r == r2 ? s.endCol : cols - 1;
      final (from, to) = c1 <= c2 ? (c1, c2) : (c2, c1);
      final line = _grid[r]
          .sublist(
            from.clamp(0, cols - 1),
            (to + 1).clamp(0, cols),
          )
          .map((c) => c.char)
          .join()
          .trimRight();
      buf.writeln(line);
    }
    return buf.toString().trimRight();
  }

  /// Write a plain string into the grid at the cursor (test/demo helper that
  /// mimics what the host's ghostty-vt diff would produce).
  TerminalDamage writeString(
    String text, {
    TerminalColor? fg,
    TerminalColor? bg,
    bool bold = false,
  }) {
    final allDamage = <(int, int, int)>[];
    var updates = <TerminalCellUpdate>[];
    void flush() {
      if (updates.isNotEmpty) {
        allDamage.addAll(updateCells(updates).ranges);
        updates = [];
      }
    }

    var col = cursor.col;
    var row = cursor.row;
    for (final ch in text.split('')) {
      if (ch == '\n') {
        col = 0;
        row++;
        if (row >= rows) {
          flush(); // apply pending cells before the grid shifts
          scrollUp(1);
          row = rows - 1;
        }
        continue;
      }
      if (col >= cols) {
        col = 0;
        row++;
        if (row >= rows) {
          flush();
          scrollUp(1);
          row = rows - 1;
        }
      }
      updates.add(TerminalCellUpdate(
        row,
        col,
        TerminalCell(char: ch, fg: fg ?? const PaletteColor(7), bg: bg ?? const PaletteColor(0), bold: bold),
      ));
      col++;
    }
    setCursor(col, row, cursor.style, cursor.visible);
    flush();
    return TerminalDamage(allDamage);
  }

  /// Build the immutable snapshot node for the native renderer.
  UiTerminal buildNode(String id) {
    final cells = scrollOffset == 0
        ? _grid
        : _viewportRows();
    return UiTerminal(
      id,
      cols: cols,
      rows: rows,
      fontFamily: fontFamily,
      fontSize: fontSize,
      lineHeight: lineHeight,
      theme: theme,
      scrollbackLines: scrollbackLines,
      cells: cells,
      cursor: cursor,
      selection: selection,
    );
  }

  List<List<TerminalCell>> _viewportRows() {
    // Show the last `rows` lines ending `scrollOffset` above the live grid.
    final all = [...scrollback, ..._grid];
    final end = all.length - scrollOffset;
    final start = (end - rows).clamp(0, all.length);
    final view = all.sublist(start.clamp(0, end), end.clamp(0, all.length));
    // Pad if the scrollback is shorter than the viewport.
    while (view.length < rows) {
      view.insert(
          0, List.filled(cols, const TerminalCell(), growable: false));
    }
    return view;
  }
}
