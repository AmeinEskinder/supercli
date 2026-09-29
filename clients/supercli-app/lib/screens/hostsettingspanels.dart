/// Host-scoped settings panels.
///
/// Port of the remote-Host settings panels in SettingsView.swift:
/// `HostAppearanceSettingsPanel` (306-521),
/// `HostAdvancedSettingsPanel` (522-641),
/// `HostAccessSettingsPanel` (642-801),
/// `HostTranscriptsSettingsPanel` (802-1003),
/// `HostNotificationsSettingsPanel` (1004-1120),
/// `HostFeaturesSettingsPanel` (1121-1270), and
/// `HostSettingsUpdateRequiredPanel` (1883-1938).
///
/// These panels edit settings that live on the remote Host through the
/// Host contract (`settings.workspace.set`); nothing here mutates local
/// state directly. Edits are owned by the app layer (via
/// `supercli-client-ffi`); the panels carry the current values plus the
/// scope label and render the rows, segmented controls, and override
/// footnotes exactly as the Swift originals describe them.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingsprimitives.dart';
import 'settingspanels.dart' as base;
import 'appearancesettingspanel.dart';

/// Theme preference for the Host appearance panel
/// (Swift: `ThemePreference` in HostAppearanceSettingsPanel).
enum ThemePreference {
  system,
  light,
  dark;

  /// Segmented-control label (Swift: `ThemePreference.title`).
  String get title {
    switch (this) {
      case ThemePreference.system:
        return 'System';
      case ThemePreference.light:
        return 'Light';
      case ThemePreference.dark:
        return 'Dark';
    }
  }
}

/// App tint swatch names in display order (Swift: `AppTint.allCases`).
List<AppTint> get appTints => AppTint.values;

/// Per-workspace settings read from the Host
/// (Swift: `RemoteWorkspaceSettings`).
final class RemoteWorkspaceSettings {
  const RemoteWorkspaceSettings({
    this.autoStopArchiveMinutes = 60,
    this.sidebarStoppedLimit = 10,
    this.mcpNonchildWriteAccess = 'ask',
    this.mcpWorktreeAccess = false,
    this.browserDefaultAccess = 'ask',
    this.mcpAutoAddBrowserScreenshots = false,
    this.transcriptSettings,
    this.appearanceSettings,
    this.notificationSettings,
    this.experimentalSettings,
  });

  final int autoStopArchiveMinutes;
  final int sidebarStoppedLimit;
  final String mcpNonchildWriteAccess;
  final bool mcpWorktreeAccess;
  final String browserDefaultAccess;
  final bool mcpAutoAddBrowserScreenshots;
  final RemoteTranscriptSettings? transcriptSettings;
  final RemoteAppearanceSettings? appearanceSettings;
  final RemoteNotificationSettings? notificationSettings;
  final RemoteExperimentalSettings? experimentalSettings;
}

