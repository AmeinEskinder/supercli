/// Markdown editing helpers ported from Swift to Dart.
///
/// Ports two pure functions from
/// `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/`:
/// - `markdownBackspaceEdit(text:selection:)` from `MarkdownInsertMenu.swift`
/// - `markdownTaskToggleEdit(text:utf16Offset:)` from `MarkdownEditorView.swift`
///
/// Both are the native text-system translation of the Rust Markdown contracts
/// (backspace removes list/quote/heading markers; tap toggles task checkboxes).
/// Block/menu vocabulary and replacements remain App-owned and never live in
/// this renderer.
///
/// UTF-16 note: Dart [String] is UTF-16, so [String.length], `[]` indexing and
/// [String.substring] operate on UTF-16 code units, mirroring the Swift
/// `NSString` semantics of the originals. All marker literals are ASCII, so
/// code-unit and scalar indexing agree on them.
library;

/// Result of [markdownBackspaceEdit].
///
/// Mirrors Swift `MarkdownBackspaceEdit`: replace the line content range
/// `[lineStart, lineEnd)` (UTF-16 offsets, excluding the line terminator)
/// with [replacement], then move the caret to [caretUtf16Offset].
final class MarkdownBackspaceEdit {
  const MarkdownBackspaceEdit({
    required this.lineStart,
    required this.lineEnd,
    required this.replacement,
    required this.caretUtf16Offset,
  });

  final int lineStart;
  final int lineEnd;
  final String replacement;
  final int caretUtf16Offset;

  @override
  bool operator ==(Object other) =>
      other is MarkdownBackspaceEdit &&
      other.lineStart == lineStart &&
      other.lineEnd == lineEnd &&
      other.replacement == replacement &&
      other.caretUtf16Offset == caretUtf16Offset;

  @override
  int get hashCode =>
      Object.hash(lineStart, lineEnd, replacement, caretUtf16Offset);

  @override
  String toString() =>
      'MarkdownBackspaceEdit([$lineStart, $lineEnd) -> "$replacement", caret $caretUtf16Offset)';
}

/// Result of [markdownTaskToggleEdit].
///
/// Mirrors Swift `MarkdownTaskToggleEdit`: replace [rangeStart,
/// rangeStart + rangeLength) (UTF-16 offsets) with [replacement].
final class MarkdownTaskToggleEdit {
  const MarkdownTaskToggleEdit({
    required this.rangeStart,
    required this.rangeLength,
    required this.replacement,
  });

  final int rangeStart;
  final int rangeLength;
  final String replacement;

  @override
  bool operator ==(Object other) =>
      other is MarkdownTaskToggleEdit &&
      other.rangeStart == rangeStart &&
      other.rangeLength == rangeLength &&
      other.replacement == replacement;

  @override
  int get hashCode => Object.hash(rangeStart, rangeLength, replacement);

  @override
  String toString() =>
      'MarkdownTaskToggleEdit([$rangeStart, ${rangeStart + rangeLength}) -> "$replacement"]';
}

bool _isSpaceOrTab(int unit) => unit == 0x20 || unit == 0x09;
bool _isLineBreak(int unit) => unit == 0x0A || unit == 0x0D;

/// Start (inclusive) of the line content containing UTF-16 [offset].
///
/// Mirrors `NSString.lineRange(for:)` for a zero-length range: the offset
/// belongs to the line it terminates, and an offset at the very end of a
/// `\n`-terminated string starts a new (empty) line.
int _lineStart(String text, int offset) {
  if (offset <= 0) return 0;
  final idx = text.lastIndexOf('\n', offset - 1);
  return idx < 0 ? 0 : idx + 1;
}

/// End (exclusive) of the line content containing UTF-16 [offset], with any
/// trailing `\r` stripped. Mirrors the Swift loop that trims 10/13 off
/// `lineRangeWithEnding`.
int _lineEnd(String text, int offset, int start) {
  var end = text.indexOf('\n', offset);
  if (end < 0) end = text.length;
  while (end > start && _isLineBreak(text.codeUnitAt(end - 1))) {
    end--;
  }
  return end;
}

