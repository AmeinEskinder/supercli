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
/// Action ids are the snake_case names of `MenuAction` in
/// `crates/supercli-native-bridge/src/macos/app_shell.rs`.
library;

/// Key-equivalent modifier.
enum MenuModifier {
  command,
  shift,
  option,
}

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

const _cmd = MenuModifier.command;
const _shift = MenuModifier.shift;
const _opt = MenuModifier.option;

/// The full macOS main menu, in HIG order. Port of
/// `AppDelegate.installMainMenu()`.
const List<AppMenu> mainMenu = [
  AppMenu(
    id: 'app',
    title: 'Supercli',
    entries: [
      // autoenablesItems = false on this menu: AppKit would otherwise
      // re-enable the targetless "Check for Updates…" item via responder
      // chain resolution.
      MenuItemEntry(title: 'About Supercli', action: 'about'),
      MenuItemEntry(title: 'Check for Updates…', action: 'check_for_updates'),
      MenuSeparator(),
      MenuItemEntry(
        title: 'Settings…',
        action: 'open_settings',
        key: ',',
        modifiers: {_cmd},
      ),
      MenuSeparator(),
      ServicesSubmenu(),
      MenuSeparator(),
      MenuItemEntry(
        title: 'Hide Supercli',
        action: 'hide',
        key: 'h',
        modifiers: {_cmd},
      ),
      MenuItemEntry(
        title: 'Hide Others',
        action: 'hide_others',
        key: 'h',
        modifiers: {_cmd, _opt},
      ),
      MenuItemEntry(title: 'Show All', action: 'show_all'),
      MenuSeparator(),
      MenuItemEntry(
        title: 'Quit Supercli',
        action: 'quit',
        key: 'q',
        modifiers: {_cmd},
      ),
    ],
  ),
  AppMenu(
    id: 'session',
    title: 'Session',
    entries: [
      MenuItemEntry(
        title: 'New Session',
        action: 'new_session',
        key: 'n',
        modifiers: {_cmd},
      ),
      MenuItemEntry(
        title: 'New Terminal',
        action: 'new_terminal',
        key: 't',
        modifiers: {_cmd},
      ),
      MenuItemEntry(
        title: 'Split Pane Right',
        action: 'split_pane_right',
        key: 'd',
        modifiers: {_cmd},
      ),
      MenuItemEntry(
        title: 'Split Pane Down',
        action: 'split_pane_down',
        key: 'd',
        modifiers: {_cmd, _shift},
      ),
      // ⇧⌘↩ (Ghostty parity): temporarily maximize the active pane.
      MenuItemEntry(
        title: 'Zoom Pane',
        action: 'zoom_pane',
        key: '\r',
        modifiers: {_cmd, _shift},
      ),
      MenuItemEntry(title: 'Equalize Splits', action: 'equalize_splits'),
      // ⌥⌘arrows move keyboard focus to the spatial neighbor pane.
      MenuItemEntry(
        title: 'Focus Pane Left',
        action: 'focus_pane_left',
        key: '←',
        modifiers: {_cmd, _opt},
      ),
      MenuItemEntry(
        title: 'Focus Pane Right',
        action: 'focus_pane_right',
        key: '→',
        modifiers: {_cmd, _opt},
      ),
      MenuItemEntry(
        title: 'Focus Pane Up',
        action: 'focus_pane_up',
        key: '↑',
        modifiers: {_cmd, _opt},
      ),
      MenuItemEntry(
        title: 'Focus Pane Down',
        action: 'focus_pane_down',
        key: '↓',
        modifiers: {_cmd, _opt},
      ),
      MenuSeparator(),
      // ⌥⌘B — the sidebar chord family (⌘B toggles the sidebar).
      MenuItemEntry(
        title: 'Collapse All Folders',
        action: 'collapse_all_folders',
        key: 'b',
        modifiers: {_cmd, _opt},
      ),
      MenuSeparator(),
      // The palette is the discoverable home for "jump to anything".
      MenuItemEntry(
        title: 'Command Palette',
        action: 'toggle_command_palette',
        key: 'k',
        modifiers: {_cmd},
      ),
      MenuSeparator(),
      // ⌘⇧S (the Firefox screenshot binding; the system's ⌘⇧3/4/5 are
      // intercepted before apps see them).
      MenuItemEntry(
        title: 'Take Screenshot…',
        action: 'take_screenshot',
        key: 's',
        modifiers: {_cmd, _shift},
      ),
    ],
  ),
  AppMenu(
    id: 'edit',
    title: 'Edit',
    entries: [
      MenuItemEntry(title: 'Undo', action: 'undo', key: 'z', modifiers: {_cmd}),
      MenuItemEntry(
        title: 'Redo',
        action: 'redo',
        key: 'Z',
        modifiers: {_cmd, _shift},
      ),
      MenuSeparator(),
      MenuItemEntry(title: 'Cut', action: 'cut', key: 'x', modifiers: {_cmd}),
      MenuItemEntry(title: 'Copy', action: 'copy', key: 'c', modifiers: {_cmd}),
      MenuItemEntry(title: 'Paste', action: 'paste', key: 'v', modifiers: {_cmd}),
      MenuItemEntry(
        title: 'Select All',
        action: 'select_all',
        key: 'a',
        modifiers: {_cmd},
      ),
      MenuSeparator(),
      // Terminal search rides libghostty's own engine; the notification
      // reaches the displayed pane's find bar.
      MenuItemEntry(
        title: 'Find…',
        action: 'find',
        key: 'f',
        modifiers: {_cmd},
      ),
      MenuItemEntry(
        title: 'Find Next',
        action: 'find_next',
        key: 'g',
        modifiers: {_cmd},
      ),
      MenuItemEntry(
        title: 'Find Previous',
        action: 'find_previous',
        key: 'G',
        modifiers: {_cmd, _shift},
      ),
    ],
  ),
  AppMenu(
    id: 'view',
    title: 'View',
    entries: [
      // Terminal font zoom edits the persisted Settings ▸ Appearance font,
      // so every pane follows and the zoom survives a restart.
      MenuItemEntry(
        title: 'Increase Font Size',
        action: 'increase_font_size',
        key: '+',
        modifiers: {_cmd},
      ),
      // Hidden twin: ⌘+ is ⌘⇧= on US layouts, so this keeps the unshifted
      // chord working.
      MenuItemEntry(
        title: 'Increase Font Size',
        action: 'increase_font_size',
        key: '=',
        modifiers: {_cmd},
        hidden: true,
        allowsKeyEquivalentWhenHidden: true,
      ),
      MenuItemEntry(
        title: 'Decrease Font Size',
        action: 'decrease_font_size',
        key: '-',
        modifiers: {_cmd},
      ),
      MenuItemEntry(
        title: 'Reset Font Size',
        action: 'reset_font_size',
        key: '0',
        modifiers: {_cmd},
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
      MenuItemEntry(
        title: 'Close Window',
        action: 'close_pane_or_window',
        key: 'w',
        modifiers: {_cmd},
      ),
      MenuItemEntry(
        title: 'Minimize',
        action: 'minimize',
        key: 'm',
        modifiers: {_cmd},
      ),
      MenuItemEntry(title: 'Zoom', action: 'zoom'),
      MenuSeparator(),
      MenuItemEntry(title: 'Bring All to Front', action: 'arrange_in_front'),
    ],
  ),
  AppMenu(
    id: 'help',
    title: 'Help',
    entries: [
      // The native binding sets this menu as NSApp.helpMenu.
      MenuItemEntry(title: 'Supercli Help', action: 'open_help'),
    ],
  ),
];
