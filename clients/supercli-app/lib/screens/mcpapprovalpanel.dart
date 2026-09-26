/// In-pane Allow / Don't Allow card for a pending MCP approval.
///
/// Port of `MCPApprovalPanel.swift` (McpApprovalPaneOverlay). The Swift
/// version renders as a glass-card overlay on the session pane with:
/// attention dot, title, body, "N more waiting" count, Allow/Don't Allow
/// buttons, and Return-to-allow / Escape-to-deny key capture.
///
/// The gpuidart port uses UiColumn + UiText + UiRow of UiButtons.
///
/// GAP (P0-1): No UiApprovalCard widget. Missing: severity styling, timeout
/// countdown, structured diff view, glass effect. Using primitives.
library;

import 'package:gpuidart/gpuidart.dart';

import '../models.dart';

/// The in-pane MCP approval overlay.
final class McpApprovalPanel {
  const McpApprovalPanel({
    required this.approval,
    this.moreWaiting = 0,
  });

  final PendingApproval approval;
  final int moreWaiting;

  UiNode build() {
    return UiColumn('mcp-approval-overlay', [
      UiRow('mcp-approval-header', [
        const UiText('mcp-approval-dot', '●'),
        UiText('mcp-approval-title', approval.summary),
      ]),
      UiText('mcp-approval-body', approval.detail),
      if (moreWaiting > 0)
        UiText('mcp-approval-more', '$moreWaiting more waiting'),
      UiRow('mcp-approval-buttons', [
        const UiButton('mcp-deny', "Don't Allow"),
        const UiButton('mcp-allow', 'Allow'),
      ]),
    ]);
  }

  List<UiAction> actions() => const [
        // Return allows, Escape denies (Swift: McpApprovalKeyMonitor).
        UiAction(name: 'mcp.approve', keys: 'enter',
            context: UiActionContext.node('mcp-approval-overlay')),
        UiAction(name: 'mcp.deny', keys: 'escape',
            context: UiActionContext.node('mcp-approval-overlay')),
      ];
}
