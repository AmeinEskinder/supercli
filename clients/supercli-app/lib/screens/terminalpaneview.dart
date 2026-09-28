/// Terminal pane: the embedded terminal surface for a session.
///
/// Real implementation of the P0-8 terminal pane (not a scaffold). The pane
/// is backed by [TerminalState] (grid + scrollback + cursor + selection) and
/// renders through [TerminalPane] via the RLE fallback (P0-8 UiTerminal proposal pending)
/// node for the native renderer plus a run-length-encoded UiRow/UiText
/// fallback that displays today with real ANSI 256 + truecolor.
///
/// See `lib/terminal/` for the implementation and
/// `docs/gpuidart-gaps-terminal.md` for what gpuidart cannot yet do.
library;

import 'package:gpuidart/gpuidart.dart';

import '../host_client.dart';
import '../terminal/session_output_stream.dart';
import '../terminal/terminal_pane.dart';
import '../terminal/terminal_state.dart';

export '../terminal/terminal_pane.dart' show TerminalPane, TerminalKeymap;
export '../terminal/terminal_state.dart' show TerminalState, TerminalCellUpdate;
export '../terminal/session_output_stream.dart' show SessionOutputStream;

/// One terminal pane within a session.
final class TerminalPaneView {
  TerminalPaneView({
    required this.paneId,
    required this.title,
    List<String> lines = const [],
    this.findBarVisible = false,
    TerminalState? state,
    this.onInput,
    this.onResize,
    this.onCopy,
  }) : state = state ?? _stateFromLines(lines),
       stream = null,
       onStreamError = null,
       sessionId = null;

  /// A terminal pane bound to a LIVE Host session.
  ///
  /// Creates one [TerminalState] shared by the pane and its
  /// [SessionOutputStream]; [start] begins polling `GET /mobile/output`
  /// and feeding the ANSI parser. Input (key bindings) forwards to
  /// `POST /mobile/write` with a per-batch idempotency id; resize forwards
  /// to `POST /mobile/resize`. All traffic goes through the authenticated
  /// [HostClient] — the view never reads Host journal files directly.
  factory TerminalPaneView.hosted({
    required HostClient client,
    required String sessionId,
    required String paneId,
    required String title,
    bool findBarVisible = false,
    void Function(String text)? onCopy,
    void Function(Object error)? onStreamError,
    int cols = 80,
    int rows = 24,
  }) {
    final state = TerminalState(cols: cols, rows: rows);
    final stream = SessionOutputStream(
      client: client,
      sessionId: sessionId,
      state: state,
    );
    var widCounter = 0;
    return TerminalPaneView._(
      paneId: paneId,
      title: title,
      state: state,
      stream: stream,
      sessionId: sessionId,
      findBarVisible: findBarVisible,
      onCopy: onCopy,
      onStreamError: onStreamError,
      onInput: (bytes) {
        // Fire-and-forget: a failed write surfaces via onStreamError on
        // the next poll cycle; the idempotency id keeps retries safe.
        stream.sendInput(bytes, writeId: 'wid-${widCounter++}').ignore();
      },
      onResize: (cols, rows) {
        stream.resize(cols, rows).ignore();
      },
    );
  }

  const TerminalPaneView._({
    required this.paneId,
    required this.title,
    required this.state,
    required this.stream,
    required this.findBarVisible,
    required this.onInput,
    required this.onResize,
    required this.onCopy,
    required this.onStreamError,
    this.sessionId,
  });

  final String paneId;
  final String title;
  final TerminalState state;

  /// The Host session id this pane streams, or null for static/demo panes.
  final String? sessionId;

  /// Non-null for [TerminalPaneView.hosted]: the live Host stream feeding
  /// [state]. Null for static/demo panes.
  final SessionOutputStream? stream;
  final bool findBarVisible;
  final void Function(List<int> bytes)? onInput;
  final void Function(int cols, int rows)? onResize;
  final void Function(String text)? onCopy;

  /// Stream errors (auth failures, session gone, …). The app surfaces these
  /// in the status line / toasts.
  final void Function(Object error)? onStreamError;

  /// True for panes bound to a live Host session.
  bool get isLive => stream != null;

  /// Start the Host output stream. No-op for static panes.
  Future<void> start() =>
      stream?.start(onError: onStreamError) ?? Future.value();

  /// Stop the Host output stream. No-op for static panes.
  void stop() => stream?.stop();

  static TerminalState _stateFromLines(List<String> lines) {
    final cols = lines.fold<int>(80, (m, l) => l.length > m ? l.length : m);
    final state = TerminalState(cols: cols, rows: 24);
    if (lines.isNotEmpty) {
      state.writeString(lines.join('\n'));
    }
    return state;
  }

  /// The underlying P0-8 pane (state + rendering + input).
  TerminalPane get pane => TerminalPane(
    paneId: paneId,
    title: title,
    state: state,
    onInput: onInput,
    onResize: onResize,
    onCopy: onCopy,
    findBarVisible: findBarVisible,
  );

  UiNode build() => pane.build();

  /// Key bindings for the pane's special keys, scoped to the terminal node.
  /// The scope must name a node that exists in the mounted tree
  /// ('terminal-pane-$paneId' from [TerminalPane.build]); gpuidart rejects
  /// action contexts that match no node.
  List<UiAction> actions() =>
      const TerminalKeymap().actionsFor('terminal-pane-$paneId');
}

/// Terminal pane window chrome (title bar, traffic lights are native).
/// See terminalarea.dart for the area that hosts panes.
