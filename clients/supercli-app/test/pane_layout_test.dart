/// Pane layout tests: exercise the REAL Rust pane-layout state machine
/// through the FFI snapshot protocol.
///
/// Requires the `SUPERCLI_FFI_LIB` environment variable pointing at the
/// built library (`cargo build -p supercli-client-ffi --release`). The suite
/// is skipped with a clear message when the variable is absent, following
/// the `ffi_smoke_test.dart` convention. There is no fake Dart model to
/// test against: all layout semantics live in Rust.
library;

import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';

import 'package:supercli_app/pane_layout.dart';

Map<String, dynamic> _rootOf(PaneLayout layout) {
  final decoded = jsonDecode(layout.snapshot) as Map<String, dynamic>;
  final groups = decoded['groups'] as List;
  final group = groups
      .cast<Map<String, dynamic>>()
      .firstWhere((g) => g['id'] == layout.groupId);
  return group['root'] as Map<String, dynamic>;
}

/// Session ids of the leaves in tree order.
List<String> _sessionIds(Map<String, dynamic> node) {
  if (node['kind'] == 'leaf') {
    final content = node['content'] as Map<String, dynamic>;
    return [content['id'] as String];
  }
  return [
    for (final child in [node['left'], node['right']])
      if (child is Map<String, dynamic>) ..._sessionIds(child),
  ];
}

