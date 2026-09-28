/// Tests for [resolveDropTarget].
///
/// Port of the `dropTarget` cases from `TerminalPaneDropTargetTests.swift`
/// in `native/SupercliNative/Tests/SupercliNativeTests`. The project-sidebar
/// pin-target cases belong to the sidebar worker; the
/// `TerminalPaneDropPreviewGeometry` cases are covered by
/// `terminal_pane_geometry.dart`.
library;

import 'package:test/test.dart';

import '../lib/terminal/terminal_pane_drop_target.dart';

void main() {
  const content = DropRect(100, 0, 800, 600);

  PaneDropTarget? resolve(double x, double y, List<DropPane> panes) =>
      resolveDropTarget(pointX: x, pointY: y, contentRect: content, panes: panes);

  group('resolveDropTarget', () {
    test('outer bands target group edges', () {
      const pane = DropPane(paneID: 'p1', isSolo: false, rect: content);
      expect(
        resolve(110, 300, [pane]),
        const PaneDropTargetGroupEdge(PaneEdge.left),
      );
      expect(
        resolve(890, 300, [pane]),
        const PaneDropTargetGroupEdge(PaneEdge.right),
      );
      // y-up: near maxY is visually the top.
      expect(
        resolve(500, 590, [pane]),
        const PaneDropTargetGroupEdge(PaneEdge.up),
      );
      expect(
        resolve(500, 10, [pane]),
        const PaneDropTargetGroupEdge(PaneEdge.down),
      );
    });

    test('pane interior targets nearest edge', () {
      const pane = DropPane(
        paneID: 'p1',
        isSolo: false,
        rect: DropRect(200, 100, 400, 400),
      );
      expect(
        resolve(220, 300, [pane]),
        const PaneDropTargetPane('p1', PaneEdge.left),
      );
      expect(
        resolve(580, 300, [pane]),
        const PaneDropTargetPane('p1', PaneEdge.right),
      );
      expect(
        resolve(400, 480, [pane]),
        const PaneDropTargetPane('p1', PaneEdge.up),
      );
      expect(
        resolve(400, 120, [pane]),
        const PaneDropTargetPane('p1', PaneEdge.down),
      );
    });

    test('short panes refuse vertical splits', () {
      const short = DropPane(
        paneID: 'p1',
        isSolo: false,
        rect: DropRect(200, 250, 400, 100),
      );
      // Cursor near the visual top of a 100pt-tall pane still resolves to a
      // horizontal half: two stacked headers would leave no terminal.
      expect(
        resolve(250, 340, [short]),
        const PaneDropTargetPane('p1', PaneEdge.left),
      );
      expect(
        resolve(550, 340, [short]),
        const PaneDropTargetPane('p1', PaneEdge.right),
      );
    });

    test('solo pane resolves to group edges', () {
      const solo = DropPane(
        paneID: 'solo:s1',
        isSolo: true,
        rect: DropRect(200, 100, 400, 400),
      );
      expect(
        resolve(400, 120, [solo]),
        const PaneDropTargetGroupEdge(PaneEdge.down),
      );
      expect(
        resolve(220, 300, [solo]),
        const PaneDropTargetGroupEdge(PaneEdge.left),
      );
    });

    test('outside content rect is null', () {
      expect(resolve(50, 300, []), isNull);
      const small = DropPane(
        paneID: 'p1',
        isSolo: false,
        rect: DropRect(200, 100, 100, 100),
      );
      expect(resolve(500, 300, [small]), isNull);
    });
  });
}
