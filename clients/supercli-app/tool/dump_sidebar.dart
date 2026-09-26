/// Builds a sample sidebar and dumps its JSON snapshot for rendering.
library;

import 'dart:convert';
import 'dart:io';

import 'package:supercli_app/models.dart';
import 'package:supercli_app/screens/projectsidebarview.dart';
import 'package:supercli_app/screens/sidebarview.dart';

SessionSummary s(String id, String title,
        {int unread = 0, bool pinned = false}) =>
    SessionSummary(
      id: id,
      title: title,
      updatedAt: DateTime.utc(2026, 9, 26, 10),
      unreadCount: unread,
    );

void main() {
  final sidebar = SidebarView(
    workspaces: const ['main', 'side-project'],
    activeWorkspaceId: 'main',
    pinned: [
      SidebarSession(summary: s('p1', 'Release checklist'), pinned: true),
    ],
    projects: [
      SidebarProject(
        id: 'pr1',
        name: 'supercli',
        folderColor: '#3465a4',
        worktrees: const ['feature-sidebar'],
        groups: [
          SidebarGroup(
            id: 'g1',
            title: 'Backend',
            color: '#4e9a06',
            sessions: [
              SidebarSession(
                  summary: s('s1', 'Implement approvals API', unread: 2),
                  groupId: 'g1',
                  attention: true),
              SidebarSession(
                  summary: s('s2', 'Fix grant race'),
                  groupId: 'g1',
                  busy: true,
                  viewers: const ['Amein']),
            ],
          ),
          SidebarGroup(
            id: 'g2',
            title: 'Frontend',
            sessions: [
              SidebarSession(
                  summary: s('s3', 'Sidebar widgets'), groupId: 'g2'),
            ],
          ),
        ],
        sessions: [
          SidebarSession(
              summary: s('s4', 'Parity audit'),
              worktree: 'feature-sidebar',
              busy: true),
        ],
      ),
      SidebarProject(
        id: 'pr2',
        name: 'docs-site',
        sessions: [
          SidebarSession(summary: s('s5', 'Write migration guide')),
        ],
      ),
    ],
    selectedSessionId: 's1',
  );

  final sessionMenu = SessionContextMenu(sessionId: 's1');
  final projectMenu = ProjectContextMenu(projectId: 'pr1');
  final drag = SidebarSessionDrag(
    draggedSessionId: 's3',
    target: SidebarDropTarget.group,
    targetId: 'g1',
  );

  final snapshot = {
    'sidebar': sidebar.build().toJson(),
    'session_menu': sessionMenu.build().toJson(),
    'project_menu': projectMenu.build().toJson(),
    'drag_overlay': drag.build().toJson(),
  };
  final out = File('/tmp/sidebar-snapshot.json');
  out.writeAsStringSync(
      const JsonEncoder.withIndent('  ').convert(snapshot));
  print('wrote ${out.path}');
}
