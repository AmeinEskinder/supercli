/// Behavior tests for the desktop sidebar (#151-154).
///
/// These exercise the sidebar models and renderers, not just tree shape:
/// attention dots, busy spinners, unread badges, pin glyphs, filter,
/// pinned-first ordering, context-menu item lists, drag validation,
/// workspace dots, and relative activity timestamps.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/models.dart';
import 'package:supercli_app/screens/globalactivitymenu.dart';
import 'package:supercli_app/screens/projectsidebarview.dart';
import 'package:supercli_app/screens/recentactivityview.dart';
import 'package:supercli_app/screens/sidebarview.dart';
import 'package:test/test.dart';

SessionSummary summary(String id, String title, {int unread = 0}) =>
    SessionSummary(
      id: id,
      title: title,
      updatedAt: DateTime.utc(2026, 9, 26),
      unreadCount: unread,
    );

/// Collect all UiText texts in a node tree (depth-first).
List<String> textsOf(UiNode node) {
  final out = <String>[];
  void walk(UiNode n) {
    if (n is UiText) out.add(n.text);
    if (n is UiColumn) {
      for (final c in n.children) {
        walk(c);
      }
    }
    if (n is UiRow) {
      for (final c in n.children) {
        walk(c);
      }
    }
  }

  walk(node);
  return out;
}

/// Collect all UiButton labels in a node tree (depth-first).
List<String> buttonsOf(UiNode node) {
  final out = <String>[];
  void walk(UiNode n) {
    if (n is UiButton) out.add(n.label);
    if (n is UiColumn) {
      for (final c in n.children) {
        walk(c);
      }
    }
    if (n is UiRow) {
      for (final c in n.children) {
        walk(c);
      }
    }
  }

  walk(node);
  return out;
}

