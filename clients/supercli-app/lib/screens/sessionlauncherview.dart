/// Session launcher: new-session sheet with template picker.
///
/// Port of `SessionLauncherView.swift`.
library;

import 'package:gpuidart/gpuidart.dart';

final class SessionLauncherView {
  const SessionLauncherView({
    this.templates = const [],
    this.selectedTemplate,
  });

  final List<String> templates;
  final String? selectedTemplate;

  UiNode build() {
    return UiColumn('session-launcher', [
      const UiText('launcher-title', 'New Session'),
      const UiInput('launcher-name', placeholder: 'Session name…'),
      const UiText('launcher-templates-title', 'Template'),
      UiTable('launcher-templates', dataset: 'launcher-templates'),
      UiRow('launcher-buttons', [
        const UiButton('launcher-cancel', 'Cancel'),
        const UiButton('launcher-create', 'Create Session'),
      ]),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'launcher-templates',
        columns: const ['Template'],
        rows: templates.map((t) => [t]).toList(),
      );
}
