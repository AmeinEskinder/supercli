/// Split-pane layout tree for the terminal area.
///
/// Port of the Ghostty/Swift `SplitPane` layout model. A [PaneLayout] owns a
/// binary tree of [PaneNode]s: leaves are terminal panes, splits are
/// horizontal (side-by-side) or vertical (stacked) containers with a divider
/// ratio. The tree supports up to 8 leaf panes (matching the upstream limit).
///
/// Rendering uses only upstream gpuidart nodes (UiRow/UiColumn/UiText/
/// UiButton) composed recursively — there is no native split primitive in
/// gpuidart (see docs/gpuidart-gaps-panes.md, P0-9). Dividers render as
/// thin UiButton strips that carry resize actions.
///
/// Keyboard model (mirrors upstream):
/// - Split Right: cmd+d, Split Down: shift+cmd+d
/// - Zoom pane: shift+cmd+enter (toggle), Equalize: cmd+shift+e
/// - Spatial focus: alt+cmd+arrows; vim-style: ctrl+w then h/j/k/l
/// - Close pane: cmd+w (when >1 pane), Detach: cmd+shift+o
library;

import 'package:gpuidart/gpuidart.dart';

/// Maximum leaf panes per window (upstream limit).
const int maxPanes = 8;

/// Split direction.
enum SplitDirection {
  /// Side-by-side (left | right).
  horizontal,
  /// Stacked (top / bottom).
  vertical,
}

/// Focus navigation direction.
enum FocusDirection { left, right, up, down }

/// A node in the pane tree.
sealed class PaneNode {
  const PaneNode();

  /// Number of leaf panes under this node.
  int get leafCount;

  /// All leaf pane ids under this node, left-to-right / top-to-bottom.
  List<String> get leafIds;

  /// Find the leaf with [paneId], or null.
  PaneLeaf? findLeaf(String paneId);

  /// Replace the leaf [paneId] with [replacement] (split insertion).
  /// Returns the new subtree, or null if [paneId] is not under this node.
  PaneNode? replaceLeaf(String paneId, PaneNode replacement);

  /// Remove the leaf [paneId], collapsing now-single-child splits.
  /// Returns the new subtree, or null if the tree would become empty.
  PaneNode? removeLeaf(String paneId);

  /// Render this subtree. [focusedId] marks the focused pane, [zoomedId]
  /// the zoomed pane (when non-null only the zoomed pane renders).
  UiNode build({
    required String? focusedId,
    required String? zoomedId,
    required int depth,
  });

  Map<String, Object> toJson();
}

/// A terminal pane leaf.
final class PaneLeaf extends PaneNode {
  const PaneLeaf({
    required this.paneId,
    required this.title,
    this.statusText = '',
  });

  final String paneId;
  final String title;
  final String statusText;

  @override
  int get leafCount => 1;

  @override
  List<String> get leafIds => [paneId];

  @override
  PaneLeaf? findLeaf(String id) => id == paneId ? this : null;

  @override
  PaneNode? replaceLeaf(String id, PaneNode replacement) =>
      id == paneId ? replacement : null;

  @override
  PaneNode? removeLeaf(String id) => id == paneId ? null : this;

  @override
  UiNode build({
    required String? focusedId,
    required String? zoomedId,
    required int depth,
  }) {
    final focused = focusedId == paneId;
    return UiColumn('pane-leaf-$paneId', [
      UiRow('pane-header-$paneId', [
        UiText('pane-title-$paneId', title),
        if (focused) const UiText('pane-focused-marker', '●'),
        UiButton('pane-zoom-$paneId', '⛶'),
        UiButton('pane-split-h-$paneId', '◫'),
        UiButton('pane-split-v-$paneId', '◧'),
        UiButton('pane-close-$paneId', '×'),
      ]),
      UiText(
        'pane-body-$paneId',
        statusText.isEmpty ? '[$title]' : statusText,
      ),
    ]);
  }

  @override
  Map<String, Object> toJson() => {
    'kind': 'leaf',
    'pane_id': paneId,
    'title': title,
  };

  @override
  bool operator ==(Object other) =>
      other is PaneLeaf && paneId == other.paneId && title == other.title;

  @override
  int get hashCode => Object.hash(paneId, title);
}

/// A horizontal or vertical split of two children with a divider ratio.
///
/// [ratio] is the fraction of space given to [first] (0.1–0.9).
final class PaneSplit extends PaneNode {
  const PaneSplit({
    required this.direction,
    required this.first,
    required this.second,
    this.ratio = 0.5,
  }) : assert(ratio >= 0.1 && ratio <= 0.9);

  final SplitDirection direction;
  final PaneNode first;
  final PaneNode second;
  final double ratio;

