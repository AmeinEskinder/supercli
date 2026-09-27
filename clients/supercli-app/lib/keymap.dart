/// Central keymap — the SINGLE source of truth for keyboard shortcuts.
///
/// Ported from the native macOS menu in
/// `clients/legacy/native/SupercliNative/Sources/SupercliNative/AppDelegate.swift`
/// (`buildMenus`, lines ~313-571) and the SwiftUI view shortcuts in
/// `clients/legacy/native/SupercliNative/Sources/SupercliNative/Views/RootView.swift`
/// (Cmd-B sidebar toggle at :335, Cmd-Shift-R recent activity at :357).
///
/// No chord literal (`'ctrl+…'`, `'cmd+…'`, `'meta+…'`) and no
/// primary-modifier string interpolation may appear outside this file —
/// enforced by `test/no_hardcoded_keys_test.dart` and the `supercli-app`
/// CI workflow. Every [UiAction] (and every palette display shortcut)
/// takes its keys from here.
///
/// Chord grammar (see `lib/keybindings.dart`): modifiers
/// `ctrl|alt|shift|meta` plus one key, joined with `+`. `meta` is the
/// platform meta key (Cmd on macOS).
///
/// Platform policy: the primary modifier is `meta` (Cmd) on macOS and
/// `ctrl` on Linux/Windows. Chords that are genuinely Ctrl on every
/// platform (Ctrl-Tab / Ctrl-Shift-Tab session switcher, Ctrl-Enter
/// approvals) are declared explicitly as `ctrl+…` constants.
///
/// Modifier order inside each chord is preserved verbatim from the
/// pre-keymap code: the key parser is treated as order-sensitive, so
/// `shift+ctrl+d` stays `shift+<primary>+d`, not `ctrl+shift+d`.
library;

import 'dart:io' show Platform;

import 'platform_keys.dart';

/// Builds [rest] on the platform primary modifier:
/// `meta` (Cmd) on macOS, `ctrl` on Linux/Windows.
String _onPrimary(String rest, bool isMacOS) =>
    '${primaryModifier(isMacOS: isMacOS)}+$rest';

/// Builds [rest] with the primary modifier in the middle, preserving the
/// original modifier order (e.g. `shift+ctrl+d`).
String _midPrimary(String before, String after, bool isMacOS) =>
    '$before+${primaryModifier(isMacOS: isMacOS)}+$after';

/// Builds [rest] with the primary modifier after a leading modifier
/// (e.g. `alt+ctrl+left`).
String _afterModifier(String first, String rest, bool isMacOS) =>
    '$first+${primaryModifier(isMacOS: isMacOS)}+$rest';

/// The running platform. Every [Keymap] entry takes an optional [isMacOS]
/// (defaulting to the running platform) so both platform mappings are
/// unit-testable.
bool get _isMacOS => Platform.isMacOS;

/// Named shortcuts for the Supercli desktop app.
///
/// Each entry resolves against the running platform unless [isMacOS] is
/// passed explicitly (tests use this to pin both mappings).
final class Keymap {
  const Keymap._();

  // ------------------------------------------------------------------
  // App menu (AppDelegate.swift)
  // ------------------------------------------------------------------

  /// Settings… — Cmd-, (AppDelegate.swift:313)
  static String settings({bool? isMacOS}) =>
      _onPrimary(',', isMacOS ?? _isMacOS);

  /// Hide Supercli — Cmd-H (AppDelegate.swift:327)
  static String hide({bool? isMacOS}) => _onPrimary('h', isMacOS ?? _isMacOS);

  /// Hide Others — Cmd-Option-H (AppDelegate.swift:334-336)
  static String hideOthers({bool? isMacOS}) =>
      _onPrimary('alt+h', isMacOS ?? _isMacOS);

  /// Quit Supercli — Cmd-Q (AppDelegate.swift:350)
  static String quit({bool? isMacOS}) => _onPrimary('q', isMacOS ?? _isMacOS);

  // ------------------------------------------------------------------
  // Session menu (AppDelegate.swift)
  // ------------------------------------------------------------------

  /// New Session — Cmd-N (AppDelegate.swift:363)
  static String newSession({bool? isMacOS}) =>
      _onPrimary('n', isMacOS ?? _isMacOS);

