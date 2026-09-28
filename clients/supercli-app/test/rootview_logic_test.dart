/// Behavior tests for the RootView state logic port.
///
/// Covers the sidebar/project-panel geometry, the activity-menu projection
/// (`ActivityMenuSessions`), dropdown sizing, titlebar controls, and the
/// surface-cache prune plan — all ported from `RootView.swift`.
library;

import 'package:supercli_app/screens/rootview_logic.dart';
import 'package:test/test.dart';

ActivityMenuEntry entry(
  String id, {
  String title = '',
  bool attention = false,
  int createdAtMs = 0,
  int? lifecycleAtMs,
}) =>
    ActivityMenuEntry(
      id: id,
      title: title.isEmpty ? id : title,
      attention: attention,
      createdAtMs: createdAtMs,
      lifecycleAtMs: lifecycleAtMs,
    );

void main() {
  group('RootSidebarGeometry', () {
    test('clampWidth enforces 220–520', () {
      expect(RootSidebarGeometry.clampWidth(100), 220);
      expect(RootSidebarGeometry.clampWidth(600), 520);
      expect(RootSidebarGeometry.clampWidth(300), 300);
      expect(RootSidebarGeometry.clampWidth(220), 220);
      expect(RootSidebarGeometry.clampWidth(520), 520);
    });

    test('dragWidth adds translation and clamps', () {
      expect(
        RootSidebarGeometry.dragWidth(startWidth: 300, translation: 50),
        350,
      );
      expect(
        RootSidebarGeometry.dragWidth(startWidth: 300, translation: -200),
        220,
      );
      // Mirrored (project panel): dragging left grows the panel.
      expect(
        RootSidebarGeometry.dragWidth(
          startWidth: 300,
          translation: -50,
          mirrored: true,
        ),
        350,
      );
      expect(
        RootSidebarGeometry.dragWidth(
          startWidth: 300,
          translation: 50,
          mirrored: true,
        ),
        250,
      );
    });

    test('shownSidebarWidth is 0 when collapsed', () {
      expect(
        RootSidebarGeometry.shownSidebarWidth(
          collapsed: true,
          width: 300,
        ),
        0,
      );
      expect(
        RootSidebarGeometry.shownSidebarWidth(
          collapsed: false,
          width: 300,
        ),
        300,
      );
    });

    test('projectSidebarWidth prefers drag, then per-project, then shared', () {
      expect(
        RootSidebarGeometry.projectSidebarWidth(
          perProjectWidths: {'p1': 250},
          projectKey: 'p1',
          sharedWidth: 300,
          draggingWidth: 400,
        ),
        400,
      );
      expect(
        RootSidebarGeometry.projectSidebarWidth(
          perProjectWidths: {'p1': 250},
          projectKey: 'p1',
          sharedWidth: 300,
        ),
        250,
      );
      // Never-resized project falls back to the shared key.
      expect(
        RootSidebarGeometry.projectSidebarWidth(
          perProjectWidths: const {},
          projectKey: 'p9',
          sharedWidth: 300,
        ),
        300,
      );
    });

    test('projectSidebarShown is derived, never toggled', () {
      expect(
        RootSidebarGeometry.projectSidebarShown(
          hasMembers: true,
          settingsVisible: false,
        ),
        isTrue,
      );
      expect(
        RootSidebarGeometry.projectSidebarShown(
          hasMembers: false,
          settingsVisible: false,
        ),
        isFalse,
      );
      // Full-content pages cover the workspace.
      expect(
        RootSidebarGeometry.projectSidebarShown(
          hasMembers: true,
          settingsVisible: true,
        ),
        isFalse,
      );
    });

    test('persistence keys match the Swift @AppStorage keys', () {
      expect(rootSidebarWidthKey, 'supercli.sidebar.width');
      expect(rootProjectSidebarWidthKey, 'supercli.projectSidebar.width');
      expect(rootProjectSidebarWidthsKey, 'supercli.projectSidebar.widths');
    });
  });

  group('ActivityMenuSessions', () {
    test('attention rows win their own section first', () {
      final sessions = ActivityMenuSessions(
        renderedInTreeOrder: [
          entry('a', attention: true),
          entry('b'),
        ],
        allEntries: [entry('a', attention: true), entry('b')],
        jobs: [entry('a'), entry('b')],
        finished: [entry('c')],
      );
      // 'a' appears only as a blocker, never as a job.
      expect(sessions.blockers.map((e) => e.id), ['a']);
      expect(sessions.jobs.map((e) => e.id), ['b']);
      expect(sessions.finished.map((e) => e.id), ['c']);
      expect(sessions.sectionCount, 3);
    });

    test('orphan blockers follow tree order, sorted by stamp then id', () {
      final sessions = ActivityMenuSessions(
        renderedInTreeOrder: [entry('tree', attention: true, createdAtMs: 1)],
        allEntries: [
          entry('tree', attention: true, createdAtMs: 1),
          entry('o1', attention: true, createdAtMs: 100),
          entry('o2', attention: true, createdAtMs: 300),
          entry('o3',
              attention: true, createdAtMs: 50, lifecycleAtMs: 400),
        ],
        jobs: const [],
        finished: const [],
      );
      // Tree order first; orphans by max(createdAt, lifecycleAt) desc.
      expect(
        sessions.blockers.map((e) => e.id).toList(),
        ['tree', 'o3', 'o2', 'o1'],
      );
    });

    test('orphan tie-break is id ascending', () {
      final sessions = ActivityMenuSessions(
        renderedInTreeOrder: const [],
        allEntries: [
          entry('b', attention: true, createdAtMs: 10),
          entry('a', attention: true, createdAtMs: 10),
        ],
        jobs: const [],
        finished: const [],
      );
      expect(sessions.blockers.map((e) => e.id).toList(), ['a', 'b']);
    });

    test('finished excludes blockers and jobs', () {
      final sessions = ActivityMenuSessions(
        renderedInTreeOrder: [entry('a', attention: true)],
        allEntries: [entry('a', attention: true)],
        jobs: [entry('b')],
        finished: [entry('a'), entry('b'), entry('c')],
      );
      expect(sessions.finished.map((e) => e.id), ['c']);
      expect(sessions.sectionCount, 3);
    });

    test('empty groups do not count as sections', () {
      final sessions = ActivityMenuSessions(
        renderedInTreeOrder: const [],
        allEntries: const [],
        jobs: [entry('b')],
        finished: const [],
      );
      expect(sessions.sectionCount, 1);
      expect(sessions.rowCount, 1);
    });

    test('duplicate ids are uniqued', () {
      final sessions = ActivityMenuSessions(
        renderedInTreeOrder: const [],
        allEntries: const [],
        jobs: [entry('b'), entry('b')],
        finished: const [],
      );
      expect(sessions.jobs.map((e) => e.id), ['b']);
    });
  });

  group('activityMenuScrollHeight', () {
    test('empty list reserves 44pt', () {
      expect(
        activityMenuScrollHeight(rowCount: 0, sectionCount: 0),
        44,
      );
    });

    test('rows are 42pt with 9pt dividers', () {
      expect(
        activityMenuScrollHeight(rowCount: 3, sectionCount: 2),
        3 * 42 + 9,
      );
      expect(
        activityMenuScrollHeight(rowCount: 1, sectionCount: 1),
        42,
      );
    });

    test('caps at 429pt', () {
      expect(
        activityMenuScrollHeight(rowCount: 20, sectionCount: 3),
        429,
      );
    });
  });

  group('RootViewTitlebar', () {
    test('open sidebar shows toggle + activity only', () {
      expect(
        RootViewTitlebar.controls(
          sidebarCollapsed: false,
          settingsVisible: false,
          recentActivityVisible: false,
        ),
        [
          RootTitlebarControl.sidebarToggle,
          RootTitlebarControl.activityMenu,
        ],
      );
    });

    test('collapsed sidebar shows the new-session control', () {
      expect(
        RootViewTitlebar.controls(
          sidebarCollapsed: true,
          settingsVisible: false,
          recentActivityVisible: false,
        ),
        [
          RootTitlebarControl.sidebarToggle,
          RootTitlebarControl.activityMenu,
          RootTitlebarControl.newSession,
        ],
      );
    });

    test('collapsed with a full-content page shows back instead', () {
      for (final args in [
        (settings: true, recent: false, archived: null),
        (settings: false, recent: true, archived: null),
        (settings: false, recent: false, archived: 'proj1'),
      ]) {
        expect(
          RootViewTitlebar.controls(
            sidebarCollapsed: true,
            settingsVisible: args.settings,
            recentActivityVisible: args.recent,
            archivedProjectID: args.archived,
          ),
          [
            RootTitlebarControl.sidebarToggle,
            RootTitlebarControl.activityMenu,
            RootTitlebarControl.back,
          ],
        );
      }
    });

    test('offsetX clears the traffic lights', () {
      expect(
        RootViewTitlebar.offsetX(windowIsFullScreen: false),
        80,
      );
      expect(
        RootViewTitlebar.offsetX(windowIsFullScreen: true),
        12,
      );
    });

    test('titleStripVisible for collapsed, settings, libraries', () {
      expect(
        RootViewTitlebar.titleStripVisible(
          sidebarCollapsed: true,
          settingsVisible: false,
          libraryVisible: false,
        ),
        isTrue,
      );
      expect(
        RootViewTitlebar.titleStripVisible(
          sidebarCollapsed: false,
          settingsVisible: true,
          libraryVisible: false,
        ),
        isTrue,
      );
      expect(
        RootViewTitlebar.titleStripVisible(
          sidebarCollapsed: false,
          settingsVisible: false,
          libraryVisible: true,
        ),
        isTrue,
      );
      expect(
        RootViewTitlebar.titleStripVisible(
          sidebarCollapsed: false,
          settingsVisible: false,
          libraryVisible: false,
        ),
        isFalse,
      );
    });
  });

  group('surfaceCachePrunePlan', () {
    test('protects pre-warmed and panel sessions', () {
      final plan = surfaceCachePrunePlan(
        liveSessionIDs: ['s1', 's2', 's3'],
        prewarmedIDs: ['s2'],
        projectPanelSessionIDs: ['s3'],
      );
      expect(plan.liveIDs, {'s1', 's2', 's3'});
      expect(plan.protectedIDs, {'s2', 's3'});
    });
  });

  group('brailleSpinnerFrame', () {
    test('cycles through the ten Theme frames every 0.12s', () {
      expect(brailleSpinnerFrames.length, 10);
      expect(brailleSpinnerIntervalSeconds, 0.12);
      expect(brailleSpinnerFrame(0), '⠋');
      expect(brailleSpinnerFrame(0.12), '⠙');
      expect(brailleSpinnerFrame(0.06), '⠋');
      // Wraps: 2.46s / 0.12 = 20.5 frames -> index 20 -> frame 0.
      expect(brailleSpinnerFrame(2.46), '⠋');
      // 1.19s / 0.12 = 9.91 frames -> index 9 -> last frame.
      expect(brailleSpinnerFrame(1.19), '⠏');
    });
  });

  group('activity button glyph and badge', () {
    ActivityMenuSessions menu({
      List<ActivityMenuEntry> jobs = const [],
      List<ActivityMenuEntry> finished = const [],
      List<ActivityMenuEntry> blockers = const [],
    }) =>
        ActivityMenuSessions(
          renderedInTreeOrder: blockers,
          allEntries: blockers,
          jobs: jobs,
          finished: finished,
        );

    test('spinner while jobs are active, bell otherwise', () {
      expect(
        activityButtonGlyph(menu(jobs: [entry('j')])),
        ActivityButtonGlyph.spinner,
      );
      expect(
        activityButtonGlyph(menu()),
        ActivityButtonGlyph.bell,
      );
    });

    test('badge is attention with blockers, unread with finished only', () {
      expect(
        activityButtonBadge(
            menu(blockers: [entry('b', attention: true)])),
        ActivityButtonBadge.attention,
      );
      expect(
        activityButtonBadge(menu(finished: [entry('f')])),
        ActivityButtonBadge.unread,
      );
      expect(activityButtonBadge(menu()), ActivityButtonBadge.none);
      // Blockers win over finished.
      expect(
        activityButtonBadge(menu(
          blockers: [entry('b', attention: true)],
          finished: [entry('f')],
        )),
        ActivityButtonBadge.attention,
      );
    });
  });
}
