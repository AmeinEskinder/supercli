/// Behaviour tests for the drop-zone preview rects and fit-to-desktop text.
///
/// Covers `terminal_pane_drop_zones.dart`, ported from
/// `TerminalPaneView.swift` (native/SupercliNative):
///
/// - `TerminalPaneDropZonePreview`: a `.pane` target renders nothing in
///   the area overlay (zero-size); a `.groupEdge` target renders the inset
///   band at the targeted edge sized by the drop-preview geometry.
/// - `PaneFitToDesktopButton`: help text names the fitted grid (cols×rows);
///   the accessibility label is "Fit to desktop".
library;

import 'package:test/test.dart';

import 'package:supercli_app/terminal/terminal_pane_drop_zones.dart';

void main() {
  group('dropZonePreviewRect', () {
    test('pane target renders a zero-size rect', () {
      expect(
        dropZonePreviewRect(
          target: const PaneTarget('p1', DropZoneEdge.left),
          width: 1000,
          height: 800,
          existingSessionLeafCount: 1,
        ),
        DropZoneRect.zero,
      );
    });

    test('left edge band hugs the leading edge', () {
      // Area 1000x800, 1 existing leaf: extent = (1000-8)/2 = 496,
      // highlight = 496-12 = 484.
      final rect = dropZonePreviewRect(
        target: const GroupEdgeTarget(DropZoneEdge.left),
        width: 1000,
        height: 800,
        existingSessionLeafCount: 1,
      );
      expect(rect.x, 6);
      expect(rect.y, 6);
      expect(rect.width, 484);
      expect(rect.height, 800 - 12);
    });

    test('right edge band hugs the trailing edge', () {
      final rect = dropZonePreviewRect(
        target: const GroupEdgeTarget(DropZoneEdge.right),
        width: 1000,
        height: 800,
        existingSessionLeafCount: 1,
      );
      expect(rect.x, closeTo(1000 - 6 - 484, 1e-9));
      expect(rect.width, 484);
      expect(rect.height, 800 - 12);
    });

    test('up edge band spans the width minus insets', () {
      final rect = dropZonePreviewRect(
        target: const GroupEdgeTarget(DropZoneEdge.up),
        width: 1000,
        height: 800,
        existingSessionLeafCount: 1,
      );
      // Vertical extent: (800-8)/2 = 396; highlight = 396-12 = 384.
      expect(rect.x, 6);
      expect(rect.y, 6);
      expect(rect.width, 1000 - 12);
      expect(rect.height, 384);
    });

    test('down edge band hugs the bottom edge', () {
      final rect = dropZonePreviewRect(
        target: const GroupEdgeTarget(DropZoneEdge.down),
        width: 1000,
        height: 800,
        existingSessionLeafCount: 1,
      );
      expect(rect.y, closeTo(800 - 6 - 384, 1e-9));
      expect(rect.height, 384);
    });

    test('more existing leaves shrink the preview band', () {
      // 3 existing leaves: extent = (1000-8)/4 = 248, highlight = 236.
      final rect = dropZonePreviewRect(
        target: const GroupEdgeTarget(DropZoneEdge.left),
        width: 1000,
        height: 800,
        existingSessionLeafCount: 3,
      );
      expect(rect.width, 236);
    });
  });

  group('fitToDesktop', () {
    test('help text names the fitted grid', () {
      expect(
        fitToDesktopHelpText(cols: 120, rows: 40),
        'Terminal fitted to another device (120×40) — fit to desktop',
      );
    });

    test('accessibility label', () {
      expect(fitToDesktopLabel, 'Fit to desktop');
    });
  });
}
