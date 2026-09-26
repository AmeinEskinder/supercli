/// Session launcher: new-session sheet with template picker.
///
/// Port of `SessionLauncherView.swift`. Row 175 — [DESKTOP] parity.
/// GAP: no native-window screenshot proof yet (screenshot proof pending).
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

/// Row 175: Transient launcher pane for starting a new session inside a
/// group without leaving the terminal area.
///
/// Unlike [SessionLauncherView] (a modal sheet), the transient pane lives in
/// the pane tree as a placeholder leaf: it shows the group's recent
/// templates plus a quick-pick of agents, and replaces itself with the new
/// session pane once launched (or collapses on cancel).
final class TransientLauncherPane {
  const TransientLauncherPane({
    required this.groupId,
    this.agents = const [],
    this.templates = const [],
  });

  final String groupId;
  final List<String> agents;
  final List<String> templates;

  UiNode build() {
    return UiColumn('transient-launcher-$groupId', [
      const UiText('transient-title', 'New session in group'),
      const UiText('transient-agents-title', 'Agent'),
      UiRow('transient-agents', [
        for (var i = 0; i < agents.length; i++)
          UiButton('transient-agent-$i', agents[i]),
      ]),
      const UiText('transient-templates-title', 'Template'),
      UiRow('transient-templates', [
        for (var i = 0; i < templates.length; i++)
          UiButton('transient-template-$i', templates[i]),
      ]),
      UiRow('transient-buttons', const [
        UiButton('transient-cancel', 'Cancel'),
        UiButton('transient-launch', 'Launch'),
      ]),
    ]);
  }
}
