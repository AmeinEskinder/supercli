/// Tests for macOS shell integrations: menu, menu-bar, notifications,
/// keychain license, Finder service, Sparkle. Rows 190–196 — [DESKTOP].
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/screens/macos_shell.dart';
import 'package:test/test.dart';

void main() {
  group('MainMenu (row 190)', () {
    test('six sections', () {
      final menu = MainMenu(mod: 'meta');
      expect(menu.sections,
          ['App', 'Session', 'Edit', 'View', 'Window', 'Help']);
    });

    test('App section has settings and quit with shortcuts', () {
      final items = MainMenu(mod: 'meta').itemsFor('App');
      expect(items.map((i) => i.id),
          containsAll(['app.settings', 'app.quit']));
      expect(items.firstWhere((i) => i.id == 'app.quit').shortcut, 'meta+q');
    });

    test('shortcuts use the injected modifier', () {
      final items = MainMenu(mod: 'ctrl').itemsFor('Session');
      expect(items.firstWhere((i) => i.id == 'session.new').shortcut,
          'ctrl+n');
    });

    test('unknown section is empty', () {
      expect(MainMenu().itemsFor('Nope'), isEmpty);
    });
  });

  group('MenuBarStatusItem (row 191)', () {
    test('spinner active while sessions busy', () {
      expect(MenuBarStatusItem(busySessionCount: 2).spinnerActive, isTrue);
      expect(MenuBarStatusItem().spinnerActive, isFalse);
    });

    test('popover shows count and actions when open', () {
      final node = MenuBarStatusItem(
        busySessionCount: 3,
        popoverOpen: true,
      ).build() as UiColumn;
      expect(node.children.length, 2);
      expect((node.children[0] as UiText).text, contains('◌'));
    });
  });

  group('MenuBarAgentMode (row 192)', () {
    test('enabled hides to menu bar on close', () {
      expect(MenuBarAgentMode(enabled: true).onLastWindowClose(),
          'hide-to-menu-bar');
    });

    test('disabled quits on close', () {
      expect(
          MenuBarAgentMode(enabled: false).onLastWindowClose(), 'quit');
    });
  });

  group('FinderService (row 193)', () {
    test('filters empty urls', () {
      const svc = FinderService();
      expect(svc.pendingFolders(['/a', '', '/b']), ['/a', '/b']);
    });
  });

  group('SparkleUpdater (row 194)', () {
    test('feed url follows beta channel', () {
      expect(SparkleUpdater(betaChannel: false).feedUrl,
          contains('appcast.xml'));
      expect(SparkleUpdater(betaChannel: true).feedUrl,
          contains('appcast-beta.xml'));
    });

    test('beta toggle reflects state', () {
      final node =
          SparkleUpdater(betaChannel: true).build() as UiColumn;
      expect((node.children[0] as UiButton).label, contains('☑'));
    });
  });

  group('NotificationCenterBridge (row 195)', () {
    test('post queues a notification', () {
      final bridge = NotificationCenterBridge();
      bridge.post(const MacNotification(
        kind: MacNotificationKind.needsInput,
        title: 'Approval needed',
      ));
      expect(bridge.pending.length, 1);
      expect(bridge.pending[0].kind, MacNotificationKind.needsInput);
    });

    test('postTest queues a diagnostics notification', () {
      final bridge = NotificationCenterBridge();
      bridge.postTest();
      expect(bridge.pending.length, 1);
      expect(bridge.pending[0].title, 'Test notification');
    });

    test('delivered removes from pending', () {
      final bridge = NotificationCenterBridge();
      const n = MacNotification(
          kind: MacNotificationKind.finished, title: 'done');
      bridge.post(n);
      bridge.delivered(n);
      expect(bridge.pending, isEmpty);
    });
  });

  group('KeychainLicense (row 196)', () {
    test('shows licensed state with seats', () {
      final node =
          KeychainLicense(valid: true, seats: 3).build() as UiRow;
      expect((node.children[0] as UiText).text, contains('3 seats'));
    });

    test('shows unlicensed state', () {
      final node = KeychainLicense().build() as UiRow;
      expect((node.children[0] as UiText).text, 'Unlicensed');
    });
  });
}
