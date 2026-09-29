/// Settings: tab identifiers, deep-link compatibility, and feature gates.
///
/// Port of `SettingsTab` (SettingsView.swift, lines 44-174). The Swift enum
/// drives the settings sidebar nav; deep-link spellings, feature gates, and
/// icons are ported here as [rawValue]/[compatibleRawValue], [visibleCases],
/// [hostScopedCases], [title], and [iconName].
///
/// Enum order is nav order (Swift): workspaces leads the nav. `features`
/// keeps the released deep-link/snapshot spelling "experimental" as its raw
/// value — never change it.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
library;

/// Settings tab identifiers.
///
/// Port of `SettingsTab` (SettingsView.swift). Cases: workspaces, agents,
/// plugins, agentAccess, presets, appearance, mobile, transcripts,
/// notifications, computer, worktrees, features, advanced.
enum SettingsTab {
  workspaces,
  agents,
  plugins,
  agentAccess,
  presets,
  appearance,
  mobile,
  transcripts,
  notifications,
  computer,
  worktrees,
  features,
  advanced;

  /// Wire/deep-link spelling. `features` keeps the released spelling
  /// "experimental" from when the tab was called Experimental (Swift:
  /// `case features = "experimental"`); never change it.
  String get rawValue => this == SettingsTab.features ? 'experimental' : name;

  /// Deep-link compatibility: maps legacy/renamed tab spellings to current tabs.
  ///
  /// Port of `SettingsTab.compatibleRawValue` (SettingsView.swift). The
  /// Agents & Apps split (2026-09-16) renamed several deep-link spellings;
  /// accepting the old ones keeps existing snapshot/dev commands valid.
  /// Note: "presets" is a live case but the switch maps it to .agents
  /// first (Swift `case "presets", "agentsApps", "mcp": return .agents`).
  static SettingsTab? compatibleRawValue(String rawValue) {
    switch (rawValue) {
      // Agents & Apps split into Agents and Plugins (2026-09-16); the
      // one-page MCP group became Agents (connections) + Agent access (policies).
      case 'presets':
      case 'agentsApps':
      case 'mcp':
        return SettingsTab.agents;
      case 'sessions':
      case 'browser':
        return SettingsTab.agentAccess;
      case 'profiles':
        return SettingsTab.workspaces;
      case 'features':
        return SettingsTab.features;
      default:
        for (final tab in SettingsTab.values) {
          if (tab.rawValue == rawValue) return tab;
        }
        return null;
    }
  }

  /// Feature-gated tabs: the tabs visible given the current feature flags.
  ///
  /// Port of `SettingsTab.visibleCases(computerUseControllable:)`
  /// (SettingsView.swift). The legacy `computerUseControllable` argument
  /// cannot restore a retired tab (Swift comment) so it is not modelled.
  /// Sessions/Browser use are Settings ▸ Features toggles; the agent-access
  /// page exists while either is on. The mobile (Remote Control) tab needs
  /// the mobile remote-control flag. Git worktrees is a Features toggle;
  /// its panel only exists while the feature is on. `computer` and
  /// `presets` never show their old panels.
  static List<SettingsTab> visibleCases({
    required bool sessionsMcp,
    required bool browserMcp,
    required bool workspacesEnabled,
    required bool worktreesEnabled,
    required bool mobileRemoteControlEnabled,
  }) {
    return SettingsTab.values.where((tab) {
      switch (tab) {
        case SettingsTab.mobile:
          return mobileRemoteControlEnabled;
        // Sessions use and Browser use are Settings ▸ Features toggles;
        // the access page exists while either is on.
        case SettingsTab.agentAccess:
          return sessionsMcp || browserMcp;
        // Keep the saved enum case readable, but never show its old panel.
        case SettingsTab.computer:
        case SettingsTab.presets:
          return false;
        case SettingsTab.workspaces:
          return workspacesEnabled;
        // Git worktrees is a Features toggle; its panel only exists while
        // the feature is on (same live gate as the sidebar folders).
        case SettingsTab.worktrees:
          return worktreesEnabled;
        default:
          return true;
      }
    }).toList();
  }

  /// The tabs that follow the Settings scope dropdown to the selected
  /// workspace/Host — i.e. whose settings operations exist on the Host
  /// contract. Grows as verbs land; `settings.presets.set` is the first.
  ///
  /// Port of `SettingsTab.hostScopedCases` (SettingsView.swift).
  static List<SettingsTab> get hostScopedCases => [
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
      ];

  /// Resolve the selected tab, falling back to the first visible tab when
  /// the stored tab's gate turned off (Swift: `resolvedSettingsTab`).
  /// Workspaces leads the enum but is itself gated, so resolve through
  /// [visibleCases].
  static SettingsTab resolved(
    SettingsTab selected, {
    required bool sessionsMcp,
    required bool browserMcp,
    required bool workspacesEnabled,
    required bool worktreesEnabled,
    required bool mobileRemoteControlEnabled,
  }) {
    final visible = visibleCases(
      sessionsMcp: sessionsMcp,
      browserMcp: browserMcp,
      workspacesEnabled: workspacesEnabled,
      worktreesEnabled: worktreesEnabled,
      mobileRemoteControlEnabled: mobileRemoteControlEnabled,
    );
    if (visible.contains(selected)) return selected;
    return visible.isNotEmpty ? visible.first : SettingsTab.presets;
  }

  /// Display title for the tab (Swift: `SettingsTab.title`).
  ///
  /// Note: `presets` renders as "Agents" in Swift (the presets tab hosts
  /// the agents preset editor).
  String get title {
    switch (this) {
      case SettingsTab.appearance:
        return 'Appearance';
      case SettingsTab.agents:
        return 'Agents';
      case SettingsTab.plugins:
        return 'Plugins';
      case SettingsTab.agentAccess:
        return 'Agent access';
      case SettingsTab.presets:
        return 'Agents';
      case SettingsTab.mobile:
        return 'Remote Control';
      case SettingsTab.workspaces:
        return 'Workspaces';
      case SettingsTab.transcripts:
        return 'Transcripts';
      case SettingsTab.notifications:
        return 'Notifications';
      case SettingsTab.computer:
        return 'Computer use';
      case SettingsTab.worktrees:
        return 'Worktrees';
      case SettingsTab.features:
        return 'Features';
      case SettingsTab.advanced:
        return 'Advanced';
    }
  }

  /// Icon name per tab, for the settings sidebar nav.
  ///
  /// Port of `SettingsTab.icon` (SettingsView.swift). Swift returns a
  /// ChromeIcon (glass-gradient nav treatment); the Dart port uses icon
  /// asset names — the treatment is applied by the renderer.
  String get iconName {
    switch (this) {
      case SettingsTab.appearance:
        return 'settings-appearance';
      case SettingsTab.agents:
        return 'settings-agents';
      case SettingsTab.plugins:
        return 'settings-plugins';
      case SettingsTab.agentAccess:
        return 'settings-agent-access';
      case SettingsTab.presets:
        return 'settings-presets';
      case SettingsTab.mobile:
        return 'settings-remote';
      case SettingsTab.workspaces:
        return 'settings-workspaces';
      case SettingsTab.transcripts:
        return 'settings-transcripts';
      case SettingsTab.notifications:
        return 'settings-notifications';
      case SettingsTab.computer:
        return 'settings-computer';
      case SettingsTab.worktrees:
        return 'settings-worktrees';
      case SettingsTab.features:
        return 'settings-features';
      case SettingsTab.advanced:
        return 'settings-advanced';
    }
  }
}
