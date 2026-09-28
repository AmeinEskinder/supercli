/// Normative multi-group split-pane state model.
///
/// Faithful port of `PaneLayoutState.swift` (SupercliNative). The operation
/// semantics, durable schema, v1 migration, and the spatial/equalize
/// algorithms implement the cross-implementation contract in
/// `protocol/pane-layout-operations-v1.json` (the TUI's panes module and the
/// Swift client run the same fixture).
///
/// Layering vs `pane_layout.dart`: that file is the rendering-oriented
/// immutable single-window tree (gpuidart [UiNode] building, focus/zoom).
/// This file is the durable state model: multiple [LayoutPaneGroup]s, the
/// session/launcher content distinction, the full operation set
/// (create/insert/detach/reconcile/swap/resize/equalize), canonicalization,
/// and the v1/v2 durable codec. [SplitDirection] is shared with
/// `pane_layout.dart` (its `.name` values match the Swift raw values).
library;

import 'dart:math';

import 'pane_layout.dart' show SplitDirection;

/// Maximum session leaves per group (upstream limit).
const int layoutMaxSessionLeaves = 8;

/// Minimum/maximum divider ratio.
const double layoutMinSplitRatio = 0.1;
const double layoutMaxSplitRatio = 0.9;

/// What a pane shows: a live session, or a transient new-session launcher.
sealed class PaneContent {
  const PaneContent();

  String? get sessionID => switch (this) {
    PaneSession(id: final id) => id,
    PaneLauncher() => null,
  };

  bool get isLauncher => this is PaneLauncher;
}

/// A pane bound to a live session.
final class PaneSession extends PaneContent {
  const PaneSession(this.id);
  final String id;

  @override
  bool operator ==(Object other) => other is PaneSession && other.id == id;
  @override
  int get hashCode => id.hashCode;
  @override
  String toString() => 'PaneSession($id)';
}

/// A transient launcher for starting a session in a project.
final class PaneLauncher extends PaneContent {
  const PaneLauncher(this.projectID);
  final String projectID;

  @override
  bool operator ==(Object other) =>
      other is PaneLauncher && other.projectID == projectID;
  @override
  int get hashCode => projectID.hashCode;
  @override
  String toString() => 'PaneLauncher($projectID)';
}

/// A leaf pane: stable id + content.
final class LayoutPane {
  LayoutPane({String? id, required this.content})
    : id = PaneStableID.canonical(id) ?? PaneStableID.make();

  final String id;
  PaneContent content;

  @override
  bool operator ==(Object other) =>
      other is LayoutPane && other.id == id && other.content == content;
  @override
  int get hashCode => Object.hash(id, content);
  @override
  String toString() => 'LayoutPane($id, $content)';
}

/// A drop/split edge. `left`/`up` place the new leaf as the left
/// (leading/top) child; `right`/`down` as the right (trailing/bottom) child.
enum LayoutPaneEdge {
  left,
  right,
  up,
  down;

  SplitDirection get splitDirection => switch (this) {
    LayoutPaneEdge.left || LayoutPaneEdge.right => SplitDirection.horizontal,
    LayoutPaneEdge.up || LayoutPaneEdge.down => SplitDirection.vertical,
  };

  bool get newLeafIsLeftChild => switch (this) {
    LayoutPaneEdge.left || LayoutPaneEdge.up => true,
    LayoutPaneEdge.right || LayoutPaneEdge.down => false,
  };

  static LayoutPaneEdge? fromName(String? name) {
    for (final edge in LayoutPaneEdge.values) {
      if (edge.name == name) return edge;
    }
    return null;
  }
}

/// Where a dragged session would land: splitting a specific pane on one of
/// its four edges, or splitting the whole group's root at a content edge.
/// Presentation state only — never persisted.
sealed class LayoutPaneDropTarget {
  const LayoutPaneDropTarget();
}

/// Split [paneID] on [edge].
final class LayoutPaneDropOnPane extends LayoutPaneDropTarget {
  const LayoutPaneDropOnPane(this.paneID, this.edge);
  final String paneID;
  final LayoutPaneEdge edge;
}

/// Split the group's root at [edge].
final class LayoutPaneDropOnGroupEdge extends LayoutPaneDropTarget {
  const LayoutPaneDropOnGroupEdge(this.edge);
  final LayoutPaneEdge edge;
}

/// Addresses a split node inside a group's tree. The empty path is the root.
final class LayoutPaneSplitPath {
  const LayoutPaneSplitPath([this.components = const []]);

  final List<LayoutPaneSplitBranch> components;

  bool get isEmpty => components.isEmpty;

  @override
  bool operator ==(Object other) {
    if (other is! LayoutPaneSplitPath) return false;
    if (components.length != other.components.length) return false;
    for (var i = 0; i < components.length; i++) {
      if (components[i] != other.components[i]) return false;
    }
    return true;
  }

  @override
  int get hashCode => Object.hashAll(components);
  @override
  String toString() => 'LayoutPaneSplitPath(${components.join(',')})';
}

/// Left/right child selector for [LayoutPaneSplitPath].
enum LayoutPaneSplitBranch {
  left,
  right;

  static LayoutPaneSplitBranch? fromName(String? name) {
    for (final branch in LayoutPaneSplitBranch.values) {
      if (branch.name == name) return branch;
    }
    return null;
  }
}

/// A binary split with a divider ratio.
final class LayoutPaneSplit {
  LayoutPaneSplit({
    required this.direction,
    required double ratio,
    required this.left,
    required this.right,
  }) : _ratio = clampedRatio(ratio);

  final SplitDirection direction;
  double _ratio;
  LayoutPaneNode left;
  LayoutPaneNode right;

  double get ratio => _ratio;
  set ratio(double value) => _ratio = clampedRatio(value);

  static double clampedRatio(double ratio) {
    if (!ratio.isFinite) return 0.5;
    return ratio.clamp(layoutMinSplitRatio, layoutMaxSplitRatio).toDouble();
  }

  @override
  bool operator ==(Object other) =>
      other is LayoutPaneSplit &&
      other.direction == direction &&
      other.ratio == ratio &&
      other.left == left &&
      other.right == right;
  @override
  int get hashCode => Object.hash(direction, ratio, left, right);
}

/// Simple axis-aligned rect (replaces CoreGraphics CGRect).
final class LayoutRect {
  const LayoutRect({
    required this.x,
    required this.y,
    required this.width,
    required this.height,
  });

  final double x;
  final double y;
  final double width;
  final double height;

  double get minX => x;
  double get maxX => x + width;
  double get minY => y;
  double get maxY => y + height;

  @override
  bool operator ==(Object other) =>
      other is LayoutRect &&
      other.x == x &&
      other.y == y &&
      other.width == width &&
      other.height == height;
  @override
  int get hashCode => Object.hash(x, y, width, height);
  @override
  String toString() => 'LayoutRect($x,$y ${width}x$height)';
}

/// A node in the recursive split tree.
sealed class LayoutPaneNode {
  const LayoutPaneNode();

  /// Leaves in preorder (left-first). Preorder is the normative leaf order:
  /// it drives sidebar rows, representative promotion, and cap trimming.
  List<LayoutPane> get leaves => switch (this) {
    LayoutPaneLeaf(pane: final pane) => [pane],
    LayoutPaneSplitNode(split: final split) => [
      ...split.left.leaves,
      ...split.right.leaves,
    ],
  };

  List<LayoutPane> get sessionLeaves =>
      leaves.where((pane) => pane.content.sessionID != null).toList();

