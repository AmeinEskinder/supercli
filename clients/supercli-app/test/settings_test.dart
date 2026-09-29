/// Tests for the settings screens.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/screens/hostpickerview.dart';
import 'package:supercli_app/screens/licensesettings.dart';
import 'package:supercli_app/screens/pluginsettingspanel.dart';
import 'package:supercli_app/screens/presetssettingspanel.dart';
import 'package:supercli_app/screens/sessionsaccesssections.dart';
import 'package:supercli_app/screens/settingspanels.dart';
import 'package:supercli_app/screens/settingsview.dart';
import 'package:supercli_app/screens/settingsshell.dart';
import 'package:supercli_app/screens/workspacessettingspanel.dart';
import 'package:supercli_app/screens/worktreessettingspanel.dart';
import 'package:test/test.dart';

void main() {
  group('AppSettings', () {
    test('toHostJson uses Host camelCase wire format', () {
      final s = AppSettings(
        sessionsMcp: true,
        browserMcp: false,
        browserDefaultAccess: BrowserDefaultAccess.ask,
        writePolicy: WritePolicy.deny,
        worktreeAccess: true,
        autoStopArchiveMinutes: 120,
        sidebarStoppedLimit: 5,
        theme: ThemeMode.dark,
      );
      final json = s.toHostJson();
      final experimental = json['experimentalSettings'] as Map<String, Object>;
      expect(experimental['sessionsMcp'], true);
      expect(experimental['browserMcp'], false);
      expect(json['browserDefaultAccess'], 'ask');
      expect(json['mcpNonchildWriteAccess'], 'deny');
      expect(json['mcpWorktreeAccess'], true);
      expect(json['autoStopArchiveMinutes'], 120);
      expect(json['sidebarStoppedLimit'], 5);
      // Theme is local-only: never sent to the Host.
      expect(json.containsKey('appearanceSettings'), isFalse);
      expect(json.containsKey('theme'), isFalse);
    });

    test('fromHostJson round-trips', () {
      final s = AppSettings(writePolicy: WritePolicy.allow);
      final json = s.toHostJson();
      final back = AppSettings.fromHostJson(
        Map<String, dynamic>.from(json.map((k, v) => MapEntry(k, v))),
      );
      expect(back.writePolicy, WritePolicy.allow);
      // Theme is local-only, so it does not round-trip through Host JSON.
      expect(back.theme, ThemeMode.system);
      expect(back.autoStopArchiveMinutes, s.autoStopArchiveMinutes);
    });

    test('fromHostJson tolerates unknown values', () {
      final back = AppSettings.fromHostJson({
        'appearanceSettings': {'theme': 'neon'},
        'mcpNonchildWriteAccess': 'sometimes',
        'browserDefaultAccess': 'maybe',
      });
      expect(back.theme, ThemeMode.system);
      expect(back.writePolicy, WritePolicy.ask);
      expect(back.browserDefaultAccess, BrowserDefaultAccess.ask);
    });

    test('fromHostJson reads nested experimental settings', () {
      final back = AppSettings.fromHostJson({
        'experimentalSettings': {'sessionsMcp': false, 'browserMcp': true},
      });
      expect(back.sessionsMcp, false);
      expect(back.browserMcp, true);
    });

    test('setCall builds single-key payload', () {
      final s = AppSettings();
      final call = s.setCall('theme', 'dark');
      expect(call['key'], 'theme');
      expect(call['value'], 'dark');
    });
  });

  group('SettingsView', () {
    test('builds sidebar + content', () {
      final view = SettingsView(settings: AppSettings());
      final node = view.build() as UiRow;
      expect(node.children.length, 2);
      final sidebar = node.children[0] as UiColumn;
      // back row, scope picker, nav column, feedback footer
      expect(sidebar.children.length, 4);
      final nav = sidebar.children[2] as UiColumn;
      // AppSettings defaults: sessionsMcp/remoteWorkspaces/gitWorktrees on,
      // so workspaces, agents, plugins, agentAccess, appearance, transcripts,
      // notifications, worktrees, features, advanced are visible.
      expect(nav.children.length, 10);
    });

    test('active tab is marked', () {
      final view = SettingsView(
        settings: AppSettings(),
        activeTab: SettingsTab.plugins,
      );
      final node = view.build() as UiRow;
      final sidebar = node.children[0] as UiColumn;
      final nav = sidebar.children[2] as UiColumn;
      final pluginsTab = nav.children.whereType<UiButton>().firstWhere(
        (b) => b.id == 'settings-tab-plugins',
      );
      expect(pluginsTab.label, startsWith('● '));
    });

    test('gated tab falls back to first visible tab', () {
      // mobile is gated off by default; the stored tab resolves to the
      // first visible tab (workspaces).
      final view = SettingsView(
        settings: AppSettings(),
        activeTab: SettingsTab.mobile,
      );
      final node = view.build() as UiRow;
      final sidebar = node.children[0] as UiColumn;
      final nav = sidebar.children[2] as UiColumn;
      final workspacesTab = nav.children.whereType<UiButton>().firstWhere(
        (b) => b.id == 'settings-tab-workspaces',
      );
      expect(workspacesTab.label, startsWith('● '));
    });

    test('each visible tab renders title strip + panel', () {
      final settings = AppSettings();
      final visible = SettingsTab.visibleCases(
        sessionsMcp: settings.sessionsMcp,
        browserMcp: settings.browserMcp,
        workspacesEnabled: settings.remoteWorkspaces,
        worktreesEnabled: settings.gitWorktrees,
        mobileRemoteControlEnabled: false,
      );
      for (final tab in visible) {
        final view = SettingsView(settings: settings, activeTab: tab);
        final node = view.build() as UiRow;
        final content = node.children[1] as UiColumn;
        expect(
          content.children.length,
          2,
          reason: 'tab $tab should render title strip + panel',
        );
        final strip = content.children[0] as UiText;
        expect(strip.text, contains(tab.title));
      }
    });

    test('every tab has a non-empty title', () {
      for (final tab in SettingsTab.values) {
        expect(tab.title, isNotEmpty);
      }
    });
  });

  group('GeneralSettingsPanel', () {
    test('shows scope picker and appearance', () {
      final panel = GeneralSettingsPanel(settings: AppSettings());
      final node = panel.build() as UiColumn;
      // title, scope select, appearance title, theme, accent, font, size row
      expect(node.children.length, 7);
    });

    test('workspace scope shows inherit row', () {
      final settings = AppSettings(scope: SettingsScope.workspace);
      final panel = GeneralSettingsPanel(settings: settings);
      final node = panel.build() as UiColumn;
      expect(node.children.length, 8);
      final inheritRow = node.children[2] as UiRow;
      expect((inheritRow.children[1] as UiButton).label, 'Reset to inherited');
    });
  });

  group('SessionsSettingsPanel', () {
    test('renders all session controls', () {
      final panel = SessionsSettingsPanel(settings: AppSettings());
      final node = panel.build() as UiColumn;
      expect(node.children.length, 7);
    });

    test('reflects write policy selection', () {
      final settings = AppSettings(writePolicy: WritePolicy.deny);
      final panel = SessionsSettingsPanel(settings: settings);
      final node = panel.build() as UiColumn;
      final select = node.children[1] as UiColumn;
      final options = select.children[1] as UiRow;
      final denyBtn = options.children[2] as UiButton;
      expect(denyBtn.label, '● Deny');
    });
  });

  group('AgentAccessSettingsPanel', () {
    test('renders scopes and approved pairs', () {
      final panel = AgentAccessSettingsPanel(
        settings: AppSettings(),
        scopes: const [
          AccessScope(id: 'files', title: 'Files', detail: 'Read/write'),
        ],
        approvedPairs: const [ApprovedPair(id: 'p1', name: 'CLI ↔ Desktop')],
      );
      final node = panel.build() as UiColumn;
      // title, sessions title, write policy, worktree, gallery,
      // scopes title, 1 scope, pairs title, 1 pair
      expect(node.children.length, 9);
    });

    test('empty pairs shows empty text', () {
      final panel = AgentAccessSettingsPanel(settings: AppSettings());
      final node = panel.build() as UiColumn;
      final last = node.children.last as UiText;
      expect(last.text, 'No approved pairs.');
    });

    test('revoke button per pair', () {
      final panel = AgentAccessSettingsPanel(
        settings: AppSettings(),
        approvedPairs: const [ApprovedPair(id: 'p1', name: 'Pair')],
      );
      final node = panel.build() as UiColumn;
      final pairRow = node.children.last as UiRow;
      expect((pairRow.children[2] as UiButton).label, 'Revoke');
    });
  });

  group('BrowserAccessSections', () {
    test('renders engine status and site rules', () {
      final sections = BrowserAccessSections(
        settings: AppSettings(),
        engineStatus: 'Ready',
        siteRules: const [BrowserSiteRule(id: 'r1', pattern: '*.example.com')],
      );
      final node = sections.build() as UiColumn;
      expect(node.children.length, 11);
    });

    test('empty site rules shows empty text', () {
      final sections = BrowserAccessSections(settings: AppSettings());
      final node = sections.build() as UiColumn;
      final empty = node.children[8] as UiText;
      expect(empty.text, 'No site rules.');
    });
  });

  group('LicenseSettingsPanel', () {
    test('inactive shows key input and activate', () {
      const panel = LicenseSettingsPanel();
      final node = panel.build() as UiColumn;
      expect(node.children.length, 8);
      expect((node.children[3] as UiButton).label, 'Activate');
    });

    test('active shows masked key and deactivate', () {
      const panel = LicenseSettingsPanel(
        licenseKey: 'TEST-LICENSE-KEY',
        status: 'Active',
        activated: true,
        seats: [LicenseSeat(id: 's1', deviceName: 'MacBook')],
      );
      final node = panel.build() as UiColumn;
      final row = node.children[2] as UiRow;
      expect((row.children[1] as UiButton).label, 'Deactivate');
      final masked = row.children[0] as UiText;
      expect(masked.text, contains('-KEY'));
      expect(masked.text, isNot(contains('TEST-LICENSE-KEY')));
    });

    test('seats dataset has rows', () {
      const panel = LicenseSettingsPanel(
        seats: [LicenseSeat(id: 's1', deviceName: 'Mac', lastSeen: 'now')],
      );
      final ds = panel.dataset();
      expect(ds.rowCount, 1);
      expect(ds.cell(0, 0), 'Mac');
    });

    test('isValidKeyFormat accepts SCLI- keys', () {
      expect(
        LicenseSettingsPanel.isValidKeyFormat('SCLI-eyJhIjoxfQ.c2ln'),
        isTrue,
      );
      expect(LicenseSettingsPanel.isValidKeyFormat('  SCLI-abc.def  '), isTrue);
    });

    test('isValidKeyFormat rejects non-SCLI keys', () {
      expect(LicenseSettingsPanel.isValidKeyFormat('CLRTY-abc.def'), isFalse);
      expect(LicenseSettingsPanel.isValidKeyFormat('nope'), isFalse);
      expect(LicenseSettingsPanel.isValidKeyFormat(''), isFalse);
      expect(LicenseSettingsPanel.isValidKeyFormat('SCLI-nodot'), isFalse);
    });

    test('isLegacyKey detects CLRTY- keys', () {
      expect(LicenseSettingsPanel.isLegacyKey('CLRTY-abc.def'), isTrue);
      expect(
        LicenseSettingsPanel.isLegacyKey('  CLRTY-eyJhIjoxfQ.c2ln  '),
        isTrue,
      );
      expect(LicenseSettingsPanel.isLegacyKey('SCLI-abc.def'), isFalse);
      expect(LicenseSettingsPanel.isLegacyKey(''), isFalse);
    });
  });

  group('PluginSettingsPanel', () {
    test('renders table and actions', () {
      const panel = PluginSettingsPanel(
        plugins: [
          PluginEntry(id: 'p1', name: 'Git', version: '1.0', enabled: true),
          PluginEntry(
            id: 'p2',
            name: 'Files',
            version: '2.0',
            enabled: false,
            updateAvailable: true,
          ),
        ],
      );
      final node = panel.build() as UiColumn;
      expect(node.children.length, 5);
      final ds = panel.dataset();
      expect(ds.rowCount, 2);
      expect(ds.cell(1, 3), 'Available');
    });

    test('row actions include toggle and reorder', () {
      const panel = PluginSettingsPanel(
        plugins: [PluginEntry(id: 'p1', name: 'Git', enabled: true)],
      );
      final actions = panel.rowActions('p1') as UiRow;
      expect(actions.children.length, 4);
      expect((actions.children[0] as UiButton).label, 'Disable');
    });
  });

  group('PresetsSettingsPanel', () {
    test('renders table and CRUD buttons', () {
      const panel = PresetsSettingsPanel(
        presets: [PresetEntry(id: 'pr1', name: 'Rust', model: 'opus')],
      );
      final node = panel.build() as UiColumn;
      expect(node.children.length, 4);
      final ds = panel.dataset();
      expect(ds.cell(0, 0), 'Rust');
    });

    test('preset JSON round-trips', () {
      const p = PresetEntry(id: 'a', name: 'N', model: 'm');
      final back = PresetEntry.fromJson(p.toJson());
      expect(back.name, 'N');
      expect(back.model, 'm');
    });

    test('editor has save/cancel', () {
      const panel = PresetsSettingsPanel();
      const preset = PresetEntry(id: 'x', name: 'X');
      final editor = panel.editor(preset) as UiColumn;
      final actions = editor.children.last as UiRow;
      expect((actions.children[0] as UiButton).label, 'Save');
    });
  });

  group('WorkspacesSettingsPanel', () {
    test('renders add and edit rows', () {
      const panel = WorkspacesSettingsPanel(
        workspaces: [
          WorkspaceEntry(
            id: 'w1',
            name: 'supercli',
            kind: 'local',
            path: '/src',
          ),
        ],
      );
      final node = panel.build() as UiColumn;
      expect(node.children.length, 4);
      final addRow = node.children[2] as UiRow;
      expect(addRow.children.length, 4);
      final ds = panel.dataset();
      expect(ds.cell(0, 1), 'local');
    });
  });

  group('WorktreesSettingsPanel', () {
    test('hides agent worktrees by default', () {
      const panel = WorktreesSettingsPanel(
        worktrees: [
          WorktreeEntry(id: 't1', path: '/a', branch: 'main'),
          WorktreeEntry(id: 't2', path: '/b', branch: 'feat', isAgent: true),
        ],
      );
      expect(panel.dataset().rowCount, 1);
    });

    test('shows agent worktrees when enabled', () {
      const panel = WorktreesSettingsPanel(
        worktrees: [
          WorktreeEntry(id: 't1', path: '/a', branch: 'main'),
          WorktreeEntry(id: 't2', path: '/b', branch: 'feat', isAgent: true),
        ],
        showAgentWorktrees: true,
      );
      expect(panel.dataset().rowCount, 2);
    });

    test('toggle is present', () {
      const panel = WorktreesSettingsPanel();
      final node = panel.build() as UiColumn;
      expect(node.children.length, 5);
    });
  });

  group('NotificationsSettingsPanel', () {
    test('renders toggles and test buttons', () {
      final panel = NotificationsSettingsPanel(settings: AppSettings());
      final node = panel.build() as UiColumn;
      expect(node.children.length, 5);
      final testRow = node.children[3] as UiRow;
      expect((testRow.children[0] as UiButton).label, 'Test on this Mac');
    });
  });

  group('TranscriptsSettingsPanel', () {
    test('renders content toggle', () {
      final panel = TranscriptsSettingsPanel(settings: AppSettings());
      final node = panel.build() as UiColumn;
      expect(node.children.length, 3);
    });
  });

  group('FeaturesSettingsPanel', () {
    test('renders four feature toggles', () {
      final panel = FeaturesSettingsPanel(settings: AppSettings());
      final node = panel.build() as UiColumn;
      expect(node.children.length, 5);
    });
  });

  group('AdvancedSettingsPanel', () {
    test('renders advanced controls', () {
      final panel = AdvancedSettingsPanel(settings: AppSettings());
      final node = panel.build() as UiColumn;
      expect(node.children.length, 6);
    });
  });

  group('SettingsToggle/SettingsSelect', () {
    test('toggle fallback shows On/Off', () {
      final on =
          const SettingsToggle(id: 't', label: 'L', value: true).fallback()
              as UiRow;
      final off =
          const SettingsToggle(id: 't', label: 'L', value: false).fallback()
              as UiRow;
      expect((on.children[1] as UiButton).label, 'On');
      expect((off.children[1] as UiButton).label, 'Off');
    });

    test('select fallback marks selected', () {
      final sel =
          const SettingsSelect(
                id: 's',
                label: 'L',
                options: ['A', 'B'],
                selected: 'B',
              ).fallback()
              as UiColumn;
      final row = sel.children[1] as UiRow;
      expect((row.children[1] as UiButton).label, '● B');
    });

    test('toggle serializes to JSON', () {
      final json = const SettingsToggle(
        id: 't',
        label: 'L',
        value: true,
      ).toJson();
      expect(json['kind'], 'settings-toggle');
      expect(json['value'], true);
    });
  });

  group('HostPickerView', () {
    test('renders hosts and pairing sheet', () {
      final picker = HostPickerView(
        hosts: const [HostEntry(id: 'h1', name: 'Mac', paired: true)],
        pairingCode: '123-456',
        selectedHostName: 'Mac',
      );
      final node = picker.build() as UiColumn;
      // title, actions, nearby title, table, pairing sheet
      expect(node.children.length, 5);
      final ds = picker.nearbyDataset();
      expect(ds.cell(0, 2), 'Paired');
    });
  });

  group('SettingsTab behaviours (SettingsView.swift)', () {
    test('rawValue keeps the experimental spelling for features', () {
      expect(SettingsTab.features.rawValue, 'experimental');
      expect(SettingsTab.appearance.rawValue, 'appearance');
      expect(SettingsTab.agentAccess.rawValue, 'agentAccess');
    });

    test('compatibleRawValue maps legacy deep-link spellings', () {
      // Agents & Apps split (2026-09-16)
      expect(SettingsTab.compatibleRawValue('agentsApps'), SettingsTab.agents);
      expect(SettingsTab.compatibleRawValue('mcp'), SettingsTab.agents);
      expect(SettingsTab.compatibleRawValue('presets'), SettingsTab.agents);
      expect(
        SettingsTab.compatibleRawValue('sessions'),
        SettingsTab.agentAccess,
      );
      expect(
        SettingsTab.compatibleRawValue('browser'),
        SettingsTab.agentAccess,
      );
      expect(
        SettingsTab.compatibleRawValue('profiles'),
        SettingsTab.workspaces,
      );
      expect(SettingsTab.compatibleRawValue('features'), SettingsTab.features);
      expect(
        SettingsTab.compatibleRawValue('experimental'),
        SettingsTab.features,
      );
      // Current spellings resolve directly
      expect(
        SettingsTab.compatibleRawValue('appearance'),
        SettingsTab.appearance,
      );
      expect(SettingsTab.compatibleRawValue('advanced'), SettingsTab.advanced);
      expect(SettingsTab.compatibleRawValue('mobile'), SettingsTab.mobile);
      // Unknown spellings return null (Swift: no 'license' case in the code)
      expect(SettingsTab.compatibleRawValue('license'), isNull);
      expect(SettingsTab.compatibleRawValue('nope'), isNull);
      expect(SettingsTab.compatibleRawValue(''), isNull);
    });

    test('visibleCases gates tabs by feature flags', () {
      List<SettingsTab> visible({
        bool sessionsMcp = false,
        bool browserMcp = false,
        bool workspacesEnabled = false,
        bool worktreesEnabled = false,
        bool mobileRemoteControlEnabled = false,
      }) => SettingsTab.visibleCases(
        sessionsMcp: sessionsMcp,
        browserMcp: browserMcp,
        workspacesEnabled: workspacesEnabled,
        worktreesEnabled: worktreesEnabled,
        mobileRemoteControlEnabled: mobileRemoteControlEnabled,
      );

      // All flags off: agentAccess, mobile, workspaces, worktrees hidden;
      // computer and presets never show their old panels.
      var v = visible();
      expect(
        v,
        orderedEquals([
          SettingsTab.agents,
          SettingsTab.plugins,
          SettingsTab.appearance,
          SettingsTab.transcripts,
          SettingsTab.notifications,
          SettingsTab.features,
          SettingsTab.advanced,
        ]),
      );
      // sessionsMcp on → agentAccess appears
      expect(visible(sessionsMcp: true), contains(SettingsTab.agentAccess));
      // browserMcp on → agentAccess appears
      expect(visible(browserMcp: true), contains(SettingsTab.agentAccess));
      // workspaces flag → workspaces tab
      expect(
        visible(workspacesEnabled: true),
        contains(SettingsTab.workspaces),
      );
      // worktrees flag → worktrees tab
      expect(visible(worktreesEnabled: true), contains(SettingsTab.worktrees));
      // mobile flag → mobile (Remote Control) tab
      expect(
        visible(mobileRemoteControlEnabled: true),
        contains(SettingsTab.mobile),
      );
    });

    test('resolved falls back to first visible tab', () {
      SettingsTab resolved(SettingsTab selected) => SettingsTab.resolved(
        selected,
        sessionsMcp: false,
        browserMcp: false,
        workspacesEnabled: true,
        worktreesEnabled: false,
        mobileRemoteControlEnabled: false,
      );
      expect(resolved(SettingsTab.appearance), SettingsTab.appearance);
      // mobile is gated off → falls back to workspaces (first visible)
      expect(resolved(SettingsTab.mobile), SettingsTab.workspaces);
      // computer never visible → falls back too
      expect(resolved(SettingsTab.computer), SettingsTab.workspaces);
    });

    test('iconName covers every tab', () {
      for (final tab in SettingsTab.values) {
        expect(
          tab.iconName,
          isNotEmpty,
          reason: 'SettingsTab.${tab.name} needs an icon',
        );
        expect(tab.iconName, startsWith('settings-'));
      }
      // Spot checks against the Swift ChromeIcon mapping
      expect(SettingsTab.workspaces.iconName, 'settings-workspaces');
      expect(SettingsTab.mobile.iconName, 'settings-remote');
      expect(SettingsTab.features.iconName, 'settings-features');
    });

    test('title matches the Swift titles', () {
      expect(SettingsTab.appearance.title, 'Appearance');
      expect(SettingsTab.agents.title, 'Agents');
      expect(SettingsTab.plugins.title, 'Plugins');
      expect(SettingsTab.agentAccess.title, 'Agent access');
      expect(SettingsTab.presets.title, 'Agents');
      expect(SettingsTab.mobile.title, 'Remote Control');
      expect(SettingsTab.workspaces.title, 'Workspaces');
      expect(SettingsTab.transcripts.title, 'Transcripts');
      expect(SettingsTab.notifications.title, 'Notifications');
      expect(SettingsTab.computer.title, 'Computer use');
      expect(SettingsTab.worktrees.title, 'Worktrees');
      expect(SettingsTab.features.title, 'Features');
      expect(SettingsTab.advanced.title, 'Advanced');
    });

    test('hostScopedCases are the Host-contract tabs', () {
      final scoped = SettingsTab.hostScopedCases;
      expect(
        scoped,
        orderedEquals([
          SettingsTab.agents,
          SettingsTab.plugins,
          SettingsTab.agentAccess,
          SettingsTab.presets,
          SettingsTab.appearance,
          SettingsTab.transcripts,
          SettingsTab.notifications,
          SettingsTab.computer,
          SettingsTab.features,
          SettingsTab.advanced,
        ]),
      );
      // Non-scoped tabs (local-only UI) are excluded
      expect(scoped, isNot(contains(SettingsTab.workspaces)));
      expect(scoped, isNot(contains(SettingsTab.mobile)));
      expect(scoped, isNot(contains(SettingsTab.worktrees)));
    });
  });
}
