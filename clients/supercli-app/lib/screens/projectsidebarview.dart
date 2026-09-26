/// Project sidebar: per-project session groups, loading skeleton, spinners,
/// workspace dots/selector, and the detached session-row drag controller.
///
/// Port of `ProjectSidebarView.swift`, `SidebarSkeleton.swift`,
/// `SidebarSpinners.swift`, `SidebarWorkspaceDots.swift`,
/// `SidebarWorkspaceSelector.swift`, and `SidebarSessionDrag.swift`.
///
/// Rendering follows the UiRow/UiText fallback pattern (no native list
/// node upstream). Drag-and-drop itself needs framework APIs that do not
/// exist yet — see docs/gpuidart-gaps-sidebar.md G-1.
library;

import 'package:gpuidart/gpuidart.dart';

import 'sidebarview.dart';

/// Loading skeleton for the sidebar: shimmer placeholder rows shown while
/// the host streams the session list.
final class SidebarSkeleton {
  const SidebarSkeleton({this.rows = 6});

  final int rows;

  UiNode build() {
    return UiColumn('sidebar-skeleton', [
      for (var i = 0; i < rows; i++)
        UiRow('skeleton-row-$i', [
          UiText('skeleton-bar-$i', '▓▓▓▓▓▓▓▓▓▓▓▓',
              style: const UiStyle(fontSize: 13)),
        ], style: const UiStyle(padding: [3, 8, 3, 8])),
    ]);
  }
}

/// Spinner indicators for sidebar rows: which sessions are busy.
///
/// The per-row spinner glyph itself is rendered by [SidebarRow.sessionRow];
/// this widget is the standalone indicator strip used in the status area.
final class SidebarSpinners {
  const SidebarSpinners({this.spinningIds = const []});

  final List<String> spinningIds;

  UiNode build() {
    return UiRow('sidebar-spinners', [
      for (final id in spinningIds)
        UiText('spinner-$id', '◌ working…',
            style: const UiStyle(fontSize: 11)),
    ]);
  }
}

/// Workspace dots: quick-switcher dots for open workspaces.
/// The active workspace renders filled (●), the rest hollow (○).
final class SidebarWorkspaceDots {
  const SidebarWorkspaceDots({
    this.workspaces = const [],
    this.activeId,
  });

  final List<String> workspaces;
  final String? activeId;

  UiNode build() {
    return UiRow('workspace-dots', [
      for (final w in workspaces)
        UiButton('wsdot-$w', w == activeId ? '● $w' : '○ $w'),
    ]);
  }
}

/// Workspace selector dropdown: lists workspaces for switching.
final class SidebarWorkspaceSelector {
  const SidebarWorkspaceSelector({
    this.workspaces = const [],
    this.selected,
  });

  final List<String> workspaces;
  final String? selected;

  UiNode build() {
    return UiColumn('workspace-selector', [
      const UiText('ws-selector-title', 'Workspace',
          style: UiStyle(fontSize: 12, fontWeight: UiFontWeight.semibold)),
      for (final w in workspaces)
        UiButton('ws-select-$w', w == selected ? '✓ $w' : '  $w'),
    ]);
  }
}

/// Where a dragged session can land.
enum SidebarDropTarget {
  /// Reorder within the same list.
  reorder,

  /// Move into a group.
  group,

  /// Move into a project (ungrouped).
  project,

  /// Drop onto a pane to split it (drop-to-split).
  split,
}

/// Detached session-row drag controller ("Dia feel", #152).
///
/// The drag *state machine* (what is dragged, valid targets, drop-to-split)
/// is fully modeled here. The OS-level pointer drag events that would feed
/// it do not exist in gpuidart upstream — see docs/gpuidart-gaps-sidebar.md
/// G-1. Until then the overlay renders statically and drops are issued as
/// [UiAction]s.
final class SidebarSessionDrag {
  const SidebarSessionDrag({
    this.draggedSessionId,
    this.target = SidebarDropTarget.reorder,
    this.targetId,
    this.dropToSplit = false,
  });

  /// The session being dragged, or null when idle.
  final String? draggedSessionId;
  final SidebarDropTarget target;
  final String? targetId;

  /// True when the pointer is over a pane: drop splits the pane.
  final bool dropToSplit;