  /// New Terminal — Cmd-T (AppDelegate.swift:372)
  static String newTerminal({bool? isMacOS}) =>
      _onPrimary('t', isMacOS ?? _isMacOS);

  /// Split Pane Right — Cmd-D (AppDelegate.swift:379)
  static String splitRight({bool? isMacOS}) =>
      _onPrimary('d', isMacOS ?? _isMacOS);

  /// Split Pane Down — Cmd-Shift-D (AppDelegate.swift:386-388)
  static String splitDown({bool? isMacOS}) =>
      _midPrimary('shift', 'd', isMacOS ?? _isMacOS);

  /// Zoom Pane — Cmd-Shift-Return (AppDelegate.swift:395-397)
  static String zoomPane({bool? isMacOS}) =>
      _midPrimary('shift', 'enter', isMacOS ?? _isMacOS);

  /// Focus Pane Left — Cmd-Option-Left (AppDelegate.swift:409)
  static String focusPaneLeft({bool? isMacOS}) =>
      _afterModifier('alt', 'left', isMacOS ?? _isMacOS);

  /// Focus Pane Right — Cmd-Option-Right (AppDelegate.swift:410)
  static String focusPaneRight({bool? isMacOS}) =>
      _afterModifier('alt', 'right', isMacOS ?? _isMacOS);

  /// Focus Pane Up — Cmd-Option-Up (AppDelegate.swift:411)
  static String focusPaneUp({bool? isMacOS}) =>
      _afterModifier('alt', 'up', isMacOS ?? _isMacOS);

  /// Focus Pane Down — Cmd-Option-Down (AppDelegate.swift:412)
  static String focusPaneDown({bool? isMacOS}) =>
      _afterModifier('alt', 'down', isMacOS ?? _isMacOS);

  /// Collapse All Folders — Cmd-Option-B (AppDelegate.swift:432-434)
  static String collapseAllFolders({bool? isMacOS}) =>
      _onPrimary('alt+b', isMacOS ?? _isMacOS);

  /// Command Palette — Cmd-K (AppDelegate.swift:443)
  static String commandPalette({bool? isMacOS}) =>
      _onPrimary('k', isMacOS ?? _isMacOS);

  /// Take Screenshot — Cmd-Shift-S (AppDelegate.swift:454-456)
  static String screenshot({bool? isMacOS}) =>
      _onPrimary('shift+s', isMacOS ?? _isMacOS);

  // ------------------------------------------------------------------
  // Edit menu (AppDelegate.swift)
  // ------------------------------------------------------------------

  /// Undo — Cmd-Z (AppDelegate.swift:465)
  static String undo({bool? isMacOS}) => _onPrimary('z', isMacOS ?? _isMacOS);

  /// Redo — Cmd-Shift-Z (AppDelegate.swift:468)
  static String redo({bool? isMacOS}) =>
      _onPrimary('shift+z', isMacOS ?? _isMacOS);

  /// Cut — Cmd-X (AppDelegate.swift:472)
  static String cut({bool? isMacOS}) => _onPrimary('x', isMacOS ?? _isMacOS);

  /// Copy — Cmd-C (AppDelegate.swift:475)
  static String copy({bool? isMacOS}) => _onPrimary('c', isMacOS ?? _isMacOS);

  /// Paste — Cmd-V (AppDelegate.swift:478)
  static String paste({bool? isMacOS}) => _onPrimary('v', isMacOS ?? _isMacOS);

  /// Select All — Cmd-A (AppDelegate.swift:483)
  static String selectAll({bool? isMacOS}) =>
      _onPrimary('a', isMacOS ?? _isMacOS);

  /// Find… — Cmd-F (AppDelegate.swift:494)
  static String find({bool? isMacOS}) => _onPrimary('f', isMacOS ?? _isMacOS);

  /// Find Next — Cmd-G (AppDelegate.swift:501)
  static String findNext({bool? isMacOS}) =>
      _onPrimary('g', isMacOS ?? _isMacOS);

  /// Find Previous — Cmd-Shift-G (AppDelegate.swift:508)
  static String findPrevious({bool? isMacOS}) =>
      _onPrimary('shift+g', isMacOS ?? _isMacOS);

