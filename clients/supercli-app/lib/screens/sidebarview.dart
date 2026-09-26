/// Session sidebar: project/session tree with pins, groups, worktree folders,
/// attention dots and busy spinners.
///
/// Port of `SidebarView.swift` (SupercliNative/Views, 4406 lines). The Swift
/// version has: session rows with presence avatars, workspace groups,
/// drag-and-drop reordering ("Dia feel"), context menus, unread badges,
/// and a filter field.
///
/// gpuidart rendering: there is no native list/tree node upstream, so rows
/// render through the UiRow/UiText fallback pattern (same as the terminal
/// RLE fallback in lib/terminal/terminal_pane.dart). Per-row tap, drag
/// reorder and popup context menus need framework APIs that do not exist
/// yet — see docs/gpuidart-gaps-sidebar.md. Until then, rows are static
/// and interactions travel as [UiAction]s scoped to node IDs.
library;

import 'package:gpuidart/gpuidart.dart';

import '../models.dart';

/// A session as the sidebar sees it: the host summary plus sidebar-local
/// state (pin, group, worktree, attention, busy).
final class SidebarSession {
  const SidebarSession({
    required this.summary,
    this.projectId = '',
    this.groupId,
    this.worktree,
    this.pinned = false,
    this.attention = false,
    this.busy = false,
    this.viewers = const [],
  });

  final SessionSummary summary;
  final String projectId;

  /// Group inside the project, or null for the ungrouped list.
  final String? groupId;

  /// Worktree folder this session belongs to, or null.
  final String? worktree;

  final bool pinned;
  final bool attention;
  final bool busy;
  final List<String> viewers;

  String get id => summary.id;
  String get title => summary.title;
  int get unreadCount => summary.unreadCount;
}

/// A named group of sessions inside a project.
final class SidebarGroup {
  const SidebarGroup({
    required this.id,
    required this.title,
    this.color,
    this.collapsed = false,
    this.sessions = const [],
  });

  final String id;
  final String title;

  /// Folder color as #RRGGBB, or null for default.
  final String? color;
  final bool collapsed;
  final List<SidebarSession> sessions;
}

/// A project: top-level sidebar node with groups, worktree folders and
/// ungrouped sessions.
final class SidebarProject {
  const SidebarProject({
    required this.id,
    required this.name,
    this.folderColor,
    this.collapsed = false,
    this.groups = const [],
    this.sessions = const [],
    this.worktrees = const [],
  });

  final String id;
  final String name;

  /// Folder color as #RRGGBB, or null for default.
  final String? folderColor;
  final bool collapsed;
  final List<SidebarGroup> groups;

  /// Sessions not in any group.
  final List<SidebarSession> sessions;

  /// Worktree folder names under this project.
  final List<String> worktrees;

  /// All sessions in this project (grouped + ungrouped).
  List<SidebarSession> get allSessions => [
        ...sessions,
        for (final g in groups) ...g.sessions,
      ];
}

/// One row of the sidebar. Rendered as a UiRow of UiText cells:
/// [attention][pin][title][spinner][unread][avatars].
final class SidebarRow {
  const SidebarRow._();

  /// Attention dot: red ● when the session needs attention.
  static String attentionGlyph(bool attention) => attention ? '●' : '';

  /// Busy spinner: ◌ while the session's agent is working.
  static String spinnerGlyph(bool busy) => busy ? '◌' : '';

  /// Pin glyph for pinned sessions.
  static String pinGlyph(bool pinned) => pinned ? '📌' : '';

  static final _red = UiColor.hex('#ef2929');
  static final _selectedBg = UiColor.hex('#2d4a6f');
  static final _selectedText = UiColor.hex('#ffffff');

