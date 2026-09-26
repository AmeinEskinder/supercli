/// Tests for the mounted app shell (worker h).
///
/// Verifies SidebarView, TerminalArea/PaneLayout, McpApprovalPanel, and
/// ToastCenter are constructed in [SupercliApp.build] and fed by Host data.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/app.dart';
import 'package:supercli_app/models.dart';
import 'package:supercli_app/notifications.dart';
import 'package:test/test.dart';

SessionSummary _session(String id, String title) => SessionSummary(
      id: id,
      title: title,
      updatedAt: DateTime(2026, 9, 26, 12, 0),
    );

PendingApproval _approval(String id) => PendingApproval(
      id: id,
      tool: 'device_tap',
      summary: 'Tap at (100, 200)',
      detail: 'x=100 y=200',
    );

/// Collect all node ids in a UiNode tree via its JSON form.
Set<String> _ids(UiNode node) {
  final ids = <String>{};
  void walk(Map<String, Object?> json) {
    final id = json['id'];
    if (id is String) ids.add(id);
    final children = json['children'];
    if (children is List) {
      for (final c in children) {
        if (c is Map<String, Object?>) walk(c);
      }
    }
  }

  walk(node.toJson());
  return ids;
}

void main() {
  group('SupercliApp shell', () {
    test('build mounts sidebar, terminal area, approval panel, toasts', () {
      final app = SupercliApp()
        ..sessions = [_session('s1', 'alpha'), _session('s2', 'beta')]
        ..pendingApprovals = [_approval('a1'), _approval('a2')];
      final ids = _ids(app.build());

      // Sidebar mounted (SidebarView root node).
      expect(ids.any((id) => id.startsWith('sidebar')), isTrue,
          reason: 'SidebarView not mounted; got: $ids');
      // Terminal area mounted (TerminalArea root node).
      expect(ids.contains('terminal-area-root'), isTrue);
      // Approval panel mounted (McpApprovalPanel overlay).
      expect(ids.contains('mcp-approval-overlay'), isTrue);
      expect(ids.contains('mcp-allow'), isTrue);
      expect(ids.contains('mcp-deny'), isTrue);
      // "1 more waiting" for the second approval.
      expect(ids.contains('mcp-approval-more'), isTrue);
      // Toast center mounted.
      expect(ids.contains('toast-center'), isTrue);
      // Composer present.
      expect(ids.contains('composer'), isTrue);
    });

    test('no approval renders the empty state, not the overlay', () {
      final app = SupercliApp()..sessions = [_session('s1', 'alpha')];
      final ids = _ids(app.build());
      expect(ids.contains('mcp-approval-overlay'), isFalse);
      expect(ids.contains('no-approval'), isTrue);
    });

    test('sidebar shows Host-fed session titles', () {
      final app = SupercliApp()
        ..sessions = [_session('s1', 'alpha'), _session('s2', 'beta')];
      final ids = _ids(app.build());
      // Sidebar session rows carry the session id in their node id.
      expect(ids.any((id) => id.contains('s1')), isTrue);
      expect(ids.any((id) => id.contains('s2')), isTrue);
    });

    test('toasts render from the notification queue', () {
      final app = SupercliApp()
        ..notifications.add(const AppNotification(
          id: 'n1',
          title: 'Approval requested',
          message: 'device_tap: Tap at (100, 200)',
          severity: NotificationSeverity.warning,
        ));
      final ids = _ids(app.build());
      expect(ids.contains('toast-n1'), isTrue);
      expect(ids.contains('toast-title-n1'), isTrue);
    });

    test('actions include approval, pane, and sidebar bindings', () {
      final app = SupercliApp();
      final names = app.actions().map((a) => a.name).toSet();
      expect(names.contains('mcp.approve'), isTrue);
      expect(names.contains('mcp.deny'), isTrue);
      expect(names.contains('sidebar.toggle'), isTrue);
    });

    test('pendingApproval getter returns the first approval', () {
      final app = SupercliApp()..pendingApprovals = [_approval('a1')];
      expect(app.pendingApproval?.id, 'a1');
      app.pendingApprovals = [];
      expect(app.pendingApproval, isNull);
    });
  });
}
