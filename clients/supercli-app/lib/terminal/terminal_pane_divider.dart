/// Divider drag math for terminal pane splits.
///
/// Ports the divider drag gesture from `TerminalPaneView.swift`
/// (TerminalPaneContainer, ~2,224 lines):
///
/// ```swift
/// .onChanged { value in
///     guard group != nil, extent > 0 else { return }
///     let start = dividerDrag?.pathKey == key
///         ? dividerDrag!.startRatio
///         : CGFloat(split.ratio)
///     let translation = horizontal
///         ? value.translation.width
///         : value.translation.height
///     let requested = start + translation / extent
///     let applied = min(
///         max(requested, PaneLayoutState.minimumSplitRatio),
///         PaneLayoutState.maximumSplitRatio
///     )
///     dividerDrag = DividerDrag(pathKey: key, startRatio: start, ratio: applied)
/// }
/// .onEnded { _ in
///     if let drag = dividerDrag, drag.pathKey == key, let group {
///         store.resizePaneSplit(groupID: group.id, path: path, ratio: Double(drag.ratio))
///     }
///     dividerDrag = nil
/// }
/// ```
///
/// The live ratio stays local during the drag (never commits to the model
/// mid-gesture, so structural identity stays stable); the model commits once
/// on release. See the Ghostty #7546 comment in the Swift source.
library;

/// Minimum and maximum split ratios, matching
/// `PaneLayoutState.minimumSplitRatio` / `maximumSplitRatio` in the Swift
/// source (PaneLayoutState.swift:446-447).
const double minimumSplitRatio = 0.1;
const double maximumSplitRatio = 0.9;

/// One in-progress divider drag.
///
/// The live [ratio] is local view state; the model commits once on release.
class DividerDrag {
  const DividerDrag({
    required this.pathKey,
    required this.startRatio,
    required this.ratio,
  });

  /// Path key of the split divider being dragged.
  final String pathKey;

  /// Ratio at drag start (or the live ratio if continuing the same drag).
  final double startRatio;

  /// Current live ratio, clamped to [minimumSplitRatio, maximumSplitRatio].
  final double ratio;

  @override
  bool operator ==(Object other) =>
      other is DividerDrag &&
      other.pathKey == pathKey &&
      other.startRatio == startRatio &&
      other.ratio == ratio;

  @override
  int get hashCode => Object.hash(pathKey, startRatio, ratio);
}

/// Computes the live divider ratio for a drag update.
///
/// - [startRatio]: ratio at drag start (or live ratio if continuing).
/// - [translation]: pointer translation in points along the split axis
///   (width for horizontal splits, height for vertical).
/// - [extent]: total extent of the split container along the drag axis.
///   Must be > 0; if not, the drag is ignored (returns [startRatio]).
///
/// Returns the requested ratio clamped to
/// [minimumSplitRatio]..[maximumSplitRatio], matching the Swift
/// `min(max(requested, minimum), maximum)`.
double dividerDragRatio({
  required double startRatio,
  required double translation,
  required double extent,
}) {
  if (extent <= 0) return startRatio;
  final requested = startRatio + translation / extent;
  return requested.clamp(minimumSplitRatio, maximumSplitRatio);
}

/// Whether a drag update should be ignored (no group, or zero extent).
///
/// Matches the Swift `guard group != nil, extent > 0 else { return }`.
bool shouldIgnoreDividerDrag({required bool hasGroup, required double extent}) {
  return !hasGroup || extent <= 0;
}
