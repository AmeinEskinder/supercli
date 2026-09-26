/// Approvals panel: pending tool approval requests with allow/deny/edit.
///
/// Port of `MCPApprovalPanel.swift` (McpApprovalPaneOverlay) plus the
/// approval list from the macOS sidebar. The Swift version renders as a
/// glass-card overlay on the session pane with: attention dot, title,
/// body, "N more waiting" count, Allow/Don't Allow buttons, and
/// Return-to-allow / Escape-to-deny key capture (McpApprovalKeyMonitor).
///
/// The gpuidart port uses UiColumn + UiRow + UiText + UiButton primitives
/// (RLE fallback pattern) until the framework ships a dedicated approval
/// card widget.
///
/// Keyboard (matches the rendered proof screenshots approve-before.png /
/// deny-before.png and the e2e harness in supercli-serve/src/mobile.rs):
/// - Ctrl+Enter: approve the focused approval
/// - Ctrl+Shift+Enter: deny the focused approval
/// - Ctrl+E: edit the request before answering (opens the detail editor)
///
/// Note: gpuidart rejects bare-letter bindings (native text input owns
/// them), so edit uses Ctrl+E rather than the Swift version's bare E.
///
/// GAP (P0-1): No UiApprovalCard widget. Missing: severity styling,
/// timeout countdown, structured diff view, glass effect. Using primitives.
/// See docs/gpuidart-gaps-approvals.md.
library;

import 'package:gpuidart/gpuidart.dart';

import '../models.dart';

/// Which answer the user gave for an approval.
enum ApprovalDecision {
  approve,
  deny,
}

/// The in-pane MCP approval overlay for a single pending approval.
///
/// Renders: attention dot, "Approval requested" title, tool line,
/// summary, detail, "N more waiting" count, and the
/// Allow (Ctrl+Enter) / Don't Allow buttons plus an Edit action.
final class McpApprovalPanel {
  const McpApprovalPanel({
    required this.approval,
    this.moreWaiting = 0,
    this.onDecision,
  });

  final PendingApproval approval;
  final int moreWaiting;

  /// Called when the user answers. Null = render-only (no callback).
  final void Function(ApprovalDecision decision)? onDecision;

  UiNode build() {
    return UiColumn('mcp-approval-overlay', [
      UiRow('mcp-approval-header', [
        const UiText('mcp-approval-dot', '●'),
        const UiText('mcp-approval-title', 'Approval requested'),
      ]),
      UiText('mcp-approval-tool', 'Tool: ${approval.tool}'),
      UiText('mcp-approval-summary', approval.summary),
      if (approval.detail.isNotEmpty)
        UiText('mcp-approval-body', approval.detail),
      if (moreWaiting > 0)
        UiText('mcp-approval-more', '$moreWaiting more waiting'),
      UiRow('mcp-approval-buttons', [
        const UiButton('mcp-allow', 'Allow (Ctrl+Enter)'),
        const UiButton('mcp-deny', "Don't Allow"),
        const UiButton('mcp-edit', 'Edit (Ctrl+E)'),
      ]),
    ]);
  }

  /// Handle a UI action by name. Returns true if the action was consumed.
  bool handleAction(String actionName) {
    final cb = onDecision;
    if (cb == null) return false;
    switch (actionName) {
      case 'mcp.approve':
        cb(ApprovalDecision.approve);
        return true;
      case 'mcp.deny':
        cb(ApprovalDecision.deny);
        return true;
      case 'mcp.edit':
        // Edit opens the detail editor; the decision comes later.
        // Rendered as a no-op here — the host opens the editor.
        return true;
    }
    return false;
  }

  List<UiAction> actions() => const [
        // Ctrl+Enter approves (matches the e2e key injector).
        UiAction(
          name: 'mcp.approve',
          keys: 'ctrl+enter',
          context: UiActionContext.node('mcp-approval-overlay'),
        ),
        // Ctrl+Shift+Enter denies.
        UiAction(
          name: 'mcp.deny',
          keys: 'ctrl+shift+enter',
          context: UiActionContext.node('mcp-approval-overlay'),
        ),
        // Ctrl+E opens the edit/detail view before answering.
        UiAction(
          name: 'mcp.edit',
          keys: 'ctrl+e',
          context: UiActionContext.node('mcp-approval-overlay'),
        ),
      ];
}

/// The full approvals list panel: all pending approvals, newest first.
///
/// Each row shows the tool, summary, and age; selecting a row focuses the
/// [McpApprovalPanel] overlay for that approval. When empty, shows the
/// "No pending approvals." empty state.
final class ApprovalsPanel {
  const ApprovalsPanel({
    this.approvals = const [],
    this.selectedId,
  });

  final List<PendingApproval> approvals;
  final String? selectedId;

  UiNode build() {
    if (approvals.isEmpty) {
      return UiColumn('approvals-panel', const [
        UiText('approvals-empty', 'No pending approvals.'),
      ]);
    }
    return UiColumn('approvals-panel', [
      UiText(
        'approvals-count',
        '${approvals.length} pending approval${approvals.length == 1 ? '' : 's'}',
      ),
      for (final a in approvals)
        UiRow('approval-row-${a.id}', [
          const UiText('approval-row-dot', '●'),
          UiText('approval-row-tool-${a.id}', a.tool),
          UiText('approval-row-summary-${a.id}', a.summary),
        ]),
      // The focused approval renders as the overlay below the list.
      if (selectedId != null)
        for (final a in approvals)
          if (a.id == selectedId)
            McpApprovalPanel(
              approval: a,
              moreWaiting: approvals.length - 1,
            ).build(),
    ]);
  }

  List<UiAction> actions() => const [
        UiAction(
          name: 'approvals.up',
          keys: 'up',
          context: UiActionContext.node('approvals-panel'),
        ),
        UiAction(
          name: 'approvals.down',
          keys: 'down',
          context: UiActionContext.node('approvals-panel'),
        ),
      ];
}
