/// Presets settings: stored in the shared app-state.json with live pickup.
///
/// Port of `PresetsSettingsPanel.swift`. Covers checklist item 216:
/// "Presets stored in the shared app-state.json with live pickup of CLI
/// edits".
///
/// Presets are session-launch templates (model, working directory, initial
/// prompt). They live in the shared app-state.json so CLI edits are picked
/// up live by the desktop app.
library;

import 'package:gpuidart/gpuidart.dart';

/// One launch preset.
final class PresetEntry {
  const PresetEntry({
    required this.id,
    required this.name,
    this.model = '',
    this.workingDirectory = '',
    this.initialPrompt = '',
  });

  final String id;
  final String name;
  final String model;
  final String workingDirectory;
  final String initialPrompt;

  Map<String, Object> toJson() => {
        'id': id,
        'name': name,
        if (model.isNotEmpty) 'model': model,
        if (workingDirectory.isNotEmpty) 'working_directory': workingDirectory,
        if (initialPrompt.isNotEmpty) 'initial_prompt': initialPrompt,
      };

  factory PresetEntry.fromJson(Map<String, dynamic> json) => PresetEntry(
        id: json['id'] as String? ?? '',
        name: json['name'] as String? ?? 'Untitled',
        model: json['model'] as String? ?? '',
        workingDirectory: json['working_directory'] as String? ?? '',
        initialPrompt: json['initial_prompt'] as String? ?? '',
      );
}

/// Presets settings panel.
final class PresetsSettingsPanel {
  const PresetsSettingsPanel({this.presets = const []});

  final List<PresetEntry> presets;

  UiNode build() {
    return UiColumn('presets-settings', [
      const UiText('presets-title', 'Presets'),
      const UiText('presets-desc',
          'Launch templates. Stored in app-state.json; CLI edits apply live.'),
      UiTable('presets-table', dataset: 'presets-settings'),
      UiRow('presets-actions', [
        const UiButton('presets-new', 'New Preset'),
        const UiButton('presets-duplicate', 'Duplicate'),
        const UiButton('presets-delete', 'Delete'),
      ]),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'presets-settings',
        columns: const ['Preset', 'Model', 'Directory'],
        rows:
            presets.map((p) => [p.name, p.model, p.workingDirectory]).toList(),
      );

  /// Editor for the selected preset.
  UiNode editor(PresetEntry preset) {
    return UiColumn('preset-editor-${preset.id}', [
      UiText('preset-editor-title', 'Edit preset: ${preset.name}'),
      const UiInput('preset-name', placeholder: 'Name…'),
      const UiInput('preset-model', placeholder: 'Model…'),
      const UiInput('preset-dir', placeholder: 'Working directory…'),
      const UiInput('preset-prompt', placeholder: 'Initial prompt…'),
      UiRow('preset-editor-actions', [
        const UiButton('preset-save', 'Save'),
        const UiButton('preset-cancel', 'Cancel'),
      ]),
    ]);
  }
}