void main() {
  final ffiLib = Platform.environment['SUPERCLI_FFI_LIB'];

  setUpAll(() {
    if (ffiLib == null || ffiLib.isEmpty) {
      markTestSkipped('SUPERCLI_FFI_LIB not set; build the cdylib first');
    }
  });

  group('pane layout (Rust FFI)', () {
    test('single creates a one-pane layout', () {
      final layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      expect(layout.paneCount, 1);
      // Dart-visible ids are logical (session) ids, not Rust internals.
      expect(layout.leafIds, ['sess-1']);
      expect(layout.focusedId, 'sess-1');
      expect(layout.titles[layout.focusedId], 'one');
      expect(layout.groupId, isNotEmpty);
    });

    test('caller-supplied pane ids are preserved verbatim', () {
      // 'my-pane' is not UUID-shaped, so Rust canonicalizes the internal
      // pane id; the Dart-visible id must still be the requested one.
      var layout = PaneLayout.single(paneId: 'my-pane', title: 'one');
      expect(layout.leafIds, ['my-pane']);
      expect(layout.focusedId, 'my-pane');
      final internalId = _rootOf(layout)['id'] as String;
      expect(internalId, isNot('my-pane'));

      layout = layout.split(
        direction: SplitDirection.horizontal,
        newPaneId: 'other-pane',
        newTitle: 'two',
      )!;
      expect(layout.leafIds, ['my-pane', 'other-pane']);
      expect(layout.focusedId, 'other-pane');
      // Mutations target by logical id: closing by logical id works even
      // though the internal id differs.
      layout = layout.closePane(paneId: 'other-pane')!;
      expect(layout.leafIds, ['my-pane']);
      expect(layout.focusedId, 'my-pane');
    });

    test('split adds a pane to the right by default', () {
      var layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      final split = layout.split(
        direction: SplitDirection.horizontal,
        newPaneId: 'sess-2',
        newTitle: 'two',
      );
      expect(split, isNotNull);
      layout = split!;
      expect(layout.paneCount, 2);
      // The new pane takes focus.
      expect(layout.focusedId, isNot(layout.leafIds.first));
      expect(layout.titles[layout.focusedId], 'two');
      // Horizontal split: the root splits left/right.
      final root = _rootOf(layout);
      expect(root['kind'], 'split');
      expect(root['direction'], 'horizontal');
      expect(_sessionIds(root), ['sess-1', 'sess-2']);
    });

    test('split down stacks vertically', () {
      var layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      layout = layout.split(
        direction: SplitDirection.vertical,
        newPaneId: 'sess-2',
        newTitle: 'two',
      )!;
      final root = _rootOf(layout);
      expect(root['direction'], 'vertical');
      expect(_sessionIds(root), ['sess-1', 'sess-2']);
    });

    test('split returns null at the 8-pane limit', () {
      var layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      for (var i = 2; i <= maxPanes; i++) {
        final next = layout.split(
          direction: SplitDirection.horizontal,
          newPaneId: 'sess-$i',
          newTitle: 'pane $i',
        );
        expect(next, isNotNull, reason: 'split $i should succeed');
        layout = next!;
      }
      expect(layout.paneCount, maxPanes);
      expect(
        layout.split(
          direction: SplitDirection.horizontal,
          newPaneId: 'sess-9',
          newTitle: 'pane 9',
        ),
        isNull,
      );
    });

    test('closePane removes the pane and moves focus', () {
      var layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      layout = layout.split(
        direction: SplitDirection.horizontal,
        newPaneId: 'sess-2',
        newTitle: 'two',
      )!;
      final first = layout.leafIds.first;
      layout = layout.closePane()!;
      expect(layout.paneCount, 1);
      expect(layout.leafIds, [first]);
      expect(layout.focusedId, first);
    });

    test('closePane on the last pane returns null', () {
      final layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      expect(layout.closePane(), isNull);
      expect(layout.paneCount, 1);
    });

    test('setRatio sets and clamps the divider ratio', () {
      var layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      layout = layout.split(
        direction: SplitDirection.horizontal,
        newPaneId: 'sess-2',
        newTitle: 'two',
      )!;
      final target = layout.leafIds.first;
      layout = layout.setRatio(target, 0.25);
      expect((_rootOf(layout)['ratio'] as num).toDouble(), 0.25);
      layout = layout.setRatio(target, 5.0);
      expect((_rootOf(layout)['ratio'] as num).toDouble(), 0.9);
      layout = layout.setRatio(target, -1.0);
      expect((_rootOf(layout)['ratio'] as num).toDouble(), 0.1);
    });

    test('equalize resets every divider to 0.5', () {
      var layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      layout = layout.split(
        direction: SplitDirection.horizontal,
        newPaneId: 'sess-2',
        newTitle: 'two',
      )!;
      layout = layout.setRatio(layout.leafIds.first, 0.7);
      expect((_rootOf(layout)['ratio'] as num).toDouble(), 0.7);
      layout = layout.equalize();
      expect((_rootOf(layout)['ratio'] as num).toDouble(), 0.5);
    });

    test('swap exchanges pane positions', () {
      var layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      layout = layout.split(
        direction: SplitDirection.horizontal,
        newPaneId: 'sess-2',
        newTitle: 'two',
      )!;
      final before = layout.leafIds;
      expect(before.length, 2);
      layout = layout.swap(before[0], before[1]);
      expect(layout.leafIds, [before[1], before[0]]);
      // Titles travel with their panes.
      expect(layout.titles[layout.leafIds[0]], 'two');
      expect(layout.titles[layout.leafIds[1]], 'one');
    });

    test('focusDirection moves focus spatially', () {
      var layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      layout = layout.split(
        direction: SplitDirection.horizontal,
        newPaneId: 'sess-2',
        newTitle: 'two',
      )!;
      final left = layout.leafIds.first;
      final right = layout.leafIds.last;
      layout = layout.focus(left);
      layout = layout.focusDirection(FocusDirection.right);
      expect(layout.focusedId, right);
      // No neighbor further right: unchanged.
      expect(
        layout.focusDirection(FocusDirection.right).focusedId,
        right,
      );
      layout = layout.focusDirection(FocusDirection.left);
      expect(layout.focusedId, left);
    });

    test('reconcile drops ineligible sessions', () {
      var layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      layout = layout.split(
        direction: SplitDirection.horizontal,
        newPaneId: 'sess-2',
        newTitle: 'two',
      )!;
      layout = layout.reconcile(['sess-2']);
      expect(layout.paneCount, 1);
      expect(_sessionIds(_rootOf(layout)), ['sess-2']);
      expect(layout.titles.keys, [layout.leafIds.single]);
    });

    test('leafBoxes returns per-pane geometry', () {
      var layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      layout = layout.split(
        direction: SplitDirection.horizontal,
        newPaneId: 'sess-2',
        newTitle: 'two',
      )!;
      final boxes = layout.leafBoxes(width: 100, height: 50);
      expect(boxes.length, 2);
      final left = boxes.firstWhere((b) => b.paneId == layout.leafIds.first);
      final right = boxes.firstWhere((b) => b.paneId == layout.leafIds.last);
      expect(left.x, 0);
      expect(left.width, 50);
      expect(right.x, 50);
      expect(right.width, 50);
      // Hit-testing on the box rectangles.
      expect(left.contains(10, 10), isTrue);
      expect(left.contains(60, 10), isFalse);
      expect(right.contains(60, 10), isTrue);
    });

    test('zoom/unzoom/toggleZoom are UI-only state', () {
      var layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      layout = layout.split(
        direction: SplitDirection.horizontal,
        newPaneId: 'sess-2',
        newTitle: 'two',
      )!;
      expect(layout.isZoomed, isFalse);
      final zoomed = layout.zoom();
      expect(zoomed.isZoomed, isTrue);
      expect(zoomed.zoomedId, zoomed.focusedId);
      expect(zoomed.unzoom().isZoomed, isFalse);
      final toggled = layout.toggleZoom();
      expect(toggled.isZoomed, isTrue);
      expect(toggled.toggleZoom().isZoomed, isFalse);
      // Zoom does not change the Rust snapshot.
      expect(zoomed.snapshot, layout.snapshot);
      expect(zoomed.paneCount, layout.paneCount);
    });

    test('focusNext cycles panes in tree order', () {
      var layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      layout = layout.split(
        direction: SplitDirection.horizontal,
        newPaneId: 'sess-2',
        newTitle: 'two',
      )!;
      layout = layout.split(
        direction: SplitDirection.horizontal,
        newPaneId: 'sess-3',
        newTitle: 'three',
      )!;
      final ids = layout.leafIds;
      expect(ids.length, 3);
      layout = layout.focus(ids[0]);
      layout = layout.focusNext();
      expect(layout.focusedId, ids[1]);
      layout = layout.focusNext();
      expect(layout.focusedId, ids[2]);
      layout = layout.focusNext();
      expect(layout.focusedId, ids[0]);
      layout = layout.focusNext(reverse: true);
      expect(layout.focusedId, ids[2]);
    });

    test('build renders the layout', () {
      var layout = PaneLayout.single(paneId: 'sess-1', title: 'one');
      layout = layout.split(
        direction: SplitDirection.horizontal,
        newPaneId: 'sess-2',
        newTitle: 'two',
      )!;
      final node = layout.build();
      expect(node.id, 'pane-layout');
      final zoomedNode = layout.zoom().build();
      expect(zoomedNode.id, 'pane-layout-zoomed');
    });
  });
}
