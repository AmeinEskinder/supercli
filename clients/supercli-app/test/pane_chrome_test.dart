/// Tests for pane chrome: header menu, scroll/exited bars, restart banner,
/// agent TUI colors. Rows 174, 182, 183, 184 — [DESKTOP] parity.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/screens/panechrome.dart';
import 'package:test/test.dart';

void main() {
  group('PaneHeaderMenu (row 174)', () {
    test('builds Agents and Plugins sections', () {
      final menu = PaneHeaderMenu(
        paneId: 'p1',
        agents: ['claude', 'codex'],
        plugins: ['git'],
      );
      final node = menu.build() as UiColumn;
      // title + 2 agents + title + 1 plugin + split + close
      expect(node.children.length, 7);
      expect((node.children[1] as UiButton).label, 'Launch claude');
      expect((node.children[4] as UiButton).label, 'Open git');
    });

    test('empty sections still show split/close', () {
      final node = PaneHeaderMenu(paneId: 'p1').build() as UiColumn;
      expect(node.children.length, 4);
    });

    test('exposes pane.menu.open action', () {
      final actions = PaneHeaderMenu(paneId: 'p1').actions();
      expect(actions.map((a) => a.name), contains('pane.menu.open'));
    });
  });

  group('ScrollToBottomButton (row 182)', () {
    test('hidden when not visible', () {
      final node =
          ScrollToBottomButton(visible: false).build() as UiRow;
      expect(node.children, isEmpty);
    });

    test('shows button when visible', () {
      final node = ScrollToBottomButton(visible: true).build();
      expect(node, isA<UiButton>());
      expect((node as UiButton).label, contains('New output'));
    });
  });

  group('ExitedSessionBar (row 182)', () {
    test('shows Resume and Start fresh', () {
      final node =
          ExitedSessionBar(sessionId: 's1').build() as UiColumn;
      final buttons = node.children[1] as UiRow;
      expect((buttons.children[0] as UiButton).label, 'Resume');
      expect((buttons.children[1] as UiButton).label, 'Start fresh');
    });

    test('resume failure shows notice', () {
      final node = ExitedSessionBar(
        sessionId: 's1',
        resumeFailed: true,
        resumeError: 'host unreachable',
      ).build() as UiColumn;
      expect(node.children.length, 3);
      expect((node.children[2] as UiText).text, contains('host unreachable'));
    });

    test('no error text when resume succeeded', () {
      final node =
          ExitedSessionBar(sessionId: 's1').build() as UiColumn;
      expect(node.children.length, 2);
    });
  });

  group('RestartBanner (row 183)', () {
    test('shows reason with restart/later actions', () {
      final node =
          RestartBanner(reason: 'update installed').build() as UiRow;
      expect((node.children[0] as UiText).text, contains('update installed'));
      expect((node.children[1] as UiButton).label, 'Restart Now');
    });

    test('dismissed banner renders empty', () {
      final node = RestartBanner(reason: 'x', dismissed: true).build() as UiRow;
      expect(node.children, isEmpty);
    });
  });

  group('AgentTuiTheme (row 184)', () {
    test('uses agent background color when known', () {
      final theme =
          AgentTuiTheme(agentId: 'claude', backgroundHex: '#2b1e1e');
      expect(theme.chromeBackground, '#2b1e1e');
    });

    test('falls back to default surface for unknown agents', () {
      final theme = AgentTuiTheme(agentId: 'unknown');
      expect(theme.chromeBackground, '#1e1e1e');
    });
  });
}
