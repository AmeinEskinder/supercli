
/// Sidebar/project-panel geometry, titlebar state, and the activity-menu
/// projection.
///
/// Ports of the state logic in `RootView.swift` (SupercliNative/Views):
/// - sidebar width persistence + drag-resize clamping (DESIGN.md §4)
/// - per-project right-panel widths (`supercli.projectSidebar.widths`)
/// - derived project-panel visibility
/// - `ActivityMenuSessions` (attention/jobs/finished buckets)
/// - the activity dropdown's deterministic scroll height
/// - titlebar button visibility + fullscreen offset
/// - the surface-cache prune plan (live vs protected ids)
///
/// Rendering stays in [RootView.build]; these are the pure behaviours the
/// Swift view derives its layout from. Theme numbers come from
/// `ThemeLayout` / `spinnerFrames` in `../theme.dart` (ported from
/// `Theme.swift`); they are not redefined here.
library;

import 'dart:math';

import '../theme.dart';

/// Persistence keys (mirroring the Swift `@AppStorage` keys).
const String rootSidebarWidthKey = 'supercli.sidebar.width';
const String rootProjectSidebarWidthKey = 'supercli.projectSidebar.width';
const String rootProjectSidebarWidthsKey = 'supercli.projectSidebar.widths';

/// Sidebar + project-panel geometry math. Port of the width derivations and
/// resizer drag handlers in `RootView.swift`.
abstract final class RootSidebarGeometry {
  /// Clamp a proposed width into [ThemeLayout.sidebarMinWidth]–[ThemeLayout.sidebarMaxWidth].
  static double clampWidth(double proposed) =>
      proposed
          .clamp(ThemeLayout.sidebarMinWidth, ThemeLayout.sidebarMaxWidth)
          .toDouble();

  /// Width after a resizer drag: `startWidth + translation`, clamped.
  /// The project panel is mirrored (dragging left grows it).
  static double dragWidth({
    required double startWidth,
    required double translation,
    bool mirrored = false,
  }) =>
      clampWidth(startWidth + (mirrored ? -translation : translation));

  /// The laid-out sidebar width: 0 when collapsed.
  static double shownSidebarWidth({
    required bool collapsed,
    required double width,
  }) =>
      collapsed ? 0 : width;

  /// The project panel's width: live drag value wins, then the per-project
  /// entry, then the shared key (fallback for never-resized projects).
  static double projectSidebarWidth({
    required Map<String, double> perProjectWidths,
    required String projectKey,
    required double sharedWidth,
    double? draggingWidth,
  }) =>
      draggingWidth ??
      perProjectWidths[projectKey] ??
      sharedWidth;

  /// Panel visibility is derived, never toggled: non-empty sidebar-group
  /// membership for the current root project shows it, and full-content
  /// pages (settings) cover the workspace.
  static bool projectSidebarShown({
    required bool hasMembers,
    required bool settingsVisible,
  }) =>
      hasMembers && !settingsVisible;
}

/// One session row for the activity-menu projection.
final class ActivityMenuEntry {
  const ActivityMenuEntry({
    required this.id,
    required this.title,
    required this.attention,
    required this.createdAtMs,
    this.lifecycleAtMs,
    this.command = '',
    this.workspace = '',
    this.projectPath = '',
    this.status = '',
    this.alertBody,
    this.unread = false,
    this.working = false,
  });

  final String id;
  final String title;
  final bool attention;
  final int createdAtMs;
  final int? lifecycleAtMs;

  /// Launch command (drives the CLI/provider icon in the full client).
  final String command;

  /// Workspace + project path shown under the title.
  final String workspace;
  final String projectPath;

  /// Lifecycle status text ("Starting", "Restarting", "Resuming", …).
  final String status;

  /// Alert body; when set the subtitle reads "Alert · <body>".
  final String? alertBody;

  /// Recently-finished row: unread dot instead of the status label.
  final bool unread;

  /// Active job: braille loader on the left.
  final bool working;
}

