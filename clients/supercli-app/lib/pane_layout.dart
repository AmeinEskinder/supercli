/// Split-pane layout state for the terminal area.
///
/// The tree model lives in Rust (`supercli-client::pane_layout`, exposed
/// through `supercli-client-ffi` as `supercli_pane_layout_*`). Dart keeps
/// only:
///
/// - rendering ([PaneNode.build], [PaneLayout.build]),
/// - hit-testing ([PaneLayout.leafBoxes]),
/// - UI-only state: focus, zoom, and pane titles.
///
/// [PaneLayout] owns the Rust JSON snapshot; every mutation (`split`,
/// `closePane`, `setRatio`, `equalize`, `swap`, `focusDirection`,
/// `reconcile`) is a thin synchronous FFI call that returns a new
/// [PaneLayout] holding the updated snapshot. Tree mutation, equalization,
/// resizing, and spatial-neighbor algorithms do not live here.
///
/// ## Pane ids
///
/// Dart-visible pane ids are *logical* ids: the session id for session
/// leaves (Rust's `PaneContent::Session.id`), which the Rust model keeps
/// unique per layout. Rust's internal pane ids (`Pane.id`) are an FFI
/// implementation detail — `Pane::new` canonicalizes caller-supplied ids
/// (only UUID-shaped ids survive verbatim) — so Dart translates logical ->
/// internal before every FFI call and internal -> logical on the way back.
/// Callers always see the ids they supplied (`single(paneId: 'pane-1')`
/// yields a leaf id of `'pane-1'`), which keeps view lookup (`views` keyed
/// by pane id) and titles working.
library;

import 'dart:convert';

import 'package:gpuidart/gpuidart.dart';

import 'keymap.dart';
import 'native_client.dart';
import 'screens/terminalpaneview.dart';

/// Maximum panes per layout (matches the Rust capacity rule).
const int maxPanes = 8;

/// A horizontal split (side-by-side panes) or vertical split (stacked).
enum SplitDirection { horizontal, vertical }

/// Cardinal directions for spatial focus movement.
enum FocusDirection { left, right, up, down }

/// Render-data node decoded from a Rust pane-layout snapshot.
///
/// Presentation only: carries just enough structure to build gpuidart nodes
/// and read leaf order. The authoritative tree lives in Rust.
sealed class PaneNode {
  const PaneNode();

  /// Decode a Rust snapshot node (`{"kind": "leaf"|"split", ...}`).
  factory PaneNode.fromJson(
    Map<String, dynamic> json, {
    Map<String, String> titles = const {},
  }) {
    final kind = json['kind'] as String?;
    if (kind == 'leaf') {
      // Dart-visible id is the logical id (session id for session
      // leaves); see the library docs.
      final paneId = _logicalLeafId(json);
      return PaneLeaf(
        paneId: paneId,
        title: titles[paneId] ?? paneId,
      );
    }
    if (kind == 'split') {
      final directionName = json['direction'] as String?;
      final direction = directionName == 'vertical'
          ? SplitDirection.vertical
          : SplitDirection.horizontal;
      final ratio = (json['ratio'] as num?)?.toDouble() ?? 0.5;
      final left = json['left'];
      final right = json['right'];
      return PaneSplit(
        direction: direction,
        first: left is Map<String, dynamic>
            ? PaneNode.fromJson(left, titles: titles)
            : const PaneLeaf(paneId: '', title: ''),
        second: right is Map<String, dynamic>
            ? PaneNode.fromJson(right, titles: titles)
            : const PaneLeaf(paneId: '', title: ''),
        ratio: ratio.clamp(0.1, 0.9),
      );
    }
    throw FormatException('unknown pane node kind: $kind');
  }

  /// Leaf pane ids in tree order (used for focus cycling).
  List<String> get leafIds;

  UiNode build({
    required String? focusedId,
    required String? zoomedId,
    required int depth,
    Map<String, TerminalPaneView>? views,
  });
}

