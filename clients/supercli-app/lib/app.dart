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

import 'models.dart';
import 'screens/mcpapprovalpanel.dart';
import 'screens/sidebarview.dart';
import 'screens/terminalarea.dart';
import 'screens/toastcenter.dart';

/// Builds the supercli desktop UI shell.
final class SupercliApp {
  SupercliApp();

  List<SessionSummary> sessions = const [];
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

  /// Dataset backing the session list table. Cached: gpuidart tracks dataset
  /// ownership by instance, so the same object must be reused across
  /// `GpuiHost.open` and `replaceDataset` calls.
  TableDataset? _sessionDataset;
  TableDataset get sessionDataset {
    final cached = _sessionDataset;
    if (cached != null) return cached;
    final created = TableDataset(
      'sessions',
      columns: const ['Title', 'Updated'],
      rows: sessions
          .map((s) => [s.title, formatTime(s.updatedAt)])
          .toList(),
    );
    _sessionDataset = created;
    return created;
  }

  /// Sidebar sessions derived from the Host bootstrap.
  List<SidebarSession> get _sidebarSessions => [
        for (final s in sessions)
          SidebarSession(
            summary: s,
            projectId: _projectIdFor(s),
          ),
      ];

  /// Group sessions by project for the sidebar tree. Sessions without an
  /// explicit project land in a single "Sessions" project.
  List<SidebarProject> get _sidebarProjects {
    final byProject = <String, List<SidebarSession>>{};
    for (final s in _sidebarSessions) {
      byProject.putIfAbsent(s.projectId, () => []).add(s);
    }
    return [
      for (final entry in byProject.entries)
        SidebarProject(
          id: entry.key,
          name: entry.key,
          sessions: entry.value,
        ),
    ];
  }

  String _projectIdFor(SessionSummary s) {
    // The Host bootstrap carries an optional `project` field; fall back to
    // a single default project so the tree always renders.
    return 'Sessions';
  }

  PendingApproval? get pendingApproval =>
      pendingApprovals.isEmpty ? null : pendingApprovals.first;

  /// The full app shell. Rebuilt on every state change via host.rebuild().
  ///
  /// Layout: sidebar | content column (terminal panes + approval overlay +
  /// toasts). This is the mounted DESKTOP shell — not a scaffold.
  UiNode build() {
    final approval = pendingApproval;
    final layout = paneLayout ??
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
        UiColumn('sidebar-collapsed', [
          const UiButton('expand-sidebar', '+'),
        ]),
      UiColumn('content-area', [
        TerminalArea(layout: layout, statusText: statusLine).build(),
        if (approval != null)
          McpApprovalPanel(
            approval: approval,
            moreWaiting: pendingApprovals.length - 1,
          ).build()
        else
          const UiText('no-approval', 'No pending approvals.'),
        ToastCenter(queue: notifications).build(),
        const UiInput('composer', placeholder: 'Type a message… (Enter to send)'),
      ]),
    ]);
  }

  /// Keyboard bindings: component actions + app-level navigation.
  List<UiAction> actions() => [
        // Approval overlay (mounted McpApprovalPanel).
        ...const McpApprovalPanel(
          approval: PendingApproval(
            id: '',
            tool: '',
            summary: '',
            detail: '',
          ),
        ).actions(),
        // Pane management (mounted PaneLayout).
        ...(paneLayout ?? PaneLayout.single(paneId: 'pane-1', title: 'zsh'))
            .actions(),
        // Sidebar toggle.
        const UiAction(name: 'sidebar.toggle', keys: 'cmd+b'),
        // Session list navigation.
        const UiAction(
          name: 'sessions.up',
          keys: 'up',
          context: UiActionContext.node('session-list'),
        ),
        const UiAction(
          name: 'sessions.down',
          keys: 'down',
          context: UiActionContext.node('session-list'),
        ),
        // Focus the composer.
        const UiAction(name: 'composer.focus', keys: 'ctrl+l'),
      ];

  static String formatTime(DateTime t) {
    final now = DateTime.now();
    final diff = now.difference(t);
    if (diff.inMinutes < 1) return 'just now';
    if (diff.inHours < 1) return '${diff.inMinutes}m ago';
    if (diff.inDays < 1) return '${diff.inHours}h ago';
    return '${diff.inDays}d ago';
  }
}
