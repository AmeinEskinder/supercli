/// Keymap parity tests: every row ported from the native macOS menu must
/// resolve to Cmd (`meta`) on macOS and Ctrl on Linux/Windows.
///
/// Source: `clients/legacy/native/SupercliNative/Sources/SupercliNative/`
/// `AppDelegate.swift` (`buildMenus`, ~lines 313-571) and
/// `Views/RootView.swift` (:335 Cmd-B sidebar, :357 Cmd-Shift-R recent
/// activity). Platform-neutral chords (Ctrl-Tab switcher, Ctrl-Enter
/// approvals) stay `ctrl+` on every platform by design.
library;

import 'dart:io' show Platform;

import 'package:supercli_app/keymap.dart';
import 'package:test/test.dart';

/// Every primary-modifier entry: (name, macOS chord, Linux/Windows chord).
const _primaryRows = <String, (String, String)>{
  // App menu
  'settings': ('meta+,', 'ctrl+,'),
  'hide': ('meta+h', 'ctrl+h'),
  'hideOthers': ('meta+alt+h', 'ctrl+alt+h'),
  'quit': ('meta+q', 'ctrl+q'),
  // Session menu
  'newSession': ('meta+n', 'ctrl+n'),
  'newTerminal': ('meta+t', 'ctrl+t'),
  'splitRight': ('meta+d', 'ctrl+d'),
  'splitDown': ('shift+meta+d', 'shift+ctrl+d'),
  'zoomPane': ('shift+meta+enter', 'shift+ctrl+enter'),
  'focusPaneLeft': ('alt+meta+left', 'alt+ctrl+left'),
  'focusPaneRight': ('alt+meta+right', 'alt+ctrl+right'),
  'focusPaneUp': ('alt+meta+up', 'alt+ctrl+up'),
  'focusPaneDown': ('alt+meta+down', 'alt+ctrl+down'),
  'collapseAllFolders': ('meta+alt+b', 'ctrl+alt+b'),
  'commandPalette': ('meta+k', 'ctrl+k'),
  'screenshot': ('meta+shift+s', 'ctrl+shift+s'),
  // Edit menu
  'undo': ('meta+z', 'ctrl+z'),
  'redo': ('meta+shift+z', 'ctrl+shift+z'),
  'cut': ('meta+x', 'ctrl+x'),
  'copy': ('meta+c', 'ctrl+c'),
  'paste': ('meta+v', 'ctrl+v'),
  'selectAll': ('meta+a', 'ctrl+a'),
  'find': ('meta+f', 'ctrl+f'),
  'findNext': ('meta+g', 'ctrl+g'),
  'findPrevious': ('meta+shift+g', 'ctrl+shift+g'),
  // View menu: font zoom
  'increaseFont': ('meta++', 'ctrl++'),
  'increaseFontAlt': ('meta+=', 'ctrl+='),
  'decreaseFont': ('meta+-', 'ctrl+-'),
  'resetFont': ('meta+0', 'ctrl+0'),
  // Window menu
  'closeWindow': ('meta+w', 'ctrl+w'),
  'minimize': ('meta+m', 'ctrl+m'),
  // SwiftUI view shortcuts (RootView.swift)
  'sidebarToggle': ('meta+b', 'ctrl+b'),
  'recentActivity': ('meta+shift+r', 'ctrl+shift+r'),
  // App-specific chords
  'composerFocus': ('meta+l', 'ctrl+l'),
  'equalizeSplits': ('meta+shift+e', 'ctrl+shift+e'),
  'detachPane': ('meta+shift+o', 'ctrl+shift+o'),
  'gitPull': ('meta+l', 'ctrl+l'),
  'gitPush': ('meta+p', 'ctrl+p'),
  'saveNote': ('meta+s', 'ctrl+s'),
  'editDetail': ('meta+e', 'ctrl+e'),
};