  bool get containsLauncher => leaves.any((pane) => pane.content.isLauncher);

  LayoutPane? leaf(String paneID) {
    for (final pane in leaves) {
      if (pane.id == paneID) return pane;
    }
    return null;
  }

  LayoutPaneSplitPath? pathToPane(String paneID) => switch (this) {
    LayoutPaneLeaf(pane: final pane) =>
      pane.id == paneID ? const LayoutPaneSplitPath() : null,
    LayoutPaneSplitNode(split: final split) => _pathInSplit(split, paneID),
  };

  static LayoutPaneSplitPath? _pathInSplit(
    LayoutPaneSplit split,
    String paneID,
  ) {
    final left = split.left.pathToPane(paneID);
    if (left != null) {
      return LayoutPaneSplitPath([
        LayoutPaneSplitBranch.left,
        ...left.components,
      ]);
    }
    final right = split.right.pathToPane(paneID);
    if (right != null) {
      return LayoutPaneSplitPath([
        LayoutPaneSplitBranch.right,
        ...right.components,
      ]);
    }
    return null;
  }

  LayoutPaneNode? nodeAt(LayoutPaneSplitPath path) {
    LayoutPaneNode current = this;
    for (final component in path.components) {
      final node = current;
      if (node is! LayoutPaneSplitNode) return null;
      current = component == LayoutPaneSplitBranch.left
          ? node.split.left
          : node.split.right;
    }
    return current;
  }

  LayoutPaneNode replacingNode(
    LayoutPaneSplitPath path,
    LayoutPaneNode node,
  ) {
    if (path.isEmpty) return node;
    final self = this;
    if (self is! LayoutPaneSplitNode) return self;
    final first = path.components.first;
    final rest = LayoutPaneSplitPath(path.components.sublist(1));
    final split = self.split;
    if (first == LayoutPaneSplitBranch.left) {
      return LayoutPaneSplitNode(
        LayoutPaneSplit(
          direction: split.direction,
          ratio: split.ratio,
          left: split.left.replacingNode(rest, node),
          right: split.right,
        ),
      );
    }
    return LayoutPaneSplitNode(
      LayoutPaneSplit(
        direction: split.direction,
        ratio: split.ratio,
        left: split.left,
        right: split.right.replacingNode(rest, node),
      ),
    );
  }

  /// Removes a leaf; the surviving sibling replaces its parent split, so
  /// single-child splits never exist. Returns null when the tree empties.
  LayoutPaneNode? removingLeaf(String paneID) => switch (this) {
    LayoutPaneLeaf(pane: final pane) => pane.id == paneID ? null : this,
    LayoutPaneSplitNode(split: final split) =>
      _removingFromSplit(split, paneID),
  };

  static LayoutPaneNode? _removingFromSplit(
    LayoutPaneSplit split,
    String paneID,
  ) {
    final left = split.left;
    final right = split.right;
    if (left is LayoutPaneLeaf && left.pane.id == paneID) return right;
    if (right is LayoutPaneLeaf && right.pane.id == paneID) return left;
    final newLeft = left.removingLeaf(paneID);
    if (newLeft == null) return right;
    final newRight = right.removingLeaf(paneID);
    if (newRight == null) return left;
    return LayoutPaneSplitNode(
      LayoutPaneSplit(
        direction: split.direction,
        ratio: split.ratio,
        left: newLeft,
        right: newRight,
      ),
    );
  }

  LayoutPaneNode updatingLeaf(
    String paneID,
    void Function(LayoutPane pane) transform,
  ) => switch (this) {
    LayoutPaneLeaf(pane: final pane) => () {
      if (pane.id != paneID) return this;
      transform(pane);
      return this;
    }(),
    LayoutPaneSplitNode(split: final split) => LayoutPaneSplitNode(
      LayoutPaneSplit(
        direction: split.direction,
        ratio: split.ratio,
        left: split.left.updatingLeaf(paneID, transform),
        right: split.right.updatingLeaf(paneID, transform),
      ),
    ),
  };

  /// Wraps the target leaf in a 0.5 split with the new leaf on the edge side.
  LayoutPaneNode? splittingLeaf(
    String paneID,
    LayoutPane pane,
    LayoutPaneEdge edge,
  ) {
    final path = pathToPane(paneID);
    final target = path == null ? null : nodeAt(path);
    if (path == null || target == null) return null;
    final split = LayoutPaneSplit(
      direction: edge.splitDirection,
      ratio: 0.5,
      left: edge.newLeafIsLeftChild
          ? LayoutPaneLeaf(pane)
          : target,
      right: edge.newLeafIsLeftChild
          ? target
          : LayoutPaneLeaf(pane),
    );
    return replacingNode(path, LayoutPaneSplitNode(split));
  }

  /// Equalizes every split: ratio = leftWeight / totalWeight, where a child
  /// weighs 1 (leaf or perpendicular split) or the sum of its same-direction
  /// children's weights.
  LayoutPaneNode equalized() => switch (this) {
    LayoutPaneLeaf() => this,
    LayoutPaneSplitNode(split: final split) => () {
      final leftWeight = split.left._weightFor(split.direction);
      final rightWeight = split.right._weightFor(split.direction);
      final ratio = leftWeight / (leftWeight + rightWeight);
      return LayoutPaneSplitNode(
        LayoutPaneSplit(
          direction: split.direction,
          ratio: ratio,
          left: split.left.equalized(),
          right: split.right.equalized(),
        ),
      );
    }(),
  };

  int _weightFor(SplitDirection direction) => switch (this) {
    LayoutPaneLeaf() => 1,
    LayoutPaneSplitNode(split: final split) =>
      split.direction != direction
          ? 1
          : split.left._weightFor(direction) +
                split.right._weightFor(direction),
  };

  /// Structure-only identity: tree shape, directions, leaf ids, and leaf
  /// content — deliberately excluding ratios so divider drags never change
  /// UI identity (retained terminal surfaces must not remount).
  ///
  /// The Swift port hashes with [Hasher] into an Int; this port emits the
  /// same structural projection as a deterministic string, which is stable
  /// across processes and testable.
  String get structuralKey {
    final buffer = StringBuffer();
    _hashStructure(buffer);
    return buffer.toString();
  }

  void _hashStructure(StringBuffer buffer) {
    switch (this) {
      case LayoutPaneLeaf(pane: final pane):
        buffer.write('L${pane.id}:');
        switch (pane.content) {
          case PaneSession(id: final id):
            buffer.write('s$id;');
          case PaneLauncher(projectID: final projectID):
            buffer.write('l$projectID;');
        }
      case LayoutPaneSplitNode(split: final split):
        buffer.write('S${split.direction.name}(');
        split.left._hashStructure(buffer);
        split.right._hashStructure(buffer);
        buffer.write(')');
    }
  }

  /// Grid dimensions: leaf = 1x1; a horizontal split sums widths and maxes
  /// heights; a vertical split sums heights and maxes widths.
  (double width, double height) get gridDimensions => switch (this) {
    LayoutPaneLeaf() => (1.0, 1.0),
    LayoutPaneSplitNode(split: final split) => () {
      final left = split.left.gridDimensions;
      final right = split.right.gridDimensions;
      return switch (split.direction) {
        SplitDirection.horizontal => (
          left.$1 + right.$1,
          left.$2 > right.$2 ? left.$2 : right.$2,
        ),
        SplitDirection.vertical => (
          left.$1 > right.$1 ? left.$1 : right.$1,
          left.$2 + right.$2,
        ),
      };
    }(),
  };

