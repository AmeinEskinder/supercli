/// Remote-host panes through the same terminal UI (row 170).
///
/// A remote pane is the same [TerminalPaneView]/[TerminalState] rendering
/// pipeline as a local pane — the only difference is where bytes come from
/// and go to. [RemoteTerminalPane] owns a [TerminalState] fed by
/// [feedOutput] (remote PTY output delivered by the host event loop) and
/// routes keystrokes from [TerminalPaneView.onInput] to [sendInput], which
/// the app shell wires to the host (e.g. POST to the remote session's
/// input). No separate renderer, no fake grid: the remote session draws
/// through the identical in-memory surface unpeel keeps per local session.
library;

import '../screens/terminalpaneview.dart';

/// One remote-host terminal pane.
class RemoteTerminalPane {
  RemoteTerminalPane({
    required this.paneId,
    required this.hostName,
    required this.sessionId,
    TerminalState? state,
    this.sendInput,
    this.title,
  }) : state = state ?? TerminalState(cols: 80, rows: 24);

  /// Pane id within the local layout.
  final String paneId;

  /// Remote host label (shown in the pane header).
  final String hostName;

  /// Remote session id on that host.
  final String sessionId;

  /// The shared in-memory terminal surface (identical type to local panes).
  final TerminalState state;

  /// Forwards keystroke bytes to the remote host. Set by the app shell.
  void Function(String sessionId, List<int> bytes)? sendInput;

  final String? title;

  /// Feed remote PTY output into the surface. Renders exactly like local
  /// output because it is the same [TerminalState].
  void feedOutput(String text) => state.writeString(text);

  /// Feed raw bytes (e.g. from a host event payload).
  void feedBytes(List<int> bytes) =>
      state.writeString(String.fromCharCodes(bytes));

  /// Build the same [TerminalPaneView] a local pane uses.
  TerminalPaneView view() => TerminalPaneView(
    paneId: paneId,
    title: title ?? '$hostName — $sessionId',
    state: state,
    onInput: (bytes) => sendInput?.call(sessionId, bytes),
  );

  /// Whether keystrokes have somewhere to go.
  bool get isConnected => sendInput != null;
}
