/// Quick-launch preset model + selection rules.
/// 
/// Port of `Presets.swift` from SupercliNative.
/// The preset list is FLAT and user-ordered (no per-CLI sections).
library;

import 'runtime_catalog.dart';
import 'tool_icons.dart';
import 'plugin_settings_list.dart';

/// Preset model.
final class Preset {
  Preset({
    required this.id,
    required this.label,
    required this.command,
    required this.enabled,
    required this.quickLaunch,
  });

  final String id;
  final String label;
  final String command;
  final bool enabled;
  final bool quickLaunch;

  /// Blank-terminal pseudo-preset.
  static const newTerminalID = '__new_terminal__';
  static final newTerminal = Preset(
    id: newTerminalID,
    label: 'Terminal',
    command: '',
    enabled: true,
    quickLaunch: false,
  );

  bool get isNewTerminal => id == newTerminalID;

  /// Tool the command maps to (null for plain shell commands).
  QuickPresetTool? get tool => QuickPresetTool.detect(command);

  /// Sanitized copy: quickLaunch only if command is non-empty.
  Preset sanitized() {
    return Preset(
      id: id,
      label: label,
      command: command,
      enabled: enabled,
      quickLaunch: quickLaunch && command.trim().isNotEmpty,
    );
  }

  Map<String, dynamic> toJson() => {
    'id': id,
    'label': label,
    'command': command,
    'enabled': enabled,
    'quick_launch': quickLaunch,
  };

  factory Preset.fromJson(Map<String, dynamic> json) => Preset(
    id: json['id'] as String,
    label: json['label'] as String,
    command: json['command'] as String,
    enabled: json['enabled'] as bool? ?? true,
    quickLaunch: json['quick_launch'] as bool? ?? false,
  );

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is Preset &&
          runtimeType == other.runtimeType &&
          id == other.id;

  @override
  int get hashCode => id.hashCode;
}

/// Splits presets into agents and plugins for the new-session menu.
/// Plugin identity comes from the Host's App catalog.
({List<Preset> agents, List<Preset> plugins}) splitPresetsForNewSessionMenu(
  List<Preset> presets,
) {
  final agents = <Preset>[];
  final plugins = <Preset>[];
  for (final preset in presets) {
    if (SupercliAppIconCatalog.isPluginCommand(preset.command)) {
      plugins.add(preset);
    } else {
      agents.add(preset);
    }
  }
  return (agents: agents, plugins: plugins);
}

/// Catalog-backed tool identification for quick presets.
final class QuickPresetTool {
  const QuickPresetTool._(this.rawValue);

  final String rawValue;

  /// Creates a tool if the runtime exists and supports quick launch.
  factory QuickPresetTool(String rawValue) {
    final runtime = SupercliRuntimeCatalog.runtime(id: rawValue);
    if (runtime == null || !runtime.supportsQuickLaunch) {
      throw ArgumentError('Invalid quick preset tool: $rawValue');
    }
    return QuickPresetTool._(runtime.legacySlug);
  }

  /// Creates a tool without validation (for source compatibility).
  const QuickPresetTool.unchecked(this.rawValue);

  static List<QuickPresetTool> get allCases =>
      SupercliRuntimeCatalog.runtimes
          .where((r) => r.supportsQuickLaunch)
          .map((r) => QuickPresetTool._(r.legacySlug))
          .toList();

  String get id => rawValue;

  SupercliRuntimeMetadata? get metadata =>
      SupercliRuntimeCatalog.runtime(id: rawValue);

  String get displayName {
    final label = metadata?.label;
    if (label == null) return rawValue;
    return label[0].toUpperCase() + label.substring(1);
  }

  String get iconKey => metadata?.iconKey ?? 'agent';

  /// Detects the tool from a command string.
  static QuickPresetTool? detect(String command) {
    final runtime = SupercliRuntimeCatalog.runtime(command: command);
    if (runtime == null || !runtime.supportsQuickLaunch) return null;
    return QuickPresetTool._(runtime.legacySlug);
  }

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is QuickPresetTool &&
          runtimeType == other.runtimeType &&
          rawValue == other.rawValue;

  @override
  int get hashCode => rawValue.hashCode;
}

/// Catalog-backed setup tool (includes non-quick-launchable tools).
final class SetupTool {
  const SetupTool._(this.rawValue);

  final String rawValue;

  factory SetupTool(String rawValue) {
    final runtime = SupercliRuntimeCatalog.runtime(id: rawValue);
    if (runtime == null) {
      throw ArgumentError('Invalid setup tool: $rawValue');
    }
    return SetupTool._(runtime.legacySlug);
  }

