/// Dump the mounted app shell UiNode tree as JSON.
///
/// Proves the shell (SidebarView + TerminalArea/PaneLayout + McpApprovalPanel
/// + ToastCenter) is constructed from real Host bootstrap data. This is the
/// tree that GpuiHost.open() would render in a real window — the native
/// window itself is blocked on missing system libs (libxkbcommon-x11.so.0).
library;

import 'dart:convert';
import 'dart:io';

import 'package:supercli_app/app.dart';
import 'package:supercli_app/host_client.dart';
import 'package:supercli_app/notifications.dart';

Future<void> main(List<String> args) async {
  final host = args.isNotEmpty ? args[0] : '127.0.0.1';
  final port = args.length > 1 ? int.parse(args[1]) : 8137;
  final out = args.length > 2 ? args[2] : 'shell-dump.json';

  final client = HostClient(baseUrl: Uri.parse('http://$host:$port'));
  final app = SupercliApp();

  try {
    final boot = await client.bootstrap();
    app.sessions = HostClient.sessionsFromBootstrap(boot);
    app.pendingApprovals = HostClient.approvalsFromBootstrap(boot);
    app.statusLine =
        'Connected — ${app.sessions.length} sessions, ${app.pendingApprovals.length} pending approval(s).';
    // Simulate an approval toast like the live event loop would.
    for (final a in app.pendingApprovals) {
      app.notifications.add(AppNotification(
        id: 'approval-${a.id}',
        title: 'Approval requested',
        message: '${a.tool}: ${a.summary}',
        severity: NotificationSeverity.warning,
      ));
    }
  } catch (e) {
    app.statusLine = 'Host error: $e';
  }

  final tree = app.build();
  final json = const JsonEncoder.withIndent('  ').convert(tree.toJson());
  File(out).writeAsStringSync(json);
  print('shell tree dumped to $out '
      '(${app.sessions.length} sessions, '
      '${app.pendingApprovals.length} approvals, '
      '${app.notifications.length} toasts)');
  client.close();
}
