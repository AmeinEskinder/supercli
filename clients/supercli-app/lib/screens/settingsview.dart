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

import 'appearancesettingspanel.dart';
import 'hostpickerview.dart';
import 'licensesettings.dart';
import 'pluginsettingspanel.dart';
import 'presetssettingspanel.dart';
import 'remotesettingspanel.dart';
import 'sessionsaccesssections.dart';
import 'settingspanels.dart';
import 'workspacessettingspanel.dart';
import 'worktreessettingspanel.dart';

/// Settings tab identifiers.
///
/// Port of `SettingsTab` (SettingsView.swift). The Swift enum drives the
/// settings sidebar nav; deep-link spellings, feature gates, and icons are
/// ported below as [compatibleRawValue], [visibleCases], and [iconName].
enum SettingsTab {
  general,
  sessions,
  agentAccess,
  browser,
  workspaces,
  worktrees,
  agents,
  plugins,
  presets,
  notifications,
  transcripts,
  features,
  advanced,
  license,
  remote,
  developer;

  /// Deep-link compatibility: maps legacy/renamed tab spellings to current tabs.
  ///
  /// Port of `SettingsTab.compatibleRawValue` (SettingsView.swift). The
  /// Agents & Apps split (2026-09-16) renamed several deep-link spellings;
  /// accepting the old ones keeps existing snapshot/dev commands valid.
  /// The standalone license tab was merged into Remote (2026-08-13).
  static SettingsTab? compatibleRawValue(String rawValue) {
    switch (rawValue) {
      // Agents & Apps split into Agents and Plugins (2026-09-16); the
      // one-page MCP group became Agents (connections) + Agent access (policies).
      case 'agentsApps':
      case 'mcp':
        return SettingsTab.agents;
      case 'sessions':
      case 'browser':
        return SettingsTab.agentAccess;
      case 'profiles':
        return SettingsTab.workspaces;
      case 'features':
      case 'experimental':
        return SettingsTab.features;
      // The standalone "Supercli Link" license tab was merged into Remote.
      case 'license':
        return SettingsTab.remote;
      default:
        for (final tab in SettingsTab.values) {
          if (tab.name == rawValue) return tab;
        }
        return null;
    }
  }

  /// Feature-gated tabs: the tabs visible given the current feature flags.
  ///
  /// Port of `SettingsTab.visibleCases(computerUseControllable:)`
  /// (SettingsView.swift). Sessions/Browser use are Settings ▸ Features
  /// toggles; the agent-access page exists while either is on. Git worktrees
  /// is a Features toggle; its panel only exists while the feature is on.
  static List<SettingsTab> visibleCases({
    required bool sessionsMcp,
    required bool browserMcp,
    required bool workspacesEnabled,
    required bool worktreesEnabled,
    required bool mobileRemoteControlEnabled,
  }) {
    return SettingsTab.values.where((tab) {
      switch (tab) {
        case SettingsTab.remote:
          return mobileRemoteControlEnabled;
        // Sessions use and Browser use are Settings ▸ Features toggles;
        // the access page exists while either is on.
        case SettingsTab.agentAccess:
          return sessionsMcp || browserMcp;
        case SettingsTab.workspaces:
          return workspacesEnabled;
        // Git worktrees is a Features toggle; its panel only exists while
        // the feature is on (same live gate as the sidebar folders).
        case SettingsTab.worktrees:
          return worktreesEnabled;
        // Deprecated/merged tabs: never show their old panels.
        // (sessions/browser merged into agentAccess; license into remote.)
        case SettingsTab.sessions:
        case SettingsTab.browser:
        case SettingsTab.license:
          return false;
        default:
          return true;
      }
    }).toList();
  }

