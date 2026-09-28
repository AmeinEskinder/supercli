/// Port of `SessionMoveRules.swift`
/// (`clients/legacy/native/SupercliNative/Sources/SupercliNative/`).
///
/// Where a Session may be filed in the sidebar.
///
/// A Session's shell runs in exactly one checkout. Its HOME is the nearest
/// git-worktree project at or above its manifest project, or the root
/// project when it is not inside a worktree. Filing is display-only (the
/// shared `project-override.json` marker), so it may never claim a different
/// checkout: the only valid targets are the home itself and plain
/// organizational groups directly under it. Reordering among siblings is a
/// different verb and is unaffected.
///
/// The Host enforces the same rule in `controller_host::
/// validate_session_project_target` for every Controller.
library;

/// The project fields the filing rules read. Mirrors the Swift `Project`
/// computed properties used by `SessionMoveRules`.
final class MoveRuleProject {
  const MoveRuleProject({
    required this.id,
    this.parentProjectId,
    this.worktreeBranch,
    this.isFolder = false,
    this.sortOrder,
  });

  final String id;
  final String? parentProjectId;
  final String? worktreeBranch;
  final bool isFolder;
  final int? sortOrder;

  /// Swift `Project.isWorktree`: `worktreeBranch != nil && parentProjectID != nil`.
  bool get isWorktree => worktreeBranch != null && parentProjectId != null;

  /// Swift `Project.acceptsSessionDrop`: only plain organizational child
  /// groups accept a running session drop. Worktree children need an explicit
  /// restart/resume to change checkout; top-level projects are reorder
  /// targets, not filing targets.
  bool get acceptsSessionDrop =>
      parentProjectId != null && worktreeBranch == null && isFolder;
}

/// Filing rules for sidebar session rows. All functions are pure.
abstract final class SessionMoveRules {
  const SessionMoveRules._();

  /// The checkout-bound project for a Session whose manifest names
  /// [forProjectId]: walk up through plain groups until a worktree project
  /// or a root is reached. Capped at 16 hops.
  static String homeProjectId({
    required String forProjectId,
    required Map<String, MoveRuleProject> projectsById,
  }) {
    var id = forProjectId;
    var hops = 0;
    while (hops < 16) {
      final project = projectsById[id];
      if (project == null || project.isWorktree) break;
      final parent = project.parentProjectId;
      if (parent == null) break;
      id = parent;
      hops++;
    }
    return id;
  }

  /// True when the Session's home is a git worktree: its row may only be
  /// filed inside that worktree.
  static bool isWorktreeBound({
    required String sessionProjectId,
    required Map<String, MoveRuleProject> projectsById,
  }) {
    final home =
        homeProjectId(forProjectId: sessionProjectId, projectsById: projectsById);
    return projectsById[home]?.isWorktree == true;
  }

  /// Whether [targetId] is a legal filing destination for a Session whose
  /// manifest names [sessionProjectId] and which currently renders under
  /// [effectiveProjectId] (its override, or the manifest project).
  static bool canFile({
    required String sessionProjectId,
    required String effectiveProjectId,
    required String targetId,
    required Map<String, MoveRuleProject> projectsById,
  }) {
    final target = projectsById[targetId];
    if (target == null) return false;
    final home =
        homeProjectId(forProjectId: sessionProjectId, projectsById: projectsById);
    final targetIsHome = targetId == home;
    final targetIsPlainGroup =
        target.acceptsSessionDrop && target.parentProjectId == home;
    if (!targetIsHome && !targetIsPlainGroup) return false;
    return effectiveProjectId != targetId;
  }

  /// "Move to ▸" destinations: the home project plus its plain groups, in
  /// sidebar order, minus the current location and any hidden group.
  static List<MoveRuleProject> destinations({
    required String sessionProjectId,
    required String effectiveProjectId,
    required Map<String, MoveRuleProject> projectsById,
    required bool Function(String id) isHiddenGroup,
  }) {
    final homeId =
        homeProjectId(forProjectId: sessionProjectId, projectsById: projectsById);
    final home = projectsById[homeId];
    if (home == null) return const [];
    final groups = projectsById.values
        .where((p) =>
            p.parentProjectId == homeId &&
            p.acceptsSessionDrop &&
            !isHiddenGroup(p.id))
        .toList()
      ..sort((a, b) => (a.sortOrder ?? 0).compareTo(b.sortOrder ?? 0));
    return [home, ...groups].where((p) => p.id != effectiveProjectId).toList();
  }

  /// A drag hovering a row owned by [hoveredProjectId] crosses a checkout
  /// boundary when the two homes differ and at least one of them is a
  /// worktree. Such a release is refused with the "no" shake instead of
  /// silently landing nowhere.
  static bool crossesCheckout({
    required String sessionProjectId,
    required String hoveredProjectId,
    required Map<String, MoveRuleProject> projectsById,
  }) {
    final sessionHome =
        homeProjectId(forProjectId: sessionProjectId, projectsById: projectsById);
    final hoveredHome =
        homeProjectId(forProjectId: hoveredProjectId, projectsById: projectsById);
    if (sessionHome == hoveredHome) return false;
    return projectsById[sessionHome]?.isWorktree == true ||
        projectsById[hoveredHome]?.isWorktree == true;
  }
}