  PaneSplit copyWith({
    SplitDirection? direction,
    PaneNode? first,
    PaneNode? second,
    double? ratio,
  }) =>
      PaneSplit(
        direction: direction ?? this.direction,
        first: first ?? this.first,
        second: second ?? this.second,
        ratio: ratio ?? this.ratio,
      );

  @override
  int get leafCount => first.leafCount + second.leafCount;

  @override
  List<String> get leafIds => [...first.leafIds, ...second.leafIds];

  @override
  PaneLeaf? findLeaf(String paneId) =>
      first.findLeaf(paneId) ?? second.findLeaf(paneId);

  @override
  PaneNode? replaceLeaf(String paneId, PaneNode replacement) {
    if (first.findLeaf(paneId) != null) {
      final newFirst = first.replaceLeaf(paneId, replacement);
      // findLeaf succeeded so replaceLeaf must return non-null.
      return copyWith(first: newFirst!);
    }
    if (second.findLeaf(paneId) != null) {
      final newSecond = second.replaceLeaf(paneId, replacement);
      return copyWith(second: newSecond!);
    }
    return null;
  }

  @override
  PaneNode? removeLeaf(String paneId) {
    if (first.findLeaf(paneId) != null) {
      final rest = first.removeLeaf(paneId);
      if (rest == null) return second; // first was the leaf: collapse
      return copyWith(first: rest);
    }
    if (second.findLeaf(paneId) != null) {
      final rest = second.removeLeaf(paneId);
      if (rest == null) return first; // second was the leaf: collapse
      return copyWith(second: rest);
    }
    return null;
  }

  /// Reset all divider ratios in this subtree to 0.5.
  PaneSplit equalized() => PaneSplit(
        direction: direction,
        first: first is PaneSplit ? (first as PaneSplit).equalized() : first,
        second:
            second is PaneSplit ? (second as PaneSplit).equalized() : second,
        ratio: 0.5,
      );

  @override
  UiNode build({
    required String? focusedId,
    required String? zoomedId,
    required int depth,
  }) {
    final dividerId = 'pane-divider-d$depth-${direction.name}';
    final firstNode =
        first.build(focusedId: focusedId, zoomedId: zoomedId, depth: depth + 1);
    final secondNode = second
        .build(focusedId: focusedId, zoomedId: zoomedId, depth: depth + 1);
    // Dividers render as button strips (no native divider in gpuidart, P0-9).
    final divider = direction == SplitDirection.horizontal
        ? UiButton(dividerId, '│')
        : UiButton(dividerId, '─');
    final children = [firstNode, divider, secondNode];
    return direction == SplitDirection.horizontal
        ? UiRow('pane-split-d$depth-h', children)
        : UiColumn('pane-split-d$depth-v', children);
  }

  @override
  Map<String, Object> toJson() => {
    'kind': 'split',
    'direction': direction.name,
    'ratio': ratio,
    'first': first.toJson(),
    'second': second.toJson(),
  };
}

/// Owns the pane tree plus focus and zoom state.
///
/// All mutations return a new [PaneLayout] (immutable model); the caller
/// pushes the new layout into the app state.
final class PaneLayout {
  const PaneLayout({
    required this.root,
    this.focusedId,
    this.zoomedId,
  });

  /// Single-pane initial layout.
  factory PaneLayout.single({required String paneId, required String title}) =>
      PaneLayout(
        root: PaneLeaf(paneId: paneId, title: title),
        focusedId: paneId,
      );

  final PaneNode root;
  final String? focusedId;
  final String? zoomedId;

  int get paneCount => root.leafCount;
  bool get isZoomed => zoomedId != null;

  PaneLayout copyWith({
    PaneNode? root,
    String? Function()? focusedId,
    String? Function()? zoomedId,
  }) =>
      PaneLayout(
        root: root ?? this.root,
        focusedId: focusedId != null ? focusedId() : this.focusedId,
        zoomedId: zoomedId != null ? zoomedId() : this.zoomedId,
      );

  /// Split [paneId] (or the focused pane) in [direction], adding a new pane
  /// with [newPaneId]/[newTitle]. Returns null at the 8-pane limit.
  PaneLayout? split({
    String? paneId,
    required SplitDirection direction,
    required String newPaneId,
    required String newTitle,
  }) {
    final target = paneId ?? focusedId;
    if (target == null) return null;
    if (paneCount >= maxPanes) return null;
    final leaf = root.findLeaf(target);
    if (leaf == null) return null;
    final replacement = PaneSplit(
      direction: direction,
      first: leaf,
      second: PaneLeaf(paneId: newPaneId, title: newTitle),
    );
    final newRoot = root.replaceLeaf(target, replacement);
    if (newRoot == null) return null;
    return copyWith(root: newRoot, focusedId: () => newPaneId);
  }

