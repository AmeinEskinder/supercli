/// macOS shell integrations: main menu, menu-bar item, notifications,
/// keychain license, Finder service, Sparkle updates.
///
/// Ports of the AppKit-layer integrations (`MainMenu.swift`,
/// `StatusBarController.swift`, `NotificationBridge.swift`,
/// `KeychainLicense.swift`, `FinderService.swift`, `SparkleUpdater.swift`).
///
/// Rows 190–196 — [DESKTOP] parity.
/// These are platform bridges: the Dart side models menus, state, and
/// intents; the native host executes them. Each class documents the bridge
/// contract. GAP: no native macOS host in this environment; behavior is
/// component-level only. GAP: no native-window screenshot proof yet
/// (screenshot proof pending).
library;

import 'package:gpuidart/gpuidart.dart';

import '../platform_keys.dart';

/// Row 190: a single main-menu item.
final class MenuItem {
  const MenuItem({
    required this.id,
    required this.label,
    this.shortcut = '',
    this.enabled = true,
  });

  final String id;
  final String label;
  final String shortcut;
  final bool enabled;
}

/// Row 190: Full main menu set (App/Session/Edit/View/Window/Help).
///
/// Shortcuts use the platform primary modifier. The native host renders
/// this model into an NSMenu; gpuidart renders a fallback menu bar.
final class MainMenu {
  MainMenu({String? mod}) : _mod = mod ?? currentPrimaryModifier;

  final String _mod;

  List<String> get sections =>
      const ['App', 'Session', 'Edit', 'View', 'Window', 'Help'];

  List<MenuItem> itemsFor(String section) {
    final m = _mod;
    return switch (section) {
      'App' => [
          MenuItem(id: 'app.about', label: 'About Supercli'),
          MenuItem(id: 'app.settings', label: 'Settings…', shortcut: '$m+,'),
          MenuItem(id: 'app.quit', label: 'Quit Supercli', shortcut: '$m+q'),
        ],
      'Session' => [
          MenuItem(id: 'session.new', label: 'New Session', shortcut: '$m+n'),
          MenuItem(id: 'session.close', label: 'Close Session', shortcut: '$m+w'),
          MenuItem(id: 'session.reveal', label: 'Reveal in Finder'),
        ],
      'Edit' => [
          MenuItem(id: 'edit.undo', label: 'Undo', shortcut: '$m+z'),
          MenuItem(id: 'edit.redo', label: 'Redo', shortcut: 'shift+$m+z'),
          MenuItem(id: 'edit.copy', label: 'Copy', shortcut: '$m+c'),
          MenuItem(id: 'edit.paste', label: 'Paste', shortcut: '$m+v'),
        ],
      'View' => [
          MenuItem(
              id: 'view.sidebar', label: 'Toggle Sidebar', shortcut: '$m+b'),
          MenuItem(
              id: 'view.palette',
              label: 'Command Palette…',
              shortcut: '$m+k'),
          MenuItem(
              id: 'view.fullscreen',
              label: 'Toggle Full Screen',
              shortcut: 'ctrl+$m+f'),
        ],
      'Window' => [
          MenuItem(
              id: 'window.minimize', label: 'Minimize', shortcut: '$m+m'),
          MenuItem(id: 'window.bring-all', label: 'Bring All to Front'),
        ],
      'Help' => [
          MenuItem(id: 'help.docs', label: 'Supercli Help'),
          MenuItem(id: 'help.check-updates', label: 'Check for Updates…'),
        ],
      _ => const [],
    };
  }

  UiNode build() {
    return UiRow('main-menu', [
      for (final s in sections) UiButton('menu-$s', s),
    ]);
  }
}

/// Row 191: Menu-bar status item with activity spinner and popover.
///
/// State model for the NSStatusItem: the spinner runs while any session is
/// busy; the popover lists busy sessions and offers show/hide.
final class MenuBarStatusItem {
  const MenuBarStatusItem({
    this.busySessionCount = 0,
    this.popoverOpen = false,
  });

  final int busySessionCount;
  final bool popoverOpen;