  /// Leaf rects from recursive ratio subdivision of [bounds]; (0,0) is
  /// top-left, y grows down, and a vertical split's left child is on top.
  /// Preorder emission is load-bearing: it is the stable tie-break for
  /// spatial navigation.
  List<({LayoutPane pane, LayoutRect bounds})> leafSlots(LayoutRect bounds) {
    switch (this) {
      case LayoutPaneLeaf(pane: final pane):
        return [(pane: pane, bounds: bounds)];
      case LayoutPaneSplitNode(split: final split):
        final LayoutRect leftBounds;
        final LayoutRect rightBounds;
        switch (split.direction) {
          case SplitDirection.horizontal:
            leftBounds = LayoutRect(
              x: bounds.minX,
              y: bounds.minY,
              width: bounds.width * split.ratio,
              height: bounds.height,
            );
            rightBounds = LayoutRect(
              x: bounds.minX + bounds.width * split.ratio,
              y: bounds.minY,
              width: bounds.width * (1 - split.ratio),
              height: bounds.height,
            );
          case SplitDirection.vertical:
            leftBounds = LayoutRect(
              x: bounds.minX,
              y: bounds.minY,
              width: bounds.width,
              height: bounds.height * split.ratio,
            );
            rightBounds = LayoutRect(
              x: bounds.minX,
              y: bounds.minY + bounds.height * split.ratio,
              width: bounds.width,
              height: bounds.height * (1 - split.ratio),
            );
        }
        return [
          ...split.left.leafSlots(leftBounds),
          ...split.right.leafSlots(rightBounds),
        ];
    }
  }

  /// The neighboring leaf in a direction, using artificial grid-dimension
  /// bounds so the answer is a pure function of the tree. Candidates must
  /// clear the reference's edge; nearest top-left-corner distance wins,
  /// preorder breaks ties.
  LayoutPane? spatialNeighbor(String paneID, LayoutPaneEdge direction) {
    final dimensions = gridDimensions;
    final slots = leafSlots(
      LayoutRect(x: 0, y: 0, width: dimensions.$1, height: dimensions.$2),
    );
    ({LayoutPane pane, LayoutRect bounds})? reference;
    for (final slot in slots) {
      if (slot.pane.id == paneID) {
        reference = slot;
        break;
      }
    }
    if (reference == null) return null;
    final ref = reference.bounds;
    ({LayoutPane pane, double distance})? best;
    for (final slot in slots) {
      if (slot.pane.id == paneID) continue;
      final bounds = slot.bounds;
      final bool qualifies = switch (direction) {
        LayoutPaneEdge.left => bounds.maxX <= ref.minX,
        LayoutPaneEdge.right => bounds.minX >= ref.maxX,
        LayoutPaneEdge.up => bounds.maxY <= ref.minY,
        LayoutPaneEdge.down => bounds.minY >= ref.maxY,
      };
      if (!qualifies) continue;
      final dx = bounds.minX - ref.minX;
      final dy = bounds.minY - ref.minY;
      final distance = sqrt(dx * dx + dy * dy);
      if (best == null || distance < best.distance) {
        best = (pane: slot.pane, distance: distance);
      }
    }
    return best?.pane;
  }
}

/// Leaf node wrapper.
final class LayoutPaneLeaf extends LayoutPaneNode {
  const LayoutPaneLeaf(this.pane);
  final LayoutPane pane;

  @override
  bool operator ==(Object other) =>
      other is LayoutPaneLeaf && other.pane == pane;
  @override
  int get hashCode => pane.hashCode;
}

/// Split node wrapper.
final class LayoutPaneSplitNode extends LayoutPaneNode {
  const LayoutPaneSplitNode(this.split);
  final LayoutPaneSplit split;

  @override
  bool operator ==(Object other) =>
      other is LayoutPaneSplitNode && other.split == split;
  @override
  int get hashCode => split.hashCode;
}

/// A group of panes sharing one split tree (e.g. one workspace window).
final class LayoutPaneGroup {
  LayoutPaneGroup({
    String? id,
    required String representativePaneID,
    required this.root,
    this.preLauncherRoot,
  }) : id = PaneStableID.canonical(id) ?? PaneStableID.make(),
       representativePaneID =
           PaneStableID.canonical(representativePaneID) ?? representativePaneID;

  final String id;
  String representativePaneID;
  LayoutPaneNode root;

  /// The exact tree from before a transient launcher was inserted, restored
  /// on cancel. Presentation-only; never durable.
  LayoutPaneNode? preLauncherRoot;

  /// Leaves in preorder — the sidebar/visual enumeration order.
  List<LayoutPane> get panes => root.leaves;

  List<String> get sessionIDs =>
      root.leaves.map((pane) => pane.content.sessionID).nonNulls.toList();

  String? get representativeSessionID =>
      root.leaf(representativePaneID)?.content.sessionID;

  @override
  bool operator ==(Object other) =>
      other is LayoutPaneGroup &&
      other.id == id &&
      other.representativePaneID == representativePaneID &&
      other.root == root &&
      other.preLauncherRoot == preLauncherRoot;
  @override
  int get hashCode => Object.hash(id, representativePaneID, root);
}

/// Addresses a pane inside a group.
final class LayoutPaneLocation {
  const LayoutPaneLocation({required this.groupID, required this.paneID});

  final String groupID;
  final String paneID;

  @override
  bool operator ==(Object other) =>
      other is LayoutPaneLocation &&
      other.groupID == groupID &&
      other.paneID == paneID;
  @override
  int get hashCode => Object.hash(groupID, paneID);
  @override
  String toString() => 'LayoutPaneLocation($groupID, $paneID)';
}

/// Describes what a detach/close/reconcile removed.
final class LayoutPaneChange {
  const LayoutPaneChange({
    required this.groupID,
    required this.removedPaneIDs,
    required this.releasedSessionIDs,
    required this.representativePaneID,
    required this.dissolved,
  });

  final String groupID;
  final List<String> removedPaneIDs;
  final List<String> releasedSessionIDs;
  final String? representativePaneID;
  final bool dissolved;
}

/// Error kinds for pane-layout operations. Port of `PaneLayoutError`.
enum PaneLayoutErrorKind {
  invalidSessionID,
  sameSession,
  duplicateSession,
  groupNotFound,
  paneNotFound,
  splitNotFound,
  capacityReached,
  launcherAlreadyPresent,
  paneIsNotLauncher,
  panesBelongToDifferentGroups,
  invalidRatio,
}

/// Thrown by [LayoutPaneLayoutState] operations.
final class PaneLayoutException implements Exception {
  const PaneLayoutException(this.kind, [this.detail = '']);

  final PaneLayoutErrorKind kind;
  final String detail;

  @override
  String toString() =>
      'PaneLayoutException(${kind.name}${detail.isEmpty ? '' : ': $detail'})';
}

/// The normative pane-layout state: a list of groups plus the full operation
/// set. Mirrors the Swift struct's mutating methods as instance methods.
final class LayoutPaneLayoutState {
  LayoutPaneLayoutState({List<LayoutPaneGroup> groups = const []}) {
    _canonicalize(groups);
  }

  final List<LayoutPaneGroup> groups = [];

  // MARK: Queries

  LayoutPaneGroup? groupContainingSession(String sessionID) {
    final location = locationOfSession(sessionID);
    if (location == null) return null;
    return group(location.groupID);
  }

