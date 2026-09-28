/// Entry point for the supercli desktop app (macOS/Linux).
///
/// Wires the gpuidart UI to the supercli Host:
/// - Bootstrap: fetch sessions + pending approvals on startup.
/// - Long-poll: poll Host events, rebuild UI on changes.
/// - Approvals: show card, answer via HostClient (idempotent).
/// - Keyboard: UiAction events drive approve/deny/list navigation.
///
/// Usage: dart run --host=127.0.0.1 --port=8137 [--token=...] [--tls] [--insecure]
///
/// The real Host serves /mobile over TLS with a self-signed Host certificate
/// and requires a paired-device `Bearer` token. `--tls` switches to https;
/// `--insecure` accepts the self-signed Host certificate (e2e only — a
/// production client pins the fingerprint from pairing).
library;

import 'dart:async';
import 'dart:io';

import 'package:gpuidart/gpuidart.dart';
import 'package:http/io_client.dart';
import 'package:supercli_app/app.dart';
import 'package:supercli_app/host_client.dart';
import 'package:supercli_app/models.dart';
import 'package:supercli_app/notifications.dart';
import 'package:supercli_app/pane_layout.dart';

Future<void> main(List<String> args) async {
  final host = _parseArg(args, '--host=') ?? '127.0.0.1';
  final port = int.tryParse(_parseArg(args, '--port=') ?? '8137') ?? 8137;
  final token = _parseArg(args, '--token=');
  final useTls = args.contains('--tls');
  final insecure = args.contains('--insecure');
  final scheme = useTls ? 'https' : 'http';

  IOClient? httpClientFor() {
    if (!insecure) return null;
    final io = HttpClient()
      ..badCertificateCallback = (cert, host, port) => true;
    return IOClient(io);
  }

  final client = HostClient(
    baseUrl: Uri.parse('$scheme://$host:$port'),
    httpClient: httpClientFor(),
    token: token,
  );
  final app = SupercliApp();

  // Allow headless smoke runs (no native window) for CI.
  final headless = args.contains('--headless');

  GpuiHost? gpui;
  if (!headless) {
    gpui = await GpuiHost.open(
      app.build(),
      datasets: [app.sessionDataset],
      actions: app.actions(),
      window: const GpuiWindowOptions(title: 'supercli'),
    );
  }

  Future<void> refresh() async {
    try {
      // Real Host route: GET /mobile/bootstrap carries sessions and
      // pendingApprovals. (GET /mobile/sessions and /mobile/approvals do
      // not exist on the Host.)
      final boot = await client.bootstrap();
      final sessions = HostClient.sessionsFromBootstrap(boot);
      final approvals = HostClient.approvalsFromBootstrap(boot);
      final prevApprovalIds = app.pendingApprovals.map((a) => a.id).toSet();
      app.sessions = sessions;
      app.pendingApprovals = approvals;
      // Keep the Ctrl-Tab MRU ordering in sync with the live sessions.
      app.syncMru();
      // Toast on newly arrived approvals (drives the ToastCenter).
      for (final a in approvals) {
        if (!prevApprovalIds.contains(a.id)) {
          app.notifications.add(
            AppNotification(
              id: 'approval-${a.id}',
              title: 'Approval requested',
              message: '${a.tool}: ${a.summary}',
              severity: NotificationSeverity.warning,
              focusTarget: 'mcp-approval-overlay',
            ),
          );
        }
      }
      app.statusLine =
          'Connected — ${sessions.length} sessions, ${approvals.length} pending approval(s).';
    } on HostException catch (e) {
      app.statusLine = 'Host error: $e';
      app.notifications.add(
        AppNotification(
          id: 'host-error',
          title: 'Host error',
          message: '$e',
          severity: NotificationSeverity.error,
        ),
      );
    } catch (e) {
      // Non-HostException failures (connection refused, timeout, TLS, JSON)
      // must not become an uncaught 255; record and continue headless.
      app.statusLine = 'Connection error: $e';
      app.notifications.add(
        AppNotification(
          id: 'connection-error',
          title: 'Connection error',
          message: '$e',
          severity: NotificationSeverity.error,
        ),
      );
      stderr.writeln('headless: bootstrap failed: $e');
    }
    final host = gpui;
    if (host != null) {
      final dataset = app.sessionDataset;
      // Rebuild rows from the fresh sessions; the dataset instance is
      // cached (see app.sessionDataset) so ownership checks pass.
      await host.replaceDataset(
        dataset,
        columns: const ['Title', 'Updated'],
        rows: [
          for (final s in app.sessions)
            [s.title, SupercliApp.formatTime(s.updatedAt)],
        ],
      );
      await host.publish(app.build(), actions: app.actions());
    }
  }

  Future<void> handleEvent(GpuiEvent event) async {
    switch (event.type) {
      case 'action':
        await _handleAction(event, app, client, refresh);
      case 'click':
        await _handleClick(event, app, client, refresh);
      case 'input':
        if (event.id == 'composer') {
          app.composerText = event.value ?? '';
        } else if (event.id == 'command-palette-filter') {
          // Live filter text from the palette input; re-render the overlay.
          app.paletteState?.setFilter(event.value ?? '');
          await refresh();
        }
      case 'table_selection':
        final sel = event.tableSelection;
        if (sel != null && sel.row != null) {
          app.selectedSession = sel.row!;
          await refresh();
        }
      case 'error':
        stderr.writeln('gpuidart error: ${event.data}');
    }
  }

  if (gpui != null) {
    final sub = gpui.events.listen((event) {
      unawaited(handleEvent(event).catchError((Object e) => stderr.writeln(e)));
    });
    await refresh();
    // Long-poll loop: refresh on Host events.
    unawaited(_pollLoop(client, refresh));
    await gpui.done;
    await sub.cancel();
  } else {
    // Headless: one refresh + approve the first pending approval, then exit.
    // Used for the scripted e2e proof.
    await refresh();
    final approval = app.pendingApproval;
    if (approval != null) {
      final sent = await client.answerApproval(
        ApprovalAnswer.approve(approval.id),
      );
      stdout.writeln('headless: answered approval ${approval.id} (sent=$sent)');
    } else {
      stdout.writeln('headless: no pending approvals');
    }
    stdout.writeln('headless: sessions=${app.sessions.length}');
  }
  client.close();
}

