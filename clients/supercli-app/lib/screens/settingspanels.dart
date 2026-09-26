/// Settings panels: Agent Access, License, Plugins, Presets, Workspaces, Worktrees.
///
/// Port of `AgentAccessSettingsPanel.swift`, `LicenseSettingsPanel.swift`,
/// `PluginSettingsPanel.swift`, `PresetsSettingsPanel.swift`,
/// `WorkspacesSettingsPanel.swift`, `WorktreesSettingsPanel.swift`,
/// `PluginListDrag.swift`, and `LocalSiteMenu.swift`.
library;

import 'package:gpuidart/gpuidart.dart';

/// Agent access settings: which agents can access what.
final class AgentAccessSettingsPanel {
  const AgentAccessSettingsPanel({this.rules = const []});

  final List<String> rules;

  UiNode build() {
    return UiColumn('agent-access', [
      const UiText('agent-access-title', 'Agent Access'),
      UiTable('agent-access-table', dataset: 'agent-access'),
      UiRow('agent-access-actions', [
        const UiButton('agent-access-add', 'Add Rule'),
        const UiButton('agent-access-remove', 'Remove'),
      ]),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'agent-access',
        columns: const ['Rule'],
        rows: rules.map((r) => [r]).toList(),
      );
}

/// License settings: key entry and status.
final class LicenseSettingsPanel {
  const LicenseSettingsPanel({
    this.licenseKey = '',
    this.status = '',
  });

  final String licenseKey;
  final String status;

  UiNode build() {
    return UiColumn('license-settings', [
      const UiText('license-title', 'License'),
      UiText('license-status', status),
      const UiInput('license-key', placeholder: 'License key…'),
      const UiButton('license-activate', 'Activate'),
    ]);
  }
}

/// Plugin settings panel.
final class PluginSettingsPanel {
  const PluginSettingsPanel({this.plugins = const []});

  final List<String> plugins;

  UiNode build() {
    return UiColumn('plugin-settings', [
      const UiText('plugin-title', 'Plugins'),
      UiTable('plugin-table', dataset: 'plugin-settings'),
      const UiButton('plugin-install', 'Install Plugin…'),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'plugin-settings',
        columns: const ['Plugin', 'Enabled'],
        rows: plugins.map((p) => [p, 'Yes']).toList(),
      );
}

/// Plugin list drag helper.
/// GAP: No drag-and-drop in gpuidart (P0-13).
final class PluginListDrag {
  const PluginListDrag();

  UiNode build() {
    return const UiText(
        'plugin-drag', '(plugin drag — needs gpuidart drag-and-drop, P0-13)');
  }
}

/// Presets settings panel.
final class PresetsSettingsPanel {
  const PresetsSettingsPanel({this.presets = const []});

  final List<String> presets;

  UiNode build() {
    return UiColumn('presets-settings', [
      const UiText('presets-title', 'Presets'),
      UiTable('presets-table', dataset: 'presets-settings'),
      UiRow('presets-actions', [
        const UiButton('presets-new', 'New Preset'),
        const UiButton('presets-delete', 'Delete'),
      ]),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'presets-settings',
        columns: const ['Preset'],
        rows: presets.map((p) => [p]).toList(),
      );
}

/// Workspaces settings panel.
final class WorkspacesSettingsPanel {
  const WorkspacesSettingsPanel({this.workspaces = const []});

  final List<String> workspaces;

  UiNode build() {
    return UiColumn('workspaces-settings', [
      const UiText('workspaces-title', 'Workspaces'),
      UiTable('workspaces-table', dataset: 'workspaces-settings'),
      UiRow('workspaces-actions', [
        const UiButton('workspaces-add', 'Add Workspace'),
        const UiButton('workspaces-remove', 'Remove'),
      ]),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'workspaces-settings',
        columns: const ['Workspace'],
        rows: workspaces.map((w) => [w]).toList(),
      );
}

/// Worktrees settings panel.
final class WorktreesSettingsPanel {
  const WorktreesSettingsPanel({this.worktrees = const []});

  final List<String> worktrees;

  UiNode build() {
    return UiColumn('worktrees-settings', [
      const UiText('worktrees-title', 'Worktrees'),
      UiTable('worktrees-table', dataset: 'worktrees-settings'),
      const UiButton('worktrees-prune', 'Prune'),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'worktrees-settings',
        columns: const ['Worktree'],
        rows: worktrees.map((w) => [w]).toList(),
      );
}

/// Local site menu: per-site navigation menu.
final class LocalSiteMenu {
  const LocalSiteMenu({this.sites = const []});

  final List<String> sites;

  UiNode build() {
    return UiColumn('local-site-menu', [
      const UiText('site-menu-title', 'Local Sites'),
      for (final s in sites) UiButton('site-$s', s),
    ]);
  }
}
