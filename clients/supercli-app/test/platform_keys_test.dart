/// Tests for platform-specific keyboard modifiers.
///
/// Verifies that [primaryModifier] returns `meta` for macOS and `ctrl` for
/// Linux/Windows, and that [currentPrimaryModifier] matches the running
/// platform.
library;

import 'dart:io' show Platform;

import 'package:test/test.dart';

import 'package:supercli_app/platform_keys.dart';
import 'package:supercli_app/keymap.dart';

void main() {
  group('primaryModifier', () {
    test('macOS uses meta (Cmd)', () {
      expect(primaryModifier(isMacOS: true), 'meta');
    });

    test('Linux uses ctrl', () {
      expect(primaryModifier(isMacOS: false), 'ctrl');
    });

    test('both mappings produce valid gpuidart modifier chords', () {
      // The gpuidart key parser accepts: ctrl|alt|shift|meta
      const validModifiers = {'ctrl', 'alt', 'shift', 'meta'};
      expect(validModifiers, contains(primaryModifier(isMacOS: true)));
      expect(validModifiers, contains(primaryModifier(isMacOS: false)));
    });

    test('macOS and Linux mappings differ', () {
      expect(
        primaryModifier(isMacOS: true),
        isNot(primaryModifier(isMacOS: false)),
      );
    });
  });

  group('currentPrimaryModifier', () {
    test('matches the running platform', () {
      expect(
        currentPrimaryModifier,
        primaryModifier(isMacOS: Platform.isMacOS),
      );
    });

    test('is meta on macOS, ctrl elsewhere', () {
      if (Platform.isMacOS) {
        expect(currentPrimaryModifier, 'meta');
      } else {
        expect(currentPrimaryModifier, 'ctrl');
      }
    });
  });

  group('settings shortcut chord', () {
    test('resolves to meta+, on macOS (Cmd+,)', () {
      expect(Keymap.settings(isMacOS: true), 'meta+,');
    });

    test('resolves to ctrl+, on Linux/Windows', () {
      expect(Keymap.settings(isMacOS: false), 'ctrl+,');
    });
  });
}
