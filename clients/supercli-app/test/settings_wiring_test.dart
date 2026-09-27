/// Tests for settings wiring: SettingsController mounted in the app shell,
/// settings overlay open/close, tab switching, and toggle persistence.
///
/// These verify the REAL wiring (not just component existence):
/// - SupercliApp holds a SettingsController slot (null until the Host
///   handshake; the app entry point sets it)
/// - settings.open/settings.close actions toggle the overlay via handleAction
/// - Tab buttons (`settings-tab-<name>` clicks) switch activeSettingsTab
/// - Toggle buttons (`<id>-toggle` clicks) flip AppSettings fields and
///   schedule a debounced Host save through the controller
/// - The settings overlay node is mounted in build() only when open
library;

import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:supercli_app/app.dart';
import 'package:supercli_app/host_client.dart';
import 'package:supercli_app/keymap.dart';
import 'package:supercli_app/screens/sidebarview.dart';
import 'package:supercli_app/screens/settings_controller.dart';
import 'package:supercli_app/screens/settingsview.dart';
import 'package:test/test.dart';

/// In-memory fake of the Host's workspace-settings store.
final class FakeSettingsHost {
  Map<String, dynamic> store = {};
  int setCalls = 0;
  Map<String, dynamic> lastSetBody = {};

  MockClient get mock => MockClient((request) async {
    if (request.method == 'POST' &&
        request.url.path == '/mobile/workspace-settings') {
      setCalls++;
      lastSetBody = jsonDecode(request.body) as Map<String, dynamic>;
      return http.Response('{"ok":true}', 200);
    }
    if (request.method == 'GET' &&
        request.url.path == '/mobile/workspace-settings') {
      return http.Response(jsonEncode(store), 200);
    }
    return http.Response('not found', 404);
  });

  HostClient client() =>
      HostClient(baseUrl: Uri.parse('http://127.0.0.1:8137'), httpClient: mock);
}

/// Recursively collect node ids from a UiNode JSON tree.
Set<String> collectIds(Map<String, Object?> json) {
  final ids = <String>{};
  void walk(Object? node) {
    if (node is Map<String, Object?>) {
      final id = node['id'];
      if (id is String) ids.add(id);
      final children = node['children'];
      if (children is List) {
        for (final child in children) {
          walk(child);
        }
      }
    }
  }

  walk(json);
  return ids;
}

SupercliApp appWithController(FakeSettingsHost fake) {
  final app = SupercliApp();
  app.settingsController = SettingsController(
    host: fake.client(),
    debounce: Duration.zero,
  );
  return app;
}