/// A leaf pane: stable id + display title.
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
  List<String> get leafIds => [paneId];

  @override
  UiNode build({
    required String? focusedId,
    required String? zoomedId,
    required int depth,
    Map<String, TerminalPaneView>? views,
  }) {
    final focused = focusedId == paneId;
    final live = views?[paneId];
    return UiColumn('pane-leaf-$paneId', [
      UiRow('pane-header-$paneId', [
        UiText('pane-title-$paneId', title),
        if (focused) const UiText('pane-focused-marker', '●'),
        UiButton('pane-zoom-$paneId', '⛶'),
        UiButton('pane-split-h-$paneId', '◫'),
        UiButton('pane-split-v-$paneId', '◧'),
        UiButton('pane-close-$paneId', '×'),
      ]),
      // A registered live view renders the real terminal surface (grid
      // fed by the Host output stream); otherwise the placeholder.
      if (live != null)
        live.build()
      else
        UiText(
          'pane-body-$paneId',
          statusText.isEmpty ? '[$title]' : statusText,
        ),
    ]);
  }
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

  @override
  List<String> get leafIds => [...first.leafIds, ...second.leafIds];

  @override
  UiNode build({
    required String? focusedId,
    required String? zoomedId,
    required int depth,
    Map<String, TerminalPaneView>? views,
  }) {
    final dividerId = 'pane-divider-d$depth-${direction.name}';
    final firstNode = first.build(
      focusedId: focusedId,
      zoomedId: zoomedId,
      depth: depth + 1,
      views: views,
    );
    final secondNode = second.build(
      focusedId: focusedId,
      zoomedId: zoomedId,
      depth: depth + 1,
      views: views,
    );
    // Dividers render as button strips (no native divider in gpuidart, P0-9).
    final divider = direction == SplitDirection.horizontal
        ? UiButton(dividerId, '│')
        : UiButton(dividerId, '─');
    final children = [firstNode, divider, secondNode];
    return direction == SplitDirection.horizontal
        ? UiRow('pane-split-d$depth-h', children)
        : UiColumn('pane-split-d$depth-v', children);
  }
}

/// A leaf's rectangle for hit-testing, in the caller's coordinate space.
final class PaneHitBox {
  const PaneHitBox({
    required this.paneId,
    required this.x,
    required this.y,
    required this.width,
    required this.height,
  });

  final String paneId;
  final double x;
  final double y;
  final double width;
  final double height;

  bool contains(double px, double py) =>
      px >= x && px < x + width && py >= y && py < y + height;
}

/// Logical (Dart-visible) id of a leaf node in a Rust snapshot: the
/// session id for session leaves, else the Rust pane id. Session ids are
/// unique per layout (the Rust model rejects duplicates), so they are a
/// stable Dart identity; Rust's internal pane ids stay behind the FFI
/// boundary.
String _logicalLeafId(Map<String, dynamic> node) {
  final content = node['content'];
  final sessionId = content is Map<String, dynamic>
      ? content['id'] as String?
      : null;
  if (sessionId != null && sessionId.isNotEmpty) return sessionId;
  final id = node['id'];
  return id is String ? id : '';
}

/// Leaf pane ids of the given Rust snapshot, for [groupId].
/// These are logical (Dart-visible) ids; see [_logicalLeafId].
List<String> _leafIdsOfSnapshot(String snapshot, String groupId) {
  final root = _groupRootOf(snapshot, groupId);
  if (root == null) return const [];
  return _collectLeafIds(root);
}

Map<String, dynamic>? _groupRootOf(String snapshot, String groupId) {
  final decoded = jsonDecode(snapshot);
  if (decoded is! Map<String, dynamic>) return null;
  final groups = decoded['groups'];
  if (groups is! List) return null;
  Map<String, dynamic>? firstRoot;
  for (final group in groups) {
    if (group is! Map<String, dynamic>) continue;
    final root = group['root'];
    if (root is! Map<String, dynamic>) continue;
    firstRoot ??= root;
    if (group['id'] == groupId) return root;
  }
  return firstRoot;
}

