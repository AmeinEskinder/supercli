/// App-kit UI protocol: widget spec types (menu, media, surface, buttons,
/// footer, canvas, toggles).
/// Faithful wire port of the corresponding sections of
/// `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIProtocol.swift`
/// (4216 lines).
///
/// Covers `MarkdownPresentation`, `UIMenuItemRole`/`UIMenuAnchor`/
/// `UIMenuPresentation`, `UIMenuItemSpec`, `UIMenuSpec`,
/// `MarkdownCommandHintVisibility`, `MarkdownCommandHint`,
/// `MarkdownMenuTrigger`, `MarkdownEditorActions`, `MarkdownEditorSpec`,
/// `MediaFit`, `MediaPixelSize`, `MediaCellSize`, `MediaPointSize`,
/// `MediaBlobReference`, `MediaSource`, `MediaSpec`, `SurfaceReference`,
/// `SurfaceCellSize`, `SurfacePointSize`, `SurfaceViewportSize`,
/// `SurfaceBackground`, `SurfaceInputPolicy`, `SurfaceSpec`, `UIButtonRole`,
/// `UIButtonSpec`, `UIFooterActionRole`, `UIFooterActionSpec`,
/// `UIFooterActionsSpec`, `UICanvasSurfaceSpec`, `UICanvasControl`,
/// `CanvasPageSpec`, `UIToggleRole`, `UIToggleSpec`, `UICheckmarkSpec`,
/// `UIComponent`, plus the `ratioCeil`, `isPortableUIIdentifier`, and
/// `isFooterAccelerator` helpers.
///
/// Wire compatibility: JSON keys match the Swift `CodingKeys` exactly.
/// Decode-time validation mirrors the Swift `init(from:)` guards; violations
/// throw [FormatException].
///
/// NOTE: `appkit_protocol.dart` contains older render-subset stubs named
/// `MediaSpec`, `SurfaceSpec`, `TextBoxSpec`, `CanvasPageSpec`,
/// `MarkdownEditorSpec`, and `MenuSpec`/`MenuItemSpec` used by
/// `appkit_renderer.dart`. Those stubs are intentionally NOT edited here; the
/// classes in this file are the wire-faithful ports using the exact Swift
/// type names. Do not import both files into the same library without a
/// prefix.
library;

import 'dart:convert';

