/// Pane "more" menu model.
///
/// Port of the menu *content* from `TerminalPaneView.swift`
/// (native/SupercliNative, `TerminalPaneContainer.paneMenu` and
/// `addPaneSessionItems`). The Swift side renders this through an AppKit
/// `NSMenu` (`PaneMoreMenuButton`/`PaneMoreMenuController` — native menu
/// plumbing, not portable); this module captures which rows exist, in what
/// order, under which conditions, and what each row does, so the gpuidart
/// menu can reproduce the same verbs.
///
/// Menu structure (session pane):
/// 1. "Copy transcript" submenu — only when `supportsTranscriptCopy`:
///    "Last 20 entries" (20), "Last 50 entries" (50),
///    "Whole conversation" (0, = all).
/// 2. "Copy session ID" — copies `Supercli Session ID: <id>`.
/// 3. Pin verbs — the panel's pin/un-pin verbs live HERE: the sidebar
///    group row is hidden from the desktop tree, so the pane menu is the
///    member's own surface. An auxiliary-region pane that is in the
///    project sidebar offers "Unpin from global project sidebar"; a
///    main-area pane that can move offers "Pin to global project sidebar".
/// 4. Session lifecycle: "Resume Agent" (canResumeAgent) else "Resume"
///    (canRestart); "Restart App" (canRestartApp); "Notify when done"
///    toggle (canNotifyWhenDone, checked when enabled); "Clear attention"
///    (status attention && canClearAttention).
/// 5. Archive/remove: "Stop and archive"/"Archive" (canArchive; live →
///    "Stop and archive"); "Remove session"/"Remove from list" (live →
///    "Remove session"). A non-archivable session removes directly (the
///    pane's counterpart to the sidebar X: nothing to archive, so remove
///    in one click even while live); archivable ones show the confirmation
///    card first.
/// 6. Group panes: separator, "Detach Pane", "Exit Multi-Pane View".
///
/// Launcher pane menu: "New Terminal", an "Agents" section (agent presets
/// then "Manage Agents…"), a "Plugins" section (plugin presets then
/// "Manage Plugins…").
library;

/// Transcript copy granularity for the "Copy transcript" submenu.
enum TranscriptCopyRange {
  /// Last 20 entries.
  last20,

  /// Last 50 entries.
  last50,

  /// The whole conversation (entries: 0).
  whole;

  /// The entry count passed to `copyTranscriptMarkdown` (0 = all).
  int get entryCount => switch (this) {
        TranscriptCopyRange.last20 => 20,
        TranscriptCopyRange.last50 => 50,
        TranscriptCopyRange.whole => 0,
      };

  /// Menu label.
  String get label => switch (this) {
        TranscriptCopyRange.last20 => 'Last 20 entries',
        TranscriptCopyRange.last50 => 'Last 50 entries',
        TranscriptCopyRange.whole => 'Whole conversation',
      };
}

/// The action a pane menu row dispatches. Payload-free; the UI layer maps
/// each to the store call with the pane's session id / preset.
enum PaneMenuAction {
  copyTranscriptLast20,
  copyTranscriptLast50,
  copyTranscriptWhole,
  copySessionId,
  unpinFromProjectSidebar,
  pinToProjectSidebar,
  resumeAgent,
  resume,
  restartApp,
  toggleNotifyWhenDone,
  clearAttention,
  stopAndArchive,
  archive,
  removeSessionDirect,
  removeSessionConfirm,
  launchNewTerminal,
  launchPreset,
  manageAgents,
  managePlugins,
  detachPane,
  exitMultiPaneView,
}

/// One row of the pane menu: a labeled action, a separator, a section
/// header, or the "Copy transcript" submenu container.
final class PaneMenuItem {
  const PaneMenuItem._({
    this.label,
    this.action,
    this.checked = false,
    this.submenu = const [],
    this.isSeparator = false,
    this.isSectionHeader = false,
  });

  /// A labeled action row.
  factory PaneMenuItem.action(String label, PaneMenuAction action,
          {bool checked = false}) =>
      PaneMenuItem._(label: label, action: action, checked: checked);

  /// The "Copy transcript" submenu row (holds the three ranges).
  factory PaneMenuItem.transcriptSubmenu() => const PaneMenuItem._(
        label: 'Copy transcript',
        submenu: [
          TranscriptCopyRange.last20,
          TranscriptCopyRange.last50,
          TranscriptCopyRange.whole,
        ],
      );

  /// A non-interactive section header ("Agents", "Plugins").
  factory PaneMenuItem.sectionHeader(String label) =>
      PaneMenuItem._(label: label, isSectionHeader: true);

  /// A separator row.
  factory PaneMenuItem.separator() => const PaneMenuItem._(isSeparator: true);

  final String? label;
  final PaneMenuAction? action;
  final bool checked;
  final List<TranscriptCopyRange> submenu;
  final bool isSeparator;
  final bool isSectionHeader;

  @override
  bool operator ==(Object other) =>
      other is PaneMenuItem &&
      other.label == label &&
      other.action == action &&
      other.checked == checked &&
      other.isSeparator == isSeparator &&
      other.isSectionHeader == isSectionHeader &&
      _listEquals(other.submenu, submenu);

  @override
  int get hashCode => Object.hash(
      label, action, checked, isSeparator, isSectionHeader, submenu.length);

  @override
  String toString() =>
      'PaneMenuItem(${isSeparator ? 'separator' : isSectionHeader ? 'header:$label' : '$label${checked ? ' [x]' : ''}'})';
}

