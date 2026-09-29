/// Tests for presets.dart
/// 
/// Port of PresetsTests.swift behaviors.
library;
import 'package:test/test.dart';
import 'package:supercli_app/presets.dart';
import 'package:supercli_app/plugin_settings_list.dart';
import 'package:supercli_app/tool_icons.dart';

void main() {
  group('Preset', () {
    test('newTerminal has expected ID', () {
      expect(Preset.newTerminalID, '__new_terminal__');
      expect(Preset.newTerminal.isNewTerminal, true);
    });

    test('sanitized clears quickLaunch for empty command', () {
      final preset = Preset(
        id: 'test',
        label: 'Test',
        command: '   ',
        enabled: true,
        quickLaunch: true,
      );
      expect(preset.sanitized().quickLaunch, false);
    });

    test('sanitized preserves quickLaunch for non-empty command', () {
      final preset = Preset(
        id: 'test',
        label: 'Test',
        command: 'claude',
        enabled: true,
        quickLaunch: true,
      );
      expect(preset.sanitized().quickLaunch, true);
    });

    test('JSON roundtrip', () {
      final preset = Preset(
        id: 'test',
        label: 'Test',
        command: 'claude',
        enabled: true,
        quickLaunch: true,
      );
      final json = preset.toJson();
      final restored = Preset.fromJson(json);
      expect(restored.id, 'test');
      expect(restored.quickLaunch, true);
    });
  });

  group('splitPresetsForNewSessionMenu', () {
    test('lists agents first and plugins in their own section', () {
      SupercliAppIconCatalog.update([
        RemoteAppSummary(
          id: 'supercli.app.markdown',
          name: 'Markdown',
          command: 'supercli-markdown',
          installed: true,
        ),
      ]);
      
      final presets = [
        Preset(id: 'markdown', label: 'Markdown', command: 'supercli-markdown', enabled: true, quickLaunch: true),
        Preset(id: 'codex', label: 'codex', command: 'codex --yolo', enabled: true, quickLaunch: true),
        Preset(id: 'dev', label: 'Dev', command: './scripts/dev.sh', enabled: true, quickLaunch: false),
      ];
      final split = splitPresetsForNewSessionMenu(presets);
      // 'supercli-markdown' matches the app, so it's a plugin
      // 'codex --yolo' and './scripts/dev.sh' are agents
      expect(split.plugins.map((p) => p.id), contains('markdown'));
      expect(split.agents.map((p) => p.id), containsAll(['codex', 'dev']));
      
      SupercliAppIconCatalog.update([]);
    });
  });

  group('collectQuickPresetGroups', () {
    test('groups by CLI with quickLaunch filter', () {
      final presets = [
        Preset(id: 'claude', label: 'Claude', command: 'claude', enabled: true, quickLaunch: true),
        Preset(id: 'notes', label: 'Notes', command: 'claude notes.md', enabled: true, quickLaunch: false),
      ];
      // 'claude' is in the minimal catalog and supports quick launch
      final groups = collectQuickPresetGroups(presets);
      // Only groups with at least one quickLaunch preset are included
      expect(groups.any((g) => g.presets.any((p) => p.id == 'claude')), true);
    });

    test('custom commands get their own group', () {
      final presets = [
        Preset(id: 'custom', label: 'Custom', command: './my-script.sh', enabled: true, quickLaunch: true),
      ];
      final groups = collectQuickPresetGroups(presets);
      expect(groups.length, 1);
      expect(groups[0].id, 'custom');
    });
  });

  group('ToolUsageStats', () {
    test('moreUsed prefers recent count', () {
      final a = ToolUsageStats(sessionCount: 10, recentCount: 5);
      final b = ToolUsageStats(sessionCount: 100, recentCount: 2);
      expect(ToolUsageStats.moreUsed(a, b), true);
    });

    test('summary formats correctly', () {
      final stats = ToolUsageStats(sessionCount: 5, recentCount: 2);
      expect(stats.summary, '5 sessions');
      
      final one = ToolUsageStats(sessionCount: 1, recentCount: 1);
      expect(one.summary, '1 session');
      
      expect(ToolUsageStats.none.summary, isNull);
    });
  });

  group('GlobalPresetFile', () {
    test('filters out project-scoped presets', () {
      final file = GlobalPresetFile(
        id: 'test',
        label: 'Test',
        command: 'claude',
        projectID: 'project-123',
      );
      expect(file.toPreset(), isNull);
    });

    test('converts global presets', () {
      final file = GlobalPresetFile(
        id: 'test',
        label: 'Test',
        command: 'claude',
        quickLaunch: true,
      );
      final preset = file.toPreset();
      expect(preset, isNotNull);
      expect(preset!.id, 'test');
    });
  });
}
