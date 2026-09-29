/// Behavior tests for the plugin settings panel: filter, search, sections.
///
/// Ports the portable logic from `PluginSettingsPanel.swift`:
/// `PluginFilter.includes`, `visibleItems` (search + filter), and
/// `entries` (Active / Inactive / Available sections).
library;

import 'package:supercli_app/plugin_settings_list.dart';
import 'package:supercli_app/screens/pluginsettingspanel.dart';
import 'package:test/test.dart';

PluginSettingsItem makeItem({
  required String id,
  required String name,
  bool installed = true,
  bool isApp = true,
  String command = '',
}) =>
    PluginSettingsItem(
      id: id,
      name: name,
      command: command.isEmpty ? name.toLowerCase() : command,
      installed: installed,
      isApp: isApp,
      isCustom: false,
    );

void main() {
  group('PluginFilter.includes', () {
    final installedApp = makeItem(id: 'a', name: 'App');
    final uninstalledApp = makeItem(id: 'b', name: 'App2', installed: false);

    test('overview shows installed items and apps', () {
      expect(PluginFilter.overview.includes(installedApp), isTrue);
      expect(PluginFilter.overview.includes(uninstalledApp), isTrue); // isApp
    });

    test('installed shows only installed', () {
      expect(PluginFilter.installed.includes(installedApp), isTrue);
      expect(PluginFilter.installed.includes(uninstalledApp), isFalse);
    });

    test('available shows only not-installed', () {
      expect(PluginFilter.available.includes(installedApp), isFalse);
      expect(PluginFilter.available.includes(uninstalledApp), isTrue);
    });
  });

  group('filterPluginItems', () {
    final items = [
      makeItem(id: 'a', name: 'Git Tools', command: 'git'),
      makeItem(id: 'b', name: 'Docker', command: 'docker'),
    ];

    test('empty search returns all matching filter', () {
      final out = filterPluginItems(
        items: items,
        search: '',
        filter: PluginFilter.overview,
        isActive: (_) => true,
      );
      expect(out.length, 2);
    });

    test('search matches name case-insensitively', () {
      final out = filterPluginItems(
        items: items,
        search: 'GIT',
        filter: PluginFilter.overview,
        isActive: (_) => true,
      );
      expect(out.map((i) => i.id), ['a']);
    });

    test('search matches command', () {
      final out = filterPluginItems(
        items: items,
        search: 'docker',
        filter: PluginFilter.overview,
        isActive: (_) => true,
      );
      expect(out.map((i) => i.id), ['b']);
    });

    test('no match returns empty', () {
      final out = filterPluginItems(
        items: items,
        search: 'zzz-no-match',
        filter: PluginFilter.overview,
        isActive: (_) => true,
      );
      expect(out, isEmpty);
    });
  });

  group('sectionPluginItems', () {
    final active = makeItem(id: 'a', name: 'Active');
    final inactive = makeItem(id: 'b', name: 'Inactive');
    final available = makeItem(id: 'c', name: 'Avail', installed: false);

    bool isActive(PluginSettingsItem i) => i.id == 'a';

    test('plugins scope order: Active, Available, Inactive', () {
      final entries = sectionPluginItems(
        visible: [inactive, available, active],
        isActive: isActive,
        agentsScope: false,
      );
      final headers = entries
          .whereType<PluginListHeader>()
          .map((h) => h.title)
          .toList();
      expect(headers, ['Active', 'Available to install', 'Inactive']);
    });

    test('agents scope order: Active, Inactive, Available', () {
      final entries = sectionPluginItems(
        visible: [available, inactive, active],
        isActive: isActive,
        agentsScope: true,
      );
      final headers = entries
          .whereType<PluginListHeader>()
          .map((h) => h.title)
          .toList();
      expect(headers, ['Active', 'Inactive', 'Available to install']);
    });

    test('empty sections are skipped', () {
      final entries = sectionPluginItems(
        visible: [active],
        isActive: isActive,
        agentsScope: false,
      );
      expect(entries.length, 2); // header + item
      expect((entries[0] as PluginListHeader).title, 'Active');
    });
  });

  group('PluginSettingsPanel', () {
    test('isActive respects activationOverrides', () {
      final item = makeItem(id: 'a', name: 'App');
      final panel = PluginSettingsPanel(
        items: [item],
        activationOverrides: {'a': false},
      );
      expect(panel.isActive(item), isFalse);
    });

    test('build renders sections and empty state', () {
      final panel = PluginSettingsPanel(items: [], search: 'zzz');
      final node = panel.build();
      expect(node, isNotNull);
    });

    test('rowActions includes expand toggle', () {
      final item = makeItem(id: 'a', name: 'App');
      final panel = PluginSettingsPanel(items: [item], expanded: {'a'});
      final node = panel.rowActions('a');
      expect(node, isNotNull);
    });
  });
}
