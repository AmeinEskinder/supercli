/// Pane identity and activation model for the terminal pane container.
///
/// Ports the portable state logic from `TerminalPaneView.swift`
/// (TerminalPaneContainer, ~2,224 lines):
///
/// - `PresentedPane.id`: session identity keeps an existing terminal surface
///   mounted while panes move; a launcher has no session yet so its stable
///   pane id is its temporary identity.
/// - `defaultActivePaneID`: first pane matching the representative session,
///   else the group's representative pane, else the synthetic solo pane.
/// - `presentedPanes`: group panes, or a single synthetic full-width pane
///   when there is no group.
library;

/// What a pane shows: a live session, or a launcher awaiting a session.
sealed class PaneContent {
  const PaneContent();
}

/// A live terminal session.
final class SessionContent extends PaneContent {
  const SessionContent(this.sessionId);
  final String sessionId;

  @override
  bool operator ==(Object other) =>
      other is SessionContent && other.sessionId == sessionId;

  @override
  int get hashCode => sessionId.hashCode;
}

/// A launcher card (no session yet).
final class LauncherContent extends PaneContent {
  const LauncherContent();

  @override
  bool operator ==(Object other) => other is LauncherContent;

  @override
  int get hashCode => 0;
}

/// A pane as presented in the container, with its stable identity.
class PresentedPane {
  const PresentedPane({
    required this.paneId,
    required this.content,
    required this.isSynthetic,
  });

  /// The pane's id within its group (or the synthetic solo id).
  final String paneId;

  final PaneContent content;

  /// True for the synthetic full-width pane rendered when there is no group.
  final bool isSynthetic;

  /// Stable identity: session identity keeps an existing terminal surface
  /// mounted while panes move or a solo session joins/leaves a group.
  /// A launcher has no session yet, so its stable pane id is its temporary
  /// identity.
  ///
  /// Matches the Swift:
  /// ```swift
  /// var id: String {
  ///     switch content {
  ///     case let .session(sessionID): return "session:\(sessionID)"
  ///     case .launcher: return "launcher:\(paneID)"
  ///     }
  /// }
  /// ```
  String get id => switch (content) {
        SessionContent(:final sessionId) => 'session:$sessionId',
        LauncherContent() => 'launcher:$paneId',
      };

  @override
  bool operator ==(Object other) =>
      other is PresentedPane &&
      other.paneId == paneId &&
      other.content == content &&
      other.isSynthetic == isSynthetic;

  @override
  int get hashCode => Object.hash(paneId, content, isSynthetic);
}

/// A pane entry in a group (id + content), mirroring the Swift `PaneGroup`.
class GroupPaneEntry {
  const GroupPaneEntry({required this.id, required this.content});

  final String id;
  final PaneContent content;
}

/// Builds the presented panes for a container.
///
/// If [groupPanes] is null, returns a single synthetic full-width pane for
/// the representative session (matching the Swift `presentedPanes` when
/// `group == nil`).
List<PresentedPane> presentedPanes({
  required String representativeId,
  List<GroupPaneEntry>? groupPanes,
}) {
  if (groupPanes == null) {
    return [
      PresentedPane(
        paneId: 'solo:$representativeId',
        content: SessionContent(representativeId),
        isSynthetic: true,
      ),
    ];
  }
  return [
    for (final pane in groupPanes)
      PresentedPane(
        paneId: pane.id,
        content: pane.content,
        isSynthetic: false,
      ),
  ];
}

/// Resolves the default active pane id.
///
/// Matches the Swift:
/// ```swift
/// private var defaultActivePaneID: String {
///     if let group {
///         return group.panes.first(where: {
///             $0.content.sessionID == representative.id
///         })?.id ?? group.representativePaneID
///     }
///     return "solo:\(representative.id)"
/// }
/// ```
String defaultActivePaneID({
  required String representativeId,
  List<GroupPaneEntry>? groupPanes,
  String? groupRepresentativePaneID,
}) {
  if (groupPanes != null) {
    for (final pane in groupPanes) {
      final content = pane.content;
      if (content is SessionContent && content.sessionId == representativeId) {
        return pane.id;
      }
    }
    if (groupRepresentativePaneID != null) {
      return groupRepresentativePaneID;
    }
    // Fallback: first pane (should not happen if the group is valid).
    if (groupPanes.isNotEmpty) return groupPanes.first.id;
  }
  return 'solo:$representativeId';
}
