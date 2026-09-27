/// Pane confirmation dialog content.
///
/// Ports `PaneConfirmation` and `PaneConfirmationOverlay` from
/// `TerminalPaneView.swift` (TerminalPaneContainer):
///
/// ```swift
/// private struct PaneConfirmation: Identifiable {
///     enum Action { case archive, remove }
///     let action: Action
///     let sessionID: String
///     let label: String
///     let isLive: Bool
///     var id: String { "\(sessionID):\(action)" }
/// }
/// ```
///
/// The overlay is an in-app confirmation (not a system alert): on macOS 26
/// the glass alert over live Metal surfaces re-rendered its focus ring
/// constantly. Esc and click-away cancel.
library;

/// The action a pane confirmation dialog confirms.
enum PaneConfirmationAction {
  /// Stop the session and archive it (restorable later).
  archive,

  /// Remove the session from the list (live) or the sidebar (not live).
  remove,
}

/// State for the pane archive/remove confirmation dialog.
class PaneConfirmation {
  const PaneConfirmation({
    required this.action,
    required this.sessionID,
    required this.label,
    required this.isLive,
  });

  final PaneConfirmationAction action;
  final String sessionID;
  final String label;

  /// Whether the session is currently live (affects the remove title).
  final bool isLive;

  /// Stable identity, matching the Swift `"\(sessionID):\(action)"`.
  String get id => '$sessionID:${action.name}';

  /// Dialog title.
  ///
  /// Matches the Swift:
  /// ```swift
  /// private var title: String {
  ///     switch confirmation.action {
  ///     case .archive: return "Stop and archive session?"
  ///     case .remove:
  ///         return confirmation.isLive ? "Remove session?" : "Remove from list?"
  ///     }
  /// }
  /// ```
  String get title => switch (action) {
        PaneConfirmationAction.archive => 'Stop and archive session?',
        PaneConfirmationAction.remove =>
          isLive ? 'Remove session?' : 'Remove from list?',
      };

  /// Dialog message.
  ///
  /// Matches the Swift:
  /// ```swift
  /// private var message: String {
  ///     switch confirmation.action {
  ///     case .archive:
  ///         return "This stops “\(label)” and files it in Supercli so you can restore and resume later. Screenshots and other session files stay."
  ///     case .remove:
  ///         return "This only removes “\(label)” from Supercli. It does not delete the agent’s conversation."
  ///     }
  /// }
  /// ```
  String get message => switch (action) {
        PaneConfirmationAction.archive =>
          'This stops “$label” and files it in Supercli so you can restore '
              'and resume later. Screenshots and other session files stay.',
        PaneConfirmationAction.remove =>
          'This only removes “$label” from Supercli. It does not delete the '
              'agent’s conversation.',
      };

  /// Confirm button label.
  String get confirmLabel =>
      action == PaneConfirmationAction.archive ? 'Archive' : 'Remove';

  @override
  bool operator ==(Object other) =>
      other is PaneConfirmation &&
      other.action == action &&
      other.sessionID == sessionID &&
      other.label == label &&
      other.isLive == isLive;

  @override
  int get hashCode => Object.hash(action, sessionID, label, isLive);
}
