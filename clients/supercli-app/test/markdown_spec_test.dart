/// Tests for [markdown_spec.dart], ported from
/// `clients/legacy/app-kit/swift/Tests/SupercliAppKitUITests/MarkdownInsertMenuTests.swift`
/// (`markdownCommandHintAndMenuTriggersComeFromTheSpec`).
///
/// Note: `nativeMarkdownSlashRoundTripPresentsAndActivatesTheAuthoritativeMenu`
/// is a native AppKit/SwiftUI test requiring NSWindow and is not portable to Dart.
library;

import 'package:test/test.dart';

import 'package:supercli_app/widgets/markdown_spec.dart';

void main() {
  group('MarkdownEditorSpec.commandHintVisible', () {
    test('visible on empty line with valid hint', () {
      // Swift: markdownCommandHintAndMenuTriggersComeFromTheSpec
      final editor = MarkdownEditorSpec(
        text: 'title\n\nbody',
        selection: UITextSelection.caret(
          const UITextPosition(line: 1, utf16Column: 0),
        ),
        commandHint: const MarkdownCommandHint(
          text: "Type '/' for commands",
        ),
        actions: const MarkdownEditorActions(openMenu: 'open-menu'),
      );
      expect(editor.commandHintVisible, isTrue);
    });

    test('hidden in preview presentation', () {
      final editor = MarkdownEditorSpec(
        text: 'title\n\nbody',
        selection: UITextSelection.caret(
          const UITextPosition(line: 1, utf16Column: 0),
        ),
        presentation: MarkdownPresentation.preview,
        commandHint: const MarkdownCommandHint(
          text: "Type '/' for commands",
        ),
        actions: const MarkdownEditorActions(openMenu: 'open-menu'),
      );
      expect(editor.commandHintVisible, isFalse);
    });

    test('hidden when line is not empty', () {
      final editor = MarkdownEditorSpec(
        text: 'not blank',
        selection: UITextSelection.caret(
          const UITextPosition(line: 0, utf16Column: 9),
        ),
        commandHint: const MarkdownCommandHint(text: 'hint'),
        actions: const MarkdownEditorActions(openMenu: 'open-menu'),
      );
      expect(editor.commandHintVisible, isFalse);
    });

    test('hidden inside code fence', () {
      final editor = MarkdownEditorSpec(
        text: '```\n\n```',
        selection: UITextSelection.caret(
          const UITextPosition(line: 1, utf16Column: 0),
        ),
        commandHint: const MarkdownCommandHint(text: 'hint'),
      );
      expect(editor.commandHintVisible, isFalse);
    });

    test('hidden with invalid hint', () {
      final editor = MarkdownEditorSpec(
        text: 'title\n\nbody',
        selection: UITextSelection.caret(
          const UITextPosition(line: 1, utf16Column: 0),
        ),
        commandHint: const MarkdownCommandHint(text: ''),
      );
      expect(editor.commandHintVisible, isFalse);
    });
  });

  group('MarkdownEditorSpec.menuTriggerForTextInput', () {
    test('slash triggers slash menu', () {
      // Swift: markdownCommandHintAndMenuTriggersComeFromTheSpec
      final editor = MarkdownEditorSpec(
        text: 'title\n\nbody',
        selection: UITextSelection.caret(
          const UITextPosition(line: 1, utf16Column: 0),
        ),
        commandHint: const MarkdownCommandHint(
          text: "Type '/' for commands",
        ),
        actions: const MarkdownEditorActions(openMenu: 'open-menu'),
      );
      expect(
        editor.menuTriggerForTextInput('/'),
        MarkdownMenuTrigger.slash,
      );
    });

    test('backslash triggers palette', () {
      final editor = MarkdownEditorSpec(
        text: 'title\n\nbody',
        selection: UITextSelection.caret(
          const UITextPosition(line: 1, utf16Column: 0),
        ),
        actions: const MarkdownEditorActions(openMenu: 'open-menu'),
      );
      expect(
        editor.menuTriggerForTextInput('\\'),
        MarkdownMenuTrigger.palette,
      );
    });

    test('other input returns null', () {
      final editor = MarkdownEditorSpec(
        text: 'title\n\nbody',
        selection: UITextSelection.caret(
          const UITextPosition(line: 1, utf16Column: 0),
        ),
        actions: const MarkdownEditorActions(openMenu: 'open-menu'),
      );
      expect(editor.menuTriggerForTextInput('a'), isNull);
    });

    test('null when openMenu action is missing', () {
      // Swift: "the Rust reducer, not the renderer, decides whether slash opens a Menu"
      final editor = MarkdownEditorSpec(
        text: 'not blank',
        selection: UITextSelection.caret(
          const UITextPosition(line: 0, utf16Column: 9),
        ),
        actions: const MarkdownEditorActions(),
      );
      expect(editor.menuTriggerForTextInput('/'), isNull);
    });

    test('null when readOnly', () {
      final editor = MarkdownEditorSpec(
        text: 'title\n\nbody',
        selection: UITextSelection.caret(
          const UITextPosition(line: 1, utf16Column: 0),
        ),
        readOnly: true,
        actions: const MarkdownEditorActions(openMenu: 'open-menu'),
      );
      expect(editor.menuTriggerForTextInput('/'), isNull);
    });
  });
}
