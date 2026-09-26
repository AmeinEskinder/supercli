/// Settings model and panels: General, Appearance, Sessions, Features,
/// Transcripts, Notifications, Advanced.
///
/// The settings model mirrors the Host's `settings.workspace.set` allowlist
/// (see `crates/supercli-cli/src/settings_cli.rs` and the Host's
/// `controller_host.rs` whitelist):
///   experimental_features.sessions_mcp   true | false
///   experimental_features.browser_mcp    true | false
///   browser_default_access               on | ask | off
///   mcp_nonchild_write_access            ask | allow | deny
///   mcp_worktree_access                  true | false
///   mcp_auto_add_browser_screenshots     true | false
///   auto_stop_archive_minutes            0 | 30 | 60 | 120 | 240 | 480 | 1440
///   sidebar_stopped_limit                0 | 3 | 5 | 10 | 15 | 25
///   theme                                system | light | dark
///
/// Desktop-only preferences (appearance, notifications, advanced) live in the
/// local app state and are not sent to the Host.
///
/// Toggle/select/slider widgets do not exist in gpuidart upstream (see
/// docs/gpuidart-gaps-settings.md); toggles render as UiButton with an
/// on/off label and selects as a row of option buttons.
/// Port of the SettingsView.swift tab panels.
library;

import 'package:gpuidart/gpuidart.dart';

/// Which settings scope is being edited.
enum SettingsScope {
  thisMac('This Mac'),
  workspace('Workspace'),
  remoteHost('Remote Host');

  const SettingsScope(this.label);
  final String label;
}

/// Theme mode.
enum ThemeMode {
  system('System'),
  light('Light'),
  dark('Dark');

  const ThemeMode(this.label);
  final String label;

  static ThemeMode fromWire(String s) => ThemeMode.values.firstWhere(
        (m) => m.name == s,
        orElse: () => ThemeMode.system,
      );
}

/// Session write policy (mcp_nonchild_write_access).
enum WritePolicy {
  ask('Ask'),
  allow('Allow'),
  deny('Deny');

  const WritePolicy(this.label);
  final String label;

  static WritePolicy fromWire(String s) => WritePolicy.values.firstWhere(
        (m) => m.name == s,
        orElse: () => WritePolicy.ask,
      );
}

/// Browser default access (browser_default_access).
enum BrowserDefaultAccess {
  on('On'),
  ask('Ask'),
  off('Off');

  const BrowserDefaultAccess(this.label);
  final String label;

  static BrowserDefaultAccess fromWire(String s) =>
      BrowserDefaultAccess.values.firstWhere(
        (m) => m.name == s,
        orElse: () => BrowserDefaultAccess.ask,
      );
}

/// The editable settings model.
///
/// Host-managed keys serialize to the `settings.workspace.set` wire format.
/// Desktop-only keys are kept locally.
final class AppSettings {
  AppSettings({
    this.scope = SettingsScope.thisMac,
    this.theme = ThemeMode.system,
    this.accentColor = 0,
    this.terminalFont = 'SF Mono',
    this.terminalFontSize = 13.0,
    this.lineHeight = 1.2,
    this.writePolicy = WritePolicy.ask,
    this.worktreeAccess = false,
    this.autoGallery = true,
    this.autoStopArchiveMinutes = 60,
    this.sidebarStoppedLimit = 10,
    this.browserDefaultAccess = BrowserDefaultAccess.ask,
    this.browserMcp = false,
    this.sessionsMcp = true,
    this.autoAddBrowserScreenshots = false,
    this.remoteWorkspaces = true,
    this.gitWorktrees = true,
    this.notifyOnCompletion = true,
    this.notifyFlags = true,
    this.transcriptContentEnabled = true,
    this.showAgentWorktrees = false,
    this.sessionsFolder = '',
    this.traceLog = false,
  });

