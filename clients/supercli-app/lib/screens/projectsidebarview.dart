/// Project sidebar: per-project session groups.
///
/// Port of `ProjectSidebarView.swift`, `SidebarSkeleton.swift`,
/// `SidebarSpinners.swift`, `SidebarWorkspaceDots.swift`,
/// `SidebarWorkspaceSelector.swift`, and `SidebarSessionDrag.swift`.
library;

import 'package:gpuidart/gpuidart.dart';

import '../models.dart';

/// Loading skeleton for the sidebar.
final class SidebarSkeleton {
  const SidebarSkeleton();

  UiNode build() {
    return UiColumn('sidebar-skeleton', [
      for (var i = 0; i < 6; i++) UiText('skeleton-$i', '▓▓▓▓▓▓▓▓'),
    ]);
  }
}

/// Spinner indicators for sidebar rows.
final class SidebarSpinners {
  const SidebarSpinners({this.spinningIds = const []});

  final List<String> spinningIds;

  UiNode build() {
    return UiRow('sidebar-spinners', [
      for (final id in spinningIds) UiText('spinner-$id', '◌'),
    ]);
  }
}

/// Workspace dots: quick-switcher dots for open workspaces.
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
        UiButton('wsdot-$w', w == activeId ? '●' : '○'),
    ]);
  }
}

/// Workspace selector dropdown.
final class SidebarWorkspaceSelector {
  const SidebarWorkspaceSelector({
    this.workspaces = const [],
    this.selected,
  });

  final List<String> workspaces;
  final String? selected;

  UiNode build() {
    return UiColumn('workspace-selector', [
      const UiText('ws-selector-title', 'Workspace'),
      UiTable('ws-selector-table', dataset: 'workspace-selector'),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'workspace-selector',
        columns: const ['Workspace'],
        rows: workspaces.map((w) => [w]).toList(),
      );
}

/// Detached session-row drag controller ("Dia feel").
/// GAP: No drag-and-drop in gpuidart (P0-13). Rows render statically.
final class SidebarSessionDrag {
  const SidebarSessionDrag();

  UiNode build() {
    return const UiText('session-drag',
        '(drag overlay — needs gpuidart drag-and-drop, P0-13)');
  }
}

/// The per-project sidebar panel.
final class ProjectSidebarView {
  const ProjectSidebarView({
    this.projectName = '',
    this.sessions = const [],
  });

  final String projectName;
  final List<SessionSummary> sessions;

  UiNode build() {
    return UiColumn('project-sidebar', [
      UiText('project-name', projectName),
      UiTable('project-sessions', dataset: 'project-sessions'),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'project-sessions',
        columns: const ['Session'],
        rows: sessions.map((s) => [s.title]).toList(),
      );
}
