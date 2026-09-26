/// Terminal area: the content pane's terminal grid.
///
/// Port of `TerminalArea.swift`. Hosts the TerminalPaneViews for the active
/// session. See terminalpaneview.dart for the pane implementation.
library;

import 'package:gpuidart/gpuidart.dart';

import 'terminalpaneview.dart';

export 'terminalpaneview.dart' show TerminalPaneView;

/// The terminal area: grid of panes for the active session.
final class TerminalArea {
  TerminalArea({
    this.panes = const [],
    this.activePaneId,
    this.statusText = '',
  });

  final List<TerminalPaneView> panes;
  final String? activePaneId;
  final String statusText;

  UiNode build() {
    return UiColumn('terminal-area-root', [
      if (panes.isEmpty)
        const UiText('terminal-area-empty',
            'No terminal panes. Select a session from the sidebar.'),
      for (final pane in panes) pane.build(),
      if (statusText.isNotEmpty) UiText('terminal-status', statusText),
    ]);
  }
}