/// Resolves every [Keymap] entry for the given platform.
Map<String, String> _render({required bool isMacOS}) => {
      'settings': Keymap.settings(isMacOS: isMacOS),
      'hide': Keymap.hide(isMacOS: isMacOS),
      'hideOthers': Keymap.hideOthers(isMacOS: isMacOS),
      'quit': Keymap.quit(isMacOS: isMacOS),
      'newSession': Keymap.newSession(isMacOS: isMacOS),
      'newTerminal': Keymap.newTerminal(isMacOS: isMacOS),
      'splitRight': Keymap.splitRight(isMacOS: isMacOS),
      'splitDown': Keymap.splitDown(isMacOS: isMacOS),
      'zoomPane': Keymap.zoomPane(isMacOS: isMacOS),
      'focusPaneLeft': Keymap.focusPaneLeft(isMacOS: isMacOS),
      'focusPaneRight': Keymap.focusPaneRight(isMacOS: isMacOS),
      'focusPaneUp': Keymap.focusPaneUp(isMacOS: isMacOS),
      'focusPaneDown': Keymap.focusPaneDown(isMacOS: isMacOS),
      'collapseAllFolders': Keymap.collapseAllFolders(isMacOS: isMacOS),
      'commandPalette': Keymap.commandPalette(isMacOS: isMacOS),
      'screenshot': Keymap.screenshot(isMacOS: isMacOS),
      'undo': Keymap.undo(isMacOS: isMacOS),
      'redo': Keymap.redo(isMacOS: isMacOS),
      'cut': Keymap.cut(isMacOS: isMacOS),
      'copy': Keymap.copy(isMacOS: isMacOS),
      'paste': Keymap.paste(isMacOS: isMacOS),
      'selectAll': Keymap.selectAll(isMacOS: isMacOS),
      'find': Keymap.find(isMacOS: isMacOS),
      'findNext': Keymap.findNext(isMacOS: isMacOS),
      'findPrevious': Keymap.findPrevious(isMacOS: isMacOS),
      'increaseFont': Keymap.increaseFont(isMacOS: isMacOS),
      'increaseFontAlt': Keymap.increaseFontAlt(isMacOS: isMacOS),
      'decreaseFont': Keymap.decreaseFont(isMacOS: isMacOS),
      'resetFont': Keymap.resetFont(isMacOS: isMacOS),
      'closeWindow': Keymap.closeWindow(isMacOS: isMacOS),
      'minimize': Keymap.minimize(isMacOS: isMacOS),
      'sidebarToggle': Keymap.sidebarToggle(isMacOS: isMacOS),
      'recentActivity': Keymap.recentActivity(isMacOS: isMacOS),
      'composerFocus': Keymap.composerFocus(isMacOS: isMacOS),
      'equalizeSplits': Keymap.equalizeSplits(isMacOS: isMacOS),
      'detachPane': Keymap.detachPane(isMacOS: isMacOS),
      'gitPull': Keymap.gitPull(isMacOS: isMacOS),
      'gitPush': Keymap.gitPush(isMacOS: isMacOS),
      'saveNote': Keymap.saveNote(isMacOS: isMacOS),
      'editDetail': Keymap.editDetail(isMacOS: isMacOS),
    };

void main() {
  group('macOS keymap renders Cmd for every row', () {
    final rendered = _render(isMacOS: true);
    test('covers every declared row', () {
      expect(rendered.keys.toSet(), _primaryRows.keys.toSet());
    });
    for (final entry in _primaryRows.entries) {
      test('${entry.key} is ${entry.value.$1}', () {
        expect(rendered[entry.key], entry.value.$1);
      });
    }
  });

  group('Linux/Windows keymap renders Ctrl for every row', () {
    final rendered = _render(isMacOS: false);
    for (final entry in _primaryRows.entries) {
      test('${entry.key} is ${entry.value.$2}', () {
        expect(rendered[entry.key], entry.value.$2);
      });
    }
  });

  group('platform-neutral chords stay Ctrl on every platform', () {
    test('submit is ctrl+enter', () => expect(Keymap.submit, 'ctrl+enter'));
    test('deny is ctrl+shift+enter',
        () => expect(Keymap.deny, 'ctrl+shift+enter'));
    test('switcherNext is ctrl+tab',
        () => expect(Keymap.switcherNext, 'ctrl+tab'));
    test('switcherPrevious is ctrl+shift+tab',
        () => expect(Keymap.switcherPrevious, 'ctrl+shift+tab'));
    test('copyPath is alt+c', () => expect(Keymap.copyPath, 'alt+c'));
  });

  group('default resolves to the running platform', () {
    test('settings() matches the pinned mapping for this machine', () {
      final expected = Platform.isMacOS
          ? Keymap.settings(isMacOS: true)
          : Keymap.settings(isMacOS: false);
      expect(Keymap.settings(), expected);
    });
  });
}
