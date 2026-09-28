/// App shell: titlebar, resizable sidebar, content area, project panel.
///
/// Port of `RootView.swift` (SupercliNative/Views): the window chrome around
/// the workspace — the titlebar button row, the collapsible sidebar, the
/// terminal content area, and the right project panel. All geometry and
/// control-visibility decisions live in `rootview_logic.dart`; this file only
/// builds the gpuidart node tree.
///
/// The Swift version uses SwiftUI with `@AppStorage` width persistence and a
/// detached session-drag overlay. Drag-resize math is ported in
/// [RootSidebarGeometry]; the caller owns persistence (via
/// [rootSidebarWidthKey] / [rootProjectSidebarWidthsKey]) and feeds the
/// live/drag widths back in.
library;

import 'package:gpuidart/gpuidart.dart';

import '../keymap.dart';
import 'rootview_logic.dart';
import 'sidebarview.dart';
import 'terminalarea.dart';

/// The root application layout: titlebar + sidebar + content + project panel.
final class RootView {
  RootView({
    required this.sidebar,
    required this.content,
    this.sidebarCollapsed = false,
    this.settingsVisible = false,
    this.recentActivityVisible = false,
    this.archivedProjectID,
    this.libraryVisible = false,
    this.windowIsFullScreen = false,
    this.savedSidebarWidth = rootSidebarDefaultWidth,
    this.draggingSidebarWidth,
    this.projectPanelSessions = const [],
    this.activeRootProjectID,
    this.perProjectPanelWidths = const {},
    this.sharedProjectPanelWidth = rootSidebarDefaultWidth,
    this.draggingProjectPanelWidth,
    this.activityMenu,
    this.spinnerElapsedSeconds = 0,
    this.showAllRecent = false,
    this.newSessionPresets = const [],
    this.showManagePlugins = false,
    this.titleSegments = const [],
    this.titleBranch,
    this.titleBranchIsWorktree = false,
    this.titleStripOpenInLabels = const [],
    this.settingsTitle,
    this.sessionSwitcher,
    this.commandPalette,
    this.toasts,
  });

  final SidebarView sidebar;
  final TerminalArea content;
  final bool sidebarCollapsed;
  final bool settingsVisible;
  final bool recentActivityVisible;
  final String? archivedProjectID;
  final bool libraryVisible;
  final bool windowIsFullScreen;

  /// Persisted sidebar width (`supercli.sidebar.width`).
  final double savedSidebarWidth;

  /// Live width while the sidebar resizer is being dragged.
  final double? draggingSidebarWidth;

  /// Sessions shown in the right project panel.
  final List<SidebarSession> projectPanelSessions;

  /// Project key for the per-project panel width (`activeRootProjectID`).
  final String? activeRootProjectID;

  /// Per-project panel widths (`supercli.projectSidebar.widths`).
  final Map<String, double> perProjectPanelWidths;

  /// Shared panel width fallback for never-resized projects.
  final double sharedProjectPanelWidth;

  /// Live panel width while its resizer is being dragged.
  final double? draggingProjectPanelWidth;

  /// The titlebar activity-menu projection, if the menu is open.
  final ActivityMenuSessions? activityMenu;

  /// Elapsed seconds driving the braille spinner frames.
  final double spinnerElapsedSeconds;

  /// Whether the activity dropdown shows the "All recent ›" footer link.
  final bool showAllRecent;

  /// Preset labels for the collapsed new-session menu.
  final List<String> newSessionPresets;

  /// Whether the new-session menu offers "Manage plugins".
  final bool showManagePlugins;

  /// Breadcrumb segments for the title strip (`TitleBarView.segments`).
  final List<String> titleSegments;

  /// Branch suffix shown after the breadcrumb (empty = none).
  final String? titleBranch;

  /// Whether the branch suffix uses the worktree icon.
  final bool titleBranchIsWorktree;

  /// Open-in/local-site chip labels at the strip's trailing edge
  /// (local-machine scopes only).
  final List<String> titleStripOpenInLabels;

  /// Settings title text (the `SettingsTitleStrip` content is owned by the
  /// settings port; this carries its title string).
  final String? settingsTitle;

  /// Overlay mounts (⌃Tab MRU switcher, ⌘K command palette, transient
  /// toasts). The components are separate ports; RootView only mounts them,
  /// switcher < palette < toasts like the Swift z-order.
  final UiNode? sessionSwitcher;
  final UiNode? commandPalette;
  final UiNode? toasts;

