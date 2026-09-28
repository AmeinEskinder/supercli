/// Tests for [markdown_editing.dart], ported from
/// `clients/legacy/app-kit/swift/Tests/SupercliAppKitUITests/MarkdownInsertMenuTests.swift`.
///
/// Swift test sources:
/// - `markdownBackspaceRemovesMarkers` -> backspace group
/// - `markdownTaskMarkersToggleOnlyAtTheCheckbox` -> task toggle group
library;

import 'package:test/test.dart';

import 'package:supercli_app/widgets/markdown_editing.dart';

void main() {
  group('markdownBackspaceEdit (MarkdownInsertMenu.swift)', () {
    test('removes a task marker, caret to indent', () {
      // Swift: markdownBackspaceRemovesMarkers
      final edit = markdownBackspaceEdit(
        '- [ ] write tests',
        location: 4,
      );
      expect(edit, isNotNull);
      expect(edit!.replacement, 'write tests');
      expect(edit.caretUtf16Offset, 0);
      expect(edit.lineStart, 0);
      expect(edit.lineEnd, 17);
    });

    test('removes a bullet marker', () {
      final edit = markdownBackspaceEdit('- hello', location: 2);
      expect(edit?.replacement, 'hello');
      expect(edit?.caretUtf16Offset, 0);
    });

    test('removes a blockquote marker', () {
      final edit = markdownBackspaceEdit('> quote', location: 2);
      expect(edit?.replacement, 'quote');
    });

    test('removes a heading marker', () {
      final edit = markdownBackspaceEdit('## title', location: 3);
      expect(edit?.replacement, 'title');
    });

    test('removes a numbered marker', () {
      final edit = markdownBackspaceEdit('10. item', location: 4);
      expect(edit?.replacement, 'item');
    });

    test('keeps the indent in the replacement', () {
      final edit = markdownBackspaceEdit('  - [ ] nested', location: 8);
      expect(edit?.replacement, '  nested');
      expect(edit?.caretUtf16Offset, 2);
    });

    test('returns null for a non-zero-length selection', () {
      expect(
        markdownBackspaceEdit('- [ ] x', location: 4, length: 1),
        isNull,
      );
    });

    test('returns null when caret is past the marker', () {
      // Caret inside the body text: backspace deletes a char, no marker edit.
      expect(markdownBackspaceEdit('- [ ] write', location: 10), isNull);
    });

    test('returns null when caret is inside the indent', () {
      expect(markdownBackspaceEdit('  - x', location: 1), isNull);
    });

    test('returns null on a plain line', () {
      expect(markdownBackspaceEdit('just text', location: 4), isNull);
    });

    test('returns null for out-of-range location', () {
      expect(markdownBackspaceEdit('abc', location: 99), isNull);
      expect(markdownBackspaceEdit('abc', location: -1), isNull);
    });

    test('operates on the caret line in multiline text', () {
      final edit = markdownBackspaceEdit(
        'first\n- [ ] second',
        location: 10,
      );
      expect(edit?.replacement, 'second');
      expect(edit?.lineStart, 6);
    });
  });

  group('markdownTaskToggleEdit (MarkdownEditorView.swift)', () {
    test('toggles an unchecked box to checked', () {
      // Swift: markdownTaskMarkersToggleOnlyAtTheCheckbox
      const text = '- [ ] first\n10. [x] second';
      final edit = markdownTaskToggleEdit(text, 3);
      expect(
        edit,
        const MarkdownTaskToggleEdit(
          rangeStart: 3,
          rangeLength: 1,
          replacement: 'x',
        ),
      );
    });

    test('toggles a checked box to unchecked', () {
      const text = '- [ ] first\n10. [x] second';
      final edit = markdownTaskToggleEdit(text, 17);
      expect(
        edit,
        const MarkdownTaskToggleEdit(
          rangeStart: 17,
          rangeLength: 1,
          replacement: ' ',
        ),
      );
    });

    test('returns null when the offset is not on the checkbox', () {
      const text = '- [ ] first\n10. [x] second';
      expect(markdownTaskToggleEdit(text, 7), isNull);
    });

    test('returns null on a plain line', () {
      expect(markdownTaskToggleEdit('no checkbox here', 3), isNull);
    });

    test('returns null for out-of-range offsets', () {
      expect(markdownTaskToggleEdit('abc', -1), isNull);
      expect(markdownTaskToggleEdit('abc', 99), isNull);
    });
  });
}
