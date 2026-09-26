/// Behavior tests for the desktop parity batch (f), rows 151–173:
/// 155 (folder colors), 158 (open workspace in new window),
/// 159 (move project), 161 (local-site globe), 162 (Open in menu),
/// 166 (number-key switching), 170 (remote-host panes).
///
/// These exercise models, renderers and controllers — not just tree shape.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/app.dart';
import 'package:supercli_app/keybindings.dart';
import 'package:supercli_app/models.dart';
import 'package:supercli_app/screens/foldercolors.dart';
import 'package:supercli_app/screens/openinmenu.dart';
import 'package:supercli_app/screens/sidebarview.dart';
import 'package:supercli_app/screens/workspacemenus.dart';
import 'package:supercli_app/screens/workspacessettingspanel.dart';
import 'package:supercli_app/terminal/remote_pane.dart';
import 'package:test/test.dart';

SessionSummary summary(String id, String title) =>
    SessionSummary(id: id, title: title, updatedAt: DateTime.utc(2026, 9, 26));

/// Collect all UiText texts in a node tree (depth-first).
List<String> textsOf(UiNode node) {
  final out = <String>[];
  void walk(UiNode n) {
    if (n is UiText) out.add(n.text);
    if (n is UiColumn) {
      for (final c in n.children) {
        walk(c);
      }
    }
    if (n is UiRow) {
      for (final c in n.children) {
        walk(c);
      }
    }
  }

  walk(node);
  return out;
}

/// Collect all UiButton labels in a node tree (depth-first).
List<String> buttonsOf(UiNode node) {
  final out = <String>[];
  void walk(UiNode n) {
    if (n is UiButton) out.add(n.label);
    if (n is UiColumn) {
      for (final c in n.children) {
        walk(c);
      }
    }
    if (n is UiRow) {
      for (final c in n.children) {
        walk(c);
      }
    }
  }

  walk(node);
  return out;
}

