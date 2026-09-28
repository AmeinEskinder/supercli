/// Behavior tests for `sidebar_projection_changes.dart`
/// (port of `SidebarProjectionChanges.swift`).
///
/// Cache inputs are per-project rows, mixed order, and pin/archive/unread
/// flags. A status change in one folder must not evict every other list.
library;

import 'package:supercli_app/screens/sidebar_projection_changes.dart';
import 'package:test/test.dart';

ProjectionNode node(
  String id, {
  Object project = 'p',
  List<String> sessions = const [],
  List<ProjectionNode> worktrees = const [],
}) =>
    ProjectionNode(
        id: id, project: project, sessions: sessions, worktrees: worktrees);

Set<String> affected({
  required List<ProjectionNode> previous,
  required List<ProjectionNode> next,
  Map<String, ProjectionSessionSummary> previousSummaries = const {},
  Map<String, ProjectionSessionSummary> nextSummaries = const {},
  Map<String, Object> previousProjects = const {},
  Map<String, Object> nextProjects = const {},
  Map<String, List<String>> previousOrder = const {},
  Map<String, List<String>> nextOrder = const {},
}) =>
    SidebarProjectionChanges.affectedProjects(
      previous: previous,
      next: next,
      previousSummaries: previousSummaries,
      nextSummaries: nextSummaries,
      previousProjects: previousProjects,
      nextProjects: nextProjects,
      previousOrder: previousOrder,
      nextOrder: nextOrder,
    );

void main() {
  group('SidebarProjectionChanges (SidebarProjectionChanges.swift)', () {
    test('identical inputs affect nothing', () {
      final tree = [node('a', sessions: ['s1'])];
      expect(affected(previous: tree, next: tree), isEmpty);
    });

    test('added and removed projects are affected', () {
      expect(
          affected(
            previous: [node('a')],
            next: [node('a'), node('b')],
          ),
          {'b'});
      expect(
          affected(
            previous: [node('a'), node('b')],
            next: [node('a')],
          ),
          {'b'});
    });

    test('session pin/archive/unread change affects its project only', () {
      const before = ProjectionSessionSummary(id: 's1');
      const pinned = ProjectionSessionSummary(id: 's1', pinned: true);
      const archived = ProjectionSessionSummary(id: 's1', archived: true);
      const unread = ProjectionSessionSummary(id: 's1', unread: true);
      for (final after in [pinned, archived, unread]) {
        expect(
            affected(
              previous: [node('a', sessions: ['s1']), node('b')],
              next: [node('a', sessions: ['s1']), node('b')],
              previousSummaries: {'s1': before},
              nextSummaries: {'s1': after},
            ),
            {'a'});
      }
    });

    test('unchanged flags do not affect the project', () {
      const s = ProjectionSessionSummary(id: 's1', pinned: true);
      expect(
          affected(
            previous: [node('a', sessions: ['s1'])],
            next: [node('a', sessions: ['s1'])],
            previousSummaries: {'s1': s},
            nextSummaries: {'s1': s},
          ),
          isEmpty);
    });

    test('project summary change affects the project', () {
      expect(
          affected(
            previous: [node('a')],
            next: [node('a')],
            previousProjects: {'a': 'v1'},
            nextProjects: {'a': 'v2'},
          ),
          {'a'});
    });

    test('order change affects the project', () {
      expect(
          affected(
            previous: [node('a')],
            next: [node('a')],
            previousOrder: {
              'a': ['s1', 's2']
            },
            nextOrder: {
              'a': ['s2', 's1']
            },
          ),
          {'a'});
    });

    test('worktree membership change affects the parent', () {
      expect(
          affected(
            previous: [node('a')],
            next: [
              node('a', worktrees: [node('wt')])
            ],
          ),
          {'a', 'wt'});
    });

    test('session list change affects the project', () {
      expect(
          affected(
            previous: [node('a', sessions: ['s1'])],
            next: [
              node('a', sessions: ['s1', 's2'])
            ],
          ),
          {'a'});
    });
  });
}
