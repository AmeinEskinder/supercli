/// Vendored terminal wire types (P0-8 proposal).
///
/// These types mirror the proposed `UiTerminal` API for the gpuidart framework
/// (see docs/internal/proposals/gpuidart-p08-uiterminal.patch). They are vendored
/// here so supercli-app builds against upstream gpuidart. When Amein ships P0-8
/// in the framework, this file should be replaced with imports from package:gpuidart.
///
/// PROPOSAL ONLY: the `UiTerminal` node itself is NOT vendored — the RLE fallback
/// (UiRow/UiText) is used until the framework provides the native node.
library;
/// Terminal pane wire types (P0-8).
///
/// `UiTerminal` is the native terminal grid widget. The Rust host owns the
/// ghostty-vt screen state, diffs it, and pushes dirty cells; the Dart side
/// carries the full grid as snapshot data so the native renderer can do
/// damage-only redraws. Callbacks (`onInput`/`onResize`/`onCopy`) are NOT part
/// of the snapshot — they travel through the host event stream (`GpuiEvent` /
/// `UiAction`) because closures cannot serialize to JSON.

/// A terminal cell color: either an ANSI palette index (0-255) resolved
/// against the [TerminalTheme], or a 24-bit truecolor triple.
sealed class TerminalColor {
  const TerminalColor._();

  /// ANSI palette index 0-255. 0-7 normal, 8-15 bright, 16-231 6x6x6 cube,
  /// 232-255 grayscale ramp.
  factory TerminalColor.palette(int index) {
    if (index < 0 || index > 255) {
      throw ArgumentError.value(index, 'index', 'must be 0-255');
    }
    return PaletteColor(index);
  }

  /// 24-bit truecolor. Each channel 0-255.
  factory TerminalColor.rgb(int r, int g, int b) {
    for (final c in [r, g, b]) {
      if (c < 0 || c > 255) {
        throw ArgumentError.value(c, 'channel', 'must be 0-255');
      }
    }
    return RgbColor(r, g, b);
  }

  Object toJson();
}

final class PaletteColor extends TerminalColor {
  const PaletteColor(this.index) : super._();
  final int index;
  @override
  Map<String, Object> toJson() => {'palette': index};
}

final class RgbColor extends TerminalColor {
  const RgbColor(this.r, this.g, this.b) : super._();
  final int r, g, b;
  @override
  Map<String, Object> toJson() => {'rgb': [r, g, b]};
}

/// Cursor shapes from the P0-8 requirements.
enum CursorStyle {
  block('block'),
  underline('underline'),
  bar('bar');

  const CursorStyle(this.wire);
  final String wire;
}

/// Color scheme: 16 ANSI colors plus the terminal chrome colors.
/// Truecolor cells override the palette per-cell.
final class TerminalTheme {
  const TerminalTheme({
    required this.ansi,
    required this.brightAnsi,
    required this.foreground,
    required this.background,
    required this.cursor,
    required this.cursorText,
    required this.selectionBackground,
    required this.selectionForeground,
  }) : assert(ansi.length == 8, 'ansi must have 8 entries'),
       assert(brightAnsi.length == 8, 'brightAnsi must have 8 entries');

  /// Normal ANSI colors 0-7 as #RRGGBB.
  final List<String> ansi;

  /// Bright ANSI colors 8-15 as #RRGGBB.
  final List<String> brightAnsi;

  final String foreground;
  final String background;
  final String cursor;
  final String cursorText;
  final String selectionBackground;
  final String selectionForeground;

  /// Resolve a [TerminalColor] to a `#RRGGBB` hex string.
  ///
  /// Palette 0-15 come from the theme; 16-231 are the 6x6x6 color cube;
  /// 232-255 are the grayscale ramp (standard xterm behavior).
  String resolve(TerminalColor color) {
    return switch (color) {
      PaletteColor(index: final i) => _resolvePalette(i),
      RgbColor(r: final r, g: final g, b: final b) => _hex(r, g, b),
    };
  }

  String _resolvePalette(int i) {
    if (i < 8) return ansi[i];
    if (i < 16) return brightAnsi[i - 8];
    if (i < 232) {
      // 6x6x6 cube: 16 + 36*r + 6*g + b, levels 0,95,135,175,215,255.
      const levels = [0, 95, 135, 175, 215, 255];
      final j = i - 16;
      return _hex(levels[j ~/ 36], levels[(j % 36) ~/ 6], levels[j % 6]);
    }
    // Grayscale ramp: 8 + 10*k for k in 0..23.
    final v = 8 + 10 * (i - 232);
    return _hex(v, v, v);
  }

