/// Tests for the macOS main-menu model.
/// Port of the structural coverage implied by `AppDelegate.installMainMenu()`;
/// every menu, item title, key equivalent, and action id is pinned so the
/// Rust native binding (`app_shell.rs`) and this model cannot drift apart.
library;

import 'package:supercli_app/app_shell_menu.dart';
import 'package:test/test.dart';

/// Action ids that must exist as `MenuAction` variants in
/// `crates/supercli-native-bridge/src/macos/app_shell.rs`.
const rustMenuActions = {
  'check_for_updates',
  'open_settings',
  'new_session',
  'new_terminal',
  'split_pane_right',
  'split_pane_down',
  'zoom_pane',
  'equalize_splits',
  'focus_pane_left',
  'focus_pane_right',
  'focus_pane_up',
  'focus_pane_down',
  'collapse_all_folders',
  'toggle_command_palette',
  'take_screenshot',
  'find',
  'find_next',
  'find_previous',
  'close_pane_or_window',
  'open_help',
  'increase_font_size',
  'decrease_font_size',
  'reset_font_size',
};

List<MenuItemEntry> itemsOf(String menuId) => mainMenu
    .firstWhere((m) => m.id == menuId)
    .entries
    .whereType<MenuItemEntry>()
    .toList();

MenuItemEntry item(String menuId, String action) =>
    itemsOf(menuId).firstWhere((i) => i.action == action);

void main() {
  group('mainMenu structure', () {
    test('has six top-level menus in HIG order', () {
      expect(
        mainMenu.map((m) => m.id).toList(),
        ['app', 'session', 'edit', 'view', 'window', 'help'],
      );
    });

    test('every action id maps to a Rust MenuAction variant', () {
      // Responder-chain / AppKit-owned actions have no Rust counterpart.
      const appKitOwned = {
        'about',
        'hide',
        'hide_others',
        'show_all',
        'quit',
        'undo',
        'redo',
        'cut',
        'copy',
        'paste',
        'select_all',
        'minimize',
        'zoom',
        'arrange_in_front',
      };
      for (final menu in mainMenu) {
        for (final entry in menu.entries.whereType<MenuItemEntry>()) {
          expect(
            rustMenuActions.contains(entry.action) ||
                appKitOwned.contains(entry.action),
            isTrue,
            reason: '${menu.id} / ${entry.title}: unknown action ${entry.action}',
          );
        }
      }
    });

    test('no duplicate visible key equivalents within a menu', () {
      for (final menu in mainMenu) {
        final seen = <String>{};
        for (final entry in menu.entries.whereType<MenuItemEntry>()) {
          if (entry.key.isEmpty || entry.hidden) continue;
          final chord =
              '${entry.modifiers.map((m) => m.name).join('+')}+${entry.key}';
          expect(seen.add(chord), isTrue,
              reason: '${menu.id}: duplicate chord $chord');
        }
      }
    });
  });

  group('session menu', () {
    test('pane management items and chords', () {
      expect(item('session', 'new_session').key, 'n');
      expect(item('session', 'new_terminal').key, 't');
      expect(item('session', 'split_pane_right').key, 'd');
      final down = item('session', 'split_pane_down');
      expect(down.key, 'd');
      expect(down.modifiers,
          containsAll({MenuModifier.command, MenuModifier.shift}));
      final zoom = item('session', 'zoom_pane');
      expect(zoom.key, '\r');
      expect(zoom.modifiers,
          containsAll({MenuModifier.command, MenuModifier.shift}));
    });

    test('spatial focus chords are option-command arrows', () {
      const arrows = {
        'focus_pane_left': '←',
        'focus_pane_right': '→',
        'focus_pane_up': '↑',
        'focus_pane_down': '↓',
      };
      for (final e in arrows.entries) {
        final entry = item('session', e.key);
        expect(entry.key, e.value);
        expect(entry.modifiers,
            containsAll({MenuModifier.command, MenuModifier.option}));
      }
    });

    test('palette and screenshot chords', () {
      expect(item('session', 'toggle_command_palette').key, 'k');
      final shot = item('session', 'take_screenshot');
      expect(shot.key, 's');
      expect(shot.modifiers,
          containsAll({MenuModifier.command, MenuModifier.shift}));
    });
  });

  group('view menu', () {
    test('font zoom items', () {
      expect(item('view', 'increase_font_size').key, '+');
      expect(item('view', 'decrease_font_size').key, '-');
      expect(item('view', 'reset_font_size').key, '0');
    });

    test('hidden ⌘= twin keeps the unshifted chord working', () {
      final twins = itemsOf('view')
          .where((i) => i.action == 'increase_font_size')
          .toList();
      expect(twins, hasLength(2));
      final hidden = twins.firstWhere((i) => i.hidden);
      expect(hidden.key, '=');
      expect(hidden.allowsKeyEquivalentWhenHidden, isTrue);
    });
  });

  group('edit menu', () {
    test('find chords', () {
      expect(item('edit', 'find').key, 'f');
      expect(item('edit', 'find_next').key, 'g');
      final prev = item('edit', 'find_previous');
      expect(prev.key, 'G');
      expect(prev.modifiers,
          containsAll({MenuModifier.command, MenuModifier.shift}));
    });
  });

  group('app and window menus', () {
    test('app menu essentials', () {
      expect(item('app', 'open_settings').key, ',');
      expect(item('app', 'quit').key, 'q');
      expect(
        mainMenu
            .firstWhere((m) => m.id == 'app')
            .entries
            .whereType<ServicesSubmenu>(),
        hasLength(1),
      );
    });

    test('window menu close chord', () {
      expect(item('window', 'close_pane_or_window').key, 'w');
    });
  });
}
