/// Appearance settings panel.
///
/// Port of `AppearanceSettingsPanel` (SettingsView.swift, 2495-2730),
/// `TerminalFontSection` (2731-2871), `TransparencySliderRow` (2872-2903),
/// and `AppTintSwatch` (2904-2931).
///
/// Sections: theme mode, app tint swatches, session titles, transparency
/// sliders, terminal font, open-resources editor picker, terminal options.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingsprimitives.dart';
import 'settingspanels.dart';
import 'openresourcessettingsrows.dart';

/// App tint options (Swift: `AppTint` in Theme.swift).
///
/// Washes the workspace's window chrome — sidebar, content, and terminal
/// canvas. Each workspace keeps its own color.
enum AppTint {
  none,
  peel,
  amber,
  green,
  teal,
  blue,
  indigo,
  violet;

  String get title {
    switch (this) {
      case AppTint.none:
        return 'Default';
      case AppTint.peel:
        return 'Peel';
      case AppTint.amber:
        return 'Amber';
      case AppTint.green:
        return 'Green';
      case AppTint.teal:
        return 'Teal';
      case AppTint.blue:
        return 'Blue';
      case AppTint.indigo:
        return 'Indigo';
      case AppTint.violet:
        return 'Violet';
    }
  }
}

/// Session title mode (Swift: `SessionTitleMode` in Models.swift).
///
/// What names a session in the sidebar until renamed.
enum SessionTitleMode {
  firstPrompt,
  agent,
  off;

  /// Raw value (Swift: `SessionTitleMode.rawValue`).
  String get rawValue {
    switch (this) {
      case SessionTitleMode.firstPrompt:
        return 'first_prompt';
      case SessionTitleMode.agent:
        return 'agent';
      case SessionTitleMode.off:
        return 'off';
    }
  }

  String get title {
    switch (this) {
      case SessionTitleMode.firstPrompt:
        return 'First prompt';
      case SessionTitleMode.agent:
        return 'Live from agent';
      case SessionTitleMode.off:
        return 'Off';
    }
  }
}

/// ⌘T action (Swift: `CommandTAction` in Models.swift).
enum CommandTAction {
  newTerminal,
  presetPicker;

  String get title {
    switch (this) {
      case CommandTAction.newTerminal:
        return 'New terminal';
      case CommandTAction.presetPicker:
        return 'Preset screen';
    }
  }
}

/// Terminal font section (Swift: `TerminalFontSection`, 2731-2871).
///
/// Shared by the local, scoped-local, and remote Appearance panels.
/// The family picker lists this Mac's monospaced faces (plus the saved
/// family even when it is not installed, so a choice never silently
/// disappears); the size stepper shares its range with the ⌘+ / ⌘− / ⌘0
/// View-menu chords.
final class TerminalFontSection {
  TerminalFontSection({
    required this.family,
    required this.size,
    required this.lineHeight,
    this.description = '',
  });

  final String family;
  final double size;
  final double lineHeight;
  final String description;

  /// Shared copy for the local panels: the chords are the same everywhere
  /// (Swift: `TerminalFontSection.localDescription`).
  static const String localDescription =
      'Family and size for every terminal on this Mac. ⌘+ and ⌘− zoom all '
      'panes together; ⌘0 returns to 13 pt. '
      'Ghostty falls back to other installed faces for glyphs the chosen '
      'font lacks.';

  UiNode build() {
    return UiColumn('terminal-font-section', [
      const UiText('terminal-font-title', 'Terminal font'),
      if (description.isNotEmpty)
        UiText('terminal-font-description', description),
      UiRow('terminal-font-family', [
        const UiText('terminal-font-family-label', 'Family'),
        UiText('terminal-font-family-value', family),
      ]),
      UiRow('terminal-font-size', [
        const UiText('terminal-font-size-label', 'Size'),
        UiText('terminal-font-size-value', '${size.toStringAsFixed(1)} pt'),
      ]),
      UiRow('terminal-font-line-height', [
        const UiText('terminal-font-line-height-label', 'Line height'),
        UiText(
          'terminal-font-line-height-value',
          lineHeight.toStringAsFixed(2),
        ),
      ]),
    ]);
  }
}

/// Transparency slider row (Swift: `TransparencySliderRow`, 2872-2903).
///
/// Label, slider, and a fixed-width live percentage so the row doesn't
/// wiggle while dragging.
final class TransparencySliderRow {
  const TransparencySliderRow({required this.title, required this.value});

  final String title;
  final double value;

  UiNode build() {
    return UiRow('transparency-$title', [
      UiText('transparency-$title-label', title),
      UiText('transparency-$title-value', '${(value * 100).round()}%'),
    ]);
  }
}

/// App tint swatch (Swift: `AppTintSwatch`, 2904-2931).
final class AppTintSwatch {
  const AppTintSwatch({required this.tint, required this.isSelected});

  final AppTint tint;
  final bool isSelected;

  UiNode build() {
    return UiButton(
      'tint-${tint.name}',
      '${isSelected ? '● ' : ''}${tint.title}',
    );
  }
}

/// Appearance settings panel (Swift: `AppearanceSettingsPanel`, 2495-2730).
///
/// The local (Controller) appearance panel. A workspace instance inherits
/// the default workspace's appearance until it sets its own; the revert is
/// offered inline and drops its own mode, transparency and font — its color
/// stays (workspace-only value).
final class AppearanceSettingsPanel {
  AppearanceSettingsPanel({
    required this.settings,
    this.isDefaultInstance = true,
    this.defaultWorkspaceLabel = 'Personal',
    this.backgroundOpacity = 1.0,
    this.surfaceOpacity = 1.0,
    this.transparencyIsDefault = true,
    this.sessionTitleMode = SessionTitleMode.firstPrompt,
    this.commandTAction = CommandTAction.newTerminal,
    this.showSessionGallery = true,
    this.codeEditor = '',
    this.editorOptions = const [],
    this.openResources = const OpenResourcesSettingsRows(),
  });

