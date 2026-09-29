/// macOS main-menu model for the native app shell.
///
/// Port of the menu *content* from `AppDelegate.installMainMenu()` in
/// `clients/legacy/native/SupercliNative/Sources/SupercliNative/AppDelegate.swift`.
///
/// Split (same as `menu_bar.rs`): Dart owns the content — menus, titles, key
/// equivalents, and action ids. Rust (`supercli-native-bridge::macos::app_shell`)
/// owns the native binding: it builds the `NSMenu` via objc2 from this model
/// and validates items with `validate_menu_item`.
///
/// Every key equivalent in this file is DERIVED from [Keymap]
/// (`lib/keymap.dart`), the single source of truth — never hardcoded.
/// [_menuItem] takes a `Keymap` chord and [_parseMenuChord] converts it to the
/// AppKit-style key + modifier set. Enforced by
/// `test/no_hardcoded_keys_test.dart` ("no hardcoded menu keys in
/// lib/app_shell_menu.dart").
///
/// Action ids are the snake_case names of `MenuAction` in
/// `crates/supercli-native-bridge/src/macos/app_shell.rs`.
library;

import 'keymap.dart';

/// Key-equivalent modifier.
enum MenuModifier { command, shift, option }

/// One entry in a menu.
sealed class MenuEntry {
  const MenuEntry();
}

/// A clickable menu item.
final class MenuItemEntry extends MenuEntry {
  const MenuItemEntry({
    required this.title,
    required this.action,
    this.key = '',
    this.modifiers = const {},
    this.hidden = false,
    this.allowsKeyEquivalentWhenHidden = false,
  });

  /// Display title. Some titles are dynamic — the Rust validator overrides
  /// them at validation time (e.g. "Close Pane"/"Close Window").
  final String title;

  /// Action id; must match a `MenuAction` variant in app_shell.rs.
  /// Empty for responder-chain items handled by AppKit (About, Undo, Cut…).
  final String action;

  /// Key equivalent (single character, or '' for none).
  final String key;
  final Set<MenuModifier> modifiers;

  /// Hidden items still work when [allowsKeyEquivalentWhenHidden] is true —
  /// used for the ⌘= twin of ⌘+ (⌘+ is ⌘⇧= on US layouts).
  final bool hidden;
  final bool allowsKeyEquivalentWhenHidden;
}

/// A separator line.
final class MenuSeparator extends MenuEntry {
  const MenuSeparator();
}

/// The Services submenu placeholder (wired to `NSApp.servicesMenu` by the
/// native binding).
final class ServicesSubmenu extends MenuEntry {
  const ServicesSubmenu();
}

/// One top-level menu.
final class AppMenu {
  const AppMenu({required this.id, required this.title, required this.entries});

  final String id;
  final String title;
  final List<MenuEntry> entries;
}

/// Converts a [Keymap] chord (e.g. `Keymap.zoomPane(isMacOS: true)` →
/// `'shift+meta+enter'`) into the AppKit-style key equivalent and modifier
/// set used by [MenuItemEntry].
///
/// The native shell menu is macOS-only, so every chord is resolved with
/// `isMacOS: true`: `meta` becomes [MenuModifier.command]. `ctrl` has no
/// menu equivalent and is rejected; no menu chord uses it.
({String key, Set<MenuModifier> modifiers}) _parseMenuChord(String chord) {
  const modifierByName = {
    'meta': MenuModifier.command,
    'shift': MenuModifier.shift,
    'alt': MenuModifier.option,
  };
  const namedKeys = {
    'enter': '\r',
    'left': '←',
    'right': '→',
    'up': '↑',
    'down': '↓',
  };

  final modifiers = <MenuModifier>{};
  var rest = chord;
  while (true) {
    final plus = rest.indexOf('+');
    if (plus < 0) break;
    final token = rest.substring(0, plus);
    if (token == 'ctrl') {
      throw ArgumentError.value(
        chord,
        'chord',
        'ctrl has no MenuModifier in the native menu',
      );
    }
    final modifier = modifierByName[token];
    if (modifier == null) break;
    modifiers.add(modifier);
    rest = rest.substring(plus + 1);
  }
  var key = namedKeys[rest] ?? rest;
  if (key.length != 1) {
    throw ArgumentError.value(chord, 'chord', 'unrecognized key "$rest"');
  }
  return (key: key, modifiers: modifiers);
}

