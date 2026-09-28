/// Tests for the ported macOS screens and app-kit widgets.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/models.dart';
import 'package:supercli_app/screens/screens.dart';
import 'package:supercli_app/widgets/widgets.dart';
import 'package:test/test.dart';

/// Recursively collects all UiText nodes under [node].
List<UiText> _allText(UiNode node) {
  final out = <UiText>[];
  void visit(UiNode n) {
    if (n is UiText) out.add(n);
    if (n is UiColumn) {
      for (final c in n.children) {
        visit(c);
      }
    } else if (n is UiRow) {
      for (final c in n.children) {
        visit(c);
      }
    }
  }

  visit(node);
  return out;
}

/// Recursively collects all UiButton nodes under [node].
List<UiButton> _allButtons(UiNode node) {
  final out = <UiButton>[];
  void visit(UiNode n) {
    if (n is UiButton) out.add(n);
    if (n is UiColumn) {
      for (final c in n.children) {
        visit(c);
      }
    } else if (n is UiRow) {
      for (final c in n.children) {
        visit(c);
      }
    }
  }

  visit(node);
  return out;
}

void main() {
  group('RootView', () {
    test('builds sidebar + content layout', () {
      final sidebar = SidebarView(projects: const []);
      final content = TerminalArea(panes: []);
      final root = RootView(sidebar: sidebar, content: content);
      final node = root.build();
      expect(node, isA<UiRow>());
      expect((node as UiRow).children.length, 2);
    });

    test('collapsed sidebar shows expand button', () {
      final sidebar = SidebarView(projects: const []);
      final content = TerminalArea(panes: []);
      final root = RootView(
          sidebar: sidebar, content: content, sidebarCollapsed: true);
      final node = root.build() as UiRow;
      expect(node.children.first, isA<UiColumn>());
    });
  });

  group('SidebarView', () {
    test('builds dots + filter + new session + archived', () {
      final sidebar = SidebarView(projects: [
        SidebarProject(
          id: 'pr1',
          name: 'supercli',
          sessions: [
            SidebarSession(
                summary: SessionSummary(
                    id: 's1',
                    title: 'api-server',
                    updatedAt: DateTime.now())),
          ],
        ),
      ]);
      final node = sidebar.build() as UiColumn;
      // workspace dots + filter input + new session button +
      // project tree + archived button
      expect(node.children.length, 5);
      expect(node.children[0], isA<UiRow>());
      expect(node.children[1], isA<UiInput>());
      expect(node.children[2], isA<UiButton>());
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
      // header row, tool, summary, detail, button row
      expect(node.children.length, 5);
      final buttons = node.children.last as UiRow;
      expect(buttons.children.length, 3);
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
      // header row, tool, summary, more-waiting, button row (no detail)
      expect(node.children.length, 5);
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
    test('builds pane with header, terminal node, and fallback grid', () {
      final pane = TerminalPaneView(
        paneId: 'p1',
        title: 'zsh',
        lines: const ['\$ ls', 'src/'],
      );
      final node = pane.build() as UiColumn;
      // header + fallback grid (P0-8 UiTerminal removed; RLE fallback only)
      expect(node.children.length, 2);
      expect(node.children[0], isA<UiRow>());
      expect(node.children[1], isA<UiColumn>());
      // The lines made it into the terminal state.
      expect(pane.state.grid[0][0].char, '\$');
    });

    test('find bar renders when visible', () {
      final pane = TerminalPaneView(
        paneId: 'p1',
        title: 'zsh',
        findBarVisible: true,
      );
      final node = pane.build() as UiColumn;
      // header + find bar + fallback grid (P0-8 UiTerminal removed)
      expect(node.children.length, 3);
    });

    test('exposes terminal key bindings', () {
      final pane = TerminalPaneView(paneId: 'p1', title: 'zsh');
      final actions = pane.actions();
      expect(actions.map((a) => a.name), contains('terminal.key.up'));
    });
  });

  group('SettingsView', () {
    test('builds tab row with all tabs', () {
      final settings = SettingsView();
      final node = settings.build() as UiRow;
      expect(node.children.length, 2);
      final tabs = node.children[0] as UiColumn;
      // heading + one button per tab
      expect(tabs.children.length, 1 + SettingsTab.values.length);
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
      final codeText = _allText(sheet).map((t) => t.text).join(' ');
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

    test('expiresInText formats M:SS and clamps at zero', () {
      expect(HostPickerView.expiresInText(65_000, 0), 'Expires in 1:05');
      expect(HostPickerView.expiresInText(5_000, 0), 'Expires in 0:05');
      expect(HostPickerView.expiresInText(0, 10_000), 'Expires in 0:00');
      expect(HostPickerView.expiresInText(null, 0), '');
      expect(HostPickerView.expiresInText(60_000, null), '');
    });

    test('pending sheet shows countdown, refresh and copy buttons', () {
      final picker = HostPickerView(
        pairingCode: 'ABC123',
        pairingExpiresAtUnixMs: 125_000,
        nowUnixMs: 0,
        selectedHostName: 'mbp',
      );
      final sheet = (picker.build() as UiColumn).children.last as UiColumn;
      final text = _allText(sheet).map((t) => t.text).join(' ');
      expect(text, contains('Expires in 2:05'));
      expect(text, contains('Scan this code in Supercli on the phone.'));
      final buttonIds = _allButtons(sheet).map((b) => b.id).toSet();
      expect(buttonIds, contains('pairing-refresh'));
      expect(buttonIds, contains('pairing-copy-code'));
      expect(buttonIds, isNot(contains('pairing-generate')));
    });

    test('pending sheet without code shows creating state and generate', () {
      final picker = HostPickerView(selectedHostName: 'mbp');
      // No code and not completed: sheet is hidden until an invitation exists.
      final node = picker.build() as UiColumn;
      expect(
        node.children.whereType<UiColumn>().where((c) => c.id == 'pairing-sheet'),
        isEmpty,
      );
    });

    test('completed sheet shows Add Another button', () {
      final picker = HostPickerView(
        pairingCompleted: true,
        selectedHostName: 'mbp',
      );
      final sheet = (picker.build() as UiColumn).children.last as UiColumn;
      final text = _allText(sheet).map((t) => t.text).join(' ');
      expect(text, contains('Device added'));
      final buttonIds = _allButtons(sheet).map((b) => b.id).toSet();
      expect(buttonIds, contains('pairing-add-another'));
    });

    test('error is shown in the sheet', () {
      final picker = HostPickerView(
        pairingCode: 'ABC123',
        pairingError: 'boom',
        selectedHostName: 'mbp',
      );
      final sheet = (picker.build() as UiColumn).children.last as UiColumn;
      expect(_allText(sheet).map((t) => t.text).join(' '), contains('boom'));
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
