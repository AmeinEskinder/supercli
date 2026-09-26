/// Terminal pane widget: real implementation of the P0-8 terminal surface.
///
/// Renders a [TerminalState] via the RLE fallback (UiRow/UiText).
///
/// Run-length-encodes consecutive cells with identical attributes into one
/// UiText. This gives real ANSI 256 colors, truecolor, bold, cursor, and
/// selection highlighting today. The P0-8 `UiTerminal` native node proposal
/// is at docs/internal/proposals/gpuidart-p08-uiterminal.patch — when Amein
/// ships it in the framework, this can switch to damage-only redraws.
library;

import 'package:gpuidart/gpuidart.dart';

import 'terminal_state.dart';
import 'terminal_types.dart';

/// Maps special keys to the byte sequences a PTY expects.
/// Sent through `onInput` when the corresponding [UiAction] fires.
///
/// NOTE (gap): [UiAction] rejects bare letters/digits ("native text input
/// and IME own them"), so printable characters cannot be bound. Full keyboard
/// input needs a raw key-event API in gpuidart (see
/// docs/gpuidart-gaps-terminal.md G-5).
class TerminalKeymap {
  const TerminalKeymap();

  /// Escape sequence for a named special key, or null if unmapped.
  /// [modifiers] is a set of 'ctrl', 'alt', 'shift'.
  String? sequenceFor(String key, Set<String> modifiers) {
    final ctrl = modifiers.contains('ctrl');
    // Ctrl+letter -> control character (the few terminals need raw).
    if (ctrl && key.length == 1) {
      final c = key.codeUnitAt(0);
      if (c >= 97 && c <= 122) return String.fromCharCode(c - 96);
      if (c >= 65 && c <= 90) return String.fromCharCode(c - 64);
    }
    return switch (key) {
      'enter' => '\r',
      'tab' => '\t',
      'backspace' => '\x7f',
      'escape' => '\x1b',
      'up' => '\x1b[A',
      'down' => '\x1b[B',
      'right' => '\x1b[C',
      'left' => '\x1b[D',
      'home' => '\x1b[H',
      'end' => '\x1b[F',
      'pageup' => '\x1b[5~',
      'pagedown' => '\x1b[6~',
      'insert' => '\x1b[2~',
      'delete' => '\x1b[3~',
      'f1' => '\x1bOP',
      'f2' => '\x1bOQ',
      'f3' => '\x1bOR',
      'f4' => '\x1bOS',
      'f5' => '\x1b[15~',
      'f6' => '\x1b[17~',
      'f7' => '\x1b[18~',
      'f8' => '\x1b[19~',
      'f9' => '\x1b[20~',
      'f10' => '\x1b[21~',
      'f11' => '\x1b[23~',
      'f12' => '\x1b[24~',
      'space' => ' ',
      _ => null,
    };
  }

  /// [UiAction] bindings for the special keys. Scoped to the terminal node.
  List<UiAction> actionsFor(String terminalNodeId) {
    const keys = [
      'enter', 'tab', 'backspace', 'escape', 'space',
      'up', 'down', 'left', 'right', 'home', 'end',
      'pageup', 'pagedown', 'insert', 'delete',
      'f1', 'f2', 'f3', 'f4', 'f5', 'f6',
      'f7', 'f8', 'f9', 'f10', 'f11', 'f12',
    ];
    return [
      for (final k in keys)
        UiAction(
          name: 'terminal.key.$k',
          keys: k,
          context: UiActionContext.node(terminalNodeId),
        ),
    ];
  }
}

/// The terminal pane: header chrome + terminal surface.
class TerminalPane {
  TerminalPane({
    required this.paneId,
    required this.title,
    required this.state,
    this.onInput,
    this.onResize,
    this.onCopy,
    this.findBarVisible = false,
    this.keymap = const TerminalKeymap(),
  });

  final String paneId;
  final String title;
  final TerminalState state;

  /// Bytes for the PTY (e.g. from key bindings). The app forwards these to
  /// the host, which writes them to the session's PTY master.
  final void Function(List<int> bytes)? onInput;

  /// Fired when the widget's pixel size changes so the host can SIGWINCH the
  /// PTY. NOTE (gap): gpuidart has no size-observation API; the app must poll
  /// or wire this manually (see docs/gpuidart-gaps-terminal.md G-6).
  final void Function(int cols, int rows)? onResize;

  /// Fired with the selected text on copy.
  final void Function(String text)? onCopy;

  final bool findBarVisible;
  final TerminalKeymap keymap;

  /// Handle a key action from the host event stream. Returns true if the key
  /// was consumed (an [onInput] callback fired).
  bool handleKeyAction(String actionName, Set<String> modifiers) {
    const prefix = 'terminal.key.';
    if (!actionName.startsWith(prefix)) return false;
    final key = actionName.substring(prefix.length);
    final seq = keymap.sequenceFor(key, modifiers);
    if (seq == null) return false;
    onInput?.call(seq.codeUnits);
    return true;
  }

