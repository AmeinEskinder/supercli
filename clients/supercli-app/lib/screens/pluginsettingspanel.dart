/// Plugin settings: Apps catalog install/update/activate/order.
///
/// Port of `PluginSettingsPanel.swift` and `PluginListDrag.swift`. Covers
/// checklist item 203: "Settings ▸ Plugins (Apps catalog
/// install/update/activate/order)".
///
/// GAP: drag-to-reorder needs gpuidart drag-and-drop (P0-13). Reorder is
/// exposed as Move up / Move down buttons instead. Logged in
/// docs/gpuidart-gaps-settings.md.
library;

import 'package:gpuidart/gpuidart.dart';

/// One installed plugin/app.
final class PluginEntry {
  const PluginEntry({
    required this.id,
    required this.name,
    this.version = '',
    this.enabled = true,
    this.updateAvailable = false,
  });

  final String id;
  final String name;
  final String version;
  final bool enabled;
  final bool updateAvailable;
}

/// Plugin settings panel.
final class PluginSettingsPanel {
  const PluginSettingsPanel({this.plugins = const []});

  final List<PluginEntry> plugins;

  UiNode build() {
    return UiColumn('plugin-settings', [
      const UiText('plugin-title', 'Plugins'),
      const UiText('plugin-desc',
          'Apps extend the agent with tools, panels, and commands.'),
      UiTable('plugin-table', dataset: 'plugin-settings'),
      UiRow('plugin-actions', [
        const UiButton('plugin-install', 'Install…'),
        const UiButton('plugin-update-all', 'Update all'),
      ]),
      const UiText('plugin-order-hint',
          'Reorder with Move up / Move down (drag needs gpuidart P0-13).'),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'plugin-settings',
        columns: const ['Plugin', 'Version', 'Enabled', 'Update'],
        rows: plugins
            .map((p) => [
                  p.name,
                  p.version,
                  p.enabled ? 'Yes' : 'No',
                  p.updateAvailable ? 'Available' : '',
                ])
            .toList(),
      );

  /// Row-level actions rendered per selected plugin.
  UiNode rowActions(String pluginId) {
    final plugin = plugins.firstWhere(
      (p) => p.id == pluginId,
      orElse: () => const PluginEntry(id: '', name: ''),
    );
    return UiRow('plugin-row-actions-$pluginId', [
      UiButton(
          'plugin-toggle-$pluginId', plugin.enabled ? 'Disable' : 'Enable'),
      if (plugin.updateAvailable)
        UiButton('plugin-update-$pluginId', 'Update'),
      UiButton('plugin-up-$pluginId', 'Move up'),
      UiButton('plugin-down-$pluginId', 'Move down'),
      UiButton('plugin-uninstall-$pluginId', 'Uninstall'),
    ]);
  }
}
