/// supercli desktop app shell: sidebar + terminal panes + approvals + toasts.
///
/// Mounts the real DESKTOP components in the running app:
/// - [SidebarView]: project/session tree, fed by Host bootstrap sessions
/// - [TerminalArea]: hosts the [PaneLayout] split-pane tree (worker e)
/// - [McpApprovalPanel]: approval overlay for pending approvals (worker c)
/// - [ToastCenter]: transient notifications from Host events (worker c)
///
/// All data comes from the Host via [HostClient]; no fixtures.
library;

import 'package:gpuidart/gpuidart.dart';

import 'keybindings.dart';
import 'keymap.dart';
import 'models.dart';
import 'screens/commandpaletteview.dart';
import 'screens/mcpapprovalpanel.dart';
import 'screens/projectsidebarview.dart';
import 'screens/settings_controller.dart';
import 'screens/settingsview.dart';
import 'screens/settingsshell.dart';
import 'screens/sidebarview.dart';
import 'screens/terminalarea.dart';
import 'screens/toastcenter.dart';

/// Builds the supercli desktop UI shell.
final class SupercliApp {
  SupercliApp();

  List<SessionSummary> sessions = const [];

  /// Projects from the Host bootstrap (`projects` array). Used for the
  /// sidebar's project tree headers (real names, not IDs).
  List<HostProject> projects = const [];
  List<PendingApproval> pendingApprovals = const [];
  String composerText = '';
  int selectedSession = 0;
  String statusLine = 'Connecting…';
  bool sidebarCollapsed = false;

  /// Transient notifications from Host events (approvals, errors, …).
  final NotificationQueue notifications = NotificationQueue();

  /// The pane layout for the active session. Rebuilt on refresh from the
  /// selected session; splits persist for the app lifetime.
  PaneLayout? paneLayout;

  /// Live terminal views keyed by pane id. A view is created per pane when
  /// a Host session is selected and streams that session's PTY output via
  /// the authenticated Host API ([SessionOutputStream]); the terminal area
  /// renders the registered view instead of the placeholder.
  final Map<String, TerminalPaneView> paneViews = {};

  /// Command palette state. Non-null while the palette is open; built from
  /// the live action registry + live sessions via [paletteCommands].
  CommandPaletteState? paletteState;

  /// True while the command palette overlay is visible.
  bool get paletteOpen => paletteState != null;

  /// MRU session switcher for Ctrl-Tab. Entries are synced from the live
  /// session list via [syncMru]; order reflects real usage.
  final MruSwitcher mruSwitcher = MruSwitcher();

  /// True while the MRU switcher overlay is visible.
  bool switcherOpen = false;

  /// Active detached session drag, or null when idle. Mounted in [build]
  /// as the drag overlay; commits go through the authenticated Host API
  /// via [SidebarSessionDrag.commitDrop] — never local-only state.
  ///
  /// Drag *initiation* needs native pointer DnD events that do not exist
  /// in gpuidart upstream (docs/gpuidart-gaps-sidebar.md G-1); the state
  /// machine, overlay, and Host commit path below are the wired half.
  SidebarSessionDrag? sessionDrag;

  /// Project the active drag belongs to (needed to commit the hand order).
  String? dragProjectId;

  /// Live hand order of the dragged project's sessions, captured at drag
  /// start so the commit recomputes against a stable base.
  List<String> dragCurrentOrder = const [];

  /// True while a session drag overlay is mounted.
  bool get dragActive => sessionDrag != null;

  /// Begin a session drag: records the dragged session, its project, and
  /// the project's current hand order. Pure app-state — unit-testable.
  void beginSessionDrag({
    required String sessionId,
    required String projectId,
    required List<String> currentOrder,
  }) {
    sessionDrag = SidebarSessionDrag(draggedSessionId: sessionId);
    dragProjectId = projectId;
    dragCurrentOrder = List.unmodifiable(currentOrder);
  }

  /// Update the drop target mid-drag (hover changes).
  void retargetSessionDrag({
    required SidebarDropTarget target,
    String? targetId,
    int? targetIndex,
    bool dropToSplit = false,
  }) {
    final drag = sessionDrag;
    if (drag == null) return;
    sessionDrag = drag.copyWith(
      target: target,
      targetId: targetId,
      targetIndex: targetIndex,
      dropToSplit: dropToSplit,
    );
  }

