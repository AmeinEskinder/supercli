/// Tests for plugin_settings_list.dart
/// 
/// Port of PluginSettingsListTests.swift behaviors.
import 'package:test/test.dart';
import '../lib/plugin_settings_list.dart';

void main() {
  group('PluginSettingsList.merging', () {
    test('filtered drag preserves hidden and inactive slots', () {
      expect(
        PluginSettingsList.merging(
          ['codex', 'claude'],
          ['claude', 'hidden', 'codex', 'inactive'],
        ),
        ['codex', 'hidden', 'claude', 'inactive'],
      );
    });

    test('appends unknown ids', () {
      expect(
        PluginSettingsList.merging(['new'], ['a', 'b']),
        ['a', 'b', 'new'],
      );
    });

    test('empty subset returns order unchanged', () {
      expect(
        PluginSettingsList.merging([], ['a', 'b']),
        ['a', 'b'],
      );
    });
  });

  group('PluginSettingsList.items', () {
    test('returns empty for null snapshot', () {
      expect(PluginSettingsList.items(null), isEmpty);
    });

    test('builds items from agents and apps', () {
      final snapshot = RemoteBootstrapSnapshot(
        workspaceSettings: RemoteWorkspaceSettings(
          availableAgents: [
            AvailableAgentSummary(
              id: 'claude',
              name: 'Claude',
              command: 'claude',
              installed: true,
            ),
          ],
        ),
        availableApps: [
          RemoteAppSummary(
            id: 'supercli.app.markdown',
            name: 'Markdown',
            command: 'supercli-markdown',
            installed: true,
          ),
        ],
        presets: [],
      );
      final items = PluginSettingsList.items(snapshot);
      expect(items.map((i) => i.id), ['claude', 'supercli.app.markdown']);
      expect(items[0].isApp, false);
      expect(items[1].isApp, true);
    });

    test('groups presets by plugin id', () {
      final snapshot = RemoteBootstrapSnapshot(
        presets: [
          RemotePresetSummary(
            id: 'base',
            label: 'Claude',
            command: 'claude',
            pluginID: 'claude',
          ),
          RemotePresetSummary(
            id: 'variant',
            label: 'Plan',
            command: 'claude --plan',
            pluginID: 'claude',
          ),
        ],
      );
      final items = PluginSettingsList.items(snapshot);
      expect(items.length, 1);
      expect(items[0].id, 'claude');
      expect(items[0].commands.map((c) => c.command), ['claude', 'claude --plan']);
    });

    test('ignores project-scoped presets', () {
      final snapshot = RemoteBootstrapSnapshot(
        presets: [
          RemotePresetSummary(
            id: 'base',
            label: 'Project override',
            command: 'ignored',
            projectID: 'project',
          ),
        ],
      );
      final items = PluginSettingsList.items(snapshot);
      expect(items, isEmpty);
    });
  });

  group('PluginSettingsItem', () {
    test('displayVersion uses installed version when installed', () {
      final item = PluginSettingsItem(
        id: 'test',
        name: 'Test',
        command: 'test',
        installed: true,
        installedVersion: '1.0',
        availableVersion: '2.0',
        isApp: true,
        isCustom: false,
      );
      expect(item.displayVersion, '1.0');
    });

    test('displayVersion uses available version when not installed', () {
      final item = PluginSettingsItem(
        id: 'test',
        name: 'Test',
        command: 'test',
        installed: false,
        installedVersion: '1.0',
        availableVersion: '2.0',
        isApp: true,
        isCustom: false,
      );
      expect(item.displayVersion, '2.0');
    });
  });
}