bool _listEquals(List<TranscriptCopyRange> a, List<TranscriptCopyRange> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

/// Inputs for building a session pane's menu. Names mirror the Swift store
/// predicates read by `paneMenu`/`addPaneSessionItems`.
final class PaneSessionMenuInput {
  const PaneSessionMenuInput({
    required this.supportsTranscriptCopy,
    required this.isAuxiliaryRegion,
    required this.sessionIsInProjectSidebar,
    required this.canMoveSessionToProjectSidebar,
    required this.canResumeAgent,
    required this.canRestart,
    required this.canRestartApp,
    required this.canNotifyWhenDone,
    required this.notifyWhenDoneEnabled,
    required this.statusIsAttention,
    required this.canClearAttention,
    required this.canArchive,
    required this.isLive,
    required this.inGroup,
  });

  final bool supportsTranscriptCopy;
  final bool isAuxiliaryRegion;
  final bool sessionIsInProjectSidebar;
  final bool canMoveSessionToProjectSidebar;
  final bool canResumeAgent;
  final bool canRestart;
  final bool canRestartApp;
  final bool canNotifyWhenDone;
  final bool notifyWhenDoneEnabled;
  final bool statusIsAttention;
  final bool canClearAttention;
  final bool canArchive;
  final bool isLive;
  final bool inGroup;
}

/// Build the pane "more" menu model for a session pane.
///
/// Follows `paneMenu`/`addPaneSessionItems` exactly: transcript submenu,
/// copy session id, pin verbs, lifecycle verbs, archive/remove, then the
/// group verbs.
List<PaneMenuItem> buildPaneSessionMenu(PaneSessionMenuInput input) {
  final items = <PaneMenuItem>[];

  if (input.supportsTranscriptCopy) {
    items.add(PaneMenuItem.transcriptSubmenu());
  }
  items.add(PaneMenuItem.action(
      'Copy session ID', PaneMenuAction.copySessionId));

  if (input.isAuxiliaryRegion && input.sessionIsInProjectSidebar) {
    items.add(PaneMenuItem.separator());
    items.add(PaneMenuItem.action('Unpin from global project sidebar',
        PaneMenuAction.unpinFromProjectSidebar));
  } else if (!input.isAuxiliaryRegion &&
      input.canMoveSessionToProjectSidebar) {
    items.add(PaneMenuItem.separator());
    items.add(PaneMenuItem.action('Pin to global project sidebar',
        PaneMenuAction.pinToProjectSidebar));
  }

  items.add(PaneMenuItem.separator());
  if (input.canResumeAgent) {
    items
        .add(PaneMenuItem.action('Resume Agent', PaneMenuAction.resumeAgent));
  } else if (input.canRestart) {
    items.add(PaneMenuItem.action('Resume', PaneMenuAction.resume));
  }
  if (input.canRestartApp) {
    items.add(PaneMenuItem.action('Restart App', PaneMenuAction.restartApp));
  }
  if (input.canNotifyWhenDone) {
    items.add(PaneMenuItem.action('Notify when done',
        PaneMenuAction.toggleNotifyWhenDone,
        checked: input.notifyWhenDoneEnabled));
  }
  if (input.statusIsAttention && input.canClearAttention) {
    items.add(
        PaneMenuItem.action('Clear attention', PaneMenuAction.clearAttention));
  }

  items.add(PaneMenuItem.separator());
  if (input.canArchive) {
    items.add(PaneMenuItem.action(
        input.isLive ? 'Stop and archive' : 'Archive',
        input.isLive ? PaneMenuAction.stopAndArchive : PaneMenuAction.archive));
  }
  items.add(PaneMenuItem.action(
      input.isLive ? 'Remove session' : 'Remove from list',
      // Non-archivable sessions remove directly (nothing to archive);
      // archivable ones show the confirmation card first.
      input.canArchive
          ? PaneMenuAction.removeSessionConfirm
          : PaneMenuAction.removeSessionDirect));

  if (input.inGroup) {
    if (items.isNotEmpty) items.add(PaneMenuItem.separator());
    items.add(PaneMenuItem.action('Detach Pane', PaneMenuAction.detachPane));
    items.add(PaneMenuItem.action(
        'Exit Multi-Pane View', PaneMenuAction.exitMultiPaneView));
  }

  return items;
}

/// Build the pane "more" menu model for a launcher (empty) pane.
///
/// Mirrors the launcher branch of `paneMenu`: "New Terminal", then the
/// "Agents" section ([agentPresetLabels] then "Manage Agents…"), then the
/// "Plugins" section ([pluginPresetLabels] then "Manage Plugins…").
List<PaneMenuItem> buildPaneLauncherMenu({
  required List<String> agentPresetLabels,
  required List<String> pluginPresetLabels,
}) {
  final items = <PaneMenuItem>[
    PaneMenuItem.action('New Terminal', PaneMenuAction.launchNewTerminal),
    PaneMenuItem.separator(),
    PaneMenuItem.sectionHeader('Agents'),
    for (final label in agentPresetLabels)
      PaneMenuItem.action(label, PaneMenuAction.launchPreset),
    PaneMenuItem.action('Manage Agents…', PaneMenuAction.manageAgents),
    PaneMenuItem.separator(),
    PaneMenuItem.sectionHeader('Plugins'),
    for (final label in pluginPresetLabels)
      PaneMenuItem.action(label, PaneMenuAction.launchPreset),
    PaneMenuItem.action('Manage Plugins…', PaneMenuAction.managePlugins),
  ];
  return items;
}

/// The clipboard text for "Copy session ID".
///
/// Mirrors Swift: `"Supercli Session ID: \(entry.id)"`.
String copySessionIdText(String sessionId) => 'Supercli Session ID: $sessionId';