  LayoutPaneGroup? group(String groupID) {
    for (final group in groups) {
      if (group.id == groupID) return group;
    }
    return null;
  }

  LayoutPane? pane(String paneID) {
    for (final group in groups) {
      final pane = group.root.leaf(paneID);
      if (pane != null) return pane;
    }
    return null;
  }

  LayoutPaneLocation? locationOfSession(String sessionID) {
    for (final group in groups) {
      for (final pane in group.root.leaves) {
        if (pane.content.sessionID == sessionID) {
          return LayoutPaneLocation(groupID: group.id, paneID: pane.id);
        }
      }
    }
    return null;
  }

  LayoutPaneLocation? locationOfPane(String paneID) {
    for (final group in groups) {
      if (group.root.leaf(paneID) != null) {
        return LayoutPaneLocation(groupID: group.id, paneID: paneID);
      }
    }
    return null;
  }

  LayoutPane? spatialNeighbor(String paneID, LayoutPaneEdge direction) {
    final location = locationOfPane(paneID);
    final group = location == null ? null : this.group(location.groupID);
    if (group == null) return null;
    return group.root.spatialNeighbor(paneID, direction);
  }

  // MARK: Insert

  LayoutPaneLocation createGroup({
    required String representativeSessionID,
    required String addingSessionID,
    required LayoutPaneEdge edge,
    String? newGroupID,
    String? newRepresentativePaneID,
    String? newPaneID,
  }) {
    _validateNewSession(addingSessionID);
    if (representativeSessionID.isEmpty) {
      throw const PaneLayoutException(PaneLayoutErrorKind.invalidSessionID);
    }
    if (representativeSessionID == addingSessionID) {
      throw const PaneLayoutException(PaneLayoutErrorKind.sameSession);
    }
    if (locationOfSession(representativeSessionID) != null) {
      throw PaneLayoutException(
        PaneLayoutErrorKind.duplicateSession,
        representativeSessionID,
      );
    }

    final representative = LayoutPane(
      id: newRepresentativePaneID,
      content: PaneSession(representativeSessionID),
    );
    final inserted = LayoutPane(
      id: newPaneID,
      content: PaneSession(addingSessionID),
    );
    final split = LayoutPaneSplit(
      direction: edge.splitDirection,
      ratio: 0.5,
      left: edge.newLeafIsLeftChild
          ? LayoutPaneLeaf(inserted)
          : LayoutPaneLeaf(representative),
      right: edge.newLeafIsLeftChild
          ? LayoutPaneLeaf(representative)
          : LayoutPaneLeaf(inserted),
    );
    final group = LayoutPaneGroup(
      id: newGroupID,
      representativePaneID: representative.id,
      root: LayoutPaneSplitNode(split),
    );
    groups.add(group);
    return LayoutPaneLocation(groupID: group.id, paneID: inserted.id);
  }

  /// Splits the target session's leaf, or creates a group when that session
  /// is currently solo.
  LayoutPaneLocation insertSession({
    required String sessionID,
    required String besideSessionID,
    required LayoutPaneEdge edge,
    String? newGroupID,
    String? newRepresentativePaneID,
    String? newPaneID,
  }) {
    _validateNewSession(sessionID);
    if (besideSessionID.isEmpty) {
      throw const PaneLayoutException(PaneLayoutErrorKind.invalidSessionID);
    }
    if (sessionID == besideSessionID) {
      throw const PaneLayoutException(PaneLayoutErrorKind.sameSession);
    }

    final target = locationOfSession(besideSessionID);
    if (target != null) {
      return insertSessionSplitting(
        sessionID: sessionID,
        targetPaneID: target.paneID,
        edge: edge,
        newPaneID: newPaneID,
      );
    }
    return createGroup(
      representativeSessionID: besideSessionID,
      addingSessionID: sessionID,
      edge: edge,
      newGroupID: newGroupID,
      newRepresentativePaneID: newRepresentativePaneID,
      newPaneID: newPaneID,
    );
  }

  LayoutPaneLocation insertSessionSplitting({
    required String sessionID,
    required String targetPaneID,
    required LayoutPaneEdge edge,
    String? newPaneID,
  }) {
    _validateNewSession(sessionID);
    final location = locationOfPane(targetPaneID);
    if (location == null) {
      throw PaneLayoutException(
        PaneLayoutErrorKind.paneNotFound,
        targetPaneID,
      );
    }
    final groupIndex = _indexOfGroup(location.groupID);
    _validateCapacity(groupIndex);

    final pane = LayoutPane(
      id: newPaneID,
      content: PaneSession(sessionID),
    );
    final root = groups[groupIndex].root.splittingLeaf(
      targetPaneID,
      pane,
      edge,
    );
    if (root == null) {
      throw PaneLayoutException(
        PaneLayoutErrorKind.paneNotFound,
        targetPaneID,
      );
    }
    // Adding a real session while a launcher is open is an intentional
    // layout mutation; its geometry becomes the new basis.
    groups[groupIndex].preLauncherRoot = null;
    groups[groupIndex].root = root;
    return LayoutPaneLocation(groupID: groups[groupIndex].id, paneID: pane.id);
  }

  /// Splits the group's root; the new leaf's share is 1/(sessionLeafCount+1).
  LayoutPaneLocation insertSessionAtGroupEdge({
    required String sessionID,
    required LayoutPaneEdge edge,
    required String groupID,
    String? newPaneID,
  }) {
    _validateNewSession(sessionID);
    final groupIndex = _indexOfGroup(groupID);
    _validateCapacity(groupIndex);

    final pane = LayoutPane(
      id: newPaneID,
      content: PaneSession(sessionID),
    );
    final share =
        1.0 / (groups[groupIndex].root.sessionLeaves.length + 1);
    final split = LayoutPaneSplit(
      direction: edge.splitDirection,
      ratio: edge.newLeafIsLeftChild ? share : 1.0 - share,
      left: edge.newLeafIsLeftChild
          ? LayoutPaneLeaf(pane)
          : groups[groupIndex].root,
      right: edge.newLeafIsLeftChild
          ? groups[groupIndex].root
          : LayoutPaneLeaf(pane),
    );
    groups[groupIndex].preLauncherRoot = null;
    groups[groupIndex].root = LayoutPaneSplitNode(split);
    return LayoutPaneLocation(groupID: groups[groupIndex].id, paneID: pane.id);
  }

  // MARK: Launcher lifecycle

  LayoutPaneLocation insertLauncher({
    required String projectID,
    required String splittingPaneID,
    required LayoutPaneEdge edge,
    String? newPaneID,
  }) {
    final location = locationOfPane(splittingPaneID);
    if (location == null) {
      throw PaneLayoutException(
        PaneLayoutErrorKind.paneNotFound,
        splittingPaneID,
      );
    }
    final groupIndex = _indexOfGroup(location.groupID);
    _validateCapacity(groupIndex);
    if (groups[groupIndex].root.containsLauncher) {
      throw PaneLayoutException(
        PaneLayoutErrorKind.launcherAlreadyPresent,
        location.groupID,
      );
    }

    final launcher = LayoutPane(
      id: newPaneID,
      content: PaneLauncher(projectID),
    );
    final snapshot = groups[groupIndex].root;
    final root = snapshot.splittingLeaf(splittingPaneID, launcher, edge);
    if (root == null) {
      throw PaneLayoutException(
        PaneLayoutErrorKind.paneNotFound,
        splittingPaneID,
      );
    }
    groups[groupIndex].preLauncherRoot = snapshot;
    groups[groupIndex].root = root;
    return LayoutPaneLocation(
      groupID: groups[groupIndex].id,
      paneID: launcher.id,
    );
  }

