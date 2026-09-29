/// Split-tree layout geometry for the terminal pane container.
///
/// Port of the portable layout rules from `TerminalPaneView.swift`
/// (native/SupercliNative, `TerminalPaneContainer`):
///
/// - `splitView`: the divider gap consumes [dividerWidth] (8pt,
///   `TerminalPaneDropPreviewGeometry.dividerWidth`); the remaining extent
///   splits by the ratio — first child `extent * ratio`, second child
///   `extent - first` — along the split direction (horizontal = width,
///   vertical = height). Trees are at most eight leaves deep.
/// - `pathKey`: a divider's identity is its split path's components joined
///   with "," — the live drag ratio stays keyed on this while dragging.
/// - `existingSessionLeafCount`: mirrors
///   `PaneLayoutState.insertSession(atGroupEdge:)`. A solo terminal counts
///   as one existing leaf; an established group uses its durable
///   Session-leaf count (launchers do not affect the insert ratio).
/// - Launcher/unavailable pane titles: a launcher awaiting a session shows
///   "New pane" (or "Starting…" while its launch is pending); a missing
///   session entry shows "Session unavailable" (a defensive render-time
///   fallback that retains the slot to avoid a geometry jump).
/// - Zoom: when a zoomed pane belongs to the group and is still present,
///   only the zoomed leaf renders; hidden siblings keep their retained
///   surfaces.
///
/// Divider width constant lives in `terminal_pane_geometry.dart`
/// (`TerminalPaneDropPreviewGeometry.dividerWidth`); the divider ratio
/// drag math lives in `terminal_pane_divider.dart`.
library;

/// The two child extents of a split for one drag frame.
final class SplitPaneExtents {
  const SplitPaneExtents({required this.first, required this.second});

  /// Extent of the left/top child.
  final double first;

  /// Extent of the right/bottom child.
  final double second;

  @override
  bool operator ==(Object other) =>
      other is SplitPaneExtents &&
      other.first == first &&
      other.second == second;

  @override
  int get hashCode => Object.hash(first, second);

  @override
  String toString() => 'SplitPaneExtents(first: $first, second: $second)';
}

/// Portable split-tree layout rules for terminal panes.
abstract final class TerminalPaneSplitLayout {
  const TerminalPaneSplitLayout._();

  /// Child extents of a split: the divider gap consumes [dividerWidth];
  /// the remainder splits by [ratio].
  ///
  /// Mirrors Swift `splitView`: `extent = max(0, total - dividerWidth)`,
  /// `first = extent * ratio`, `second = extent - first`.
  static SplitPaneExtents splitPaneExtents({
    required double totalExtent,
    required double dividerWidth,
    required double ratio,
  }) {
    final extent = (totalExtent - dividerWidth).clamp(0.0, double.infinity);
    final first = extent * ratio;
    return SplitPaneExtents(first: first, second: extent - first);
  }

  /// A divider's identity: its split path's components joined with ",".
  ///
  /// Mirrors Swift `TerminalPaneContainer.pathKey`: the live drag ratio in
  /// `dividerDrag` is keyed on this so a drag never changes SwiftUI
  /// structural identity.
  static String splitPathKey(List<String> components) =>
      components.join(',');

  /// Session-leaf count feeding the group-edge drop preview.
  ///
  /// Mirrors Swift `TerminalPaneDropZonesOverlay.existingSessionLeafCount`:
  /// with no selected session or group a solo terminal counts as one;
  /// otherwise the group's durable Session-leaf count, at least one
  /// (launchers do not affect the insert ratio).
  static int existingSessionLeafCount({
    required bool hasSelectedSessionGroup,
    required int groupSessionLeafCount,
  }) {
    if (!hasSelectedSessionGroup) return 1;
    return groupSessionLeafCount < 1 ? 1 : groupSessionLeafCount;
  }

  /// Title of a launcher pane: "Starting…" while its launch is pending,
  /// otherwise "New pane".
  static String launcherPaneTitle({required bool starting}) =>
      starting ? 'Starting…' : 'New pane';

  /// Title of the defensive fallback pane shown when a session entry is
  /// missing at render time. Never appears in the durable layout; it just
  /// retains the slot so the tree does not jump mid-render.
  static const String unavailablePaneTitle = 'Session unavailable';
}