  static UiRow sessionRow(
    SidebarSession session, {
    required bool selected,
  }) {
    final id = session.id;
    return UiRow(
      'session-$id',
      [
        if (session.attention)
          UiText('attn-$id', '●',
              style: UiStyle(foreground: _red, fontSize: 11)),
        if (session.pinned)
          UiText('pin-$id', '📌', style: const UiStyle(fontSize: 11)),
        UiText(
          'title-$id',
          session.title,
          style: UiStyle(
            fontSize: 13,
            fontWeight: selected || session.unreadCount > 0
                ? UiFontWeight.semibold
                : UiFontWeight.normal,
            foreground: selected ? _selectedText : null,
          ),
        ),
        if (session.busy)
          UiText('spin-$id', ' ◌', style: const UiStyle(fontSize: 11)),
        if (session.unreadCount > 0)
          UiText(
            'unread-$id',
            ' ${session.unreadCount}',
            style: UiStyle(
              fontSize: 11,
              foreground: _red,
              fontWeight: UiFontWeight.bold,
            ),
          ),
        if (session.viewers.isNotEmpty)
          UiText(
            'viewers-$id',
            ' ${session.viewers.map((v) => v.isNotEmpty ? v[0] : '?').join()}',
            style: const UiStyle(fontSize: 11),
          ),
      ],
      style: UiStyle(
        background: selected ? _selectedBg : null,
        padding: const [3, 8, 3, 8],
        gap: 6,
      ),
    );
  }
}

/// The full session sidebar.
///
/// Layout (top to bottom): workspace dots, filter input, new-session
/// button, pinned section, per-project trees (header + worktree folders +
/// groups + sessions), archived footer.
final class SidebarView {
  SidebarView({
    this.workspaces = const [],
    this.activeWorkspaceId,
    this.projects = const [],
    this.pinned = const [],
    this.filterText = '',
    this.selectedSessionId,
  });

  final List<String> workspaces;
  final String? activeWorkspaceId;
  final List<SidebarProject> projects;
  final List<SidebarSession> pinned;
  final String filterText;
  final String? selectedSessionId;

  /// Sessions matching [filterText] (case-insensitive substring on title).
  bool matchesFilter(SidebarSession s) {
    if (filterText.isEmpty) return true;
    return s.title.toLowerCase().contains(filterText.toLowerCase());
  }

  UiNode build() {
    final children = <UiNode>[
      // Workspace quick-switch dots.
      UiRow('workspace-dots', [
        for (final w in workspaces)
          UiButton('wsdot-$w', w == activeWorkspaceId ? '● $w' : '○ $w'),
      ]),
      const UiInput('sidebar-filter', placeholder: 'Filter sessions…'),
      const UiButton('new-session', '+ New Session'),
    ];

    final visiblePinned = pinned.where(matchesFilter).toList();
    if (visiblePinned.isNotEmpty) {
      children.add(const UiText('section-pinned', 'Pinned'));
      for (final s in visiblePinned) {
        children.add(
            SidebarRow.sessionRow(s, selected: s.id == selectedSessionId));
      }
    }

    for (final project in projects) {
      children.add(_projectNode(project));
    }

    children.add(const UiButton('show-archived', 'Archived Sessions'));
    return UiColumn('sidebar', children);
  }

  UiNode _projectNode(SidebarProject project) {
    final children = <UiNode>[
      UiRow('project-${project.id}', [
        UiText(
          'project-color-${project.id}',
          '■', // folder color swatch
          style: UiStyle(
            fontSize: 12,
            foreground: project.folderColor != null
                ? UiColor.hex(project.folderColor!)
                : UiColor.hex('#8b8b8b'),
          ),
        ),
        UiText('project-name-${project.id}', project.name,
            style: const UiStyle(
                fontSize: 13, fontWeight: UiFontWeight.semibold)),
        UiText('project-collapse-${project.id}',
            project.collapsed ? '▸' : '▾',
            style: const UiStyle(fontSize: 11)),
      ]),
    ];
    if (!project.collapsed) {
      // Worktree folders.
      for (final wt in project.worktrees) {
        final wtSessions = project.allSessions
            .where((s) => s.worktree == wt && matchesFilter(s))
            .toList();
        children.add(UiText('worktree-${project.id}-$wt', '  📁 $wt',
            style: const UiStyle(fontSize: 12)));
        for (final s in wtSessions) {
          children.add(SidebarRow.sessionRow(s,
              selected: s.id == selectedSessionId));
        }
      }
      // Groups.
      for (final group in project.groups) {
        children.add(UiRow('group-${group.id}', [
          if (group.color != null)
            UiText('group-color-${group.id}', '■',
                style: UiStyle(
                    fontSize: 12, foreground: UiColor.hex(group.color!))),
          UiText('group-title-${group.id}', group.title,
              style: const UiStyle(fontSize: 12)),
          UiText('group-collapse-${group.id}',
              group.collapsed ? '▸' : '▾',
              style: const UiStyle(fontSize: 11)),
        ]));
        if (!group.collapsed) {
          for (final s in group.sessions.where(matchesFilter)) {
            children.add(SidebarRow.sessionRow(s,
                selected: s.id == selectedSessionId));
          }
        }
      }
      // Ungrouped sessions (excluding those shown under worktree folders).
      for (final s in project.sessions
          .where((s) => s.worktree == null && matchesFilter(s))) {
        children.add(
            SidebarRow.sessionRow(s, selected: s.id == selectedSessionId));
      }
    }
    return UiColumn('project-tree-${project.id}', children);
  }