/// Transcript content settings on the Host
/// (Swift: `RemoteTranscriptSettings`).
final class RemoteTranscriptSettings {
  const RemoteTranscriptSettings({
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
}

/// Appearance settings on the Host
/// (Swift: `RemoteAppearanceSettings`).
final class RemoteAppearanceSettings {
  const RemoteAppearanceSettings({
    this.theme = 'system',
    this.appTint = 'none',
    this.sessionTitleMode = 'agent',
    this.backgroundOpacity = 1.0,
    this.surfaceOpacity = 1.0,
    this.backgroundTone = 0.0,
    this.surfaceTone = 0.0,
  });

  final String theme;
  final String appTint;
  final String sessionTitleMode;
  final double backgroundOpacity;
  final double surfaceOpacity;
  final double backgroundTone;
  final double surfaceTone;
}

/// Notification settings on the Host
/// (Swift: `RemoteNotificationSettings`).
final class RemoteNotificationSettings {
  const RemoteNotificationSettings({
    this.menuAttentionDetection = true,
  });

  final bool menuAttentionDetection;
}

/// Experimental/shipped feature flags on the Host
/// (Swift: `RemoteExperimentalSettings`).
final class RemoteExperimentalSettings {
  const RemoteExperimentalSettings({
    this.worktrees = false,
    this.sessionsMcp = false,
    this.browserMcp = false,
    this.computerUse = false,
    this.workspaces = false,
  });

  final bool worktrees;
  final bool sessionsMcp;
  final bool browserMcp;
  final bool computerUse;
  final bool workspaces;
}

/// A feature toggle row descriptor
/// (Swift: `AppFeature` in HostFeaturesSettingsPanel).
final class AppFeature {
  const AppFeature({
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

/// Minute options for the auto-cleanup stepper
/// (Swift: `HostAdvancedSettingsPanel.minuteOptions`).
const List<int> autoStopMinuteOptions = [0, 15, 30, 60, 120, 240];

/// Labels for the minute options (Swift: `HostAdvancedSettingsPanel.minuteLabels`).
const Map<int, String> autoStopMinuteLabels = {
  0: 'Off',
  15: '15 minutes',
  30: '30 minutes',
  60: '1 hour',
  120: '2 hours',
  240: '4 hours',
};

/// Stopped-sidebar limit options (Swift: `limitOptions`).
const List<int> sidebarStoppedLimitOptions = [5, 10, 20, 50, 100];

/// Labeled toggle row with an optional subtitle
/// (Swift: `LabeledContent` + `Toggle` rows used across Host panels).
UiNode labeledToggleRow({
  required String id,
  required String title,
  String subtitle = '',
  required bool value,
}) {
  return UiColumn('${id}-labeled', [
    base.SettingsToggle(id: id, label: title, value: value).fallback(),
    if (subtitle.isNotEmpty) UiText('$id-subtitle', subtitle),
  ]);
}

/// Host appearance panel (Swift: `HostAppearanceSettingsPanel`, 306-521).
///
/// Edits the remote Host's workspace appearance through
/// `settings.workspace.set`. Mode, tint, transparency, and terminal font
/// persist per workspace. Color is a workspace-only value and survives the
/// inheritance reset. Inherited resets notify; the app layer debounces
/// reload requests so one drag emits a single reload.
final class HostAppearanceSettingsPanel {
  const HostAppearanceSettingsPanel({
    required this.home,
    required this.name,
    this.isDefaultInstance = false,
    this.defaultWorkspaceLabel = '',
    this.mode = ThemePreference.system,
    this.tint = AppTint.none,
    this.backgroundOpacity = 1.0,
    this.surfaceOpacity = 1.0,
    this.fontFamily = 'SF Mono',
    this.fontSize = 13.0,
    this.fontLineHeight = 1.2,
    this.hasOverrides = false,
  });

  final String home;
  final String name;
  final bool isDefaultInstance;
  final String defaultWorkspaceLabel;
  final ThemePreference mode;
  final AppTint tint;
  final double backgroundOpacity;
  final double surfaceOpacity;
  final String fontFamily;
  final double fontSize;
  final double fontLineHeight;
  final bool hasOverrides;

  UiNode build() {
    return UiColumn('host-appearance', [
      const SettingsPaneHeader(
        title: 'Appearance',
        description:
            'Theme, app color, transparency, and terminal font for this workspace.',
      ).build(),
      if (!isDefaultInstance)
        UiColumn('host-appearance-inherit', [
          const SettingsSectionHeader(title: 'Workspace settings').build(),
          UiText('host-appearance-inherit-desc',
              'Inherits $defaultWorkspaceLabel’s appearance.'),
          const UiButton('host-appearance-use-custom', 'Use custom values'),
          if (hasOverrides)
            const UiButton(
                'host-appearance-reset-inherited', 'Reset to inherited'),
        ]),
      const SettingsSectionHeader(title: 'Mode').build(),
      UiRow('host-appearance-mode', [
        for (final m in ThemePreference.values)
          UiButton('host-appearance-mode-${m.name}',
              '${m.title}${m == mode ? ' ✓' : ''}'),
      ]),
      const SettingsSectionHeader(title: 'App color').build(),
      UiRow('host-appearance-tint', [
        for (final t in appTints)
          UiButton('host-appearance-tint-${t.name}',
              '${t.title}${t == tint ? ' ✓' : ''}'),
      ]),
      const SettingsSectionHeader(
        title: 'Transparency',
        description:
            'Reverting to opaque surfaces is instant; every change notifies the app.',
      ).build(),
      UiColumn('host-appearance-transparency', [
        UiText('host-appearance-bg-opacity',
            'Window background: ${(backgroundOpacity * 100).round()}%'),
        UiText('host-appearance-surface-opacity',
            'Panels: ${(surfaceOpacity * 100).round()}%'),
      ]),
      const SettingsSectionHeader(title: 'Terminal font').build(),
      UiColumn('host-appearance-font', [
        UiText('host-appearance-font-family', fontFamily),
        UiText('host-appearance-font-size', '${fontSize.toStringAsFixed(1)} pt'),
        UiText('host-appearance-font-lineheight',
            'Line height ${fontLineHeight.toStringAsFixed(2)}'),
      ]),
    ]);
  }
}

/// Host advanced panel (Swift: `HostAdvancedSettingsPanel`, 522-641).
///
/// Auto-cleanup minutes and stopped-sidebar limit are edited through
/// `settings.workspace.set`. Invalid text input restores the last saved
/// value on commit; the Host save error (if any) surfaces inline.
final class HostAdvancedSettingsPanel {
  const HostAdvancedSettingsPanel({
    required this.scopeName,
    this.settings,
    this.draftMinutes = '60',
    this.draftLimit = '10',
    this.errorMessage,
  });

  final String scopeName;
  final RemoteWorkspaceSettings? settings;
  final String draftMinutes;
  final String draftLimit;
  final String? errorMessage;

  /// Minute label for an option (Swift: `minuteLabels`).
  static String minuteLabel(int minutes) =>
      autoStopMinuteLabels[minutes] ?? '$minutes minutes';

  UiNode build() {
    final saved = settings;
    return UiColumn('host-advanced', [
      SettingsPaneHeader(
        title: 'Advanced',
        description: 'Advanced settings for $scopeName.',
      ).build(),
      const SettingsSectionHeader(
        title: 'Auto-cleanup',
        description:
            'Stopped sessions are archived automatically after the chosen idle time.',
      ).build(),
      UiRow('host-advanced-minutes', [
        UiText('host-advanced-minutes-label', 'Archive stopped sessions after'),
        UiInput('host-advanced-minutes-input', placeholder: draftMinutes),
        if (saved != null)
          UiText('host-advanced-minutes-saved',
              'Saved: ${minuteLabel(saved.autoStopArchiveMinutes)}'),
      ]),
      UiRow('host-advanced-minute-options', [
        for (final m in autoStopMinuteOptions)
          UiButton('host-advanced-minute-$m', minuteLabel(m)),
      ]),
      const SettingsSectionHeader(
        title: 'Sidebar',
        description: 'How many stopped sessions the sidebar keeps visible.',
      ).build(),
      UiRow('host-advanced-limit', [
        UiText('host-advanced-limit-label', 'Stopped sessions in sidebar'),
        UiInput('host-advanced-limit-input', placeholder: draftLimit),
        if (saved != null)
          UiText('host-advanced-limit-saved',
              'Saved: ${saved.sidebarStoppedLimit}'),
      ]),
      UiRow('host-advanced-limit-options', [
        for (final l in sidebarStoppedLimitOptions)
          UiButton('host-advanced-limit-$l', '$l'),
      ]),
      if (errorMessage != null)
        UiText('host-advanced-error', errorMessage!),
    ]);
  }
}

/// Host access panel (Swift: `HostAccessSettingsPanel`, 642-801).
///
/// Session write policy, worktree permission, browser access, and the
/// screenshot gallery toggle for the scoped Host. The first four rows are
/// `settings.workspace.set` edits; the gallery row is a Host-persisted
/// workspace preference outside the `settings.workspace.set` contract.
final class HostAccessSettingsPanel {
  const HostAccessSettingsPanel({
    required this.scopeName,
    this.settings,
    this.sessionsMcpEnabled = false,
    this.browserMcpEnabled = false,
    this.stringOverrides = const {},
    this.boolOverrides = const {},
    this.errorMessage,
  });

  final String scopeName;
  final RemoteWorkspaceSettings? settings;
  final bool sessionsMcpEnabled;
  final bool browserMcpEnabled;
  final Map<String, String> stringOverrides;
  final Map<String, bool> boolOverrides;
  final String? errorMessage;

  /// Write-policy options (Swift: `McpNonchildWriteAccess.allCases`).
  static const List<String> writePolicyOptions = ['ask', 'allow', 'deny'];

  /// Browser default access options (Swift: `BrowserDefaultAccess.allCases`).
  static const List<String> browserAccessOptions = ['ask', 'allow', 'deny'];

  UiNode build() {
    final saved = settings;
    return UiColumn('host-access', [
      SettingsPaneHeader(
        title: 'Agent access',
        description: 'What agents may do in $scopeName.',
      ).build(),
      UiColumn('host-access-sessions', [
        const SettingsSectionHeader(
          title: 'Sessions',
          description: 'Write access for non-child sessions.',
        ).build(),
        UiRow('host-access-write-policy', [
          for (final o in writePolicyOptions)
            UiButton(
                'host-access-write-policy-$o',
                '$o${saved != null && saved.mcpNonchildWriteAccess == o ? ' ✓' : ''}'),
        ]),
        if (stringOverrides.containsKey('mcpNonchildWriteAccess'))
          UiText('host-access-write-policy-override',
              'Overridden: ${stringOverrides['mcpNonchildWriteAccess']}'),
        labeledToggleRow(
          id: 'host-access-worktree',
          title: 'Allow worktree access',
          subtitle: 'Agents may create and use git worktrees.',
          value: saved?.mcpWorktreeAccess ?? false,
        ),
      ]),
      UiColumn('host-access-browser', [
        const SettingsSectionHeader(
          title: 'Browser',
          description: 'Default browser access for agents.',
        ).build(),
        UiRow('host-access-browser-default', [
          for (final o in browserAccessOptions)
            UiButton(
                'host-access-browser-default-$o',
                '$o${saved != null && saved.browserDefaultAccess == o ? ' ✓' : ''}'),
        ]),
        labeledToggleRow(
          id: 'host-access-screenshots',
          title: 'Auto-add browser screenshots',
          subtitle:
              'Attach browser screenshots to the session gallery automatically.',
          value: saved?.mcpAutoAddBrowserScreenshots ?? false,
        ),
      ]),
      if (errorMessage != null)
        UiText('host-access-error', errorMessage!),
    ]);
  }
}

/// Host transcripts panel (Swift: `HostTranscriptsSettingsPanel`, 802-1003).
///
/// Seven content toggles plus the max-entries stepper (0/20/50/100);
/// the stepper's labels come from `SettingsMaxEntriesOption` and 0 means
/// "unlimited". Changes persist through `settings.workspace.set` as a
/// full replace (Swift builds a complete `RemoteTranscriptSettings` on
/// every save).
final class HostTranscriptsSettingsPanel {
  const HostTranscriptsSettingsPanel({
    required this.scopeName,
    this.settings,
    this.overrides = const {},
    this.maxEntriesOverride,
    this.errorMessage,
  });

  final String scopeName;
  final RemoteTranscriptSettings? settings;
  final Map<String, bool> overrides;
  final int? maxEntriesOverride;
  final String? errorMessage;

  /// Max-entries options (Swift: `SettingsMaxEntriesOption`).
  static const List<int> maxEntriesOptions = [0, 20, 50, 100];

  /// Label for a max-entries option (0 = unlimited).
  static String maxEntriesLabel(int entries) =>
      entries == 0 ? 'Unlimited' : '$entries';

  UiNode build() {
    final saved = settings;
    bool v(bool? b) => b ?? true;
    final rows = <UiNode>[
      SettingsPaneHeader(
        title: 'Transcripts',
        description: 'What transcript content is kept for $scopeName.',
      ).build(),
      const SettingsSectionHeader(title: 'Content').build(),
      labeledToggleRow(
          id: 'host-transcripts-session-info',
          title: 'Session info',
          value: v(saved?.includeSessionInfo)),
      labeledToggleRow(
          id: 'host-transcripts-user',
          title: 'User messages',
          value: v(saved?.includeUser)),
      labeledToggleRow(
          id: 'host-transcripts-assistant',
          title: 'Assistant messages',
          value: v(saved?.includeAssistant)),
      labeledToggleRow(
          id: 'host-transcripts-reasoning',
          title: 'Reasoning',
          value: v(saved?.includeReasoning)),
      labeledToggleRow(
          id: 'host-transcripts-tools',
          title: 'Tool calls',
          value: v(saved?.includeTools)),
      labeledToggleRow(
          id: 'host-transcripts-file-changes',
          title: 'File changes',
          value: v(saved?.includeFileChanges)),
      labeledToggleRow(
          id: 'host-transcripts-plan-updates',
          title: 'Plan updates',
          value: v(saved?.includePlanUpdates)),
      const SettingsSectionHeader(
        title: 'History',
        description: 'How many transcript entries to keep. 0 means unlimited.',
      ).build(),
      UiRow('host-transcripts-max-entries', [
        for (final o in maxEntriesOptions)
          UiButton(
              'host-transcripts-max-entries-$o',
              '${maxEntriesLabel(o)}'
              '${saved != null && saved.maxEntries == o ? ' ✓' : ''}'),
      ]),
    ];
    if (errorMessage != null) {
      rows.add(UiText('host-transcripts-error', errorMessage!));
    }
    return UiColumn('host-transcripts', rows);
  }
}

/// Host notifications panel (Swift: `HostNotificationsSettingsPanel`, 1004-1120).
///
/// The menu-attention detection toggle supports inherited reset for
/// non-default workspaces: the inherited value resolves to
/// `defaultWorkspace.notificationSettings.menuAttentionDetection`.
final class HostNotificationsSettingsPanel {
  const HostNotificationsSettingsPanel({
    required this.name,
    this.isDefaultInstance = false,
    this.defaultWorkspaceLabel = '',
    this.menuAttentionDetection = true,
    this.hasOverride = false,
    this.errorMessage,
  });

  final String name;
  final bool isDefaultInstance;
  final String defaultWorkspaceLabel;
  final bool menuAttentionDetection;
  final bool hasOverride;
  final String? errorMessage;

  UiNode build() {
    final rows = <UiNode>[
      const SettingsPaneHeader(
        title: 'Notifications',
        description: 'Notification behaviour for this workspace.',
      ).build(),
    ];
    if (!isDefaultInstance) {
      rows.add(UiColumn('host-notifications-inherit', [
        UiText('host-notifications-inherit-desc',
            'Inherits $defaultWorkspaceLabel’s notification settings.'),
        if (hasOverride)
          const UiButton(
              'host-notifications-reset-inherited', 'Reset to inherited'),
      ]));
    }
    rows.add(labeledToggleRow(
      id: 'host-notifications-menu-attention',
      title: 'Menu attention detection',
      subtitle:
          'Watch the menu bar for attention requests while sessions run.',
      value: menuAttentionDetection,
    ));
    if (errorMessage != null) {
      rows.add(UiText('host-notifications-error', errorMessage!));
    }
    return UiColumn('host-notifications', rows);
  }
}

/// Host features panel (Swift: `HostFeaturesSettingsPanel`, 1121-1270).
///
/// Shipped features are always visible; experimental ones hide behind the
/// experimental section toggle. Persisted as additive settings fields
/// (stable feature keys → settings keys, e.g. `experimentalSessionsMcp`)
/// via `settings.workspace.set`; the Host computes the default for the
/// current workspace when the key is absent. The inherited reset is
/// available for non-default workspaces and clears overrides.
final class HostFeaturesSettingsPanel {
  const HostFeaturesSettingsPanel({
    required this.name,
    this.isDefaultInstance = false,
    this.defaultWorkspaceLabel = '',
    this.features = const [],
    this.values = const {},
    this.hasOverride = false,
    this.errorMessage,
  });

  final String name;
  final bool isDefaultInstance;
  final String defaultWorkspaceLabel;
  final List<AppFeature> features;
  final Map<String, bool> values;
  final bool hasOverride;
  final String? errorMessage;

  UiNode build() {
    final shipped =
        features.where((f) => !f.isExperimental).toList(growable: false);
    final experimental =
        features.where((f) => f.isExperimental).toList(growable: false);
    final rows = <UiNode>[
      const SettingsPaneHeader(
        title: 'Features',
        description: 'Optional capabilities for this workspace.',
      ).build(),
    ];
    if (!isDefaultInstance) {
      rows.add(UiColumn('host-features-inherit', [
        UiText('host-features-inherit-desc',
            'Inherits $defaultWorkspaceLabel’s feature flags.'),
        if (hasOverride)
          const UiButton(
              'host-features-reset-inherited', 'Reset to inherited'),
      ]));
    }
    rows.add(const SettingsSectionHeader(title: 'Features').build());
    for (final f in shipped) {
      rows.add(labeledToggleRow(
        id: 'host-features-${f.key}',
        title: f.title,
        subtitle: f.summary,
        value: values[f.key] ?? f.defaultOn,
      ));
    }
    if (experimental.isNotEmpty) {
      rows.add(const SettingsSectionHeader(title: 'Experimental').build());
      for (final f in experimental) {
        rows.add(labeledToggleRow(
          id: 'host-features-${f.key}',
          title: f.title,
          subtitle: f.summary,
          value: values[f.key] ?? f.defaultOn,
        ));
      }
    }
    if (errorMessage != null) {
      rows.add(UiText('host-features-error', errorMessage!));
    }
    return UiColumn('host-features', rows);
  }
}

/// Update-required panel for older Hosts
/// (Swift: `HostSettingsUpdateRequiredPanel`, 1883-1938).
///
/// Rendered when a tab's settings aren't available on the connected Host
/// because it doesn't support `settings.workspace.set` yet.
final class HostSettingsUpdateRequiredPanel {
  const HostSettingsUpdateRequiredPanel({
    required this.tabTitle,
    required this.scopeName,
    this.protocolLabel = 'Host protocol',
  });

  final String tabTitle;
  final String scopeName;
  final String protocolLabel;

  UiNode build() {
    return UiColumn('host-update-required', [
      const UiText('host-update-required-title', 'Update required'),
      UiText('host-update-required-body',
          '$tabTitle settings need a newer $scopeName ($protocolLabel).'),
      const UiButton('host-update-required-action', 'How to update'),
    ]);
  }
}
