/// Workspaces settings: unified list with add/rename/color/forget/delete.
///
/// Port of `WorkspacesSettingsPanel.swift`. Covers checklist item 201:
/// "Settings ▸ Workspaces (unified list, add local/nearby/code/SSH, rename,
/// color, forget, delete)".
library;

import 'package:gpuidart/gpuidart.dart';

/// One workspace entry.
final class WorkspaceEntry {
  const WorkspaceEntry({
    required this.id,
    required this.name,
    this.path = '',
    this.kind = 'local',
    this.color = 0,
  });

  final String id;
  final String name;
  final String path;
  final String kind; // local | nearby | code | ssh
  final int color; // 0-7 accent index
}

/// Workspaces settings panel.
final class WorkspacesSettingsPanel {
  const WorkspacesSettingsPanel({this.workspaces = const []});

  final List<WorkspaceEntry> workspaces;

  UiNode build() {
    return UiColumn('workspaces-settings', [
      const UiText('workspaces-title', 'Workspaces'),
      UiTable('workspaces-table', dataset: 'workspaces-settings'),
      UiRow('workspaces-add-row', [
        const UiButton('workspaces-add-local', 'Add Local…'),
        const UiButton('workspaces-add-nearby', 'Add Nearby…'),
        const UiButton('workspaces-add-code', 'Add with Code…'),
        const UiButton('workspaces-add-ssh', 'Add SSH…'),
      ]),
      UiRow('workspaces-edit-row', [
        const UiButton('workspaces-rename', 'Rename'),
        const UiButton('workspaces-color', 'Color'),
        const UiButton('workspaces-forget', 'Forget'),
        const UiButton('workspaces-delete', 'Delete'),
      ]),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'workspaces-settings',
        columns: const ['Workspace', 'Kind', 'Path'],
        rows: workspaces.map((w) => [w.name, w.kind, w.path]).toList(),
      );
}