  bool get spinnerActive => busySessionCount > 0;

  UiNode build() {
    return UiColumn('menubar-status', [
      UiText('menubar-spinner', spinnerActive ? '◌ busy' : '● idle'),
      if (popoverOpen)
        UiColumn('menubar-popover', [
          UiText('menubar-busy-count', '$busySessionCount sessions busy'),
          const UiButton('menubar-show', 'Show Window'),
          const UiButton('menubar-quit', 'Quit'),
        ]),
    ]);
  }
}

/// Row 192: Keep running as a menu-bar agent when the window closes.
///
/// Policy model: closing the last window hides the app to the menu bar
/// instead of quitting, unless the user disabled the agent mode.
final class MenuBarAgentMode {
  const MenuBarAgentMode({this.enabled = true});

  final bool enabled;

  /// What closing the last window does under this policy.
  String onLastWindowClose() => enabled ? 'hide-to-menu-bar' : 'quit';
}

/// Row 193: Finder "New Supercli Session Here" service.
///
/// The service receives folder URLs from Finder and asks the Host to open
/// a session rooted at each folder.
final class FinderService {
  const FinderService();

  /// Folders the service was invoked with (for the pending open request).
  List<String> pendingFolders(List<String> urls) =>
      urls.where((u) => u.isNotEmpty).toList();

  UiNode build() {
    return const UiText(
        'finder-service', 'New Supercli Session Here (Finder service)');
  }
}

/// Row 194: Sparkle auto-updates with a beta channel opt-in.
final class SparkleUpdater {
  const SparkleUpdater({
    this.betaChannel = false,
    this.lastCheckAt,
    this.updateAvailable = false,
  });

  final bool betaChannel;
  final DateTime? lastCheckAt;
  final bool updateAvailable;

  /// The appcast URL for the selected channel.
  String get feedUrl => betaChannel
      ? 'https://releases.superc.li/appcast-beta.xml'
      : 'https://releases.superc.li/appcast.xml';

  UiNode build() {
    return UiColumn('sparkle-updater', [
      UiButton('sparkle-beta-toggle',
          betaChannel ? '☑ Beta channel' : '☐ Beta channel'),
      UiText('sparkle-status',
          updateAvailable ? 'Update available' : 'Up to date'),
      const UiButton('sparkle-check', 'Check for Updates…'),
    ]);
  }
}

/// Row 195: macOS Notification Center banners.
///
/// Kinds: needs-input, finished, app alerts. The Dart side queues the
/// request; the native host posts the NSUserNotification. Includes a test
/// notification action for the diagnostics panel.
enum MacNotificationKind { needsInput, finished, appAlert }

final class MacNotification {
  const MacNotification({
    required this.kind,
    required this.title,
    this.body = '',
  });

  final MacNotificationKind kind;
  final String title;
  final String body;
}

final class NotificationCenterBridge {
  NotificationCenterBridge() : _pending = [];

  final List<MacNotification> _pending;

  List<MacNotification> get pending => List.unmodifiable(_pending);

  void post(MacNotification n) {
    _pending.add(n);
  }

  /// Test notification for the delivery diagnostics panel.
  void postTest() {
    post(const MacNotification(
      kind: MacNotificationKind.appAlert,
      title: 'Test notification',
      body: 'If you see this, delivery works.',
    ));
  }

  void delivered(MacNotification n) {
    _pending.remove(n);
  }
}

/// Row 196: Keychain-backed Link license.
///
/// The license key lives in the macOS keychain (kSecClassGenericPassword);
/// the Dart side holds only the lookup account name and caches the last
/// validated state. No key material crosses the bridge.
final class KeychainLicense {
  const KeychainLicense({
    this.account = 'link-license',
    this.valid = false,
    this.seats = 0,
  });

  final String account;
  final bool valid;
  final int seats;

  UiNode build() {
    return UiRow('keychain-license', [
      UiText('license-status', valid ? 'Licensed ($seats seats)' : 'Unlicensed'),
      const UiButton('license-activate', 'Activate…'),
    ]);
  }
}