List<String> _collectLeafIds(Map<String, dynamic> node) {
  if (node['kind'] == 'leaf') {
    return [_logicalLeafId(node)];
  }
  return [
    for (final child in [node['left'], node['right']])
      if (child is Map<String, dynamic>) ..._collectLeafIds(child),
  ];
}

/// Internal Rust pane id for the leaf with logical id [paneId], or null
/// when the snapshot has no such leaf.
String? _rustIdFor(String snapshot, String groupId, String paneId) {
  final root = _groupRootOf(snapshot, groupId);
  if (root == null) return null;
  return _findRustId(root, paneId);
}

String? _findRustId(Map<String, dynamic> node, String logicalId) {
  if (node['kind'] == 'leaf') {
    return _logicalLeafId(node) == logicalId ? node['id'] as String? : null;
  }
  for (final child in [node['left'], node['right']]) {
    if (child is Map<String, dynamic>) {
      final found = _findRustId(child, logicalId);
      if (found != null) return found;
    }
  }
  return null;
}

/// Logical (Dart-visible) id for the leaf with internal Rust id [rustId],
/// or null when the snapshot has no such leaf.
String? _logicalIdForRustId(String snapshot, String groupId, String rustId) {
  final root = _groupRootOf(snapshot, groupId);
  if (root == null) return null;
  return _findLogicalId(root, rustId);
}

String? _findLogicalId(Map<String, dynamic> node, String rustId) {
  if (node['kind'] == 'leaf') {
    return node['id'] == rustId ? _logicalLeafId(node) : null;
  }
  for (final child in [node['left'], node['right']]) {
    if (child is Map<String, dynamic>) {
      final found = _findLogicalId(child, rustId);
      if (found != null) return found;
    }
  }
  return null;
}

/// Owns the Rust pane-layout snapshot plus focus and zoom state.
///
/// All mutations return a new [PaneLayout] (immutable model); the caller
/// pushes the new layout into the app state.
final class PaneLayout {
  const PaneLayout({
    required this.snapshot,
    required this.groupId,
    this.focusedId,
    this.zoomedId,
    this.titles = const {},
  });

  /// Single-pane initial layout. The Dart-visible pane id is [paneId]
  /// itself (it becomes the session id, which is the logical id); Rust's
  /// internal pane id is an FFI detail and is never exposed.
  factory PaneLayout.single({required String paneId, required String title}) {
    final result = SupercliNative.paneLayoutSingle(
      sessionId: paneId,
      paneId: paneId,
    );
    if (result == null) {
      throw StateError(
        'pane layout FFI failed: ${SupercliNativeBindings.lastError}',
      );
    }
    final rawSnapshot = result['snapshot'];
    return PaneLayout(
      snapshot: rawSnapshot is Map<String, dynamic>
          ? jsonEncode(rawSnapshot)
          : '{}',
      groupId: result['group_id'] as String? ?? '',
      focusedId: paneId,
      titles: {paneId: title},
    );
  }

  /// The Rust `PaneLayoutState` snapshot as JSON. Opaque to Dart except for
  /// rendering and hit-testing.
  final String snapshot;

  /// The Rust group this layout renders and mutates.
  final String groupId;

  final String? focusedId;
  final String? zoomedId;

  /// Display titles keyed by logical (Dart-visible) pane id.
  final Map<String, String> titles;

  /// Logical (Dart-visible) pane ids in tree order.
  List<String> get leafIds => _leafIdsOfSnapshot(snapshot, groupId);
  int get paneCount => leafIds.length;
  bool get isZoomed => zoomedId != null;