  final AppSettings settings;
  final bool isDefaultInstance;
  final String defaultWorkspaceLabel;
  final double backgroundOpacity;
  final double surfaceOpacity;
  final bool transparencyIsDefault;
  final SessionTitleMode sessionTitleMode;
  final CommandTAction commandTAction;
  final bool showSessionGallery;
  final String codeEditor;
  final List<String> editorOptions;
  final OpenResourcesSettingsRows openResources;

  UiNode build() {
    final sections = <UiNode>[
      const SettingsPaneHeader(
        title: 'Appearance',
        description:
            'How Supercli looks. System follows your macOS appearance.',
      ).build(),
    ];
    if (!isDefaultInstance) {
      sections.add(
        UiColumn('appearance-inherit', [
          SettingsSectionHeader(
            title: 'Inherits from $defaultWorkspaceLabel',
            description:
                'This workspace uses the default workspace\'s appearance '
                'until a setting below is changed. Revert drops its own mode, '
                'transparency and font; its color stays.',
          ).build(),
          UiButton(
            'appearance-use-inherited',
            'Use $defaultWorkspaceLabel\'s appearance',
          ),
        ]),
      );
    }
    sections.addAll([
      UiColumn('appearance-mode', [
        const SettingsSectionHeader(
          title: 'Mode',
          description:
              'Applies to the window, sidebar and terminal colors. '
              'Claude Code has its own theme setting — run /config inside '
              'Claude Code and change Theme to match.',
        ).build(),
        UiRow('appearance-mode-picker', [
          for (final mode in ThemeMode.values)
            UiButton(
              'theme-${mode.name}',
              '${settings.theme == mode ? '● ' : ''}${mode.name}',
            ),
        ]),
      ]),
      UiColumn('appearance-tint', [
        const SettingsSectionHeader(
          title: 'App color',
          description:
              'Washes this workspace\'s window chrome — sidebar, '
              'content, and terminal canvas. Each workspace keeps its own color '
              '(also editable per workspace in Settings ▸ Workspaces).',
        ).build(),
        UiRow('appearance-tint-swatches', [
          for (final tint in AppTint.values)
            AppTintSwatch(
              tint: tint,
              isSelected: settings.accentColor == tint.index,
            ).build(),
        ]),
      ]),
      UiColumn('appearance-session-titles', [
        const SettingsSectionHeader(
          title: 'Session titles',
          description:
              'What names a session in the sidebar until you rename it. '
              'First prompt titles it once from your first message. Live from '
              'agent follows the agent\'s own task summary as it works (agents '
              'that publish one — Claude today), falling back to the first prompt '
              'until it appears. Renaming a session always wins.',
        ).build(),
        SettingsSelect(
          id: 'appearance-session-titles-picker',
          label: 'Session titles',
          selected: sessionTitleMode.title,
          options: [for (final m in SessionTitleMode.values) m.title],
        ).fallback(),
      ]),
      UiColumn('appearance-transparency', [
        const SettingsSectionHeader(
          title: 'Transparency',
          description:
              'Background is the window backdrop — the sidebar and '
              'everything behind the content; below 100% the desktop shows '
              'through it, natively blurred. Surface covers the terminal canvas, '
              'settings, and the other pages on top of it. 100% is fully opaque. '
              'Terminal text always stays fully opaque.',
        ).build(),
        TransparencySliderRow(
          title: 'Background',
          value: backgroundOpacity,
        ).build(),
        TransparencySliderRow(title: 'Surface', value: surfaceOpacity).build(),
        const UiButton('transparency-revert', 'Revert to default'),
      ]),
      TerminalFontSection(
        family: settings.terminalFont,
        size: settings.terminalFontSize,
        lineHeight: settings.lineHeight,
        description: TerminalFontSection.localDescription,
      ).build(),
      UiColumn('appearance-open-resources', [
        const SettingsSectionHeader(
          title: 'Open resources',
          description:
              'Choose what opens each supported type in this workspace. '
              'The editor is also used by "Open in editor" and the titlebar open button.',
        ).build(),
        SettingsSelect(
          id: 'appearance-editor',
          label: 'Editor',
          selected: codeEditor,
          options: editorOptions,
        ).fallback(),
        openResources.build(),
      ]),
      UiColumn('appearance-terminal', [
        const SettingsSectionHeader(
          title: 'Terminal',
          description: 'Choose what ⌘T opens and configure extras around the terminal view.',
        ).build(),
        UiRow('appearance-commandt-picker', [
          for (final action in CommandTAction.values)
            UiButton(
              'commandt-${action.name}',
              '${commandTAction == action ? '● ' : ''}${action.title}',
            ),
        ]),
        SettingsToggle(
          id: 'appearance-session-gallery',
          label: 'Session gallery',
          value: showSessionGallery,
        ).fallback(),
        const UiText(
          'appearance-session-gallery-desc',
          'Photo chip in the terminal title bar with the session\'s captures, plus Take Screenshot (⇧⌘S) to shoot into the session and attach it to the prompt. Turn off if you use your own screenshot tools.',
        ),
      ]),
    ]);
    return UiColumn('appearance-settings', sections);
  }
}
