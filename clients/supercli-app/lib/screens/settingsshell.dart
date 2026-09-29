/// Settings shell: sidebar nav, scope picker, content host, title strip.
///
/// Port of the settings chrome in SettingsView.swift:
/// `SettingsSidebarPanel` (175-305), `SettingsScopePickerControl`
/// (1939-2022), `SettingsContentHost` (2023-2244), `SettingsTitleStrip`
/// (2245-2311), `SettingsMainBackground` (2312-2325), and `SettingsNavRow`
/// (4198-4248). Shared row primitives live in settingsprimitives.dart.
///
/// The sidebar lists the feature-gated visible tabs (Workspaces registry
/// first via enum order), the Back row closes Settings, and the stored tab
/// falls back to the first visible tab when its gate turned off. The
/// content host routes each tab to its panel per the selected scope: a
/// local workspace renders the local panel, a sibling local workspace
/// renders the Host panels, and a remote Host renders the remote panels or
/// the update-required panel when the Host predates `settings.workspace.set`.
///
/// Escape closes Settings (wired by the app's key handling; the canonical
/// comma keybinding is not registered — see the sidecar verification note).
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingsview.dart';
import 'settingspanels.dart' as panels;
import 'appearancesettingspanel.dart';
import 'remoteappearancesettingspanel.dart';
import 'hostsettingspanels.dart';
import 'remotehostsettingspanels.dart';
import 'remotesettingspanel.dart';
import 'pluginsettingspanel.dart';
import 'workspacessettingspanel.dart';
import 'worktreessettingspanel.dart';
import 'agentaccesssettingspanel.dart';

/// Sidebar nav row (Swift: `SettingsNavRow`, 4198-4248).
final class SettingsNavRow {
  const SettingsNavRow({required this.tab, required this.isActive});

  final SettingsTab tab;
  final bool isActive;

  UiNode build() {
    // Id matches the `settings-tab-<rawValue>` click prefix handled in
    // app.dart (deep-link spelling, e.g. `settings-tab-experimental`).
    return UiButton(
      'settings-tab-${tab.rawValue}',
      '${isActive ? '● ' : ''}${tab.title}',
    );
  }
}

/// Scope picker control (Swift: `SettingsScopePickerControl`, 1939-2022).
///
/// Button showing the workspace tint dot + name; opens the workspace
/// picker (the app layer wires the action).
final class SettingsScopePickerControl {
  const SettingsScopePickerControl({
    this.scopeName = 'Personal',
    this.scopeTintName = 'Default',
  });

  final String scopeName;
  final String scopeTintName;

  UiNode build() {
    return UiButton('settings-scope-picker', '$scopeTintName $scopeName ›');
  }
}

/// Settings sidebar (Swift: `SettingsSidebarPanel`, 175-305).
///
/// Back row (closes settings), scope picker, the Workspaces registry row
/// first (enum order), then the remaining visible tabs, and the pinned
/// "Feedback & bugs" footer.
final class SettingsSidebarPanel {
  const SettingsSidebarPanel({
    required this.tabs,
    required this.activeTab,
    this.scopeName = 'Personal',
    this.scopeTintName = 'Default',
  });

  final List<SettingsTab> tabs;
  final SettingsTab activeTab;
  final String scopeName;
  final String scopeTintName;

  UiNode build() {
    return UiColumn('settings-sidebar', [
      const UiButton('settings-back', '‹ Back'),
      SettingsScopePickerControl(
        scopeName: scopeName,
        scopeTintName: scopeTintName,
      ).build(),
      UiColumn('settings-nav', [
        for (final tab in tabs)
          SettingsNavRow(tab: tab, isActive: tab == activeTab).build(),
      ]),
      const UiButton('settings-feedback', 'Feedback & bugs'),
    ]);
  }
}

/// Title strip (Swift: `SettingsTitleStrip`, 2245-2311).
///
/// `"Settings — <workspace> › <Tab>"` breadcrumb segments.
final class SettingsTitleStrip {
  const SettingsTitleStrip({required this.workspaceName, required this.tab});