  const SetupTool.unchecked(this.rawValue);

  static List<SetupTool> get allCases =>
      SupercliRuntimeCatalog.runtimes
          .map((r) => SetupTool._(r.legacySlug))
          .toList();

  String get id => rawValue;

  SupercliRuntimeMetadata? get metadata =>
      SupercliRuntimeCatalog.runtime(id: rawValue);

  String get displayName {
    final label = metadata?.label;
    if (label == null) return rawValue;
    return label[0].toUpperCase() + label.substring(1);
  }

  List<String> get commandNames => metadata?.commandAliases ?? [rawValue];
  String get commandName => metadata?.commandAliases.firstOrNull ?? rawValue;

  String get defaultPresetCommand =>
      metadata?.defaultPreset?.command ?? commandName;

  QuickPresetTool? get quickPresetTool {
    try {
      return QuickPresetTool(rawValue);
    } catch (_) {
      return null;
    }
  }

  bool get isFavoriteCapable => metadata?.supportsQuickLaunch ?? false;

  String? get installCommand => metadata?.installCommand;
  String? get websiteURL => metadata?.installURL;

  bool get usesLifecycleHooks =>
      metadata?.capabilities.contains(SupercliRuntimeCapability.lifecycleHooks) == true;

  /// Resolves a command to the CLI it launches.
  static SetupTool? detect(String command) {
    final runtime = SupercliRuntimeCatalog.runtime(command: command);
    if (runtime == null) return null;
    return SetupTool._(runtime.legacySlug);
  }

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is SetupTool &&
          runtimeType == other.runtimeType &&
          rawValue == other.rawValue;

  @override
  int get hashCode => rawValue.hashCode;
}

/// Install status for a tool.
final class ToolInstallStatus {
  const ToolInstallStatus({
    required this.tool,
    this.path,
    this.usage = ToolUsageStats.none,
  });

  final SetupTool tool;
  final String? path;
  final ToolUsageStats usage;

  String get id => tool.id;
  bool get installed => path != null;
}

/// Usage statistics for a tool.
final class ToolUsageStats {
  const ToolUsageStats({
    required this.sessionCount,
    required this.recentCount,
    this.lastUsed,
  });

  final int sessionCount;
  final int recentCount;
  final DateTime? lastUsed;

  static const none = ToolUsageStats(sessionCount: 0, recentCount: 0);

  bool get hasAny => sessionCount > 0;

  /// Ordering: recent activity beats lifetime volume.
  static bool moreUsed(ToolUsageStats a, ToolUsageStats b) {
    if (a.recentCount != b.recentCount) return a.recentCount > b.recentCount;
    if (a.sessionCount != b.sessionCount) return a.sessionCount > b.sessionCount;
    final aLast = a.lastUsed ?? DateTime.fromMillisecondsSinceEpoch(0);
    final bLast = b.lastUsed ?? DateTime.fromMillisecondsSinceEpoch(0);
    return aLast.isAfter(bLast);
  }

  /// Human usage summary, e.g. "342 sessions · used today".
  String? get summary {
    if (sessionCount == 0) return null;
    final sessions = sessionCount == 1 ? '1 session' : '$sessionCount sessions';
    final last = lastUsed;
    if (last == null) return sessions;
    final now = DateTime.now();
    final today = DateTime(now.year, now.month, now.day);
    final lastDay = DateTime(last.year, last.month, last.day);
    final days = today.difference(lastDay).inDays;
    if (days == 0) return '$sessions · used today';
    if (days == 1) return '$sessions · used yesterday';
    if (days <= 60) return '$sessions · used $days days ago';
    return sessions;
  }

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is ToolUsageStats &&
          runtimeType == other.runtimeType &&
          sessionCount == other.sessionCount &&
          recentCount == other.recentCount;

  @override
  int get hashCode => Object.hash(sessionCount, recentCount);
}

/// Scan report for all tools.
final class ToolScanReport {
  const ToolScanReport({required this.statuses});

  final List<ToolInstallStatus> statuses;

  List<ToolInstallStatus> get installedStatuses =>
      statuses.where((s) => s.installed).toList();

  List<ToolInstallStatus> get missingStatuses =>
      statuses.where((s) => !s.installed).toList();

  bool get anyAIInstalled => installedStatuses.isNotEmpty;

  Set<QuickPresetTool> get installedQuickTools =>
      installedStatuses
          .map((s) => s.tool.quickPresetTool)
          .whereType<QuickPresetTool>()
          .toSet();

  ToolInstallStatus? statusFor(SetupTool tool) =>
      statuses.where((s) => s.tool == tool).firstOrNull;

