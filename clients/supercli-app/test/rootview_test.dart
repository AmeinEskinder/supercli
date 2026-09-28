/// Widget tests for the RootView shell wiring.
///
/// Verifies the gpuidart node tree [RootView.build] produces: titlebar
/// controls (sidebar toggle / activity / back / new-session), the activity
/// dropdown sections, the project panel's derived visibility, and sidebar
/// width styling. Pure logic lives in `rootview_logic_test.dart`.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/models.dart';
import 'package:supercli_app/screens/rootview.dart';
import 'package:supercli_app/screens/rootview_logic.dart';
import 'package:supercli_app/screens/sidebarview.dart';
import 'package:supercli_app/screens/terminalarea.dart';
import 'package:test/test.dart';

/// Collect all node ids in a UiNode tree via its JSON form.
Set<String> _ids(UiNode node) {
  final ids = <String>{};
  void walk(Map<String, Object?> json) {
    final id = json['id'];
    if (id is String) ids.add(id);
    final children = json['children'];
    if (children is List) {
      for (final child in children) {
        if (child is Map<String, Object?>) walk(child);
      }
    }
  }

  walk(node.toJson());
  return ids;
}

/// Find one node by id in a UiNode JSON tree.
Map<String, Object?> _find(Map<String, Object?> json, String id) {
  if (json['id'] == id) return json;
  final children = json['children'];
  if (children is List) {
    for (final child in children) {
      if (child is Map<String, Object?>) {
        final found = _find(child, id);
        if (found.isNotEmpty) return found;
      }
    }
  }
  return const {};
}

SidebarSession _panelSession(String id, String title) => SidebarSession(
      summary: SessionSummary(
        id: id,
        title: title,
        updatedAt: DateTime(2026, 9, 29, 12, 0),
      ),
    );

RootView _view({
  bool sidebarCollapsed = false,
  bool settingsVisible = false,
  bool recentActivityVisible = false,
  String? archivedProjectID,
  List<SidebarSession> projectPanelSessions = const [],
  ActivityMenuSessions? activityMenu,
}) =>
    RootView(
      sidebar: SidebarView(),
      content: TerminalArea(),
      sidebarCollapsed: sidebarCollapsed,
      settingsVisible: settingsVisible,
      recentActivityVisible: recentActivityVisible,
      archivedProjectID: archivedProjectID,
      projectPanelSessions: projectPanelSessions,
      activityMenu: activityMenu,
    );

ActivityMenuSessions _menu() => ActivityMenuSessions(
      renderedInTreeOrder: const [],
      allEntries: const [],
      jobs: [
        const ActivityMenuEntry(
          id: 'j1',
          title: 'job one',
          attention: false,
          createdAtMs: 1,
        ),
      ],
      finished: [
        const ActivityMenuEntry(
          id: 'f1',
          title: 'finished one',
          attention: false,
          createdAtMs: 2,
        ),
      ],
    );