  /// Derived laid-out sidebar width (0 when collapsed).
  double get shownSidebarWidth => RootSidebarGeometry.shownSidebarWidth(
        collapsed: sidebarCollapsed,
        width: draggingSidebarWidth ?? savedSidebarWidth,
      );

  /// Derived project-panel visibility: non-empty membership, covered by
  /// full-content pages.
  bool get projectPanelShown => RootSidebarGeometry.projectSidebarShown(
        hasMembers: projectPanelSessions.isNotEmpty,
        settingsVisible: settingsVisible,
      );

  /// Derived project-panel width: drag value, then per-project, then shared.
  double get projectPanelWidth => RootSidebarGeometry.projectSidebarWidth(
        perProjectWidths: perProjectPanelWidths,
        projectKey: activeRootProjectID ?? 'default',
        sharedWidth: sharedProjectPanelWidth,
        draggingWidth: draggingProjectPanelWidth,
      );

  /// Titlebar controls in layout order.
  List<RootTitlebarControl> get titlebarControls =>
      RootViewTitlebar.controls(
        sidebarCollapsed: sidebarCollapsed,
        settingsVisible: settingsVisible,
        recentActivityVisible: recentActivityVisible,
        archivedProjectID: archivedProjectID,
      );

  UiNode build() {
    return UiColumn('root-view', [
      if (RootViewTitlebar.titleStripVisible(
        sidebarCollapsed: sidebarCollapsed,
        settingsVisible: settingsVisible,
        libraryVisible: libraryVisible,
      ))
        _titleStrip(),
      _titlebar(),
      UiRow('root-layout', [
        if (!sidebarCollapsed)
          UiColumn('sidebar-pane', [sidebar.build()],
              style: UiStyle(width: UiSize.px(shownSidebarWidth)))
        else
          UiColumn('sidebar-collapsed', [
            const UiButton('expand-sidebar', 'Show sidebar'),
            const UiButton('new-session-collapsed', 'New session'),
          ]),
        UiColumn('content-area', [
          content.build(),
        ]),
        if (projectPanelShown) _projectPanel(),
      ]),
      // Overlay mounts: switcher < palette < toasts (Swift z-order).
      if (sessionSwitcher != null) sessionSwitcher!,
      if (commandPalette != null) commandPalette!,
      if (toasts != null) toasts!,
    ]);
  }

  /// Port of `WindowTitleStrip` / `WorkspaceTitleStrip` (`TitleBarView`):
  /// centered breadcrumb segments with `›` separators, the branch suffix,
  /// and trailing Open-in chips. Settings shows its own title.
  UiNode _titleStrip() {
    final settings = settingsTitle;
    final children = <UiNode>[
      if (settingsVisible && settings != null)
        UiText('title-strip-settings', settings,
            style: const UiStyle(fontSize: 13))
      else ...[
        UiText(
          'title-strip-segments',
          titleSegments.join(' › '),
          style: const UiStyle(fontSize: 13),
        ),
        if (titleBranch != null && titleBranch!.isNotEmpty)
          UiText(
            'title-strip-branch',
            '${titleBranchIsWorktree ? '⑂' : '⎇'} $titleBranch',
            style: const UiStyle(fontSize: 12),
          ),
      ],
      for (var i = 0; i < titleStripOpenInLabels.length; i++)
        UiButton(
            'title-strip-openin-$i', 'Open in ${titleStripOpenInLabels[i]}'),
    ];
    return UiRow('root-title-strip', children,
        style: UiStyle(height: UiSize.px(rootTitleStripHeight), gap: 5));
  }

  UiNode _titlebar() {    final offsetX = RootViewTitlebar.offsetX(
      windowIsFullScreen: windowIsFullScreen,
    );
    return UiRow('root-titlebar', [
      for (final control in titlebarControls)
        switch (control) {
          RootTitlebarControl.sidebarToggle => UiButton(
              'titlebar-sidebar-toggle',
              sidebarCollapsed ? 'Show sidebar (⌘B)' : 'Hide sidebar (⌘B)',
            ),
          RootTitlebarControl.activityMenu => _activityButton(),
          RootTitlebarControl.back =>
            const UiButton('titlebar-back', 'Back to workspace'),
          RootTitlebarControl.newSession => _newSessionMenu(),
        },
      if (activityMenu != null) _activityMenuList(activityMenu!),
    ], style: UiStyle(padding: [0, 0, 0, offsetX], gap: 4));
  }

