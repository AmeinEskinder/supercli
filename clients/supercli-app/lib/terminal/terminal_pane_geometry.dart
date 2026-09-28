/// Drop-preview geometry for terminal pane drag & drop.
///
/// Port of `TerminalPaneDropPreviewGeometry` from
/// `TerminalPaneView.swift` (native/SupercliNative). Pixel geometry shared
/// by the split renderer and its drop preview: a group-edge insert gives
/// the arriving session `1/(existing + 1)` of the root, after reserving the
/// same divider gap the final layout will use.
///
/// Behaviours ported:
/// - `groupEdgePaneExtent(totalExtent:existingSessionLeafCount:)` →
///   [groupEdgePaneExtent]
/// - `insetHighlightExtent(for:)` → [insetHighlightExtent]
/// - `dividerWidth` / `previewInset` constants
library;

/// Pixel geometry shared by the split renderer and its drop preview.
abstract final class TerminalPaneDropPreviewGeometry {
  /// Width of the divider strip between panes (also the resize handle).
  static const double dividerWidth = 8;

  /// Inset of the drop highlight inside the preview band.
  static const double previewInset = 6;

  /// Extent the arriving session gets on a group-edge insert:
  /// `1/(existing + 1)` of the root after reserving one divider gap.
  ///
  /// Mirrors `TerminalPaneDropPreviewGeometry.groupEdgePaneExtent`.
  static double groupEdgePaneExtent({
    required double totalExtent,
    required int existingSessionLeafCount,
  }) {
    final available = totalExtent - dividerWidth <= 0
        ? 0.0
        : totalExtent - dividerWidth;
    final existing =
        existingSessionLeafCount < 1 ? 1 : existingSessionLeafCount;
    return available / (existing + 1);
  }

  /// Highlight extent inside a preview band: the band minus the inset on
  /// both sides, clamped at zero.
  ///
  /// Mirrors `TerminalPaneDropPreviewGeometry.insetHighlightExtent`.
  static double insetHighlightExtent(double paneExtent) {
    final v = paneExtent - previewInset * 2;
    return v <= 0 ? 0.0 : v;
  }
}
