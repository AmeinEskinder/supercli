/// Port of `SidebarProjectionChanges.swift`
/// (`clients/legacy/native/SupercliNative/Sources/SupercliNative/`).
///
/// Cache inputs are per-project rows, mixed order, and pin/archive/unread
/// flags. A status change in one folder must not evict every other list —
/// this computes exactly which project ids need re-rendering when the
/// sidebar's inputs change.
library;

/// A top-level project plus its session ids and worktree children, ready for
/// the sidebar to render. Mirrors Swift `ProjectNode` (only the fields the
/// diff reads).
final class ProjectionNode {
  const ProjectionNode({
    required this.id,
    required this.project,
    this.sessions = const [],
    this.worktrees = const [],
  });

  final String id;

  /// The project value; compared with `==` like Swift's `Equatable` Project.
  final Object project;
  final List<String> sessions;
  final List<ProjectionNode> worktrees;
}

/// The session flags the diff reads. Mirrors the `RemoteSessionSummary`
/// fields compared by `SidebarProjectionChanges`.
final class ProjectionSessionSummary {
  const ProjectionSessionSummary({
    required this.id,
    this.pinned = false,
    this.archived = false,
    this.unread = false,
  });

  final String id;
  final bool pinned;
  final bool archived;
  final bool unread;
}

/// Computes the set of project ids whose rendered output may have changed.
abstract final class SidebarProjectionChanges {
  const SidebarProjectionChanges._();

  static Set<String> affectedProjects({
    required List<ProjectionNode> previous,
    required List<ProjectionNode> next,
    required Map<String, ProjectionSessionSummary> previousSummaries,
    required Map<String, ProjectionSessionSummary> nextSummaries,
    required Map<String, Object> previousProjects,
    required Map<String, Object> nextProjects,
    required Map<String, List<String>> previousOrder,
    required Map<String, List<String>> nextOrder,
  }) {
    Map<String, ProjectionNode> index(List<ProjectionNode> roots) {
      final result = <String, ProjectionNode>{};
      final pending = [...roots];
      while (pending.isNotEmpty) {
        final node = pending.removeLast();
        result[node.id] = node;
        pending.addAll(node.worktrees);
      }
      return result;
    }

    bool listEquals(List<String> a, List<String> b) {
      if (a.length != b.length) return false;
      for (var i = 0; i < a.length; i++) {
        if (a[i] != b[i]) return false;
      }
      return true;
    }

    final old = index(previous);
    final fresh = index(next);
    final changed = <String>{}
      ..addAll(old.keys.where((id) => !fresh.containsKey(id)))
      ..addAll(fresh.keys.where((id) => !old.containsKey(id)));
    for (final entry in fresh.entries) {
      final prior = old[entry.key];
      if (prior == null) continue;
      final node = entry.value;
      if (prior.project != node.project ||
          !listEquals(prior.sessions, node.sessions) ||
          !listEquals(
              prior.worktrees.map((w) => w.id).toList(),
              node.worktrees.map((w) => w.id).toList()) ||
          previousProjects[entry.key] != nextProjects[entry.key] ||
          !listEquals(
              previousOrder[entry.key] ?? const [],
              nextOrder[entry.key] ?? const [])) {
        changed.add(entry.key);
        continue;
      }
      for (final sessionId in node.sessions) {
        final lhs = previousSummaries[sessionId];
        final rhs = nextSummaries[sessionId];
        if (lhs?.pinned != rhs?.pinned ||
            lhs?.archived != rhs?.archived ||
            lhs?.unread != rhs?.unread) {
          changed.add(entry.key);
          break;
        }
      }
    }
    return changed;
  }
}
