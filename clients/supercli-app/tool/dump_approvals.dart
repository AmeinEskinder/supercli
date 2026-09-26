/// Builds a sample approvals panel + toast center and dumps JSON snapshots.
library;

import 'dart:convert';
import 'dart:io';

import 'package:supercli_app/models.dart';

import 'package:supercli_app/screens/mcpapprovalpanel.dart';
import 'package:supercli_app/screens/toastcenter.dart';

void main() {
  final approvals = [
    const PendingApproval(
      id: 'appr-1',
      tool: 'write',
      summary: 'Write to /home/user/notes/todo.md',
      detail:
          'The agent wants to write 42 lines to todo.md in the notes workspace.',
      callerSessionId: 'sess-abc',
      requestedAtUnixMs: 1727220000000,
    ),
    const PendingApproval(
      id: 'appr-2',
      tool: 'browser',
      summary: 'Open https://example.com/docs in the browser',
      detail: 'The agent wants to open an external URL.',
      callerSessionId: 'sess-abc',
      requestedAtUnixMs: 1727220005000,
    ),
  ];

  final panel = ApprovalsPanel(approvals: approvals, selectedId: 'appr-1');
  final overlay = McpApprovalPanel(
    approval: approvals[0],
    moreWaiting: 1,
  );

  final queue = NotificationQueue();
  queue.add(const AppNotification(
    id: 'n1',
    title: 'Device connected',
    message: 'MacBook Pro paired via relay',
    severity: NotificationSeverity.info,
    createdAtUnixMs: 1727220010000,
  ));
  queue.add(const AppNotification(
    id: 'n2',
    title: 'Approval requested',
    message: 'write wants to modify todo.md',
    severity: NotificationSeverity.warning,
    createdAtUnixMs: 1727220015000,
    focusTarget: 'mcp-approval-overlay',
  ));
  final toasts = ToastCenter(queue: queue);

  final out = {
    'approvalsPanel': panel.build().toJson(),
    'overlay': overlay.build().toJson(),
    'toastCenter': toasts.build().toJson(),
  };
  final file = File('/tmp/approvals-snapshot.json');
  file.writeAsStringSync(const JsonEncoder.withIndent('  ').convert(out));
  print('wrote ${file.path}');
}
