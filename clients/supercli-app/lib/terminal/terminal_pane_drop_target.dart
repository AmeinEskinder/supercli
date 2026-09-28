/// Session-drag drop-target hit testing.
///
/// Port of `SidebarSessionDragController.dropTarget(at:contentRect:panes:)`
/// from `native/SupercliNative/Sources/SupercliNative/Views/SidebarSessionDrag.swift`.
/// Pure geometry in window coordinates (y-up: [PaneEdge.up] is the maxY side).
/// The project-sidebar pin-target functions from
/// `TerminalPaneDropTargetTests.swift` belong to the sidebar worker.
library;

/// A pane edge for drop targeting.
enum PaneEdge { left, right, up, down }

/// Where a dragged Session would land: splitting a specific pane on one of
/// its four edges, or splitting the whole group's root at a content edge.
sealed class PaneDropTarget {
  const PaneDropTarget();

  @override
  bool operator ==(Object other);
  @override
  int get hashCode;
}

/// Drop onto a specific pane's edge.
final class PaneDropTargetPane extends PaneDropTarget {
  const PaneDropTargetPane(this.paneID, this.edge);
  final String paneID;
  final PaneEdge edge;

  @override
  bool operator ==(Object other) =>
      other is PaneDropTargetPane &&
      other.paneID == paneID &&
      other.edge == edge;
  @override
  int get hashCode => Object.hash(paneID, edge);
  @override
  String toString() => 'PaneDropTarget.pane($paneID, $edge)';
}

/// Drop onto a group edge (whole-root split).
final class PaneDropTargetGroupEdge extends PaneDropTarget {
  const PaneDropTargetGroupEdge(this.edge);
  final PaneEdge edge;

  @override
  bool operator ==(Object other) =>
      other is PaneDropTargetGroupEdge && other.edge == edge;
  @override
  int get hashCode => edge.hashCode;
  @override
  String toString() => 'PaneDropTarget.groupEdge($edge)';
}

/// Simple rectangle in window coordinates (y-up).
final class DropRect {
  const DropRect(this.x, this.y, this.width, this.height);
  final double x, y, width, height;

  double get minX => x;
  double get minY => y;
  double get maxX => x + width;
  double get maxY => y + height;

  bool contains(double px, double py) =>
      px >= minX && px < maxX && py >= minY && py < maxY;
}

/// A pane candidate for drop targeting.
final class DropPane {
  const DropPane({required this.paneID, required this.isSolo, required this.rect});
  final String paneID;
  final bool isSolo;
  final DropRect rect;
}

/// Width of the outer band that targets group edges.
const double groupEdgeBandWidth = 28;

/// Minimum pane height for a vertical split target.
const double minimumVerticalSplitTargetHeight = 120;

/// Resolves the drop target for a session drag at [point].
/// Returns `null` when the point is outside [contentRect] or hits no pane.
PaneDropTarget? resolveDropTarget({
  required double pointX,
  required double pointY,
  required DropRect contentRect,
  required List<DropPane> panes,
}) {
  if (!contentRect.contains(pointX, pointY)) return null;

  const band = groupEdgeBandWidth;
  if (pointX - contentRect.minX < band) {
    return const PaneDropTargetGroupEdge(PaneEdge.left);
  }
  if (contentRect.maxX - pointX < band) {
    return const PaneDropTargetGroupEdge(PaneEdge.right);
  }
  if (contentRect.maxY - pointY < band) {
    return const PaneDropTargetGroupEdge(PaneEdge.up);
  }
  if (pointY - contentRect.minY < band) {
    return const PaneDropTargetGroupEdge(PaneEdge.down);
  }

  for (final pane in panes) {
    if (!pane.rect.contains(pointX, pointY)) continue;
    final distanceLeft = pointX - pane.rect.minX;
    final distanceRight = pane.rect.maxX - pointX;
    final distanceUp = pane.rect.maxY - pointY;
    final distanceDown = pointY - pane.rect.minY;
    PaneEdge edge;
    if (distanceLeft <= distanceRight && distanceLeft <= distanceUp && distanceLeft <= distanceDown) {
      edge = PaneEdge.left;
    } else if (distanceRight <= distanceUp && distanceRight <= distanceDown) {
      edge = PaneEdge.right;
    } else if (distanceUp <= distanceDown) {
      edge = PaneEdge.up;
    } else {
      edge = PaneEdge.down;
    }
    // Short panes refuse vertical splits: two stacked headers would leave
    // no terminal.
    if ((edge == PaneEdge.up || edge == PaneEdge.down) &&
        pane.rect.height < minimumVerticalSplitTargetHeight) {
      edge = distanceLeft <= distanceRight ? PaneEdge.left : PaneEdge.right;
    }
    return pane.isSolo
        ? PaneDropTargetGroupEdge(edge)
        : PaneDropTargetPane(pane.paneID, edge);
  }
  return null;
}
