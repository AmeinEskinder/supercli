/// Settings model and panels: General, Appearance, Sessions, Features,
/// Transcripts, Notifications, Advanced.
///
/// The settings model mirrors the Host's `settings.workspace.set` allowlist
/// (see `crates/supercli-core/src/controller_host.rs`
/// `workspace_settings_response`, served at `POST /mobile/workspace-settings`
/// and read back via `GET /mobile/workspace-settings`):
///   experimentalSettings.sessionsMcp   true | false
///   experimentalSettings.browserMcp    true | false
///   browserDefaultAccess               on | ask | off
///   mcpNonchildWriteAccess             ask | allow | deny
///   mcpWorktreeAccess                  true | false
///   mcpAutoAddBrowserScreenshots       true | false
///   autoStopArchiveMinutes             0 | 30 | 60 | 120 | 240 | 480 | 1440
///   sidebarStoppedLimit                0 | 3 | 5 | 10 | 15 | 25
///   appearanceSettings.theme           system | light | dark
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

import 'settingsprimitives.dart';

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

  static BrowserDefaultAccess fromWire(String s) => BrowserDefaultAccess.values
      .firstWhere((m) => m.name == s, orElse: () => BrowserDefaultAccess.ask);
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

  /// Host wire format for `POST /mobile/workspace-settings`
  /// (`settings.workspace.set`): camelCase keys matching the Host's
  /// `workspace_settings_response` whitelist in
  /// `crates/supercli-core/src/controller_host.rs`. The GET route returns
  /// the same shape, so this round-trips.
  Map<String, Object> toHostJson() => {
    'experimentalSettings': {
      'sessionsMcp': sessionsMcp,
      'browserMcp': browserMcp,
    },
    'browserDefaultAccess': browserDefaultAccess.name,
    'mcpNonchildWriteAccess': writePolicy.name,
    'mcpWorktreeAccess': worktreeAccess,
    'mcpAutoAddBrowserScreenshots': autoAddBrowserScreenshots,
    'autoStopArchiveMinutes': autoStopArchiveMinutes,
    'sidebarStoppedLimit': sidebarStoppedLimit,
  };

  /// Local-only fields (theme, appearance, notifications, advanced prefs)
  /// persist through [SettingsLocalStore] — the app's own config — never
  /// through the Host. They are intentionally absent from [toHostJson] and
  /// are not overridden by Host responses.

  /// Parse the `GET /mobile/workspace-settings` response body (same
  /// camelCase wire format as [toHostJson]). Missing keys fall back to
  /// the model defaults.
  factory AppSettings.fromHostJson(Map<String, dynamic> json) {
    T get<T>(String key, T fallback) {
      final v = json[key];
      return v is T ? v : fallback;
    }

    Map<String, dynamic> nested(String key) {
      final v = json[key];
      return v is Map<String, dynamic>
          ? v
          : v is Map
          ? Map<String, dynamic>.from(v)
          : <String, dynamic>{};
    }

    final experimental = nested('experimentalSettings');
    final appearance = nested('appearanceSettings');

    T nget<T>(Map<String, dynamic> m, String key, T fallback) {
      final v = m[key];
      return v is T ? v : fallback;
    }

    return AppSettings(
      sessionsMcp: nget<bool>(experimental, 'sessionsMcp', true),
      browserMcp: nget<bool>(experimental, 'browserMcp', false),
      browserDefaultAccess: BrowserDefaultAccess.fromWire(
        get<String>('browserDefaultAccess', 'ask'),
      ),
      writePolicy: WritePolicy.fromWire(
        get<String>('mcpNonchildWriteAccess', 'ask'),
      ),
      worktreeAccess: get<bool>('mcpWorktreeAccess', false),
      autoAddBrowserScreenshots: get<bool>(
        'mcpAutoAddBrowserScreenshots',
        false,
      ),
      autoStopArchiveMinutes: get<int>('autoStopArchiveMinutes', 60),
      sidebarStoppedLimit: get<int>('sidebarStoppedLimit', 10),
      theme: ThemeMode.fromWire(
        nget<String>(appearance, 'theme', get<String>('theme', 'system')),
      ),
    );
  }

  /// One `settings.workspace.set` call payload for a single key.
  Map<String, Object> setCall(String key, Object value) => {
    'key': key,
    'value': value,
  };

  /// Full local snapshot for [SettingsLocalStore]: every field, including
  /// desktop-only preferences that never go to the Host. Flat keys; enums
  /// serialize by `name` so [applyLocalJson] can round-trip them.
  Map<String, Object> toLocalJson() => {
    'scope': scope.name,
    'theme': theme.name,
    'accentColor': accentColor,
    'terminalFont': terminalFont,
    'terminalFontSize': terminalFontSize,
    'lineHeight': lineHeight,
    'writePolicy': writePolicy.name,
    'worktreeAccess': worktreeAccess,
    'autoGallery': autoGallery,
    'autoStopArchiveMinutes': autoStopArchiveMinutes,
    'sidebarStoppedLimit': sidebarStoppedLimit,
    'browserDefaultAccess': browserDefaultAccess.name,
    'browserMcp': browserMcp,
    'sessionsMcp': sessionsMcp,
    'autoAddBrowserScreenshots': autoAddBrowserScreenshots,
    'remoteWorkspaces': remoteWorkspaces,
    'gitWorktrees': gitWorktrees,
    'notifyOnCompletion': notifyOnCompletion,
    'notifyFlags': notifyFlags,
    'transcriptContentEnabled': transcriptContentEnabled,
    'showAgentWorktrees': showAgentWorktrees,
    'sessionsFolder': sessionsFolder,
    'traceLog': traceLog,
  };

  /// Apply a snapshot previously produced by [toLocalJson]. Unknown or
  /// mistyped values fall back to the field's current value, so a corrupt
  /// or older snapshot can never break the in-memory model.
  void applyLocalJson(Map<String, Object?> json) {
    T get<T>(String key, T current) {
      final v = json[key];
      return v is T ? v : current;
    }

    E enumByName<E extends Enum>(List<E> values, String key, E current) {
      final v = json[key];
      if (v is String) {
        for (final e in values) {
          if (e.name == v) return e;
        }
      }
      return current;
    }

    scope = enumByName(SettingsScope.values, 'scope', scope);
    theme = enumByName(ThemeMode.values, 'theme', theme);
    accentColor = get<int>('accentColor', accentColor);
    terminalFont = get<String>('terminalFont', terminalFont);
    // JSON numbers decode as int when they have no fraction; accept num.
    final fontSize = json['terminalFontSize'];
    if (fontSize is num) terminalFontSize = fontSize.toDouble();
    final lh = json['lineHeight'];
    if (lh is num) lineHeight = lh.toDouble();
    writePolicy = enumByName(WritePolicy.values, 'writePolicy', writePolicy);
    worktreeAccess = get<bool>('worktreeAccess', worktreeAccess);
    autoGallery = get<bool>('autoGallery', autoGallery);
    autoStopArchiveMinutes = get<int>(
      'autoStopArchiveMinutes',
      autoStopArchiveMinutes,
    );
    sidebarStoppedLimit = get<int>('sidebarStoppedLimit', sidebarStoppedLimit);
    browserDefaultAccess = enumByName(
      BrowserDefaultAccess.values,
      'browserDefaultAccess',
      browserDefaultAccess,
    );
    browserMcp = get<bool>('browserMcp', browserMcp);
    sessionsMcp = get<bool>('sessionsMcp', sessionsMcp);
    autoAddBrowserScreenshots = get<bool>(
      'autoAddBrowserScreenshots',
      autoAddBrowserScreenshots,
    );
    remoteWorkspaces = get<bool>('remoteWorkspaces', remoteWorkspaces);
    gitWorktrees = get<bool>('gitWorktrees', gitWorktrees);
    notifyOnCompletion = get<bool>('notifyOnCompletion', notifyOnCompletion);
    notifyFlags = get<bool>('notifyFlags', notifyFlags);
    transcriptContentEnabled = get<bool>(
      'transcriptContentEnabled',
      transcriptContentEnabled,
    );
    showAgentWorktrees = get<bool>('showAgentWorktrees', showAgentWorktrees);
    sessionsFolder = get<String>('sessionsFolder', sessionsFolder);
    traceLog = get<bool>('traceLog', traceLog);
  }
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
          const UiText(
            'scope-inherit-label',
            'Inherits from This Mac unless overridden.',
          ),
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
          'Graphite',
        ],
        selected: const [
          'Blue',
          'Purple',
          'Pink',
          'Red',
          'Orange',
          'Yellow',
          'Green',
          'Graphite',
        ][settings.accentColor.clamp(0, 7)],
      ).fallback(),
      SettingsSelect(
        id: 'terminal-font',
        label: 'Terminal font',
        options: const ['SF Mono', 'Menlo', 'JetBrains Mono', 'Fira Code'],
        selected: settings.terminalFont,
      ).fallback(),
      UiRow('terminal-size-row', [
        UiText(
          'terminal-size-label',
          'Terminal font size: ${settings.terminalFontSize.toStringAsFixed(1)}',
        ),
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
          '24 hours',
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

/// Local Notifications settings panel.
///
/// Port of `NotificationsSettingsPanel` (SettingsView.swift, 3909-4059).
///
/// How Supercli flags a session that needs the user and when it sends a
/// notification banner: the menu-attention detection toggle, the Mac/phone
/// test buttons, and the Link/push diagnostics rows.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
final class NotificationsSettingsPanel {
  const NotificationsSettingsPanel({
    this.isDefaultInstance = true,
    this.defaultWorkspaceLabel = 'Personal',
    this.hasOwnMenuAttentionSetting = false,
    this.menuAttentionDetection = true,
    this.macTestInFlight = false,
    this.lastMacTestLabel = '',
    this.macTestNeedsSystemSettings = false,
    this.pairedPhoneTokenCount = 0,
    this.linkStatusLabel = '',
    this.lastPhonePushLabel = '',
    this.lastPushAttemptText,
  });

  final bool isDefaultInstance;
  final String defaultWorkspaceLabel;
  final bool hasOwnMenuAttentionSetting;
  final bool menuAttentionDetection;
  final bool macTestInFlight;
  final String lastMacTestLabel;
  final bool macTestNeedsSystemSettings;
  final int pairedPhoneTokenCount;
  final String linkStatusLabel;
  final String lastPhonePushLabel;
  final String? lastPushAttemptText;

  String get pairedPhoneTokensText => pairedPhoneTokenCount == 0
      ? 'None registered'
      : '$pairedPhoneTokenCount ready';

  UiNode _menuAttentionSection() {
    return UiColumn('notifications-attention', [
      const SettingsSectionHeader(
        title: 'Attention',
        description:
            'When a session is waiting for you to answer an on-screen menu.',
      ).build(),
      SettingsToggle(
        id: 'notifications-menu-attention',
        label: 'Flag menus waiting for a choice',
        value: menuAttentionDetection,
      ).fallback(),
      const UiText(
        'notifications-menu-attention-desc',
        'Show the yellow attention dot when an agent draws a pick-an-option menu. These prompts send no signal on their own, so Supercli reads them off the screen.',
      ),
    ]);
  }

  UiNode _notificationsSection() {
    final rows = <UiNode>[
      const SettingsSectionHeader(
        title: 'Notifications',
        description:
            'A macOS banner (and a push to a paired iPhone) when a '
            'session needs input, or finishes if you turned on “Notify when done” '
            'for it. Phone alerts use Link/APNs even while terminal traffic stays '
            'Direct or SSH. Mac and phone tests exercise their respective delivery '
            'paths; phone diagnostics distinguish a missing APNs token, Link '
            'entitlement failure, and APNs rejection.',
      ).build(),
      UiButton(
        'notifications-mac-test',
        macTestInFlight ? 'Sending…' : 'Send a test Mac notification',
      ),
      SettingsValueRow(label: 'Last Mac test', value: lastMacTestLabel).build(),
    ];
    if (macTestNeedsSystemSettings) {
      rows.add(
        const UiButton(
          'notifications-mac-settings',
          'Open Mac Notification Settings…',
        ),
      );
    }
    rows.addAll([
      const UiButton(
        'notifications-phone-test',
        'Send a test phone notification',
      ),
      SettingsValueRow(
        label: 'Paired phone tokens',
        value: pairedPhoneTokensText,
      ).build(),
      SettingsValueRow(label: 'Supercli Link', value: linkStatusLabel).build(),
      SettingsValueRow(
        label: 'Last phone push',
        value: lastPhonePushLabel,
      ).build(),
      if (lastPushAttemptText != null)
        SettingsValueRow(
          label: 'Last attempt',
          value: lastPushAttemptText!,
        ).build(),
    ]);
    return UiColumn('notifications-tests', rows);
  }

  UiNode build() {
    final sections = <UiNode>[
      const SettingsPaneHeader(
        title: 'Notifications',
        description: 'How Supercli flags a session that needs you and when it sends a notification banner.',
      ).build(),
    ];
    if (!isDefaultInstance) {
      sections.add(
        UiColumn('notifications-inherit', [
          SettingsSectionHeader(
            title: 'Inherits from $defaultWorkspaceLabel',
            description:
                'This workspace uses the default workspace\'s '
                'notification settings until a setting below is changed. '
                'Revert drops its own values.',
          ).build(),
          UiButton(
            'notifications-use-inherited',
            'Use $defaultWorkspaceLabel\'s notifications',
          ),
        ]),
      );
    }
    sections.add(_menuAttentionSection());
    sections.add(_notificationsSection());
    return UiColumn('notifications-settings', sections);
  }
}

/// Range options for the transcript range picker
/// (Swift: the `maxEntries` Picker in `transcriptSection`).
const List<int> transcriptRangeOptions = [0, 20, 50, 100];

/// Label for a transcript range option (Swift: picker tags).
String transcriptRangeLabel(int entries) {
  switch (entries) {
    case 0:
      return 'Whole conversation';
    case 20:
      return 'Last 20 entries';
    case 50:
      return 'Last 50 entries';
    case 100:
      return 'Last 100 entries';
    default:
      return 'Last $entries entries';
  }
}

/// Local Transcripts settings panel.
///
/// Port of `TranscriptsSettingsPanel` (SettingsView.swift, 4060-4197).
///
/// Which content types the Markdown transcript includes and how much of
/// it. Shared by the session context menu's "Copy transcript" action
/// (desktop and phone) and the Sessions MCP `read_transcript` tool (as its
/// defaults), so all of them stay in sync.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
final class TranscriptsSettingsPanel {
  const TranscriptsSettingsPanel({
    this.includeSessionInfo = true,
    this.includeUser = true,
    this.includeAssistant = true,
    this.includeReasoning = true,
    this.includeTools = true,
    this.includeFileChanges = true,
    this.includePlanUpdates = true,
    this.maxEntries = 50,
  });

  final bool includeSessionInfo;
  final bool includeUser;
  final bool includeAssistant;
  final bool includeReasoning;
  final bool includeTools;
  final bool includeFileChanges;
  final bool includePlanUpdates;
  final int maxEntries;

  UiNode _toggle({
    required String id,
    required String title,
    String subtitle = '',
    required bool value,
  }) {
    return UiColumn('$id-labeled', [
      SettingsToggle(id: id, label: title, value: value).fallback(),
      if (subtitle.isNotEmpty) UiText('$id-subtitle', subtitle),
    ]);
  }

  UiNode build() {
    return UiColumn('transcripts-settings', [
      const SettingsPaneHeader(
        title: 'Transcripts',
        description:
            'A session\'s conversation, rendered as Markdown — '
            'what "Copy transcript" copies and what agents read.',
      ).build(),
      UiColumn('transcripts-content', [
        const SettingsSectionHeader(
          title: 'Transcript content',
          description:
              'What "Copy transcript" (right-click a session) puts on '
              'the clipboard as Markdown. These options also drive the defaults '
              'for agents reading a session\'s transcript. Range is the default '
              'for agent reads; the Copy transcript menu picks its own range.',
        ).build(),
        _toggle(
          id: 'transcripts-session-info',
          title: 'Session info header',
          subtitle:
              'Start with the session\'s title, ID, CLI, and model. '
              'The ID lets another agent target this session with the '
              'Sessions MCP tools.',
          value: includeSessionInfo,
        ),
        _toggle(
          id: 'transcripts-user',
          title: 'User messages',
          value: includeUser,
        ),
        _toggle(
          id: 'transcripts-assistant',
          title: 'Assistant messages',
          value: includeAssistant,
        ),
        _toggle(
          id: 'transcripts-reasoning',
          title: 'Reasoning',
          subtitle: 'The agent\'s thinking blocks.',
          value: includeReasoning,
        ),
        _toggle(
          id: 'transcripts-tools',
          title: 'Tool calls & results',
          subtitle: 'Commands the agent ran and their output.',
          value: includeTools,
        ),
        _toggle(
          id: 'transcripts-file-changes',
          title: 'File changes & diffs',
          value: includeFileChanges,
        ),
        _toggle(
          id: 'transcripts-plan-updates',
          title: 'Plan updates',
          value: includePlanUpdates,
        ),
        UiColumn('transcripts-range-labeled', [
          SettingsSelect(
            id: 'transcripts-range',
            label: 'Range',
            selected: transcriptRangeLabel(maxEntries),
            options: [
              for (final o in transcriptRangeOptions) transcriptRangeLabel(o),
            ],
          ).fallback(),
          const UiText(
            'transcripts-range-subtitle',
            'How much of the conversation to include.',
          ),
        ]),
      ]),
    ]);
  }
}

/// Header copy for the Features tab's Experimental section, shared by the
/// local, per-workspace, and remote Host panels
/// (Swift: `SupercliFeatureFlags.experimentalSectionDescription`).
const String experimentalSectionDescription =
    'Early features that are still being shaped. They can change or '
    'disappear between releases. Turn one off here if it gets in the way.';

/// A feature toggle descriptor (Swift: `AppFeature`).
final class FeatureDefinition {
  const FeatureDefinition({
    required this.key,
    required this.title,
    required this.summary,
    this.defaultOn = false,
    this.isExperimental = false,
  });

  final String key;
  final String title;
  final String summary;
  final bool defaultOn;
  final bool isExperimental;
}

/// Everything shown in Settings ▸ Features, in display order (shipped
/// features first; the panel groups the experimental ones under their own
/// section). Remote workspaces, Git worktrees, Sessions use, and Workspaces
/// graduated on 2026-09-08; Browser use stays experimental.
/// (Swift: `SupercliFeatureFlags.all`.)
const List<FeatureDefinition> allFeatures = [
  FeatureDefinition(
    key: 'remoteWorkspaces',
    title: 'Remote workspaces',
    summary:
        'Add and control workspaces on other machines — pair another Mac, a '
        'headless `supercli serve` box, or an SSH host — and share this Mac with '
        'other devices. Direct connections are for your own network or VPN; '
        'Supercli Link carries the encrypted path when you are away.',
    defaultOn: true,
  ),
  FeatureDefinition(
    key: 'worktrees',
    title: 'Git worktrees',
    summary:
        'Run sessions in an isolated git worktree of a project so multiple '
        'agents can work the same repo in parallel without touching each other\'s '
        'files. Adds worktree controls to the project menu, sidebar, and the '
        'Worktrees settings tab.',
    defaultOn: true,
  ),
  FeatureDefinition(
    key: 'sessionsMcp',
    title: 'Sessions use',
    summary:
        'Let an agent session see your other sessions: it can read them all, '
        'and asks before writing to another session unless you already approved '
        'that pair. These are cooperation controls, not a sandbox against commands '
        'running as your macOS user. Adds the Sessions settings tab. Applies when '
        'a session starts, so already-running sessions pick it up after a restart.',
    defaultOn: true,
  ),
  FeatureDefinition(
    // The persisted key is deliberately still `profiles`: shipped
    // experimental-feature keys are immutable.
    key: 'profiles',
    title: 'Workspaces',
    summary:
        'Use extra, fully separate workspaces on this Mac — each '
        'workspace has its own sessions, projects, presets, settings, and '
        'pairs with your phone as its own workspace. Adds the Workspaces '
        'settings tab.',
    defaultOn: true,
  ),
  FeatureDefinition(
    key: 'browserMcp',
    title: 'Browser use',
    summary:
        'Let agent sessions drive a real browser — open pages, click, '
        'fill forms, and take screenshots. Each session gets its own isolated '
        'browser with no access to your normal browser profile. Browser access '
        'prompts are cooperation controls, not a sandbox against commands '
        'running as your macOS user. Adds the Browser settings tab.',
    defaultOn: true,
    isExperimental: true,
  ),
];

/// Local Features settings panel.
///
/// Port of `FeaturesSettingsPanel` (SettingsView.swift, 4330-4429).
///
/// Turn Supercli's optional features on or off — no restart needed. A
/// workspace instance inherits the default workspace's feature flags until
/// it sets its own; the revert is offered inline. Shipped features render
/// first; experimental ones group under their own section.
///
/// Feature definitions come from `AppFeature` (FeatureFlags.swift):
/// everything shown in Settings ▸ Features, in display order. Shipped
/// feature keys are immutable — the workspaces feature's persisted key is
/// deliberately still `profiles`.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
final class FeaturesSettingsPanel {
  const FeaturesSettingsPanel({
    this.isDefaultInstance = true,
    this.defaultWorkspaceLabel = 'Personal',
    this.hasOwnSettings = false,
    this.values = const {},
  });

  final bool isDefaultInstance;
  final String defaultWorkspaceLabel;
  final bool hasOwnSettings;
  final Map<String, bool> values;

  List<FeatureDefinition> get shipped =>
      allFeatures.where((f) => !f.isExperimental).toList();

  List<FeatureDefinition> get experimental =>
      allFeatures.where((f) => f.isExperimental).toList();

  UiNode _featureRow(FeatureDefinition feature) {
    return UiColumn('feature-${feature.key}-labeled', [
      SettingsToggle(
        id: 'feature-${feature.key}',
        label: feature.title,
        value: values[feature.key] ?? feature.defaultOn,
      ).fallback(),
      UiText('feature-${feature.key}-summary', feature.summary),
    ]);
  }

  UiNode build() {
    final sections = <UiNode>[
      const SettingsPaneHeader(
        title: 'Features',
        description:
            'Turn Supercli\'s optional features on or off — no restart needed.',
      ).build(),
    ];
    if (!isDefaultInstance) {
      sections.add(
        UiColumn('features-inherit', [
          SettingsSectionHeader(
            title: 'Inherits from $defaultWorkspaceLabel',
            description:
                'This workspace uses the default workspace\'s features '
                'until a toggle below is changed. Revert drops its own values.',
          ).build(),
          UiButton(
            'features-use-inherited',
            'Use $defaultWorkspaceLabel\'s features',
          ),
        ]),
      );
    }
    if (allFeatures.isEmpty) {
      sections.add(
        const UiText(
          'features-empty',
          'No optional features right now. Check back after an update.',
        ),
      );
    } else {
      sections.add(
        UiColumn('features-shipped', [for (final f in shipped) _featureRow(f)]),
      );
      if (experimental.isNotEmpty) {
        sections.add(
          UiColumn('features-experimental', [
            const SettingsSectionHeader(
              title: 'Experimental',
              description: experimentalSectionDescription,
            ).build(),
            for (final f in experimental) _featureRow(f),
          ]),
        );
      }
    }
    return UiColumn('features-settings', sections);
  }
}

/// Auto-stop/archive minute options for the local cleanup picker
/// (Swift: `SupercliStore.autoStopArchiveMinuteOptions`).
const List<int> autoStopArchiveMinuteOptions = [0, 30, 60, 120, 240, 480, 1440];

/// Stopped-sidebar limit options for the local cleanup picker
/// (Swift: `SupercliStore.sidebarStoppedLimitOptions`).
const List<int> sidebarStoppedLimitOptions = [0, 3, 5, 10, 15, 25];

/// Label for an auto-stop/archive minute option
/// (Swift: `SupercliStore.autoStopArchiveLabel(for:)`).
String autoStopArchiveLabel(int minutes) {
  if (minutes == 0) return 'Never';
  if (minutes < 60) return 'After $minutes minutes';
  if (minutes == 60) return 'After 1 hour';
  if (minutes == 1440) return 'After 1 day';
  return 'After ${minutes ~/ 60} hours';
}

/// Label for a stopped-sidebar limit option
/// (Swift: `SupercliStore.sidebarStoppedLimitLabel(for:)`).
String sidebarStoppedLimitLabel(int limit) => limit == 0 ? 'None' : '$limit';

/// "42 MB" (Swift: `AdvancedSettingsPanel.formatMB`).
String formatMB(int bytes) => '${(bytes / (1024 * 1024)).round()} MB';

/// "4.2%" / "42%" (Swift: `AdvancedSettingsPanel.formatCpu`).
String formatCpu(double value) => value >= 10
    ? '${value.toStringAsFixed(0)}%'
    : '${value.toStringAsFixed(1)}%';

/// ".../last/two" for long paths; "No folder" for empty
/// (Swift: `AdvancedSettingsPanel.compactPath`).
String compactPath(String path) {
  if (path.isEmpty) return 'No folder';
  final parts = path.split('/').where((p) => p.isNotEmpty).toList();
  if (parts.length <= 2) return path;
  return '.../${parts.sublist(parts.length - 2).join('/')}';
}

/// Process memory snapshot (Swift: `MemorySnapshot`).
final class MemorySnapshot {
  const MemorySnapshot({
    required this.processFootprintBytes,
    required this.runningHostCount,
    required this.hostedSessionCount,
  });

  final int processFootprintBytes;
  final int runningHostCount;
  final int hostedSessionCount;
}

/// A running terminal host row (Swift: `RunningTerminal`).
final class RunningTerminal {
  const RunningTerminal({
    required this.id,
    required this.projectID,
    required this.label,
    required this.command,
    required this.cwd,
    required this.pid,
    required this.processCount,
    required this.cpuPercent,
    required this.rssBytes,
    this.canArchive = false,
    this.isRemoving = false,
  });

  final String id;
  final String projectID;
  final String label;
  final String command;
  final String cwd;
  final int pid;
  final int processCount;
  final double cpuPercent;
  final int rssBytes;
  final bool canArchive;
  final bool isRemoving;

  /// "Blank shell" when the command is empty (Swift: `commandLabel`).
  String get commandLabel => command.trim().isEmpty ? 'Blank shell' : command;
}

/// Diagnostics data (Swift: `AdvancedDiagnostics.Snapshot`).
///
/// Collection (`AdvancedDiagnostics.collect()`) is Host/FFI work — the app
/// injects a snapshot; this file only renders it.
final class AdvancedDiagnosticsSnapshot {
  const AdvancedDiagnosticsSnapshot({
    this.memory,
    this.terminals = const [],
    this.sessionsFolder = '',
    this.traceLogExists = false,
  });

  final MemorySnapshot? memory;
  final List<RunningTerminal> terminals;
  final String sessionsFolder;
  final bool traceLogExists;
}

/// Local Advanced settings panel.
///
/// Port of `AdvancedSettingsPanel` (SettingsView.swift, 4430-4740) with the
/// pure display helpers `formatMB`/`formatCpu`/`compactPath` and the data
/// shapes `MemorySnapshot` (4741-4746) and `RunningTerminal` (4747-4762).
///
/// `AdvancedDiagnostics.collect()` shells out and reads processes and the
/// filesystem in Swift. The Dart port does NOT perform that collection:
/// the panel accepts an injected [AdvancedDiagnosticsSnapshot] (populated
/// by the app through `supercli-client-ffi`/the Host) and renders it. Only
/// the display/pure formatting logic is ported here.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
final class AdvancedSettingsPanel {
  const AdvancedSettingsPanel({
    this.autoStopArchiveMinutes = 60,
    this.sidebarStoppedLimit = 10,
    this.snapshot = const AdvancedDiagnosticsSnapshot(),
    this.loading = false,
  });

  final int autoStopArchiveMinutes;
  final int sidebarStoppedLimit;
  final AdvancedDiagnosticsSnapshot snapshot;
  final bool loading;

  double get _totalCpu =>
      snapshot.terminals.fold(0.0, (sum, t) => sum + t.cpuPercent);

  int get _totalRss => snapshot.terminals.fold(0, (sum, t) => sum + t.rssBytes);

  /// "N running · X% CPU · Y MB memory. Sorted by current CPU usage."
  /// (Swift: `AdvancedSettingsPanel.summaryText`).
  String get summaryText {
    if (snapshot.terminals.isEmpty) {
      return 'Live terminal hosts sorted by current CPU usage.';
    }
    return '${snapshot.terminals.length} running · ${formatCpu(_totalCpu)} CPU · '
        '${formatMB(_totalRss)} memory. Sorted by current CPU usage.';
  }

  UiNode _cleanupSection() {
    return UiColumn('advanced-cleanup', [
      const SettingsSectionHeader(
        title: 'Cleanup',
        description:
            'Sessions that have stayed idle for the selected time are '
            'stopped and archived — the same as clicking "Stop and archive": the '
            'terminal stops and the session files away into the project\'s archive '
            'library, where Restore & Resume continues the conversation. Sessions '
            'that keep working (including loops — any activity resets the clock), '
            'or that are pinned, selected, unread, or waiting for input, are left '
            'alone; plain shell terminals are never touched. Nothing is deleted '
            'automatically. Sessions that stop or die on their own are never '
            'archived automatically. Choose how many stopped or archived sessions '
            'each project previews; older rows are hidden from the sidebar only.',
      ).build(),
      UiColumn('advanced-cleanup-auto-stop-labeled', [
        SettingsSelect(
          id: 'advanced-auto-stop',
          label: 'Auto-stop and archive inactive terminals',
          selected: autoStopArchiveLabel(autoStopArchiveMinutes),
          options: [
            for (final m in autoStopArchiveMinuteOptions)
              autoStopArchiveLabel(m),
          ],
        ).fallback(),
      ]),
      UiColumn('advanced-cleanup-limit-labeled', [
        SettingsSelect(
          id: 'advanced-sidebar-limit',
          label: 'Stopped or archived sessions shown in sidebar',
          selected: sidebarStoppedLimitLabel(sidebarStoppedLimit),
          options: [
            for (final l in sidebarStoppedLimitOptions)
              sidebarStoppedLimitLabel(l),
          ],
        ).fallback(),
      ]),
    ]);
  }

  UiNode _memorySection() {
    final memory = snapshot.memory;
    return UiColumn('advanced-memory', [
      const SettingsSectionHeader(
        title: 'Memory',
        description: 'Current process memory usage and active session counts.',
      ).build(),
      if (memory != null) ...[
        SettingsValueRow(
          label: 'App memory (Supercli Native)',
          value: formatMB(memory.processFootprintBytes),
        ).build(),
        SettingsValueRow(
          label: 'Running terminal hosts',
          value: '${memory.runningHostCount}',
        ).build(),
        SettingsValueRow(
          label: 'Hosted sessions on disk',
          value: '${memory.hostedSessionCount}',
        ).build(),
      ] else
        UiText(
          'advanced-memory-state',
          loading ? 'Loading…' : 'Unable to read memory usage',
        ),
    ]);
  }

  UiNode _terminalRow(RunningTerminal terminal) {
    return UiRow('advanced-terminal-${terminal.id}', [
      UiColumn('advanced-terminal-info-${terminal.id}', [
        UiText('advanced-terminal-label-${terminal.id}', terminal.label),
        UiText(
          'advanced-terminal-sub-${terminal.id}',
          '${terminal.commandLabel} • PID ${terminal.pid} • '
              '${terminal.processCount} proc • ${compactPath(terminal.cwd)}',
        ),
      ]),
      UiColumn('advanced-terminal-cpu-${terminal.id}', [
        UiText(
          'advanced-terminal-cpu-value-${terminal.id}',
          formatCpu(terminal.cpuPercent),
        ),
        const UiText('advanced-terminal-cpu-label', 'CPU'),
      ]),
      UiColumn('advanced-terminal-mem-${terminal.id}', [
        UiText(
          'advanced-terminal-mem-value-${terminal.id}',
          formatMB(terminal.rssBytes),
        ),
        const UiText('advanced-terminal-mem-label', 'Memory'),
      ]),
      UiButton('advanced-terminal-open-${terminal.id}', 'Open'),
      UiButton(
        'advanced-terminal-archive-${terminal.id}',
        terminal.canArchive ? 'Stop and archive' : 'Remove',
      ),
    ]);
  }

  UiNode _terminalsSection() {
    return UiColumn('advanced-terminals', [
      UiRow('advanced-terminals-header', [
        const SettingsSectionHeader(title: 'Running Terminals').build(),
        UiButton(
          'advanced-terminals-refresh',
          loading ? 'Refreshing…' : 'Refresh',
        ),
      ]),
      UiText('advanced-terminals-summary', summaryText),
      if (snapshot.terminals.isEmpty)
        UiText(
          'advanced-terminals-empty',
          loading ? 'Loading terminals…' : 'No running terminals.',
        )
      else
        UiColumn('advanced-terminals-rows', [
          for (final t in snapshot.terminals) _terminalRow(t),
        ]),
    ]);
  }

  UiNode _diagnosticsSection() {
    return UiColumn('advanced-diagnostics', [
      const SettingsSectionHeader(
        title: 'Diagnostics',
        description: 'Quick access to Supercli\'s on-disk session data and hook trace log.',
      ).build(),
      UiRow('advanced-diagnostics-sessions', [
        const UiText('advanced-diagnostics-sessions-label', 'Sessions folder'),
        const UiButton('advanced-diagnostics-sessions-open', 'Show in Finder'),
      ]),
      UiRow('advanced-diagnostics-trace', [
        const UiText('advanced-diagnostics-trace-label', 'Hooks trace log'),
        const UiButton('advanced-diagnostics-trace-open', 'Show in Finder'),
      ]),
    ]);
  }

  UiNode build() {
    return UiColumn('advanced-settings', [
      const SettingsPaneHeader(
        title: 'Advanced',
        description: 'Resource usage, cleanup, and on-disk data for Supercli\'s terminal hosts.',
      ).build(),
      _cleanupSection(),
      _memorySection(),
      _terminalsSection(),
      _diagnosticsSection(),
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
      'plugin-drag',
      '(plugin drag — needs gpuidart drag-and-drop, P0-13)',
    );
  }
}