Future<void> _handleAction(
  GpuiEvent event,
  SupercliApp app,
  HostClient client,
  Future<void> Function() refresh,
) async {
  final action = event.data['name'] as String?;
  if (action != null) {
    await _dispatchAction(action, app, client, refresh);
  }
}

/// Dispatch a named action. Palette command execution routes through here,
/// so "execute" always means "run the real handler".
Future<void> _dispatchAction(
  String action,
  SupercliApp app,
  HostClient client,
  Future<void> Function() refresh,
) async {
  switch (action) {
    case 'approval.approve':
    case 'mcp.approve':
      final approval = app.pendingApproval;
      if (approval != null) {
        await client.answerApproval(ApprovalAnswer.approve(approval.id));
        await refresh();
      }
    case 'approval.deny':
    case 'mcp.deny':
      final approval = app.pendingApproval;
      if (approval != null) {
        await client.answerApproval(ApprovalAnswer.deny(approval.id));
        await refresh();
      }
    case 'mcp.edit':
      // Detail view is rendered inline in the panel; nothing to do beyond
      // keeping the overlay visible.
      await refresh();
    case 'sessions.up':
      if (app.selectedSession > 0) {
        app.selectedSession--;
        await refresh();
      }
    case 'sessions.down':
      if (app.selectedSession < app.sessions.length - 1) {
        app.selectedSession++;
        await refresh();
      }
    case 'composer.focus':
      // GAP: gpuidart has no programmatic focus API yet. Logged as P0 gap.
      stderr.writeln('gap: programmatic focus not available in gpuidart');
    // --- Command palette (Cmd-K) ---
    case 'palette.open':
      app.openPalette();
      await refresh();
    case 'palette.up':
      app.paletteState?.moveUp();
      await refresh();
    case 'palette.down':
      app.paletteState?.moveDown();
      await refresh();
    case 'palette.confirm':
      final cmd = app.paletteState?.confirm();
      app.closePalette();
      if (cmd != null) {
        // Session entries are executed against app state here; command
        // entries return the action name for the real dispatcher below.
        final actionName = app.executePaletteCommand(cmd);
        if (actionName != null) {
          await _dispatchAction(actionName, app, client, refresh);
        } else {
          await refresh();
        }
      } else {
        await refresh();
      }
    case 'palette.dismiss':
      app.closePalette();
      await refresh();
    // --- MRU switcher (Ctrl-Tab) ---
    case 'switcher.next':
      app.switcherOpen = true;
      app.mruSwitcher.next();
      await refresh();
    case 'switcher.previous':
      app.switcherOpen = true;
      app.mruSwitcher.previous();
      await refresh();
    case 'switcher.dismiss':
      app.switcherOpen = false;
      app.mruSwitcher.resetSelection();
      await refresh();
    case 'switcher.confirm':
      // Enter on the Ctrl-Tab overlay: switch to the highlighted entry.
      final current = app.mruSwitcher.current;
      app.switcherOpen = false;
      if (current != null) {
        final idx = app.sessions.indexWhere((s) => s.id == current.id);
        if (idx >= 0) app.selectedSession = idx;
        app.mruSwitcher.markUsed(current.id);
      }
      await refresh();
    // --- Session drag (detached drag, #152) ---
    // The drag state machine + overlay live in the app; the commit goes
    // through the authenticated Host API (never local-only state).
    // Initiation awaits gpuidart DnD events (G-1).
    case 'sidebar.drag.cancel':
      app.cancelSessionDrag();
      await refresh();
    case 'sidebar.drag.commit':
      final drag = app.sessionDrag;
      final projectId = app.dragProjectId;
      if (drag != null && projectId != null) {
        try {
          await drag.commitDrop(
            host: client,
            projectId: projectId,
            currentOrder: app.dragCurrentOrder,
          );
          app.cancelSessionDrag();
        } on HostException catch (e) {
          app.cancelSessionDrag();
          app.notifications.add(
            AppNotification(
              id: 'drag-error',
              title: 'Drag failed',
              message: e.message,
              severity: NotificationSeverity.error,
            ),
          );
        }
      }
      await refresh();
    // --- Pane management (also reachable from the palette) ---
    case 'pane.splitRight':
      _splitPane(app, SplitDirection.horizontal);
      await refresh();
    case 'pane.splitDown':
      _splitPane(app, SplitDirection.vertical);
      await refresh();
    case 'pane.zoom':
      final layout = app.paneLayout;
      if (layout != null) {
        app.paneLayout = layout.isZoomed
            ? layout.unzoom()
            : layout.toggleZoom();
      }
      await refresh();
    case 'pane.equalize':
      final layout = app.paneLayout;
      if (layout != null) app.paneLayout = layout.equalize();
      await refresh();
    case 'pane.close':
      final layout = app.paneLayout;
      if (layout != null) {
        final closed = layout.closePane();
        if (closed != null) app.paneLayout = closed;
      }
      await refresh();
    case 'pane.detach':
      // Detach is a window-manager op with no headless equivalent yet.
      stderr.writeln('gap: pane.detach has no implementation yet');
      await refresh();
    case 'pane.focusNext':
      _focusPane(app, forward: true);
      await refresh();
    case 'pane.focusPrev':
      _focusPane(app, forward: false);
      await refresh();
    case 'pane.focusLeft':
    case 'pane.focusRight':
    case 'pane.focusUp':
    case 'pane.focusDown':
      final layout = app.paneLayout;
      if (layout != null) {
        final dir = switch (action) {
          'pane.focusLeft' => FocusDirection.left,
          'pane.focusRight' => FocusDirection.right,
          'pane.focusUp' => FocusDirection.up,
          _ => FocusDirection.down,
        };
        app.paneLayout = layout.focusDirection(dir);
      }
      await refresh();
    case 'find.show':
      // Find UI is not mounted in this shell yet.
      stderr.writeln('gap: find.show has no mounted UI yet');
      await refresh();
    case 'sidebar.toggle':
      app.sidebarCollapsed = !app.sidebarCollapsed;
      await refresh();
  }
}

