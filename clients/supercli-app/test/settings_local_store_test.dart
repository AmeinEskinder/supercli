/// Tests for local settings persistence: the desktop-only fields (theme,
/// appearance, notifications, advanced prefs) survive app restarts through
/// the app's own config file, and the Host remains authoritative for its
/// own fields.
library;

import 'dart:convert';
import 'dart:io';

import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:supercli_app/host_client.dart';
import 'package:supercli_app/screens/settingspanels.dart';
import 'package:supercli_app/screens/settings_controller.dart';
import 'package:supercli_app/screens/settings_local_store.dart';
import 'package:test/test.dart';

/// In-memory local store shared across controller "restarts".
final class MemorySettingsLocalStore implements SettingsLocalStore {
  Map<String, Object?> snapshot = {};
  bool failOnSave = false;
  int saveCalls = 0;

  @override
  Future<Map<String, Object?>> load() async => Map.of(snapshot);

  @override
  Future<void> save(Map<String, Object?> json) async {
    saveCalls++;
    if (failOnSave) throw const FileSystemException('disk full');
    snapshot = Map.of(json);
  }
}

MockClient _hostMock(Map<String, dynamic> store) {
  return MockClient((request) async {
    if (request.method == 'GET' &&
        request.url.path == '/mobile/workspace-settings') {
      return http.Response(jsonEncode(store), 200);
    }
    if (request.method == 'POST' &&
        request.url.path == '/mobile/workspace-settings') {
      return http.Response('{"ok":true}', 200);
    }
    return http.Response('not found', 404);
  });
}