  /// The installed CLI with the clearest usage lead.
  SetupTool? get mostUsedTool {
    final ranked = installedStatuses
        .where((s) => s.usage.sessionCount >= 3)
        .toList()
      ..sort((a, b) => ToolUsageStats.moreUsed(a.usage, b.usage) ? -1 : 1);
    return ranked.firstOrNull?.tool;
  }

  /// Installed CLIs ordered most-used first.
  List<SetupTool> get usageOrderedInstalledTools {
    final indexed = installedStatuses.asMap().entries.toList();
    indexed.sort((a, b) {
      if (a.value.usage != b.value.usage) {
        return ToolUsageStats.moreUsed(a.value.usage, b.value.usage) ? -1 : 1;
      }
      return a.key.compareTo(b.key);
    });
    return indexed.map((e) => e.value.tool).toList();
  }
}

/// Starred presets of one CLI, in flat-list order.
final class QuickPresetGroup {
  const QuickPresetGroup._({
    this.cli,
    this.app,
    required this.presets,
  });

  factory QuickPresetGroup.cli(SetupTool cli, List<Preset> presets) =>
      QuickPresetGroup._(cli: cli, presets: presets);

  factory QuickPresetGroup.app(RemoteAppSummary app, List<Preset> presets) =>
      QuickPresetGroup._(app: app, presets: presets);

  factory QuickPresetGroup.custom(List<Preset> presets) =>
      QuickPresetGroup._(presets: presets);

  final SetupTool? cli;
  final RemoteAppSummary? app;
  final List<Preset> presets;

  String get id => cli?.rawValue ?? app?.id ?? leader.id;
  String get displayName => cli?.displayName ?? app?.name ?? leader.label;
  Preset get leader => presets.first;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is QuickPresetGroup &&
          runtimeType == other.runtimeType &&
          id == other.id;

  @override
  int get hashCode => id.hashCode;
}

/// One quick-access chip per agent or App, with its command variants in order.
List<QuickPresetGroup> collectQuickPresetGroups(
  List<Preset> items, {
  List<RemoteAppSummary> apps = const [],
}) {
  final order = <String>[];
  final groups = <String, List<Preset>>{};
  final identities = <String, (SetupTool?, RemoteAppSummary?)>{};

  for (final preset in items.where((p) => p.enabled)) {
    final cli = SetupTool.detect(preset.command);
    final executable = preset.command.split(' ').firstOrNull ?? '';
    final trimmed = executable.replaceAll(RegExp('^[\'"]|[\'"]\$'), '');
    final head = trimmed.split('/').last;
    final app = apps.where((a) => a.command == head).firstOrNull;
    
    final id = cli?.rawValue ?? app?.id ?? 'custom:${preset.id}';
    if (!groups.containsKey(id)) {
      order.add(id);
      identities[id] = (cli, app);
    }
    groups.putIfAbsent(id, () => []).add(preset);
  }

  final result = <QuickPresetGroup>[];
  for (final id in order) {
    final identity = identities[id];
    final presets = groups[id];
    if (identity == null || presets == null) continue;
    if (!presets.any((p) => p.quickLaunch)) continue;
    
    final (cli, app) = identity;
    if (cli != null) {
      result.add(QuickPresetGroup.cli(cli, presets));
    } else if (app != null) {
      result.add(QuickPresetGroup.app(app, presets));
    } else {
      result.add(QuickPresetGroup.custom(presets));
    }
  }
  return result;
}

/// Entry of the global `presets` array in app-state.json.
final class GlobalPresetFile {
  const GlobalPresetFile({
    required this.id,
    required this.label,
    required this.command,
    this.projectID,
    this.enabled,
    this.quickLaunch,
  });

  final String id;
  final String label;
  final String command;
  final String? projectID;
  final bool? enabled;
  final bool? quickLaunch;

  factory GlobalPresetFile.fromJson(Map<String, dynamic> json) => GlobalPresetFile(
    id: json['id'] as String,
    label: json['label'] as String,
    command: json['command'] as String,
    projectID: json['project_id'] as String?,
    enabled: json['enabled'] as bool?,
    quickLaunch: json['quick_launch'] as bool?,
  );

  /// Converts to a Preset, filtering out project-scoped entries.
  Preset? toPreset() {
    if (projectID != null) return null; // Filter out legacy project presets
    return Preset(
      id: id,
      label: label,
      command: command,
      enabled: enabled ?? true,
      quickLaunch: (quickLaunch ?? false) && SetupTool.detect(command) != null,
    );
  }
}
