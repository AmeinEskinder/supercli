/// Pure editing logic ported from the AppKit markdown views.
///
/// Ports of `markdownBackspaceEdit(text:selection:)` (MarkdownInsertMenu.swift)
/// and `markdownTaskToggleEdit(text:utf16Offset:)` (MarkdownEditorView.swift).
///
/// Both operate on UTF-16 code-unit offsets, matching `NSString` semantics:
/// Dart `String` length/indexing is UTF-16 based, so offsets line up with the
/// Swift originals without conversion.
library;

/// A marker-removal edit produced by [markdownBackspaceEdit].
final class MarkdownBackspaceEdit {
  const MarkdownBackspaceEdit({
    required this.lineStart,
    required this.lineLength,
    required this.replacement,
    required this.caretUtf16Offset,
  });

  /// UTF-16 offset of the start of the edited line.
  final int lineStart;

  /// UTF-16 length of the edited line, excluding the line terminator.
  final int lineLength;

  /// Replacement text for the line (the marker prefix removed).
  final String replacement;

  /// UTF-16 caret offset the caret should move to, relative to [lineStart].
  final int caretUtf16Offset;

  @override
  bool operator ==(Object other) =>
      other is MarkdownBackspaceEdit &&
      other.lineStart == lineStart &&
      other.lineLength == lineLength &&
      other.replacement == replacement &&
      other.caretUtf16Offset == caretUtf16Offset;

  @override
  int get hashCode =>
      Object.hash(lineStart, lineLength, replacement, caretUtf16Offset);

  @override
  String toString() =>
      'MarkdownBackspaceEdit(lineStart: $lineStart, lineLength: $lineLength, '
      'replacement: $replacement, caretUtf16Offset: $caretUtf16Offset)';
}

/// A checkbox-state toggle edit produced by [markdownTaskToggleEdit].
final class MarkdownTaskToggleEdit {
  const MarkdownTaskToggleEdit({
    required this.stateOffset,
    required this.stateLength,
    required this.replacement,
  });

  /// UTF-16 offset of the checkbox state character (the char inside `[ ]`).
  final int stateOffset;

  /// UTF-16 length of the replaced range (always 1: the state character).
  final int stateLength;

  /// Replacement for the state character (`"x"` to check, `" "` to uncheck).
  final String replacement;

  @override
  bool operator ==(Object other) =>
      other is MarkdownTaskToggleEdit &&
      other.stateOffset == stateOffset &&
      other.stateLength == stateLength &&
      other.replacement == replacement;

  @override
  int get hashCode => Object.hash(stateOffset, stateLength, replacement);

  @override
  String toString() =>
      'MarkdownTaskToggleEdit(stateOffset: $stateOffset, '
      'stateLength: $stateLength, replacement: $replacement)';
}