  PaneNode _renderRoot() {
    final root = _groupRootOf(snapshot, groupId);
    if (root == null) return const PaneLeaf(paneId: '', title: '');
    return PaneNode.fromJson(root, titles: titles);
  }

  PaneLayout copyWith({
    String? snapshot,
    String? groupId,
    String? Function()? focusedId,
    String? Function()? zoomedId,
    Map<String, String>? titles,
  }) => PaneLayout(
    snapshot: snapshot ?? this.snapshot,
    groupId: groupId ?? this.groupId,
    focusedId: focusedId != null ? focusedId() : this.focusedId,
    zoomedId: zoomedId != null ? zoomedId() : this.zoomedId,
    titles: titles ?? this.titles,
  );

  /// Apply an FFI mutation result: new snapshot, group id, pruned titles.
  PaneLayout _withResult(
    Map<String, dynamic> result, {
    String? Function()? focusedId,
    Map<String, String>? extraTitles,
  }) {
    final rawSnapshot = result['snapshot'];
    final newSnapshot = rawSnapshot is Map<String, dynamic>
        ? jsonEncode(rawSnapshot)
        : snapshot;
    final newGroupId = result['group_id'] as String? ?? groupId;
    final newTitles = <String, String>{};
    for (final id in _leafIdsOfSnapshot(newSnapshot, newGroupId)) {
      final title = extraTitles?[id] ?? titles[id];
      if (title != null) newTitles[id] = title;
    }
    return PaneLayout(
      snapshot: newSnapshot,
      groupId: newGroupId,
      focusedId: focusedId != null ? focusedId() : this.focusedId,
      zoomedId: zoomedId,
      titles: newTitles,
    );
  }

  /// Split [paneId] (or the focused pane) in [direction], adding a new pane
  /// with session [newPaneId] and display [newTitle]. The new pane's
  /// Dart-visible id is [newPaneId]. Returns null at the 8-pane limit, when
  /// there is no target, or when Rust rejects the split.
  PaneLayout? split({
    String? paneId,
    required SplitDirection direction,
    required String newPaneId,
    required String newTitle,
  }) {
    final target = paneId ?? focusedId;
    if (target == null) return null;
    final rustTarget = _rustIdFor(snapshot, groupId, target);
    if (rustTarget == null) return null;
    if (paneCount >= maxPanes) return null;
    final edge = direction == SplitDirection.horizontal ? 'right' : 'down';
    final result = SupercliNative.paneLayoutInsert(
      snapshot: snapshot,
      sessionId: newPaneId,
      targetPaneId: rustTarget,
      edge: edge,
    );
    if (result == null) return null;
    return _withResult(
      result,
      focusedId: () => newPaneId,
      extraTitles: {newPaneId: newTitle},
    );
  }

