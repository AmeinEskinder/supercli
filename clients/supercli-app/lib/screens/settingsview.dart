/// Settings: tabbed preferences panel.
///
/// Port of `SettingsView.swift` (4910 lines). The Swift version has tabs for:
/// General, Sessions, Terminal, Workspaces, Worktrees, Plugins, Presets,
/// Agent Access, License, Remote, and Developer. Each tab is a separate
/// panel (see the *settingspanel.dart files).
///
/// The gpuidart port uses UiTable-free layout with UiText section headers.
library;

import 'package:gpuidart/gpuidart.dart';

/// Settings tab identifiers.
enum SettingsTab {
  general,
  sessions,
  terminal,
  workspaces,
  worktrees,
  plugins,
  presets,
  agentAccess,
  license,
  remote,
  developer,
}

/// The settings panel with tab navigation.
final class SettingsView {
  SettingsView({
    this.activeTab = SettingsTab.general,
  });

  final SettingsTab activeTab;

  static const tabTitles = {
    SettingsTab.general: 'General',
    SettingsTab.sessions: 'Sessions',
    SettingsTab.terminal: 'Terminal',
    SettingsTab.workspaces: 'Workspaces',
    SettingsTab.worktrees: 'Worktrees',
    SettingsTab.plugins: 'Plugins',
    SettingsTab.presets: 'Presets',
    SettingsTab.agentAccess: 'Agent Access',
    SettingsTab.license: 'License',
    SettingsTab.remote: 'Remote',
    SettingsTab.developer: 'Developer',
  };

  UiNode build() {
    return UiRow('settings', [
      UiColumn('settings-tabs', [
        for (final tab in SettingsTab.values)
          UiButton('settings-tab-${tab.name}',
              '${tab == activeTab ? '● ' : ''}${tabTitles[tab]}'),
      ]),
      UiColumn('settings-content', [
        UiText('settings-title', tabTitles[activeTab] ?? ''),
        const UiText('settings-body',
            'Settings content renders here. See the individual panel screens.'),
      ]),
    ]);
  }
}