  final String workspaceName;
  final SettingsTab tab;

  /// `"Settings — <workspace> › <Tab>"` (Swift: title strip segments).
  String get text => 'Settings — $workspaceName › ${tab.title}';

  UiNode build() {
    return UiText('settings-title-strip', text);
  }
}

/// Main background (Swift: `SettingsMainBackground`, 2312-2325).
///
/// Surface backdrop behind the settings content. The native glass/material
/// treatment has no gpuidart equivalent; the renderer draws the plain
/// surface.
final class SettingsMainBackground {
  const SettingsMainBackground();

  UiNode build() {
    return const UiText('settings-background', '');
  }
}

/// Which panel the content host renders for a tab/scope combination.
///
/// Mirrors the `SettingsContentHost.panelContent` routing table
/// (SettingsView.swift, 2023-2244).
enum SettingsPanelKind {
  appearance,
  agentsPlugins,
  plugins,
  agentAccess,
  mobile,
  advancedHost,
  transcriptsHost,
  accessHost,
  appearanceHost,
  appearanceRemote,
  notificationsHost,
  notificationsRemote,
  featuresHost,
  featuresRemote,
  worktrees,
  workspaces,
  updateRequired,
  computerEmpty,
  transcripts,
  notifications,
  features,
  advanced,
}

/// The scope the settings content renders for (Swift: `HostScope` selection).
final class SettingsScopeContext {
  const SettingsScopeContext({
    this.isLocal = true,
    this.localWorkspaceHome,
    this.localWorkspaceName,
    this.remoteHostId,
    this.supportsWorkspaceSettingsSet = false,
    this.hasTranscriptSettings = false,
    this.hasAppearanceSettings = false,
    this.hasNotificationSettings = false,
    this.hasExperimentalSettings = false,
  });

  final bool isLocal;
  final String? localWorkspaceHome;
  final String? localWorkspaceName;
  final String? remoteHostId;
  final bool supportsWorkspaceSettingsSet;
  final bool hasTranscriptSettings;
  final bool hasAppearanceSettings;
  final bool hasNotificationSettings;
  final bool hasExperimentalSettings;

  /// A sibling local workspace scope (Swift: `.localWorkspace(home, name)`).
  ///
  /// This is NOT `.local`: Swift's `SettingsContentHost.panelContent` sends
  /// `.localWorkspace` down the remote branch (only `.local` takes
  /// `localPanel`), where appearance/notifications/features resolve to the
  /// Host file-based panels.
  bool get isLocalWorkspace => !isLocal && localWorkspaceHome != null;
}

/// Settings content host (Swift: `SettingsContentHost`, 2023-2244).
///
/// Routes each tab to its panel per the selected scope. Workspaces is
/// machine-level; other tabs follow the scope dropdown to the selected
/// workspace/Host.
final class SettingsContentHost {
  const SettingsContentHost({
    required this.activeTab,
    required this.scope,
    required this.workspaceName,
    required this.panelBuilder,
  });

  final SettingsTab activeTab;
  final SettingsScopeContext scope;
  final String workspaceName;
  final UiNode Function(SettingsPanelKind kind) panelBuilder;

  /// Route a tab to its panel kind for a scope
  /// (Swift: `SettingsContentHost.panelContent`).
  static SettingsPanelKind panelKindFor(
    SettingsTab tab,
    SettingsScopeContext scope,
  ) {
    if (!scope.isLocal && tab != SettingsTab.workspaces) {
      return _remotePanelKind(tab, scope);
    }
    return _localPanelKind(tab);
  }

