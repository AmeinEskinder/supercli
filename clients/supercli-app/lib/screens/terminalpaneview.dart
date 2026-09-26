/// Terminal pane: the embedded terminal surface for a session.
///
/// Real implementation of the P0-8 terminal pane (not a scaffold). The pane
/// is backed by [TerminalState] (grid + scrollback + cursor + selection) and
/// renders through [TerminalPane], which builds the `UiTerminal` snapshot
/// node for the native renderer plus a run-length-encoded UiRow/UiText
/// fallback that displays today with real ANSI 256 + truecolor.
///
/// See `lib/terminal/` for the implementation and
/// `docs/gpuidart-gaps-terminal.md` for what gpuidart cannot yet do.
library;

import 'package:gpuidart/gpuidart.dart';

import '../terminal/terminal_pane.dart';
import '../terminal/terminal_state.dart';

export '../terminal/terminal_pane.dart' show TerminalPane, TerminalKeymap;
export '../terminal/terminal_state.dart' show TerminalState, TerminalCellUpdate;

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
  }) : state = state ?? _stateFromLines(lines);

  final String paneId;
  final String title;
  final TerminalState state;
  final bool findBarVisible;
  final void Function(List<int> bytes)? onInput;
  final void Function(int cols, int rows)? onResize;
  final void Function(String text)? onCopy;

  static TerminalState _stateFromLines(List<String> lines) {
    final cols = lines.fold<int>(
        80, (m, l) => l.length > m ? l.length : m);
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
  List<UiAction> actions() =>
      const TerminalKeymap().actionsFor('terminal-$paneId');
}

/// Terminal pane window chrome (title bar, traffic lights are native).
/// See terminalarea.dart for the area that hosts panes.
