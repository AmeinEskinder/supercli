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
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingsprimitives.dart';
import 'settingspanels.dart' as base;

/// Local Notifications settings panel
/// (Swift: `NotificationsSettingsPanel`).
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
      base.SettingsToggle(
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