  /// Splits the target session's leaf, or creates a session + launcher group
  /// when that session is currently solo.
  LayoutPaneLocation insertLauncherBeside({
    required String projectID,
    required String targetSessionID,
    required LayoutPaneEdge edge,
    String? newGroupID,
    String? newRepresentativePaneID,
    String? newPaneID,
  }) {
    if (targetSessionID.isEmpty) {
      throw const PaneLayoutException(PaneLayoutErrorKind.invalidSessionID);
    }

    final target = locationOfSession(targetSessionID);
    if (target != null) {
      return insertLauncher(
        projectID: projectID,
        splittingPaneID: target.paneID,
        edge: edge,
        newPaneID: newPaneID,
      );
    }

    final representative = LayoutPane(
      id: newRepresentativePaneID,
      content: PaneSession(targetSessionID),
    );
    final launcher = LayoutPane(
      id: newPaneID,
      content: PaneLauncher(projectID),
    );
    final split = LayoutPaneSplit(
      direction: edge.splitDirection,
      ratio: 0.5,
      left: edge.newLeafIsLeftChild
          ? LayoutPaneLeaf(launcher)
          : LayoutPaneLeaf(representative),
      right: edge.newLeafIsLeftChild
          ? LayoutPaneLeaf(representative)
          : LayoutPaneLeaf(launcher),
    );
    final group = LayoutPaneGroup(
      id: newGroupID,
      representativePaneID: representative.id,
      root: LayoutPaneSplitNode(split),
    );
    groups.add(group);
    return LayoutPaneLocation(groupID: group.id, paneID: launcher.id);
  }

  void bindLauncher(String paneID, String sessionID) {
    _validateNewSession(sessionID);
    final location = locationOfPane(paneID);
    if (location == null) {
      throw PaneLayoutException(PaneLayoutErrorKind.paneNotFound, paneID);
    }
    final groupIndex = _indexOfGroup(location.groupID);
    if (groups[groupIndex].root.leaf(paneID)?.content.isLauncher != true) {
      throw PaneLayoutException(
        PaneLayoutErrorKind.paneIsNotLauncher,
        paneID,
      );
    }
    groups[groupIndex].root = groups[groupIndex].root.updatingLeaf(
      paneID,
      (pane) => pane.content = PaneSession(sessionID),
    );
    groups[groupIndex].preLauncherRoot = null;
  }

  LayoutPaneChange removeLauncher(String paneID) {
    final location = locationOfPane(paneID);
    if (location == null) {
      throw PaneLayoutException(PaneLayoutErrorKind.paneNotFound, paneID);
    }
    final groupIndex = _indexOfGroup(location.groupID);
    if (groups[groupIndex].root.leaf(paneID)?.content.isLauncher != true) {
      throw PaneLayoutException(
        PaneLayoutErrorKind.paneIsNotLauncher,
        paneID,
      );
    }
    return detachPane(paneID);
  }

  // MARK: Detach / close

  LayoutPaneChange detachPane(String paneID) {
    final location = locationOfPane(paneID);
    if (location == null) {
      throw PaneLayoutException(PaneLayoutErrorKind.paneNotFound, paneID);
    }
    final groupIndex = _indexOfGroup(location.groupID);
    final original = groups[groupIndex];
    final removedPane = original.root.leaf(paneID);
    if (removedPane == null) {
      throw PaneLayoutException(PaneLayoutErrorKind.paneNotFound, paneID);
    }

    final newRoot = original.root.removingLeaf(paneID);
    if (newRoot == null) {
      groups.removeAt(groupIndex);
      return LayoutPaneChange(
        groupID: original.id,
        removedPaneIDs: original.panes.map((pane) => pane.id).toList(),
        releasedSessionIDs: original.sessionIDs,
        representativePaneID: null,
        dissolved: true,
      );
    }

    final sessionLeaves = newRoot.sessionLeaves;
    final canRemainGrouped =
        sessionLeaves.length >= 2 ||
        (sessionLeaves.length == 1 && newRoot.containsLauncher);
    if (!canRemainGrouped) {
      groups.removeAt(groupIndex);
      return LayoutPaneChange(
        groupID: original.id,
        removedPaneIDs: original.panes.map((pane) => pane.id).toList(),
        releasedSessionIDs: original.sessionIDs,
        representativePaneID: null,
        dissolved: true,
      );
    }

    final updated = LayoutPaneGroup(
      id: original.id,
      representativePaneID: original.representativePaneID,
      root: original.root,
      preLauncherRoot: original.preLauncherRoot,
    );
    if (removedPane.content.isLauncher) {
      // Cancel restores the snapshot only when it still describes
      // exactly the surviving session leaves.
      final snapshot = updated.preLauncherRoot;
      if (snapshot != null &&
          _idSetsEqual(
            _leafIDSet(snapshot.sessionLeaves),
            _leafIDSet(sessionLeaves),
          )) {
        updated.root = snapshot;
      } else {
        updated.root = newRoot;
      }
      updated.preLauncherRoot = null;
    } else {
      updated.root = newRoot;
      final snapshot = updated.preLauncherRoot;
      if (snapshot != null) {
        final pruned = snapshot.removingLeaf(paneID);
        updated.preLauncherRoot =
            (pruned?.sessionLeaves.length ?? 0) >= 2 ? pruned : null;
      }
    }
    if (updated.root.leaf(updated.representativePaneID)?.content.sessionID ==
        null) {
      updated.representativePaneID = updated.root.sessionLeaves[0].id;
    }
    groups[groupIndex] = updated;
    return LayoutPaneChange(
      groupID: original.id,
      removedPaneIDs: [paneID],
      releasedSessionIDs: [
        if (removedPane.content.sessionID case final sessionID?)
          sessionID,
      ],
      representativePaneID: updated.representativePaneID,
      dissolved: false,
    );
  }

  LayoutPaneChange closeGroup(String groupID) {
    final groupIndex = _indexOfGroup(groupID);
    final group = groups.removeAt(groupIndex);
    return LayoutPaneChange(
      groupID: group.id,
      removedPaneIDs: group.panes.map((pane) => pane.id).toList(),
      releasedSessionIDs: group.sessionIDs,
      representativePaneID: null,
      dissolved: true,
    );
  }

  // MARK: Geometry

  /// Returns the applied (clamped) ratio.
  double resizeSplit(
    String groupID,
    LayoutPaneSplitPath path,
    double ratio,
  ) {
    if (!ratio.isFinite) {
      throw const PaneLayoutException(PaneLayoutErrorKind.invalidRatio);
    }
    final groupIndex = _indexOfGroup(groupID);
    final node = groups[groupIndex].root.nodeAt(path);
    if (node is! LayoutPaneSplitNode) {
      throw PaneLayoutException(PaneLayoutErrorKind.splitNotFound, groupID);
    }
    final applied = LayoutPaneSplit.clampedRatio(ratio);
    if (node.split.ratio == applied) return applied;
    node.split.ratio = applied;
    // Resizing while a launcher is visible is explicit user intent, so a
    // later cancel must not resurrect the pre-resize geometry.
    groups[groupIndex].preLauncherRoot = null;
    return applied;
  }

