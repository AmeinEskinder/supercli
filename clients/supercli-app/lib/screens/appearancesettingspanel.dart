/// Appearance settings panel.
///
/// Port of `AppearanceSettingsPanel` (SettingsView.swift, 2495-2731).
/// Sections: theme mode, app tint swatches, session titles, transparency
/// sliders, terminal font, open-resources editor picker, terminal options.
///
/// Renders through the RLE fallback pattern (UiRow/UiColumn/UiText/UiButton)
/// since gpuidart has no native settings widgets.
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingspanels.dart';

/// App tint options (Swift: `AppTint.allCases`).
///
/// Washes the workspace's window chrome — sidebar, content, and terminal
/// canvas. Each workspace keeps its own color.
enum AppTint {
  graphite,
  blue,
  purple,
  pink,
  red,
  orange,
  yellow,
  green;

  String get title {
    switch (this) {
      case AppTint.graphite:
        return 'Graphite';
      case AppTint.blue:
        return 'Blue';
      case AppTint.purple:
        return 'Purple';
      case AppTint.pink:
        return 'Pink';
      case AppTint.red:
        return 'Red';
      case AppTint.orange:
        return 'Orange';
      case AppTint.yellow:
        return 'Yellow';
      case AppTint.green:
        return 'Green';
    }
  }
}

/// Session title mode (Swift: `SessionTitleMode`).
///
/// What names a session in the sidebar until renamed.
enum SessionTitleMode {
  firstPrompt,
  liveFromAgent,
  manual;

  String get title {
    switch (this) {
      case SessionTitleMode.firstPrompt:
        return 'First prompt';
      case SessionTitleMode.liveFromAgent:
        return 'Live from agent';
      case SessionTitleMode.manual:
        return 'Manual';
    }
  }
}

/// Command-T action (Swift: `CommandTAction`).
enum CommandTAction {
  newSession,
  commandPalette,
  quickOpen;

  String get title {
    switch (this) {
      case CommandTAction.newSession:
        return 'New session';
      case CommandTAction.commandPalette:
        return 'Command palette';
      case CommandTAction.quickOpen:
        return 'Quick open';
    }
  }
}

/// Terminal font section (Swift: `TerminalFontSection`).
///
/// Shared by the local, scoped-local, and remote Appearance panels.
/// The family picker lists monospaced faces; the size stepper shares its
/// range with the zoom View-menu chords.
final class TerminalFontSection {
  TerminalFontSection({
    required this.family,
    required this.size,
    required this.lineHeight,
  });

  final String family;
  final double size;
  final double lineHeight;

  UiNode build() {
    return UiColumn('terminal-font-section', [
      const UiText('terminal-font-title', 'Terminal font'),
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
        UiText('terminal-font-line-height-value', lineHeight.toStringAsFixed(2)),
      ]),
    ]);
  }
}

/// Transparency slider row (Swift: `TransparencySliderRow`).
final class TransparencySliderRow {
  const TransparencySliderRow({
    required this.title,
    required this.value,
  });

  final String title;
  final double value;

  UiNode build() {
    return UiRow('transparency-$title', [
      UiText('transparency-$title-label', title),
      UiText(
        'transparency-$title-value',
        '${(value * 100).round()}%',
      ),
    ]);
  }
}

/// App tint swatch (Swift: `AppTintSwatch`).
final class AppTintSwatch {
  const AppTintSwatch({
    required this.tint,
    required this.isSelected,
  });

  final AppTint tint;
  final bool isSelected;

  UiNode build() {
    return UiButton(
      'tint-${tint.name}',
      '${isSelected ? '● ' : ''}${tint.title}',
    );
  }
}

/// Appearance settings panel.
///
/// Port of `AppearanceSettingsPanel` (SettingsView.swift).
final class AppearanceSettingsPanel {
  AppearanceSettingsPanel({
    required this.settings,
    this.backgroundOpacity = 1.0,
    this.surfaceOpacity = 1.0,
    this.sessionTitleMode = SessionTitleMode.firstPrompt,
    this.commandTAction = CommandTAction.newSession,
    this.showSessionGallery = true,
    this.codeEditor = '',
  });

  final AppSettings settings;
  final double backgroundOpacity;
  final double surfaceOpacity;
  final SessionTitleMode sessionTitleMode;
  final CommandTAction commandTAction;
  final bool showSessionGallery;
  final String codeEditor;

  UiNode build() {
    return UiColumn('appearance-settings', [
      const UiText('appearance-title', 'Appearance'),
      const UiText(
        'appearance-description',
        'How Supercli looks. System follows your macOS appearance.',
      ),
      // Mode: theme preference (system/light/dark)
      UiColumn('appearance-mode', [
        const UiText('appearance-mode-title', 'Mode'),
        const UiText(
          'appearance-mode-description',
          'Applies to the window, sidebar and terminal colors.',
        ),
        UiRow('appearance-mode-picker', [
          for (final mode in ThemeMode.values)
            UiButton(
              'theme-${mode.name}',
              '${settings.theme == mode ? '● ' : ''}${mode.name}',
            ),
        ]),
      ]),
      // App color: tint swatches
      UiColumn('appearance-tint', [
        const UiText('appearance-tint-title', 'App color'),
        const UiText(
          'appearance-tint-description',
          "Washes this workspace's window chrome — sidebar, content, and "
          'terminal canvas.',
        ),
        UiRow('appearance-tint-swatches', [
          for (final tint in AppTint.values)
            AppTintSwatch(
              tint: tint,
              isSelected: settings.accentColor == tint.index,
            ).build(),
        ]),
      ]),
      // Session titles
      UiColumn('appearance-session-titles', [
        const UiText('appearance-session-titles-title', 'Session titles'),
        UiText(
          'appearance-session-titles-value',
          sessionTitleMode.title,
        ),
      ]),
      // Transparency
      UiColumn('appearance-transparency', [
        const UiText('appearance-transparency-title', 'Transparency'),
        const TransparencySliderRow(title: 'Background', value: 1.0).build(),
        const TransparencySliderRow(title: 'Surface', value: 1.0).build(),
        UiButton(
          'transparency-revert',
          'Revert to default',
        ),
      ]),
      // Terminal font
      TerminalFontSection(
        family: settings.terminalFont,
        size: settings.terminalFontSize,
        lineHeight: settings.lineHeight,
      ).build(),
      // Open resources: editor picker
      UiColumn('appearance-editor', [
        const UiText('appearance-editor-title', 'Open resources'),
        UiRow('appearance-editor-picker', [
          const UiText('appearance-editor-label', 'Editor'),
          UiText(
            'appearance-editor-value',
            codeEditor.isEmpty ? 'System default' : codeEditor,
          ),
        ]),
      ]),
      // Terminal: Command-T action + session gallery
      UiColumn('appearance-terminal', [
        const UiText('appearance-terminal-title', 'Terminal'),
        UiRow('appearance-commandt', [
          const UiText('appearance-commandt-label', '⌘T'),
          UiText('appearance-commandt-value', commandTAction.title),
        ]),
        SettingsToggle(
          id: 'appearance-session-gallery',
          label: 'Session gallery',
          value: showSessionGallery,
        ).fallback(),
      ]),
    ]);
  }
}
