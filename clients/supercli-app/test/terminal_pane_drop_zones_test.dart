/// Behaviour tests for the terminal pane drop-zone preview port.
///
/// Covers the portable logic from `TerminalPaneView.swift`
/// (`TerminalPaneDropZonePreview`, `PaneFitToDesktopButton`) that does not
/// depend on SwiftUI:
///
/// - `terminal_pane_drop_zones.dart` — preview rect computation for pane
///   vs group-edge targets, fit-to-desktop help text.
library;

import 'package:test/test.dart';

import 'package:supercli_app/terminal/terminal_pane_drop_zones.dart';

void main() {
  group('isGroupEdgeTarget', () {
    test('pane target is not a group edge', () {
      expect(isGroupEdgeTarget(PaneDropTarget.pane), isFalse);
    });

    test('edge targets are group edges', () {
      expect(isGroupEdgeTarget(PaneDropTarget.groupEdgeLeft), isTrue);
      expect(isGroupEdgeTarget(PaneDropTarget.groupEdgeRight), isTrue);
      expect(isGroupEdgeTarget(PaneDropTarget.groupEdgeUp), isTrue);
      expect(isGroupEdgeTarget(PaneDropTarget.groupEdgeDown), isTrue);
    });
  });

  group('dropZonePreviewRect', () {
    test('pane target yields a zero-size rect', () {
      final r = dropZonePreviewRect(
        target: PaneDropTarget.pane,
        width: 800,
        height: 600,
        existingSessionLeafCount: 1,
      );
      expect((r.w, r.h), (0.0, 0.0));
    });

    test('left edge: preview sits at the left inset', () {
      final r = dropZonePreviewRect(
        target: PaneDropTarget.groupEdgeLeft,
        width: 800,
        height: 600,
        existingSessionLeafCount: 1,
      );
      // 1/(1+1) of (800-8) = 396, minus 2*6 inset = 384 wide.
      expect(r.w, closeTo(384.0, 1e-9));
      expect(r.x, 6.0);
      expect(r.y, 6.0);
      expect(r.h, closeTo(588.0, 1e-9));
    });

    test('right edge: preview sits at the right inset', () {
      final r = dropZonePreviewRect(
        target: PaneDropTarget.groupEdgeRight,
        width: 800,
        height: 600,
        existingSessionLeafCount: 1,
      );
      expect(r.w, closeTo(384.0, 1e-9));
      expect(r.x, closeTo(800 - 6 - 384, 1e-9));
    });

    test('down edge: preview spans the width at the bottom', () {
      final r = dropZonePreviewRect(
        target: PaneDropTarget.groupEdgeDown,
        width: 800,
        height: 600,
        existingSessionLeafCount: 1,
      );
      // 1/(1+1) of (600-8) = 296, minus 12 = 284 tall.
      expect(r.h, closeTo(284.0, 1e-9));
      expect(r.w, closeTo(788.0, 1e-9));
      expect(r.y, closeTo(600 - 6 - 284, 1e-9));
    });

    test('more existing leaves shrink the arriving share', () {
      final one = dropZonePreviewRect(
        target: PaneDropTarget.groupEdgeLeft,
        width: 800,
        height: 600,
        existingSessionLeafCount: 1,
      );
      final three = dropZonePreviewRect(
        target: PaneDropTarget.groupEdgeLeft,
        width: 800,
        height: 600,
        existingSessionLeafCount: 3,
      );
      expect(three.w, lessThan(one.w));
    });
  });

  group('PaneFitToDesktopButton', () {
    test('help text names the fitted grid size', () {
      const b = PaneFitToDesktopButton(gridCols: 120, gridRows: 40);
      expect(
        b.helpText,
        'Terminal fitted to another device (120×40) — fit to desktop',
      );
    });
  });
}
