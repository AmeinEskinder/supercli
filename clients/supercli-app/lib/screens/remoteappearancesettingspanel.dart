/// Remote appearance settings panel.
///
/// Port of `RemoteAppearanceSettingsPanel` (SettingsView.swift, 1271-1562).
///
/// How Controllers look while scoped to a remote Host: theme mode, app
/// color, session titles, Host-side open-resources, and transparency. The
/// Host stores these values through `settings.workspace.set`, so another
/// Mac or phone uses the same workspace appearance. The terminal font is
/// never Host state — it is this Mac's display preference and renders the
/// remote panes too.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingsprimitives.dart';
import 'settingspanels.dart' as base;
import 'appearancesettingspanel.dart';
import 'hostsettingspanels.dart';
import 'openresourcessettingsrows.dart';

/// Remote appearance settings panel (Swift: `RemoteAppearanceSettingsPanel`).
final class RemoteAppearanceSettingsPanel {
  const RemoteAppearanceSettingsPanel({
    required this.scopeName,
    this.settings,
    this.fontFamily = 'SF Mono',
    this.fontSize = 13.0,
    this.fontLineHeight = 1.2,
    this.openResources = const OpenResourcesSettingsRows(),
    this.errorMessage,
  });

  final String scopeName;
  final RemoteAppearanceSettings? settings;
  final String fontFamily;
  final double fontSize;
  final double fontLineHeight;
  final OpenResourcesSettingsRows openResources;
  final String? errorMessage;

  UiNode build() {
    final settings = this.settings;
    final sections = <UiNode>[
      SettingsPaneHeader(
        title: 'Appearance',
        description: 'How Controllers look while scoped to $scopeName. '
            'The Host stores these values, so another Mac or phone uses the '
            'same workspace appearance.',
      ).build(),
    ];
    if (settings == null) {
      sections.add(UiText('remote-appearance-waiting',
          'Waiting for $scopeName\'s appearance…'));
    } else {
      final mode = ThemePreference.values.firstWhere(
        (m) => m.name == settings.theme,
        orElse: () => ThemePreference.system,
      );
      final tint = AppTint.values.firstWhere(
        (t) => t.name == settings.appTint,
        orElse: () => AppTint.none,
      );
      final titleMode = SessionTitleMode.values.firstWhere(
        (m) => m.rawValue == settings.sessionTitleMode,
        orElse: () => SessionTitleMode.agent,
      );
      sections.addAll([
        UiColumn('remote-appearance-mode', [
          const SettingsSectionHeader(
            title: 'Mode',
            description: '',
          ).build(),
          UiText('remote-appearance-mode-desc',
              'Applies to this Controller\'s window, sidebar and terminal colors while $scopeName is active.'),
          UiRow('remote-appearance-mode-picker', [
            for (final m in ThemePreference.values)
              UiButton('remote-appearance-mode-${m.name}',
                  '${m.title}${m == mode ? ' ✓' : ''}'),
          ]),
        ]),
        UiColumn('remote-appearance-tint', [
          SettingsSectionHeader(
            title: 'App color',
            description: 'Washes the Controller chrome and identifies $scopeName in workspace pickers.',
          ).build(),
          UiRow('remote-appearance-tint-swatches', [
            for (final t in AppTint.values)
              AppTintSwatch(tint: t, isSelected: t == tint).build(),
          ]),
        ]),
        UiColumn('remote-appearance-titles', [
          const SettingsSectionHeader(
            title: 'Session titles',
            description: 'What names new sessions on the Host until you rename them. Running session hosts read this live.',
          ).build(),
          base.SettingsSelect(
            id: 'remote-appearance-titles-picker',
            label: 'Session titles',
            selected: titleMode.title,
            options: [for (final m in SessionTitleMode.values) m.title],
          ).fallback(),
        ]),
        UiColumn('remote-appearance-open-resources', [
          const SettingsSectionHeader(
            title: 'Open resources',
            description: 'Choose which Host-side App opens each supported type in this workspace.',
          ).build(),
          openResources.build(),
        ]),
        UiColumn('remote-appearance-transparency', [
          SettingsSectionHeader(
            title: 'Transparency',
            description: 'Controls this Controller\'s background and terminal surface whenever $scopeName is selected.',
          ).build(),
          TransparencySliderRow(
            title: 'Background',
            value: settings.backgroundOpacity,
          ).build(),
          TransparencySliderRow(
            title: 'Surface',
            value: settings.surfaceOpacity,
          ).build(),
          const UiButton(
              'remote-appearance-transparency-revert', 'Revert to default'),
        ]),
        TerminalFontSection(
          family: fontFamily,
          size: fontSize,
          lineHeight: fontLineHeight,
          description: 'Fonts render on this Mac, so this is this Controller\'s '
              'own setting: it applies to $scopeName\'s terminals and every other '
              'workspace alike. ⌘+ and ⌘− zoom all panes; ⌘0 returns to 13 pt.',
        ).build(),
      ]);
    }
    if (errorMessage != null) {
      sections.add(UiText('remote-appearance-error', errorMessage!));
    }
    return UiColumn('remote-appearance-settings', sections);
  }
}
