/// Terminal find bar: in-pane search UI.
///
/// Port of `TerminalFindBar.swift`. Row 177 — [DESKTOP] parity.
/// Keybindings use the platform primary modifier (Cmd on macOS, Ctrl on
/// Linux/Windows) via [primaryModifier].
library;

import 'package:gpuidart/gpuidart.dart';

import '../platform_keys.dart';

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

  /// Row 177: ⌘F opens, ⌘G / ⇧⌘G move between matches.
  List<UiAction> actions() {
    final mod = currentPrimaryModifier;
    return [
      UiAction(
          name: 'find.open',
          keys: '$mod+f',
          context: UiActionContext.node('terminal-find-bar')),
      UiAction(
          name: 'find.next',
          keys: '$mod+g',
          context: UiActionContext.node('terminal-find-bar')),
      UiAction(
          name: 'find.prev',
          keys: 'shift+$mod+g',
          context: UiActionContext.node('terminal-find-bar')),
      const UiAction(
          name: 'find.close',
          keys: 'escape',
          context: UiActionContext.node('terminal-find-bar')),
    ];
  }
}