  bool get isDragging => draggedSessionId != null;

  /// Validate a drop: cannot drop a session onto itself or into its own
  /// current group (no-op), and drop-to-split requires a pane target.
  bool canDrop({
    required String sessionId,
    required String? currentGroupId,
  }) {
    if (!isDragging) return false;
    if (draggedSessionId == sessionId) return false;
    return switch (target) {
      SidebarDropTarget.reorder => true,
      SidebarDropTarget.group =>
        targetId != null && targetId != currentGroupId,
      SidebarDropTarget.project => true,
      SidebarDropTarget.split => dropToSplit && targetId != null,
    };
  }

  UiNode build() {
    if (!isDragging) {
      return const UiText('session-drag-idle', '');
    }
    final hint = switch (target) {
      SidebarDropTarget.reorder => 'reorder',
      SidebarDropTarget.group => 'move to group ${targetId ?? ''}',
      SidebarDropTarget.project => 'move to project ${targetId ?? ''}',
      SidebarDropTarget.split => 'split pane',
    };
    return UiColumn('session-drag-overlay', [
      UiText('drag-title', 'Dragging $draggedSessionId → $hint',
          style: const UiStyle(fontSize: 12)),
      const UiText('drag-gap', '(live drag needs gpuidart DnD, G-1)',
          style: UiStyle(fontSize: 11)),
    ]);
  }

  List<UiAction> actions() => const [
        UiAction(
            name: 'sidebar.drag.cancel',
            keys: 'escape',
            context: UiActionContext.node('session-drag-overlay')),
      ];
}

/// The per-project sidebar panel: project header with folder color,
/// worktree folders, groups, and session rows.
final class ProjectSidebarView {
  const ProjectSidebarView({
    required this.project,
    this.selectedSessionId,
    this.filterText = '',
  });

  final SidebarProject project;
  final String? selectedSessionId;
  final String filterText;

  bool _matches(SidebarSession s) => filterText.isEmpty ||
      s.title.toLowerCase().contains(filterText.toLowerCase());

  UiNode build() {
    final children = <UiNode>[
      UiRow('project-header-${project.id}', [
        UiText(
          'project-swatch-${project.id}',
          '■',
          style: UiStyle(
            fontSize: 13,
            foreground: project.folderColor != null
                ? UiColor.hex(project.folderColor!)
                : UiColor.hex('#8b8b8b'),
          ),
        ),
        UiText('project-title-${project.id}', project.name,
            style: const UiStyle(
                fontSize: 13, fontWeight: UiFontWeight.semibold)),
        UiText('project-toggle-${project.id}',
            project.collapsed ? '▸' : '▾',
            style: const UiStyle(fontSize: 11)),
      ]),
    ];
    if (!project.collapsed) {
      for (final wt in project.worktrees) {
        children.add(UiText('wt-${project.id}-$wt', '  📁 $wt',
            style: const UiStyle(fontSize: 12)));
        for (final s in project.allSessions
            .where((s) => s.worktree == wt && _matches(s))) {
          children.add(SidebarRow.sessionRow(s,
              selected: s.id == selectedSessionId));
        }
      }
      for (final group in project.groups) {
        children.add(UiRow('pgroup-${group.id}', [
          if (group.color != null)
            UiText('pgroup-color-${group.id}', '■',
                style: UiStyle(
                    fontSize: 12, foreground: UiColor.hex(group.color!))),
          UiText('pgroup-title-${group.id}',
              '${group.title} (${group.sessions.length})',
              style: const UiStyle(fontSize: 12)),
          UiText('pgroup-toggle-${group.id}',
              group.collapsed ? '▸' : '▾',
              style: const UiStyle(fontSize: 11)),
        ]));
        if (!group.collapsed) {
          for (final s in group.sessions.where(_matches)) {
            children.add(SidebarRow.sessionRow(s,
                selected: s.id == selectedSessionId));
          }
        }
      }
      for (final s in project.sessions
          .where((s) => s.worktree == null && _matches(s))) {
        children.add(
            SidebarRow.sessionRow(s, selected: s.id == selectedSessionId));
      }
    }
    return UiColumn('project-sidebar-${project.id}', children);
  }
}
