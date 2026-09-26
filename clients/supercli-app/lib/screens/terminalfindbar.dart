/// Terminal find bar: in-pane search UI.
///
/// Port of `TerminalFindBar.swift`.
library;

import 'package:gpuidart/gpuidart.dart';

final class TerminalFindBar {
  const TerminalFindBar({
    this.query = '',
    this.matchIndex = 0,
    this.matchCount = 0,
  });

  final String query;
  final int matchIndex;
  final int matchCount;

  UiNode build() {
    return UiRow('terminal-find-bar', [
      const UiInput('find-query', placeholder: 'Find in terminal…'),
      UiText('find-count', matchCount > 0 ? '$matchIndex of $matchCount' : 'No results'),
      const UiButton('find-prev', '↑'),
      const UiButton('find-next', '↓'),
      const UiButton('find-close', '×'),
    ]);
  }

  List<UiAction> actions() => const [
        UiAction(name: 'find.next', keys: 'enter',
            context: UiActionContext.node('terminal-find-bar')),
        UiAction(name: 'find.close', keys: 'escape',
            context: UiActionContext.node('terminal-find-bar')),
      ];
}