  // ------------------------------------------------------------------
  // View menu: terminal font zoom (AppDelegate.swift)
  // ------------------------------------------------------------------

  /// Increase Font Size — Cmd-+ (AppDelegate.swift:526)
  static String increaseFont({bool? isMacOS}) =>
      _onPrimary('+', isMacOS ?? _isMacOS);

  /// Increase Font Size (hidden twin) — Cmd-= (AppDelegate.swift:533)
  static String increaseFontAlt({bool? isMacOS}) =>
      _onPrimary('=', isMacOS ?? _isMacOS);

  /// Decrease Font Size — Cmd-- (AppDelegate.swift:542)
  static String decreaseFont({bool? isMacOS}) =>
      _onPrimary('-', isMacOS ?? _isMacOS);

  /// Reset Font Size — Cmd-0 (AppDelegate.swift:549)
  static String resetFont({bool? isMacOS}) =>
      _onPrimary('0', isMacOS ?? _isMacOS);

  // ------------------------------------------------------------------
  // Window menu (AppDelegate.swift)
  // ------------------------------------------------------------------

  /// Close Window — Cmd-W (AppDelegate.swift:564)
  static String closeWindow({bool? isMacOS}) =>
      _onPrimary('w', isMacOS ?? _isMacOS);

  /// Minimize — Cmd-M (AppDelegate.swift:571)
  static String minimize({bool? isMacOS}) =>
      _onPrimary('m', isMacOS ?? _isMacOS);

  // ------------------------------------------------------------------
  // SwiftUI view shortcuts (Views/RootView.swift)
  // ------------------------------------------------------------------

  /// Toggle sidebar — Cmd-B (RootView.swift:335)
  static String sidebarToggle({bool? isMacOS}) =>
      _onPrimary('b', isMacOS ?? _isMacOS);

  /// Recent activity page — Cmd-Shift-R (RootView.swift:357)
  static String recentActivity({bool? isMacOS}) =>
      _onPrimary('shift+r', isMacOS ?? _isMacOS);

  // ------------------------------------------------------------------
  // App-specific chords (Dart app; no macOS menu equivalent)
  // ------------------------------------------------------------------

  /// Focus the message composer — primary+L.
  static String composerFocus({bool? isMacOS}) =>
      _onPrimary('l', isMacOS ?? _isMacOS);

  /// Equalize pane sizes — primary+Shift+E.
  static String equalizeSplits({bool? isMacOS}) =>
      _onPrimary('shift+e', isMacOS ?? _isMacOS);

  /// Detach the focused pane — primary+Shift+O.
  static String detachPane({bool? isMacOS}) =>
      _onPrimary('shift+o', isMacOS ?? _isMacOS);

  /// Git pull — primary+L.
  static String gitPull({bool? isMacOS}) => _onPrimary('l', isMacOS ?? _isMacOS);

  /// Git push — primary+P.
  static String gitPush({bool? isMacOS}) => _onPrimary('p', isMacOS ?? _isMacOS);

  /// Save the current note — primary+S.
  static String saveNote({bool? isMacOS}) =>
      _onPrimary('s', isMacOS ?? _isMacOS);

  /// Open the edit/detail view before answering — primary+E.
  static String editDetail({bool? isMacOS}) =>
      _onPrimary('e', isMacOS ?? _isMacOS);

  /// Copy path — Alt+C on all platforms (no primary modifier involved).
  static const String copyPath = 'alt+c';

  // ------------------------------------------------------------------
  // Platform-neutral: genuinely Ctrl on every platform
  // ------------------------------------------------------------------

  /// Submit / approve — Ctrl+Enter on all platforms by design
  /// (see `lib/platform_keys.dart`).
  static const String submit = 'ctrl+enter';

  /// Deny — Ctrl+Shift+Enter on all platforms by design.
  static const String deny = 'ctrl+shift+enter';

  /// MRU session switcher forward — Ctrl-Tab on all platforms.
  static const String switcherNext = 'ctrl+tab';

  /// MRU session switcher backward — Ctrl-Shift-Tab on all platforms.
  static const String switcherPrevious = 'ctrl+shift+tab';
}