  SettingsScope scope;
  ThemeMode theme;
  int accentColor; // 0-7
  String terminalFont;
  double terminalFontSize;
  double lineHeight;
  WritePolicy writePolicy;
  bool worktreeAccess;
  bool autoGallery;
  int autoStopArchiveMinutes;
  int sidebarStoppedLimit;
  BrowserDefaultAccess browserDefaultAccess;
  bool browserMcp;
  bool sessionsMcp;
  bool autoAddBrowserScreenshots;
  bool remoteWorkspaces;
  bool gitWorktrees;
  bool notifyOnCompletion;
  bool notifyFlags;
  bool transcriptContentEnabled;
  bool showAgentWorktrees;
  String sessionsFolder;
  bool traceLog;

  /// Host wire format for `settings.workspace.set` (allowlisted keys only).
  Map<String, Object> toHostJson() => {
        'experimental_features.sessions_mcp': sessionsMcp,
        'experimental_features.browser_mcp': browserMcp,
        'browser_default_access': browserDefaultAccess.name,
        'mcp_nonchild_write_access': writePolicy.name,
        'mcp_worktree_access': worktreeAccess,
        'mcp_auto_add_browser_screenshots': autoAddBrowserScreenshots,
        'auto_stop_archive_minutes': autoStopArchiveMinutes,
        'sidebar_stopped_limit': sidebarStoppedLimit,
        'theme': theme.name,
      };

  factory AppSettings.fromHostJson(Map<String, dynamic> json) {
    T get<T>(String key, T fallback) {
      final v = json[key];
      return v is T ? v : fallback;
    }

    return AppSettings(
      sessionsMcp: get<bool>('experimental_features.sessions_mcp', true),
      browserMcp: get<bool>('experimental_features.browser_mcp', false),
      browserDefaultAccess: BrowserDefaultAccess.fromWire(
          get<String>('browser_default_access', 'ask')),
      writePolicy:
          WritePolicy.fromWire(get<String>('mcp_nonchild_write_access', 'ask')),
      worktreeAccess: get<bool>('mcp_worktree_access', false),
      autoAddBrowserScreenshots:
          get<bool>('mcp_auto_add_browser_screenshots', false),
      autoStopArchiveMinutes: get<int>('auto_stop_archive_minutes', 60),
      sidebarStoppedLimit: get<int>('sidebar_stopped_limit', 10),
      theme: ThemeMode.fromWire(get<String>('theme', 'system')),
    );
  }

  /// One `settings.workspace.set` call payload for a single key.
  Map<String, Object> setCall(String key, Object value) => {
        'key': key,
        'value': value,
      };
}

/// A toggle row: label + on/off button.
/// GAP: gpuidart has no UiToggle; the button label carries the state.
final class SettingsToggle {
  const SettingsToggle({
    required this.id,
    required this.label,
    required this.value,
  });

  final String id;
  final String label;
  final bool value;

  Map<String, Object> toJson() => {
        'kind': 'settings-toggle',
        'id': id,
        'label': label,
        'value': value,
      };

  /// Render through the RLE fallback: a row with label + state button.
  UiNode fallback() => UiRow('$id-row', [
        UiText('$id-label', label),
        UiButton('$id-toggle', value ? 'On' : 'Off'),
      ]);
}

/// A select row: label + one button per option, the active one marked.
/// GAP: gpuidart has no UiSelect/UiDropdown.
final class SettingsSelect {
  const SettingsSelect({
    required this.id,
    required this.label,
    required this.options,
    required this.selected,
  });

  final String id;
  final String label;
  final List<String> options;
  final String selected;

  Map<String, Object> toJson() => {
        'kind': 'settings-select',
        'id': id,
        'label': label,
        'options': options,
        'selected': selected,
      };

  /// Render through the RLE fallback.
  UiNode fallback() => UiColumn('$id-col', [
        UiText('$id-label', label),
        UiRow('$id-options', [
          for (final o in options)
            UiButton('$id-opt-$o', o == selected ? '● $o' : o),
        ]),
      ]);
}

/// General settings panel: scope picker + appearance.
final class GeneralSettingsPanel {
  GeneralSettingsPanel({required this.settings});

  final AppSettings settings;

