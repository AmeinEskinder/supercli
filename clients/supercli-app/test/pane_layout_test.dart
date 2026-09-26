/// Behavior tests for the split-pane layout tree.
///
/// Exercises split/close/move operations, the 8-pane limit, zoom/unzoom,
/// divider ratios/equalize, spatial focus navigation, and rendering.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/pane_layout.dart';
import 'package:test/test.dart';

PaneLayout singlePane() => PaneLayout.single(paneId: 'p1', title: 'zsh');

void main() {
  group('PaneLayout.split', () {
    test('split right creates a horizontal split with two panes', () {
      final layout = singlePane().split(
        direction: SplitDirection.horizontal,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      expect(layout.paneCount, 2);
      expect(layout.root, isA<PaneSplit>());
      final split = layout.root as PaneSplit;
      expect(split.direction, SplitDirection.horizontal);
      expect(split.leafIds, ['p1', 'p2']);
      // Focus moves to the new pane.
      expect(layout.focusedId, 'p2');
    });

    test('split down creates a vertical split', () {
      final layout = singlePane().split(
        direction: SplitDirection.vertical,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      final split = layout.root as PaneSplit;
      expect(split.direction, SplitDirection.vertical);
    });

    test('splits nest recursively', () {
      var layout = singlePane();
      layout = layout.split(
          direction: SplitDirection.horizontal,
          newPaneId: 'p2',
          newTitle: 'b')!;
      layout = layout.split(
          direction: SplitDirection.vertical,
          newPaneId: 'p3',
          newTitle: 'c')!;
      expect(layout.paneCount, 3);
      expect(layout.root.leafIds, ['p1', 'p2', 'p3']);
    });

    test('refuses to exceed 8 panes', () {
      var layout = singlePane();
      for (var i = 2; i <= maxPanes; i++) {
        layout = layout.split(
            direction: SplitDirection.horizontal,
            newPaneId: 'p$i',
            newTitle: 't$i')!;
      }
      expect(layout.paneCount, maxPanes);
      expect(
        layout.split(
            direction: SplitDirection.horizontal,
            newPaneId: 'p9',
            newTitle: 't9'),
        isNull,
      );
    });

    test('split of unknown pane returns null', () {
      final layout = singlePane();
      expect(
        layout.split(
            paneId: 'nope',
            direction: SplitDirection.horizontal,
            newPaneId: 'p2',
            newTitle: 't'),
        isNull,
      );
    });
  });

  group('PaneLayout.closePane', () {
    test('closing a pane collapses the split', () {
      var layout = singlePane().split(
        direction: SplitDirection.horizontal,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      layout = layout.closePane(paneId: 'p2')!;
      expect(layout.paneCount, 1);
      expect(layout.root, isA<PaneLeaf>());
      expect((layout.root as PaneLeaf).paneId, 'p1');
    });

    test('closing the focused pane moves focus to a survivor', () {
      var layout = singlePane().split(
        direction: SplitDirection.horizontal,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      expect(layout.focusedId, 'p2');
      layout = layout.closePane()!;
      expect(layout.paneCount, 1);
      expect(layout.focusedId, 'p1');
    });

    test('cannot close the last pane', () {
      final layout = singlePane();
      expect(layout.closePane(), isNull);
    });

    test('closing a nested pane keeps the rest of the tree', () {
      var layout = singlePane();
      layout = layout.split(
          direction: SplitDirection.horizontal,
          newPaneId: 'p2',
          newTitle: 'b')!;
      layout = layout.focus('p2').split(
          direction: SplitDirection.vertical,
          newPaneId: 'p3',
          newTitle: 'c')!;
      expect(layout.paneCount, 3);
      layout = layout.closePane(paneId: 'p3')!;
      expect(layout.paneCount, 2);
      expect(layout.root.leafIds, ['p1', 'p2']);
    });
  });

  group('PaneLayout zoom', () {
    test('zoom marks the pane; unzoom clears', () {
      var layout = singlePane().split(
        direction: SplitDirection.horizontal,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      expect(layout.isZoomed, isFalse);
      layout = layout.zoom(paneId: 'p1');
      expect(layout.isZoomed, isTrue);
      expect(layout.zoomedId, 'p1');
      layout = layout.unzoom();
      expect(layout.isZoomed, isFalse);
    });

    test('toggleZoom flips', () {
      var layout = singlePane();
      layout = layout.toggleZoom();
      expect(layout.zoomedId, 'p1');
      layout = layout.toggleZoom();
      expect(layout.zoomedId, isNull);
    });

    test('closing the zoomed pane clears zoom', () {
      var layout = singlePane().split(
        direction: SplitDirection.horizontal,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      layout = layout.zoom(paneId: 'p2');
      layout = layout.closePane(paneId: 'p2')!;
      expect(layout.isZoomed, isFalse);
    });

    test('zoomed build renders only the zoomed pane plus banner', () {
      var layout = singlePane().split(
        direction: SplitDirection.horizontal,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      layout = layout.zoom(paneId: 'p1');
      final node = layout.build() as UiColumn;
      expect(node.id, 'pane-layout-zoomed');
      // Banner row + the single zoomed pane.
      expect(node.children.length, 2);
      expect((node.children[0] as UiRow).id, 'pane-zoom-banner');
    });
  });

  group('PaneLayout ratios', () {
    test('setRatio clamps to 0.1-0.9', () {
      var layout = singlePane().split(
        direction: SplitDirection.horizontal,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      layout = layout.setRatio('p2', 0.99);
      expect((layout.root as PaneSplit).ratio, 0.9);
      layout = layout.setRatio('p2', 0.01);
      expect((layout.root as PaneSplit).ratio, 0.1);
    });

    test('equalize resets all ratios to 0.5', () {
      var layout = singlePane().split(
        direction: SplitDirection.horizontal,
        newPaneId: 'p2',
        newTitle: 'b')!;
      layout = layout.setRatio('p2', 0.8);
      layout = layout.focus('p2').split(
          direction: SplitDirection.vertical,
          newPaneId: 'p3',
          newTitle: 'c')!;
      layout = layout.setRatio('p3', 0.2);
      layout = layout.equalize();
      final root = layout.root as PaneSplit;
      expect(root.ratio, 0.5);
      final inner = root.second as PaneSplit;
      expect(inner.ratio, 0.5);
    });
  });

  group('PaneLayout focus navigation', () {
    test('focus() moves focus directly', () {
      var layout = singlePane().split(
        direction: SplitDirection.horizontal,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      layout = layout.focus('p1');
      expect(layout.focusedId, 'p1');
      // Unknown id keeps focus.
      layout = layout.focus('nope');
      expect(layout.focusedId, 'p1');
    });

    test('focusDirection moves spatially left/right', () {
      var layout = singlePane().split(
        direction: SplitDirection.horizontal,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      layout = layout.focus('p1');
      layout = layout.focusDirection(FocusDirection.right);
      expect(layout.focusedId, 'p2');
      layout = layout.focusDirection(FocusDirection.left);
      expect(layout.focusedId, 'p1');
      // No pane further left: focus stays.
      layout = layout.focusDirection(FocusDirection.left);
      expect(layout.focusedId, 'p1');
    });

    test('focusDirection moves spatially up/down in nested tree', () {
      var layout = singlePane().split(
          direction: SplitDirection.horizontal,
          newPaneId: 'p2',
          newTitle: 'b')!;
      // Split p2 vertically: p2 on top, p3 below.
      layout = layout.focus('p2').split(
          direction: SplitDirection.vertical,
          newPaneId: 'p3',
          newTitle: 'c')!;
      layout = layout.focus('p2');
      layout = layout.focusDirection(FocusDirection.down);
      expect(layout.focusedId, 'p3');
      layout = layout.focusDirection(FocusDirection.up);
      expect(layout.focusedId, 'p2');
      // p1 is left of both.
      layout = layout.focusDirection(FocusDirection.left);
      expect(layout.focusedId, 'p1');
    });

    test('focusNext cycles through panes', () {
      var layout = singlePane().split(
        direction: SplitDirection.horizontal,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      layout = layout.focus('p1');
      layout = layout.focusNext();
      expect(layout.focusedId, 'p2');
      layout = layout.focusNext();
      expect(layout.focusedId, 'p1');
      layout = layout.focusNext(reverse: true);
      expect(layout.focusedId, 'p2');
    });
  });

  group('PaneLayout rendering', () {
    test('single pane renders header with split/zoom/close buttons', () {
      final layout = singlePane();
      final node = layout.build() as UiColumn;
      expect(node.id, 'pane-layout');
      final leaf = node.children[0] as UiColumn;
      final header = leaf.children[0] as UiRow;
      final buttonLabels = header.children
          .whereType<UiButton>()
          .map((b) => b.label)
          .toList();
      expect(buttonLabels, contains('⛶')); // zoom
      expect(buttonLabels, contains('×')); // close
    });

    test('split renders divider between panes', () {
      final layout = singlePane().split(
        direction: SplitDirection.horizontal,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      final node = layout.build() as UiColumn;
      final split = node.children[0] as UiRow;
      expect(split.children.length, 3);
      expect(split.children[1], isA<UiButton>()); // divider
      expect((split.children[1] as UiButton).label, '│');
    });

    test('vertical split renders horizontal divider', () {
      final layout = singlePane().split(
        direction: SplitDirection.vertical,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      final node = layout.build() as UiColumn;
      final split = node.children[0] as UiColumn;
      expect((split.children[1] as UiButton).label, '─');
    });

    test('focused pane shows the focus marker', () {
      final layout = singlePane();
      final node = layout.build() as UiColumn;
      final leaf = node.children[0] as UiColumn;
      final header = leaf.children[0] as UiRow;
      final markers = header.children.whereType<UiText>().where(
          (t) => t.id == 'pane-focused-marker');
      expect(markers, isNotEmpty);
    });

    test('actions declare the pane key bindings', () {
      final actions = singlePane().actions();
      final names = actions.map((a) => a.name).toSet();
      expect(names, contains('pane.splitRight'));
      expect(names, contains('pane.splitDown'));
      expect(names, contains('pane.zoom'));
      expect(names, contains('pane.equalize'));
      expect(names, contains('pane.close'));
      expect(names, contains('pane.focusLeft'));
      expect(names, contains('pane.focusRight'));
      expect(names, contains('pane.focusUp'));
      expect(names, contains('pane.focusDown'));
      expect(names, contains('pane.focusNext'));
      expect(names, contains('pane.focusPrev'));
    });

    test('toJson round-trips the tree shape', () {
      final layout = singlePane().split(
        direction: SplitDirection.horizontal,
        newPaneId: 'p2',
        newTitle: 'vim',
      )!;
      final json = layout.toJson();
      expect((json['root'] as Map)['kind'], 'split');
      expect(json['focused_id'], 'p2');
    });
  });
}