  /// Cancel the drag and clear its captured context.
  void cancelSessionDrag() {
    sessionDrag = null;
    dragProjectId = null;
    dragCurrentOrder = const [];
  }

  /// Settings persistence controller. Set by the app entry point on startup
  /// (after the Host handshake); null until then. Owns the [AppSettings]
  /// model and syncs it with the Host via GET/POST /mobile/workspace-settings.
  SettingsController? settingsController;

  /// True while the settings overlay is visible.
  bool settingsOpen = false;

  /// The currently selected settings tab.
  SettingsTab activeSettingsTab = SettingsTab.appearance;

  /// Open the settings overlay. No-op until [settingsController] is set.
  void openSettings() {
    if (settingsController != null) {
      settingsOpen = true;
    }
  }

  /// Close the settings overlay.
  void closeSettings() {
    settingsOpen = false;
  }

  /// Handle a UI action by name. Returns true if the action was consumed.
  ///
  /// The app entry point routes `host.events` action events here (the same
  /// pattern as [McpApprovalPanel.handleAction]).
  bool handleAction(String actionName) {
    switch (actionName) {
      case 'settings.open':
        openSettings();
        return true;
      case 'settings.close':
        closeSettings();
        return true;
    }
    if (actionName.startsWith('settings.tab.')) {
      final tabName = actionName.substring('settings.tab.'.length);
      final tab = SettingsTab.compatibleRawValue(tabName);
      if (tab != null) {
        activeSettingsTab = tab;
        return true;
      }
    }
    return false;
  }

  /// Handle a click on a node by id. Returns true if consumed.
  ///
  /// Tab buttons are rendered with ids `settings-tab-<rawValue>` (see
  /// [SettingsNavRow]); clicking one switches the active tab. Deep-link
  /// spellings resolve through [SettingsTab.compatibleRawValue].
  /// Toggle buttons render as `<toggle-id>-toggle` (see [SettingsToggle]);
  /// clicking one flips the setting and persists via the controller.
  bool handleClick(String nodeId) {
    // Menu path to Settings: the sidebar's 'open-settings' button.
    if (nodeId == 'open-settings') {
      openSettings();
      return settingsController != null;
    }
    const tabPrefix = 'settings-tab-';
    if (nodeId.startsWith(tabPrefix)) {
      final tabName = nodeId.substring(tabPrefix.length);
      final tab = SettingsTab.compatibleRawValue(tabName);
      if (tab != null) {
        activeSettingsTab = tab;
        return true;
      }
    }
    const toggleSuffix = '-toggle';
    if (nodeId.endsWith(toggleSuffix)) {
      return toggleSetting(
        nodeId.substring(0, nodeId.length - toggleSuffix.length),
      );
    }
    return false;
  }

  /// Flip the [AppSettings] field bound to [toggleId] and schedule a
  /// debounced Host save via the settings controller. Returns true if the
  /// toggle id is known. Toggle ids are defined by [SettingsToggle] usages
  /// in settingspanels.dart / sessionsaccesssections.dart /
  /// worktreessettingspanel.dart.
  bool toggleSetting(String toggleId) {
    final settings = settingsController?.settings;
    if (settings == null) return false;
    switch (toggleId) {
      case 'worktree-access':
      case 'agent-worktree-permission':
        settings.worktreeAccess = !settings.worktreeAccess;
      case 'auto-gallery':
      case 'agent-auto-gallery':
        settings.autoGallery = !settings.autoGallery;
      case 'sessions-mcp':
        settings.sessionsMcp = !settings.sessionsMcp;
      case 'feat-remote-ws':
        settings.remoteWorkspaces = !settings.remoteWorkspaces;
      case 'feat-git-worktrees':
        settings.gitWorktrees = !settings.gitWorktrees;
      case 'feat-browser-mcp':
      case 'browser-mcp':
        settings.browserMcp = !settings.browserMcp;
      case 'feat-auto-screenshots':
      case 'browser-auto-screenshots':
        settings.autoAddBrowserScreenshots =
            !settings.autoAddBrowserScreenshots;
      case 'transcript-content':
        settings.transcriptContentEnabled = !settings.transcriptContentEnabled;
      case 'notify-completion':
        settings.notifyOnCompletion = !settings.notifyOnCompletion;
      case 'notify-flags':
        settings.notifyFlags = !settings.notifyFlags;
      case 'adv-show-worktrees':
      case 'show-agent-worktrees':
        settings.showAgentWorktrees = !settings.showAgentWorktrees;
      case 'adv-trace-log':
        settings.traceLog = !settings.traceLog;
      default:
        return false;
    }
    settingsController?.edited();
    return true;
  }