  UiNode build() {
    return UiColumn('general-settings', [
      const UiText('general-title', 'General'),
      SettingsSelect(
        id: 'settings-scope',
        label: 'Settings scope',
        options: SettingsScope.values.map((s) => s.label).toList(),
        selected: settings.scope.label,
      ).fallback(),
      if (settings.scope != SettingsScope.thisMac)
        UiRow('scope-inherit-row', [
          const UiText('scope-inherit-label',
              'Inherits from This Mac unless overridden.'),
          const UiButton('scope-reset', 'Reset to inherited'),
        ]),
      const UiText('appearance-title', 'Appearance'),
      SettingsSelect(
        id: 'theme',
        label: 'Theme',
        options: ThemeMode.values.map((m) => m.label).toList(),
        selected: settings.theme.label,
      ).fallback(),
      SettingsSelect(
        id: 'accent',
        label: 'Accent color',
        options: const [
          'Blue',
          'Purple',
          'Pink',
          'Red',
          'Orange',
          'Yellow',
          'Green',
          'Graphite'
        ],
        selected: const [
          'Blue',
          'Purple',
          'Pink',
          'Red',
          'Orange',
          'Yellow',
          'Green',
          'Graphite'
        ][settings.accentColor.clamp(0, 7)],
      ).fallback(),
      SettingsSelect(
        id: 'terminal-font',
        label: 'Terminal font',
        options: const ['SF Mono', 'Menlo', 'JetBrains Mono', 'Fira Code'],
        selected: settings.terminalFont,
      ).fallback(),
      UiRow('terminal-size-row', [
        UiText('terminal-size-label',
            'Terminal font size: ${settings.terminalFontSize.toStringAsFixed(1)}'),
        const UiButton('terminal-size-dec', '−'),
        const UiButton('terminal-size-inc', '+'),
      ]),
    ]);
  }
}

/// Sessions settings panel.
final class SessionsSettingsPanel {
  SessionsSettingsPanel({required this.settings});

  final AppSettings settings;

  UiNode build() {
    return UiColumn('sessions-settings', [
      const UiText('sessions-settings-title', 'Sessions'),
      SettingsSelect(
        id: 'write-policy',
        label: 'Write policy (non-child paths)',
        options: WritePolicy.values.map((w) => w.label).toList(),
        selected: settings.writePolicy.label,
      ).fallback(),
      SettingsToggle(
        id: 'worktree-access',
        label: 'Allow worktree access',
        value: settings.worktreeAccess,
      ).fallback(),
      SettingsToggle(
        id: 'auto-gallery',
        label: 'Auto-add to gallery',
        value: settings.autoGallery,
      ).fallback(),
      SettingsToggle(
        id: 'sessions-mcp',
        label: 'Sessions MCP (experimental)',
        value: settings.sessionsMcp,
      ).fallback(),
      SettingsSelect(
        id: 'auto-stop',
        label: 'Auto-archive stopped sessions after',
        options: const [
          'Never',
          '30 min',
          '1 hour',
          '2 hours',
          '4 hours',
          '8 hours',
          '24 hours'
        ],
        selected: _minutesLabel(settings.autoStopArchiveMinutes),
      ).fallback(),
      SettingsSelect(
        id: 'sidebar-limit',
        label: 'Stopped sessions in sidebar',
        options: const ['None', '3', '5', '10', '15', '25'],
        selected: settings.sidebarStoppedLimit == 0
            ? 'None'
            : settings.sidebarStoppedLimit.toString(),
      ).fallback(),
    ]);
  }

  String _minutesLabel(int m) {
    if (m == 0) return 'Never';
    if (m < 60) return '$m min';
    return '${m ~/ 60} hour${m == 60 ? '' : 's'}';
  }
}

/// Features settings panel: experimental gates.
final class FeaturesSettingsPanel {
  FeaturesSettingsPanel({required this.settings});

  final AppSettings settings;

