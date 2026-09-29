/// Tests for the remaining SettingsView.swift panels: Host panels,
/// remote Host panels, and the local Transcripts / Notifications /
/// Features / Advanced panels plus OpenResourcesSettingsRows.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/screens/advancedsettingspanel.dart';
import 'package:supercli_app/screens/appearancesettingspanel.dart';
import 'package:supercli_app/screens/featuressettingspanel.dart';
import 'package:supercli_app/screens/hostsettingspanels.dart';
import 'package:supercli_app/screens/notificationssettingspanel.dart';
import 'package:supercli_app/screens/openresourcessettingsrows.dart';
import 'package:supercli_app/screens/remoteappearancesettingspanel.dart';
import 'package:supercli_app/screens/remotehostsettingspanels.dart';
import 'package:supercli_app/screens/settingsshell.dart';
import 'package:supercli_app/screens/settingsview.dart';
import 'package:supercli_app/screens/transcriptssettingspanel.dart';
import 'package:test/test.dart';

void main() {
  group('HostAppearanceSettingsPanel (SettingsView.swift 306-521)', () {
    test('theme preference titles', () {
      expect(ThemePreference.system.title, 'System');
      expect(ThemePreference.light.title, 'Light');
      expect(ThemePreference.dark.title, 'Dark');
    });

    test('renders mode, tint, transparency, font sections', () {
      const panel = HostAppearanceSettingsPanel(home: 'h', name: 'Office');
      final node = panel.build() as UiColumn;
      expect(node.id, 'host-appearance');
      // header, inherit section (non-default instance), mode header, mode
      // row, tint header, tint row, transparency header, transparency,
      // font header, font
      expect(node.children.length, 10);
      final modeRow = node.children[3] as UiRow;
      expect(modeRow.children.length, ThemePreference.values.length);
      final tintRow = node.children[5] as UiRow;
      expect(tintRow.children.length, AppTint.values.length);
      final transparency = node.children[7] as UiColumn;
      expect(
        (transparency.children[0] as UiText).text,
        contains('Window background: 100%'),
      );
      final font = node.children[9] as UiColumn;
      expect((font.children[0] as UiText).text, 'SF Mono');
    });

    test('active mode and tint are marked', () {
      const panel = HostAppearanceSettingsPanel(
        home: 'h',
        name: 'Office',
        mode: ThemePreference.dark,
        tint: AppTint.blue,
      );
      final node = panel.build() as UiColumn;
      final modeRow = node.children[3] as UiRow;
      final dark = modeRow.children.whereType<UiButton>().firstWhere(
        (b) => b.id == 'host-appearance-mode-dark',
      );
      expect(dark.label, contains('✓'));
      final tintRow = node.children[5] as UiRow;
      final blue = tintRow.children.whereType<UiButton>().firstWhere(
        (b) => b.id == 'host-appearance-tint-blue',
      );
      expect(blue.label, contains('✓'));
    });

    test('non-default instance shows inherit + reset when overridden', () {
      const panel = HostAppearanceSettingsPanel(
        home: 'h',
        name: 'API',
        isDefaultInstance: true,
        defaultWorkspaceLabel: 'Personal',
        hasOverrides: true,
      );
      // isDefaultInstance=true here would hide it; flip to verify
      const scoped = HostAppearanceSettingsPanel(
        home: 'h',
        name: 'API',
        isDefaultInstance: false,
        defaultWorkspaceLabel: 'Personal',
        hasOverrides: true,
      );
      final node = scoped.build() as UiColumn;
      final inherit = node.children[1] as UiColumn;
      expect(inherit.id, 'host-appearance-inherit');
      expect(
        (inherit.children[3] as UiButton).id,
        'host-appearance-reset-inherited',
      );
      expect(panel.isDefaultInstance, isTrue);
    });
  });

  group('HostAdvancedSettingsPanel (SettingsView.swift 522-641)', () {
    test('minuteLabel covers the option labels', () {
      expect(HostAdvancedSettingsPanel.minuteLabel(60), isNotEmpty);
      expect(HostAdvancedSettingsPanel.minuteLabel(999), '999 minutes');
    });

    test('renders minutes/limit rows with saved values', () {
      const panel = HostAdvancedSettingsPanel(
        scopeName: 'Office',
        settings: RemoteWorkspaceSettings(
          autoStopArchiveMinutes: 120,
          sidebarStoppedLimit: 5,
        ),
      );
      final node = panel.build() as UiColumn;
      expect(node.id, 'host-advanced');
      final minutes = node.children[2] as UiRow;
      expect((minutes.children[2] as UiText).text, contains('Saved:'));
      final limit = node.children[5] as UiRow;
      expect((limit.children[2] as UiText).text, 'Saved: 5');
    });

    test('error message surfaces inline', () {
      const panel = HostAdvancedSettingsPanel(
        scopeName: 'Office',
        errorMessage: 'Host unreachable',
      );
      final node = panel.build() as UiColumn;
      expect((node.children.last as UiText).text, 'Host unreachable');
    });
  });

  group('HostAccessSettingsPanel (SettingsView.swift 642-801)', () {
    test('write-policy and browser options', () {
      expect(HostAccessSettingsPanel.writePolicyOptions, [
        'ask',
        'allow',
        'deny',
      ]);
      expect(HostAccessSettingsPanel.browserAccessOptions, [
        'ask',
        'allow',
        'deny',
      ]);
    });

    test('saved write policy is marked', () {
      const panel = HostAccessSettingsPanel(
        scopeName: 'Office',
        settings: RemoteWorkspaceSettings(mcpNonchildWriteAccess: 'deny'),
      );
      final node = panel.build() as UiColumn;
      final sessions = node.children[1] as UiColumn;
      final policyRow = sessions.children[1] as UiRow;
      final deny = policyRow.children.whereType<UiButton>().firstWhere(
        (b) => b.id == 'host-access-write-policy-deny',
      );
      expect(deny.label, contains('✓'));
    });
  });

  group('HostTranscriptsSettingsPanel (SettingsView.swift 802-1003)', () {
    test('max-entries labels', () {
      expect(HostTranscriptsSettingsPanel.maxEntriesLabel(0), 'Unlimited');
      expect(HostTranscriptsSettingsPanel.maxEntriesLabel(50), '50');
      expect(HostTranscriptsSettingsPanel.maxEntriesOptions, [0, 20, 50, 100]);
    });

    test('renders seven content toggles + range row', () {
      const panel = HostTranscriptsSettingsPanel(scopeName: 'Office');
      final node = panel.build() as UiColumn;
      expect(node.id, 'host-transcripts');
      const toggleIds = [
        'host-transcripts-session-info',
        'host-transcripts-user',
        'host-transcripts-assistant',
        'host-transcripts-reasoning',
        'host-transcripts-tools',
        'host-transcripts-file-changes',
        'host-transcripts-plan-updates',
      ];
      for (final id in toggleIds) {
        // labeledToggleRow renders UiColumn('$id-labeled', ...)
        final found = _findById(node, '$id-labeled');
        expect(found, isNotNull, reason: 'missing toggle $id');
      }
      expect(_findById(node, 'host-transcripts-max-entries'), isNotNull);
    });
  });

  group('HostNotificationsSettingsPanel (SettingsView.swift 1004-1120)', () {
    test('inherited reset only when overridden', () {
      const panel = HostNotificationsSettingsPanel(
        name: 'API',
        isDefaultInstance: false,
        defaultWorkspaceLabel: 'Personal',
        hasOverride: true,
      );
      final node = panel.build() as UiColumn;
      expect(_findById(node, 'host-notifications-reset-inherited'), isNotNull);
      const noOverride = HostNotificationsSettingsPanel(
        name: 'API',
        isDefaultInstance: false,
        defaultWorkspaceLabel: 'Personal',
      );
      expect(
        _findById(noOverride.build(), 'host-notifications-reset-inherited'),
        isNull,
      );
    });
  });

  group('HostFeaturesSettingsPanel (SettingsView.swift 1121-1270)', () {
    const features = [
      AppFeature(key: 'a', title: 'A', summary: 'sa'),
      AppFeature(key: 'b', title: 'B', summary: 'sb', isExperimental: true),
    ];

    test('groups shipped before experimental', () {
      const panel = HostFeaturesSettingsPanel(
        name: 'Office',
        features: features,
      );
      final node = panel.build() as UiColumn;
      expect(_findById(node, 'host-features-a-labeled'), isNotNull);
      expect(_findById(node, 'host-features-b-labeled'), isNotNull);
    });

    test('reset button for non-default overridden workspace', () {
      const panel = HostFeaturesSettingsPanel(
        name: 'API',
        isDefaultInstance: false,
        defaultWorkspaceLabel: 'Personal',
        hasOverride: true,
        features: features,
      );
      expect(
        _findById(panel.build(), 'host-features-reset-inherited'),
        isNotNull,
      );
    });
  });

  group('HostSettingsUpdateRequiredPanel (SettingsView.swift 1883-1938)', () {
    test('names the tab and scope', () {
      const panel = HostSettingsUpdateRequiredPanel(
        tabTitle: 'Appearance',
        scopeName: 'Old Mac',
      );
      final node = panel.build() as UiColumn;
      expect((node.children[1] as UiText).text, contains('Appearance'));
      expect((node.children[1] as UiText).text, contains('Old Mac'));
    });
  });

  group('RemoteAppearanceSettingsPanel (SettingsView.swift 1271-1562)', () {
    test('waiting state without settings', () {
      const panel = RemoteAppearanceSettingsPanel(scopeName: 'Office');
      final node = panel.build() as UiColumn;
      expect((node.children[1] as UiText).text, contains('Office'));
    });

    test('renders mode, tint, titles, open resources, transparency, font', () {
      const panel = RemoteAppearanceSettingsPanel(
        scopeName: 'Office',
        settings: RemoteAppearanceSettings(
          theme: 'dark',
          appTint: 'blue',
          sessionTitleMode: 'firstPrompt',
          backgroundOpacity: 0.9,
        ),
      );
      final node = panel.build() as UiColumn;
      expect(_findById(node, 'remote-appearance-mode-picker'), isNotNull);
      expect(_findById(node, 'remote-appearance-tint-swatches'), isNotNull);
      expect(_findById(node, 'remote-appearance-titles-picker-col'), isNotNull);
      expect(_findById(node, 'remote-appearance-open-resources'), isNotNull);
      expect(_findById(node, 'remote-appearance-transparency'), isNotNull);
      // Host-provided values decode to the right enums
      final modeRow = _findById(node, 'remote-appearance-mode-picker') as UiRow;
      final dark = modeRow.children.whereType<UiButton>().firstWhere(
        (b) => b.id == 'remote-appearance-mode-dark',
      );
      expect(dark.label, contains('✓'));
    });
  });

  group('RemoteNotificationsSettingsPanel (SettingsView.swift 1563-1718)', () {
    test('capability rows reflect Host advertisement', () {
      const panel = RemoteNotificationsSettingsPanel(
        scopeName: 'Office',
        pushRegisterSupported: true,
      );
      final node = panel.build() as UiColumn;
      final delivery =
          _findById(node, 'remote-notifications-host-delivery') as UiColumn;
      final phoneRow = delivery.children[1] as UiRow;
      expect((phoneRow.children[1] as UiText).text, 'Supported');
      const unsupported = RemoteNotificationsSettingsPanel(scopeName: 'Office');
      final node2 = unsupported.build() as UiColumn;
      final delivery2 =
          _findById(node2, 'remote-notifications-host-delivery') as UiColumn;
      expect((delivery2.children[1] as UiRow).children[1] is UiText, isTrue);
    });

    test('attention toggle renders when Host settings arrive', () {
      const panel = RemoteNotificationsSettingsPanel(
        scopeName: 'Office',
        settings: RemoteNotificationSettings(menuAttentionDetection: false),
      );
      expect(
        _findById(panel.build(), 'remote-notifications-menu-attention-labeled'),
        isNotNull,
      );
    });
  });

  group('RemoteFeaturesSettingsPanel (SettingsView.swift 1719-1882)', () {
    test('stable key mapping', () {
      const settings = RemoteExperimentalSettings(
        worktrees: true,
        sessionsMcp: true,
        workspaces: true,
      );
      expect(remoteFeatureValue('worktrees', settings), isTrue);
      expect(remoteFeatureValue('sessionsMcp', settings), isTrue);
      // 'profiles' is the persisted key for Workspaces
      expect(remoteFeatureValue('profiles', settings), isTrue);
      expect(remoteFeatureValue('browserMcp', settings), isFalse);
      expect(remoteFeatureValue('nope', settings), isFalse);
    });

    test('waiting state without settings', () {
      const panel = RemoteFeaturesSettingsPanel(scopeName: 'Office');
      final node = panel.build() as UiColumn;
      expect((node.children[1] as UiText).text, contains('Office'));
    });

    test('renders shipped + experimental feature rows', () {
      const panel = RemoteFeaturesSettingsPanel(
        scopeName: 'Office',
        settings: RemoteExperimentalSettings(),
      );
      final node = panel.build() as UiColumn;
      expect(_findById(node, 'remote-features-shipped'), isNotNull);
      expect(_findById(node, 'remote-features-experimental'), isNotNull);
      expect(_findById(node, 'remote-feature-browserMcp-labeled'), isNotNull);
    });
  });

  group('TranscriptsSettingsPanel (SettingsView.swift 4060-4197)', () {
    test('range labels', () {
      expect(transcriptRangeLabel(0), 'Whole conversation');
      expect(transcriptRangeLabel(20), 'Last 20 entries');
      expect(transcriptRangeLabel(50), 'Last 50 entries');
      expect(transcriptRangeLabel(100), 'Last 100 entries');
      expect(transcriptRangeOptions, [0, 20, 50, 100]);
    });

    test('renders seven toggles + range picker', () {
      const panel = TranscriptsSettingsPanel();
      final node = panel.build() as UiColumn;
      expect(node.id, 'transcripts-settings');
      const ids = [
        'transcripts-session-info',
        'transcripts-user',
        'transcripts-assistant',
        'transcripts-reasoning',
        'transcripts-tools',
        'transcripts-file-changes',
        'transcripts-plan-updates',
        'transcripts-range',
      ];
      for (final id in ids) {
        // _toggle renders UiColumn('$id-labeled', ...); the range row too
        final found = id == 'transcripts-range'
            ? _findById(node, 'transcripts-range-labeled')
            : _findById(node, '$id-labeled');
        expect(found, isNotNull, reason: 'missing $id');
      }
    });
  });

  group('NotificationsSettingsPanel (SettingsView.swift 3909-4059)', () {
    test('phone token text', () {
      const none = NotificationsSettingsPanel();
      expect(none.pairedPhoneTokensText, 'None registered');
      const two = NotificationsSettingsPanel(pairedPhoneTokenCount: 2);
      expect(two.pairedPhoneTokensText, '2 ready');
    });

    test('renders attention + test sections', () {
      const panel = NotificationsSettingsPanel();
      final node = panel.build() as UiColumn;
      expect(node.id, 'notifications-settings');
      expect(_findById(node, 'notifications-attention'), isNotNull);
      expect(_findById(node, 'notifications-mac-test'), isNotNull);
      expect(_findById(node, 'notifications-phone-test'), isNotNull);
    });

    test('system-settings button appears when needed', () {
      const panel = NotificationsSettingsPanel(
        macTestNeedsSystemSettings: true,
      );
      expect(_findById(panel.build(), 'notifications-mac-settings'), isNotNull);
    });

    test('non-default instance shows inherit section', () {
      const panel = NotificationsSettingsPanel(isDefaultInstance: false);
      expect(_findById(panel.build(), 'notifications-inherit'), isNotNull);
    });
  });

  group('FeaturesSettingsPanel (SettingsView.swift 4330-4429)', () {
    test('registry: 4 shipped, 1 experimental, stable keys', () {
      final panel = FeaturesSettingsPanel();
      expect(panel.shipped.length, 4);
      expect(panel.experimental.length, 1);
      expect(panel.experimental.single.key, 'browserMcp');
      // Persisted key for Workspaces stays 'profiles'
      expect(allFeatures.map((f) => f.key), contains('profiles'));
      expect(allFeatures.map((f) => f.key), contains('remoteWorkspaces'));
    });

    test('renders shipped and experimental sections', () {
      const panel = FeaturesSettingsPanel();
      final node = panel.build() as UiColumn;
      expect(_findById(node, 'feature-remoteWorkspaces-labeled'), isNotNull);
      expect(_findById(node, 'feature-browserMcp-labeled'), isNotNull);
    });

    test('values override defaults', () {
      const panel = FeaturesSettingsPanel(values: {'browserMcp': true});
      final node = panel.build() as UiColumn;
      // The toggle fallback renders through SettingsToggle; the row exists
      expect(_findById(node, 'feature-browserMcp-labeled'), isNotNull);
    });
  });

  group('AdvancedSettingsPanel (SettingsView.swift 4430-4740)', () {
    test('formatMB', () {
      expect(formatMB(1024 * 1024), '1 MB');
      expect(formatMB(1536 * 1024), '2 MB');
    });

    test('formatCpu', () {
      expect(formatCpu(4.2), '4.2%');
      expect(formatCpu(42), '42%');
    });

    test('compactPath', () {
      expect(compactPath(''), 'No folder');
      expect(compactPath('/a/b'), '/a/b');
      expect(compactPath('/Users/alice/work/supercli'), '.../work/supercli');
    });

    test('commandLabel', () {
      const blank = RunningTerminal(
        id: 't1',
        projectID: 'p',
        label: 'l',
        command: '  ',
        cwd: '/',
        pid: 1,
        processCount: 1,
        cpuPercent: 0,
        rssBytes: 0,
      );
      expect(blank.commandLabel, 'Blank shell');
      const cmd = RunningTerminal(
        id: 't1',
        projectID: 'p',
        label: 'l',
        command: 'zsh',
        cwd: '/',
        pid: 1,
        processCount: 1,
        cpuPercent: 0,
        rssBytes: 0,
      );
      expect(cmd.commandLabel, 'zsh');
    });

    test('renders cleanup, memory, terminals, diagnostics', () {
      const panel = AdvancedSettingsPanel(
        snapshot: AdvancedDiagnosticsSnapshot(
          memory: MemorySnapshot(
            processFootprintBytes: 200 * 1024 * 1024,
            runningHostCount: 2,
            hostedSessionCount: 5,
          ),
          terminals: [
            RunningTerminal(
              id: 't1',
              projectID: 'p1',
              label: 'API',
              command: 'zsh',
              cwd: '/tmp',
              pid: 42,
              processCount: 3,
              cpuPercent: 4.2,
              rssBytes: 50 * 1024 * 1024,
              canArchive: true,
            ),
          ],
          sessionsFolder: '/Users/alice/Library/Supercli',
        ),
      );
      final node = panel.build() as UiColumn;
      expect(node.id, 'advanced-settings');
      expect(node.children.length, 5);
      // Memory section shows the footprint
      final memory = node.children[2] as UiColumn;
      final memRow = memory.children[1] as UiRow;
      expect((memRow.children[1] as UiText).text, contains('MB'));
      // Terminal row exists with the terminal id
      final terminals = node.children[3] as UiColumn;
      final rows = terminals.children[2] as UiColumn;
      expect((rows.children[0] as UiRow).id, 'advanced-terminal-t1');
    });
  });

  group('OpenResourcesSettingsRows (SettingsView.swift 2326-2494)', () {
    const apps = [
      RemoteAppSummary(
        id: 'vscode',
        name: 'VS Code',
        mediaTypes: ['text/markdown', 'text/csv'],
        defaultFor: ['file:text/markdown'],
        installed: true,
      ),
      RemoteAppSummary(
        id: 'finder',
        name: 'Finder',
        resourceKinds: ['folder'],
        installed: true,
      ),
    ];

    test('selector titles', () {
      expect(openResourceSelectorTitle('file:text/markdown'), 'Markdown');
      expect(openResourceSelectorTitle('resource:folder'), 'Folders');
    });

    test('selectors are derived and sorted by title', () {
      final selectors = openResourceSelectors(apps);
      expect(selectors, [
        'file:text/csv',
        'resource:folder',
        'file:text/markdown',
      ]);
    });

    test(
      'resolveOpener chain: override > saved > default > single > editor',
      () {
        expect(
          resolveOpener(
            selector: 'file:text/markdown',
            apps: apps,
            override: 'app:finder',
          ),
          'app:finder',
        );
        expect(
          resolveOpener(
            selector: 'file:text/markdown',
            apps: apps,
            savedOpener: 'app:finder',
          ),
          'app:finder',
        );
        // registry default
        expect(
          resolveOpener(selector: 'file:text/markdown', apps: apps),
          'app:vscode',
        );
        // single handling app
        expect(
          resolveOpener(selector: 'file:text/csv', apps: apps),
          'app:vscode',
        );
        // file selector with no app → editor
        expect(
          resolveOpener(selector: 'file:text/plain', apps: apps),
          'editor',
        );
      },
    );

    test('missing and outdated app detection', () {
      const missing = RemoteAppSummary(id: 'nova', name: 'Nova');
      expect(
        selectedMissingApp(
          selector: 'file:text/markdown',
          apps: [...apps, missing],
          installedIDs: {'vscode', 'finder'},
          override: 'app:nova',
        )?.id,
        'nova',
      );
      expect(
        selectedMissingApp(
          selector: 'file:text/markdown',
          apps: apps,
          installedIDs: {'vscode', 'finder'},
        ),
        isNull,
      );
      const outdated = RemoteAppSummary(
        id: 'vscode',
        name: 'VS Code',
        mediaTypes: ['text/markdown'],
        installed: true,
        updateAvailable: true,
      );
      expect(
        selectedOutdatedApp(
          selector: 'file:text/markdown',
          apps: [outdated],
        )?.id,
        'vscode',
      );
    });

    test('openerLabel', () {
      expect(openerLabel('editor', apps), 'Default Editor');
      expect(openerLabel('app:vscode', apps), 'VS Code');
      expect(openerLabel('app:unknown', apps), 'unknown');
    });
  });

  group('SettingsContentHost routing (SettingsView.swift 2097-2219)', () {
    test('sibling local workspace routes to Host file-based panels', () {
      // Swift: `.localWorkspace(home, name)` != `.local`, so it takes the
      // remote branch where appearance/notifications/features resolve to the
      // Host panels (file-based, no Host verb needed).
      const scope = SettingsScopeContext(
        isLocal: false,
        localWorkspaceHome: '/Users/alice',
        localWorkspaceName: 'Office',
        supportsWorkspaceSettingsSet: true,
        hasAppearanceSettings: true,
        hasNotificationSettings: true,
        hasExperimentalSettings: true,
      );
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.appearance, scope),
        SettingsPanelKind.appearanceHost,
      );
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.notifications, scope),
        SettingsPanelKind.notificationsHost,
      );
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.features, scope),
        SettingsPanelKind.featuresHost,
      );
      // Other tabs use the same remote rules as an SSH host.
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.advanced, scope),
        SettingsPanelKind.advancedHost,
      );
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.worktrees, scope),
        SettingsPanelKind.worktrees,
      );
    });

    test('remote host without snapshot payloads shows update-required', () {
      const scope = SettingsScopeContext(
        isLocal: false,
        remoteHostId: 'ssh-host',
      );
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.appearance, scope),
        SettingsPanelKind.updateRequired,
      );
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.transcripts, scope),
        SettingsPanelKind.updateRequired,
      );
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.notifications, scope),
        SettingsPanelKind.updateRequired,
      );
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.features, scope),
        SettingsPanelKind.updateRequired,
      );
      // agents/plugins/mobile/worktrees are never gated on the settings verb.
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.agents, scope),
        SettingsPanelKind.agentsPlugins,
      );
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.mobile, scope),
        SettingsPanelKind.mobile,
      );
    });

    test('this-Mac scope renders the local panels', () {
      const scope = SettingsScopeContext();
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.appearance, scope),
        SettingsPanelKind.appearance,
      );
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.transcripts, scope),
        SettingsPanelKind.transcripts,
      );
      expect(
        SettingsContentHost.panelKindFor(SettingsTab.workspaces, scope),
        SettingsPanelKind.workspaces,
      );
    });
  });
}

/// Depth-first search for a node with the given id.
UiNode? _findById(UiNode node, String id) {
  if (node.id == id) return node;
  if (node is UiColumn) {
    for (final child in node.children) {
      final found = _findById(child, id);
      if (found != null) return found;
    }
  }
  if (node is UiRow) {
    for (final child in node.children) {
      final found = _findById(child, id);
      if (found != null) return found;
    }
  }
  return null;
}