void main() {
  group('SettingsController slot', () {
    test('SupercliApp starts with no controller and closed overlay', () {
      final app = SupercliApp();
      expect(app.settingsController, isNull);
      expect(app.settingsOpen, isFalse);
      expect(app.activeSettingsTab, SettingsTab.general);
    });

    test('settings.open is a no-op until the controller is set', () {
      final app = SupercliApp();
      expect(app.handleAction('settings.open'), isTrue);
      expect(
        app.settingsOpen,
        isFalse,
        reason: 'must not open without a controller',
      );
    });

    test('settings.open/settings.close toggle the overlay', () {
      final app = appWithController(FakeSettingsHost());
      expect(app.handleAction('settings.open'), isTrue);
      expect(app.settingsOpen, isTrue);
      expect(app.handleAction('settings.close'), isTrue);
      expect(app.settingsOpen, isFalse);
    });

    test('unknown settings action is not consumed', () {
      final app = appWithController(FakeSettingsHost());
      expect(app.handleAction('settings.bogus'), isFalse);
    });
  });

  group('Settings tab switching', () {
    test('settings.tab.<name> action switches tabs', () {
      final app = appWithController(FakeSettingsHost());
      expect(app.handleAction('settings.tab.plugins'), isTrue);
      expect(app.activeSettingsTab, SettingsTab.plugins);
      expect(app.handleAction('settings.tab.license'), isTrue);
      expect(app.activeSettingsTab, SettingsTab.license);
    });

    test('settings.tab.<unknown> is not consumed', () {
      final app = appWithController(FakeSettingsHost());
      expect(app.handleAction('settings.tab.nope'), isFalse);
      expect(app.activeSettingsTab, SettingsTab.general);
    });

    test('clicking a tab button switches tabs', () {
      final app = appWithController(FakeSettingsHost());
      expect(app.handleClick('settings-tab-agentAccess'), isTrue);
      expect(app.activeSettingsTab, SettingsTab.agentAccess);
      expect(app.handleClick('settings-tab-remote'), isTrue);
      expect(app.activeSettingsTab, SettingsTab.remote);
    });

    test('clicking an unknown tab button is not consumed', () {
      final app = appWithController(FakeSettingsHost());
      expect(app.handleClick('settings-tab-nope'), isFalse);
    });

    test('all SettingsTab values have titles', () {
      for (final tab in SettingsTab.values) {
        expect(
          SettingsView.tabTitles[tab],
          isNotNull,
          reason: 'Missing title for $tab',
        );
        expect(SettingsView.tabTitles[tab]!.isNotEmpty, isTrue);
      }
    });
  });

  group('Settings toggles', () {
    test('toggle click flips the AppSettings field', () {
      final app = appWithController(FakeSettingsHost());
      final settings = app.settingsController!.settings;
      final before = settings.worktreeAccess;
      expect(app.handleClick('worktree-access-toggle'), isTrue);
      expect(settings.worktreeAccess, isNot(before));
    });

    test('toggleSetting maps every documented toggle id', () {
      final app = appWithController(FakeSettingsHost());
      const ids = [
        'worktree-access',
        'agent-worktree-permission',
        'auto-gallery',
        'agent-auto-gallery',
        'sessions-mcp',
        'feat-remote-ws',
        'feat-git-worktrees',
        'feat-browser-mcp',
        'browser-mcp',
        'feat-auto-screenshots',
        'browser-auto-screenshots',
        'transcript-content',
        'notify-completion',
        'notify-flags',
        'adv-show-worktrees',
        'show-agent-worktrees',
        'adv-trace-log',
      ];
      for (final id in ids) {
        expect(app.toggleSetting(id), isTrue, reason: 'toggle id: $id');
      }
      expect(app.toggleSetting('not-a-toggle'), isFalse);
    });

    test('toggle is a no-op without a controller', () {
      final app = SupercliApp();
      expect(app.toggleSetting('worktree-access'), isFalse);
    });

    test('toggle schedules a Host save', () async {
      final fake = FakeSettingsHost();
      final app = appWithController(fake);
      app.toggleSetting('notify-completion');
      // Debounce is zero in this harness; let the timer fire.
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(fake.setCalls, 1);
      expect(app.settingsController!.settings.notifyOnCompletion, isFalse);
    });
  });

  group('Settings overlay mount', () {
    test('settings node is absent when closed', () {
      final app = appWithController(FakeSettingsHost());
      final ids = collectIds(app.build().toJson().cast<String, Object?>());
      expect(ids.contains('settings'), isFalse);
    });

    test('settings node is mounted when open with a controller', () {
      final app = appWithController(FakeSettingsHost());
      app.handleAction('settings.open');
      final ids = collectIds(app.build().toJson().cast<String, Object?>());
      expect(ids.contains('settings'), isTrue);
      expect(ids.contains('settings-tabs'), isTrue);
      expect(ids.contains('settings-content'), isTrue);
    });

    test('active tab renders its panel', () {
      final app = appWithController(FakeSettingsHost());
      app.handleAction('settings.open');
      app.handleAction('settings.tab.plugins');
      final ids = collectIds(app.build().toJson().cast<String, Object?>());
      expect(ids.contains('settings'), isTrue);
    });
  });

  group('Settings actions', () {
    test('settings.open is NOT registered natively (punctuation gap)', () {
      final app = appWithController(FakeSettingsHost());
      final open = app
          .actions()
          .where((a) => a.name == 'settings.open')
          .toList();
      // gpuidart's native key parser rejects punctuation keys (see
      // docs/gpuidart-gaps-keys.md), so the canonical Cmd-,/Ctrl-, chord
      // (Keymap.settings) is NOT registered natively. Settings stays
      // reachable from the command palette, which dispatches the action
      // name directly through handleAction.
      expect(open, isEmpty);
      expect(Keymap.settings(), contains(','));
    });

    test('settings.open is reachable from the command palette', () {
      final app = appWithController(FakeSettingsHost());
      final entry = app
          .paletteCommands()
          .where((c) => c.id == 'action:settings.open')
          .toList();
      expect(entry, hasLength(1));
      expect(entry.single.title, 'Open settings');
      // The palette shows the canonical chord for documentation.
      expect(entry.single.shortcut, Keymap.settings());
      // And the palette selection dispatches straight to handleAction.
      expect(app.executePaletteCommand(entry.single), 'settings.open');
      expect(app.handleAction('settings.open'), isTrue);
      expect(app.settingsOpen, isTrue);
    });

    test('settings.close appears only while the overlay is open', () {
      final app = appWithController(FakeSettingsHost());
      expect(app.actions().where((a) => a.name == 'settings.close'), isEmpty);
      app.handleAction('settings.open');
      final close = app
          .actions()
          .where((a) => a.name == 'settings.close')
          .toList();
      expect(close, hasLength(1));
      expect(close.single.keys, 'escape');
    });

    test('settings is reachable from the sidebar menu button', () {
      final app = appWithController(FakeSettingsHost());
      // The menu button is mounted in the sidebar.
      final sidebarIds = collectIds(
        SidebarView(
          workspaces: const ['local'],
          activeWorkspaceId: 'local',
          projects: const [],
          selectedSessionId: null,
        ).build().toJson().cast<String, Object?>(),
      );
      expect(sidebarIds.contains('open-settings'), isTrue);
      // Clicking it opens Settings through the menu path.
      expect(app.settingsOpen, isFalse);
      expect(app.handleClick('open-settings'), isTrue);
      expect(app.settingsOpen, isTrue);
      // And the settings overlay mounts with the controller.
      final appIds = collectIds(app.build().toJson().cast<String, Object?>());
      expect(appIds.contains('settings'), isTrue);
    });

    test('menu settings button is a no-op without a controller', () {
      final app = SupercliApp();
      expect(app.settingsController, isNull);
      expect(app.handleClick('open-settings'), isFalse);
      expect(app.settingsOpen, isFalse);
    });
  });
}