void _splitPane(SupercliApp app, SplitDirection direction) {
  final layout = app.paneLayout;
  if (layout == null) return;
  final split = layout.split(
    direction: direction,
    newPaneId: 'pane-${DateTime.now().millisecondsSinceEpoch}',
    newTitle: 'zsh',
  );
  if (split != null) app.paneLayout = split;
}

void _focusPane(SupercliApp app, {required bool forward}) {
  final layout = app.paneLayout;
  if (layout == null) return;
  app.paneLayout = layout.focusNext(reverse: !forward);
}

Future<void> _handleClick(
  GpuiEvent event,
  SupercliApp app,
  HostClient client,
  Future<void> Function() refresh,
) async {
  final approval = app.pendingApproval;
  if (approval == null) return;
  if (event.id == 'mcp-allow' || event.id == 'approve') {
    await client.answerApproval(ApprovalAnswer.approve(approval.id));
    await refresh();
  } else if (event.id == 'mcp-deny' || event.id == 'deny') {
    await client.answerApproval(ApprovalAnswer.deny(approval.id));
    await refresh();
  }
}

/// Long-poll the Host for events; refresh the UI when something changes.
Future<void> _pollLoop(
  HostClient client,
  Future<void> Function() refresh,
) async {
  while (true) {
    try {
      final event = await client.pollEvents();
      if (event['type'] != 'timeout') {
        await refresh();
      }
    } on HostException {
      await Future<void>.delayed(const Duration(seconds: 5));
    }
  }
}

String? _parseArg(List<String> args, String prefix) {
  for (final arg in args) {
    if (arg.startsWith(prefix)) return arg.substring(prefix.length);
  }
  return null;
}
