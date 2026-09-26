/// Terminal area: the content pane's terminal grid.
///
/// Port of `TerminalArea.swift`. Hosts the [PaneLayout] split-pane tree for
/// the active session. See terminalpaneview.dart for the pane implementation
/// and pane_layout.dart for the split/zoom/focus model.
library;

import 'package:gpuidart/gpuidart.dart';

import '../pane_layout.dart';
import 'terminalfindbar.dart';
import 'terminalpaneview.dart';

export '../pane_layout.dart'
    show PaneLayout, PaneNode, PaneLeaf, PaneSplit, SplitDirection, FocusDirection, maxPanes;
export 'terminalpaneview.dart' show TerminalPaneView;

/// The terminal area: split-pane layout for the active session.
final class TerminalArea {
  TerminalArea({
    PaneLayout? layout,
    this.findBar,
    this.statusText = '',
    // ignore: deprecated_member_use_from_same_package
    List<TerminalPaneView> panes = const [],
  }) : layout = layout ?? _layoutFromPanes(panes);

  /// Deprecated: prefer [PaneLayout]. Builds a single-pane layout from
  /// legacy [panes] for backward compatibility.
  static PaneLayout _layoutFromPanes(List<TerminalPaneView> panes) {
    if (panes.isEmpty) {
      return PaneLayout.single(paneId: 'pane-1', title: 'zsh');
    }
    PaneLayout layout = PaneLayout.single(
        paneId: panes.first.paneId, title: panes.first.title);
    for (var i = 1; i < panes.length; i++) {
      layout = layout.split(
            direction: SplitDirection.vertical,
            newPaneId: panes[i].paneId,
            newTitle: panes[i].title,
          ) ??
          layout;
    }
    return layout;
  }

  final PaneLayout layout;
  final TerminalFindBar? findBar;
  final String statusText;

  UiNode build() {
    return UiColumn('terminal-area-root', [
      layout.build(),
      if (findBar != null) findBar!.build(),
      if (statusText.isNotEmpty) UiText('terminal-status', statusText),
    ]);
  }

  /// Pane-management key bindings plus find-bar actions.
  List<UiAction> actions() => [
        ...layout.actions(),
        if (findBar != null) ...findBar!.actions(),
      ];
}
