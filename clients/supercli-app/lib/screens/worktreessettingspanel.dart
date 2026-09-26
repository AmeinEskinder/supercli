/// Worktrees settings: "Show agent worktrees", list with create/reveal/remove.
///
/// Port of `WorktreesSettingsPanel.swift`. Covers checklist item 212:
/// "Settings ▸ Worktrees (\"Show agent worktrees\", list with
/// create/reveal/remove)".
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingspanels.dart';

/// One git worktree.
final class WorktreeEntry {
  const WorktreeEntry({
    required this.id,
    required this.path,
    this.branch = '',
    this.isAgent = false,
  });

  final String id;
  final String path;
  final String branch;
  final bool isAgent;
}

/// Worktrees settings panel.
final class WorktreesSettingsPanel {
  const WorktreesSettingsPanel({
    this.worktrees = const [],
    this.showAgentWorktrees = false,
  });

  final List<WorktreeEntry> worktrees;
  final bool showAgentWorktrees;

  UiNode build() {
    final visible = showAgentWorktrees
        ? worktrees
        : worktrees.where((w) => !w.isAgent).toList();
    return UiColumn('worktrees-settings', [
      const UiText('worktrees-title', 'Worktrees'),
      SettingsToggle(
        id: 'show-agent-worktrees',
        label: 'Show agent worktrees',
        value: showAgentWorktrees,
      ).fallback(),
      UiTable('worktrees-table', dataset: 'worktrees-settings'),
      UiRow('worktrees-actions', [
        const UiButton('worktrees-create', 'Create…'),
        const UiButton('worktrees-reveal', 'Reveal in Finder'),
        const UiButton('worktrees-remove', 'Remove'),
        const UiButton('worktrees-prune', 'Prune'),
      ]),
      if (visible.isEmpty)
        const UiText('worktrees-empty', 'No worktrees.'),
    ]);
  }

  TableDataset dataset() {
    final visible = showAgentWorktrees
        ? worktrees
        : worktrees.where((w) => !w.isAgent).toList();
    return TableDataset(
      'worktrees-settings',
      columns: const ['Path', 'Branch', 'Agent'],
      rows: visible
          .map((w) => [w.path, w.branch, w.isAgent ? 'Yes' : ''])
          .toList(),
    );
  }
}