  static SettingsPanelKind _remotePanelKind(
    SettingsTab tab,
    SettingsScopeContext scope,
  ) {
    switch (tab) {
      case SettingsTab.agents:
      case SettingsTab.presets:
        return SettingsPanelKind.agentsPlugins;
      case SettingsTab.plugins:
        return SettingsPanelKind.plugins;
      case SettingsTab.mobile:
        return SettingsPanelKind.mobile;
      case SettingsTab.worktrees:
        return SettingsPanelKind.worktrees;
      case SettingsTab.advanced:
        if (scope.supportsWorkspaceSettingsSet) {
          return SettingsPanelKind.advancedHost;
        }
        return SettingsPanelKind.updateRequired;
      case SettingsTab.transcripts:
        if (scope.supportsWorkspaceSettingsSet && scope.hasTranscriptSettings) {
          return SettingsPanelKind.transcriptsHost;
        }
        return SettingsPanelKind.updateRequired;
      case SettingsTab.agentAccess:
        if (scope.supportsWorkspaceSettingsSet) {
          return SettingsPanelKind.accessHost;
        }
        return SettingsPanelKind.updateRequired;
      case SettingsTab.appearance:
        if (scope.isLocalWorkspace) {
          return SettingsPanelKind.appearanceHost;
        }
        if (scope.supportsWorkspaceSettingsSet && scope.hasAppearanceSettings) {
          return SettingsPanelKind.appearanceRemote;
        }
        return SettingsPanelKind.updateRequired;
      case SettingsTab.notifications:
        if (scope.isLocalWorkspace) {
          return SettingsPanelKind.notificationsHost;
        }
        if (scope.supportsWorkspaceSettingsSet &&
            scope.hasNotificationSettings) {
          return SettingsPanelKind.notificationsRemote;
        }
        return SettingsPanelKind.updateRequired;
      case SettingsTab.features:
        if (scope.isLocalWorkspace) {
          return SettingsPanelKind.featuresHost;
        }
        if (scope.supportsWorkspaceSettingsSet &&
            scope.hasExperimentalSettings) {
          return SettingsPanelKind.featuresRemote;
        }
        return SettingsPanelKind.updateRequired;
      case SettingsTab.computer:
      case SettingsTab.workspaces:
        return SettingsPanelKind.updateRequired;
    }
  }

  static SettingsPanelKind _localPanelKind(SettingsTab tab) {
    switch (tab) {
      case SettingsTab.appearance:
        return SettingsPanelKind.appearance;
      case SettingsTab.agents:
      case SettingsTab.presets:
        return SettingsPanelKind.agentsPlugins;
      case SettingsTab.plugins:
        return SettingsPanelKind.plugins;
      case SettingsTab.transcripts:
        return SettingsPanelKind.transcripts;
      case SettingsTab.notifications:
        return SettingsPanelKind.notifications;
      case SettingsTab.agentAccess:
        return SettingsPanelKind.agentAccess;
      case SettingsTab.computer:
        // The Computer use panel was removed when the feature retired;
        // the case is kept so saved selections still resolve.
        return SettingsPanelKind.computerEmpty;
      case SettingsTab.features:
        return SettingsPanelKind.features;
      case SettingsTab.mobile:
        return SettingsPanelKind.mobile;
      case SettingsTab.workspaces:
        return SettingsPanelKind.workspaces;
      case SettingsTab.worktrees:
        return SettingsPanelKind.worktrees;
      case SettingsTab.advanced:
        return SettingsPanelKind.advanced;
    }
  }

  UiNode build() {
    final kind = panelKindFor(activeTab, scope);
    return UiColumn('settings-content', [
      SettingsTitleStrip(workspaceName: workspaceName, tab: activeTab).build(),
      panelBuilder(kind),
    ]);
  }
}

/// Settings root view (Swift: `SettingsView`).
///
/// Sidebar + content host. Escape closes Settings (wired by the app's key
/// handling). The default panel builder constructs each panel from
/// [settings] with default data; the app layer injects live Host data
/// (via `supercli-client-ffi`) by overriding the builder.
final class SettingsView {
  SettingsView({
    required this.settings,
    this.activeTab = SettingsTab.workspaces,
    this.scope = const SettingsScopeContext(),
    this.workspaceName = 'Personal',
    this.panelBuilder,
  });

  final panels.AppSettings settings;
  final SettingsTab activeTab;
  final SettingsScopeContext scope;
  final String workspaceName;
  final UiNode Function(SettingsPanelKind kind)? panelBuilder;