  void equalize(String groupID) {
    final groupIndex = _indexOfGroup(groupID);
    groups[groupIndex].preLauncherRoot = null;
    groups[groupIndex].root = groups[groupIndex].root.equalized();
  }

  /// Exchanges the positions of two leaves in the same group. Pane ids
  /// travel with their leaves, so the representative id is unaffected.
  bool swapPanes(String paneID, String otherPaneID) {
    final source = locationOfPane(paneID);
    if (source == null) {
      throw PaneLayoutException(PaneLayoutErrorKind.paneNotFound, paneID);
    }
    final target = locationOfPane(otherPaneID);
    if (target == null) {
      throw PaneLayoutException(
        PaneLayoutErrorKind.paneNotFound,
        otherPaneID,
      );
    }
    if (source.groupID != target.groupID) {
      throw const PaneLayoutException(
        PaneLayoutErrorKind.panesBelongToDifferentGroups,
      );
    }
    if (paneID == otherPaneID) return false;

    final groupIndex = _indexOfGroup(source.groupID);
    final root = groups[groupIndex].root;
    final first = root.leaf(paneID);
    final second = root.leaf(otherPaneID);
    final firstPath = root.pathToPane(paneID);
    final secondPath = root.pathToPane(otherPaneID);
    if (first == null || second == null || firstPath == null || secondPath == null) {
      throw PaneLayoutException(PaneLayoutErrorKind.paneNotFound, paneID);
    }
    // Replace by position, not by id: paths stay valid because only leaf
    // payloads change, never the tree shape.
    groups[groupIndex].preLauncherRoot = null;
    groups[groupIndex].root = root
        .replacingNode(firstPath, LayoutPaneLeaf(second))
        .replacingNode(secondPath, LayoutPaneLeaf(first));
    return true;
  }

  // MARK: Reconcile

  /// Drops sessions no longer eligible, collapses around them, promotes the
  /// first remaining session leaf when the representative disappears, and
  /// dissolves non-transient groups with fewer than two sessions. A session
  /// + launcher pair remains while its launcher interaction is active.
  List<LayoutPaneChange> reconcile(Set<String> eligibleSessionIDs) {
    final reconciled = <LayoutPaneGroup>[];
    final changes = <LayoutPaneChange>[];

    for (final original in groups) {
      final ineligible = original.root.sessionLeaves.where((pane) {
        final sessionID = pane.content.sessionID;
        return sessionID != null && !eligibleSessionIDs.contains(sessionID);
      }).toList();
      LayoutPaneNode? newRoot = original.root;
      for (final pane in ineligible) {
        newRoot = newRoot?.removingLeaf(pane.id);
      }

      final sessionLeaves = newRoot?.sessionLeaves ?? [];
      final hasLauncher = newRoot?.containsLauncher ?? false;
      final canRemainGrouped =
          sessionLeaves.length >= 2 ||
          (sessionLeaves.length == 1 && hasLauncher);

      if (newRoot == null || !canRemainGrouped) {
        changes.add(
          LayoutPaneChange(
            groupID: original.id,
            removedPaneIDs: original.panes.map((pane) => pane.id).toList(),
            releasedSessionIDs: original.sessionIDs,
            representativePaneID: null,
            dissolved: true,
          ),
        );
        continue;
      }

      final updated = LayoutPaneGroup(
        id: original.id,
        representativePaneID: original.representativePaneID,
        root: newRoot,
        preLauncherRoot: original.preLauncherRoot,
      );
      if (updated.root.leaf(updated.representativePaneID)?.content.sessionID ==
          null) {
        updated.representativePaneID = sessionLeaves[0].id;
      }
      final snapshot = updated.preLauncherRoot;
      if (snapshot != null && hasLauncher) {
        LayoutPaneNode? pruned = snapshot;
        for (final pane in ineligible) {
          pruned = pruned?.removingLeaf(pane.id);
        }
        final prunedLeafIDs = _leafIDSet(pruned?.sessionLeaves ?? []);
        final liveLeafIDs = _leafIDSet(sessionLeaves);
        updated.preLauncherRoot =
            (pruned?.sessionLeaves.length ?? 0) >= 2 &&
                _idSetsEqual(prunedLeafIDs, liveLeafIDs)
            ? pruned
            : null;
      } else {
        updated.preLauncherRoot = null;
      }
      reconciled.add(updated);

      if (updated != original) {
        final retainedPaneIDs = updated.panes.map((pane) => pane.id).toSet();
        changes.add(
          LayoutPaneChange(
            groupID: original.id,
            removedPaneIDs: original.panes
                .map((pane) => pane.id)
                .where((id) => !retainedPaneIDs.contains(id))
                .toList(),
            releasedSessionIDs: original.sessionIDs
                .where((id) => !eligibleSessionIDs.contains(id))
                .toList(),
            representativePaneID: updated.representativePaneID,
            dissolved: false,
          ),
        );
      }
    }

    groups
      ..clear()
      ..addAll(reconciled);
    return changes;
  }

  // MARK: Canonicalization

  void _canonicalize(List<LayoutPaneGroup> candidates) {
    final usedGroupIDs = <String>{};
    var usedPaneIDs = <String>{};
    var usedSessionIDs = <String>{};

    for (final candidate in candidates) {
      var groupID = candidate.id;
      if (usedGroupIDs.contains(groupID)) {
        groupID = PaneStableID.make();
      }

      // Drop invalid leaves: empty/duplicate sessions, duplicate pane
      // ids, and any launcher after the first.
      var candidatePaneIDs = Set<String>.from(usedPaneIDs);
      var candidateSessionIDs = Set<String>.from(usedSessionIDs);
      var hasLauncher = false;
      LayoutPaneNode? root = candidate.root;
      for (final pane in candidate.root.leaves) {
        var keep = !candidatePaneIDs.contains(pane.id);
        if (keep) {
          switch (pane.content) {
            case PaneSession(id: final sessionID):
              keep = sessionID.isNotEmpty &&
                  !candidateSessionIDs.contains(sessionID);
              if (keep) candidateSessionIDs.add(sessionID);
            case PaneLauncher():
              keep = !hasLauncher;
              hasLauncher = hasLauncher || keep;
          }
        }
        if (keep) {
          candidatePaneIDs.add(pane.id);
        } else {
          root = root?.removingLeaf(pane.id);
        }
      }

      // Enforce the session-leaf cap by trimming trailing preorder leaves.
      var current = root;
      while (current != null &&
          current.sessionLeaves.length > layoutMaxSessionLeaves) {
        final last = current.sessionLeaves.last;
        candidatePaneIDs.remove(last.id);
        final sessionID = last.content.sessionID;
        if (sessionID != null) candidateSessionIDs.remove(sessionID);
        current = current.removingLeaf(last.id);
      }
      root = current;

      if (root == null) continue;
      final sessionLeaves = root.sessionLeaves;
      final canRemainGrouped =
          sessionLeaves.length >= 2 ||
          (sessionLeaves.length == 1 && hasLauncher);
      if (!canRemainGrouped) continue;

      usedGroupIDs.add(groupID);
      usedPaneIDs = candidatePaneIDs;
      usedSessionIDs = candidateSessionIDs;

      final representative =
          root.leaf(candidate.representativePaneID)?.content.sessionID != null
          ? candidate.representativePaneID
          : sessionLeaves[0].id;
      LayoutPaneNode? snapshot;
      final candidateSnapshot = candidate.preLauncherRoot;
      if (hasLauncher && candidateSnapshot != null) {
        final snapshotIDs = _leafIDSet(candidateSnapshot.sessionLeaves);
        final liveIDs = _leafIDSet(sessionLeaves);
        snapshot = _idSetsEqual(snapshotIDs, liveIDs) && snapshotIDs.length >= 2
            ? candidateSnapshot
            : null;
      }
      groups.add(
        LayoutPaneGroup(
          id: groupID,
          representativePaneID: representative,
          root: root,
          preLauncherRoot: snapshot,
        ),
      );
    }
  }