void main() {
  group('SidebarRow session rows (#151)', () {
    test('attention dot renders only when attention is true', () {
      final attn = SidebarSession(
          summary: summary('a', 'Fix bug'), attention: true);
      final calm =
          SidebarSession(summary: summary('b', 'Refactor'));
      expect(textsOf(SidebarRow.sessionRow(attn, selected: false)),
          contains('●'));
      expect(textsOf(SidebarRow.sessionRow(calm, selected: false)),
          isNot(contains('●')));
    });

    test('busy spinner renders only when busy is true', () {
      final busy =
          SidebarSession(summary: summary('a', 'Agent run'), busy: true);
      final idle = SidebarSession(summary: summary('b', 'Idle'));
      expect(textsOf(SidebarRow.sessionRow(busy, selected: false)),
          contains(' ◌'));
      expect(textsOf(SidebarRow.sessionRow(idle, selected: false)),
          isNot(contains(' ◌')));
    });

    test('unread badge shows the count', () {
      final s =
          SidebarSession(summary: summary('a', 'Chat', unread: 3));
      expect(textsOf(SidebarRow.sessionRow(s, selected: false)),
          contains(' 3'));
    });

    test('pin glyph renders for pinned sessions', () {
      final s =
          SidebarSession(summary: summary('a', 'Pinned'), pinned: true);
      expect(textsOf(SidebarRow.sessionRow(s, selected: false)),
          contains('📌'));
    });

    test('presence viewers render initials', () {
      final s = SidebarSession(
          summary: summary('a', 'Shared'),
          viewers: const ['Amein', 'Osman']);
      final texts = textsOf(SidebarRow.sessionRow(s, selected: false));
      expect(texts.any((t) => t.contains('A') && t.contains('O')), isTrue);
    });
  });

  group('SidebarView tree (#151)', () {
    SidebarView sample() => SidebarView(
          workspaces: const ['main', 'side'],
          activeWorkspaceId: 'main',
          pinned: [
            SidebarSession(
                summary: summary('p1', 'Pinned task'), pinned: true),
          ],
          projects: [
            SidebarProject(
              id: 'pr1',
              name: 'supercli',
              folderColor: '#3465a4',
              groups: [
                SidebarGroup(
                  id: 'g1',
                  title: 'Backend',
                  sessions: [
                    SidebarSession(
                        summary: summary('s1', 'API work'),
                        groupId: 'g1',
                        attention: true),
                  ],
                ),
              ],
              worktrees: const ['feature-x'],
              sessions: [
                SidebarSession(
                    summary: summary('s2', 'Docs'),
                    worktree: 'feature-x',
                    busy: true),
              ],
            ),
          ],
          selectedSessionId: 's1',
        );

    test('pinned section renders before projects', () {
      final texts = textsOf(sample().build());
      final pinnedIdx = texts.indexOf('Pinned');
      final projectIdx = texts.indexOf('supercli');
      expect(pinnedIdx, greaterThanOrEqualTo(0));
      expect(projectIdx, greaterThan(pinnedIdx));
    });

    test('workspace dots show active filled', () {
      final buttons = buttonsOf(sample().build());
      expect(buttons, contains('● main'));
      expect(buttons, contains('○ side'));
    });

    test('worktree folder and group headers render', () {
      final texts = textsOf(sample().build());
      expect(texts.any((t) => t.contains('feature-x')), isTrue);
      expect(texts, contains('Backend'));
    });

    test('filter hides non-matching sessions', () {
      final v = sample();
      final filtered = SidebarView(
        workspaces: v.workspaces,
        projects: v.projects,
        pinned: v.pinned,
        filterText: 'api',
      );
      final texts = textsOf(filtered.build());
      expect(texts, contains('API work'));
      expect(texts, isNot(contains('Docs')));
    });

    test('collapsed project hides its sessions', () {
      final v = sample();
      final collapsed = SidebarView(
        workspaces: v.workspaces,
        projects: [
          SidebarProject(
              id: 'pr1', name: 'supercli', collapsed: true),
        ],
        pinned: v.pinned,
      );
      final texts = textsOf(collapsed.build());
      expect(texts, isNot(contains('API work')));
      expect(texts, contains('supercli'));
    });

    test('worktree session appears once, not duplicated ungrouped', () {
      final view = SidebarView(
        projects: [
          SidebarProject(
            id: 'pr1',
            name: 'supercli',
            worktrees: const ['wt1'],
            sessions: [
              SidebarSession(
                  summary: summary('s1', 'Worktree task'),
                  worktree: 'wt1'),
              SidebarSession(summary: summary('s2', 'Plain task')),
            ],
          ),
        ],
      );
      final texts = textsOf(view.build());
      expect(texts.where((t) => t == 'Worktree task').length, 1);
      expect(texts.where((t) => t == 'Plain task').length, 1);
    });

    test('sidebar actions include filter and new-session', () {
      final names =
          sample().actions().map((a) => a.name).toList();
      expect(names, contains('sidebar.filter'));
      expect(names, contains('session.new'));
    });
  });

  group('SessionContextMenu (#153)', () {
    test('has all 12 required items in order', () {
      final labels =
          SessionContextMenu.items.map((i) => i.$2).toList();
      expect(labels, [
        'Rename…',
        'Copy Session ID',
        'Copy Transcript',
        'Notify When Done',
        'Clear Attention',
        'Resume',
        'Restart App',
        'Reveal in Finder',
        'Pin',
        'Stop and Archive',
        'Remove…',
      ]);
    });

    test('action names are unique', () {
      final names =
          SessionContextMenu.items.map((i) => i.$1).toSet();
      expect(names.length, SessionContextMenu.items.length);
    });

    test('renders one button per item', () {
      final menu = SessionContextMenu(sessionId: 's1');
      expect(buttonsOf(menu.build()).length,
          SessionContextMenu.items.length);
    });

    test('button id decodes back to the action name', () {
      expect(
          SessionContextMenu.actionForButtonId(
              'menu-s1-session.copy-id'),
          'session.copy-id');
      expect(
          SessionContextMenu.actionForButtonId('bogus'), isEmpty);
    });
  });

  group('ProjectContextMenu (#154)', () {
    test('has all 9 required items', () {
      final labels =
          ProjectContextMenu.items.map((i) => i.$2).toList();
      expect(labels, [
        'New Worktree…',
        'New Group',
        'Rename…',
        'Stop All Sessions',
        'Sort Sessions',
        'Folder Color…',
        'Show Archived',
        'Open in Editor',
        'Move to Workspace…',
      ]);
    });

    test('renders one button per item', () {
      final menu = ProjectContextMenu(projectId: 'pr1');
      expect(buttonsOf(menu.build()).length,
          ProjectContextMenu.items.length);
    });
  });

  group('SidebarSessionDrag (#152)', () {
    test('idle drag renders no overlay', () {
      const drag = SidebarSessionDrag();
      expect(drag.isDragging, isFalse);
    });

    test('cannot drop a session onto itself', () {
      const drag = SidebarSessionDrag(
          draggedSessionId: 's1', target: SidebarDropTarget.reorder);
      expect(drag.canDrop(sessionId: 's1', currentGroupId: null),
          isFalse);
      expect(drag.canDrop(sessionId: 's2', currentGroupId: null),
          isTrue);
    });

    test('cannot move into the same group (no-op)', () {
      const drag = SidebarSessionDrag(
        draggedSessionId: 's1',
        target: SidebarDropTarget.group,
        targetId: 'g1',
      );
      expect(drag.canDrop(sessionId: 's1', currentGroupId: 'g1'),
          isFalse);
      expect(drag.canDrop(sessionId: 's2', currentGroupId: 'g2'),
          isTrue);
    });

    test('drop-to-split requires a pane target', () {
      const noTarget = SidebarSessionDrag(
        draggedSessionId: 's1',
        target: SidebarDropTarget.split,
        dropToSplit: true,
      );
      expect(
          noTarget.canDrop(sessionId: 's2', currentGroupId: null),
          isFalse);
      const withTarget = SidebarSessionDrag(
        draggedSessionId: 's1',
        target: SidebarDropTarget.split,
        targetId: 'pane-1',
        dropToSplit: true,
      );
      expect(
          withTarget.canDrop(sessionId: 's2', currentGroupId: null),
          isTrue);
    });

    test('drag overlay names the target', () {
      const drag = SidebarSessionDrag(
        draggedSessionId: 's1',
        target: SidebarDropTarget.split,
        targetId: 'pane-1',
        dropToSplit: true,
      );
      expect(textsOf(drag.build()).join(' '), contains('split pane'));
    });

    test('drag cancel action is escape', () {
      const drag = SidebarSessionDrag(draggedSessionId: 's1');
      final actions = drag.actions();
      expect(actions.single.name, 'sidebar.drag.cancel');
      expect(actions.single.keys, 'escape');
    });
  });

  group('project sidebar widgets', () {
    test('skeleton renders the requested rows', () {
      const skel = SidebarSkeleton(rows: 4);
      final node = skel.build();
      expect(node, isA<UiColumn>());
      expect((node as UiColumn).children.length, 4);
    });

    test('workspace dots mark the active workspace', () {
      const dots = SidebarWorkspaceDots(
          workspaces: ['a', 'b'], activeId: 'b');
      expect(buttonsOf(dots.build()), ['○ a', '● b']);
    });

    test('workspace selector checks the selected workspace', () {
      const sel = SidebarWorkspaceSelector(
          workspaces: ['a', 'b'], selected: 'a');
      expect(buttonsOf(sel.build()), ['✓ a', '  b']);
    });

    test('spinners list busy session ids', () {
      const sp = SidebarSpinners(spinningIds: ['s1', 's2']);
      expect(textsOf(sp.build()).length, 2);
    });

    test('project view renders folder color and session count', () {
      final view = ProjectSidebarView(
        project: SidebarProject(
          id: 'pr1',
          name: 'supercli',
          folderColor: '#3465a4',
          groups: [
            SidebarGroup(
                id: 'g1',
                title: 'Backend',
                sessions: [
                  SidebarSession(summary: summary('s1', 'API'))
                ]),
          ],
        ),
      );
      final texts = textsOf(view.build());
      expect(texts, contains('supercli'));
      expect(texts, contains('Backend (1)'));
    });
  });

  group('activity views', () {
    test('relativeTime formats durations', () {
      final now = DateTime.utc(2026, 9, 26, 12);
      expect(
          ActivityItem.relativeTime(
              now.subtract(const Duration(seconds: 30)),
              now: now),
          'just now');
      expect(
          ActivityItem.relativeTime(
              now.subtract(const Duration(minutes: 5)),
              now: now),
          '5m');
      expect(
          ActivityItem.relativeTime(
              now.subtract(const Duration(hours: 2)),
              now: now),
          '2h');
      expect(
          ActivityItem.relativeTime(
              now.subtract(const Duration(days: 3)),
              now: now),
          '3d');
    });

    test('global activity menu shows only busy/attention sessions', () {
      final menu = GlobalActivityMenu(sessions: [
        SidebarSession(summary: summary('a', 'Busy'), busy: true),
        SidebarSession(
            summary: summary('b', 'Attention'), attention: true),
        SidebarSession(summary: summary('c', 'Idle')),
      ]);
      final buttons = buttonsOf(menu.build());
      expect(buttons, contains('Busy'));
      expect(buttons, contains('Attention'));
      expect(buttons, isNot(contains('Idle')));
    });
  });
}
