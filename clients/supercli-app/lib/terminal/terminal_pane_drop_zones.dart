/// Terminal pane drop-zone preview.
///
/// Port of `TerminalPaneDropZonePreview` from `TerminalPaneView.swift`
/// (native/SupercliNative). Computes the preview footprint for a drag
/// drop target: pane-edge highlights render inside each leaf (zero-size
/// here); group-edge previews occupy the footprint the arriving terminal
/// will receive after the split, using [TerminalPaneDropPreviewGeometry].
library;

import 'terminal_pane_geometry.dart';

/// A drop target for a terminal pane drag.
enum PaneDropTarget {
  /// Drop onto a specific pane (highlight renders in the leaf; the area
  /// overlay draws nothing).
  pane,

  /// Drop at a group edge; the arriving session gets a new split pane.
  groupEdgeLeft,
  groupEdgeRight,
  groupEdgeUp,
  groupEdgeDown,
}

/// Whether the target is a group-edge insert (vs a pane highlight).
bool isGroupEdgeTarget(PaneDropTarget target) =>
    target != PaneDropTarget.pane;

/// Preview rectangle for a drop target within an area of [width]×[height].
///
/// Returns (x, y, w, h) in area coordinates. For [.pane] the preview is
/// zero-size (the leaf draws its own highlight). For group edges, the
/// footprint is the band the arriving terminal will occupy after the
/// split, inset by [TerminalPaneDropPreviewGeometry.previewInset].
({double x, double y, double w, double h}) dropZonePreviewRect({
  required PaneDropTarget target,
  required double width,
  required double height,
  required int existingSessionLeafCount,
}) {
  const inset = TerminalPaneDropPreviewGeometry.previewInset;
  switch (target) {
    case PaneDropTarget.pane:
      return (x: 0, y: 0, w: 0, h: 0);
    case PaneDropTarget.groupEdgeLeft:
    case PaneDropTarget.groupEdgeRight:
      final paneExtent =
          TerminalPaneDropPreviewGeometry.groupEdgePaneExtent(
            totalExtent: width,
            existingSessionLeafCount: existingSessionLeafCount,
          );
      final highlight =
          TerminalPaneDropPreviewGeometry.insetHighlightExtent(paneExtent);
      final h = height - inset * 2 <= 0 ? 0.0 : height - inset * 2;
      final x = target == PaneDropTarget.groupEdgeLeft
          ? inset
          : width - inset - highlight;
      return (x: x, y: inset, w: highlight, h: h);
    case PaneDropTarget.groupEdgeUp:
    case PaneDropTarget.groupEdgeDown:
      final paneExtent =
          TerminalPaneDropPreviewGeometry.groupEdgePaneExtent(
            totalExtent: height,
            existingSessionLeafCount: existingSessionLeafCount,
          );
      final highlight =
          TerminalPaneDropPreviewGeometry.insetHighlightExtent(paneExtent);
      final w = width - inset * 2 <= 0 ? 0.0 : width - inset * 2;
      final y = target == PaneDropTarget.groupEdgeUp
          ? inset
          : height - inset - highlight;
      return (x: inset, y: y, w: w, h: highlight);
  }
}

/// Fit-to-desktop button (TerminalPaneView.swift: PaneFitToDesktopButton).
///
/// Shown when the terminal grid was fitted to another device
/// ([gridCols]×[gridRows]); tapping reverts to the desktop size.
final class PaneFitToDesktopButton {
  const PaneFitToDesktopButton({
    required this.gridCols,
    required this.gridRows,
    this.onRevert,
  });

  final int gridCols;
  final int gridRows;
  final void Function()? onRevert;

  /// Accessibility label / tooltip text.
  String get helpText =>
      'Terminal fitted to another device (${gridCols}×$gridRows) — fit to desktop';

  /// Button glyph (expand arrows).
  static const String glyph = '⤢';
}
