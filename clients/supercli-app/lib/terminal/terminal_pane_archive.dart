/// Pane-close archive decision and agent-terminal detection.
///
/// Port of the portable lifecycle logic from `TerminalPaneView.swift`
/// (native/SupercliNative, `TerminalPaneContainer`):
///
/// - `requestPaneArchive`: the pane menu's "Stop and archive"/"Archive"
///   verb. A starting/busy/attention session gets the in-app confirmation
///   card (the destructive state needs an explicit user decision);
///   idle/exited sessions archive directly via
///   `store.requestArchiveSession`.
/// - `isAgentTerminal`: a pane whose session hosts a recognized agent or
///   App occupant, or was launched as one — never a plain shell. Only
///   these panes get the top-right corner gallery controls (in local scope
///   with the gallery visible).
library;

import 'terminal_pane_focus.dart';

/// What the pane's archive verb does.
enum PaneArchiveDecision {
  /// Show the in-app confirmation card first.
  requestConfirmation,

  /// Archive directly via `store.requestArchiveSession`.
  requestDirect,
}

/// Portable archive-verb rules for terminal panes.
abstract final class TerminalPaneArchive {
  const TerminalPaneArchive._();

  /// Decide what "Stop and archive"/"Archive" does for a session status.
  ///
  /// Mirrors Swift `requestPaneArchive`: starting/busy/attention →
  /// confirmation card; idle/exited → direct archive request.
  static PaneArchiveDecision paneArchiveDecision(PaneSessionStatus status) {
    switch (status) {
      case PaneSessionStatus.starting:
      case PaneSessionStatus.busy:
      case PaneSessionStatus.attention:
        return PaneArchiveDecision.requestConfirmation;
      case PaneSessionStatus.idle:
      case PaneSessionStatus.exited:
        return PaneArchiveDecision.requestDirect;
    }
  }

  /// Whether the session is an agent terminal (gets corner gallery
  /// controls), as opposed to a plain shell.
  ///
  /// Mirrors Swift `isAgentTerminal`: `entry.activeRuntimeID != nil ||
  /// entry.activeApp != nil || SetupTool.detect(in: entry.command) != nil`.
  ///
  /// [activeRuntimeID] is the Host-observed foreground runtime id
  /// (e.g. "claude"; `SessionSummary.agentId`); [activeAppName] is the
  /// Host-resolved installed App name (`SessionSummary.appName`);
  /// [setupToolDetected] is the caller's verdict from the Rust
  /// `supercli-core::setup::SetupTool::detect` scan of the launch command —
  /// terminal emulation and command scanning stay in Rust.
  static bool isAgentTerminal({
    required String? activeRuntimeID,
    required String? activeAppName,
    required bool setupToolDetected,
  }) =>
      (activeRuntimeID != null && activeRuntimeID.isNotEmpty) ||
      (activeAppName != null && activeAppName.isNotEmpty) ||
      setupToolDetected;
}
