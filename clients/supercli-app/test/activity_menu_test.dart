/// Tests for the activity menu projection.
///
/// Port of the `WorkspaceActivityMenuSlice` / `GlobalActivityMenuSessions`
/// behaviour in `GlobalActivityMenu.swift`: session classification,
/// project-path resolution, title fallback, status labels, and the
/// multi-workspace merge.
library;

import 'package:supercli_app/activity_menu.dart';
import 'package:test/test.dart';

ActivityMenuSessionInput sess(
  String id, {
  String title = 't',
  String command = 'cmd',
  String projectId = 'p1',
  bool running = true,
  ActivityMenuSessionActivity activity = ActivityMenuSessionActivity.working,
  bool unread = false,
  bool archived = false,
  String? alertBody,
}) => ActivityMenuSessionInput(
  id: id,
  title: title,
  command: command,
  projectId: projectId,
  running: running,
  activity: activity,
  unread: unread,
  archived: archived,
  alertBody: alertBody,
);

void main() {
  group('WorkspaceActivityMenuSlice.build', () {
    test('classifies blocked / working / unread sessions', () {
      final slice = WorkspaceActivityMenuSlice.build(
        projects: const [ActivityMenuProject(id: 'p1', name: 'Proj')],
        sessions: [
          sess('b', activity: ActivityMenuSessionActivity.blocked),
          sess('w', activity: ActivityMenuSessionActivity.working),
          sess('s', activity: ActivityMenuSessionActivity.starting),
          sess('f', running: false, unread: true),
          sess('i', running: false, activity: ActivityMenuSessionActivity.idle),
        ],
      );
      expect(slice.blockers.map((s) => s.sessionId), ['b']);
      expect(slice.jobs.map((s) => s.sessionId).toSet(), {'w', 's'});
      expect(slice.finished.map((s) => s.sessionId), ['f']);
    });

    test('skips archived sessions and duplicate ids', () {
      final slice = WorkspaceActivityMenuSlice.build(
        projects: const [ActivityMenuProject(id: 'p1', name: 'Proj')],
        sessions: [
          sess('a', archived: true),
          sess('b'),
          sess('b', title: 'dup'),
        ],
      );
      expect(slice.jobs.map((s) => s.sessionId), ['b']);
      expect(slice.jobs.single.title, 't');
    });

    test('resolves nested project paths', () {
      final slice = WorkspaceActivityMenuSlice.build(
        projects: const [
          ActivityMenuProject(id: 'root', name: 'Root'),
          ActivityMenuProject(id: 'child', name: 'Child', parentId: 'root'),
        ],
        sessions: [sess('a', projectId: 'child')],
      );
      expect(slice.jobs.single.projectPath, 'Root › Child');
    });

    test('unknown project yields placeholder path', () {
      final slice = WorkspaceActivityMenuSlice.build(
        projects: const [],
        sessions: [sess('a', projectId: 'nope')],
      );
      expect(slice.jobs.single.projectPath, 'Unknown project');
    });

    test('cyclic ancestry terminates', () {
      final slice = WorkspaceActivityMenuSlice.build(
        projects: const [
          ActivityMenuProject(id: 'a', name: 'A', parentId: 'b'),
          ActivityMenuProject(id: 'b', name: 'B', parentId: 'a'),
        ],
        sessions: [sess('s', projectId: 'a')],
      );
      expect(slice.jobs.single.projectPath.isNotEmpty, isTrue);
    });

    test('title falls back to command then Untitled session', () {
      final slice = WorkspaceActivityMenuSlice.build(
        projects: const [ActivityMenuProject(id: 'p1', name: 'P')],
        sessions: [
          sess('a', title: '  ', command: '  '),
          sess('b', title: '', command: 'make build'),
        ],
      );
      final byId = {for (final s in slice.jobs) s.sessionId: s};
      expect(byId['a']!.title, 'Untitled session');
      expect(byId['b']!.title, 'make build');
    });

    test('status labels match Swift', () {
      String labelFor(ActivityMenuSessionInput s) {
        final slice = WorkspaceActivityMenuSlice.build(
          projects: const [ActivityMenuProject(id: 'p1', name: 'P')],
          sessions: [s],
        );
        final all = [...slice.jobs, ...slice.blockers, ...slice.finished];
        expect(all, hasLength(1));
        return all.single.status;
      }

      String label(ActivityMenuSessionActivity a, {bool running = true}) =>
          labelFor(sess('x', running: running, activity: a, unread: true));

      expect(label(ActivityMenuSessionActivity.starting), 'Starting');
      expect(label(ActivityMenuSessionActivity.working), 'Working');
      expect(label(ActivityMenuSessionActivity.blocked), 'Blocked');
      expect(label(ActivityMenuSessionActivity.done, running: false), 'Exited');
      expect(label(ActivityMenuSessionActivity.idle, running: false), 'Exited');
      expect(
        label(ActivityMenuSessionActivity.unknown, running: false),
        'Exited',
      );
    });
  });

  group('buildGlobalActivityMenu', () {
    const ws1 = ActivityMenuWorkspace(id: 'w1', name: 'One');
    const ws2 = ActivityMenuWorkspace(id: 'w2', name: 'Two');

    WorkspaceActivityMenuSlice sliceWith(String sessionId) =>
        WorkspaceActivityMenuSlice.build(
          projects: const [ActivityMenuProject(id: 'p1', name: 'P')],
          sessions: [sess(sessionId)],
        );

    test('foreground slice wins over cached slice', () {
      final live = sliceWith('live');
      final result = buildGlobalActivityMenu(
        workspaces: const [ws1],
        foregroundKey: 'w1',
        foreground: live,
        cachedSlice: (_) => sliceWith('stale'),
      );
      expect(result.jobs.single.session.sessionId, 'live');
      expect(result.jobs.single.workspaceKey, 'w1');
      expect(result.jobs.single.workspaceName, 'One');
    });

    test('non-foreground workspaces use cached slices', () {
      final result = buildGlobalActivityMenu(
        workspaces: const [ws1, ws2],
        foregroundKey: 'w1',
        foreground: const WorkspaceActivityMenuSlice(),
        cachedSlice: (id) => id == 'w2' ? sliceWith('cached') : null,
      );
      expect(result.jobs.single.session.sessionId, 'cached');
      expect(result.jobs.single.workspaceName, 'Two');
    });

    test('workspaces without a slice are skipped', () {
      final result = buildGlobalActivityMenu(
        workspaces: const [ws1],
        foregroundKey: 'other',
        foreground: const WorkspaceActivityMenuSlice(),
        cachedSlice: (_) => null,
      );
      expect(result, GlobalActivityMenuSessions.empty);
      expect(result.sectionCount, 0);
      expect(result.rowCount, 0);
    });

    test('section and row counts', () {
      final slice = WorkspaceActivityMenuSlice.build(
        projects: const [ActivityMenuProject(id: 'p1', name: 'P')],
        sessions: [
          sess('b', activity: ActivityMenuSessionActivity.blocked),
          sess('w'),
        ],
      );
      final result = buildGlobalActivityMenu(
        workspaces: const [ws1],
        foregroundKey: 'w1',
        foreground: slice,
        cachedSlice: (_) => null,
      );
      expect(result.sectionCount, 2);
      expect(result.rowCount, 2);
    });

    test('menu item id joins workspace and session with unit separator', () {
      const item = GlobalActivityMenuItem(
        workspaceKey: 'w1',
        workspaceName: 'One',
        session: WorkspaceActivityMenuSession(
          sessionId: 's1',
          title: 't',
          command: 'c',
          projectPath: 'P',
          status: 'Working',
        ),
      );
      expect(item.id, 'w1\x1fs1');
    });
  });
}