  /// Port of `TitlebarActivityMenuButton`'s label: braille spinner while
  /// jobs are active, bell otherwise, with the unread/attention badge dot.
  UiNode _activityButton() {
    final sessions = activityMenu;
    final glyph = sessions == null
        ? ActivityButtonGlyph.bell
        : activityButtonGlyph(sessions);
    final badge = sessions == null
        ? ActivityButtonBadge.none
        : activityButtonBadge(sessions);
    final label = StringBuffer(
      glyph == ActivityButtonGlyph.spinner
          ? brailleSpinnerFrame(spinnerElapsedSeconds)
          : '🔔',
    );
    if (badge != ActivityButtonBadge.none) label.write(' ●');
    return UiButton(
      'titlebar-activity-menu',
      label.toString(),
    );
  }

  /// Port of `TitlebarNewSessionMenu`: preset items plus the manage verbs.
  UiNode _newSessionMenu() {
    return UiColumn('titlebar-new-session-menu', [
      const UiButton('titlebar-new-session', '+ New session'),
      for (var i = 0; i < newSessionPresets.length; i++)
        UiButton('new-session-preset-$i', newSessionPresets[i]),
      const UiButton('new-session-manage-presets', 'Manage presets'),
      if (showManagePlugins)
        const UiButton('new-session-manage-plugins', 'Manage plugins'),
    ]);
  }

  /// Port of `ActivityMenuList` + `TitlebarActivityMenuRow`: blockers first,
  /// then jobs, then finished, with dividers between non-empty sections, the
  /// deterministic scroll height (42pt rows, 9pt dividers, capped at 429pt),
  /// and the "All recent ›" footer link.
  UiNode _activityMenuList(ActivityMenuSessions sessions) {
    final sections = [
      ('blockers', sessions.blockers, true),
      ('jobs', sessions.jobs, false),
      ('finished', sessions.finished, false),
    ].where((section) => section.$2.isNotEmpty).toList();
    final height = activityMenuScrollHeight(
      rowCount: sessions.rowCount,
      sectionCount: sessions.sectionCount,
    );
    return UiColumn('activity-menu-list', [
      for (var i = 0; i < sections.length; i++) ...[
        if (i > 0) UiText('activity-menu-divider-$i', '─'),
        for (final entry in sections[i].$2)
          _activityMenuRow(entry, blocked: sections[i].$3),
      ],
      if (showAllRecent)
        const UiButton('activity-menu-all-recent', 'All recent ›'),
    ], style: UiStyle(height: UiSize.px(height)));
  }

  /// Port of `TitlebarActivityMenuRow`: leading status glyph, title over the
  /// workspace › path subtitle (or "Alert · <body>"), and the trailing
  /// Blocked/Alert/logo/status verb.
  UiNode _activityMenuRow(ActivityMenuEntry entry, {required bool blocked}) {
    final leading = entry.working
        ? brailleSpinnerFrame(spinnerElapsedSeconds)
        : blocked
            ? '●'
            : entry.unread
                ? '●'
                : '▸';
    final subtitle = entry.alertBody != null && entry.alertBody!.isNotEmpty
        ? 'Alert · ${entry.alertBody}'
        : '${entry.workspace} › ${entry.projectPath}';
    final trailing = blocked
        ? 'Blocked'
        : entry.unread && entry.alertBody != null
            ? 'Alert'
            : entry.working || entry.unread
                ? '▸'
                : (entry.status == 'Starting' ||
                        entry.status == 'Restarting' ||
                        entry.status == 'Resuming')
                    ? entry.status
                    : '';
    return UiRow('activity-row-${entry.id}', [
      UiText('activity-leading-${entry.id}', leading,
          style: const UiStyle(fontSize: 11)),
      UiColumn('activity-text-${entry.id}', [
        UiText('activity-title-${entry.id}', entry.title,
            style: const UiStyle(fontSize: 13)),
        UiText('activity-subtitle-${entry.id}', subtitle,
            style: const UiStyle(fontSize: 11)),
      ]),
      if (trailing.isNotEmpty)
        UiText('activity-trailing-${entry.id}', trailing,
            style: const UiStyle(fontSize: 11)),
    ], style: const UiStyle(gap: 8));
  }

  UiNode _projectPanel() {
    return UiColumn('project-panel', [
      for (final session in projectPanelSessions)
        UiButton('project-panel-${session.id}', session.displayTitle),
    ], style: UiStyle(width: UiSize.px(projectPanelWidth)));
  }

  List<UiAction> actions() => [
        UiAction(name: 'sidebar.toggle', keys: Keymap.sidebarToggle()),
      ];
}
