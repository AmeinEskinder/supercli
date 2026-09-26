/// Tests for SettingsController: Host-backed settings persistence.
library;

import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:supercli_app/host_client.dart';
import 'package:supercli_app/screens/settingspanels.dart';
import 'package:supercli_app/screens/settings_controller.dart';
import 'package:test/test.dart';

/// In-memory fake of the Host's workspace-settings store, shared across
/// "restarts" to prove persistence.
final class FakeSettingsHost {
  Map<String, dynamic> store = {};
  int setCalls = 0;

  MockClient get mock => MockClient((request) async {
        if (request.method == 'POST' &&
            request.url.path == '/mobile/workspace-settings') {
          setCalls++;
          final body = jsonDecode(request.body) as Map<String, dynamic>;
          // Merge like the real Host: nested objects merge, scalars replace.
          body.forEach((key, value) {
            if (value is Map && store[key] is Map) {
              store[key] = {...store[key] as Map, ...value};
            } else {
              store[key] = value;
            }
          });
          return http.Response('{"ok":true}', 200);
        }
        if (request.method == 'GET' &&
            request.url.path == '/mobile/workspace-settings') {
          return http.Response(jsonEncode(store), 200);
        }
        return http.Response('not found', 404);
      });

  HostClient client() => HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: mock,
      );
}

void main() {
  group('SettingsController', () {
    test('load populates settings from Host', () async {
      final fake = FakeSettingsHost();
      fake.store = {
        'autoStopArchiveMinutes': 240,
        'browserDefaultAccess': 'on',
        'experimentalSettings': {'sessionsMcp': false, 'browserMcp': true},
        'appearanceSettings': {'theme': 'dark'},
      };
      final controller = SettingsController(host: fake.client());
      await controller.load();
      expect(controller.settings.autoStopArchiveMinutes, 240);
      expect(controller.settings.browserDefaultAccess, BrowserDefaultAccess.on);
      expect(controller.settings.sessionsMcp, false);
      expect(controller.settings.browserMcp, true);
      expect(controller.settings.theme, ThemeMode.dark);
      controller.dispose();
    });

    test('load keeps defaults when Host store is empty', () async {
      final fake = FakeSettingsHost();
      final controller = SettingsController(host: fake.client());
      await controller.load();
      expect(controller.settings.autoStopArchiveMinutes, 60);
      expect(controller.settings.sessionsMcp, true);
      expect(controller.settings.theme, ThemeMode.system);
      controller.dispose();
    });

    test('settings persist across app restarts (set -> restart -> get)',
        () async {
      final fake = FakeSettingsHost();

      // First "app run": load defaults, change settings, save.
      final run1 = SettingsController(host: fake.client());
      await run1.load();
      run1.settings.autoStopArchiveMinutes = 480;
      run1.settings.browserDefaultAccess = BrowserDefaultAccess.off;
      run1.settings.sessionsMcp = false;
      run1.settings.theme = ThemeMode.dark;
      await run1.saveNow();
      run1.dispose();

      // Second "app run": fresh controller, fresh client, same Host store.
      final run2 = SettingsController(host: fake.client());
      await run2.load();
      expect(run2.settings.autoStopArchiveMinutes, 480);
      expect(run2.settings.browserDefaultAccess, BrowserDefaultAccess.off);
      expect(run2.settings.sessionsMcp, false);
      expect(run2.settings.theme, ThemeMode.dark);
      run2.dispose();
    });

    test('edited() debounces saves', () async {
      final fake = FakeSettingsHost();
      final controller = SettingsController(
        host: fake.client(),
        debounce: const Duration(milliseconds: 50),
      );
      await controller.load();

      // Rapid edits collapse into one save.
      controller.settings.autoStopArchiveMinutes = 30;
      controller.edited();
      controller.settings.autoStopArchiveMinutes = 120;
      controller.edited();
      controller.settings.autoStopArchiveMinutes = 240;
      controller.edited();

      // Not yet saved (debounce pending).
      expect(fake.setCalls, 0);
      await Future.delayed(const Duration(milliseconds: 150));
      expect(fake.setCalls, 1);
      expect(fake.store['autoStopArchiveMinutes'], 240);
      controller.dispose();
    });

    test('save error surfaces via onError, model keeps edits', () async {
      final errors = <String>[];
      final mock = MockClient((request) async {
        return http.Response('{"error":"disk full"}', 500);
      });
      final client = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: mock,
      );
      final controller = SettingsController(
        host: client,
        onError: errors.add,
      );
      controller.settings.autoStopArchiveMinutes = 120;
      await controller.saveNow();
      expect(errors, hasLength(1));
      expect(errors.first, contains('Could not save settings'));
      // The user's edit is preserved for the next retry.
      expect(controller.settings.autoStopArchiveMinutes, 120);
      controller.dispose();
      client.close();
    });

    test('load error surfaces via onError', () async {
      final errors = <String>[];
      final mock = MockClient((request) async {
        return http.Response('boom', 503);
      });
      final client = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: mock,
      );
      final controller = SettingsController(
        host: client,
        onError: errors.add,
      );
      await controller.load();
      expect(errors, hasLength(1));
      expect(errors.first, contains('Could not load settings'));
      controller.dispose();
      client.close();
    });

    test('dispose cancels pending save', () async {
      final fake = FakeSettingsHost();
      final controller = SettingsController(
        host: fake.client(),
        debounce: const Duration(milliseconds: 50),
      );
      controller.settings.autoStopArchiveMinutes = 30;
      controller.edited();
      controller.dispose();
      await Future.delayed(const Duration(milliseconds: 150));
      expect(fake.setCalls, 0);
    });
  });
}