  // MARK: Helpers

  void _validateNewSession(String sessionID) {
    if (sessionID.isEmpty) {
      throw const PaneLayoutException(PaneLayoutErrorKind.invalidSessionID);
    }
    if (locationOfSession(sessionID) != null) {
      throw PaneLayoutException(
        PaneLayoutErrorKind.duplicateSession,
        sessionID,
      );
    }
  }

  void _validateCapacity(int groupIndex) {
    // A live launcher counts prospectively so a later bind cannot push the
    // group past the cap.
    final prospective = groups[groupIndex].root.sessionLeaves.length +
        (groups[groupIndex].root.containsLauncher ? 1 : 0);
    if (prospective >= layoutMaxSessionLeaves) {
      throw PaneLayoutException(
        PaneLayoutErrorKind.capacityReached,
        groups[groupIndex].id,
      );
    }
  }

  int _indexOfGroup(String groupID) {
    final index = groups.indexWhere((group) => group.id == groupID);
    if (index < 0) {
      throw PaneLayoutException(PaneLayoutErrorKind.groupNotFound, groupID);
    }
    return index;
  }

  static Set<String> _leafIDSet(List<LayoutPane> panes) =>
      panes.map((pane) => pane.id).toSet();

  static bool _idSetsEqual(Set<String> a, Set<String> b) =>
      a.length == b.length && a.containsAll(b);

  @override
  bool operator ==(Object other) =>
      other is LayoutPaneLayoutState &&
      _groupsEqual(groups, other.groups);

  static bool _groupsEqual(
    List<LayoutPaneGroup> a,
    List<LayoutPaneGroup> b,
  ) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  @override
  int get hashCode => Object.hashAll(groups);
}

/// Stable UUID ids. Port of `PaneStableID`.
abstract final class PaneStableID {
  static final Random _random = Random.secure();

  /// Random v4 UUID, lowercased.
  static String make() {
    final bytes = List<int>.generate(16, (_) => _random.nextInt(256));
    // Version 4 + RFC 4122 variant bits.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    final hex = bytes
        .map((b) => b.toRadixString(16).padLeft(2, '0'))
        .join();
    return '${hex.substring(0, 8)}-${hex.substring(8, 12)}-'
        '${hex.substring(12, 16)}-${hex.substring(16, 20)}-'
        '${hex.substring(20)}';
  }

  static final RegExp _uuidPattern = RegExp(
    r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-'
    r'[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$',
  );

  /// Lowercased canonical form when [value] parses as a UUID, else null.
  static String? canonical(String? value) {
    if (value == null || !_uuidPattern.hasMatch(value)) return null;
    return value.toLowerCase();
  }
}

/// Additive projection of the project sidebar (the right panel): which
/// sessions it shows and the main-area session it is displayed beside.
/// Arrangement only — never focus, never geometry.
final class DurableSidebarProjection {
  const DurableSidebarProjection({required this.sessionIDs, this.besideSessionID});

  final List<String> sessionIDs;
  final String? besideSessionID;

  Map<String, Object?> toJson() => {
    'sessionIDs': sessionIDs,
    if (besideSessionID != null) 'besideSessionID': besideSessionID,
  };