  /// Open the palette, building its command list from the real app state:
  /// every registered action plus every live session. No fixtures.
  void openPalette() {
    paletteState = CommandPaletteState(commands: paletteCommands());
  }

  /// Close the palette and discard its filter state.
  void closePalette() {
    paletteState = null;
  }

  /// The real command registry: app actions (with their registered
  /// shortcuts) plus the live sessions. This is what the palette lists,
  /// filters, and executes — not a hardcoded list.
  List<PaletteCommand> paletteCommands() {
    final actionDefs = [
      // (action name, human title, shortcut)
      // NOTE: shortcuts come from Keymap (lib/keymap.dart), the single
      // source of truth. The primary modifier is `meta` (Cmd) on macOS,
      // `ctrl` on Linux/Windows. Platform-neutral chords (ctrl+tab, ...)
      // stay as `ctrl+` on all platforms by design. The approval overlay
      // uses plain Return / Escape (MCPApprovalPanel.swift:249-266).
      ('approval.approve', 'Approve pending request', Keymap.approvalAllow),
      ('approval.deny', 'Deny pending request', Keymap.approvalDeny),
      ('mcp.approve', 'Approve pending MCP request', Keymap.approvalAllow),
      ('mcp.deny', 'Deny pending MCP request', Keymap.approvalDeny),
      (
        'mcp.edit',
        'Edit pending MCP request before answering',
        Keymap.editDetail(),
      ),
      ('sidebar.toggle', 'Toggle sidebar', Keymap.sidebarToggle()),
      ('sessions.up', 'Select previous session', 'up'),
      ('sessions.down', 'Select next session', 'down'),
      ('composer.focus', 'Focus message composer', Keymap.composerFocus()),
      ('pane.splitRight', 'Split pane right', Keymap.splitRight()),
      ('pane.splitDown', 'Split pane down', Keymap.splitDown()),
      ('pane.zoom', 'Zoom focused pane', Keymap.zoomPane()),
      ('pane.equalize', 'Equalize pane sizes', Keymap.equalizeSplits()),
      ('pane.close', 'Close focused pane', Keymap.closeWindow()),
      ('pane.detach', 'Detach focused pane', Keymap.detachPane()),
      // NOTE: no shortcut is claimed for pane.focusNext/focusPrev: Ctrl-Tab
      // is the MRU switcher's chord (see switcher.next), so labeling these
      // with it would be misleading. They are reachable from the palette.
      ('pane.focusNext', 'Focus next pane', ''),
      ('pane.focusPrev', 'Focus previous pane', ''),
      ('find.show', 'Find in terminal', Keymap.find()),
      ('switcher.next', 'Switch to next recent session', Keymap.switcherNext),
      (
        'switcher.previous',
        'Switch to previous recent session',
        Keymap.switcherPrevious,
      ),
      // Settings: the canonical Cmd-,/Ctrl-, chord (Keymap.settings) is
      // shown for documentation, but it is NOT registered natively —
      // gpuidart's native key parser rejects punctuation keys (see
      // docs/gpuidart-gaps-keys.md). The palette dispatches the action
      // name directly through handleAction, so no native chord is needed.
      ('settings.open', 'Open settings', Keymap.settings()),
    ];
    final commands = <PaletteCommand>[
      for (final (name, title, shortcut) in actionDefs)
        PaletteCommand(
          id: 'action:$name',
          title: title,
          shortcut: shortcut,
          kind: PaletteCommandKind.command,
        ),
      for (final s in sessions)
        PaletteCommand(
          id: 'session:${s.id}',
          title: s.title,
          subtitle: 'Switch to session',
          kind: PaletteCommandKind.session,
        ),
    ];
    return commands;
  }

  /// Sync the MRU switcher entries with the live session list: add new
  /// sessions (most-recent first for brand-new ones), drop gone ones, keep
  /// the established MRU order for the rest, and refresh titles in place
  /// (a renamed session keeps its MRU position but shows the new title).
  void syncMru() {
    final liveIds = sessions.map((s) => s.id).toSet();
    for (final e in List.of(mruSwitcher.entries)) {
      if (!liveIds.contains(e.id)) mruSwitcher.remove(e.id);
    }
    // Iterate in reverse: add() inserts at the front, so this preserves the
    // session-list order (most-recent-first from the Host) for new entries.
    for (final s in sessions.reversed) {
      if (!mruSwitcher.entries.any((e) => e.id == s.id)) {
        mruSwitcher.add(MruEntry(id: s.id, title: s.title));
      } else {
        mruSwitcher.update(MruEntry(id: s.id, title: s.title));
      }
    }
  }

