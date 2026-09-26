/// Tests for the ported macOS screens and app-kit widgets.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/models.dart';
import 'package:supercli_app/screens/screens.dart';
import 'package:supercli_app/widgets/widgets.dart';
import 'package:test/test.dart';

void main() {
  group('RootView', () {
    test('builds sidebar + content layout', () {
      final sidebar = SidebarView(sections: const []);
      final content = TerminalArea(panes: []);
      final root = RootView(sidebar: sidebar, content: content);
      final node = root.build();
      expect(node, isA<UiRow>());
      expect((node as UiRow).children.length, 2);
    });

    test('collapsed sidebar shows expand button', () {
      final sidebar = SidebarView(sections: const []);
      final content = TerminalArea(panes: []);
      final root = RootView(
          sidebar: sidebar, content: content, sidebarCollapsed: true);
      final node = root.build() as UiRow;
      expect(node.children.first, isA<UiColumn>());
    });
  });

  group('SidebarView', () {
    test('builds filter + new session + sections', () {
      final sidebar = SidebarView(sections: [
        SidebarSection(
          id: 'active',
          title: 'Active',
          sessions: [
            SessionSummary(
                id: 's1',
                title: 'api-server',
                updatedAt: DateTime.now()),
          ],
        ),
      ]);
      final node = sidebar.build() as UiColumn;
      // filter input + new session button + section title + table
      expect(node.children.length, 4);
      expect(node.children[0], isA<UiInput>());
      expect(node.children[1], isA<UiButton>());
    });
  });

  group('McpApprovalPanel', () {
    test('builds approval card with allow/deny', () {
      final approval = PendingApproval(
        id: 'a1',
        tool: 'tool',
        summary: 'Write file',
        detail: 'src/main.rs',
      );
      final panel = McpApprovalPanel(approval: approval);
      final node = panel.build() as UiColumn;
      expect(node.children.length, 3);
      final buttons = node.children.last as UiRow;
      expect(buttons.children.length, 2);
    });

    test('shows more-waiting count', () {
      final approval = PendingApproval(
        id: 'a1',
        tool: 'tool',
        summary: 'Write file',
        detail: '',
      );
      final panel = McpApprovalPanel(approval: approval, moreWaiting: 2);
      final node = panel.build() as UiColumn;
      expect(node.children.length, 4);
    });

    test('has keyboard actions', () {
      final approval = PendingApproval(
        id: 'a1',
        tool: 'tool',
        summary: 'x',
        detail: '',
      );
      final panel = McpApprovalPanel(approval: approval);
      final actions = panel.actions();
      expect(actions.map((a) => a.name),
          containsAll(['mcp.approve', 'mcp.deny']));
    });
  });

  group('TerminalPaneView', () {
    test('builds pane with header and lines', () {
      final pane = TerminalPaneView(
        paneId: 'p1',
        title: 'zsh',
        lines: const ['\$ ls', 'src/'],
      );
      final node = pane.build() as UiColumn;
      expect(node.children.length, 2);
      expect(node.children[0], isA<UiRow>());
      expect(node.children[1], isA<UiColumn>());
    });

    test('find bar renders when visible', () {
      final pane = TerminalPaneView(
        paneId: 'p1',
        title: 'zsh',
        findBarVisible: true,
      );
      final node = pane.build() as UiColumn;
      expect(node.children.length, 3);
    });
  });

  group('SettingsView', () {
    test('builds tab row with all tabs', () {
      final settings = SettingsView();
      final node = settings.build() as UiRow;
      expect(node.children.length, 2);
      final tabs = node.children[0] as UiColumn;
      expect(tabs.children.length, SettingsTab.values.length);
    });
  });

  group('HostPickerView', () {
    test('shows pairing code as text (P0-11 gap)', () {
      final picker = HostPickerView(
        pairingCode: 'ABC123',
        selectedHostName: 'mbp',
      );
      final node = picker.build() as UiColumn;
      // Last child is the pairing sheet.
      final sheet = node.children.last as UiColumn;
      final codeText =
          sheet.children.whereType<UiText>().map((t) => t.text).join(' ');
      expect(codeText, contains('ABC123'));
    });

    test('nearby dataset has host rows', () {
      final picker = HostPickerView(hosts: const [
        HostEntry(id: 'h1', name: 'mbp', address: '192.168.1.2'),
      ]);
      final ds = picker.nearbyDataset();
      expect(ds.rowCount, 1);
      expect(ds.row(0)[0], 'mbp');
    });
  });

  group('CommandPaletteView', () {
    test('dataset filters by query', () {
      final palette = CommandPaletteView(
        commands: const [
          PaletteCommand(id: 'c1', title: 'New Session'),
          PaletteCommand(id: 'c2', title: 'Open Settings'),
        ],
        filter: 'session',
      );
      final ds = palette.dataset();
      expect(ds.rowCount, 1);
      expect(ds.row(0)[0], 'New Session');
    });
  });

  group('app-kit widgets', () {
    test('TreeView renders nested nodes', () {
      final tree = TreeView(nodes: const [
        TreeNode(
          id: 'src',
          label: 'src',
          expanded: true,
          children: [
            TreeNode(id: 'src/main.rs', label: 'main.rs'),
          ],
        ),
      ]);
      final node = tree.build() as UiColumn;
      expect(node.children.length, 1);
    });

    test('PageView shows current page with nav', () {
      final page = PageView(
        pages: const [
          UiText('p1', 'Page 1'),
          UiText('p2', 'Page 2'),
        ],
        currentPage: 1,
      );
      final node = page.build() as UiColumn;
      expect(node.children.length, 2);
      final indicator = (node.children[1] as UiRow).children[1] as UiText;
      expect(indicator.text, '2 / 2');
    });

    test('SemanticMenuView groups actions', () {
      final menu = SemanticMenuView(groups: const [
        SemanticMenuGroup(title: 'File', items: ['Open', 'Save']),
      ]);
      final node = menu.build() as UiColumn;
      expect(node.children.length, 3); // title + 2 buttons
    });

    test('ListNavigation has keyboard actions', () {
      final list = ListNavigation(items: const ['a', 'b']);
      final actions = list.actions();
      expect(actions.map((a) => a.name),
          containsAll(['list.up', 'list.down']));
    });
  });
}
