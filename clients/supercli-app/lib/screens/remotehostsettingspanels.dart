/// Remote Host notifications and features panels.
///
/// Port of `RemoteNotificationsSettingsPanel` (SettingsView.swift,
/// 1563-1718) and `RemoteFeaturesSettingsPanel` (1719-1882).
///
/// How a remote Host flags sessions that need the user, plus delivery
/// diagnostics for the Controller Mac; and the feature switches stored and
/// enforced by the selected Host. The feature rows stay data-driven from
/// the native registry; the wire uses stable named fields (still spelled
/// `experimentalSettings`) so unknown future additions remain additive.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingsprimitives.dart';
import 'settingspanels.dart' as base;
import 'hostsettingspanels.dart';
import 'featuressettingspanel.dart';

/// Maps a stable feature key to its additive settings field
/// (Swift: `RemoteFeaturesSettingsPanel.value(_:in:)`).
bool remoteFeatureValue(String key, RemoteExperimentalSettings settings) {
  switch (key) {
    case 'worktrees':
      return settings.worktrees;
    case 'sessionsMcp':
      return settings.sessionsMcp;
    case 'browserMcp':
      return settings.browserMcp;
    case 'computerUse':
      return settings.computerUse;
    case 'profiles':
      return settings.workspaces;
    default:
      return false;
  }
}

/// Remote Host notifications panel
/// (Swift: `RemoteNotificationsSettingsPanel`).
final class RemoteNotificationsSettingsPanel {
  const RemoteNotificationsSettingsPanel({
    required this.scopeName,
    this.settings,
    this.macTestInFlight = false,
    this.lastMacTestLabel = '',
    this.macTestNeedsSystemSettings = false,
    this.pushRegisterSupported = false,
    this.notifyWhenDoneSupported = false,
    this.errorMessage,
  });

  final String scopeName;
  final RemoteNotificationSettings? settings;
  final bool macTestInFlight;
  final String lastMacTestLabel;
  final bool macTestNeedsSystemSettings;
  final bool pushRegisterSupported;
  final bool notifyWhenDoneSupported;
  final String? errorMessage;

  UiNode build() {
    final settings = this.settings;
    final sections = <UiNode>[
      SettingsPaneHeader(
        title: 'Notifications',
        description: 'How $scopeName flags sessions that need you, plus delivery diagnostics for this Controller Mac.',
      ).build(),
    ];
    if (settings != null) {
      sections.add(UiColumn('remote-notifications-attention', [
        const SettingsSectionHeader(
          title: 'Attention',
          description: 'Applied by the Host to its session activity before every Controller receives it.',
        ).build(),
        UiColumn('remote-notifications-menu-attention-labeled', [
          base.SettingsToggle(
            id: 'remote-notifications-menu-attention',
            label: 'Flag menus waiting for a choice',
            value: settings.menuAttentionDetection,
          ).fallback(),
          const UiText('remote-notifications-menu-attention-desc',
              'Show the yellow attention dot when an agent draws a pick-an-option menu on the Host.'),
        ]),
      ]));
    }
    sections.add(UiColumn('remote-notifications-this-mac', [
      const SettingsSectionHeader(
        title: 'This Mac',
        description: '',
      ).build(),
      UiText('remote-notifications-this-mac-desc',
          'Tests the notification banner on the Controller you are using now; it does not run anything on $scopeName.'),
      UiButton('remote-notifications-mac-test',
          macTestInFlight ? 'Sending…' : 'Send a test notification on this Mac'),
      SettingsValueRow(
        label: 'Last Mac test',
        value: lastMacTestLabel,
      ).build(),
      if (macTestNeedsSystemSettings)
        const UiButton('remote-notifications-mac-settings',
            'Open Mac Notification Settings…'),
    ]));
    sections.add(UiColumn('remote-notifications-host-delivery', [
      const SettingsSectionHeader(
        title: 'Host delivery',
        description: 'Phone delivery is shown only when the Host advertises it. Upstash/Linux Hosts can still surface attention here without pretending to own an APNs path.',
      ).build(),
      SettingsValueRow(
        label: 'Phone registration',
        value: pushRegisterSupported
            ? 'Supported'
            : 'Not advertised by this Host',
      ).build(),
      SettingsValueRow(
        label: 'Notify when done',
        value: notifyWhenDoneSupported
            ? 'Available per session'
            : 'Not advertised by this Host',
      ).build(),
    ]));
    if (errorMessage != null) {
      sections.add(UiText('remote-notifications-error', errorMessage!));
    }
    return UiColumn('remote-notifications-settings', sections);
  }
}

/// Remote Host features panel (Swift: `RemoteFeaturesSettingsPanel`).
///
/// Feature switches stored and enforced by the selected Host. Session-tool
/// changes apply to sessions started after the toggle.
final class RemoteFeaturesSettingsPanel {
  const RemoteFeaturesSettingsPanel({
    required this.scopeName,
    this.settings,
    this.overrides = const {},
    this.errorMessage,
  });

  final String scopeName;
  final RemoteExperimentalSettings? settings;
  final Map<String, bool> overrides;
  final String? errorMessage;

  UiNode _featureRow(FeatureDefinition feature) {
    final value = overrides[feature.key] ??
        (settings == null
            ? feature.defaultOn
            : remoteFeatureValue(feature.key, settings!));
    return UiColumn('remote-feature-${feature.key}-labeled', [
      base.SettingsToggle(
        id: 'remote-feature-${feature.key}',
        label: feature.title,
        value: value,
      ).fallback(),
      UiText('remote-feature-${feature.key}-summary', feature.summary),
    ]);
  }

  UiNode build() {
    final settings = this.settings;
    final sections = <UiNode>[
      SettingsPaneHeader(
        title: 'Features',
        description: 'Optional features owned by $scopeName. Session-tool changes apply to sessions started after the toggle.',
      ).build(),
    ];
    if (settings == null) {
      sections.add(UiText('remote-features-waiting',
          'Waiting for $scopeName\'s feature settings…'));
    } else if (allFeatures.isEmpty) {
      sections.add(const UiText('remote-features-empty',
          'No optional features are available in this build.'));
    } else {
      final shipped =
          allFeatures.where((f) => !f.isExperimental).toList(growable: false);
      final experimental =
          allFeatures.where((f) => f.isExperimental).toList(growable: false);
      sections.add(UiColumn('remote-features-shipped', [
        for (final f in shipped) _featureRow(f),
      ]));
      if (experimental.isNotEmpty) {
        sections.add(UiColumn('remote-features-experimental', [
          const SettingsSectionHeader(
            title: 'Experimental',
            description: experimentalSectionDescription,
          ).build(),
          for (final f in experimental) _featureRow(f),
        ]));
      }
    }
    if (errorMessage != null) {
      sections.add(UiText('remote-features-error', errorMessage!));
    }
    return UiColumn('remote-features-settings', sections);
  }
}