  /// Close [paneId] (or the focused pane). Returns null when it would leave
  /// zero panes.
  PaneLayout? closePane({String? paneId}) {
    final target = paneId ?? focusedId;
    if (target == null) return null;
    if (paneCount <= 1) return null;
    final newRoot = root.removeLeaf(target);
    if (newRoot == null) return null;
    // Move focus to the first remaining leaf; clear zoom if it was zoomed.
    final newFocus = newRoot.leafIds.first;
    return PaneLayout(
      root: newRoot,
      focusedId: newFocus,
      zoomedId: zoomedId == target ? null : zoomedId,
    );
  }

  /// Zoom [paneId] (or the focused pane) to fill the window.
  PaneLayout zoom({String? paneId}) {
    final target = paneId ?? focusedId;
    return copyWith(zoomedId: () => target);
  }

  /// Exit zoom.
  PaneLayout unzoom() => copyWith(zoomedId: () => null);

  /// Toggle zoom for [paneId] (or the focused pane).
  PaneLayout toggleZoom({String? paneId}) {
    final target = paneId ?? focusedId;
    if (zoomedId == target) return unzoom();
    return zoom(paneId: target);
  }

  /// Reset every divider ratio to 0.5.
  PaneLayout equalize() {
    final newRoot = root is PaneSplit ? (root as PaneSplit).equalized() : root;
    return copyWith(root: newRoot);
  }

  /// Set the divider ratio for the split directly containing [paneId].
  PaneLayout setRatio(String paneId, double ratio) {
    final clamped = ratio.clamp(0.1, 0.9);
    PaneNode update(PaneNode node) {
      if (node is PaneSplit) {
        if (node.first.findLeaf(paneId) != null ||
            node.second.findLeaf(paneId) != null) {
          // Only adjust the *innermost* split containing the pane: recurse
          // first so deeper splits win.
          final newFirst =
              node.first.findLeaf(paneId) != null ? update(node.first) : node.first;
          final newSecond = node.second.findLeaf(paneId) != null
              ? update(node.second)
              : node.second;
          if (!identical(newFirst, node.first) ||
              !identical(newSecond, node.second)) {
            return node.copyWith(first: newFirst, second: newSecond);
          }
          return node.copyWith(ratio: clamped);
        }
      }
      return node;
    }

    return copyWith(root: update(root));
  }

  /// Focus [paneId] directly.
  PaneLayout focus(String paneId) {
    if (root.findLeaf(paneId) == null) return this;
    return copyWith(focusedId: () => paneId);
  }

  /// Move focus spatially from the focused pane in [direction].
  ///
  /// Computes each leaf's bounding box in unit space from the split ratios,
  /// then picks the nearest leaf whose box lies in the requested direction.
  PaneLayout focusDirection(FocusDirection direction) {
    final current = focusedId;
    if (current == null) return this;
    final boxes = _layoutBoxes();
    final from = boxes[current];
    if (from == null) return this;

    String? best;
    var bestScore = double.infinity;
    for (final entry in boxes.entries) {
      if (entry.key == current) continue;
      final to = entry.value;
      final score = _directionScore(from, to, direction);
      if (score != null && score < bestScore) {
        bestScore = score;
        best = entry.key;
      }
    }
    if (best == null) return this;
    return copyWith(focusedId: () => best);
  }

  /// Focus the next/previous pane in tree order (Ctrl-Tab style cycling).
  PaneLayout focusNext({bool reverse = false}) {
    final ids = root.leafIds;
    if (ids.isEmpty) return this;
    final idx = ids.indexOf(focusedId ?? '');
    final next = reverse
        ? (idx <= 0 ? ids.length - 1 : idx - 1)
        : (idx < 0 || idx >= ids.length - 1 ? 0 : idx + 1);
    return copyWith(focusedId: () => ids[next]);
  }

  /// Unit-space bounding boxes for every leaf: (left, top, right, bottom).
  Map<String, (double, double, double, double)> _layoutBoxes() {
    final boxes = <String, (double, double, double, double)>{};
    void walk(PaneNode node, double l, double t, double r, double b) {
      switch (node) {
        case PaneLeaf(paneId: final id):
          boxes[id] = (l, t, r, b);
        case PaneSplit(
            direction: final dir,
            first: final f,
            second: final s,
            ratio: final ratio
          ):
          if (dir == SplitDirection.horizontal) {
            final mid = l + (r - l) * ratio;
            walk(f, l, t, mid, b);
            walk(s, mid, t, r, b);
          } else {
            final mid = t + (b - t) * ratio;
            walk(f, l, t, r, mid);
            walk(s, l, mid, r, b);
          }
      }
    }

    walk(root, 0, 0, 1, 1);
    return boxes;
  }

