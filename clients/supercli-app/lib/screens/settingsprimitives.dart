/// Shared settings chrome primitives.
///
/// Port of `SettingsPaneHeader` (SettingsView.swift, 4249-4277),
/// `SettingsSectionHeader` (4278-4305), and `SettingsValueRow` (4306-4329):
/// the pane title/description header, the section title/description header,
/// and the label/value row used across every settings panel.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
library;

import 'package:gpuidart/gpuidart.dart';

/// Pane title + description header (Swift: `SettingsPaneHeader`).
final class SettingsPaneHeader {
  const SettingsPaneHeader({
    required this.title,
    this.description = '',
  });

  final String title;
  final String description;

  UiNode build() {
    return UiColumn('pane-header', [
      UiText('pane-header-title', title),
      if (description.isNotEmpty)
        UiText('pane-header-description', description),
    ]);
  }
}

/// Section title + description header (Swift: `SettingsSectionHeader`).
final class SettingsSectionHeader {
  const SettingsSectionHeader({
    required this.title,
    this.description = '',
  });

  final String title;
  final String description;

  UiNode build() {
    return UiColumn('section-header', [
      UiText('section-header-title', title),
      if (description.isNotEmpty)
        UiText('section-header-description', description),
    ]);
  }
}

/// Label/value row (Swift: `SettingsValueRow`).
final class SettingsValueRow {
  const SettingsValueRow({
    required this.label,
    required this.value,
  });

  final String label;
  final String value;

  UiNode build() {
    return UiRow('value-row', [
      UiText('value-row-label', label),
      UiText('value-row-value', value),
    ]);
  }
}
