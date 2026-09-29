/// Plugin settings: Apps catalog install/update/activate/order.
///
/// Port of `PluginSettingsPanel.swift` and `PluginListDrag.swift`. Covers
/// checklist item 203: "Settings ▸ Plugins (Apps catalog
/// install/update/activate/order)".
///
/// GAP: drag-to-reorder needs gpuidart drag-and-drop (P0-13). Reorder is
/// exposed as Move up / Move down buttons instead. Logged in
/// docs/gpuidart-gaps-settings.md.
///
/// The filter/section/expand logic below ports the portable state from
/// `PluginSettingsPanel.swift` (PluginFilter, visibleItems, entries,
/// expanded). SwiftUI rendering (ViewBuilder, GeometryReader, drag
/// controllers, the installation terminal embed) is AppKit-only and dropped.
library;

import 'package:gpuidart/gpuidart.dart';

import '../plugin_settings_list.dart';

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

/// List filter. Port of `PluginSettingsPanel.PluginFilter`.
enum PluginFilter {
  overview,
  installed,
  available;

  /// Port of `PluginFilter.includes(_:)`.
  bool includes(PluginSettingsItem item) {
    switch (this) {
      case PluginFilter.overview:
        return item.installed || item.isApp;
      case PluginFilter.installed:
        return item.installed;
      case PluginFilter.available:
        return !item.installed;
    }
  }

  String get label {
    switch (this) {
      case PluginFilter.overview:
        return 'Overview';
      case PluginFilter.installed:
        return 'Installed';
      case PluginFilter.available:
        return 'Not Installed';
    }
  }
}

/// A section in the panel list. Port of `PluginSettingsPanel.ListEntry`.
sealed class PluginListEntry {
  const PluginListEntry();
}

/// Section header row.
final class PluginListHeader extends PluginListEntry {
  const PluginListHeader(this.title);
  final String title;
}

/// A plugin/app row.
final class PluginListItem extends PluginListEntry {
  const PluginListItem(this.item);
  final PluginSettingsItem item;
}

/// Filter items by search text and [filter].
/// Port of `PluginSettingsPanel.visibleItems`.
List<PluginSettingsItem> filterPluginItems({
  required List<PluginSettingsItem> items,
  required String search,
  required PluginFilter filter,
  required bool Function(PluginSettingsItem) isActive,
}) {
  final q = search.trim().toLowerCase();
  return items.where((item) {
    if (!filter.includes(item)) return false;
    if (q.isEmpty) return true;
    if (item.name.toLowerCase().contains(q)) return true;
    if (item.command.toLowerCase().contains(q)) return true;
    return item.commands.any((c) => c.command.toLowerCase().contains(q));
  }).toList();
}

/// Group visible items into Active / Inactive / Available sections.
/// Port of `PluginSettingsPanel.entries`.
///
/// Agents scope order: Active, Inactive, Available.
/// Plugins scope order: Active, Available, Inactive.
List<PluginListEntry> sectionPluginItems({
  required List<PluginSettingsItem> visible,
  required bool Function(PluginSettingsItem) isActive,
  required bool agentsScope,
}) {
  final active = visible.where(isActive).toList();
  final inactive =
      visible.where((i) => i.installed && !isActive(i)).toList();
  final available = visible.where((i) => !i.installed).toList();
  final sections = agentsScope
      ? [('Active', active), ('Inactive', inactive), ('Available to install', available)]
      : [('Active', active), ('Available to install', available), ('Inactive', inactive)];
  final out = <PluginListEntry>[];
  for (final (title, items) in sections) {
    if (items.isEmpty) continue;
    out.add(PluginListHeader(title));
    out.addAll(items.map(PluginListItem.new));
  }
  return out;
}

/// Plugin settings panel.
///
/// Renders the sectioned list (Active / Inactive / Available) with search
/// and filter. Row expand/collapse, integration connect flows, the
/// command/draft editor, and install/update flows are Host operations
/// surfaced here as actions; the SwiftUI row rendering is AppKit-only.
final class PluginSettingsPanel {
  const PluginSettingsPanel({
    this.items = const [],
    this.search = '',
    this.filter = PluginFilter.overview,
    this.expanded = const {},
    this.activationOverrides = const {},
    this.agentsScope = false,
  });

  final List<PluginSettingsItem> items;
  final String search;
  final PluginFilter filter;

  /// IDs of expanded rows (agent details / command editor visible).
  final Set<String> expanded;

  /// Local activation toggles not yet confirmed by the Host.
  final Map<String, bool> activationOverrides;

  /// True for the Agents scope, false for the Plugins scope.
  final bool agentsScope;

  bool isActive(PluginSettingsItem item) {
    if (!item.installed) return false;
    return activationOverrides[item.id] ?? true;
  }

  List<PluginSettingsItem> get visibleItems => filterPluginItems(
        items: items,
        search: search,
        filter: filter,
        isActive: isActive,
      );

  List<PluginListEntry> get entries => sectionPluginItems(
        visible: visibleItems,
        isActive: isActive,
        agentsScope: agentsScope,
      );

  UiNode build() {
    final entries = this.entries;
    return UiColumn('plugin-settings', [
      UiText('plugin-title', agentsScope ? 'Agents' : 'Plugins'),
      UiText(
          'plugin-search',
          search.isEmpty
              ? (agentsScope ? 'Search agents' : 'Search plugins')
              : 'Search: $search'),
      UiText('plugin-filter', 'Show: ${filter.label}'),
      for (final entry in entries)
        switch (entry) {
          PluginListHeader(title: final t) =>
            UiText('plugin-section-$t', t),
          PluginListItem(item: final item) => UiRow(
              'plugin-row-${item.id}',
              [
                UiText('plugin-name-${item.id}', item.name),
                UiText(
                    'plugin-state-${item.id}',
                    isActive(item)
                        ? 'Active'
                        : (item.installed ? 'Inactive' : 'Available')),
                if (expanded.contains(item.id))
                  const UiText('plugin-expanded', 'expanded'),
              ],
            ),
        },
      if (entries.isEmpty)
        UiText('plugin-empty',
            agentsScope ? 'No agents match your search.' : 'No plugins match your search.'),
      UiRow('plugin-actions', [
        const UiButton('plugin-install', 'Install…'),
        const UiButton('plugin-update-all', 'Update all'),
      ]),
      const UiText('plugin-order-hint',
          'Reorder with Move up / Move down (drag needs gpuidart P0-13).'),
    ]);
  }

  /// Row-level actions rendered per selected plugin.
  UiNode rowActions(String pluginId) {
    final item = items.firstWhere(
      (p) => p.id == pluginId,
      orElse: () => const PluginSettingsItem(
        id: '',
        name: '',
        command: '',
        installed: false,
        isApp: false,
        isCustom: false,
      ),
    );
    final active = isActive(item);
    return UiRow('plugin-row-actions-$pluginId', [
      if (item.installed)
        UiButton('plugin-toggle-$pluginId', active ? 'Disable' : 'Enable'),
      if (!item.installed)
        UiButton('plugin-install-$pluginId', 'Install'),
      if (item.availableVersion != null &&
          item.availableVersion != item.installedVersion)
        UiButton('plugin-update-$pluginId', 'Update'),
      UiButton('plugin-up-$pluginId', 'Move up'),
      UiButton('plugin-down-$pluginId', 'Move down'),
      if (item.installed)
        UiButton('plugin-uninstall-$pluginId', 'Uninstall'),
      UiButton(
          'plugin-expand-$pluginId',
          expanded.contains(pluginId) ? 'Collapse' : 'Expand'),
    ]);
  }
}
