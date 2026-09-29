/// Plugin settings list logic.
/// 
/// Port of `PluginSettingsList.swift` from SupercliNative.
/// Value projection shared by the live list and the frozen drag card.
library;

/// A plugin/app/preset entry in the settings list.
final class PluginSettingsItem {
  const PluginSettingsItem({
    required this.id,
    required this.name,
    required this.command,
    this.appID,
    required this.installed,
    this.installCommand,
    this.websiteURL,
    this.installedVersion,
    this.availableVersion,
    required this.isApp,
    required this.isCustom,
    this.integrationInstallable = false,
    this.integrationInstalled = false,
    this.integrationSummary,
    this.integrationManualCommand,
    this.commands = const [],
  });

  final String id;
  final String name;
  final String command;
  final String? appID;
  final bool installed;
  final String? installCommand;
  final String? websiteURL;
  final String? installedVersion;
  final String? availableVersion;
  final bool isApp;
  final bool isCustom;
  
  /// The Host can install this runtime's Supercli integration (hooks + MCP).
  final bool integrationInstallable;
  
  /// The user installed it on this Host.
  final bool integrationInstalled;
  
  /// What the installer edits, in the user's words (runtime package copy).
  final String? integrationSummary;
  
  /// The provider's own command for registering the MCP shim by hand.
  final String? integrationManualCommand;
  
  final List<RemotePresetSummary> commands;

  /// Display version: installed version if installed, else available version.
  String? get displayVersion => installed ? installedVersion : availableVersion;

  PluginSettingsItem copyWith({List<RemotePresetSummary>? commands}) {
    return PluginSettingsItem(
      id: id,
      name: name,
      command: command,
      appID: appID,
      installed: installed,
      installCommand: installCommand,
      websiteURL: websiteURL,
      installedVersion: installedVersion,
      availableVersion: availableVersion,
      isApp: isApp,
      isCustom: isCustom,
      integrationInstallable: integrationInstallable,
      integrationInstalled: integrationInstalled,
      integrationSummary: integrationSummary,
      integrationManualCommand: integrationManualCommand,
      commands: commands ?? this.commands,
    );
  }

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is PluginSettingsItem &&
          runtimeType == other.runtimeType &&
          id == other.id &&
          name == other.name &&
          command == other.command;

  @override
  int get hashCode => Object.hash(id, name, command);
}

/// Summary of a remote preset (from Host).
final class RemotePresetSummary {
  const RemotePresetSummary({
    required this.id,
    required this.label,
    required this.command,
    this.projectID,
    this.pluginID,
  });

  final String id;
  final String label;
  final String command;
  final String? projectID;
  final String? pluginID;
}

/// Summary of a remote app (from Host).
final class RemoteAppSummary {
  const RemoteAppSummary({
    required this.id,
    required this.name,
    required this.command,
    required this.installed,
    this.installCommand,
    this.installedVersion,
    this.version,
  });

  final String id;
  final String name;
  final String command;
  final bool installed;
  final String? installCommand;
  final String? installedVersion;
  final String? version;
}

/// Summary of an available agent (from Host).
final class AvailableAgentSummary {
  const AvailableAgentSummary({
    required this.id,
    required this.name,
    required this.command,
    required this.installed,
    this.installCommand,
    this.websiteURL,
    this.integrationInstallable,
    this.integrationInstalled,
    this.integrationSummary,
    this.integrationManualCommand,
  });

  final String id;
  final String name;
  final String command;
  final bool installed;
  final String? installCommand;
  final String? websiteURL;
  final bool? integrationInstallable;
  final bool? integrationInstalled;
  final String? integrationSummary;
  final String? integrationManualCommand;
}

/// Workspace settings (from Host).
final class RemoteWorkspaceSettings {
  const RemoteWorkspaceSettings({
    this.availableAgents = const [],
    this.pluginOrder = const [],
  });

  final List<AvailableAgentSummary> availableAgents;
  final List<String> pluginOrder;
}

/// Bootstrap snapshot (from Host).
final class RemoteBootstrapSnapshot {
  const RemoteBootstrapSnapshot({
    this.workspaceSettings,
    this.availableApps = const [],
    this.presets = const [],
  });