  factory DurableSidebarProjection.fromJson(Map<String, Object?> json) {
    return DurableSidebarProjection(
      sessionIDs:
          ((json['sessionIDs'] as List?) ?? []).map((e) => e as String).toList(),
      besideSessionID: json['besideSessionID'] as String?,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is DurableSidebarProjection &&
      _listEquals(other.sessionIDs, sessionIDs) &&
      other.besideSessionID == besideSessionID;
  @override
  int get hashCode => Object.hash(Object.hashAll(sessionIDs), besideSessionID);
}

/// The Codable projection of a layout. Launchers and groups that do not
/// contain at least two sessions are omitted; while a launcher is live and a
/// pre-launcher snapshot exists, the snapshot is what gets encoded. Writes
/// are always version 2; version 1 (the flat pane list) migrates on read.
final class DurablePaneLayout {
  const DurablePaneLayout({
    required this.version,
    required this.groups,
    this.sidebar,
  });

  static const int currentVersion = 2;

  /// Versions this codec can read.
  static bool supportsVersion(int version) => version == 1 || version == 2;

  final int version;
  final List<DurablePaneGroup> groups;
  final DurableSidebarProjection? sidebar;

  factory DurablePaneLayout.fromState(
    LayoutPaneLayoutState state, {
    DurableSidebarProjection? sidebar,
  }) {
    return DurablePaneLayout(
      version: currentVersion,
      groups: state.groups
          .map(DurablePaneGroup.fromGroup)
          .nonNulls
          .toList(),
      sidebar: sidebar,
    );
  }

  LayoutPaneLayoutState restoredState() {
    return LayoutPaneLayoutState(
      groups: groups
          .map(
            (durable) => LayoutPaneGroup(
              id: durable.id,
              representativePaneID: durable.representativePaneID,
              root: durable.root.paneNode(),
            ),
          )
          .toList(),
    );
  }

  factory DurablePaneLayout.fromJson(Map<String, Object?> json) {
    final version = (json['version'] as num?)?.toInt() ?? 0;
    final sidebarJson = json['sidebar'] as Map<String, Object?>?;
    final sidebar = sidebarJson == null
        ? null
        : DurableSidebarProjection.fromJson(sidebarJson);
    switch (version) {
      case 2:
        final groupsJson = (json['groups'] as List?) ?? [];
        return DurablePaneLayout(
          version: version,
          groups: groupsJson
              .map(
                (e) => DurablePaneGroup.fromJson(e as Map<String, Object?>),
              )
              .toList(),
          sidebar: sidebar,
        );
      case 1:
        final groupsJson = (json['groups'] as List?) ?? [];
        return DurablePaneLayout(
          version: version,
          groups: groupsJson
              .map(
                (e) => _LegacyDurablePaneGroup.fromJson(
                  e as Map<String, Object?>,
                ).migrated(),
              )
              .nonNulls
              .toList(),
          sidebar: sidebar,
        );
      default:
        // Unknown future version: capture the version so writers can fail
        // closed; the content is not preserved.
        return DurablePaneLayout(version: version, groups: [], sidebar: sidebar);
    }
  }

  Map<String, Object?> toJson() => {
    'version': currentVersion,
    'groups': groups.map((group) => group.toJson()).toList(),
    if (sidebar != null) 'sidebar': sidebar!.toJson(),
  };
}

final class DurablePaneGroup {
  const DurablePaneGroup({
    required this.id,
    required this.representativePaneID,
    required this.root,
  });

  final String id;
  final String representativePaneID;
  final DurablePaneNode root;

  /// Null when the group must not persist (fewer than two session leaves).
  static DurablePaneGroup? fromGroup(LayoutPaneGroup group) {
    // Persist the pre-launcher snapshot while a launcher interaction is
    // live; otherwise strip launcher leaves via the removal algorithm.
    LayoutPaneNode? candidate;
    if (group.root.containsLauncher && group.preLauncherRoot != null) {
      candidate = group.preLauncherRoot;
    } else {
      candidate = group.root;
      for (final pane in group.root.leaves) {
        if (pane.content.isLauncher) {
          candidate = candidate?.removingLeaf(pane.id);
        }
      }
    }
    if (candidate == null || candidate.sessionLeaves.length < 2) return null;
    final representative =
        candidate.leaf(group.representativePaneID) != null
        ? group.representativePaneID
        : candidate.sessionLeaves[0].id;
    return DurablePaneGroup(
      id: group.id,
      representativePaneID: representative,
      root: DurablePaneNode.fromNode(candidate),
    );
  }

  factory DurablePaneGroup.fromJson(Map<String, Object?> json) {
    return DurablePaneGroup(
      id: json['id'] as String,
      representativePaneID: json['representativePaneID'] as String,
      root: DurablePaneNode.fromJson(json['root'] as Map<String, Object?>),
    );
  }

  Map<String, Object?> toJson() => {
    'id': id,
    'representativePaneID': representativePaneID,
    'root': root.toJson(),
  };

  @override
  bool operator ==(Object other) =>
      other is DurablePaneGroup &&
      other.id == id &&
      other.representativePaneID == representativePaneID &&
      other.root == root;
  @override
  int get hashCode => Object.hash(id, representativePaneID, root);
}

/// Durable tree node. JSON envelope matches the Swift codec:
/// `{"pane": {"id", "sessionID"}}` or
/// `{"split": {"direction", "ratio", "left", "right"}}`.
sealed class DurablePaneNode {
  const DurablePaneNode();

  factory DurablePaneNode.fromNode(LayoutPaneNode node) => switch (node) {
    LayoutPaneLeaf(pane: final pane) => DurablePaneLeaf(
      pane.id,
      pane.content.sessionID ?? '',
    ),
    LayoutPaneSplitNode(split: final split) => DurablePaneSplit(
      split.direction,
      split.ratio,
      DurablePaneNode.fromNode(split.left),
      DurablePaneNode.fromNode(split.right),
    ),
  };

  LayoutPaneNode paneNode();

  factory DurablePaneNode.fromJson(Map<String, Object?> json) {
    final paneJson = json['pane'] as Map<String, Object?>?;
    if (paneJson != null) {
      return DurablePaneLeaf(
        paneJson['id'] as String,
        paneJson['sessionID'] as String,
      );
    }
    final splitJson = json['split'] as Map<String, Object?>?;
    if (splitJson == null) {
      throw FormatException('Unknown pane node envelope: $json');
    }
    final direction = SplitDirection.values.firstWhere(
      (d) => d.name == splitJson['direction'],
      orElse: () => throw FormatException(
        'Unknown split direction ${splitJson['direction']}',
      ),
    );
    return DurablePaneSplit(
      direction,
      (splitJson['ratio'] as num).toDouble(),
      DurablePaneNode.fromJson(splitJson['left'] as Map<String, Object?>),
      DurablePaneNode.fromJson(splitJson['right'] as Map<String, Object?>),
    );
  }

  Map<String, Object?> toJson();
}

final class DurablePaneLeaf extends DurablePaneNode {
  const DurablePaneLeaf(this.id, this.sessionID);

  final String id;
  final String sessionID;

  @override
  LayoutPaneNode paneNode() =>
      LayoutPaneLeaf(LayoutPane(id: id, content: PaneSession(sessionID)));

  @override
  Map<String, Object?> toJson() => {
    'pane': {'id': id, 'sessionID': sessionID},
  };

  @override
  bool operator ==(Object other) =>
      other is DurablePaneLeaf &&
      other.id == id &&
      other.sessionID == sessionID;
  @override
  int get hashCode => Object.hash(id, sessionID);
}

final class DurablePaneSplit extends DurablePaneNode {
  const DurablePaneSplit(this.direction, this.ratio, this.left, this.right);

  final SplitDirection direction;
  final double ratio;
  final DurablePaneNode left;
  final DurablePaneNode right;

  @override
  LayoutPaneNode paneNode() => LayoutPaneSplitNode(
    LayoutPaneSplit(
      direction: direction,
      ratio: ratio,
      left: left.paneNode(),
      right: right.paneNode(),
    ),
  );

  @override
  Map<String, Object?> toJson() => {
    'split': {
      'direction': direction.name,
      'ratio': ratio,
      'left': left.toJson(),
      'right': right.toJson(),
    },
  };

  @override
  bool operator ==(Object other) =>
      other is DurablePaneSplit &&
      other.direction == direction &&
      other.ratio == ratio &&
      other.left == left &&
      other.right == right;
  @override
  int get hashCode => Object.hash(direction, ratio, left, right);
}

/// The version-1 flat shape, decoded only for migration. Accepts both the
/// Swift (`sessionID`/`representativePaneID`) and legacy Rust
/// (`sessionId`/`representativePaneId`) key spellings.
final class _LegacyDurablePaneGroup {
  const _LegacyDurablePaneGroup({
    required this.id,
    required this.representativePaneID,
    required this.panes,
  });

  final String id;
  final String representativePaneID;
  final List<_LegacyDurablePane> panes;

  factory _LegacyDurablePaneGroup.fromJson(Map<String, Object?> json) {
    final representativePaneID =
        json['representativePaneID'] as String? ??
        json['representativePaneId'] as String;
    final panesJson = (json['panes'] as List?) ?? [];
    return _LegacyDurablePaneGroup(
      id: json['id'] as String,
      representativePaneID: representativePaneID,
      panes: panesJson
          .map((e) => _LegacyDurablePane.fromJson(e as Map<String, Object?>))
          .toList(),
    );
  }

  /// Folds the flat pane list into a right-leaning horizontal chain:
  /// node(i) = split(horizontal, clamp(f_i / (f_i + … + f_n)), leaf(p_i),
  /// node(i+1)). Divide, then clamp.
  DurablePaneGroup? migrated() {
    if (panes.length < 2) return null;

    DurablePaneNode fold(int index) {
      final pane = panes[index];
      final leaf = DurablePaneLeaf(pane.id, pane.sessionID);
      if (index >= panes.length - 1) return leaf;
      final remaining = panes
          .sublist(index)
          .fold<double>(0, (sum, p) => sum + _nonNegativeFinite(p.fraction));
      final fraction = _nonNegativeFinite(pane.fraction);
      final ratio = remaining > 0
          ? LayoutPaneSplit.clampedRatio(fraction / remaining)
          : 0.5;
      return DurablePaneSplit(
        SplitDirection.horizontal,
        ratio,
        leaf,
        fold(index + 1),
      );
    }

    return DurablePaneGroup(
      id: id,
      representativePaneID: representativePaneID,
      root: fold(0),
    );
  }

  static double _nonNegativeFinite(double value) =>
      value.isFinite ? (value > 0 ? value : 0) : 0;
}

final class _LegacyDurablePane {
  const _LegacyDurablePane({
    required this.id,
    required this.sessionID,
    required this.fraction,
  });

  final String id;
  final String sessionID;
  final double fraction;

  factory _LegacyDurablePane.fromJson(Map<String, Object?> json) {
    final sessionID =
        json['sessionID'] as String? ?? json['sessionId'] as String;
    return _LegacyDurablePane(
      id: json['id'] as String,
      sessionID: sessionID,
      fraction: (json['fraction'] as num).toDouble(),
    );
  }
}

bool _listEquals<T>(List<T> a, List<T> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}
