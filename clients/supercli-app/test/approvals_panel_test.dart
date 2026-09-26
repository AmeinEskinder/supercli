/// Behavior tests for the approvals panel and toast notification center.
///
/// Covers: approval approve/deny decision flow via keyboard actions,
/// the approvals list panel (empty state, count, selection), the
/// notification queue (FIFO, bounded eviction, TTL auto-dismiss,
/// manual dismiss), and toast center rendering + click-to-focus.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/models.dart';
import 'package:supercli_app/screens/mcpapprovalpanel.dart';
import 'package:supercli_app/screens/toastcenter.dart';
import 'package:test/test.dart';

PendingApproval makeApproval({
  String id = 'appr-1',
  String tool = 'tool',
  String summary = 'E2E approval from the rendered dart client',
  String detail = 'The rendered dart app must answer this via keyboard.',
}) =>
    PendingApproval(
      id: id,
      tool: tool,
      summary: summary,
      detail: detail,
      callerSessionId: 'sess-1',
      requestedAtUnixMs: 1000,
    );

void main() {
  group('McpApprovalPanel', () {
    test('build renders attention dot, title, tool, summary, detail', () {
      final panel = McpApprovalPanel(approval: makeApproval());
      final node = panel.build() as UiColumn;
      expect(node.id, 'mcp-approval-overlay');

      // Header row: dot + title.
      final header = node.children[0] as UiRow;
      expect((header.children[0] as UiText).text, '●');
      expect((header.children[1] as UiText).text, 'Approval requested');

      // Tool line, summary, detail.
      expect((node.children[1] as UiText).text, 'Tool: tool');
      expect((node.children[2] as UiText).text, contains('E2E approval'));
      expect((node.children[3] as UiText).text, contains('keyboard'));
    });

    test('more-waiting count renders when > 0, hidden when 0', () {
      final withMore =
          McpApprovalPanel(approval: makeApproval(), moreWaiting: 2);
      final node = withMore.build() as UiColumn;
      final more = node.children
          .whereType<UiText>()
          .where((t) => t.id == 'mcp-approval-more');
      expect(more.single.text, '2 more waiting');

      final withoutMore = McpApprovalPanel(approval: makeApproval());
      final node2 = withoutMore.build() as UiColumn;
      expect(
        node2.children.whereType<UiText>().where((t) => t.id == 'mcp-approval-more'),
        isEmpty,
      );
    });

    test('buttons: Allow (Ctrl+Enter), Don\'t Allow, Edit', () {
      final panel = McpApprovalPanel(approval: makeApproval());
      final node = panel.build() as UiColumn;
      final buttons = node.children.last as UiRow;
      final labels =
          buttons.children.whereType<UiButton>().map((b) => b.label).toList();
      expect(labels, contains('Allow (Ctrl+Enter)'));
      expect(labels, contains("Don't Allow"));
      expect(labels, contains('Edit (Ctrl+E)'));
    });

    test('approve action fires the decision callback', () {
      ApprovalDecision? got;
      final panel = McpApprovalPanel(
        approval: makeApproval(),
        onDecision: (d) => got = d,
      );
      expect(panel.handleAction('mcp.approve'), isTrue);
      expect(got, ApprovalDecision.approve);
    });

    test('deny action fires the decision callback', () {
      ApprovalDecision? got;
      final panel = McpApprovalPanel(
        approval: makeApproval(),
        onDecision: (d) => got = d,
      );
      expect(panel.handleAction('mcp.deny'), isTrue);
      expect(got, ApprovalDecision.deny);
    });

    test('edit action is consumed without a decision', () {
      var called = false;
      final panel = McpApprovalPanel(
        approval: makeApproval(),
        onDecision: (_) => called = true,
      );
      expect(panel.handleAction('mcp.edit'), isTrue);
      expect(called, false);
    });

    test('unknown action is not consumed', () {
      final panel = McpApprovalPanel(approval: makeApproval());
      expect(panel.handleAction('bogus.action'), false);
    });

    test('actions declare ctrl+enter / ctrl+shift+enter / ctrl+e', () {
      final panel = McpApprovalPanel(approval: makeApproval());
      final actions = panel.actions();
      final byName = {for (final a in actions) a.name: a};
      expect(byName['mcp.approve']!.keys, 'ctrl+enter');
      expect(byName['mcp.deny']!.keys, 'ctrl+shift+enter');
      expect(byName['mcp.edit']!.keys, 'ctrl+e');
      // All scoped to the overlay node.
      for (final a in actions) {
        expect(a.context, isA<UiNodeActionContext>());
        expect(
          (a.context as UiNodeActionContext).id,
          'mcp-approval-overlay',
        );
      }
    });
  });

  group('ApprovalsPanel', () {
    test('empty state', () {
      final panel = const ApprovalsPanel();
      final node = panel.build() as UiColumn;
      expect((node.children.single as UiText).text, 'No pending approvals.');
    });

    test('count line and one row per approval', () {
      final panel = ApprovalsPanel(approvals: [
        makeApproval(id: 'a1', tool: 'write'),
        makeApproval(id: 'a2', tool: 'browser'),
      ]);
      final node = panel.build() as UiColumn;
      expect((node.children[0] as UiText).text, '2 pending approvals');
      final rows =
          node.children.where((c) => c.id.startsWith('approval-row-'));
      expect(rows.length, 2);
    });

    test('selected approval renders the overlay below the list', () {
      final panel = ApprovalsPanel(
        approvals: [makeApproval(id: 'a1'), makeApproval(id: 'a2')],
        selectedId: 'a2',
      );
      final node = panel.build() as UiColumn;
      final overlays =
          node.children.where((c) => c.id == 'mcp-approval-overlay');
      expect(overlays.length, 1);
    });

    test('no overlay when nothing is selected', () {
      final panel = ApprovalsPanel(approvals: [makeApproval(id: 'a1')]);
      final node = panel.build() as UiColumn;
      expect(
        node.children.where((c) => c.id == 'mcp-approval-overlay'),
        isEmpty,
      );
    });
  });

  group('NotificationQueue', () {
    AppNotification note(String id, {NotificationSeverity s = NotificationSeverity.info}) =>
        AppNotification(
          id: id,
          title: 't',
          message: 'm',
          severity: s,
          createdAtUnixMs: 1000,
          ttlMs: 5000,
        );

    test('FIFO order', () {
      final q = NotificationQueue();
      q.add(note('a'));
      q.add(note('b'));
      expect(q.items.map((n) => n.id), ['a', 'b']);
    });

    test('add replaces same-id notification', () {
      final q = NotificationQueue();
      q.add(note('a'));
      q.add(note('a'));
      expect(q.length, 1);
    });

    test('bounded: oldest info evicted first', () {
      final q = NotificationQueue(maxSize: 2);
      q.add(note('info1'));
      q.add(note('warn', s: NotificationSeverity.warning));
      q.add(note('info2'));
      // info1 evicted; warn kept.
      expect(q.items.map((n) => n.id), ['warn', 'info2']);
    });

    test('dismiss removes by id', () {
      final q = NotificationQueue();
      q.add(note('a'));
      q.add(note('b'));
      expect(q.dismiss('a'), isTrue);
      expect(q.items.map((n) => n.id), ['b']);
      expect(q.dismiss('missing'), false);
    });

    test('pruneExpired removes TTL-expired notifications', () {
      final q = NotificationQueue();
      q.add(note('short'));
      q.add(const AppNotification(
        id: 'long',
        title: 't',
        message: 'm',
        createdAtUnixMs: 1000,
        ttlMs: 60000,
      ));
      // now = 1000 + 5000 -> 'short' expired (ttl 5000), 'long' alive.
      expect(q.pruneExpired(6000), 1);
      expect(q.items.map((n) => n.id), ['long']);
    });

    test('ttl 0 never expires', () {
      const n = AppNotification(
        id: 'x',
        title: 't',
        message: 'm',
        createdAtUnixMs: 0,
        ttlMs: 0,
      );
      expect(n.isExpired(999999999), false);
    });

    test('clear empties the queue', () {
      final q = NotificationQueue();
      q.add(note('a'));
      q.clear();
      expect(q.isEmpty, isTrue);
    });
  });

  group('ToastCenter', () {
    test('renders one row per toast with dot, body, dismiss', () {
      const center = ToastCenter(toasts: [
        Toast(id: 't1', message: 'device connected'),
      ]);
      final node = center.build() as UiColumn;
      expect(node.id, 'toast-center');
      final row = node.children.single as UiRow;
      expect(row.id, 'toast-t1');
      expect((row.children[0] as UiText).text, '○'); // info dot
      expect((row.children[1] as UiColumn).children.last, isA<UiText>());
      expect((row.children[2] as UiButton).label, '✕');
    });

    test('severity dots: warning ●, error ●!', () {
      final q = NotificationQueue();
      q.add(const AppNotification(
        id: 'w',
        title: '',
        message: 'approval needed',
        severity: NotificationSeverity.warning,
      ));
      q.add(const AppNotification(
        id: 'e',
        title: '',
        message: 'failed',
        severity: NotificationSeverity.error,
      ));
      final center = ToastCenter(queue: q);
      final node = center.build() as UiColumn;
      final dots = node.children
          .whereType<UiRow>()
          .map((r) => (r.children[0] as UiText).text)
          .toList();
      expect(dots, ['●', '●!']);
    });

    test('queue drives rendering when provided', () {
      final q = NotificationQueue();
      q.add(const AppNotification(
        id: 'q1',
        title: 'Approval requested',
        message: 'tool wants to write',
        focusTarget: 'mcp-approval-overlay',
      ));
      final center = ToastCenter(queue: q);
      final node = center.build() as UiColumn;
      expect(node.children.length, 1);
      final row = node.children.single as UiRow;
      final body = row.children[1] as UiColumn;
      expect((body.children[0] as UiText).text, 'Approval requested');
    });

    test('dismiss action removes from the queue', () {
      final q = NotificationQueue();
      q.add(const AppNotification(id: 'd1', title: '', message: 'm'));
      final center = ToastCenter(queue: q);
      expect(center.handleAction('toast.dismiss', 'd1'), isTrue);
      expect(q.isEmpty, isTrue);
    });

    test('focus action fires the onFocus callback', () {
      String? focused;
      final center = ToastCenter(onFocus: (id) => focused = id);
      expect(center.handleAction('toast.focus', 't9'), isTrue);
      expect(focused, 't9');
    });

    test('unknown action is not consumed', () {
      const center = ToastCenter();
      expect(center.handleAction('bogus', 't1'), false);
    });
  });
}