  /// Execute a palette selection against live app state.
  ///
  /// Session entries switch to that session (updating [selectedSession]
  /// and recording MRU use) and return null. Command entries return the
  /// action name for the host to dispatch. Unknown ids are ignored and
  /// return null. Pure app-state logic — unit-testable without the host.
  String? executePaletteCommand(PaletteCommand cmd) {
    if (cmd.kind == PaletteCommandKind.session &&
        cmd.id.startsWith('session:')) {
      final sessionId = cmd.id.substring('session:'.length);
      final idx = sessions.indexWhere((s) => s.id == sessionId);
      if (idx >= 0) {
        selectedSession = idx;
        mruSwitcher.markUsed(sessionId);
      }
      return null;
    }
    if (cmd.id.startsWith('action:')) {
      return cmd.id.substring('action:'.length);
    }
    return null;
  }

  /// Dataset backing the session list table. Cached: gpuidart tracks dataset
  /// ownership by instance, so the same object must be reused across
  /// `GpuiHost.open` and `replaceDataset` calls.
  TableDataset? _sessionDataset;
  TableDataset get sessionDataset {
    final cached = _sessionDataset;
    if (cached != null) return cached;
    final created = TableDataset(
      'sessions',
      columns: const ['Session', 'Details', 'Updated'],
      rows: sessions
          .map((s) => [s.displayTitle, s.subtitle, formatTime(s.updatedAt)])
          .toList(),
    );
    _sessionDataset = created;
    return created;
  }

  /// Sidebar sessions derived from the Host bootstrap.
  ///
  /// Status signals (attention dot, busy spinner, pin) come from the
  /// Host's `activity`/`pinned` fields via [SidebarSession.fromHostSummary];
  /// no client-side guessing.
  List<SidebarSession> get _sidebarSessions => [
    for (final s in sessions) SidebarSession.fromHostSummary(s),
  ];

  /// Group sessions by project for the sidebar tree. Project headers use
  /// the real names from the Host bootstrap `projects` array; sessions
  /// whose project is unknown land in a single "Sessions" project
  /// (see [SidebarSession.fromHostSummary]).
  List<SidebarProject> get _sidebarProjects {
    final byProject = <String, List<SidebarSession>>{};
    for (final s in _sidebarSessions) {
      byProject.putIfAbsent(s.projectId, () => []).add(s);
    }
    final names = {for (final p in projects) p.id: p.name};
    return [
      for (final entry in byProject.entries)
        SidebarProject(
          id: entry.key,
          name: names[entry.key] ?? entry.key,
          sessions: entry.value,
        ),
    ];
  }

  PendingApproval? get pendingApproval =>
      pendingApprovals.isEmpty ? null : pendingApprovals.first;

  /// The full app shell. Rebuilt on every state change via host.rebuild().
  ///
  /// Layout: sidebar | content column (terminal panes + approval overlay +
  /// toasts). This is the mounted DESKTOP shell — not a scaffold.
  UiNode build() {
    final approval = pendingApproval;
    final layout =
        paneLayout ??
        PaneLayout.single(
          paneId: 'pane-1',
          title: sessions.isEmpty
              ? 'zsh'
              : sessions[selectedSession.clamp(0, sessions.length - 1)].title,
        );
    return UiRow('app-shell', [
      if (!sidebarCollapsed)
        SidebarView(
          workspaces: const ['local'],
          activeWorkspaceId: 'local',
          projects: _sidebarProjects,
          selectedSessionId: sessions.isEmpty
              ? null
              : sessions[selectedSession.clamp(0, sessions.length - 1)].id,
        ).build()
      else
        UiColumn('sidebar-collapsed', [const UiButton('expand-sidebar', '+')]),
      UiColumn('content-area', [
        TerminalArea(
          layout: layout,
          statusText: statusLine,
          views: paneViews,
        ).build(),
        if (approval != null)
          McpApprovalPanel(
            approval: approval,
            moreWaiting: pendingApprovals.length - 1,
          ).build()
        else
          const UiText('no-approval', 'No pending approvals.'),
        ToastCenter(queue: notifications).build(),
        const UiInput(
          'composer',
          placeholder: 'Type a message… (Enter to send)',
        ),
      ]),
      // Command palette overlay (Cmd-K): mounted when open, fed by the
      // live action registry + live sessions.
      if (paletteOpen)
        CommandPaletteView(
          commands: paletteState!.commands,
          filter: paletteState!.filter,
          selectedIndex: paletteState!.selectedIndex,
        ).build(),
      // MRU session switcher overlay (Ctrl-Tab): live MRU ordering.
      if (switcherOpen) MruSwitcherView(switcher: mruSwitcher).build(),
      // Detached session-drag overlay: mounted while a drag is active.
      // Commits go through the authenticated Host API (see
      // SidebarSessionDrag.commitDrop); initiation awaits gpuidart DnD
      // events (docs/gpuidart-gaps-sidebar.md G-1).
      if (dragActive) sessionDrag!.build(),
      // Settings overlay (Cmd-,): mounted when open, fed by the live
      // SettingsController (Host-backed settings via GET/POST
      // /mobile/workspace-settings; desktop-only fields persist locally).
      if (settingsOpen && settingsController != null)
        SettingsView(
          settings: settingsController!.settings,
          activeTab: activeSettingsTab,
        ).build(),
    ]);
  }