bool _listEq<T>(List<T> a, List<T> b) {
  if (identical(a, b)) return true;
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

int _utf8Length(String s) => utf8.encode(s).length;

const int _uInt32Max = 4294967295;
const int _uInt16Max = 65535;

// ---------------------------------------------------------------------------
// Protocol identity and capabilities. Mirrors the protocol declaration at
// the top of the Swift `UIProtocol.swift`.
// ---------------------------------------------------------------------------

/// Wire protocol identity and capability strings. Mirrors the protocol
/// constants at the top of the Swift `UIProtocol.swift`.
abstract final class UIProtocol {
  static const name = 'supercli.ui';
  static const minimumVersion = 1;
  static const maximumVersion = 1;
  static const version = maximumVersion;

  static const deltaCapability = 'serverDelta';
  static const markdownEditorCapability = 'markdownEditor';
  static const markdownCommandHintCapability = 'markdownCommandHint';
  static const menuCapability = 'menu';
  static const menuAnchorCapability = 'menuAnchor';
  static const mediaCapability = 'media';
  static const pageCapability = 'page';
  static const listCapability = 'list';
  static const listItemCapability = 'listItem';
  static const listItemMetadataCapability = 'listItemMetadata';
  static const listItemActivateCapability = 'listItemActivate';
  static const listItemPresentationCapability = 'listItemPresentation';
  static const listItemStyledTextCapability = 'listItemStyledText';
  static const listItemRoleCapability = 'listItemRole';
  static const listSelectionCapability = 'listSelection';
  static const statusSymbolCapability = 'statusSymbol';
  static const badgeCapability = 'badge';
  static const sparklineCapability = 'sparkline';
  static const barChartCapability = 'barChart';
  static const lineChartCapability = 'lineChart';
  static const gaugeCapability = 'gauge';
  static const toggleCapability = 'toggle';
  static const inputCapability = 'input';
  static const buttonCapability = 'button';
  static const pageTabsCapability = 'pageTabs';
  static const pageToolbarCapability = 'pageToolbar';
  static const pageBackCapability = 'pageBack';
  static const footerActionsCapability = 'footerActions';
  static const footerStatusCapability = 'footerStatus';
  static const contentCapability = 'content';
  static const contentSelectionCapability = 'contentSelection';
  static const surfaceCapability = 'surface';
  static const canvasPageCapability = 'canvasPage';
  static const treeCapability = 'tree';
  static const treeHierarchyCapability = 'treeHierarchy';
  static const treeFilterCapability = 'treeFilter';
  static const treeParentCapability = 'treeParent';
  static const textBoxCapability = 'textBox';

  /// Components renderable without an injected Host-owned presenter.
  /// Mirrors the Swift `supportedComponentCapabilities` list.
  static const supportedComponentCapabilities = [
    markdownEditorCapability,
    textBoxCapability,
    markdownCommandHintCapability,
    menuCapability,
    menuAnchorCapability,
    mediaCapability,
    pageCapability,
    listCapability,
    listItemCapability,
    listItemMetadataCapability,
    listItemActivateCapability,
    listItemPresentationCapability,
    listItemStyledTextCapability,
    listItemRoleCapability,
    listSelectionCapability,
    statusSymbolCapability,
    badgeCapability,
    sparklineCapability,
    barChartCapability,
    lineChartCapability,
    gaugeCapability,
    toggleCapability,
    inputCapability,
    buttonCapability,
    pageTabsCapability,
    pageToolbarCapability,
    pageBackCapability,
    footerActionsCapability,
    footerStatusCapability,
    contentCapability,
    contentSelectionCapability,
    treeCapability,
    treeHierarchyCapability,
    treeFilterCapability,
    treeParentCapability,
  ];

  static const int _maximumWireVersion = 4294967295; // UInt32.max

  /// Mirrors the Swift `supports(_:)` version check.
  static bool supports(int version) =>
      version >= minimumVersion && version <= maximumVersion;

  /// Mirrors the Swift `negotiate(minimum:maximum:)`.
  static int? negotiate({required int minimum, required int maximum}) {
    if (minimum <= 0 || minimum > maximum || maximum > _maximumWireVersion) {
      return null;
    }
    final sharedMinimum = minimum > minimumVersion ? minimum : minimumVersion;
    final sharedMaximum = maximum < maximumVersion ? maximum : maximumVersion;
    return sharedMinimum <= sharedMaximum ? sharedMaximum : null;
  }
}

/// Mirrors Swift `ratioCeil(_:_:_:)`: `ceil(value * numerator / denominator)`
/// with clamping, using unsigned 64-bit math.
int ratioCeil(int value, int numerator, int denominator) {
  final v = value < 0 ? 0 : value;
  final n = numerator < 1 ? 1 : numerator;
  final d = denominator < 1 ? 1 : denominator;
  // Dart ints are 64-bit; the product fits in 64 bits for any realistic
  // layout input, matching the Swift UInt64 math.
  final scaled = v * n;
  final resolved = scaled ~/ d + (scaled % d == 0 ? 0 : 1);
  return resolved > _uInt32Max ? _uInt32Max : resolved;
}

/// Mirrors Swift `isPortableUIIdentifier(_:)`.
bool isPortableUIIdentifier(String value) {
  if (value.isEmpty || _utf8Length(value) > 256) return false;
  const punctuation = {0x2E, 0x5F, 0x3A, 0x2F, 0x2D}; // . _ : / -
  return value.codeUnits.every((c) =>
      (c >= 48 && c <= 57) ||
      (c >= 65 && c <= 90) ||
      (c >= 97 && c <= 122) ||
      punctuation.contains(c));
}

/// Mirrors Swift `isFooterAccelerator(_:)`.
bool isFooterAccelerator(String value) {
  if (value == 'escape' || value == 'enter' || value == 'space') return true;
  if (value.startsWith('ctrl+')) {
    final key = value.substring(5);
    return key.length == 1 &&
        ((key.codeUnitAt(0) >= 48 && key.codeUnitAt(0) <= 57) ||
            (key.codeUnitAt(0) >= 65 && key.codeUnitAt(0) <= 90) ||
            (key.codeUnitAt(0) >= 97 && key.codeUnitAt(0) <= 122));
  }
  return value.length == 1 &&
      value.codeUnitAt(0) >= 33 &&
      value.codeUnitAt(0) <= 126;
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Text primitives
// ---------------------------------------------------------------------------

/// Mirrors Swift `UITextPosition` (Comparable by line, then utf16Column).
final class UITextPosition implements Comparable<UITextPosition> {
  const UITextPosition({required this.line, required this.utf16Column});

  final int line;
  final int utf16Column;

  factory UITextPosition.fromJson(Map<String, dynamic> json) =>
      UITextPosition(
        line: json['line'] as int,
        utf16Column: json['utf16Column'] as int,
      );

  Map<String, dynamic> toJson() => {'line': line, 'utf16Column': utf16Column};

  @override
  int compareTo(UITextPosition other) {
    if (line != other.line) return line.compareTo(other.line);
    return utf16Column.compareTo(other.utf16Column);
  }

  @override
  bool operator ==(Object other) =>
      other is UITextPosition &&
      other.line == line &&
      other.utf16Column == utf16Column;

  @override
  int get hashCode => Object.hash(line, utf16Column);
}

/// Mirrors Swift `UITextRange`.
final class UITextRange {
  const UITextRange({required this.start, required this.end});

  final UITextPosition start;
  final UITextPosition end;

  factory UITextRange.fromJson(Map<String, dynamic> json) => UITextRange(
        start: UITextPosition.fromJson(json['start'] as Map<String, dynamic>),
        end: UITextPosition.fromJson(json['end'] as Map<String, dynamic>),
      );

  Map<String, dynamic> toJson() =>
      {'start': start.toJson(), 'end': end.toJson()};

  @override
  bool operator ==(Object other) =>
      other is UITextRange && other.start == start && other.end == end;

  @override
  int get hashCode => Object.hash(start, end);
}

/// Mirrors Swift `UITextSelection`.
final class UITextSelection {
  const UITextSelection({required this.anchor, required this.head});

  final UITextPosition anchor;
  final UITextPosition head;

  /// Mirrors Swift `UITextSelection.caret(_:)`.
  factory UITextSelection.caret(UITextPosition position) =>
      UITextSelection(anchor: position, head: position);

  factory UITextSelection.fromJson(Map<String, dynamic> json) =>
      UITextSelection(
        anchor: UITextPosition.fromJson(json['anchor'] as Map<String, dynamic>),
        head: UITextPosition.fromJson(json['head'] as Map<String, dynamic>),
      );

  Map<String, dynamic> toJson() =>
      {'anchor': anchor.toJson(), 'head': head.toJson()};

  @override
  bool operator ==(Object other) =>
      other is UITextSelection &&
      other.anchor == anchor &&
      other.head == head;

  @override
  int get hashCode => Object.hash(anchor, head);
}

/// Mirrors Swift `UITextEdit`.
final class UITextEdit {
  const UITextEdit({required this.range, required this.text});

  final UITextRange range;
  final String text;

  factory UITextEdit.fromJson(Map<String, dynamic> json) => UITextEdit(
        range: UITextRange.fromJson(json['range'] as Map<String, dynamic>),
        text: json['text'] as String,
      );

  Map<String, dynamic> toJson() => {'range': range.toJson(), 'text': text};

  @override
  bool operator ==(Object other) =>
      other is UITextEdit && other.range == range && other.text == text;

  @override
  int get hashCode => Object.hash(range, text);
}

// Menu
// ---------------------------------------------------------------------------

/// Mirrors Swift `MarkdownPresentation`.
enum MarkdownPresentation {
  source,
  preview,
  split;

  static MarkdownPresentation fromJson(String v) =>
      MarkdownPresentation.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `UIMenuItemRole` (`standard` encodes as `"default"`).
enum UIMenuItemRole {
  standard('default'),
  danger('danger');

  const UIMenuItemRole(this.wire);
  final String wire;

  static UIMenuItemRole fromJson(String v) =>
      UIMenuItemRole.values.firstWhere((e) => e.wire == v,
          orElse: () => throw FormatException('Unknown UIMenuItemRole $v'));
  String toJson() => wire;
}

/// Mirrors Swift `UIMenuAnchor`.
enum UIMenuAnchor {
  control,
  caret,
  pointer;

  static UIMenuAnchor fromJson(String v) =>
      UIMenuAnchor.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `UIMenuPresentation`.
enum UIMenuPresentation {
  popup,
  context;

  static UIMenuPresentation fromJson(String v) =>
      UIMenuPresentation.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `UIMenuItemSpec`.
final class UIMenuItemSpec {
  const UIMenuItemSpec({
    required this.id,
    required this.label,
    required this.action,
    this.hint,
    this.disabled = false,
    this.role = UIMenuItemRole.standard,
  });

  final String id;
  final String label;
  final String action;
  final String? hint;
  final bool disabled;
  final UIMenuItemRole role;

  factory UIMenuItemSpec.fromJson(Map<String, dynamic> json) =>
      UIMenuItemSpec(
        id: json['id'] as String,
        label: json['label'] as String,
        action: json['action'] as String,
        hint: json['hint'] as String?,
        disabled: json['disabled'] as bool? ?? false,
        role: json['role'] == null
            ? UIMenuItemRole.standard
            : UIMenuItemRole.fromJson(json['role'] as String),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'action': action,
        if (hint != null) 'hint': hint,
        'disabled': disabled,
        'role': role.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIMenuItemSpec &&
      other.id == id &&
      other.label == label &&
      other.action == action &&
      other.hint == hint &&
      other.disabled == disabled &&
      other.role == role;

  @override
  int get hashCode =>
      Object.hash(id, label, action, hint, disabled, role);
}

/// Mirrors Swift `UIMenuSpec`.
final class UIMenuSpec {
  const UIMenuSpec({
    required this.label,
    this.presentation = UIMenuPresentation.popup,
    this.anchor = UIMenuAnchor.control,
    required this.items,
    this.selectedID,
    this.dismiss,
  });

  final String label;
  final UIMenuPresentation presentation;
  final UIMenuAnchor anchor;
  final List<UIMenuItemSpec> items;
  final String? selectedID;
  final String? dismiss;

  factory UIMenuSpec.fromJson(Map<String, dynamic> json) => UIMenuSpec(
        label: json['label'] as String,
        presentation: json['presentation'] == null
            ? UIMenuPresentation.popup
            : UIMenuPresentation.fromJson(json['presentation'] as String),
        anchor: json['anchor'] == null
            ? UIMenuAnchor.control
            : UIMenuAnchor.fromJson(json['anchor'] as String),
        items: (json['items'] as List)
            .map((i) => UIMenuItemSpec.fromJson(i as Map<String, dynamic>))
            .toList(),
        selectedID: json['selectedId'] as String?,
        dismiss: json['dismiss'] as String?,
      );

  Map<String, dynamic> toJson() => {
        'label': label,
        'presentation': presentation.toJson(),
        'anchor': anchor.toJson(),
        'items': items.map((i) => i.toJson()).toList(),
        if (selectedID != null) 'selectedId': selectedID,
        if (dismiss != null) 'dismiss': dismiss,
      };

  /// Mirrors Swift `UIMenuSpec.requiredCapabilities`.
  List<String>? get requiredCapabilities {
    final ids = items.map((i) => i.id).toSet();
    if (items.length > 256 ||
        ids.length != items.length ||
        (selectedID != null && !ids.contains(selectedID)) ||
        items.any((i) => i.id == selectedID && i.disabled)) {
      return null;
    }
    return [UIProtocol.menuCapability, UIProtocol.menuAnchorCapability];
  }

  @override
  bool operator ==(Object other) =>
      other is UIMenuSpec &&
      other.label == label &&
      other.presentation == presentation &&
      other.anchor == anchor &&
      _listEq(other.items, items) &&
      other.selectedID == selectedID &&
      other.dismiss == dismiss;

  @override
  int get hashCode =>
      Object.hash(label, presentation, anchor, items.length, selectedID, dismiss);
}

/// Mirrors Swift `MarkdownCommandHintVisibility`.
enum MarkdownCommandHintVisibility {
  cursorOnEmptyLineOutsideCodeFence;

  static MarkdownCommandHintVisibility fromJson(String v) =>
      MarkdownCommandHintVisibility.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `MarkdownCommandHint`.
final class MarkdownCommandHint {
  const MarkdownCommandHint({
    required this.text,
    this.visibility =
        MarkdownCommandHintVisibility.cursorOnEmptyLineOutsideCodeFence,
  });

  final String text;
  final MarkdownCommandHintVisibility visibility;

  /// Mirrors Swift `MarkdownCommandHint.isValid`.
  bool get isValid =>
      text.isNotEmpty &&
      _utf8Length(text) <= 4096 &&
      !text.contains('\x00') &&
      !text.contains('\r') &&
      !text.contains('\n');

  factory MarkdownCommandHint.fromJson(Map<String, dynamic> json) =>
      MarkdownCommandHint(
        text: json['text'] as String,
        visibility: json['visibility'] == null
            ? MarkdownCommandHintVisibility.cursorOnEmptyLineOutsideCodeFence
            : MarkdownCommandHintVisibility.fromJson(
                json['visibility'] as String),
      );

  Map<String, dynamic> toJson() => {
        'text': text,
        'visibility': visibility.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is MarkdownCommandHint &&
      other.text == text &&
      other.visibility == visibility;

  @override
  int get hashCode => Object.hash(text, visibility);
}

/// Mirrors Swift `MarkdownMenuTrigger`.
enum MarkdownMenuTrigger {
  slash,
  palette;

  static MarkdownMenuTrigger fromJson(String v) =>
      MarkdownMenuTrigger.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `MarkdownEditorActions`.
final class MarkdownEditorActions {
  const MarkdownEditorActions({
    this.replaceRange = 'replace-range',
    this.setSelection = 'set-selection',
    this.save = 'save',
    this.undo = 'undo',
    this.redo = 'redo',
    this.setPresentation = 'set-presentation',
    this.openMenu,
  });

  final String? replaceRange;
  final String? setSelection;
  final String? save;
  final String? undo;
  final String? redo;
  final String? setPresentation;
  final String? openMenu;

  factory MarkdownEditorActions.fromJson(Map<String, dynamic> json) {
    // Swift uses synthesized Codable: absent keys decode as nil here, so the
    // Dart port keeps explicit nulls rather than Swift's init defaults.
    return MarkdownEditorActions(
      replaceRange: json['replaceRange'] as String?,
      setSelection: json['setSelection'] as String?,
      save: json['save'] as String?,
      undo: json['undo'] as String?,
      redo: json['redo'] as String?,
      setPresentation: json['setPresentation'] as String?,
      openMenu: json['openMenu'] as String?,
    );
  }

  Map<String, dynamic> toJson() => {
        if (replaceRange != null) 'replaceRange': replaceRange,
        if (setSelection != null) 'setSelection': setSelection,
        if (save != null) 'save': save,
        if (undo != null) 'undo': undo,
        if (redo != null) 'redo': redo,
        if (setPresentation != null) 'setPresentation': setPresentation,
        if (openMenu != null) 'openMenu': openMenu,
      };

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
      replaceRange, setSelection, save, undo, redo, setPresentation, openMenu);
}

/// Mirrors Swift `MarkdownEditorSpec`.
final class MarkdownEditorSpec {
  const MarkdownEditorSpec({
    required this.text,
    required this.anchorLine,
    required this.anchorColumn,
    required this.headLine,
    required this.headColumn,
    this.presentation = MarkdownPresentation.source,
    this.readOnly = false,
    this.dirty = false,
    this.placeholder = '',
    this.commandHint,
    this.title,
    this.back,
    this.actions = const MarkdownEditorActions(),
    this.insertMenu,
    this.contextMenu,
    this.footer = const UIFooterActionsSpec(),
  });

  // Selection is a UITextSelection (anchor/head line+utf16Column); this file
  // stays free of the messages file's types by storing the two pairs.
  final String text;
  final int anchorLine;
  final int anchorColumn;
  final int headLine;
  final int headColumn;
  final MarkdownPresentation presentation;
  final bool readOnly;
  final bool dirty;
  final String placeholder;
  final MarkdownCommandHint? commandHint;
  final String? title;
  final String? back;
  final MarkdownEditorActions actions;
  final UIMenuSpec? insertMenu;
  final UIMenuSpec? contextMenu;
  final UIFooterActionsSpec footer;

  factory MarkdownEditorSpec.fromJson(Map<String, dynamic> json) {
    final selection = json['selection'] as Map<String, dynamic>;
    final anchor = selection['anchor'] as Map<String, dynamic>;
    final head = selection['head'] as Map<String, dynamic>;
    return MarkdownEditorSpec(
      text: json['text'] as String,
      anchorLine: anchor['line'] as int,
      anchorColumn: anchor['utf16Column'] as int,
      headLine: head['line'] as int,
      headColumn: head['utf16Column'] as int,
      presentation: json['presentation'] == null
          ? MarkdownPresentation.source
          : MarkdownPresentation.fromJson(json['presentation'] as String),
      readOnly: json['readOnly'] as bool? ?? false,
      dirty: json['dirty'] as bool? ?? false,
      placeholder: json['placeholder'] as String? ?? '',
      commandHint: json['commandHint'] == null
          ? null
          : MarkdownCommandHint.fromJson(
              json['commandHint'] as Map<String, dynamic>),
      title: json['title'] as String?,
      back: json['back'] as String?,
      actions: json['actions'] == null
          ? const MarkdownEditorActions()
          : MarkdownEditorActions.fromJson(
              json['actions'] as Map<String, dynamic>),
      insertMenu: json['insertMenu'] == null
          ? null
          : UIMenuSpec.fromJson(json['insertMenu'] as Map<String, dynamic>),
      contextMenu: json['contextMenu'] == null
          ? null
          : UIMenuSpec.fromJson(json['contextMenu'] as Map<String, dynamic>),
      footer: json['footer'] == null
          ? const UIFooterActionsSpec()
          : UIFooterActionsSpec.fromJson(
              json['footer'] as Map<String, dynamic>),
    );
  }

  Map<String, dynamic> toJson() => {
        'text': text,
        'selection': {
          'anchor': {'line': anchorLine, 'utf16Column': anchorColumn},
          'head': {'line': headLine, 'utf16Column': headColumn},
        },
        'presentation': presentation.toJson(),
        'readOnly': readOnly,
        'dirty': dirty,
        'placeholder': placeholder,
        if (commandHint != null) 'commandHint': commandHint!.toJson(),
        if (title != null) 'title': title,
        if (back != null) 'back': back,
        'actions': actions.toJson(),
        if (insertMenu != null) 'insertMenu': insertMenu!.toJson(),
        if (contextMenu != null) 'contextMenu': contextMenu!.toJson(),
        'footer': footer.toJson(),
      };

  /// Mirrors Swift `MarkdownEditorSpec.commandHintVisible`: pure
  /// interpretation of the closed visibility rule.
  bool get commandHintVisible {
    final hint = commandHint;
    if (hint == null ||
        !hint.isValid ||
        presentation == MarkdownPresentation.preview ||
        !(anchorLine == headLine && anchorColumn == headColumn) ||
        insertMenu != null ||
        (text.isEmpty && placeholder.isNotEmpty)) {
      return false;
    }
    final lines = text.split('\n');
    final line = headLine;
    if (line < 0 || line >= lines.length || lines[line].isNotEmpty) {
      return false;
    }
    switch (hint.visibility) {
      case MarkdownCommandHintVisibility.cursorOnEmptyLineOutsideCodeFence:
        var insideFence = false;
        for (var index = 0; index <= line; index++) {
          if (lines[index].trimLeft().startsWith('```')) {
            if (index == line) return false;
            insideFence = !insideFence;
          }
        }
        return !insideFence;
    }
  }

  /// Mirrors Swift `MarkdownEditorSpec.menuTrigger(forTextInput:)`.
  MarkdownMenuTrigger? menuTriggerForTextInput(String input) {
    if (readOnly || insertMenu != null || actions.openMenu == null) {
      return null;
    }
    switch (input) {
      case '/':
        return MarkdownMenuTrigger.slash;
      case '\\':
        return MarkdownMenuTrigger.palette;
      default:
        return null;
    }
  }

  @override
  bool operator ==(Object other) =>
      other is MarkdownEditorSpec &&
      other.text == text &&
      other.anchorLine == anchorLine &&
      other.anchorColumn == anchorColumn &&
      other.headLine == headLine &&
      other.headColumn == headColumn &&
      other.presentation == presentation &&
      other.readOnly == readOnly &&
      other.dirty == dirty &&
      other.placeholder == placeholder &&
      other.commandHint == commandHint &&
      other.title == title &&
      other.back == back &&
      other.actions == actions &&
      other.insertMenu == insertMenu &&
      other.contextMenu == contextMenu &&
      other.footer == footer;

  @override
  int get hashCode => Object.hash(
      text,
      anchorLine,
      anchorColumn,
      headLine,
      headColumn,
      presentation,
      readOnly,
      dirty,
      placeholder,
      commandHint,
      title,
      back,
      actions,
      insertMenu,
      contextMenu,
      footer);
}

// ---------------------------------------------------------------------------
// Media
// ---------------------------------------------------------------------------

/// Mirrors Swift `MediaFit`.
enum MediaFit {
  contain,
  cover,
  fill;

  static MediaFit fromJson(String v) => MediaFit.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `MediaPixelSize`. Dimensions must be positive UInt32.
final class MediaPixelSize {
  const MediaPixelSize({required this.w, required this.h});

  final int w;
  final int h;

  factory MediaPixelSize.fromJson(Map<String, dynamic> json) {
    final w = json['w'] as int;
    final h = json['h'] as int;
    if (!(w >= 1 && w <= _uInt32Max && h >= 1 && h <= _uInt32Max)) {
      throw FormatException(
          'Media intrinsic dimensions must be positive UInt32 values', json);
    }
    return MediaPixelSize(w: w, h: h);
  }

  Map<String, dynamic> toJson() => {'w': w, 'h': h};

  @override
  bool operator ==(Object other) =>
      other is MediaPixelSize && other.w == w && other.h == h;

  @override
  int get hashCode => Object.hash(w, h);
}

/// Mirrors Swift `MediaCellSize`: at least one positive UInt16 axis.
final class MediaCellSize {
  const MediaCellSize({this.w, this.h});

  final int? w;
  final int? h;

  factory MediaCellSize.fromJson(Map<String, dynamic> json) {
    final w = json['w'] as int?;
    final h = json['h'] as int?;
    if (!((w != null || h != null) &&
        (w == null || (w >= 1 && w <= _uInt16Max)) &&
        (h == null || (h >= 1 && h <= _uInt16Max)))) {
      throw FormatException(
          'Media cell size needs at least one positive UInt16 axis', json);
    }
    return MediaCellSize(w: w, h: h);
  }

  Map<String, dynamic> toJson() => {
        if (w != null) 'w': w,
        if (h != null) 'h': h,
      };

  @override
  bool operator ==(Object other) =>
      other is MediaCellSize && other.w == w && other.h == h;

  @override
  int get hashCode => Object.hash(w, h);
}

/// Mirrors Swift `MediaPointSize`: at least one positive UInt32 axis.
final class MediaPointSize {
  const MediaPointSize({this.w, this.h});

  final int? w;
  final int? h;

  factory MediaPointSize.fromJson(Map<String, dynamic> json) {
    final w = json['w'] as int?;
    final h = json['h'] as int?;
    if (!((w != null || h != null) &&
        (w == null || (w >= 1 && w <= _uInt32Max)) &&
        (h == null || (h >= 1 && h <= _uInt32Max)))) {
      throw FormatException(
          'Media point size needs at least one positive UInt32 axis', json);
    }
    return MediaPointSize(w: w, h: h);
  }

  Map<String, dynamic> toJson() => {
        if (w != null) 'w': w,
        if (h != null) 'h': h,
      };

  @override
  bool operator ==(Object other) =>
      other is MediaPointSize && other.w == w && other.h == h;

  @override
  int get hashCode => Object.hash(w, h);
}

/// Mirrors Swift `MediaBlobReference`.
final class MediaBlobReference {
  const MediaBlobReference({
    required this.sha256,
    required this.mediaType,
    required this.byteLength,
  });

  final String sha256;
  final String mediaType;
  final int byteLength;

  factory MediaBlobReference.fromJson(Map<String, dynamic> json) =>
      MediaBlobReference(
        sha256: json['sha256'] as String,
        mediaType: json['mediaType'] as String,
        byteLength: json['byteLength'] as int,
      );

  Map<String, dynamic> toJson() => {
        'sha256': sha256,
        'mediaType': mediaType,
        'byteLength': byteLength,
      };

  @override
  bool operator ==(Object other) =>
      other is MediaBlobReference &&
      other.sha256 == sha256 &&
      other.mediaType == mediaType &&
      other.byteLength == byteLength;

  @override
  int get hashCode => Object.hash(sha256, mediaType, byteLength);
}

bool _isAlnum(int c) =>
    (c >= 48 && c <= 57) || (c >= 65 && c <= 90) || (c >= 97 && c <= 122);

/// Mirrors Swift `MediaSource.validateMediaType`: must be an `image/` MIME
/// type of at most 127 bytes over alphanumerics and `!#$&^_.+-/`.
bool isValidMediaType(String value) {
  const punctuation = {
    0x21, 0x23, 0x24, 0x26, 0x5E, 0x5F, 0x2E, 0x2B, 0x2D, 0x2F
  }; // ! # $ & ^ _ . + - /
  return value.startsWith('image/') &&
      _utf8Length(value) <= 127 &&
      value.codeUnits.every((c) => _isAlnum(c) || punctuation.contains(c));
}

/// Mirrors Swift `MediaSource` with its custom kind-discriminated Codable.
sealed class MediaSource {
  const MediaSource();

  factory MediaSource.fromJson(Map<String, dynamic> json) {
    switch (json['kind'] as String) {
      case 'path':
        final path = json['path'] as String;
        if (path.isEmpty || _utf8Length(path) > 4096 || path.contains('\x00')) {
          throw FormatException(
              'Media path must contain 1...4096 non-NUL bytes', json);
        }
        return MediaSourcePath(path);
      case 'inline':
        final mediaType = json['mediaType'] as String;
        final base64 = json['base64'] as String;
        if (!isValidMediaType(mediaType)) {
          throw FormatException(
              'Media mediaType must be an image MIME type', json);
        }
        // Mirrors Swift: valid base64, canonical re-encoding, at most 256 KiB
        // decoded. (Dart's decoder is strict about padding where Swift's is
        // lenient; the canonical round-trip check dominates either way.)
        List<int> data;
        try {
          data = base64Decode(base64);
        } on FormatException {
          throw FormatException(
              'Inline Media must be valid base64 containing at most 256 KiB',
              json);
        }
        if (_utf8Length(base64) > 349528 ||
            base64Encode(data) != base64 ||
            data.isEmpty ||
            data.length > 262144) {
          throw FormatException(
              'Inline Media must be valid base64 containing at most 256 KiB',
              json);
        }
        return MediaSourceInline(mediaType: mediaType, base64: base64);
      case 'blob':
        final reference = MediaBlobReference.fromJson(json);
        if (!isValidMediaType(reference.mediaType)) {
          throw FormatException(
              'Media mediaType must be an image MIME type', json);
        }
        final sha = reference.sha256;
        if (!(sha.length == 64 &&
            sha.codeUnits.every((c) =>
                (c >= 48 && c <= 57) || (c >= 97 && c <= 102)) &&
            reference.byteLength >= 1 &&
            reference.byteLength <= 9007199254740991)) {
          throw FormatException('Media blob metadata is invalid', json);
        }
        return MediaSourceBlob(reference);
      default:
        throw FormatException('Unknown MediaSource kind', json);
    }
  }

  Map<String, dynamic> toJson();
}

final class MediaSourcePath extends MediaSource {
  const MediaSourcePath(this.path);
  final String path;
  @override
  Map<String, dynamic> toJson() => {'kind': 'path', 'path': path};
  @override
  bool operator ==(Object other) =>
      other is MediaSourcePath && other.path == path;
  @override
  int get hashCode => path.hashCode;
}

final class MediaSourceInline extends MediaSource {
  const MediaSourceInline({required this.mediaType, required this.base64});
  final String mediaType;
  final String base64;
  @override
  Map<String, dynamic> toJson() =>
      {'kind': 'inline', 'mediaType': mediaType, 'base64': base64};
  @override
  bool operator ==(Object other) =>
      other is MediaSourceInline &&
      other.mediaType == mediaType &&
      other.base64 == base64;
  @override
  int get hashCode => Object.hash(mediaType, base64);
}

final class MediaSourceBlob extends MediaSource {
  const MediaSourceBlob(this.reference);
  final MediaBlobReference reference;
  @override
  Map<String, dynamic> toJson() => {
        'kind': 'blob',
        'sha256': reference.sha256,
        'mediaType': reference.mediaType,
        'byteLength': reference.byteLength,
      };
  @override
  bool operator ==(Object other) =>
      other is MediaSourceBlob && other.reference == reference;
  @override
  int get hashCode => reference.hashCode;
}

/// Mirrors Swift `MediaSpec`, including `resolvedPointSize`.
final class MediaSpec {
  const MediaSpec({
    required this.source,
    required this.intrinsic,
    this.cells,
    this.points,
    this.fit = MediaFit.contain,
    required this.alt,
    this.activate,
  });

  final MediaSource source;
  final MediaPixelSize intrinsic;
  final MediaCellSize? cells;
  final MediaPointSize? points;
  final MediaFit fit;
  final String alt;
  final String? activate;

  factory MediaSpec.fromJson(Map<String, dynamic> json) {
    final alt = json['alt'] as String;
    if (_utf8Length(alt) > 16384) {
      throw FormatException(
          'Media alt text must contain at most 16384 bytes', json);
    }
    final activate = json['activate'] as String?;
    if (activate != null) {
      const punctuation = {46, 95, 58, 47, 45}; // . _ : / -
      if (!(activate.isNotEmpty &&
          _utf8Length(activate) <= 256 &&
          activate.codeUnits.every((c) => _isAlnum(c) || punctuation.contains(c)))) {
        throw FormatException(
            'Media activate must be a portable action identifier', json);
      }
    }
    return MediaSpec(
      source:
          MediaSource.fromJson(json['source'] as Map<String, dynamic>),
      intrinsic:
          MediaPixelSize.fromJson(json['intrinsic'] as Map<String, dynamic>),
      cells: json['cells'] == null
          ? null
          : MediaCellSize.fromJson(json['cells'] as Map<String, dynamic>),
      points: json['points'] == null
          ? null
          : MediaPointSize.fromJson(json['points'] as Map<String, dynamic>),
      fit: json['fit'] == null
          ? MediaFit.contain
          : MediaFit.fromJson(json['fit'] as String),
      alt: alt,
      activate: activate,
    );
  }

  Map<String, dynamic> toJson() => {
        'source': source.toJson(),
        'intrinsic': intrinsic.toJson(),
        if (cells != null) 'cells': cells!.toJson(),
        if (points != null) 'points': points!.toJson(),
        'fit': fit.toJson(),
        'alt': alt,
        if (activate != null) 'activate': activate,
      };

  /// Mirrors Swift `MediaSpec.resolvedPointSize`.
  ({int w, int h}) get resolvedPointSize {
    final p = points;
    if (p == null) return (w: intrinsic.w, h: intrinsic.h);
    final pw = p.w;
    final ph = p.h;
    if (pw != null && ph != null) return (w: pw, h: ph);
    if (pw != null) return (w: pw, h: ratioCeil(pw, intrinsic.h, intrinsic.w));
    if (ph != null) return (w: ratioCeil(ph, intrinsic.w, intrinsic.h), h: ph);
    return (w: intrinsic.w, h: intrinsic.h);
  }

  @override
  bool operator ==(Object other) =>
      other is MediaSpec &&
      other.source == source &&
      other.intrinsic == intrinsic &&
      other.cells == cells &&
      other.points == points &&
      other.fit == fit &&
      other.alt == alt &&
      other.activate == activate;

  @override
  int get hashCode =>
      Object.hash(source, intrinsic, cells, points, fit, alt, activate);
}

// ---------------------------------------------------------------------------
// Surface
// ---------------------------------------------------------------------------

/// Mirrors Swift `SurfaceReference`: an opaque route resolved by the
/// authenticated Host (not a socket path, URL, or credential).
final class SurfaceReference {
  const SurfaceReference({required this.sessionID, required this.streamID});

  final String sessionID;
  final String streamID;

  factory SurfaceReference.fromJson(Map<String, dynamic> json) =>
      SurfaceReference(
        sessionID: json['sessionId'] as String,
        streamID: json['streamId'] as String,
      );

  Map<String, dynamic> toJson() =>
      {'sessionId': sessionID, 'streamId': streamID};

  @override
  bool operator ==(Object other) =>
      other is SurfaceReference &&
      other.sessionID == sessionID &&
      other.streamID == streamID;

  @override
  int get hashCode => Object.hash(sessionID, streamID);
}

/// Mirrors Swift `SurfaceCellSize`: at least one positive UInt16 axis.
final class SurfaceCellSize {
  const SurfaceCellSize({this.w, this.h});

  final int? w;
  final int? h;

  factory SurfaceCellSize.fromJson(Map<String, dynamic> json) {
    final w = json['w'] as int?;
    final h = json['h'] as int?;
    if (!((w != null || h != null) &&
        (w == null || (w >= 1 && w <= _uInt16Max)) &&
        (h == null || (h >= 1 && h <= _uInt16Max)))) {
      throw FormatException(
          'Surface cell size needs at least one positive UInt16 axis', json);
    }
    return SurfaceCellSize(w: w, h: h);
  }

  Map<String, dynamic> toJson() => {
        if (w != null) 'w': w,
        if (h != null) 'h': h,
      };

  @override
  bool operator ==(Object other) =>
      other is SurfaceCellSize && other.w == w && other.h == h;

  @override
  int get hashCode => Object.hash(w, h);
}

/// Mirrors Swift `SurfacePointSize`: at least one positive UInt32 axis.
final class SurfacePointSize {
  const SurfacePointSize({this.w, this.h});

  final int? w;
  final int? h;

  factory SurfacePointSize.fromJson(Map<String, dynamic> json) {
    final w = json['w'] as int?;
    final h = json['h'] as int?;
    if (!((w != null || h != null) &&
        (w == null || (w >= 1 && w <= _uInt32Max)) &&
        (h == null || (h >= 1 && h <= _uInt32Max)))) {
      throw FormatException(
          'Surface point size needs at least one positive UInt32 axis', json);
    }
    return SurfacePointSize(w: w, h: h);
  }

  Map<String, dynamic> toJson() => {
        if (w != null) 'w': w,
        if (h != null) 'h': h,
      };

  @override
  bool operator ==(Object other) =>
      other is SurfacePointSize && other.w == w && other.h == h;

  @override
  int get hashCode => Object.hash(w, h);
}

/// Mirrors Swift `SurfaceViewportSize`: live viewport metadata supplied out
/// of band, used only to derive a missing layout axis; never snapshot state.
final class SurfaceViewportSize {
  const SurfaceViewportSize({required this.w, required this.h});

  final int w;
  final int h;

  @override
  bool operator ==(Object other) =>
      other is SurfaceViewportSize && other.w == w && other.h == h;

  @override
  int get hashCode => Object.hash(w, h);
}

/// Mirrors Swift `SurfaceBackground` with its custom kind-discriminated
/// Codable.
sealed class SurfaceBackground {
  const SurfaceBackground();

  factory SurfaceBackground.fromJson(Map<String, dynamic> json) {
    switch (json['kind'] as String) {
      case 'transparent':
        return const SurfaceBackgroundTransparent();
      case 'solid':
        final color = json['color'] as String;
        if (!RegExp(r'^#[0-9A-Fa-f]{6}([0-9A-Fa-f]{2})?$').hasMatch(color)) {
          throw FormatException(
              'Surface solid color must be #RRGGBB or #RRGGBBAA sRGBA', json);
        }
        return SurfaceBackgroundSolid(color);
      default:
        throw FormatException('Unknown SurfaceBackground kind', json);
    }
  }

  Map<String, dynamic> toJson();
}

final class SurfaceBackgroundTransparent extends SurfaceBackground {
  const SurfaceBackgroundTransparent();
  @override
  Map<String, dynamic> toJson() => {'kind': 'transparent'};
  @override
  bool operator ==(Object other) => other is SurfaceBackgroundTransparent;
  @override
  int get hashCode => 0;
}

final class SurfaceBackgroundSolid extends SurfaceBackground {
  const SurfaceBackgroundSolid(this.color);
  final String color;
  @override
  Map<String, dynamic> toJson() => {'kind': 'solid', 'color': color};
  @override
  bool operator ==(Object other) =>
      other is SurfaceBackgroundSolid && other.color == color;
  @override
  int get hashCode => color.hashCode;
}

/// Mirrors Swift `SurfaceInputPolicy`.
enum SurfaceInputPolicy {
  none,
  pointer,
  pointerAndKeyboard;

  static SurfaceInputPolicy fromJson(String v) =>
      SurfaceInputPolicy.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `SurfaceSpec`: a reference-only Surface leaf.
final class SurfaceSpec {
  const SurfaceSpec({
    required this.reference,
    this.cells,
    this.points,
    this.background = const SurfaceBackgroundTransparent(),
    this.inputPolicy = SurfaceInputPolicy.none,
  });

  final SurfaceReference reference;
  final SurfaceCellSize? cells;
  final SurfacePointSize? points;
  final SurfaceBackground background;
  final SurfaceInputPolicy inputPolicy;

  static void _validateIdentifier(String value, Map<String, dynamic> json) {
    if (!isPortableUIIdentifier(value)) {
      throw FormatException(
          'Surface reference must use portable identifiers', json);
    }
  }

  factory SurfaceSpec.fromJson(Map<String, dynamic> json) {
    final reference = SurfaceReference.fromJson(
        json['reference'] as Map<String, dynamic>);
    _validateIdentifier(reference.sessionID, json);
    _validateIdentifier(reference.streamID, json);
    return SurfaceSpec(
      reference: reference,
      cells: json['cells'] == null
          ? null
          : SurfaceCellSize.fromJson(json['cells'] as Map<String, dynamic>),
      points: json['points'] == null
          ? null
          : SurfacePointSize.fromJson(json['points'] as Map<String, dynamic>),
      background: json['background'] == null
          ? const SurfaceBackgroundTransparent()
          : SurfaceBackground.fromJson(
              json['background'] as Map<String, dynamic>),
      inputPolicy: json['inputPolicy'] == null
          ? SurfaceInputPolicy.none
          : SurfaceInputPolicy.fromJson(json['inputPolicy'] as String),
    );
  }

  Map<String, dynamic> toJson() => {
        'reference': reference.toJson(),
        if (cells != null) 'cells': cells!.toJson(),
        if (points != null) 'points': points!.toJson(),
        'background': background.toJson(),
        'inputPolicy': inputPolicy.toJson(),
      };

  /// Mirrors Swift `SurfaceSpec.resolvedPointSize(viewport:)`.
  ({int w, int h})? resolvedPointSize(SurfaceViewportSize viewport) {
    final p = points;
    if (p == null) return null;
    final pw = p.w;
    final ph = p.h;
    if (pw != null && ph != null) return (w: pw, h: ph);
    if (pw != null) return (w: pw, h: ratioCeil(pw, viewport.h, viewport.w));
    if (ph != null) return (w: ratioCeil(ph, viewport.w, viewport.h), h: ph);
    return null;
  }

  @override
  bool operator ==(Object other) =>
      other is SurfaceSpec &&
      other.reference == reference &&
      other.cells == cells &&
      other.points == points &&
      other.background == background &&
      other.inputPolicy == inputPolicy;

  @override
  int get hashCode =>
      Object.hash(reference, cells, points, background, inputPolicy);
}

// ---------------------------------------------------------------------------
// Buttons, footer actions
// ---------------------------------------------------------------------------

/// Mirrors Swift `UIButtonRole` (`standard` encodes as `"default"`).
enum UIButtonRole {
  standard('default'),
  primary('primary'),
  destructive('destructive');

  const UIButtonRole(this.wire);
  final String wire;

  static UIButtonRole fromJson(String v) =>
      UIButtonRole.values.firstWhere((e) => e.wire == v,
          orElse: () => throw FormatException('Unknown UIButtonRole $v'));
  String toJson() => wire;
}

/// Mirrors Swift `UIButtonSpec`.
final class UIButtonSpec {
  const UIButtonSpec({
    required this.id,
    required this.label,
    required this.action,
    this.role = UIButtonRole.standard,
  });

  final String id;
  final String label;
  final String action;
  final UIButtonRole role;

  factory UIButtonSpec.fromJson(Map<String, dynamic> json) => UIButtonSpec(
        id: json['id'] as String,
        label: json['label'] as String,
        action: json['action'] as String,
        role: json['role'] == null
            ? UIButtonRole.standard
            : UIButtonRole.fromJson(json['role'] as String),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'action': action,
        'role': role.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIButtonSpec &&
      other.id == id &&
      other.label == label &&
      other.action == action &&
      other.role == role;

  @override
  int get hashCode => Object.hash(id, label, action, role);
}

/// Mirrors Swift `UIFooterActionRole` (`standard` encodes as `"default"`).
enum UIFooterActionRole {
  standard('default'),
  danger('danger');

  const UIFooterActionRole(this.wire);
  final String wire;

  static UIFooterActionRole fromJson(String v) =>
      UIFooterActionRole.values.firstWhere((e) => e.wire == v,
          orElse: () =>
              throw FormatException('Unknown UIFooterActionRole $v'));
  String toJson() => wire;
}

/// Mirrors Swift `UIFooterActionSpec`.
final class UIFooterActionSpec {
  const UIFooterActionSpec({
    required this.id,
    required this.label,
    required this.action,
    this.accelerator,
    this.role = UIFooterActionRole.standard,
    this.disabled = false,
    this.busy = false,
  });

  final String id;
  final String label;
  final String action;
  final String? accelerator;
  final UIFooterActionRole role;
  final bool disabled;
  final bool busy;

  factory UIFooterActionSpec.fromJson(Map<String, dynamic> json) =>
      UIFooterActionSpec(
        id: json['id'] as String,
        label: json['label'] as String,
        action: json['action'] as String,
        accelerator: json['accelerator'] as String?,
        role: json['role'] == null
            ? UIFooterActionRole.standard
            : UIFooterActionRole.fromJson(json['role'] as String),
        disabled: json['disabled'] as bool? ?? false,
        busy: json['busy'] as bool? ?? false,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'action': action,
        if (accelerator != null) 'accelerator': accelerator,
        'role': role.toJson(),
        'disabled': disabled,
        'busy': busy,
      };

  @override
  bool operator ==(Object other) =>
      other is UIFooterActionSpec &&
      other.id == id &&
      other.label == label &&
      other.action == action &&
      other.accelerator == accelerator &&
      other.role == role &&
      other.disabled == disabled &&
      other.busy == busy;

  @override
  int get hashCode =>
      Object.hash(id, label, action, accelerator, role, disabled, busy);
}

/// Mirrors Swift `UIFooterActionsSpec` with its `isValid` rule.
final class UIFooterActionsSpec {
  const UIFooterActionsSpec({this.actions = const [], this.status});

  final List<UIFooterActionSpec> actions;
  final String? status;

  factory UIFooterActionsSpec.fromJson(Map<String, dynamic> json) =>
      UIFooterActionsSpec(
        actions: ((json['actions'] as List?) ?? [])
            .map((a) =>
                UIFooterActionSpec.fromJson(a as Map<String, dynamic>))
            .toList(),
        status: json['status'] as String?,
      );

  Map<String, dynamic> toJson() => {
        'actions': actions.map((a) => a.toJson()).toList(),
        if (status != null) 'status': status,
      };

  bool get isEmpty => actions.isEmpty && (status == null || status!.isEmpty);

  /// Mirrors Swift `UIFooterActionsSpec.isValid`.
  bool get isValid {
    if (actions.length > 100000) return false;
    final status = this.status;
    if (status != null) {
      if (_utf8Length(status) > 4 * 1024 ||
          status.contains('\n') ||
          status.contains('\r')) {
        return false;
      }
    }
    final ids = <String>{};
    final accelerators = <String>{};
    for (final action in actions) {
      if (!isPortableUIIdentifier(action.id) ||
          !isPortableUIIdentifier(action.action) ||
          _utf8Length(action.label) > 4 * 1024 ||
          action.label.contains('\n') ||
          action.label.contains('\r') ||
          !ids.add(action.id)) {
        return false;
      }
      final accelerator = action.accelerator;
      if (accelerator != null) {
        if (!isFooterAccelerator(accelerator) ||
            !accelerators.add(accelerator)) {
          return false;
        }
      }
    }
    return true;
  }

  @override
  bool operator ==(Object other) =>
      other is UIFooterActionsSpec &&
      _listEq(other.actions, actions) &&
      other.status == status;

  @override
  int get hashCode => Object.hash(actions.length, status);
}

// ---------------------------------------------------------------------------
// Canvas
// ---------------------------------------------------------------------------

/// Mirrors Swift `UICanvasSurfaceSpec`. The JSON is flat: `id` plus the
/// inlined `SurfaceSpec` fields.
final class UICanvasSurfaceSpec {
  const UICanvasSurfaceSpec({required this.id, required this.surface});

  final String id;
  final SurfaceSpec surface;

  factory UICanvasSurfaceSpec.fromJson(Map<String, dynamic> json) =>
      UICanvasSurfaceSpec(
        id: json['id'] as String,
        surface: SurfaceSpec.fromJson(json),
      );

  Map<String, dynamic> toJson() => {'id': id, ...surface.toJson()};

  @override
  bool operator ==(Object other) =>
      other is UICanvasSurfaceSpec &&
      other.id == id &&
      other.surface == surface;

  @override
  int get hashCode => Object.hash(id, surface);
}

/// Mirrors Swift `UICanvasControl` with its custom type-discriminated Codable.
sealed class UICanvasControl {
  const UICanvasControl();

  String get kind;

  factory UICanvasControl.fromJson(Map<String, dynamic> json) {
    switch (json['type'] as String) {
      case 'button':
        return UICanvasControlButton(
            UIButtonSpec.fromJson(json));
      default:
        return UICanvasControlUnsupported(json['type'] as String);
    }
  }

  Map<String, dynamic> toJson();
}

final class UICanvasControlButton extends UICanvasControl {
  const UICanvasControlButton(this.button);
  final UIButtonSpec button;
  @override
  String get kind => 'button';
  @override
  Map<String, dynamic> toJson() => {'type': 'button', ...button.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UICanvasControlButton && other.button == button;
  @override
  int get hashCode => button.hashCode;
}

final class UICanvasControlUnsupported extends UICanvasControl {
  const UICanvasControlUnsupported(this.kind);
  @override
  final String kind;
  @override
  Map<String, dynamic> toJson() => {'type': kind};
  @override
  bool operator ==(Object other) =>
      other is UICanvasControlUnsupported && other.kind == kind;
  @override
  int get hashCode => kind.hashCode;
}

/// Mirrors Swift `CanvasPageSpec`, including `requiredCapabilities`.
final class CanvasPageSpec {
  const CanvasPageSpec({
    required this.title,
    required this.surface,
    this.controls = const [],
  });

  final String title;
  final UICanvasSurfaceSpec surface;
  final List<UICanvasControl> controls;

  factory CanvasPageSpec.fromJson(Map<String, dynamic> json) =>
      CanvasPageSpec(
        title: json['title'] as String,
        surface: UICanvasSurfaceSpec.fromJson(
            json['surface'] as Map<String, dynamic>),
        controls: ((json['controls'] as List?) ?? [])
            .map((c) => UICanvasControl.fromJson(c as Map<String, dynamic>))
            .toList(),
      );

  Map<String, dynamic> toJson() => {
        'title': title,
        'surface': surface.toJson(),
        'controls': controls.map((c) => c.toJson()).toList(),
      };

  /// Mirrors Swift `CanvasPageSpec.requiredCapabilities`.
  List<String>? get requiredCapabilities {
    if (!controls.every((c) => c is UICanvasControlButton)) return null;
    final capabilities = [
      UIProtocol.canvasPageCapability,
      UIProtocol.surfaceCapability
    ];
    if (controls.isNotEmpty) capabilities.add(UIProtocol.buttonCapability);
    return capabilities;
  }

  @override
  bool operator ==(Object other) =>
      other is CanvasPageSpec &&
      other.title == title &&
      other.surface == surface &&
      _listEq(other.controls, controls);

  @override
  int get hashCode => Object.hash(title, surface, controls.length);
}

// ---------------------------------------------------------------------------
// Toggle, checkmark
// ---------------------------------------------------------------------------

/// Mirrors Swift `UIToggleRole`.
enum UIToggleRole {
  completion,
  setting;

  static UIToggleRole fromJson(String v) =>
      UIToggleRole.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `UIToggleSpec`. Encoding omits `role` when `.completion`,
/// mirroring Swift's custom `encode(to:)`.
final class UIToggleSpec {
  const UIToggleSpec({
    required this.id,
    required this.label,
    required this.value,
    required this.setValue,
    this.role = UIToggleRole.completion,
  });

  final String id;
  final String label;
  final bool value;
  final String setValue;
  final UIToggleRole role;

  factory UIToggleSpec.fromJson(Map<String, dynamic> json) => UIToggleSpec(
        id: json['id'] as String,
        label: json['label'] as String,
        value: json['value'] as bool,
        setValue: json['setValue'] as String,
        role: json['role'] == null
            ? UIToggleRole.completion
            : UIToggleRole.fromJson(json['role'] as String),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'value': value,
        'setValue': setValue,
        if (role != UIToggleRole.completion) 'role': role.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIToggleSpec &&
      other.id == id &&
      other.label == label &&
      other.value == value &&
      other.setValue == setValue &&
      other.role == role;

  @override
  int get hashCode => Object.hash(id, label, value, setValue, role);
}

/// Mirrors Swift `UICheckmarkSpec`.
final class UICheckmarkSpec {
  const UICheckmarkSpec({
    required this.id,
    required this.label,
    required this.value,
    required this.setValue,
  });

  final String id;
  final String label;
  final bool value;
  final String setValue;

  factory UICheckmarkSpec.fromJson(Map<String, dynamic> json) =>
      UICheckmarkSpec(
        id: json['id'] as String,
        label: json['label'] as String,
        value: json['value'] as bool,
        setValue: json['setValue'] as String,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'value': value,
        'setValue': setValue,
      };

  @override
  bool operator ==(Object other) =>
      other is UICheckmarkSpec &&
      other.id == id &&
      other.label == label &&
      other.value == value &&
      other.setValue == setValue;

  @override
  int get hashCode => Object.hash(id, label, value, setValue);
}