  /// Score for moving from [from] to [to] in [direction]; null when [to] is
  /// not in that direction. Lower is better (nearest edge, then overlap).
  static double? _directionScore(
    (double, double, double, double) from,
    (double, double, double, double) to,
    FocusDirection direction,
  ) {
    final (fl, ft, fr, fb) = from;
    final (tl, tt, tr, tb) = to;
    switch (direction) {
      case FocusDirection.left:
        if (tr > fl) return null;
        final gap = fl - tr;
        final overlap = _overlap(ft, fb, tt, tb);
        return gap * 1000 - overlap;
      case FocusDirection.right:
        if (tl < fr) return null;
        final gap = tl - fr;
        final overlap = _overlap(ft, fb, tt, tb);
        return gap * 1000 - overlap;
      case FocusDirection.up:
        if (tb > ft) return null;
        final gap = ft - tb;
        final overlap = _overlap(fl, fr, tl, tr);
        return gap * 1000 - overlap;
      case FocusDirection.down:
        if (tt < fb) return null;
        final gap = tt - fb;
        final overlap = _overlap(fl, fr, tl, tr);
        return gap * 1000 - overlap;
    }
  }

  static double _overlap(double a1, double a2, double b1, double b2) {
    final lo = a1 > b1 ? a1 : b1;
    final hi = a2 < b2 ? a2 : b2;
    return hi > lo ? hi - lo : 0;
  }

  /// Render the layout. When zoomed, only the zoomed pane renders (plus a
  /// zoom banner).
  UiNode build() {
    if (zoomedId != null) {
      final leaf = root.findLeaf(zoomedId!);
      if (leaf != null) {
        return UiColumn('pane-layout-zoomed', [
          UiRow('pane-zoom-banner', [
            const UiText('pane-zoom-label', 'ZOOMED'),
            UiButton('pane-unzoom', 'Unzoom (⇧⌘↩)'),
          ]),
          leaf.build(focusedId: focusedId, zoomedId: zoomedId, depth: 0),
        ]);
      }
    }
    return UiColumn('pane-layout', [
      root.build(focusedId: focusedId, zoomedId: zoomedId, depth: 0),
    ]);
  }

  /// Key bindings for pane management, scoped to the terminal area node.
  List<UiAction> actions() => const [
        UiAction(
            name: 'pane.splitRight',
            keys: 'cmd+d',
            context: UiActionContext.node('pane-layout')),
        UiAction(
            name: 'pane.splitDown',
            keys: 'shift+cmd+d',
            context: UiActionContext.node('pane-layout')),
        UiAction(
            name: 'pane.zoom',
            keys: 'shift+cmd+enter',
            context: UiActionContext.node('pane-layout')),
        UiAction(
            name: 'pane.equalize',
            keys: 'cmd+shift+e',
            context: UiActionContext.node('pane-layout')),
        UiAction(
            name: 'pane.close',
            keys: 'cmd+w',
            context: UiActionContext.node('pane-layout')),
        UiAction(
            name: 'pane.detach',
            keys: 'cmd+shift+o',
            context: UiActionContext.node('pane-layout')),
        UiAction(
            name: 'pane.focusLeft',
            keys: 'alt+cmd+left',
            context: UiActionContext.node('pane-layout')),
        UiAction(
            name: 'pane.focusRight',
            keys: 'alt+cmd+right',
            context: UiActionContext.node('pane-layout')),
        UiAction(
            name: 'pane.focusUp',
            keys: 'alt+cmd+up',
            context: UiActionContext.node('pane-layout')),
        UiAction(
            name: 'pane.focusDown',
            keys: 'alt+cmd+down',
            context: UiActionContext.node('pane-layout')),
        UiAction(
            name: 'pane.focusNext',
            keys: 'ctrl+tab',
            context: UiActionContext.node('pane-layout')),
        UiAction(
            name: 'pane.focusPrev',
            keys: 'ctrl+shift+tab',
            context: UiActionContext.node('pane-layout')),
        UiAction(
            name: 'find.show',
            keys: 'cmd+f',
            context: UiActionContext.node('pane-layout')),
      ];

  Map<String, Object> toJson() {
    final map = <String, Object>{'root': root.toJson()};
    final f = focusedId;
    if (f != null) map['focused_id'] = f;
    final z = zoomedId;
    if (z != null) map['zoomed_id'] = z;
    return map;
  }
}