void main() {
  group('row 155: folder color palette', () {
    test('has exactly 8 colors with unique hex values', () {
      expect(folderColors.length, 8);
      final hexes = folderColors.map((c) => c.hex).toSet();
      expect(hexes.length, 8);
      for (final c in folderColors) {
        expect(c.hex, matches(RegExp(r'^#[0-9A-F]{6}$')));
      }
    });

    test('palette renders 8 swatches + None', () {
      final node = const FolderColorPalette().build();
      final buttons = buttonsOf(node);
      expect(buttons.length, 9);
      expect(buttons.last, contains('None'));
    });

    test('selected color is marked', () {
      final node = const FolderColorPalette(selected: '#E5484D').build();
      final buttons = buttonsOf(node);
      expect(buttons.where((b) => b.contains('✓')).length, 1);
      expect(buttons.firstWhere((b) => b.contains('✓')), contains('Red'));
    });

    test('colorForAction decodes swatch and clear buttons', () {
      expect(
        FolderColorPalette.colorForAction('folder-color-#3E63DD'),
        '#3E63DD',
      );
      expect(FolderColorPalette.colorForAction('folder-color-none'), isNull);
      expect(FolderColorPalette.colorForAction('nope'), isNull);
      expect(
        FolderColorPalette.isPaletteAction('folder-color-#3E63DD'),
        isTrue,
      );
      expect(FolderColorPalette.isPaletteAction('nope'), isFalse);
    });

    test('withFolderColor / withColor copy models', () {
      const p = SidebarProject(id: 'p', name: 'n');
      final p2 = p.withFolderColor('#30A46C');
      expect(p2.folderColor, '#30A46C');
      expect(p.folderColor, isNull);
      final cleared = p2.withFolderColor(null);
      expect(cleared.folderColor, isNull);

      const g = SidebarGroup(id: 'g', title: 't');
      expect(g.withColor('#8E4EC6').color, '#8E4EC6');
    });

    test('withFolderColor preserves workspaceId', () {
      const p = SidebarProject(id: 'p', name: 'n', workspaceId: 'w1');
      expect(p.withFolderColor('#30A46C').workspaceId, 'w1');
    });
  });

  group('row 158: open workspace in new window', () {
    test('renders and fires the host payload', () {
      Map<String, String>? fired;
      const w = WorkspaceEntry(id: 'w1', name: 'Main');
      final comp = OpenWorkspaceInNewWindow(
        workspace: w,
        onAction: (p) => fired = p,
      );
      expect(
        buttonsOf(comp.build()).single,
        contains('Open “Main” in New Window'),
      );
      expect(comp.actionPayload(), {
        'action': 'workspace.openInNewWindow',
        'workspaceId': 'w1',
      });
      comp.fire();
      expect(fired, {
        'action': 'workspace.openInNewWindow',
        'workspaceId': 'w1',
      });
    });
  });

  group('row 159: move project between workspaces', () {
    test('destinations exclude the current workspace', () {
      const project = SidebarProject(id: 'p', name: 'n', workspaceId: 'w1');
      const menu = MoveProjectMenu(
        project: project,
        workspaces: [
          WorkspaceEntry(id: 'w1', name: 'One'),
          WorkspaceEntry(id: 'w2', name: 'Two'),
        ],
      );
      expect(menu.destinations.map((w) => w.id), ['w2']);
      expect(buttonsOf(menu.build()), contains('Two'));
    });

    test('movedTo records the new workspace id', () {
      const p = SidebarProject(id: 'p', name: 'n', workspaceId: 'w1');
      final moved = p.movedTo('w2');
      expect(moved.workspaceId, 'w2');
      expect(moved.name, 'n');
      expect(moved.folderColor, p.folderColor);
    });

    test('moveAction payload', () {
      const menu = MoveProjectMenu(
        project: SidebarProject(id: 'p', name: 'n'),
      );
      expect(menu.moveAction('w9'), {
        'action': 'workspace.project.move',
        'projectId': 'p',
        'workspaceId': 'w9',
      });
    });

    test('empty destinations show guidance', () {
      const menu = MoveProjectMenu(
        project: SidebarProject(id: 'p', name: 'n', workspaceId: 'w1'),
        workspaces: [WorkspaceEntry(id: 'w1', name: 'One')],
      );
      expect(textsOf(menu.build()).join(' '), contains('No other workspaces'));
    });
  });

  group('row 161: local-site globe button', () {
    test('no site renders disabled globe only', () {
      final node = const LocalSiteGlobe().build();
      expect(buttonsOf(node), ['🌐 (no site)']);
      expect(const LocalSiteGlobe().hasSite, isFalse);
    });

    test('site renders open/copy/stop', () {
      const menu = LocalSiteGlobe(siteUrl: 'http://localhost:3000');
      expect(menu.hasSite, isTrue);
      final buttons = buttonsOf(menu.build());
      expect(buttons.any((b) => b.startsWith('Open http')), isTrue);
      expect(buttons, contains('Copy URL'));
      expect(buttons, contains('Stop server'));
      expect(menu.openAction(), {
        'action': 'site.open',
        'url': 'http://localhost:3000',
      });
      expect(menu.stopAction(), {'action': 'site.stop'});
    });
  });

  group('row 162: Open in menu', () {
    test('exactly 24 targets with unique ids', () {
      expect(openInTargets.length, 24);
      expect(openInTargets.map((t) => t.id).toSet().length, 24);
    });

    test('covers editors, terminals and git clients', () {
      final kinds = openInTargets.map((t) => t.kind).toSet();
      expect(kinds, containsAll(['editor', 'terminal', 'git']));
    });

    test('unavailable entries render disabled', () {
      const menu = OpenInMenu(cwd: '/tmp', available: {'vscode'});
      final buttons = buttonsOf(menu.build());
      expect(buttons, contains('Visual Studio Code'));
      expect(buttons.any((b) => b.contains('not installed')), isTrue);
      expect(OpenInMenu.isOpenInAction('openin-vscode'), isTrue);
      expect(OpenInMenu.isOpenInAction('openin-nope'), isFalse);
      expect(OpenInMenu.targetForAction('openin-kitty'), 'kitty');
      expect(menu.openAction('zed'), {
        'action': 'open-in',
        'target': 'zed',
        'cwd': '/tmp',
      });
    });
  });

  group('row 166: number-key session/project switching', () {
    test('keybinding chords', () {
      expect(AppKeybindings.sessionNumber(1), 'meta+1');
      expect(AppKeybindings.sessionNumber(9), 'meta+9');
      expect(AppKeybindings.projectNumber(1), 'ctrl+1');
      expect(AppKeybindings.projectNumber(9), 'ctrl+9');
    });

    test('globalActions registers 18 number bindings', () {
      final actions = const AppKeybindings().globalActions();
      final names = actions.map((a) => a.name).toSet();
      for (var n = 1; n <= 9; n++) {
        expect(names, contains('session.switch$n'));
        expect(names, contains('project.switch$n'));
      }
    });

    test('selectSessionByIndex selects and clamps', () {
      final app = SupercliApp()
        ..sessions = [summary('a', 'A'), summary('b', 'B')];
      app.selectSessionByIndex(2);
      expect(app.selectedSession, 1);
      app.selectSessionByIndex(9); // out of range: ignored
      expect(app.selectedSession, 1);
      app.selectSessionByIndex(0); // out of range: ignored
      expect(app.selectedSession, 1);
    });

    test('number hints render ⌘ badges on first 9 sessions', () {
      final view = SidebarView(
        projects: [
          SidebarProject(
            id: 'p',
            name: 'P',
            sessions: [
              SidebarSession(summary: summary('a', 'A')),
              SidebarSession(summary: summary('b', 'B')),
            ],
          ),
        ],
        showNumberHints: true,
      );
      final texts = textsOf(view.build());
      expect(texts, contains('⌘1'));
      expect(texts, contains('⌘2'));
    });

    test('hints hidden by default', () {
      final view = SidebarView(
        projects: [
          SidebarProject(
            id: 'p',
            name: 'P',
            sessions: [SidebarSession(summary: summary('a', 'A'))],
          ),
        ],
      );
      expect(textsOf(view.build()), isNot(contains('⌘1')));
    });
  });

  group('row 170: remote-host panes through the same terminal UI', () {
    test('feedOutput renders through the shared TerminalState', () {
      final pane = RemoteTerminalPane(
        paneId: 'pane-r1',
        hostName: 'buildbox',
        sessionId: 'rs-1',
      );
      pane.feedOutput('hello\r\n');
      expect(pane.state.grid[0].map((c) => c.char).join().trim(), 'hello');
    });

    test('view() builds a TerminalPaneView bound to the same state', () {
      final pane = RemoteTerminalPane(
        paneId: 'pane-r1',
        hostName: 'buildbox',
        sessionId: 'rs-1',
      );
      final view = pane.view();
      expect(view.paneId, 'pane-r1');
      expect(identical(view.state, pane.state), isTrue);
    });

    test('input routes to sendInput with the remote session id', () {
      String? gotSession;
      List<int>? gotBytes;
      final pane = RemoteTerminalPane(
        paneId: 'pane-r1',
        hostName: 'buildbox',
        sessionId: 'rs-1',
        sendInput: (sid, bytes) {
          gotSession = sid;
          gotBytes = bytes;
        },
      );
      expect(pane.isConnected, isTrue);
      pane.view().onInput?.call('ls\n'.codeUnits);
      expect(gotSession, 'rs-1');
      expect(gotBytes, 'ls\n'.codeUnits);
    });

    test('disconnected pane reports isConnected false', () {
      final pane = RemoteTerminalPane(
        paneId: 'pane-r1',
        hostName: 'buildbox',
        sessionId: 'rs-1',
      );
      expect(pane.isConnected, isFalse);
    });
  });

  group('app wiring', () {
    test('actions() includes the 18 number-key bindings', () {
      final app = SupercliApp()..sessions = [summary('a', 'A')];
      final names = app.actions().map((a) => a.name).toSet();
      expect(names, contains('session.switch1'));
      expect(names, contains('project.switch9'));
    });

    test('palette lists number-key commands', () {
      final app = SupercliApp();
      final cmds = app.paletteCommands();
      final byId = {for (final c in cmds) c.id: c};
      expect(byId['action:session.switch3']!.shortcut, 'meta+3');
      expect(byId['action:project.switch3']!.shortcut, 'ctrl+3');
    });
  });
}
