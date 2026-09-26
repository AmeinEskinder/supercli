/// supercli desktop app UI built on gpuidart.
///
/// Port order: macOS/Linux desktop first. Uses existing gpuidart primitives;
/// gaps are logged in docs/gpuidart-requirements.md (see P0 items).
///
/// Current primitive mapping:
/// - Session list: UiTable with a registered TableDataset (P0-2 UiList pending)
/// - Composer: UiInput single-line (P0-3 multiline pending)
/// - Approval card: UiRow of UiText + UiButtons (P0-1 approval card pending)
/// - Keyboard: real UiAction API from gpuidart 135d300 (P0-4, shipped)
library;

import 'package:gpuidart/gpuidart.dart';

import 'models.dart';

/// Builds the supercli desktop UI tree.
final class SupercliApp {
  SupercliApp();

  List<SessionSummary> sessions = const [];
  PendingApproval? pendingApproval;
  String composerText = '';
  int selectedSession = 0;
  String statusLine = 'Connecting…';

  /// Dataset backing the session list table.
  TableDataset get sessionDataset => TableDataset(
        'sessions',
        columns: const ['Title', 'Updated'],
        rows: sessions
            .map((s) => [s.title, _formatTime(s.updatedAt)])
            .toList(),
      );

  /// The full UI tree. Rebuilt on every state change via host.rebuild().
  UiNode build() {
    final approval = pendingApproval;
    return UiColumn('root', [
      const UiText('app-title', 'supercli'),
      UiText('status', statusLine),
      UiTable('session-list', dataset: 'sessions'),
      if (approval != null)
        _approvalCard(approval)
      else
        const UiText('no-approval', 'No pending approvals.'),
      const UiInput('composer', placeholder: 'Type a message… (Enter to send)'),
    ]);
  }

  /// Approval card built from primitives until P0-1 lands.
  ///
  /// GAP (P0-1): no dedicated approval card widget. Using UiRow + UiText +
  /// UiButton. Missing: structured diff view, risk badge, timeout countdown.
  UiNode _approvalCard(PendingApproval approval) {
    return UiColumn('approval-card', [
      UiText('approval-title', 'Approval requested'),
      UiText('approval-tool', 'Tool: ${approval.tool}'),
      UiText('approval-summary', approval.summary),
      if (approval.detail.isNotEmpty)
        UiText('approval-detail', approval.detail),
      UiRow('approval-buttons', [
        const UiButton('approve', 'Approve (Ctrl+Enter)'),
        const UiButton('deny', 'Deny'),
      ]),
    ]);
  }

  /// Keyboard bindings using the real UiAction API (gpuidart 135d300, P0-4).
  List<UiAction> actions() => const [
        // Approve the pending approval from anywhere.
        UiAction(name: 'approval.approve', keys: 'ctrl+enter'),
        // Deny the pending approval.
        UiAction(name: 'approval.deny', keys: 'ctrl+shift+enter'),
        // Move selection in the session list.
        UiAction(
          name: 'sessions.up',
          keys: 'up',
          context: UiActionContext.node('session-list'),
        ),
        UiAction(
          name: 'sessions.down',
          keys: 'down',
          context: UiActionContext.node('session-list'),
        ),
        // Focus the composer.
        UiAction(name: 'composer.focus', keys: 'ctrl+l'),
      ];

  static String _formatTime(DateTime t) {
    final now = DateTime.now();
    final diff = now.difference(t);
    if (diff.inMinutes < 1) return 'just now';
    if (diff.inHours < 1) return '${diff.inMinutes}m ago';
    if (diff.inDays < 1) return '${diff.inHours}h ago';
    return '${diff.inDays}d ago';
  }
}
