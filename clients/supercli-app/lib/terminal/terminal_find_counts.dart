/// Terminal find-bar match-count display logic.
///
/// Port of `TerminalFindBar.updateCounts(total:selected:)` from
/// `native/SupercliNative/Sources/SupercliNative/TerminalFindBar.swift`.
/// The rest of that file is AppKit UI (NSView/NSTextField/NSStackView) and
/// is not portable to Dart unit tests; the gpuidart widget lives in
/// `lib/screens/terminalfindbar.dart`.
library;

/// Computes the match-count label for the terminal find bar.
///
/// - Empty query or unknown total → `""` (label hidden).
/// - Live query with zero matches → `"No results"`.
/// - A selected (0-based) match → `"3 of 17"` (1-based display).
/// - Otherwise → the bare total, e.g. `"17"`.
String terminalFindCountLabel({
  required String query,
  required int? total,
  required int? selected,
}) {
  if (query.isEmpty || total == null) return '';
  if (total <= 0) return 'No results';
  if (selected != null && selected >= 0) return '${selected + 1} of $total';
  return '$total';
}

/// Notification names posted for menu-driven find commands (Edit ▸ Find).
/// Port of the `Notification.Name` extension in `TerminalFindBar.swift`.
abstract final class TerminalFindNotifications {
  static const String find = 'supercli.terminal.find';
  static const String findNext = 'supercli.terminal.find-next';
  static const String findPrevious = 'supercli.terminal.find-previous';
}
