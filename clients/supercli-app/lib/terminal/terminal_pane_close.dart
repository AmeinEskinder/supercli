/// Close-action decision logic for terminal panes.
///
/// Port of `TerminalPaneCloseAction` and `terminalPaneCloseAction(for:canArchiveSession:)`
/// from `TerminalPaneView.swift` (native/SupercliNative).
///
/// Closing is a Session lifecycle action, deliberately distinct from the
/// presentation-only "Detach Pane" verb:
/// - Empty launchers just disappear ([TerminalPaneCloseAction.detachPane]).
/// - Terminals without resumable provider state are disposable
///   ([TerminalPaneCloseAction.removeSession]).
/// - An agent conversation is stopped and archived only after confirmation
///   ([TerminalPaneCloseAction.confirmArchive]).
library;

/// What closing a pane does.
sealed class TerminalPaneCloseAction {
  const TerminalPaneCloseAction();

  /// The pane has no session (empty launcher): just remove the pane.
  static const detachPane = _DetachPane();

  /// Remove a disposable session (no resumable provider state).
  static TerminalPaneCloseAction removeSession(String sessionId) =>
      _RemoveSession(sessionId);

  /// Ask for confirmation, then stop and archive an agent conversation.
  static TerminalPaneCloseAction confirmArchive(String sessionId) =>
      _ConfirmArchive(sessionId);
}

final class _DetachPane extends TerminalPaneCloseAction {
  const _DetachPane();
  @override
  bool operator ==(Object other) => other is _DetachPane;
  @override
  int get hashCode => 0;
  @override
  String toString() => 'TerminalPaneCloseAction.detachPane';
}

final class _RemoveSession extends TerminalPaneCloseAction {
  const _RemoveSession(this.sessionId);
  final String sessionId;
  @override
  bool operator ==(Object other) =>
      other is _RemoveSession && other.sessionId == sessionId;
  @override
  int get hashCode => sessionId.hashCode;
  @override
  String toString() => 'TerminalPaneCloseAction.removeSession($sessionId)';
}

final class _ConfirmArchive extends TerminalPaneCloseAction {
  const _ConfirmArchive(this.sessionId);
  final String sessionId;
  @override
  bool operator ==(Object other) =>
      other is _ConfirmArchive && other.sessionId == sessionId;
  @override
  int get hashCode => sessionId.hashCode ^ 0x9e3779b9;
  @override
  String toString() => 'TerminalPaneCloseAction.confirmArchive($sessionId)';
}

/// Decide what closing a pane does.
///
/// Mirrors `terminalPaneCloseAction(for:canArchiveSession:)`:
/// - `sessionId == null` (launcher, no session) → [detachPane].
/// - `canArchiveSession` → [confirmArchive] (agent conversation needs
///   confirmation before stop+archive).
/// - otherwise → [removeSession] (disposable terminal).
TerminalPaneCloseAction terminalPaneCloseAction({
  required String? sessionId,
  required bool canArchiveSession,
}) {
  if (sessionId == null) return TerminalPaneCloseAction.detachPane;
  return canArchiveSession
      ? TerminalPaneCloseAction.confirmArchive(sessionId)
      : TerminalPaneCloseAction.removeSession(sessionId);
}
