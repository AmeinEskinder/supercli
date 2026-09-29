/// Rename-editing logic for the pane title chip.
///
/// Port of the portable state logic from `TerminalPaneView.swift`
/// (native/SupercliNative, `TerminalPaneTitleChip`):
///
/// - The title chip is the pane's drag handle; double-click begins an
///   inline rename. While editing, a drag gesture must not start
///   (`beginPaneTitleDrag` is guarded by `!isEditing`).
/// - The rename draft is prefilled with the session label on appear and
///   the field grabs focus asynchronously.
/// - Commit (`commitRename`): trims whitespace/newlines; commits only when
///   the trimmed value is non-empty AND differs from the session label;
///   always ends editing afterwards.
/// - Cancel (Esc via `onExitCommand`): ends editing without committing.
/// - Losing focus while editing commits (same as submit).
/// - Tooltip: "Edit session title" while editing, otherwise
///   "Double-click to rename; drag to move".
library;

/// The outcome of ending a rename edit: either commit a new title, or just
/// end editing (cancel, empty draft, or unchanged draft).
sealed class RenameEnd {
  const RenameEnd();
}

/// End editing without committing.
final class RenameJustEnd extends RenameEnd {
  const RenameJustEnd();

  @override
  bool operator ==(Object other) => other is RenameJustEnd;

  @override
  int get hashCode => 0;

  @override
  String toString() => 'RenameEnd.justEnd';
}

/// Commit [value] as the new session title, then end editing.
final class RenameCommit extends RenameEnd {
  const RenameCommit(this.value);

  final String value;

  @override
  bool operator ==(Object other) =>
      other is RenameCommit && other.value == value;

  @override
  int get hashCode => value.hashCode;

  @override
  String toString() => 'RenameEnd.commit($value)';
}

/// Pure rename-editing rules for a pane title chip.
abstract final class TerminalPaneRename {
  const TerminalPaneRename._();

  /// Tooltip for the title chip.
  ///
  /// Mirrors Swift: `isEditing ? "Edit session title"
  /// : "Double-click to rename; drag to move"`.
  static String tooltip({required bool isEditing}) =>
      isEditing ? 'Edit session title' : 'Double-click to rename; drag to move';

  /// Resolve what happens when the edit ends (submit, Esc, or focus loss).
  ///
  /// Mirrors Swift's `commitRename`/`cancelRename`:
  /// - `cancelled` (Esc) → [RenameJustEnd].
  /// - otherwise the draft is trimmed; a non-empty value that differs from
  ///   [currentLabel] commits ([RenameCommit]), else just ends.
  static RenameEnd endEdit({
    required String draft,
    required String currentLabel,
    required bool cancelled,
  }) {
    if (cancelled) return const RenameJustEnd();
    final trimmed = draft.trim();
    if (trimmed.isNotEmpty && trimmed != currentLabel) {
      return RenameCommit(trimmed);
    }
    return const RenameJustEnd();
  }

  /// Whether a pane-title drag may start: never while editing.
  ///
  /// Mirrors the Swift guard `guard !isEditing else { return }` in both the
  /// drag gesture's `onChanged` and `onEnded`.
  static bool dragAllowed({required bool isEditing}) => !isEditing;
}