/// Mirrors Swift `markdownBackspaceEdit(text:selection:)`.
///
/// When the caret (a zero-length selection at [location]) sits just after a
/// Markdown list marker (`- [ ] `, `- `, `> `, `# `, `1. `, …) — i.e. strictly
/// past the indent but within the marker — backspace should delete the marker
/// rather than a single character. Returns the edit, or null when the caret
/// is not in that position (including non-zero-length selections and
/// out-of-range locations, mirroring the Swift guards).
MarkdownBackspaceEdit? markdownBackspaceEdit(
  String text, {
  required int location,
  int length = 0,
}) {
  if (length != 0) return null;
  if (location < 0 || location > text.length) return null;

  final start = _lineStart(text, location);
  final end = _lineEnd(text, location, start);
  final line = text.substring(start, end);

  var indentEnd = 0;
  while (indentEnd < line.length && _isSpaceOrTab(line.codeUnitAt(indentEnd))) {
    indentEnd++;
  }
  final indent = line.substring(0, indentEnd);
  final rest = line.substring(indentEnd);

  final int markerLength;
  const taskMarkers = ['- [ ] ', '- [x] ', '- [X] '];
  const bulletMarkers = ['- ', '* ', '+ ', '> '];
  final taskMarker = taskMarkers.where(rest.startsWith).firstOrNull;
  if (taskMarker != null) {
    markerLength = taskMarker.length;
  } else {
    final bulletMarker = bulletMarkers.where(rest.startsWith).firstOrNull;
    if (bulletMarker != null) {
      markerLength = bulletMarker.length;
    } else {
      var hashes = 0;
      while (hashes < rest.length && rest.codeUnitAt(hashes) == 0x23) {
        hashes++;
      }
      if (hashes >= 1 &&
          hashes <= 6 &&
          hashes < rest.length &&
          rest.codeUnitAt(hashes) <= 0x20) {
        // `# ` heading marker: hashes + one whitespace.
        markerLength = hashes + 1;
      } else {
        var digits = 0;
        while (digits < rest.length &&
            rest.codeUnitAt(digits) >= 0x30 &&
            rest.codeUnitAt(digits) <= 0x39) {
          digits++;
        }
        final numberedMarker = digits > 0 ? '${rest.substring(0, digits)}. ' : '';
        if (numberedMarker.isNotEmpty && rest.startsWith(numberedMarker)) {
          markerLength = numberedMarker.length;
        } else {
          return null;
        }
      }
    }
  }

  final column = location - start;
  final prefixLength = indent.length + markerLength;
  if (column <= indent.length || column > prefixLength) return null;

  final body = rest.substring(markerLength);
  return MarkdownBackspaceEdit(
    lineStart: start,
    lineEnd: end,
    replacement: indent + body,
    caretUtf16Offset: indent.length,
  );
}

/// Matches a Markdown task-list item opener: `- [ ]`, `* [x]`, `1. [ ]`, …
/// Mirrors the Swift `NSRegularExpression` in `markdownTaskToggleEdit`.
final RegExp _taskItemOpener =
    RegExp(r'^(\s*(?:(?:[-+*])|(?:\d+\.))\s+)\[([ xX])\]');

/// Mirrors Swift `markdownTaskToggleEdit(text:utf16Offset:)`.
///
/// When the tap/caret offset falls on the `[ ]`/`[x]` checkbox of a task-list
/// item, returns the single-character toggle edit; otherwise null (including
/// out-of-range offsets, mirroring the Swift guards).
MarkdownTaskToggleEdit? markdownTaskToggleEdit(String text, int utf16Offset) {
  if (utf16Offset < 0 || utf16Offset > text.length) return null;
  final location = utf16Offset.clamp(0, text.isNotEmpty ? text.length - 1 : 0);

  final start = _lineStart(text, location);
  final end = _lineEnd(text, location, start);
  final line = text.substring(start, end);

  final match = _taskItemOpener.firstMatch(line);
  if (match == null) return null;
  final markerStart = start + match.group(1)!.length;
  final markerEnd = markerStart + 2;
  if (utf16Offset < markerStart || utf16Offset > markerEnd) return null;

  final checked = text[markerStart + 1] != ' ';
  return MarkdownTaskToggleEdit(
    rangeStart: markerStart + 1,
    rangeLength: 1,
    replacement: checked ? ' ' : 'x',
  );
}
