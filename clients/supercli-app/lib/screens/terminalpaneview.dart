/// Terminal pane: the embedded terminal surface for a session.
///
/// Port of `TerminalPaneView.swift` (2224 lines) and `TerminalArea.swift`.
/// The Swift version embeds a ghostty terminal surface (via GhosttyBridge)
/// with: pane title chips, split controls, find bar, drop targets, and
/// per-pane close policy. The gpuidart port renders the terminal as text
/// rows until P0-6 (video/texture surface) lands.
///
/// GAP (P0-6): No GPU texture/video surface widget. Terminal output renders
/// as UiText rows — no ANSI colors, no cursor positioning, no scrollback
/// virtualization. P0-10 (terminal grid widget) logged separately.
library;

import 'package:gpuidart/gpuidart.dart';

/// One terminal pane within a session.
final class TerminalPaneView {
  TerminalPaneView({
    required this.paneId,
    required this.title,
    this.lines = const [],
    this.findBarVisible = false,
  });

  final String paneId;
  final String title;
  final List<String> lines;
  final bool findBarVisible;

  UiNode build() {
    return UiColumn('terminal-pane-$paneId', [
      UiRow('pane-header-$paneId', [
        UiText('pane-title-$paneId', title),
        const UiButton('pane-split-h', 'Split H'),
        const UiButton('pane-split-v', 'Split V'),
        const UiButton('pane-close', '×'),
      ]),
      if (findBarVisible)
        UiRow('find-bar-$paneId', [
          const UiInput('find-input', placeholder: 'Find…'),
          const UiButton('find-next', 'Next'),
          const UiButton('find-prev', 'Prev'),
        ]),
      // Terminal content as text rows (stopgap for P0-6/P0-10).
      UiColumn('terminal-lines-$paneId', [
        for (var i = 0; i < lines.length; i++)
          UiText('line-$paneId-$i', lines[i]),
      ]),
    ]);
  }
}

/// Terminal pane window chrome (title bar, traffic lights are native).
/// See terminalarea.dart for the area that hosts panes.