  List<UiAction> actions() => [
        const UiAction(
            name: 'sidebar.filter',
            keys: 'cmd+f',
            context: UiActionContext.node('sidebar')),
        const UiAction(name: 'session.new', keys: 'cmd+n'),
        const UiAction(
            name: 'session.select-next',
            keys: 'ctrl+tab',
            context: UiActionContext.node('sidebar')),
      ];
}

/// Session context menu (#153).
///
/// Items: rename, copy ID, copy transcript, notify when done, clear
/// attention, resume, restart app, reveal, pin, stop and archive, remove.
/// Rendered as a column of buttons; popup positioning needs a framework
/// menu primitive (see docs/gpuidart-gaps-sidebar.md G-2).
final class SessionContextMenu {
  const SessionContextMenu({required this.sessionId});

  final String sessionId;

  /// (action name, label) pairs, in menu order.
  static const List<(String, String)> items = [
    ('session.rename', 'Rename…'),
    ('session.copy-id', 'Copy Session ID'),
    ('session.copy-transcript', 'Copy Transcript'),
    ('session.notify-when-done', 'Notify When Done'),
    ('session.clear-attention', 'Clear Attention'),
    ('session.resume', 'Resume'),
    ('session.restart-app', 'Restart App'),
    ('session.reveal', 'Reveal in Finder'),
    ('session.pin', 'Pin'),
    ('session.stop-archive', 'Stop and Archive'),
    ('session.remove', 'Remove…'),
  ];

  UiNode build() {
    return UiColumn('session-menu-$sessionId', [
      for (final (name, label) in items)
        UiButton('menu-$sessionId-$name', label),
    ]);
  }

  /// Menu item activation arrives as a native `click` event whose id is the
  /// button id (`menu-<sessionId>-<action>`); the host decodes the action
  /// from the id suffix. No key bindings: menu items are pointer-driven.
  static String actionForButtonId(String buttonId) {
    const prefix = 'menu-';
    if (!buttonId.startsWith(prefix)) return '';
    final rest = buttonId.substring(prefix.length);
    final dash = rest.indexOf('-');
    if (dash < 0) return '';
    return rest.substring(dash + 1);
  }
}

/// Project context menu (#154).
///
/// Items: new worktree, new group, rename, stop all, sort, folder color,
/// archived, open in editor, move to workspace.
final class ProjectContextMenu {
  const ProjectContextMenu({required this.projectId});

  final String projectId;

  static const List<(String, String)> items = [
    ('project.new-worktree', 'New Worktree…'),
    ('project.new-group', 'New Group'),
    ('project.rename', 'Rename…'),
    ('project.stop-all', 'Stop All Sessions'),
    ('project.sort', 'Sort Sessions'),
    ('project.folder-color', 'Folder Color…'),
    ('project.archived', 'Show Archived'),
    ('project.open-in-editor', 'Open in Editor'),
    ('project.move-to-workspace', 'Move to Workspace…'),
  ];

  UiNode build() {
    return UiColumn('project-menu-$projectId', [
      for (final (name, label) in items)
        UiButton('menu-$projectId-$name', label),
    ]);
  }

  /// Same click-id decoding as [SessionContextMenu.actionForButtonId].
  static String actionForButtonId(String buttonId) =>
      SessionContextMenu.actionForButtonId(buttonId);
}
