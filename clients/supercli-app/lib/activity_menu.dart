/// Activity menu projection: cached per-workspace session slices.
///
/// Port of the pure-logic half of `GlobalActivityMenu.swift`:
/// `WorkspaceActivityMenuSession`, `WorkspaceActivityMenuSlice`,
/// `GlobalActivityMenuItem`, and `GlobalActivityMenuSessions`.
///
/// A compact, cached projection for the activity dropdown. The slice is
/// built once per accepted bootstrap: project lookup/path construction is
/// O(projects), session classification is O(sessions), and neither work is
/// repeated by menu rendering or the spinner timer.
///
/// The reactive `GlobalActivityMenuModel` (Combine `ObservableObject`) is
/// intentionally not ported: the Dart app drives rebuilds through its own
/// state layer. Feed it [GlobalActivityMenuSessions] built by
/// [buildGlobalActivityMenu].
///
/// Rendered by `screens/globalactivitymenu.dart` (`GlobalActivityMenu`).
library;

/// One session row in the activity menu.
final class WorkspaceActivityMenuSession {
  const WorkspaceActivityMenuSession({
    required this.sessionId,
    required this.title,
    required this.command,
    required this.projectPath,
    required this.status,
    this.alertBody,
  });

  final String sessionId;
  final String title;
  final String command;

  /// "Parent › Child" breadcrumb, or "Unknown project".
  final String projectPath;

  /// Human status label: Starting / Working / Blocked / Done / Idle / Exited.
  final String status;

  /// Latest app alert when it is this session's newest activity.
  final String? alertBody;

  @override
  bool operator ==(Object other) =>
      other is WorkspaceActivityMenuSession &&
      other.sessionId == sessionId &&
      other.title == title &&
      other.command == command &&
      other.projectPath == projectPath &&
      other.status == status &&
      other.alertBody == alertBody;

  @override
  int get hashCode =>
      Object.hash(sessionId, title, command, projectPath, status, alertBody);
}

/// Minimal project input for slice building.
final class ActivityMenuProject {
  const ActivityMenuProject({
    required this.id,
    required this.name,
    this.parentId,
  });

  final String id;
  final String name;
  final String? parentId;
}

/// Session activity, mirroring the Host wire values.
enum ActivityMenuSessionActivity {
  starting,
  working,
  blocked,
  done,
  idle,
  unknown,
}

/// Minimal session input for slice building.
///
/// Decoupled from `models.dart`'s `SessionSummary` so the projection stays
/// stable while the wire models evolve.
final class ActivityMenuSessionInput {
  const ActivityMenuSessionInput({
    required this.id,
    required this.title,
    required this.command,
    required this.projectId,
    required this.running,
    this.activity = ActivityMenuSessionActivity.unknown,
    this.unread = false,
    this.archived = false,
    this.alertBody,
  });

  final String id;
  final String title;
  final String command;
  final String projectId;
  final bool running;
  final ActivityMenuSessionActivity activity;
  final bool unread;
  final bool archived;
  final String? alertBody;
}

/// One workspace's classified session slice.
final class WorkspaceActivityMenuSlice {
  const WorkspaceActivityMenuSlice({
    this.jobs = const [],
    this.blockers = const [],
    this.finished = const [],
  });

  /// Running + starting/working.
  final List<WorkspaceActivityMenuSession> jobs;

  /// Running + blocked.
  final List<WorkspaceActivityMenuSession> blockers;

  /// Unread (finished) sessions.
  final List<WorkspaceActivityMenuSession> finished;

  /// Build once per accepted bootstrap.
  ///
  /// Mirrors `WorkspaceActivityMenuSlice.init(snapshot:)`: project paths are
  /// resolved with cycle protection (max 32 ancestors), archived sessions
  /// are skipped, and duplicate session IDs are dropped.
  factory WorkspaceActivityMenuSlice.build({
    required List<ActivityMenuProject> projects,
    required List<ActivityMenuSessionInput> sessions,
  }) {
    final byId = {for (final p in projects) p.id: p};
    final pathCache = <String, String>{};

    String projectPath(String projectId) {
      final cached = pathCache[projectId];
      if (cached != null) return cached;
      String resolve() {
        if (!byId.containsKey(projectId)) return 'Unknown project';
        final names = <String>[];
        final seen = <String>{};
        String? cursor = projectId;
        while (cursor != null && seen.add(cursor) && names.length < 32) {
          final project = byId[cursor];
          if (project == null) break;
          names.add(project.name);
          cursor = project.parentId;
        }
        return names.reversed.join(' › ');
      }

      final path = resolve();
      pathCache[projectId] = path;
      return path;
    }

    final jobs = <WorkspaceActivityMenuSession>[];
    final blockers = <WorkspaceActivityMenuSession>[];
    final finished = <WorkspaceActivityMenuSession>[];
    final seen = <String>{};
    for (final s in sessions) {
      if (s.archived || !seen.add(s.id)) continue;
      final item = WorkspaceActivityMenuSession(
        sessionId: s.id,
        title: _title(s.title, fallback: s.command),
        command: s.command,
        projectPath: projectPath(s.projectId),
        status: _statusLabel(s),
        alertBody: s.alertBody,
      );
      if (s.running && s.activity == ActivityMenuSessionActivity.blocked) {
        blockers.add(item);
      } else if (s.running &&
          (s.activity == ActivityMenuSessionActivity.starting ||
              s.activity == ActivityMenuSessionActivity.working)) {
        jobs.add(item);
      } else if (s.unread) {
        finished.add(item);
      }
    }
    return WorkspaceActivityMenuSlice(
      jobs: jobs,
      blockers: blockers,
      finished: finished,
    );
  }