  final RemoteWorkspaceSettings? workspaceSettings;
  final List<RemoteAppSummary>? availableApps;
  final List<RemotePresetSummary> presets;
}

/// Plugin settings list construction and ordering.
abstract final class PluginSettingsList {
  /// Builds the list items from a bootstrap snapshot.
  static List<PluginSettingsItem> items(RemoteBootstrapSnapshot? snapshot) {
    if (snapshot == null) return [];
    
    final items = <PluginSettingsItem>[];
    
    // Available agents
    for (final AvailableAgentSummary agent in snapshot.workspaceSettings?.availableAgents ?? []) {
      items.add(PluginSettingsItem(
        id: agent.id,
        name: agent.name,
        command: agent.command,
        installed: agent.installed,
        installCommand: agent.installCommand,
        websiteURL: agent.websiteURL,
        isApp: false,
        isCustom: false,
        integrationInstallable: agent.integrationInstallable ?? false,
        integrationInstalled: agent.integrationInstalled ?? false,
        integrationSummary: agent.integrationSummary,
        integrationManualCommand: agent.integrationManualCommand,
      ));
    }
    
    // Available apps
    for (final RemoteAppSummary app in snapshot.availableApps ?? []) {
      items.add(PluginSettingsItem(
        id: app.id,
        name: app.name,
        command: app.command,
        appID: app.id,
        installed: app.installed,
        installCommand: app.installCommand,
        installedVersion: app.installedVersion,
        availableVersion: app.version,
        isApp: true,
        isCustom: false,
      ));
    }
    
    // Presets (global only, projectID == null)
    for (final preset in snapshot.presets.where((p) => p.projectID == null)) {
      final executable = preset.command.split(' ').firstOrNull ?? '';
      final trimmed = executable.replaceAll(RegExp('^[\'"]|[\'"]\$'), '');
      final lastComponent = trimmed.split('/').last;
      final fallback = items.where((i) => i.command == lastComponent).firstOrNull?.id;
      final id = preset.pluginID ?? fallback ?? 'preset:${preset.id}';
      
      final index = items.indexWhere((i) => i.id == id);
      if (index >= 0) {
        final existing = items[index];
        items[index] = existing.copyWith(
          commands: [...existing.commands, preset],
        );
      } else {
        items.add(PluginSettingsItem(
          id: id,
          name: preset.label,
          command: preset.command,
          installed: true,
          isApp: false,
          isCustom: true,
          commands: [preset],
        ));
      }
    }
    
    // Ordering: retain user's pluginOrder, then append preset-derived order
    final presetOrder = snapshot.presets
        .where((p) => p.projectID == null)
        .map((preset) {
          final item = items.where(
            (i) => i.commands.any((c) => c.id == preset.id),
          ).firstOrNull;
          return item?.id;
        })
        .whereType<String>()
        .toList();
    
    final order = [
      ...?snapshot.workspaceSettings?.pluginOrder,
      ...presetOrder,
    ];
    
    final rank = <String, int>{};
    for (var i = 0; i < order.length; i++) {
      rank.putIfAbsent(order[i], () => i);
    }
    
    final indexed = items.asMap().entries.toList();
    indexed.sort((a, b) {
      final left = rank[a.value.id] ?? 0x7FFFFFFF;
      final right = rank[b.value.id] ?? 0x7FFFFFFF;
      if (left == right) return a.key.compareTo(b.key);
      return left.compareTo(right);
    });
    
    return indexed.map((e) => e.value).toList();
  }

  /// Reorder only the visible subset, leaving filtered and inactive rows in
  /// their original slots. The Host applies the same merge under its lock.
  static List<String> merging(List<String> subset, List<String> order) {
    final result = List<String>.from(order);
    for (final id in subset) {
      if (!result.contains(id)) result.add(id);
    }
    final moved = subset.toSet();
    final replacements = subset.iterator;
    return result.map((id) {
      if (moved.contains(id)) {
        return replacements.moveNext() ? replacements.current : id;
      }
      return id;
    }).toList();
  }
}
