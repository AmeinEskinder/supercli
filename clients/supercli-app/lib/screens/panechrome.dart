/// Pane chrome: header menu, scroll/exited bars, restart banner, TUI colors.
///
/// Ports of `PaneHeaderMenu.swift`, `ScrollToBottomButton.swift`,
/// `ExitedSessionBar.swift`, `RestartBanner.swift`, and the agent TUI
/// background matching from `TerminalTheme.swift`.
///
/// Rows 174, 182, 183, 184 — [DESKTOP] parity.
/// GAP: no native-window screenshot proof yet (screenshot proof pending).
library;

import 'package:gpuidart/gpuidart.dart';

/// Row 174: Pane header menu with Agents and Plugins launch sections.
///
/// Each pane header carries a menu button; the menu lists launchable agents
/// and installed plugins so a new session can be started in the pane's
/// group without leaving the terminal area.
final class PaneHeaderMenu {
  const PaneHeaderMenu({
    required this.paneId,
    this.agents = const [],
    this.plugins = const [],
  });

  final String paneId;
  final List<String> agents;
  final List<String> plugins;

  UiNode build() {
    return UiColumn('pane-header-menu-$paneId', [
      const UiText('pane-menu-agents-title', 'Agents'),
      for (var i = 0; i < agents.length; i++)
        UiButton('pane-menu-agent-$i', 'Launch ${agents[i]}'),
      const UiText('pane-menu-plugins-title', 'Plugins'),
      for (var i = 0; i < plugins.length; i++)
        UiButton('pane-menu-plugin-$i', 'Open ${plugins[i]}'),
      const UiButton('pane-menu-split', 'Split Pane'),
      const UiButton('pane-menu-close', 'Close Pane'),
    ]);
  }

  List<UiAction> actions() => [
        UiAction(
            name: 'pane.menu.open',
            keys: 'alt+enter',
            context: UiActionContext.node('pane-header-menu-$paneId')),
      ];
}

/// Row 182: Scroll-to-bottom button shown when the pane is scrolled up.
final class ScrollToBottomButton {
  const ScrollToBottomButton({required this.visible});

  final bool visible;

  UiNode build() {
    if (!visible) return UiRow('scroll-bottom-hidden', const []);
    return const UiButton('scroll-to-bottom', '↓ New output');
  }
}

/// Row 182: Exited-session bar with Resume / Start fresh and resume-failure notice.
final class ExitedSessionBar {
  const ExitedSessionBar({
    required this.sessionId,
    this.resumeFailed = false,
    this.resumeError = '',
  });

  final String sessionId;
  final bool resumeFailed;
  final String resumeError;

  UiNode build() {
    return UiColumn('exited-bar-$sessionId', [
      const UiText('exited-title', 'Session exited'),
      UiRow('exited-buttons', const [
        UiButton('exited-resume', 'Resume'),
        UiButton('exited-fresh', 'Start fresh'),
      ]),
      if (resumeFailed)
        UiText('exited-resume-error',
            'Resume failed${resumeError.isEmpty ? '' : ': $resumeError'}'),
    ]);
  }
}

/// Row 183: Restart recommendation banner (e.g. after an update or crash).
final class RestartBanner {
  const RestartBanner({required this.reason, this.dismissed = false});

  final String reason;
  final bool dismissed;

  UiNode build() {
    if (dismissed) return UiRow('restart-banner-hidden', const []);
    return UiRow('restart-banner', [
      UiText('restart-reason', 'Restart recommended: $reason'),
      const UiButton('restart-now', 'Restart Now'),
      const UiButton('restart-later', 'Later'),
    ]);
  }
}

/// Row 184: Agent TUI background color matching for chrome.
///
/// The terminal chrome (pane headers, find bar) tints itself to the running
/// agent's TUI background so the pane reads as one surface. Colors are
/// `UiHexColor` values sourced from the agent manifest; unknown agents fall
/// back to the default surface color.
final class AgentTuiTheme {
  const AgentTuiTheme({required this.agentId, this.backgroundHex});

  final String agentId;
  final String? backgroundHex;

  /// Resolved chrome background: agent color or default surface.
  String get chromeBackground => backgroundHex ?? '#1e1e1e';

  UiNode build() {
    return UiRow('agent-tui-$agentId', [
      UiText('agent-tui-label', 'Agent: $agentId'),
      UiText('agent-tui-color', chromeBackground),
    ]);
  }
}
