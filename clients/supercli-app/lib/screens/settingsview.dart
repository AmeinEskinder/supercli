/// Settings: tabbed preferences panel.
///
/// Port of `SettingsView.swift` (4910 lines). Tabs: General, Sessions,
/// Agent Access, Browser, Workspaces, Worktrees, Plugins, Presets,
/// Notifications, Transcripts, Features, Advanced, License, Remote,
/// Developer.
///
/// Each tab renders its panel against a shared [AppSettings] model; edits
/// serialize to the Host's `settings.workspace.set` wire format for
/// allowlisted keys (see settingspanels.dart).
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
library;

import 'package:gpuidart/gpuidart.dart';

import 'hostpickerview.dart';
import 'licensesettings.dart';
import 'pluginsettingspanel.dart';
import 'presetssettingspanel.dart';
import 'sessionsaccesssections.dart';
import 'settingspanels.dart';
import 'workspacessettingspanel.dart';
import 'worktreessettingspanel.dart';

/// Settings tab identifiers.
enum SettingsTab {
  general,
  sessions,
  agentAccess,
  browser,
  workspaces,
  worktrees,
  plugins,
  presets,
  notifications,
  transcripts,
  features,
  advanced,
  license,
  remote,
  developer,
}

/// The settings panel with tab navigation and scope picker.
///
/// Covers checklist items 200 (scope picker), 201 (workspaces), 202
/// (agents — see AgentAccessSettingsPanel), 203 (plugins), 204/205 (agent
/// access), 206 (appearance, in General tab), 207 (remote control), 209
/// (license), 210 (transcripts), 211 (notifications), 212 (worktrees),
/// 213 (features), 214 (advanced), 216 (presets).
final class SettingsView {
  SettingsView({
    AppSettings? settings,
    this.activeTab = SettingsTab.general,
    this.hostPicker,
    this.plugins = const [],
    this.presets = const [],
    this.workspaces = const [],
    this.worktrees = const [],
    this.showAgentWorktrees = false,
    this.approvedPairs = const [],
    this.license = const LicenseSettingsPanel(),
  }) : settings = settings ?? AppSettings();

  final AppSettings settings;
  final SettingsTab activeTab;
  final HostPickerView? hostPicker;
  final List<PluginEntry> plugins;
  final List<PresetEntry> presets;
  final List<WorkspaceEntry> workspaces;
  final List<WorktreeEntry> worktrees;
  final bool showAgentWorktrees;
  final List<ApprovedPair> approvedPairs;
  final LicenseSettingsPanel license;

  static const tabTitles = {
    SettingsTab.general: 'General',
    SettingsTab.sessions: 'Sessions',
    SettingsTab.agentAccess: 'Agent Access',
    SettingsTab.browser: 'Browser',
    SettingsTab.workspaces: 'Workspaces',
    SettingsTab.worktrees: 'Worktrees',
    SettingsTab.plugins: 'Plugins',
    SettingsTab.presets: 'Presets',
    SettingsTab.notifications: 'Notifications',
    SettingsTab.transcripts: 'Transcripts',
    SettingsTab.features: 'Features',
    SettingsTab.advanced: 'Advanced',
    SettingsTab.license: 'License',
    SettingsTab.remote: 'Remote',
    SettingsTab.developer: 'Developer',
  };

  UiNode build() {
    return UiRow('settings', [
      UiColumn('settings-tabs', [
        const UiText('settings-heading', 'Settings'),
        for (final tab in SettingsTab.values)
          UiButton('settings-tab-${tab.name}',
              '${tab == activeTab ? '● ' : ''}${tabTitles[tab]}'),
      ]),
      UiColumn('settings-content', [
        UiText('settings-title', tabTitles[activeTab] ?? ''),
        _panelFor(activeTab),
      ]),
    ]);
  }

  UiNode _panelFor(SettingsTab tab) {
    switch (tab) {
      case SettingsTab.general:
        return GeneralSettingsPanel(settings: settings).build();
      case SettingsTab.sessions:
        return SessionsSettingsPanel(settings: settings).build();
      case SettingsTab.agentAccess:
        return AgentAccessSettingsPanel(
          settings: settings,
          approvedPairs: approvedPairs,
        ).build();
      case SettingsTab.browser:
        return BrowserAccessSections(settings: settings).build();
      case SettingsTab.workspaces:
        return WorkspacesSettingsPanel(workspaces: workspaces).build();
      case SettingsTab.worktrees:
        return WorktreesSettingsPanel(
          worktrees: worktrees,
          showAgentWorktrees: showAgentWorktrees,
        ).build();
      case SettingsTab.plugins:
        return PluginSettingsPanel(plugins: plugins).build();
      case SettingsTab.presets:
        return PresetsSettingsPanel(presets: presets).build();
      case SettingsTab.notifications:
        return NotificationsSettingsPanel(settings: settings).build();
      case SettingsTab.transcripts:
        return TranscriptsSettingsPanel(settings: settings).build();
      case SettingsTab.features:
        return FeaturesSettingsPanel(settings: settings).build();
      case SettingsTab.advanced:
        return AdvancedSettingsPanel(settings: settings).build();
      case SettingsTab.license:
        return license.build();
      case SettingsTab.remote:
        return (hostPicker ?? HostPickerView()).build();
      case SettingsTab.developer:
        return const DeveloperSettingsPanel().build();
    }
  }
}
