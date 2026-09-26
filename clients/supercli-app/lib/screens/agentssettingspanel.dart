/// Settings ▸ Agents: agent CLI management.
///
/// Port of `AgentsSettingsPanel.swift`. Covers checklist row 202:
/// "Settings ▸ Agents (install CLI, install/reinstall integration,
/// commands and variants, default, activate, reorder)".
library;

import 'package:gpuidart/gpuidart.dart';

/// One installed agent CLI.
final class AgentInfo {
  const AgentInfo({
    required this.id,
    required this.name,
    this.cliPath = '',
    this.version = '',
    this.isDefault = false,
    this.isActive = true,
    this.commands = const [],
  });

  final String id;
  final String name;
  final String cliPath;
  final String version;
  final bool isDefault;
  final bool isActive;
  final List<String> commands;

  AgentInfo copyWith({
    bool? isDefault,
    bool? isActive,
  }) =>
      AgentInfo(
        id: id,
        name: name,
        cliPath: cliPath,
        version: version,
        isDefault: isDefault ?? this.isDefault,
        isActive: isActive ?? this.isActive,
        commands: commands,
      );
}

/// Pure list operations for the agents panel (reorder, default, active).
final class AgentListModel {
  AgentListModel(List<AgentInfo> agents) : _agents = List.of(agents);

  final List<AgentInfo> _agents;
  List<AgentInfo> get agents => List.unmodifiable(_agents);

  /// Move the agent with [id] to [newIndex]. Returns false if not found.
  bool move(String id, int newIndex) {
    final oldIndex = _agents.indexWhere((a) => a.id == id);
    if (oldIndex < 0) return false;
    final clamped = newIndex.clamp(0, _agents.length - 1);
    final agent = _agents.removeAt(oldIndex);
    _agents.insert(clamped, agent);
    return true;
  }

  /// Set the default agent (exactly one). Returns false if not found.
  bool setDefault(String id) {
    if (!_agents.any((a) => a.id == id)) return false;
    for (var i = 0; i < _agents.length; i++) {
      _agents[i] = _agents[i].copyWith(isDefault: _agents[i].id == id);
    }
    return true;
  }

  /// Toggle the active flag. Returns the new value, or null if not found.
  bool? toggleActive(String id) {
    final i = _agents.indexWhere((a) => a.id == id);
    if (i < 0) return null;
    _agents[i] = _agents[i].copyWith(isActive: !_agents[i].isActive);
    return _agents[i].isActive;
  }

  AgentInfo? get defaultAgent {
    for (final a in _agents) {
      if (a.isDefault) return a;
    }
    return null;
  }
}

/// Settings ▸ Agents panel.
final class AgentsSettingsPanel {
  const AgentsSettingsPanel({
    this.agents = const [],
  });

  final List<AgentInfo> agents;

  UiNode build() {
    return UiColumn('agents-settings', [
      const UiText('agents-title', 'Agents'),
      const UiText('agents-sub',
          'Agent CLIs available to sessions. The default handles new sessions.'),
      UiTable('agents-table', dataset: 'agents-settings'),
      UiRow('agents-actions', [
        const UiButton('agents-install-cli', 'Install CLI…'),
        const UiButton('agents-install-integration', 'Install Integration'),
        const UiButton('agents-reinstall-integration', 'Reinstall Integration'),
        const UiButton('agents-set-default', 'Set Default'),
        const UiButton('agents-move-up', 'Move Up'),
        const UiButton('agents-move-down', 'Move Down'),
      ]),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'agents-settings',
        columns: const ['Agent', 'Version', 'Default', 'Active', 'Commands'],
        rows: agents
            .map((a) => [
                  a.name,
                  a.version,
                  a.isDefault ? 'Yes' : '',
                  a.isActive ? 'Yes' : '',
                  a.commands.join(', '),
                ])
            .toList(),
      );
}
