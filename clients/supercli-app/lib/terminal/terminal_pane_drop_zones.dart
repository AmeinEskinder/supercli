/// Drop-zone preview rects and the fit-to-desktop button text.
///
/// Port of the rendering-independent parts of `TerminalPaneView.swift`
/// (native/SupercliNative):
///
/// - `TerminalPaneDropZonePreview`: the area overlay draws group-edge
///   previews only. Pane-edge highlights render inside each leaf, so a
///   `.pane` target produces a zero-size (invisible) rect here. A group
///   edge preview occupies the footprint the arriving terminal will
///   receive after the split: the group-edge pane extent (from
///   [TerminalPaneDropPreviewGeometry]) inset by the preview padding,
///   hugging the targeted edge. The hit band stays deliberately narrow,
///   but the preview shows the full post-split footprint.
/// - `PaneFitToDesktopButton`: the recovery button shown when the shared
///   grid is fitted to another device; its help text names the fitted
///   grid (cols × rows).
///
/// Spring/magnet animations and the AppKit overlay plumbing are platform
/// rendering, not ported.
library;

import 'terminal_pane_geometry.dart';

/// One of the four content edges a group-edge drop can target.
enum DropZoneEdge { left, right, up, down }

/// Where a dragged session hovers: one of the four edges of a specific
/// pane (pane-edge split, highlighted inside the leaf), or a group edge
/// (group-edge split, highlighted by the area overlay).
sealed class PaneDropTarget {
  const PaneDropTarget();
}

/// Hovering one of a pane's four edges (rendered inside the leaf).
final class PaneTarget extends PaneDropTarget {
  const PaneTarget(this.paneId, this.edge);

  final String paneId;
  final DropZoneEdge edge;

  @override
  bool operator ==(Object other) =>
      other is PaneTarget && other.paneId == paneId && other.edge == edge;

  @override
  int get hashCode => Object.hash(paneId, edge);
}

/// Hovering a group edge (rendered by the area overlay).
final class GroupEdgeTarget extends PaneDropTarget {
  const GroupEdgeTarget(this.edge);

  final DropZoneEdge edge;

  @override
  bool operator ==(Object other) =>
      other is GroupEdgeTarget && other.edge == edge;

  @override
  int get hashCode => edge.hashCode;
}

/// An axis-aligned rect: the drop-preview footprint in area coordinates.
final class DropZoneRect {
  const DropZoneRect({
    required this.x,
    required this.y,
    required this.width,
    required this.height,
  });

  /// The zero-size rect a `.pane` target renders (invisible; the leaf
  /// itself draws the half-pane highlight).
  static const DropZoneRect zero =
      DropZoneRect(x: 0, y: 0, width: 0, height: 0);

  final double x;
  final double y;
  final double width;
  final double height;

  @override
  bool operator ==(Object other) =>
      other is DropZoneRect &&
      other.x == x &&
      other.y == y &&
      other.width == width &&
      other.height == height;

  @override
  int get hashCode => Object.hash(x, y, width, height);

  @override
  String toString() => 'DropZoneRect($x, $y, $width, $height)';
}

/// The area-overlay preview rect for a drop target.
///
/// Mirrors Swift `TerminalPaneDropZonePreview`:
/// - `.pane` → zero-size (the highlight renders inside the leaf).
/// - `.groupEdge` → the inset band at the targeted edge: the group-edge
///   pane extent for this area size, inset by `previewInset` on all sides,
///   flush to the edge.
DropZoneRect dropZonePreviewRect({
  required PaneDropTarget target,
  required double width,
  required double height,
  required int existingSessionLeafCount,
}) {
  if (target is! GroupEdgeTarget) return DropZoneRect.zero;

  final edge = target.edge;
  final horizontal = edge == DropZoneEdge.left || edge == DropZoneEdge.right;
  final totalExtent = horizontal ? width : height;
  final paneExtent = TerminalPaneDropPreviewGeometry.groupEdgePaneExtent(
    totalExtent: totalExtent,
    existingSessionLeafCount: existingSessionLeafCount,
  );
  final highlightExtent =
      TerminalPaneDropPreviewGeometry.insetHighlightExtent(paneExtent);
  const inset = TerminalPaneDropPreviewGeometry.previewInset;

  switch (edge) {
    case DropZoneEdge.left:
      return DropZoneRect(
        x: inset,
        y: inset,
        width: highlightExtent,
        height: (height - inset * 2).clamp(0.0, double.infinity),
      );
    case DropZoneEdge.right:
      return DropZoneRect(
        x: width - inset - highlightExtent,
        y: inset,
        width: highlightExtent,
        height: (height - inset * 2).clamp(0.0, double.infinity),
      );
    case DropZoneEdge.up:
      return DropZoneRect(
        x: inset,
        y: inset,
        width: (width - inset * 2).clamp(0.0, double.infinity),
        height: highlightExtent,
      );
    case DropZoneEdge.down:
      return DropZoneRect(
        x: inset,
        y: height - inset - highlightExtent,
        width: (width - inset * 2).clamp(0.0, double.infinity),
        height: highlightExtent,
      );
  }
}

/// Help text for the fit-to-desktop button.
///
/// Mirrors Swift `PaneFitToDesktopButton`: the help names the fitted grid
/// the shared terminal is currently sized to.
String fitToDesktopHelpText({required int cols, required int rows}) =>
    'Terminal fitted to another device ($cols×$rows) — fit to desktop';

/// Accessibility label for the fit-to-desktop button.
const String fitToDesktopLabel = 'Fit to desktop';