/// Builds a [MenuItemEntry] whose key equivalent is derived from a [Keymap]
/// chord — never hardcoded. Pass the chord resolved for macOS, e.g.
/// `Keymap.settings(isMacOS: true)`.
///
/// [shiftedKey]: when true and the chord carries shift, the key equivalent
/// is the shifted character ('Z', 'G'), preserving AppDelegate's original
/// keyEquivalent for Redo and Find Previous. AppKit treats 'z'+shift and
/// 'Z'+shift identically; the flag keeps the pinned model byte-exact.
MenuItemEntry _menuItem({
  required String title,
  required String action,
  required String chord,
  bool shiftedKey = false,
  bool hidden = false,
  bool allowsKeyEquivalentWhenHidden = false,
}) {
  final parsed = _parseMenuChord(chord);
  var key = parsed.key;
  if (shiftedKey && parsed.modifiers.contains(MenuModifier.shift)) {
    key = key.toUpperCase();
  }
  return MenuItemEntry(
    title: title,
    action: action,
    key: key,
    modifiers: parsed.modifiers,
    hidden: hidden,
    allowsKeyEquivalentWhenHidden: allowsKeyEquivalentWhenHidden,
  );
}

/// The full macOS main menu, in HIG order. Port of
/// `AppDelegate.installMainMenu()`.
///
/// Non-const: every key equivalent is computed from [Keymap] at startup.
final List<AppMenu> mainMenu = [
  AppMenu(
    id: 'app',
    title: 'Supercli',
    entries: [
      // autoenablesItems = false on this menu: AppKit would otherwise
      // re-enable the targetless "Check for Updates…" item via responder
      // chain resolution.
      const MenuItemEntry(title: 'About Supercli', action: 'about'),
      const MenuItemEntry(
        title: 'Check for Updates…',
        action: 'check_for_updates',
      ),
      const MenuSeparator(),
      _menuItem(
        title: 'Settings…',
        action: 'open_settings',
        chord: Keymap.settings(isMacOS: true),
      ),
      const MenuSeparator(),
      const ServicesSubmenu(),
      const MenuSeparator(),
      _menuItem(
        title: 'Hide Supercli',
        action: 'hide',
        chord: Keymap.hide(isMacOS: true),
      ),
      _menuItem(
        title: 'Hide Others',
        action: 'hide_others',
        chord: Keymap.hideOthers(isMacOS: true),
      ),
      const MenuItemEntry(title: 'Show All', action: 'show_all'),
      const MenuSeparator(),
      _menuItem(
        title: 'Quit Supercli',
        action: 'quit',
        chord: Keymap.quit(isMacOS: true),
      ),
    ],
  ),
  AppMenu(
    id: 'session',
    title: 'Session',
    entries: [
      _menuItem(
        title: 'New Session',
        action: 'new_session',
        chord: Keymap.newSession(isMacOS: true),
      ),
      _menuItem(
        title: 'New Terminal',
        action: 'new_terminal',
        chord: Keymap.newTerminal(isMacOS: true),
      ),
      _menuItem(
        title: 'Split Pane Right',
        action: 'split_pane_right',
        chord: Keymap.splitRight(isMacOS: true),
      ),
      _menuItem(
        title: 'Split Pane Down',
        action: 'split_pane_down',
        chord: Keymap.splitDown(isMacOS: true),
      ),
      // ⇧⌘↩ (Ghostty parity): temporarily maximize the active pane.
      _menuItem(
        title: 'Zoom Pane',
        action: 'zoom_pane',
        chord: Keymap.zoomPane(isMacOS: true),
      ),
      const MenuItemEntry(title: 'Equalize Splits', action: 'equalize_splits'),
      // ⌥⌘arrows move keyboard focus to the spatial neighbor pane.
      _menuItem(
        title: 'Focus Pane Left',
        action: 'focus_pane_left',
        chord: Keymap.focusPaneLeft(isMacOS: true),
      ),
      _menuItem(
        title: 'Focus Pane Right',
        action: 'focus_pane_right',
        chord: Keymap.focusPaneRight(isMacOS: true),
      ),
      _menuItem(
        title: 'Focus Pane Up',
        action: 'focus_pane_up',
        chord: Keymap.focusPaneUp(isMacOS: true),
      ),
      _menuItem(
        title: 'Focus Pane Down',
        action: 'focus_pane_down',
        chord: Keymap.focusPaneDown(isMacOS: true),
      ),
      const MenuSeparator(),
      // ⌥⌘B — the sidebar chord family (⌘B toggles the sidebar).
      _menuItem(
        title: 'Collapse All Folders',
        action: 'collapse_all_folders',
        chord: Keymap.collapseAllFolders(isMacOS: true),
      ),
      const MenuSeparator(),
      // The palette is the discoverable home for "jump to anything".
      _menuItem(
        title: 'Command Palette',
        action: 'toggle_command_palette',
        chord: Keymap.commandPalette(isMacOS: true),
      ),
      const MenuSeparator(),
      // ⌘⇧S (the Firefox screenshot binding; the system's ⌘⇧3/4/5 are
      // intercepted before apps see them).
      _menuItem(
        title: 'Take Screenshot…',
        action: 'take_screenshot',
        chord: Keymap.screenshot(isMacOS: true),
      ),
    ],
  ),
  AppMenu(
    id: 'edit',
    title: 'Edit',
    entries: [
      _menuItem(
        title: 'Undo',
        action: 'undo',
        chord: Keymap.undo(isMacOS: true),
      ),
      _menuItem(
        title: 'Redo',
        action: 'redo',
        chord: Keymap.redo(isMacOS: true),
        shiftedKey: true,
      ),
      const MenuSeparator(),
      _menuItem(title: 'Cut', action: 'cut', chord: Keymap.cut(isMacOS: true)),
      _menuItem(
        title: 'Copy',
        action: 'copy',
        chord: Keymap.copy(isMacOS: true),
      ),
      _menuItem(
        title: 'Paste',
        action: 'paste',
        chord: Keymap.paste(isMacOS: true),
      ),
      _menuItem(
        title: 'Select All',
        action: 'select_all',
        chord: Keymap.selectAll(isMacOS: true),
      ),
      const MenuSeparator(),
      // Terminal search rides libghostty's own engine; the notification
      // reaches the displayed pane's find bar.
      _menuItem(
        title: 'Find…',
        action: 'find',
        chord: Keymap.find(isMacOS: true),
      ),
      _menuItem(
        title: 'Find Next',
        action: 'find_next',
        chord: Keymap.findNext(isMacOS: true),
      ),
      _menuItem(
        title: 'Find Previous',
        action: 'find_previous',
        chord: Keymap.findPrevious(isMacOS: true),
        shiftedKey: true,
      ),
    ],
  ),
  AppMenu(
    id: 'view',
    title: 'View',
    entries: [
      // Terminal font zoom edits the persisted Settings ▸ Appearance font,
      // so every pane follows and the zoom survives a restart.
      _menuItem(
        title: 'Increase Font Size',
        action: 'increase_font_size',
        chord: Keymap.increaseFont(isMacOS: true),
      ),
      // Hidden twin: ⌘+ is ⌘⇧= on US layouts, so this keeps the unshifted
      // chord working.
      _menuItem(
        title: 'Increase Font Size',
        action: 'increase_font_size',
        chord: Keymap.increaseFontAlt(isMacOS: true),
        hidden: true,
        allowsKeyEquivalentWhenHidden: true,
      ),
      _menuItem(
        title: 'Decrease Font Size',
        action: 'decrease_font_size',
        chord: Keymap.decreaseFont(isMacOS: true),
      ),
      _menuItem(
        title: 'Reset Font Size',
        action: 'reset_font_size',
        chord: Keymap.resetFont(isMacOS: true),
      ),
    ],
  ),
  AppMenu(
    id: 'window',
    title: 'Window',
    entries: [
      // ⌘W follows Ghostty's active-surface convention while a terminal is
      // shown, then falls back to closing the window. The native binding
      // sets this menu as NSApp.windowsMenu.
      _menuItem(
        title: 'Close Window',
        action: 'close_pane_or_window',
        chord: Keymap.closeWindow(isMacOS: true),
      ),
      _menuItem(
        title: 'Minimize',
        action: 'minimize',
        chord: Keymap.minimize(isMacOS: true),
      ),
      const MenuItemEntry(title: 'Zoom', action: 'zoom'),
      const MenuSeparator(),
      const MenuItemEntry(
        title: 'Bring All to Front',
        action: 'arrange_in_front',
      ),
    ],
  ),
  AppMenu(
    id: 'help',
    title: 'Help',
    entries: [
      // The native binding sets this menu as NSApp.helpMenu.
      const MenuItemEntry(title: 'Supercli Help', action: 'open_help'),
    ],
  ),
];