  static String _title(String raw, {required String fallback}) {
    final title = raw.trim();
    if (title.isNotEmpty) return title;
    final fb = fallback.trim();
    return fb.isEmpty ? 'Untitled session' : fb;
  }

  static String _statusLabel(ActivityMenuSessionInput s) {
    if (!s.running) return 'Exited';
    return switch (s.activity) {
      ActivityMenuSessionActivity.starting => 'Starting',
      ActivityMenuSessionActivity.working => 'Working',
      ActivityMenuSessionActivity.blocked => 'Blocked',
      ActivityMenuSessionActivity.done => 'Done',
      ActivityMenuSessionActivity.idle ||
      ActivityMenuSessionActivity.unknown => 'Idle',
    };
  }
}

/// One session row tagged with its workspace.
final class GlobalActivityMenuItem {
  const GlobalActivityMenuItem({
    required this.workspaceKey,
    required this.workspaceName,
    required this.session,
  });

  final String workspaceKey;
  final String workspaceName;
  final WorkspaceActivityMenuSession session;

  /// Mirrors Swift's `workspaceKey + "\u{1f}" + session.sessionID`.
  String get id => '$workspaceKey\x1f${session.sessionId}';

  @override
  bool operator ==(Object other) =>
      other is GlobalActivityMenuItem &&
      other.workspaceKey == workspaceKey &&
      other.workspaceName == workspaceName &&
      other.session == session;

  @override
  int get hashCode => Object.hash(workspaceKey, workspaceName, session);
}

/// Minimal workspace row input for the multi-workspace merge.
final class ActivityMenuWorkspace {
  const ActivityMenuWorkspace({required this.id, required this.name});

  final String id;
  final String name;
}

/// Merged activity across workspaces.
///
/// Mirrors `GlobalActivityMenuSessions`: the foreground workspace uses the
/// live slice; other workspaces use their cached slice. Even an empty
/// foreground slice overrides an older cached slice.
final class GlobalActivityMenuSessions {
  const GlobalActivityMenuSessions({
    this.jobs = const [],
    this.blockers = const [],
    this.finished = const [],
  });

  static const empty = GlobalActivityMenuSessions();

  final List<GlobalActivityMenuItem> jobs;
  final List<GlobalActivityMenuItem> blockers;
  final List<GlobalActivityMenuItem> finished;

  int get sectionCount =>
      [blockers, jobs, finished].where((s) => s.isNotEmpty).length;

  int get rowCount => jobs.length + blockers.length + finished.length;

  @override
  bool operator ==(Object other) =>
      other is GlobalActivityMenuSessions &&
      _listEquals(jobs, other.jobs) &&
      _listEquals(blockers, other.blockers) &&
      _listEquals(finished, other.finished);

  @override
  int get hashCode => Object.hash(
    Object.hashAll(jobs),
    Object.hashAll(blockers),
    Object.hashAll(finished),
  );

  static bool _listEquals<T>(List<T> a, List<T> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
}

/// Build the merged activity menu.
///
/// [foregroundKey] selects the workspace whose live [foreground] slice wins;
/// every other workspace falls back to [cachedSlice].
GlobalActivityMenuSessions buildGlobalActivityMenu({
  required List<ActivityMenuWorkspace> workspaces,
  String? foregroundKey,
  required WorkspaceActivityMenuSlice foreground,
  required WorkspaceActivityMenuSlice? Function(String workspaceId) cachedSlice,
}) {
  final jobs = <GlobalActivityMenuItem>[];
  final blockers = <GlobalActivityMenuItem>[];
  final finished = <GlobalActivityMenuItem>[];
  for (final ws in workspaces) {
    final slice = ws.id == foregroundKey ? foreground : cachedSlice(ws.id);
    if (slice == null) continue;
    List<GlobalActivityMenuItem> wrap(
      List<WorkspaceActivityMenuSession> sessions,
    ) {
      return sessions
          .map(
            (s) => GlobalActivityMenuItem(
              workspaceKey: ws.id,
              workspaceName: ws.name,
              session: s,
            ),
          )
          .toList();
    }

    blockers.addAll(wrap(slice.blockers));
    jobs.addAll(wrap(slice.jobs));
    finished.addAll(wrap(slice.finished));
  }
  return GlobalActivityMenuSessions(
    jobs: jobs,
    blockers: blockers,
    finished: finished,
  );
}
