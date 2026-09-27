/// Tests for the AppKit markdown editing logic.
/// Ports the portable cases from
/// `app-kit/swift/Tests/SupercliAppKitUITests/MarkdownInsertMenuTests.swift`.
///
/// Audit: 2 of the 4 Swift tests are portable pure logic
/// (`markdownTaskMarkersToggleOnlyAtTheCheckbox`,
/// `markdownBackspaceRemovesMarkers`). `markdownCommandHintAndMenuTriggersComeFromTheSpec`
/// needs the full `MarkdownEditorSpec` (not yet ported), and
/// `nativeMarkdownSlashRoundTripPresentsAndActivatesTheAuthoritativeMenu`
/// needs a real native window.
library;

import 'package:supercli_app/widgets/markdown_editing.dart';
import 'package:test/test.dart';

void main() {
  group('markdownTaskToggleEdit', () {
    // Port of markdownTaskMarkersToggleOnlyAtTheCheckbox.
    test('toggles task markers only at the checkbox', () {
      const text = '- [ ] first\n10. [x] second';
      expect(
        markdownTaskToggleEdit(text: text, utf16Offset: 3),
        const MarkdownTaskToggleEdit(
          stateOffset: 3,
          stateLength: 1,
          replacement: 'x',
        ),
      );
      expect(
        markdownTaskToggleEdit(text: text, utf16Offset: 17),
        const MarkdownTaskToggleEdit(
          stateOffset: 17,
          stateLength: 1,
          replacement: ' ',
        ),
      );
      expect(markdownTaskToggleEdit(text: text, utf16Offset: 7), isNull);
    });

    test('unchecked task becomes checked', () {
      const text = '- [ ] todo';
      final edit = markdownTaskToggleEdit(text: text, utf16Offset: 3);
      expect(edit, isNotNull);
      expect(edit!.replacement, 'x');
      expect(text.substring(edit.stateOffset, edit.stateOffset + 1), ' ');
    });

    test('checked task becomes unchecked', () {
      const text = '* [x] done';
      final edit = markdownTaskToggleEdit(text: text, utf16Offset: 3);
      expect(edit, isNotNull);
      expect(edit!.replacement, ' ');
    });

    test('uppercase X counts as checked', () {
      const text = '+ [X] done';
      final edit = markdownTaskToggleEdit(text: text, utf16Offset: 3);
      expect(edit, isNotNull);
      expect(edit!.replacement, ' ');
    });

    test('click on the brackets but outside the marker is ignored', () {
      const text = '- [ ] todo';
      // On the '['
      expect(markdownTaskToggleEdit(text: text, utf16Offset: 2), isNotNull);
      // On the ']'
      expect(markdownTaskToggleEdit(text: text, utf16Offset: 4), isNotNull);
      // Past the marker
      expect(markdownTaskToggleEdit(text: text, utf16Offset: 5), isNull);
    });

    test('non-task lines yield no edit', () {
      expect(
          markdownTaskToggleEdit(text: 'plain text', utf16Offset: 2), isNull);
      expect(markdownTaskToggleEdit(text: '- bullet', utf16Offset: 2), isNull);
      expect(markdownTaskToggleEdit(text: '', utf16Offset: 0), isNull);
    });

    test('out-of-range offsets yield no edit', () {
      const text = '- [ ] todo';
      expect(markdownTaskToggleEdit(text: text, utf16Offset: -1), isNull);
      expect(
          markdownTaskToggleEdit(text: text, utf16Offset: text.length + 1),
          isNull);
    });

    test('numbered task items toggle', () {
      const text = '3. [ ] third';
      final edit = markdownTaskToggleEdit(text: text, utf16Offset: 4);
      expect(edit, isNotNull);
      expect(edit!.replacement, 'x');
      expect(edit.stateOffset, 4);
    });
  });

  group('markdownBackspaceEdit', () {
    // Port of markdownBackspaceRemovesMarkers.
    test('backspace inside a task marker removes the marker', () {
      const task = '- [ ] write tests';
      final edit =
          markdownBackspaceEdit(text: task, caretOffset: 4);
      expect(edit, isNotNull);
      expect(edit!.replacement, 'write tests');
      expect(edit.caretUtf16Offset, 0);
      expect(edit.lineStart, 0);
      expect(edit.lineLength, task.length);
    });

    test('backspace removes bullet markers', () {
      const text = '  - item';
      final edit = markdownBackspaceEdit(text: text, caretOffset: 4);
      expect(edit, isNotNull);
      expect(edit!.replacement, '  item');
      expect(edit.caretUtf16Offset, 2);
    });

    test('backspace removes heading markers', () {
      const text = '## heading';
      final edit = markdownBackspaceEdit(text: text, caretOffset: 3);
      expect(edit, isNotNull);
      expect(edit!.replacement, 'heading');
      expect(edit.caretUtf16Offset, 0);
    });

    test('backspace removes numbered markers', () {
      const text = '10. item';
      final edit = markdownBackspaceEdit(text: text, caretOffset: 4);
      expect(edit, isNotNull);
      expect(edit!.replacement, 'item');
      expect(edit.caretUtf16Offset, 0);
    });

    test('backspace removes blockquote markers', () {
      const text = '> quote';
      final edit = markdownBackspaceEdit(text: text, caretOffset: 2);
      expect(edit, isNotNull);
      expect(edit!.replacement, 'quote');
    });

    test('too many hashes is not a heading', () {
      const text = '####### not a heading';
      expect(markdownBackspaceEdit(text: text, caretOffset: 8), isNull);
    });

    test('caret past the marker yields no edit', () {
      const text = '- [ ] write tests';
      expect(markdownBackspaceEdit(text: text, caretOffset: 8), isNull);
    });

    test('caret at line start yields no edit', () {
      const text = '- item';
      expect(markdownBackspaceEdit(text: text, caretOffset: 0), isNull);
    });

    test('plain lines yield no edit', () {
      const text = 'just text';
      expect(markdownBackspaceEdit(text: text, caretOffset: 4), isNull);
    });

    test('non-collapsed selection yields no edit', () {
      const text = '- item';
      expect(
          markdownBackspaceEdit(
              text: text, caretOffset: 2, selectionLength: 3),
          isNull);
    });

    test('operates on the caret line in multiline text', () {
      const text = 'first\n- [ ] second\nthird';
      final edit = markdownBackspaceEdit(text: text, caretOffset: 10);
      expect(edit, isNotNull);
      expect(edit!.replacement, 'second');
      expect(edit.lineStart, 6);
    });

    test('checked task markers are removed too', () {
      const text = '- [x] done';
      final edit = markdownBackspaceEdit(text: text, caretOffset: 4);
      expect(edit, isNotNull);
      expect(edit!.replacement, 'done');
    });
  });
}
