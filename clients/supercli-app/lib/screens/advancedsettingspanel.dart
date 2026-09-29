/// Local Advanced settings panel.
///
/// Port of `AdvancedSettingsPanel` (SettingsView.swift, 4430-4740) with the
/// pure display helpers `formatMB`/`formatCpu`/`compactPath` and the data
/// shapes `MemorySnapshot` (4741-4746) and `RunningTerminal` (4747-4762).
///
/// `AdvancedDiagnostics.collect()` shells out and reads processes and the
/// filesystem in Swift. The Dart port does NOT perform that collection:
/// the panel accepts an injected [AdvancedDiagnosticsSnapshot] (populated
/// by the app through `supercli-client-ffi`/the Host) and renders it. Only
/// the display/pure formatting logic is ported here.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingsprimitives.dart';
import 'settingspanels.dart' as base;

/// Auto-stop/archive minute options for the local cleanup picker
/// (Swift: `SupercliStore.autoStopArchiveMinuteOptions`).
const List<int> autoStopArchiveMinuteOptions = [0, 30, 60, 120, 240, 480, 1440];

/// Stopped-sidebar limit options for the local cleanup picker
/// (Swift: `SupercliStore.sidebarStoppedLimitOptions`).
const List<int> sidebarStoppedLimitOptions = [0, 3, 5, 10, 15, 25];

/// Label for an auto-stop/archive minute option
/// (Swift: `SupercliStore.autoStopArchiveLabel(for:)`).
String autoStopArchiveLabel(int minutes) {
  if (minutes == 0) return 'Never';
  if (minutes < 60) return 'After $minutes minutes';
  if (minutes == 60) return 'After 1 hour';
  if (minutes == 1440) return 'After 1 day';
  return 'After ${minutes ~/ 60} hours';
}

/// Label for a stopped-sidebar limit option
/// (Swift: `SupercliStore.sidebarStoppedLimitLabel(for:)`).
String sidebarStoppedLimitLabel(int limit) => limit == 0 ? 'None' : '$limit';

/// "42 MB" (Swift: `AdvancedSettingsPanel.formatMB`).
String formatMB(int bytes) => '${(bytes / (1024 * 1024)).round()} MB';

/// "4.2%" / "42%" (Swift: `AdvancedSettingsPanel.formatCpu`).
String formatCpu(double value) =>
    value >= 10 ? '${value.toStringAsFixed(0)}%' : '${value.toStringAsFixed(1)}%';

/// ".../last/two" for long paths; "No folder" for empty
/// (Swift: `AdvancedSettingsPanel.compactPath`).
String compactPath(String path) {
  if (path.isEmpty) return 'No folder';
  final parts = path.split('/').where((p) => p.isNotEmpty).toList();
  if (parts.length <= 2) return path;
  return '.../${parts.sublist(parts.length - 2).join('/')}';
}

/// Process memory snapshot (Swift: `MemorySnapshot`).
final class MemorySnapshot {
  const MemorySnapshot({
    required this.processFootprintBytes,
    required this.runningHostCount,
    required this.hostedSessionCount,
  });

  final int processFootprintBytes;
  final int runningHostCount;
  final int hostedSessionCount;
}

/// A running terminal host row (Swift: `RunningTerminal`).
final class RunningTerminal {
  const RunningTerminal({
    required this.id,
    required this.projectID,
    required this.label,
    required this.command,
    required this.cwd,
    required this.pid,
    required this.processCount,
    required this.cpuPercent,
    required this.rssBytes,
    this.canArchive = false,
    this.isRemoving = false,
  });

  final String id;
  final String projectID;
  final String label;
  final String command;
  final String cwd;
  final int pid;
  final int processCount;
  final double cpuPercent;
  final int rssBytes;
  final bool canArchive;
  final bool isRemoving;

  /// "Blank shell" when the command is empty (Swift: `commandLabel`).
  String get commandLabel =>
      command.trim().isEmpty ? 'Blank shell' : command;
}

/// Diagnostics data (Swift: `AdvancedDiagnostics.Snapshot`).
///
/// Collection (`AdvancedDiagnostics.collect()`) is Host/FFI work — the app
/// injects a snapshot; this file only renders it.
final class AdvancedDiagnosticsSnapshot {
  const AdvancedDiagnosticsSnapshot({
    this.memory,
    this.terminals = const [],
    this.sessionsFolder = '',
    this.traceLogExists = false,
  });

  final MemorySnapshot? memory;
  final List<RunningTerminal> terminals;
  final String sessionsFolder;
  final bool traceLogExists;
}

/// Local Advanced settings panel (Swift: `AdvancedSettingsPanel`).
final class AdvancedSettingsPanel {
  const AdvancedSettingsPanel({
    this.autoStopArchiveMinutes = 60,
    this.sidebarStoppedLimit = 10,
    this.snapshot = const AdvancedDiagnosticsSnapshot(),
    this.loading = false,
  });

  final int autoStopArchiveMinutes;
  final int sidebarStoppedLimit;
  final AdvancedDiagnosticsSnapshot snapshot;
  final bool loading;

  double get _totalCpu =>
      snapshot.terminals.fold(0.0, (sum, t) => sum + t.cpuPercent);

  int get _totalRss =>
      snapshot.terminals.fold(0, (sum, t) => sum + t.rssBytes);

  /// "N running · X% CPU · Y MB memory. Sorted by current CPU usage."
  /// (Swift: `AdvancedSettingsPanel.summaryText`).
  String get summaryText {
    if (snapshot.terminals.isEmpty) {
      return 'Live terminal hosts sorted by current CPU usage.';
    }
    return '${snapshot.terminals.length} running · ${formatCpu(_totalCpu)} CPU · '
        '${formatMB(_totalRss)} memory. Sorted by current CPU usage.';
  }

