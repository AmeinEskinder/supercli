/// Terminal pane window: detached terminal window chrome.
///
/// Port of `TerminalPaneWindow.swift`. A terminal pane popped out into its
/// own window. The gpuidart port renders the same TerminalPaneView content;
/// window management is native (one host, one window per process — P0).
///
/// GAP: gpuidart supports one window per process. Detached windows need
/// multi-window support (P0-14).
library;

import 'package:gpuidart/gpuidart.dart';

import 'terminalpaneview.dart';

final class TerminalPaneWindow {
  TerminalPaneWindow({
    required this.pane,
  });

  final TerminalPaneView pane;

  UiNode build() {
    return UiColumn('terminal-pane-window', [
      UiText('pane-window-title', pane.title),
      pane.build(),
    ]);
  }
}