  List<SettingsTab> get _visibleTabs => SettingsTab.visibleCases(
    sessionsMcp: settings.sessionsMcp,
    browserMcp: settings.browserMcp,
    workspacesEnabled: settings.remoteWorkspaces,
    worktreesEnabled: settings.gitWorktrees,
    mobileRemoteControlEnabled: false,
  );

  SettingsTab get _resolvedTab => SettingsTab.resolved(
    activeTab,
    sessionsMcp: settings.sessionsMcp,
    browserMcp: settings.browserMcp,
    workspacesEnabled: settings.remoteWorkspaces,
    worktreesEnabled: settings.gitWorktrees,
    mobileRemoteControlEnabled: false,
  );

  UiNode _defaultPanel(SettingsPanelKind kind) {
    switch (kind) {
      case SettingsPanelKind.appearance:
        return AppearanceSettingsPanel(settings: settings).build();
      case SettingsPanelKind.agentsPlugins:
      case SettingsPanelKind.plugins:
        return const PluginSettingsPanel().build();
      case SettingsPanelKind.agentAccess:
        return AgentAccessSettingsPanel(settings: settings).build();
      case SettingsPanelKind.transcripts:
        return const panels.TranscriptsSettingsPanel().build();
      case SettingsPanelKind.notifications:
        return const panels.NotificationsSettingsPanel().build();
      case SettingsPanelKind.features:
        return const panels.FeaturesSettingsPanel().build();
      case SettingsPanelKind.advanced:
        return const panels.AdvancedSettingsPanel().build();
      case SettingsPanelKind.mobile:
        return const RemoteSettingsPanel().build();
      case SettingsPanelKind.workspaces:
        return const WorkspacesSettingsPanel().build();
      case SettingsPanelKind.worktrees:
        return WorktreesSettingsPanel(
          showAgentWorktrees: settings.showAgentWorktrees,
        ).build();
      case SettingsPanelKind.appearanceHost:
        return HostAppearanceSettingsPanel(
          home: scope.localWorkspaceHome ?? '',
          name: scope.localWorkspaceName ?? workspaceName,
        ).build();
      case SettingsPanelKind.advancedHost:
        return HostAdvancedSettingsPanel(scopeName: workspaceName).build();
      case SettingsPanelKind.accessHost:
        return HostAccessSettingsPanel(scopeName: workspaceName).build();
      case SettingsPanelKind.transcriptsHost:
        return HostTranscriptsSettingsPanel(scopeName: workspaceName).build();
      case SettingsPanelKind.notificationsHost:
        return HostNotificationsSettingsPanel(name: workspaceName).build();
      case SettingsPanelKind.featuresHost:
        return HostFeaturesSettingsPanel(name: workspaceName).build();
      case SettingsPanelKind.appearanceRemote:
        return RemoteAppearanceSettingsPanel(scopeName: workspaceName).build();
      case SettingsPanelKind.notificationsRemote:
        return RemoteNotificationsSettingsPanel(scopeName: workspaceName)
            .build();
      case SettingsPanelKind.featuresRemote:
        return RemoteFeaturesSettingsPanel(scopeName: workspaceName).build();
      case SettingsPanelKind.updateRequired:
        return HostSettingsUpdateRequiredPanel(
          tabTitle: activeTab.title,
          scopeName: workspaceName,
        ).build();
      case SettingsPanelKind.computerEmpty:
        return const UiText('settings-computer-empty', '');
    }
  }

  UiNode build() {
    final tabs = _visibleTabs;
    final resolved = _resolvedTab;
    return UiRow('settings', [
      SettingsSidebarPanel(
        tabs: tabs,
        activeTab: resolved,
        scopeName: workspaceName,
      ).build(),
      SettingsContentHost(
        activeTab: resolved,
        scope: scope,
        workspaceName: workspaceName,
        panelBuilder: panelBuilder ?? _defaultPanel,
      ).build(),
    ]);
  }
}
