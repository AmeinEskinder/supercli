/// Entry point for the supercli desktop app (macOS/Linux).
///
/// Wires the gpuidart UI to the supercli Host:
/// - Bootstrap: fetch sessions + pending approvals on startup.
/// - Long-poll: poll Host events, rebuild UI on changes.
/// - Approvals: show card, answer via HostClient (idempotent).
/// - Keyboard: UiAction events drive approve/deny/list navigation.
///
/// Usage: dart run --host=127.0.0.1 --port=8137
library;

import 'dart:async';
import 'dart:io';

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/app.dart';
import 'package:supercli_app/host_client.dart';
import 'package:supercli_app/models.dart';

Future<void> main(List<String> args) async {
  final host = _parseArg(args, '--host=') ?? '127.0.0.1';
  final port = int.tryParse(_parseArg(args, '--port=') ?? '8137') ?? 8137;
  final client = HostClient(baseUrl: Uri.parse('http://$host:$port'));
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
      final sessions = await client.listSessions();
      final approvals = await client.listApprovals();
      app.sessions = sessions;
      app.pendingApproval = approvals.isEmpty ? null : approvals.first;
      app.statusLine =
          'Connected — ${sessions.length} sessions, ${approvals.length} pending approval(s).';
    } on HostException catch (e) {
      app.statusLine = 'Host error: $e';
    }
    final host = gpui;
    if (host != null) {
      final dataset = app.sessionDataset;
      await host.replaceDataset(
        dataset,
        columns: dataset.columns,
        rows: [
          for (var i = 0; i < dataset.rowCount; i++)
            [for (var j = 0; j < dataset.columns.length; j++) dataset.cell(i, j)],
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
      final sent = await client.answerApproval(ApprovalAnswer.approve(approval.id));
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
  switch (action) {
    case 'approval.approve':
      final approval = app.pendingApproval;
      if (approval != null) {
        await client.answerApproval(ApprovalAnswer.approve(approval.id));
        await refresh();
      }
    case 'approval.deny':
      final approval = app.pendingApproval;
      if (approval != null) {
        await client.answerApproval(ApprovalAnswer.deny(approval.id));
        await refresh();
      }
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
  }
}

Future<void> _handleClick(
  GpuiEvent event,
  SupercliApp app,
  HostClient client,
  Future<void> Function() refresh,
) async {
  final approval = app.pendingApproval;
  if (approval == null) return;
  if (event.id == 'approve') {
    await client.answerApproval(ApprovalAnswer.approve(approval.id));
    await refresh();
  } else if (event.id == 'deny') {
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