void main() {
  group('FileSettingsLocalStore', () {
    late Directory tmp;

    setUp(() async {
      tmp = await Directory.systemTemp.createTemp('settings-local-test');
    });

    tearDown(() async {
      await tmp.delete(recursive: true);
    });

    test('load returns empty map when the file is missing', () async {
      final store = FileSettingsLocalStore('${tmp.path}/nested/settings.json');
      expect(await store.load(), isEmpty);
    });

    test('load returns empty map on corrupt JSON', () async {
      final path = '${tmp.path}/settings.json';
      await File(path).writeAsString('{not valid json');
      final store = FileSettingsLocalStore(path);
      expect(await store.load(), isEmpty);
    });

    test('load returns empty map on non-object JSON', () async {
      final path = '${tmp.path}/settings.json';
      await File(path).writeAsString('[1,2,3]');
      final store = FileSettingsLocalStore(path);
      expect(await store.load(), isEmpty);
    });

    test('save then load round-trips, creating parent dirs', () async {
      final store = FileSettingsLocalStore(
        '${tmp.path}/deep/nested/settings.json',
      );
      await store.save({'theme': 'dark', 'accentColor': 3});
      final loaded = await store.load();
      expect(loaded['theme'], 'dark');
      expect(loaded['accentColor'], 3);
    });

    test('save leaves no torn temp file behind', () async {
      final path = '${tmp.path}/settings.json';
      final store = FileSettingsLocalStore(path);
      await store.save({'a': 1});
      final leftovers = tmp
          .listSync()
          .where((e) => e.path.contains('.tmp.'))
          .toList();
      expect(leftovers, isEmpty);
      expect(jsonDecode(await File(path).readAsString()), {'a': 1});
    });
  });

  group('AppSettings local snapshot', () {
    test('toLocalJson/applyLocalJson round-trips every field', () {
      final original = AppSettings(
        scope: SettingsScope.workspace,
        theme: ThemeMode.dark,
        accentColor: 5,
        terminalFont: 'JetBrains Mono',
        terminalFontSize: 14.5,
        lineHeight: 1.4,
        writePolicy: WritePolicy.deny,
        worktreeAccess: true,
        autoGallery: false,
        autoStopArchiveMinutes: 240,
        sidebarStoppedLimit: 25,
        browserDefaultAccess: BrowserDefaultAccess.off,
        browserMcp: true,
        sessionsMcp: false,
        autoAddBrowserScreenshots: true,
        remoteWorkspaces: false,
        gitWorktrees: false,
        notifyOnCompletion: false,
        notifyFlags: false,
        transcriptContentEnabled: false,
        showAgentWorktrees: true,
        sessionsFolder: '/tmp/sessions',
        traceLog: true,
      );
      final restored = AppSettings();
      restored.applyLocalJson(original.toLocalJson());

      expect(restored.scope, SettingsScope.workspace);
      expect(restored.theme, ThemeMode.dark);
      expect(restored.accentColor, 5);
      expect(restored.terminalFont, 'JetBrains Mono');
      expect(restored.terminalFontSize, 14.5);
      expect(restored.lineHeight, 1.4);
      expect(restored.writePolicy, WritePolicy.deny);
      expect(restored.worktreeAccess, true);
      expect(restored.autoGallery, false);
      expect(restored.autoStopArchiveMinutes, 240);
      expect(restored.sidebarStoppedLimit, 25);
      expect(restored.browserDefaultAccess, BrowserDefaultAccess.off);
      expect(restored.browserMcp, true);
      expect(restored.sessionsMcp, false);
      expect(restored.autoAddBrowserScreenshots, true);
      expect(restored.remoteWorkspaces, false);
      expect(restored.gitWorktrees, false);
      expect(restored.notifyOnCompletion, false);
      expect(restored.notifyFlags, false);
      expect(restored.transcriptContentEnabled, false);
      expect(restored.showAgentWorktrees, true);
      expect(restored.sessionsFolder, '/tmp/sessions');
      expect(restored.traceLog, true);
    });

    test('applyLocalJson tolerates corrupt and partial snapshots', () {
      final settings = AppSettings();
      settings.applyLocalJson({
        'theme': 'neon', // unknown enum value
        'accentColor': 'three', // wrong type
        'terminalFontSize': 15, // int where double expected
        // everything else missing
      });
      expect(settings.theme, ThemeMode.system); // unchanged
      expect(settings.accentColor, 0); // unchanged
      expect(settings.terminalFontSize, 15.0); // num accepted
    });
  });

  group('SettingsController with localStore', () {
    test(
      'local-only fields persist across controller reconstruction',
      () async {
        // The real Host always returns a full settings object; an empty store
        // here means "Host defaults", like a fresh Host.
        final host = HostClient(
          baseUrl: Uri.parse('http://127.0.0.1:8137'),
          httpClient: _hostMock({
            'appearanceSettings': {'theme': 'system'},
          }),
        );
        final store = MemorySettingsLocalStore();

        final first = SettingsController(
          host: host,
          localStore: store,
          debounce: Duration.zero,
        );
        first.settings.theme = ThemeMode.dark;
        first.settings.accentColor = 4;
        first.settings.terminalFont = 'Fira Code';
        first.settings.notifyOnCompletion = false;
        first.edited();
        await Future<void>.delayed(const Duration(milliseconds: 20));
        expect(store.saveCalls, 1);
        // The snapshot on disk carries the local-only fields (and the theme
        // the user chose, for offline use).
        expect(store.snapshot['accentColor'], 4);
        expect(store.snapshot['terminalFont'], 'Fira Code');
        expect(store.snapshot['theme'], 'dark');
        first.dispose();

        // A fresh controller (simulated restart) restores the local-only
        // fields even though the Host knows nothing about them. The Host
        // is authoritative for its own fields only; theme is local-only, so
        // the local snapshot wins even though the Host says 'system'.
        final second = SettingsController(host: host, localStore: store);
        await second.load();
        expect(second.settings.accentColor, 4);
        expect(second.settings.terminalFont, 'Fira Code');
        expect(second.settings.notifyOnCompletion, false);
        expect(second.settings.theme, ThemeMode.dark);
        second.dispose();
      },
    );

    test(
      'Host overrides Host-owned fields, preserves local-only fields',
      () async {
        final host = HostClient(
          baseUrl: Uri.parse('http://127.0.0.1:8137'),
          httpClient: _hostMock({
            'autoStopArchiveMinutes': 480,
            'appearanceSettings': {'theme': 'light'},
          }),
        );
        final store = MemorySettingsLocalStore();
        store.snapshot = {
          'theme': 'dark', // local-only: local wins over Host
          'accentColor': 6, // local-only: local wins
          'autoStopArchiveMinutes': 60,
        };

        final controller = SettingsController(host: host, localStore: store);
        await controller.load();
        expect(controller.settings.theme, ThemeMode.dark);
        expect(controller.settings.autoStopArchiveMinutes, 480);
        expect(controller.settings.accentColor, 6);
        controller.dispose();
      },
    );

    test('local snapshot keeps working when the Host is unreachable', () async {
      final host = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: MockClient(
          (_) async => throw http.ClientException('offline'),
        ),
      );
      final store = MemorySettingsLocalStore();
      store.snapshot = {'theme': 'dark', 'accentColor': 2};
      final errors = <String>[];

      final controller = SettingsController(
        host: host,
        localStore: store,
        onError: errors.add,
      );
      await controller.load();
      expect(controller.settings.theme, ThemeMode.dark);
      expect(controller.settings.accentColor, 2);
      expect(errors, hasLength(1));
      expect(errors.single, contains('Could not load settings'));
      controller.dispose();
    });

    test('local save failure reports via onError without throwing', () async {
      final host = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: _hostMock({}),
      );
      final store = MemorySettingsLocalStore()..failOnSave = true;
      final errors = <String>[];

      final controller = SettingsController(
        host: host,
        localStore: store,
        onError: errors.add,
      );
      controller.settings.accentColor = 7;
      await controller.saveNow(); // must not throw
      expect(errors, hasLength(1));
      expect(errors.single, contains('Could not persist local settings'));
      // The in-memory model is never rolled back.
      expect(controller.settings.accentColor, 7);
      controller.dispose();
    });

    test('local edits persist even when the Host save fails', () async {
      final host = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: MockClient((request) async {
          return http.Response('down', 500);
        }),
      );
      final store = MemorySettingsLocalStore();
      final errors = <String>[];

      final controller = SettingsController(
        host: host,
        localStore: store,
        onError: errors.add,
      );
      controller.settings.accentColor = 3;
      await controller.saveNow();
      expect(errors.single, contains('Could not save settings'));
      // The offline edit is still captured locally.
      expect(store.snapshot['accentColor'], 3);
      controller.dispose();
    });

    test('toLocalJson never contains secret-like keys', () {
      final settings = AppSettings();
      final json = settings.toLocalJson();
      final secretPattern = RegExp(
        r'token|password|credential|secret|api[_-]?key|private[_-]?key|auth',
        caseSensitive: false,
      );
      for (final key in json.keys) {
        expect(
          secretPattern.hasMatch(key),
          isFalse,
          reason: 'local store key "$key" looks like a secret',
        );
      }
    });
  });
}
