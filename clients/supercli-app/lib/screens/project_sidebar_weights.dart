/// Port of the height-weight math in `ProjectSidebarView.swift`
/// (`clients/legacy/native/SupercliNative/Sources/SupercliNative/Views/`).
///
/// The right-side project panel stacks terminal panes with draggable
/// dividers. Each session carries a relative height weight (missing = 1);
/// weights persist so the stack's proportions survive restarts, live drags
/// stay in memory and write through on release, and stale ids are pruned
/// when the membership changes.
///
/// This file ports the pure math + persistence codec. The SwiftUI divider
/// drag gesture itself needs framework pointer APIs (see
/// docs/gpuidart-gaps-sidebar.md G-1).
library;

/// Persistence key, matching Swift `ProjectSidebarView.weightsKey`.
const projectSidebarWeightsKey = 'supercli.projectSidebar.weights';

/// Minimum persisted weight: Swift `loadWeights` clamps every value to >= 0.05.
const projectSidebarMinWeight = 0.05;

/// Decode persisted weights, clamping each to the Swift minimum.
Map<String, double> decodeProjectSidebarWeights(Map<String, Object?> raw) {
  final out = <String, double>{};
  for (final entry in raw.entries) {
    final v = entry.value;
    final d = v is num ? v.toDouble() : null;
    if (d != null) out[entry.key] = d < projectSidebarMinWeight ? projectSidebarMinWeight : d;
  }
  return out;
}

/// Sum of weights (missing id = 1). Swift `totalWeight`.
double totalWeight(Iterable<String> sessionIds, Map<String, double> weights) =>
    sessionIds.fold(0.0, (sum, id) => sum + (weights[id] ?? 1.0));

/// Resolve each session's pixel height from the available space.
/// Swift `resolvedHeights(sessions:available:extraWeight:)`.
Map<String, double> resolvedHeights({
  required List<String> sessionIds,
  required double available,
  required Map<String, double> weights,
  double extraWeight = 0.0,
}) {
  if (sessionIds.isEmpty) return const {};
  final sum = totalWeight(sessionIds, weights) + extraWeight;
  final safe = sum < 0.001 ? 0.001 : sum;
  return {
    for (final id in sessionIds) id: available * (weights[id] ?? 1.0) / safe,
  };
}

/// Drop weights for sessions no longer in the stack. Returns the pruned map;
/// Swift `pruneWeights` also persists when anything changed.
Map<String, double> pruneWeights(
    Map<String, double> weights, Iterable<String> liveIds) {
  final keep = liveIds.toSet();
  return {for (final e in weights.entries) if (keep.contains(e.key)) e.key: e.value};
}

/// Apply one divider drag step: transfer [deltaWeight] from the pane below
/// to the pane above, clamping each to [minWeight]. Swift's divider
/// `.onChanged` keeps the pair's combined weight invariant.
({double above, double below}) dividerDragStep({
  required double above,
  required double below,
  required double deltaWeight,
  required double minWeight,
}) {
  var a = above + deltaWeight;
  var b = below - deltaWeight;
  if (a < minWeight) {
    b -= (minWeight - a);
    a = minWeight;
  }
  if (b < minWeight) {
    a -= (minWeight - b);
    b = minWeight;
  }
  return (above: a, below: b);
}