  UiNode build() {
    return UiColumn('features-settings', [
      const UiText('features-title', 'Features'),
      SettingsToggle(
        id: 'feat-remote-ws',
        label: 'Remote workspaces',
        value: settings.remoteWorkspaces,
      ).fallback(),
      SettingsToggle(
        id: 'feat-git-worktrees',
        label: 'Git worktrees',
        value: settings.gitWorktrees,
      ).fallback(),
      SettingsToggle(
        id: 'feat-browser-mcp',
        label: 'Browser MCP (experimental)',
        value: settings.browserMcp,
      ).fallback(),
      SettingsToggle(
        id: 'feat-auto-screenshots',
        label: 'Auto-add browser screenshots',
        value: settings.autoAddBrowserScreenshots,
      ).fallback(),
    ]);
  }
}

/// Transcripts settings panel.
final class TranscriptsSettingsPanel {
  TranscriptsSettingsPanel({required this.settings});

  final AppSettings settings;

  UiNode build() {
    return UiColumn('transcripts-settings', [
      const UiText('transcripts-title', 'Transcripts'),
      const UiText('transcripts-info',
          'Control what session content is stored in transcripts.'),
      SettingsToggle(
        id: 'transcript-content',
        label: 'Store message content',
        value: settings.transcriptContentEnabled,
      ).fallback(),
    ]);
  }
}

/// Notifications settings panel.
final class NotificationsSettingsPanel {
  NotificationsSettingsPanel({required this.settings});

  final AppSettings settings;

  UiNode build() {
    return UiColumn('notifications-settings', [
      const UiText('notifications-title', 'Notifications'),
      SettingsToggle(
        id: 'notify-completion',
        label: 'Notify on session completion',
        value: settings.notifyOnCompletion,
      ).fallback(),
      SettingsToggle(
        id: 'notify-flags',
        label: 'Flag select menus',
        value: settings.notifyFlags,
      ).fallback(),
      UiRow('notify-test-row', [
        const UiButton('notify-test-mac', 'Test on this Mac'),
        const UiButton('notify-test-phone', 'Test on phone'),
      ]),
      const UiButton('notify-diagnostics', 'Delivery diagnostics'),
    ]);
  }
}

/// Advanced settings panel.
final class AdvancedSettingsPanel {
  AdvancedSettingsPanel({required this.settings});

  final AppSettings settings;

  UiNode build() {
    return UiColumn('advanced-settings', [
      const UiText('advanced-title', 'Advanced'),
      SettingsToggle(
        id: 'adv-show-worktrees',
        label: 'Show agent worktrees',
        value: settings.showAgentWorktrees,
      ).fallback(),
      UiRow('sessions-folder-row', [
        UiText('sessions-folder-label',
            'Sessions folder: ${settings.sessionsFolder.isEmpty ? '(default)' : settings.sessionsFolder}'),
        const UiButton('sessions-folder-choose', 'Choose…'),
      ]),
      SettingsToggle(
        id: 'adv-trace-log',
        label: 'Trace log',
        value: settings.traceLog,
      ).fallback(),
      const UiButton('adv-running-hosts', 'Running hosts…'),
      const UiText('adv-memory', 'Memory usage: see Activity Monitor'),
    ]);
  }
}

/// Developer settings panel.
final class DeveloperSettingsPanel {
  const DeveloperSettingsPanel();

  UiNode build() {
    return UiColumn('developer-settings', [
      const UiText('developer-title', 'Developer'),
      const UiButton('dev-reload', 'Reload UI'),
      const UiButton('dev-inspect', 'Inspect element'),
      const UiButton('dev-logs', 'Open logs'),
    ]);
  }
}

/// Local site menu: per-site navigation menu.
final class LocalSiteMenu {
  const LocalSiteMenu({this.sites = const []});

  final List<String> sites;

  UiNode build() {
    return UiColumn('local-site-menu', [
      const UiText('site-menu-title', 'Local Sites'),
      for (final s in sites) UiButton('site-$s', s),
    ]);
  }
}

/// Plugin list drag helper.
/// GAP: No drag-and-drop in gpuidart (P0-13).
final class PluginListDrag {
  const PluginListDrag();

  UiNode build() {
    return const UiText(
        'plugin-drag', '(plugin drag — needs gpuidart drag-and-drop, P0-13)');
  }
}