  UiNode _cleanupSection() {
    return UiColumn('advanced-cleanup', [
      const SettingsSectionHeader(
        title: 'Cleanup',
        description: 'Sessions that have stayed idle for the selected time are '
            'stopped and archived — the same as clicking "Stop and archive": the '
            'terminal stops and the session files away into the project\'s archive '
            'library, where Restore & Resume continues the conversation. Sessions '
            'that keep working (including loops — any activity resets the clock), '
            'or that are pinned, selected, unread, or waiting for input, are left '
            'alone; plain shell terminals are never touched. Nothing is deleted '
            'automatically. Sessions that stop or die on their own are never '
            'archived automatically. Choose how many stopped or archived sessions '
            'each project previews; older rows are hidden from the sidebar only.',
      ).build(),
      UiColumn('advanced-cleanup-auto-stop-labeled', [
        base.SettingsSelect(
          id: 'advanced-auto-stop',
          label: 'Auto-stop and archive inactive terminals',
          selected: autoStopArchiveLabel(autoStopArchiveMinutes),
          options: [
            for (final m in autoStopArchiveMinuteOptions)
              autoStopArchiveLabel(m),
          ],
        ).fallback(),
      ]),
      UiColumn('advanced-cleanup-limit-labeled', [
        base.SettingsSelect(
          id: 'advanced-sidebar-limit',
          label: 'Stopped or archived sessions shown in sidebar',
          selected: sidebarStoppedLimitLabel(sidebarStoppedLimit),
          options: [
            for (final l in sidebarStoppedLimitOptions)
              sidebarStoppedLimitLabel(l),
          ],
        ).fallback(),
      ]),
    ]);
  }

  UiNode _memorySection() {
    final memory = snapshot.memory;
    return UiColumn('advanced-memory', [
      const SettingsSectionHeader(
        title: 'Memory',
        description: 'Current process memory usage and active session counts.',
      ).build(),
      if (memory != null) ...[
        SettingsValueRow(
          label: 'App memory (Supercli Native)',
          value: formatMB(memory.processFootprintBytes),
        ).build(),
        SettingsValueRow(
          label: 'Running terminal hosts',
          value: '${memory.runningHostCount}',
        ).build(),
        SettingsValueRow(
          label: 'Hosted sessions on disk',
          value: '${memory.hostedSessionCount}',
        ).build(),
      ] else
        UiText('advanced-memory-state',
            loading ? 'Loading…' : 'Unable to read memory usage'),
    ]);
  }

  UiNode _terminalRow(RunningTerminal terminal) {
    return UiRow('advanced-terminal-${terminal.id}', [
      UiColumn('advanced-terminal-info-${terminal.id}', [
        UiText('advanced-terminal-label-${terminal.id}', terminal.label),
        UiText('advanced-terminal-sub-${terminal.id}',
            '${terminal.commandLabel} • PID ${terminal.pid} • '
            '${terminal.processCount} proc • ${compactPath(terminal.cwd)}'),
      ]),
      UiColumn('advanced-terminal-cpu-${terminal.id}', [
        UiText('advanced-terminal-cpu-value-${terminal.id}',
            formatCpu(terminal.cpuPercent)),
        const UiText('advanced-terminal-cpu-label', 'CPU'),
      ]),
      UiColumn('advanced-terminal-mem-${terminal.id}', [
        UiText('advanced-terminal-mem-value-${terminal.id}',
            formatMB(terminal.rssBytes)),
        const UiText('advanced-terminal-mem-label', 'Memory'),
      ]),
      UiButton('advanced-terminal-open-${terminal.id}', 'Open'),
      UiButton(
          'advanced-terminal-archive-${terminal.id}',
          terminal.canArchive ? 'Stop and archive' : 'Remove'),
    ]);
  }

  UiNode _terminalsSection() {
    return UiColumn('advanced-terminals', [
      UiRow('advanced-terminals-header', [
        const SettingsSectionHeader(title: 'Running Terminals').build(),
        UiButton('advanced-terminals-refresh',
            loading ? 'Refreshing…' : 'Refresh'),
      ]),
      UiText('advanced-terminals-summary', summaryText),
      if (snapshot.terminals.isEmpty)
        UiText('advanced-terminals-empty',
            loading ? 'Loading terminals…' : 'No running terminals.')
      else
        UiColumn('advanced-terminals-rows', [
          for (final t in snapshot.terminals) _terminalRow(t),
        ]),
    ]);
  }

  UiNode _diagnosticsSection() {
    return UiColumn('advanced-diagnostics', [
      const SettingsSectionHeader(
        title: 'Diagnostics',
        description: 'Quick access to Supercli\'s on-disk session data and hook trace log.',
      ).build(),
      UiRow('advanced-diagnostics-sessions', [
        const UiText(
            'advanced-diagnostics-sessions-label', 'Sessions folder'),
        const UiButton(
            'advanced-diagnostics-sessions-open', 'Show in Finder'),
      ]),
      UiRow('advanced-diagnostics-trace', [
        const UiText(
            'advanced-diagnostics-trace-label', 'Hooks trace log'),
        const UiButton('advanced-diagnostics-trace-open', 'Show in Finder'),
      ]),
    ]);
  }

  UiNode build() {
    return UiColumn('advanced-settings', [
      const SettingsPaneHeader(
        title: 'Advanced',
        description: 'Resource usage, cleanup, and on-disk data for Supercli\'s terminal hosts.',
      ).build(),
      _cleanupSection(),
      _memorySection(),
      _terminalsSection(),
      _diagnosticsSection(),
    ]);
  }
}