  /// Copy the current selection through [onCopy].
  void copySelection() {
    final text = state.copySelection();
    if (text.isNotEmpty) onCopy?.call(text);
  }

  /// Visible grid rows for the RLE fallback renderer.
  /// (P0-8 native node proposal: docs/internal/proposals/gpuidart-p08-uiterminal.patch)

  /// Full pane: header + terminal surface + optional find bar.
  UiNode build() {
    return UiColumn('terminal-pane-$paneId', [
      UiRow('pane-header-$paneId', [
        UiText('pane-title-$paneId', title),
        const UiButton('pane-split-h', 'Split H'),
        const UiButton('pane-split-v', 'Split V'),
        const UiButton('pane-close', '×'),
      ]),
      if (findBarVisible)
        UiRow('find-bar-$paneId', [
          const UiInput('find-input', placeholder: 'Find…'),
          const UiButton('find-next', 'Next'),
          const UiButton('find-prev', 'Prev'),
        ]),
      // RLE fallback grid (P0-8 native node proposal pending at
      // docs/internal/proposals/gpuidart-p08-uiterminal.patch).
      buildFallback(),
    ]);
  }

  /// Renders the terminal grid with UiRow/UiText primitives.
  ///
  /// Consecutive cells with identical attributes merge into one UiText run
  /// (run-length encoding), so a typical mostly-one-color line is a single
  /// node. Colors resolve through the theme: ANSI 256 palette + 24-bit
  /// truecolor both become #RRGGBB via [TerminalTheme.resolve].
  UiColumn buildFallback() {
    final theme = state.theme;
    final nodeRows = <UiNode>[];
    final grid = state.grid;
    for (var r = 0; r < state.rows; r++) {
      nodeRows.add(_buildRow(r, grid[r], theme));
    }
    return UiColumn(
      'terminal-fallback-$paneId',
      nodeRows,
      style: UiStyle(
        background: UiColor.hex(theme.background),
        fontSize: state.fontSize,
      ),
    );
  }

  UiRow _buildRow(int r, List<TerminalCell> cells, TerminalTheme theme) {
    final runs = <UiNode>[];
    var i = 0;
    while (i < cells.length) {
      var j = i + 1;
      while (j < cells.length && cells[j].runKey == cells[i].runKey) {
        j++;
      }
      runs.add(_buildRun(r, i, j, cells.sublist(i, j), theme));
      i = j;
    }
    return UiRow('trow-$paneId-$r', runs);
  }

  UiText _buildRun(
    int r,
    int from,
    int to,
    List<TerminalCell> run,
    TerminalTheme theme,
  ) {
    final cell = run[0];
    var text = run.map((c) => c.char).join();
    var fg = theme.resolve(cell.fg);
    var bg = theme.resolve(cell.bg);
    var bold = cell.bold;
    // NOTE (gap G-3): underline/italic have no UiStyle field, so they are
    // tracked in the model but cannot render in the fallback.

    // Selection highlighting wins over cell colors.
    final sel = state.selection;
    if (sel != null && _inSelection(r, from, to - 1, sel)) {
      bg = theme.selectionBackground;
      fg = theme.selectionForeground;
    }

    // Cursor: block inverts the cell, underline/bar draw as styled marker.
    // (Approximation: no canvas API to draw a real bar/underline cursor.)
    final c = state.cursor;
    if (c.visible && c.row == r && c.col >= from && c.col < to) {
      switch (c.style) {
        case CursorStyle.block:
          final tmp = fg;
          fg = bg;
          bg = tmp;
          // Swap to cursor chrome when on the default background so the
          // cursor is visible against any cell color.
          if (bg == theme.background) {
            bg = theme.cursor;
            fg = theme.cursorText;
          }
        case CursorStyle.underline:
          // Underline cursor: no decoration primitive in UiStyle (gap G-3);
          // fall through to bold marker so the cursor stays visible.
          bold = true;
        case CursorStyle.bar:
          // No bar primitive: render the cursor cell bold + underlined as
          // the closest approximation.
          bold = true;
      }
      // Ensure the cursor cell itself shows even on empty cells.
      if (text.trim().isEmpty) text = ' ';
    }

    return UiText(
      'trun-$paneId-$r-$from',
      text,
      style: UiStyle(
        foreground: UiColor.hex(fg),
        background: UiColor.hex(bg),
        fontSize: state.fontSize,
        fontWeight: bold ? UiFontWeight.bold : UiFontWeight.normal,
      ),
    );
  }

  bool _inSelection(int r, int from, int to, TerminalSelection sel) {
    final (r1, c1, r2, c2) = sel.startRow <= sel.endRow
        ? (sel.startRow, sel.startCol, sel.endRow, sel.endCol)
        : (sel.endRow, sel.endCol, sel.startRow, sel.startCol);
    if (r < r1 || r > r2) return false;
    final first = r == r1 ? c1 : 0;
    final last = r == r2 ? c2 : 0x7fffffff;
    return from <= last && to >= first;
  }
}