/// Native text-system translation of the Rust Markdown backspace contract.
/// Block/menu vocabulary and replacements remain App-owned and never live in
/// this renderer.
///
/// When the caret sits inside the marker prefix of a list item, task item,
/// blockquote, or ATX heading line, backspace removes the marker instead of
/// deleting a character.
///
/// [caretOffset] is the collapsed caret position in UTF-16 code units;
/// [selectionLength] must be 0 (a non-collapsed selection yields no edit).
/// Returns null when no marker-removal edit applies.
MarkdownBackspaceEdit? markdownBackspaceEdit({
  required String text,
  required int caretOffset,
  int selectionLength = 0,
}) {
  if (selectionLength != 0) return null;
  if (caretOffset < 0 || caretOffset > text.length) return null;

  // NSString.lineRange(for:) equivalent: line bounds including the terminator.
  var lineStart = caretOffset;
  while (lineStart > 0 && text[lineStart - 1] != '\n') {
    lineStart--;
  }
  var lineEndWithTerminator = caretOffset;
  while (lineEndWithTerminator < text.length &&
      text[lineEndWithTerminator] != '\n') {
    lineEndWithTerminator++;
  }
  if (lineEndWithTerminator < text.length) lineEndWithTerminator++;

  // Strip trailing \n (10) / \r (13), mirroring the Swift loop.
  var contentEnd = lineEndWithTerminator;
  while (contentEnd > lineStart) {
    final c = text.codeUnitAt(contentEnd - 1);
    if (c == 10 || c == 13) {
      contentEnd--;
    } else {
      break;
    }
  }

  final line = text.substring(lineStart, contentEnd);
  var indentEnd = 0;
  while (indentEnd < line.length &&
      (line[indentEnd] == ' ' || line[indentEnd] == '\t')) {
    indentEnd++;
  }
  final indent = line.substring(0, indentEnd);
  final rest = line.substring(indentEnd);

  final int markerLength;
  final taskMarker = _firstPrefix(rest, const ['- [ ] ', '- [x] ', '- [X] ']);
  if (taskMarker != null) {
    markerLength = taskMarker.length;
  } else {
    final bullet = _firstPrefix(rest, const ['- ', '* ', '+ ', '> ']);
    if (bullet != null) {
      markerLength = bullet.length;
    } else {
      var hashes = 0;
      while (hashes < rest.length && rest[hashes] == '#') {
        hashes++;
      }
      if (hashes >= 1 &&
          hashes <= 6 &&
          hashes < rest.length &&
          _isWhitespace(rest[hashes])) {
        markerLength = hashes + 1;
      } else {
        var digits = 0;
        while (digits < rest.length && _isAsciiDigit(rest[digits])) {
          digits++;
        }
        final numberedMarker = '${rest.substring(0, digits)}. ';
        if (digits > 0 && rest.startsWith(numberedMarker)) {
          markerLength = numberedMarker.length;
        } else {
          return null;
        }
      }
    }
  }

  final column = caretOffset - lineStart;
  final prefixLength = indent.length + markerLength;
  if (column <= indent.length || column > prefixLength) return null;

  final body = rest.substring(markerLength);
  return MarkdownBackspaceEdit(
    lineStart: lineStart,
    lineLength: contentEnd - lineStart,
    replacement: indent + body,
    caretUtf16Offset: indent.length,
  );
}

/// Toggles a task-list checkbox when the caret/click lands on its marker.
///
/// Returns the single-character replacement for the checkbox state, or null
/// when [utf16Offset] is not on a `[ ]`/`[x]`/`[X]` marker of a list item.
///
/// Port of `markdownTaskToggleEdit(text:utf16Offset:)` (MarkdownEditorView.swift).
MarkdownTaskToggleEdit? markdownTaskToggleEdit({
  required String text,
  required int utf16Offset,
}) {
  if (utf16Offset < 0 || utf16Offset > text.length) return null;
  final location =
      utf16Offset < text.length ? utf16Offset : (text.isEmpty ? 0 : text.length - 1);

  // Line bounds including the terminator (NSString.lineRange(for:) equivalent).
  var lineStart = location;
  while (lineStart > 0 && text[lineStart - 1] != '\n') {
    lineStart--;
  }
  var lineEnd = location;
  while (lineEnd < text.length && text[lineEnd] != '\n') {
    lineEnd++;
  }
  final line = text.substring(lineStart, lineEnd);

  // ^(\s*(?:(?:[-+*])|(?:\d+\.))\s+)\[([ xX])\]
  final expression = RegExp(r'^(\s*(?:(?:[-+*])|(?:\d+\.))\s+)\[([ xX])\]');
  final match = expression.firstMatch(line);
  if (match == null) return null;

  final markerStart = lineStart + match.group(1)!.length;
  final markerEnd = markerStart + 2;
  if (utf16Offset < markerStart || utf16Offset > markerEnd) return null;

  final stateOffset = markerStart + 1;
  final checked = text[stateOffset] != ' ';
  return MarkdownTaskToggleEdit(
    stateOffset: stateOffset,
    stateLength: 1,
    replacement: checked ? ' ' : 'x',
  );
}

String? _firstPrefix(String s, List<String> prefixes) {
  for (final p in prefixes) {
    if (s.startsWith(p)) return p;
  }
  return null;
}

bool _isWhitespace(String c) => RegExp(r'\s').hasMatch(c);

bool _isAsciiDigit(String c) =>
    c.length == 1 && c.codeUnitAt(0) >= 0x30 && c.codeUnitAt(0) <= 0x39;