  static String _hex(int r, int g, int b) =>
      '#${r.toRadixString(16).padLeft(2, '0')}'
      '${g.toRadixString(16).padLeft(2, '0')}'
      '${b.toRadixString(16).padLeft(2, '0')}';

  Map<String, Object> toJson() => {
    'ansi': ansi,
    'bright_ansi': brightAnsi,
    'foreground': foreground,
    'background': background,
    'cursor': cursor,
    'cursor_text': cursorText,
    'selection_background': selectionBackground,
    'selection_foreground': selectionForeground,
  };

  /// A sensible dark default (approximates the ghostty default theme).
  static final TerminalTheme dark = TerminalTheme(
    ansi: [
      '#000000', // black
      '#cc0000', // red
      '#4e9a06', // green
      '#c4a000', // yellow
      '#3465a4', // blue
      '#75507b', // magenta
      '#06989a', // cyan
      '#d3d7cf', // white
    ],
    brightAnsi: [
      '#555753', // bright black
      '#ef2929', // bright red
      '#8ae234', // bright green
      '#fce94f', // bright yellow
      '#729fcf', // bright blue
      '#ad7fa8', // bright magenta
      '#34e2e2', // bright cyan
      '#eeeeec', // bright white
    ],
    foreground: '#eeeeec',
    background: '#1e1e1e',
    cursor: '#eeeeec',
    cursorText: '#1e1e1e',
    selectionBackground: '#264f78',
    selectionForeground: '#eeeeec',
  );
}

/// One cell of the terminal grid.
final class TerminalCell {
  const TerminalCell({
    this.char = ' ',
    this.fg = const PaletteColor(7),
    this.bg = const PaletteColor(0),
    this.bold = false,
    this.italic = false,
    this.underline = false,
    this.inverse = false,
  });

  /// A single grapheme cluster. Must render in exactly one cell width.
  final String char;
  final TerminalColor fg;
  final TerminalColor bg;
  final bool bold;
  final bool italic;
  final bool underline;
  final bool inverse;

  /// Attribute key used for run-length encoding: consecutive cells with the
  /// same key merge into one text run.
  String get runKey =>
      '${fg.toJson()}|${bg.toJson()}|$bold|$italic|$underline|$inverse';

  Map<String, Object> toJson() => {
    'char': char,
    'fg': fg.toJson(),
    'bg': bg.toJson(),
    if (bold) 'bold': true,
    if (italic) 'italic': true,
    if (underline) 'underline': true,
    if (inverse) 'inverse': true,
  };

  @override
  bool operator ==(Object other) =>
      other is TerminalCell &&
      char == other.char &&
      fg.toJson().toString() == other.fg.toJson().toString() &&
      bg.toJson().toString() == other.bg.toJson().toString() &&
      bold == other.bold &&
      italic == other.italic &&
      underline == other.underline &&
      inverse == other.inverse;

  @override
  int get hashCode => Object.hash(char, runKey);
}

/// Cursor state.
final class TerminalCursor {
  const TerminalCursor({
    required this.col,
    required this.row,
    this.style = CursorStyle.block,
    this.visible = true,
    this.blinking = false,
  });

  final int col;
  final int row;
  final CursorStyle style;
  final bool visible;
  final bool blinking;

  Map<String, Object> toJson() => {
    'col': col,
    'row': row,
    'style': style.wire,
    'visible': visible,
    'blinking': blinking,
  };
}

/// A text selection range. Rows/cols are in grid coordinates; when
/// [scrollback] is true the rows address the scrollback buffer instead.
final class TerminalSelection {
  const TerminalSelection({
    required this.startCol,
    required this.startRow,
    required this.endCol,
    required this.endRow,
    this.scrollback = false,
  });

  final int startCol;
  final int startRow;
  final int endCol;
  final int endRow;
  final bool scrollback;

  Map<String, Object> toJson() => {
    'start': [startCol, startRow],
    'end': [endCol, endRow],
    'scrollback': scrollback,
  };
}

/// A set of dirty cell ranges produced by diffing ghostty-vt screen state.
/// Each entry is (row, firstCol, onePastLastCol).
final class TerminalDamage {
  const TerminalDamage(this.ranges);

  final List<(int, int, int)> ranges;

  List<List<int>> toJson() =>
      ranges.map((r) => [r.$1, r.$2, r.$3]).toList();
}