/// The common activity-menu projection used by the titlebar and menu-bar
/// surfaces. Attention rows form their own section and win over the active
/// or unread buckets, so one session can never appear twice with conflicting
/// states (for example, as both Blocked and recently finished).
///
/// Port of `ActivityMenuSessions` (RootView.swift).
final class ActivityMenuSessions {
  ActivityMenuSessions._({
    required this.blockers,
    required this.jobs,
    required this.finished,
  });

  factory ActivityMenuSessions({
    required List<ActivityMenuEntry> renderedInTreeOrder,
    required List<ActivityMenuEntry> allEntries,
    required List<ActivityMenuEntry> jobs,
    required List<ActivityMenuEntry> finished,
  }) {
    final blockers = _buildBlockers(renderedInTreeOrder, allEntries);
    final filteredJobs = _buildJobs(jobs, blockers);
    return ActivityMenuSessions._(
      blockers: blockers,
      jobs: filteredJobs,
      finished: _buildFinished(finished, blockers, filteredJobs),
    );
  }

  final List<ActivityMenuEntry> blockers;
  final List<ActivityMenuEntry> jobs;
  final List<ActivityMenuEntry> finished;

  int get sectionCount =>
      [blockers, jobs, finished].where((s) => s.isNotEmpty).length;

  int get rowCount => blockers.length + jobs.length + finished.length;

  static List<ActivityMenuEntry> _buildBlockers(
    List<ActivityMenuEntry> renderedInTreeOrder,
    List<ActivityMenuEntry> allEntries,
  ) {
    final candidates = renderedInTreeOrder
        .where((entry) => entry.attention)
        .toList();
    // Keep the visible project-tree order first, then cover the transient
    // case where a live manifest exists before (or without) its project
    // node. Orphan blockers use lifecycle/id order like Recent surfaces.
    final renderedIDs = candidates.map((entry) => entry.id).toSet();
    final orphans = allEntries
        .where(
          (entry) => entry.attention && !renderedIDs.contains(entry.id),
        )
        .toList()
      ..sort((a, b) {
        final aStamp = max(a.createdAtMs, a.lifecycleAtMs ?? 0);
        final bStamp = max(b.createdAtMs, b.lifecycleAtMs ?? 0);
        if (aStamp != bStamp) return bStamp.compareTo(aStamp);
        return a.id.compareTo(b.id);
      });
    return _uniqued([...candidates, ...orphans]);
  }

  static List<ActivityMenuEntry> _buildJobs(
    List<ActivityMenuEntry> jobs,
    List<ActivityMenuEntry> blockers,
  ) =>
      _uniqued(jobs, excluding: blockers.map((entry) => entry.id).toSet());

  static List<ActivityMenuEntry> _buildFinished(
    List<ActivityMenuEntry> finished,
    List<ActivityMenuEntry> blockers,
    List<ActivityMenuEntry> jobs,
  ) =>
      _uniqued(
        finished,
        excluding: {
          ...blockers.map((entry) => entry.id),
          ...jobs.map((entry) => entry.id),
        },
      );

  static List<ActivityMenuEntry> _uniqued(
    List<ActivityMenuEntry> entries, {
    Set<String> excluding = const {},
  }) {
    final seen = Set<String>.from(excluding);
    return entries.where((entry) => seen.add(entry.id)).toList();
  }
}

/// Deterministic dropdown sizing (ActivityMenuList): 42pt rows, 9pt dividers,
/// capped at 429pt (ten visible rows, the rest scroll); 44pt when empty.
/// Port of `ActivityMenuList.scrollHeight` / `emptyScrollHeight`.
double activityMenuScrollHeight({
  required int rowCount,
  required int sectionCount,
}) {
  const emptyHeight = 44.0;
  if (rowCount == 0) return emptyHeight;
  final dividers = max(0, sectionCount - 1) * 9.0;
  return min(rowCount * 42.0 + dividers, 429.0);
}

/// Titlebar controls. Port of `RootView.titlebarButtons`.
enum RootTitlebarControl {
  /// The 28×28 sidebar toggle (⌘B).
  sidebarToggle,

  /// The braille-spinner/bell activity menu.
  activityMenu,

  /// "Back to workspace" (full-content page covering the workspace).
  back,