  /// The tabs whose settings operations exist on the Host contract — i.e.
  /// that follow the Settings scope dropdown to the selected workspace/Host.
  ///
  /// Port of `SettingsTab.hostScopedCases` (SettingsView.swift).
  static List<SettingsTab> get hostScopedCases => [
        SettingsTab.agents,
        SettingsTab.plugins,
        SettingsTab.agentAccess,
        SettingsTab.presets,
        SettingsTab.general,
        SettingsTab.transcripts,
        SettingsTab.notifications,
        SettingsTab.features,
        SettingsTab.advanced,
      ];

  /// Display title for the tab.
  String get title => SettingsView.tabTitles[this] ?? name;

  /// Icon name per tab, for the settings sidebar nav.
  ///
  /// Port of `SettingsTab.icon` (SettingsView.swift). Swift returns a
  /// ChromeIcon; the Dart port uses icon asset names (the glass-gradient
  /// nav treatment is applied by the renderer).
  String get iconName {
    switch (this) {
      case SettingsTab.general:
        return 'settings-appearance';
      case SettingsTab.sessions:
        return 'settings-sessions';
      case SettingsTab.agentAccess:
        return 'settings-agent-access';
      case SettingsTab.browser:
        return 'settings-browser';
      case SettingsTab.workspaces:
        return 'settings-workspaces';
      case SettingsTab.worktrees:
        return 'settings-worktrees';
      case SettingsTab.agents:
        return 'settings-agents';
      case SettingsTab.plugins:
        return 'settings-plugins';
      case SettingsTab.presets:
        return 'settings-presets';
      case SettingsTab.notifications:
        return 'settings-notifications';
      case SettingsTab.transcripts:
        return 'settings-transcripts';
      case SettingsTab.features:
        return 'settings-features';
      case SettingsTab.advanced:
        return 'settings-advanced';
      case SettingsTab.license:
        return 'settings-license';
      case SettingsTab.remote:
        return 'settings-remote';
      case SettingsTab.developer:
        return 'settings-developer';
    }
  }
}

/// The settings panel with tab navigation and scope picker.
///
/// Covers checklist items 200 (scope picker), 201 (workspaces), 202
/// (agents — see AgentAccessSettingsPanel), 203 (plugins), 204/205 (agent
/// access), 206 (appearance, in General tab), 207 (remote control), 209
/// (license), 210 (transcripts), 211 (notifications), 212 (worktrees),
/// 213 (features), 214 (advanced), 216 (presets).
///
/// ## Host persistence wiring
///
/// The view itself is stateless; persistence lives in [SettingsController]
/// (see settings_controller.dart). The app shell wires them:
///
/// ```dart
/// final controller = SettingsController(
///   host: hostClient,
///   onError: (msg) => toastCenter.show(msg), // route to ToastCenter
/// );
/// await controller.load(); // GET /mobile/workspace-settings on startup
/// final view = SettingsView(settings: controller.settings);
/// // After any user edit to controller.settings:
/// controller.edited(); // debounced POST /mobile/workspace-settings
/// ```
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
    SettingsTab.agents: 'Agents',
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
        // Swift: AppearanceSettingsPanel is the Appearance tab; in the Dart
        // tab set it renders as the appearance section of General.
        return UiColumn('general-with-appearance', [
          GeneralSettingsPanel(settings: settings).build(),
          AppearanceSettingsPanel(settings: settings).build(),
        ]);
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
      case SettingsTab.agents:
        // Swift: Agents tab shows PluginSettingsPanel with scope .agents
        // (the agent CLIs: install, connect, launch). For now, reuse the
        // plugins panel; agent-specific filtering comes with the Host
        // runtime catalog integration.
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
        // Swift: RemoteSettingsPanel (Remote Control). The HostPickerView
        // (pairing + nearby hosts) is shown below it.
        return UiColumn('remote-with-picker', [
          const RemoteSettingsPanel().build(),
          (hostPicker ?? HostPickerView()).build(),
        ]);
      case SettingsTab.developer:
        return const DeveloperSettingsPanel().build();
    }
  }
}