void main() {
  group('RootView titlebar', () {
    test('open sidebar shows toggle + activity only', () {
      final ids = _ids(_view().build());
      expect(ids.contains('titlebar-sidebar-toggle'), isTrue);
      expect(ids.contains('titlebar-activity-menu'), isTrue);
      expect(ids.contains('titlebar-new-session'), isFalse);
      expect(ids.contains('titlebar-back'), isFalse);
    });

    test('collapsed sidebar shows the new-session control', () {
      final ids = _ids(_view(sidebarCollapsed: true).build());
      expect(ids.contains('titlebar-new-session'), isTrue);
      expect(ids.contains('titlebar-back'), isFalse);
    });

    test('collapsed with settings visible shows back instead', () {
      final ids = _ids(
        _view(sidebarCollapsed: true, settingsVisible: true).build(),
      );
      expect(ids.contains('titlebar-back'), isTrue);
      expect(ids.contains('titlebar-new-session'), isFalse);
    });

    test('collapsed with an archived project shows back', () {
      final ids = _ids(
        _view(sidebarCollapsed: true, archivedProjectID: 'proj1').build(),
      );
      expect(ids.contains('titlebar-back'), isTrue);
    });

    test('activity dropdown renders sections with a divider', () {
      final ids = _ids(_view(activityMenu: _menu()).build());
      expect(ids.contains('activity-menu-list'), isTrue);
      expect(ids.contains('activity-row-j1'), isTrue);
      expect(ids.contains('activity-row-f1'), isTrue);
      expect(ids.contains('activity-menu-divider-1'), isTrue);
    });

    test('activity button shows the spinner while jobs are active', () {
      final tree = _view(activityMenu: _menu()).build().toJson();
      final button = _find(tree, 'titlebar-activity-menu');
      // First braille frame at t=0 plus the unread badge for finished rows.
      expect(button['label'], '⠋ ●');
    });

    test('activity button shows the bell with no jobs', () {
      final menu = ActivityMenuSessions(
        renderedInTreeOrder: const [],
        allEntries: const [],
        jobs: const [],
        finished: const [],
      );
      final tree = _view(activityMenu: menu).build().toJson();
      expect(_find(tree, 'titlebar-activity-menu')['label'], '🔔');
    });

    test('all-recent footer appears when enabled', () {
      final ids = _ids(
        RootView(
          sidebar: SidebarView(),
          content: TerminalArea(),
          activityMenu: _menu(),
          showAllRecent: true,
        ).build(),
      );
      expect(ids.contains('activity-menu-all-recent'), isTrue);
    });

    test('blocked rows show the Blocked trailing verb', () {
      final menu = ActivityMenuSessions(
        renderedInTreeOrder: [
          const ActivityMenuEntry(
            id: 'b1',
            title: 'blocked one',
            attention: true,
            createdAtMs: 1,
          ),
        ],
        allEntries: [
          const ActivityMenuEntry(
            id: 'b1',
            title: 'blocked one',
            attention: true,
            createdAtMs: 1,
          ),
        ],
        jobs: const [],
        finished: const [],
      );
      final tree = _view(activityMenu: menu).build().toJson();
      expect(_find(tree, 'activity-trailing-b1')['text'], 'Blocked');
    });

    test('new-session menu lists presets and manage verbs', () {
      final ids = _ids(
        RootView(
          sidebar: SidebarView(),
          content: TerminalArea(),
          sidebarCollapsed: true,
          newSessionPresets: ['claude', 'codex'],
          showManagePlugins: true,
        ).build(),
      );
      expect(ids.contains('titlebar-new-session-menu'), isTrue);
      expect(ids.contains('new-session-preset-0'), isTrue);
      expect(ids.contains('new-session-preset-1'), isTrue);
      expect(ids.contains('new-session-manage-presets'), isTrue);
      expect(ids.contains('new-session-manage-plugins'), isTrue);
    });
  });

  group('RootView project panel', () {
    test('shown when it has members and settings are hidden', () {
      final ids = _ids(
        _view(
          projectPanelSessions: [_panelSession('s1', 'alpha')],
        ).build(),
      );
      expect(ids.contains('project-panel'), isTrue);
      expect(ids.contains('project-panel-s1'), isTrue);
    });

    test('hidden when empty', () {
      expect(_ids(_view().build()).contains('project-panel'), isFalse);
    });

    test('covered by full-content pages', () {
      final ids = _ids(
        _view(
          projectPanelSessions: [_panelSession('s1', 'alpha')],
          settingsVisible: true,
        ).build(),
      );
      expect(ids.contains('project-panel'), isFalse);
    });
  });

  group('RootView title strip', () {
    test('hidden when the sidebar is open and no full-content page', () {
      expect(_ids(_view().build()).contains('root-title-strip'), isFalse);
    });

    test('renders breadcrumb segments and the branch suffix', () {
      final tree = RootView(
        sidebar: SidebarView(),
        content: TerminalArea(),
        sidebarCollapsed: true,
        titleSegments: ['Home', 'myproj'],
        titleBranch: 'main',
        titleBranchIsWorktree: true,
        titleStripOpenInLabels: ['VS Code'],
      ).build().toJson();
      expect(
        _find(tree, 'title-strip-segments')['text'],
        'Home › myproj',
      );
      expect(_find(tree, 'title-strip-branch')['text'], '⑂ main');
      expect(_find(tree, 'title-strip-openin-0')['label'], 'Open in VS Code');
    });

    test('settings shows its own title', () {
      final tree = RootView(
        sidebar: SidebarView(),
        content: TerminalArea(),
        settingsVisible: true,
        settingsTitle: 'Settings',
      ).build().toJson();
      expect(_find(tree, 'title-strip-settings')['text'], 'Settings');
    });
  });

  group('RootView sidebar', () {
    test('collapsed sidebar renders the collapsed affordance', () {
      final ids = _ids(_view(sidebarCollapsed: true).build());
      expect(ids.contains('sidebar-collapsed'), isTrue);
      expect(ids.contains('sidebar-pane'), isFalse);
    });

    test('open sidebar renders the pane with the saved width', () {
      final view = _view();
      expect(view.shownSidebarWidth, rootSidebarDefaultWidth);
      expect(
        _ids(view.build()).contains('sidebar-pane'),
        isTrue,
      );
    });

    test('overlay mounts render when provided', () {
      final ids = _ids(
        RootView(
          sidebar: SidebarView(),
          content: TerminalArea(),
          commandPalette: const UiText('test-palette', 'palette'),
          toasts: const UiText('test-toasts', 'toasts'),
        ).build(),
      );
      expect(ids.contains('test-palette'), isTrue);
      expect(ids.contains('test-toasts'), isTrue);
    });
  });
}