  /// Collapsed-sidebar "+" new-session menu.
  newSession,
}

/// Titlebar button visibility + layout. Port of the `titlebarButtons`
/// conditions in `RootView.swift`.
abstract final class RootViewTitlebar {
  /// Always [sidebarToggle, activityMenu]; when the sidebar is collapsed,
  /// [back] replaces [newSession] while a full-content page (settings,
  /// recent activity, archived) covers the workspace.
  static List<RootTitlebarControl> controls({
    required bool sidebarCollapsed,
    required bool settingsVisible,
    required bool recentActivityVisible,
    String? archivedProjectID,
  }) {
    final controls = [
      RootTitlebarControl.sidebarToggle,
      RootTitlebarControl.activityMenu,
    ];
    if (sidebarCollapsed) {
      if (settingsVisible ||
          recentActivityVisible ||
          archivedProjectID != null) {
        controls.add(RootTitlebarControl.back);
      } else {
        controls.add(RootTitlebarControl.newSession);
      }
    }
    return controls;
  }

  /// Horizontal offset of the titlebar button row: clear of the traffic
  /// lights, flush left in fullscreen.
  static double offsetX({required bool windowIsFullScreen}) =>
      windowIsFullScreen ? 12 : 80;

  /// The window title strip shows for a collapsed sidebar, Settings, and the
  /// libraries (`showsWindowTitleStrip`).
  static bool titleStripVisible({
    required bool sidebarCollapsed,
    required bool settingsVisible,
    required bool libraryVisible,
  }) =>
      sidebarCollapsed || settingsVisible || libraryVisible;
}

/// Which session surfaces the cache may evict. Port of
/// `RootView.pruneSurfaceCache`'s derived sets: live sessions are kept, and
/// selected + pre-warmed + project-panel sessions are protected from the
/// LRU sweep.
final class SurfaceCachePrunePlan {  const SurfaceCachePrunePlan({
    required this.liveIDs,
    required this.protectedIDs,
  });

  /// Sessions still live (from the session index + project-tree walk).
  final Set<String> liveIDs;

  /// Pre-warmed + right-panel sessions: never evicted by the LRU sweep.
  final Set<String> protectedIDs;
}

/// Derives the prune plan from the walked tree + panel state.
SurfaceCachePrunePlan surfaceCachePrunePlan({
  required Iterable<String> liveSessionIDs,
  required Iterable<String> prewarmedIDs,
  required Iterable<String> projectPanelSessionIDs,
}) =>
    SurfaceCachePrunePlan(
      liveIDs: liveSessionIDs.toSet(),
      protectedIDs: {...prewarmedIDs, ...projectPanelSessionIDs},
    );

/// The spinner frame for [elapsedSeconds] (port of
/// `TitlebarBrailleSpinner.frame(for:)`). Frames and interval come from
/// `spinnerFrames` / `spinnerInterval` in `../theme.dart`.
String brailleSpinnerFrame(double elapsedSeconds) {
  final index = (elapsedSeconds / spinnerInterval).floor();
  return spinnerFrames[index % spinnerFrames.length];
}

/// The titlebar activity button's glyph. Port of
/// `TitlebarActivityMenuButton`: a braille spinner while sessions are
/// actively working, a bell otherwise.
enum ActivityButtonGlyph { spinner, bell }

/// The titlebar activity button's badge dot: attention-orange when blockers
/// exist, unread-blue when only finished rows exist, none otherwise.
enum ActivityButtonBadge { none, unread, attention }

/// Glyph derivation for the activity button.
ActivityButtonGlyph activityButtonGlyph(ActivityMenuSessions sessions) =>
    sessions.jobs.isNotEmpty
        ? ActivityButtonGlyph.spinner
        : ActivityButtonGlyph.bell;

/// Badge derivation for the activity button.
ActivityButtonBadge activityButtonBadge(ActivityMenuSessions sessions) {
  if (sessions.blockers.isNotEmpty) return ActivityButtonBadge.attention;
  if (sessions.finished.isNotEmpty) return ActivityButtonBadge.unread;
  return ActivityButtonBadge.none;
}