  /// Keyboard bindings: component actions + app-level navigation.
  List<UiAction> actions() {
    final approval = pendingApproval;
    return [
      // Approval overlay actions — only when the panel is actually mounted
      // (gpuidart rejects action contexts that aren't nodes in the tree).
      if (approval != null) ...McpApprovalPanel(approval: approval).actions(),
      // Pane management (mounted PaneLayout).
      ...(paneLayout ?? PaneLayout.single(paneId: 'pane-1', title: 'zsh'))
          .actions(),
      // Live terminal key bindings (terminal.key.* scoped to each mounted
      // terminal node; dispatched to the Host write route).
      for (final view in paneViews.values) ...view.actions(),
      // Sidebar toggle (platform primary modifier: meta/Cmd on macOS,
      // ctrl on Linux/Windows).
      UiAction(name: 'sidebar.toggle', keys: Keymap.sidebarToggle()),
      // Session list navigation (scoped to the sidebar node, which is
      // the rendered UiColumn('sidebar'); the old 'session-list' scope
      // matched no node and was dead).
      const UiAction(
        name: 'sessions.up',
        keys: 'up',
        context: UiActionContext.node('sidebar'),
      ),
      const UiAction(
        name: 'sessions.down',
        keys: 'down',
        context: UiActionContext.node('sidebar'),
      ),
      // Focus the composer.
      UiAction(name: 'composer.focus', keys: Keymap.composerFocus()),
      // Command palette (Cmd-K) + MRU switcher (Ctrl-Tab): global chords.
      ...AppKeybindings().globalActions(),
      // Palette-scoped navigation while the overlay is open.
      if (paletteOpen)
        ...const AppKeybindings().paletteActions('command-palette'),
      // Switcher-scoped dismiss while the overlay is open.
      if (switcherOpen)
        ...const AppKeybindings().switcherActions('mru-switcher'),
      // Session-drag overlay actions — only while a drag is mounted
      // (gpuidart rejects action contexts that aren't nodes in the tree).
      if (dragActive) ...sessionDrag!.actions(),
      // Settings overlay: gpuidart's native key parser rejects punctuation
      // keys (see docs/gpuidart-gaps-keys.md), so the canonical Cmd-,/Ctrl-,
      // chord (Keymap.settings) is NOT registered natively — no UiAction
      // for settings.open. Settings stays reachable from the command
      // palette ("Open settings"), which dispatches the action name
      // directly through handleAction without native registration.
      // Escape closes the settings overlay — scoped to the 'settings'
      // node so it is only active while the overlay is actually mounted
      // (gpuidart rejects action contexts that aren't nodes in the tree).
      if (settingsOpen)
        const UiAction(
          name: 'settings.close',
          keys: 'escape',
          context: UiActionContext.node('settings'),
        ),
    ];
  }

  static String formatTime(DateTime t) {
    final now = DateTime.now();
    final diff = now.difference(t);
    if (diff.inMinutes < 1) return 'just now';
    if (diff.inHours < 1) return '${diff.inMinutes}m ago';
    if (diff.inDays < 1) return '${diff.inHours}h ago';
    return '${diff.inDays}d ago';
  }
}