  /// Close [paneId] (or the focused pane). Returns null when it would leave
  /// zero panes, when there is no target, or when Rust rejects the close.
  PaneLayout? closePane({String? paneId}) {
    final target = paneId ?? focusedId;
    if (target == null) return null;
    final rustTarget = _rustIdFor(snapshot, groupId, target);
    if (rustTarget == null) return null;
    if (paneCount <= 1) return null;
    final result = SupercliNative.paneLayoutClose(
      snapshot: snapshot,
      paneId: rustTarget,
    );
    if (result == null) return null;
    final next = _withResult(result);
    // Move focus to the first remaining leaf; clear zoom if it was zoomed.
    final newFocus = next.leafIds.isEmpty ? null : next.leafIds.first;
    return next.copyWith(
      focusedId: () => newFocus,
      zoomedId: () => zoomedId == target ? null : zoomedId,
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

  /// Reset every divider ratio to equal shares. Unchanged on FFI failure.
  PaneLayout equalize() {
    final result = SupercliNative.paneLayoutEqualize(
      snapshot: snapshot,
      groupId: groupId,
    );
    if (result == null) return this;
    return _withResult(result);
  }

  /// Exchange the positions of two panes. Unchanged on FFI failure.
  PaneLayout swap(String paneIdA, String paneIdB) {
    final rustA = _rustIdFor(snapshot, groupId, paneIdA);
    final rustB = _rustIdFor(snapshot, groupId, paneIdB);
    if (rustA == null || rustB == null) return this;
    final result = SupercliNative.paneLayoutSwap(
      snapshot: snapshot,
      paneIdA: rustA,
      paneIdB: rustB,
    );
    if (result == null) return this;
    return _withResult(result);
  }

  /// Set the divider ratio for the split containing [paneId], clamped to
  /// 0.1–0.9. Unchanged on FFI failure.
  PaneLayout setRatio(String paneId, double ratio) {
    final clamped = ratio.clamp(0.1, 0.9);
    final rustTarget = _rustIdFor(snapshot, groupId, paneId);
    if (rustTarget == null) return this;
    final result = SupercliNative.paneLayoutResize(
      snapshot: snapshot,
      groupId: groupId,
      paneId: rustTarget,
      ratio: clamped,
    );
    if (result == null) return this;
    return _withResult(result);
  }

  /// Focus [paneId] directly.
  PaneLayout focus(String paneId) {
    if (!leafIds.contains(paneId)) return this;
    return copyWith(focusedId: () => paneId);
  }

  /// Move focus spatially from the focused pane in [direction], using the
  /// Rust spatial-neighbor lookup. Unchanged when there is no neighbor.
  /// [focusedId] and the result are logical (Dart-visible) ids.
  PaneLayout focusDirection(FocusDirection direction) {
    final current = focusedId;
    if (current == null) return this;
    final rustCurrent = _rustIdFor(snapshot, groupId, current);
    if (rustCurrent == null) return this;
    final directionName = switch (direction) {
      FocusDirection.left => 'left',
      FocusDirection.right => 'right',
      FocusDirection.up => 'up',
      FocusDirection.down => 'down',
    };
    final neighbor = SupercliNative.paneLayoutNeighbor(
      snapshot: snapshot,
      paneId: rustCurrent,
      direction: directionName,
    );
    if (neighbor == null) return this;
    final logical = _logicalIdForRustId(snapshot, groupId, neighbor);
    if (logical == null) return this;
    return copyWith(focusedId: () => logical);
  }

  /// Focus the next/previous pane in tree order (Ctrl-Tab style cycling).
  PaneLayout focusNext({bool reverse = false}) {
    final ids = leafIds;
    if (ids.isEmpty) return this;
    final idx = ids.indexOf(focusedId ?? '');
    final next = reverse
        ? (idx <= 0 ? ids.length - 1 : idx - 1)
        : (idx < 0 || idx >= ids.length - 1 ? 0 : idx + 1);
    return copyWith(focusedId: () => ids[next]);
  }

  /// Drop sessions not in [eligibleSessionIds], collapsing and dissolving
  /// as the Rust model dictates. Unchanged on FFI failure.
  PaneLayout reconcile(Iterable<String> eligibleSessionIds) {
    final result = SupercliNative.paneLayoutReconcile(
      snapshot: snapshot,
      eligibleSessionIds: eligibleSessionIds.toList(),
    );
    if (result == null) return this;
    final next = _withResult(result);
    final ids = next.leafIds;
    final keptFocus = focusedId;
    final keptZoom = zoomedId;
    return next.copyWith(
      focusedId: () => keptFocus != null && ids.contains(keptFocus)
          ? keptFocus
          : (ids.isEmpty ? null : ids.first),
      zoomedId: () =>
          keptZoom != null && ids.contains(keptZoom) ? keptZoom : null,
    );
  }

  /// Leaf rectangles for rendering/hit-testing in the given bounds.
  /// [PaneHitBox.paneId] values are logical (Dart-visible) ids.
  List<PaneHitBox> leafBoxes({
    required double width,
    required double height,
  }) {
    final boxes = SupercliNative.paneLayoutLeafBoxes(
      snapshot: snapshot,
      groupId: groupId,
      x: 0,
      y: 0,
      width: width,
      height: height,
    );
    return [
      for (final box in boxes)
        PaneHitBox(
          paneId:
              _logicalIdForRustId(
                snapshot,
                groupId,
                box['pane_id'] as String? ?? '',
              ) ??
              '',
          x: (box['x'] as num?)?.toDouble() ?? 0,
          y: (box['y'] as num?)?.toDouble() ?? 0,
          width: (box['width'] as num?)?.toDouble() ?? 0,
          height: (box['height'] as num?)?.toDouble() ?? 0,
        ),
    ];
  }

  /// Render the layout. When zoomed, only the zoomed pane renders (plus a
  /// zoom banner). [views] carries live terminal views keyed by pane id.
  UiNode build({Map<String, TerminalPaneView>? views}) {
    if (zoomedId != null) {
      final zoomed = zoomedId!;
      if (leafIds.contains(zoomed)) {
        final leaf = PaneLeaf(
          paneId: zoomed,
          title: titles[zoomed] ?? zoomed,
        );
        return UiColumn('pane-layout-zoomed', [
          UiRow('pane-zoom-banner', [
            const UiText('pane-zoom-label', 'ZOOMED'),
            UiButton('pane-unzoom', 'Unzoom (⇧⌘↩)'),
          ]),
          leaf.build(
            focusedId: focusedId,
            zoomedId: zoomedId,
            depth: 0,
            views: views,
          ),
        ]);
      }
    }
    return UiColumn('pane-layout', [
      _renderRoot().build(
        focusedId: focusedId,
        zoomedId: zoomedId,
        depth: 0,
        views: views,
      ),
    ]);
  }

  /// Key bindings for pane management, scoped to the terminal area node.
  List<UiAction> actions() => [
    UiAction(
      name: 'pane.splitRight',
      keys: Keymap.splitRight(),
      context: UiActionContext.node('pane-layout'),
    ),
    UiAction(
      name: 'pane.splitDown',
      keys: Keymap.splitDown(),
      context: UiActionContext.node('pane-layout'),
    ),
    UiAction(
      name: 'pane.zoom',
      keys: Keymap.zoomPane(),
      context: UiActionContext.node('pane-layout'),
    ),
    UiAction(
      name: 'pane.equalize',
      keys: Keymap.equalizeSplits(),
      context: UiActionContext.node('pane-layout'),
    ),
    UiAction(
      name: 'pane.close',
      keys: Keymap.closeWindow(),
      context: UiActionContext.node('pane-layout'),
    ),
    UiAction(
      name: 'pane.detach',
      keys: Keymap.detachPane(),
      context: UiActionContext.node('pane-layout'),
    ),
    UiAction(
      name: 'pane.focusLeft',
      keys: Keymap.focusPaneLeft(),
      context: UiActionContext.node('pane-layout'),
    ),
    UiAction(
      name: 'pane.focusRight',
      keys: Keymap.focusPaneRight(),
      context: UiActionContext.node('pane-layout'),
    ),
    UiAction(
      name: 'pane.focusUp',
      keys: Keymap.focusPaneUp(),
      context: UiActionContext.node('pane-layout'),
    ),
    UiAction(
      name: 'pane.focusDown',
      keys: Keymap.focusPaneDown(),
      context: UiActionContext.node('pane-layout'),
    ),
    UiAction(
      name: 'pane.focusNext',
      keys: Keymap.switcherNext,
      context: UiActionContext.node('pane-layout'),
    ),
    UiAction(
      name: 'pane.focusPrev',
      keys: Keymap.switcherPrevious,
      context: UiActionContext.node('pane-layout'),
    ),
    UiAction(
      name: 'find.show',
      keys: Keymap.find(),
      context: UiActionContext.node('pane-layout'),
    ),
  ];
}
