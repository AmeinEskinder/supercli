/// Dart port of MarkdownEditorSpec command-hint and menu-trigger logic.
///
/// Swift source:
/// `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIProtocol.swift`
/// (MarkdownEditorSpec.commandHintVisible, menuTrigger(forTextInput:),
/// MarkdownCommandHint, MarkdownMenuTrigger, MarkdownEditorActions,
/// UITextPosition, UITextSelection, MarkdownPresentation)
///
/// This ports the pure model logic used by
/// `MarkdownInsertMenuTests.swift::markdownCommandHintAndMenuTriggersComeFromTheSpec`.
/// The native UI test (`nativeMarkdownSlashRoundTripPresentsAndActivatesTheAuthoritativeMenu`)
/// requires AppKit/SwiftUI and is not portable.
library;

/// Text position: line index + UTF-16 column.
class UITextPosition {
  final int line;
  final int utf16Column;

  const UITextPosition({required this.line, required this.utf16Column});

  @override
  bool operator ==(Object other) =>
      other is UITextPosition &&
      other.line == line &&
      other.utf16Column == utf16Column;

  @override
  int get hashCode => Object.hash(line, utf16Column);
}

/// Text selection: anchor + head positions.
class UITextSelection {
  final UITextPosition anchor;
  final UITextPosition head;

  const UITextSelection({required this.anchor, required this.head});

  /// Collapsed caret selection.
  factory UITextSelection.caret(UITextPosition position) =>
      UITextSelection(anchor: position, head: position);

  @override
  bool operator ==(Object other) =>
      other is UITextSelection &&
      other.anchor == anchor &&
      other.head == head;

  @override
  int get hashCode => Object.hash(anchor, head);
}

/// Markdown editor presentation mode.
enum MarkdownPresentation {
  source,
  preview,
  split,
}

/// Visibility rule for the command hint.
enum MarkdownCommandHintVisibility {
  cursorOnEmptyLineOutsideCodeFence,
}

/// Command hint model.
class MarkdownCommandHint {
  final String text;
  final MarkdownCommandHintVisibility visibility;

  const MarkdownCommandHint({
    required this.text,
    this.visibility = MarkdownCommandHintVisibility.cursorOnEmptyLineOutsideCodeFence,
  });

  /// Mirrors Swift `MarkdownCommandHint.isValid`.
  bool get isValid {
    if (text.isEmpty) return false;
    if (text.length > 4096) return false;
    for (final unit in text.codeUnits) {
      if (unit == 0 || unit == 13 || unit == 10) return false;
    }
    return true;
  }

  @override
  bool operator ==(Object other) =>
      other is MarkdownCommandHint &&
      other.text == text &&
      other.visibility == visibility;

  @override
  int get hashCode => Object.hash(text, visibility);
}

/// Menu trigger kinds.
enum MarkdownMenuTrigger {
  slash,
  palette,
}

/// Editor actions (action identifiers).
class MarkdownEditorActions {
  final String? replaceRange;
  final String? setSelection;
  final String? save;
  final String? undo;
  final String? redo;
  final String? setPresentation;
  final String? openMenu;

  const MarkdownEditorActions({
    this.replaceRange = 'replace-range',
    this.setSelection = 'set-selection',
    this.save = 'save',
    this.undo = 'undo',
    this.redo = 'redo',
    this.setPresentation = 'set-presentation',
    this.openMenu,
  });

  @override
  bool operator ==(Object other) =>
      other is MarkdownEditorActions &&
      other.replaceRange == replaceRange &&
      other.setSelection == setSelection &&
      other.save == save &&
      other.undo == undo &&
      other.redo == redo &&
      other.setPresentation == setPresentation &&
      other.openMenu == openMenu;

  @override
  int get hashCode => Object.hash(
        replaceRange,
        setSelection,
        save,
        undo,
        redo,
        setPresentation,
        openMenu,
      );
}

/// Minimal Markdown editor spec for command-hint and menu-trigger logic.
///
/// Mirrors the subset of Swift `MarkdownEditorSpec` needed for
/// `commandHintVisible` and `menuTrigger(forTextInput:)`.
class MarkdownEditorSpec {
  final String text;
  final UITextSelection selection;
  final MarkdownPresentation presentation;
  final bool readOnly;
  final String placeholder;
  final MarkdownCommandHint? commandHint;
  final MarkdownEditorActions actions;
  final bool hasInsertMenu;

  const MarkdownEditorSpec({
    required this.text,
    required this.selection,
    this.presentation = MarkdownPresentation.source,
    this.readOnly = false,
    this.placeholder = '',
    this.commandHint,
    this.actions = const MarkdownEditorActions(),
    this.hasInsertMenu = false,
  });

  /// Pure interpretation of the closed Rust visibility rule.
  /// Mirrors Swift `MarkdownEditorSpec.commandHintVisible`.
  bool get commandHintVisible {
    final hint = commandHint;
    if (hint == null || !hint.isValid) return false;
    if (presentation == MarkdownPresentation.preview) return false;
    if (selection.anchor != selection.head) return false;
    if (hasInsertMenu) return false;
    if (text.isEmpty && placeholder.isNotEmpty) return false;

    final lines = text.split('\n');
    final line = selection.head.line;
    if (line < 0 || line >= lines.length) return false;
    if (lines[line].isNotEmpty) return false;

    switch (hint.visibility) {
      case MarkdownCommandHintVisibility.cursorOnEmptyLineOutsideCodeFence:
        var insideFence = false;
        for (var index = 0; index <= line; index++) {
          if (lines[index].trim().startsWith('```')) {
            if (index == line) return false;
            insideFence = !insideFence;
          }
        }
        return !insideFence;
    }
  }

  /// Closed text-input triggers for the App-owned Menu action.
  /// Mirrors Swift `MarkdownEditorSpec.menuTrigger(forTextInput:)`.
  MarkdownMenuTrigger? menuTriggerForTextInput(String input) {
    if (readOnly || hasInsertMenu || actions.openMenu == null) return null;
    switch (input) {
      case '/':
        return MarkdownMenuTrigger.slash;
      case '\\':
        return MarkdownMenuTrigger.palette;
      default:
        return null;
    }
  }
}
