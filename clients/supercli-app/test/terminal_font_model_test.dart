/// Tests for the terminal font model pure logic.
///
/// Port of the portable cases from `TerminalFontModelTests.swift` in
/// `native/SupercliNative/Tests/SupercliNativeTests`. The `UserDefaults`
/// persistence, `Theme` mirrors, Ghostty overlay, and installed-family
/// enumeration cases are platform bound and not portable.
library;

import 'package:test/test.dart';

import '../lib/terminal/terminal_font_model.dart';

void main() {
  group('normalizeTerminalFontFamily', () {
    test('trims surrounding whitespace', () {
      expect(normalizeTerminalFontFamily('  Sarasa Mono K '), 'Sarasa Mono K');
    });

    test('blank becomes null', () {
      expect(normalizeTerminalFontFamily('   '), isNull);
      expect(normalizeTerminalFontFamily(''), isNull);
      expect(normalizeTerminalFontFamily(null), isNull);
    });
  });

  group('clampTerminalFontSize', () {
    test('clamps to the settings range', () {
      expect(clampTerminalFontSize(99), terminalFontSizeMax);
      expect(clampTerminalFontSize(2), terminalFontSizeMin);
    });

    test('snaps to the step', () {
      expect(clampTerminalFontSize(13.4), 13);
      expect(clampTerminalFontSize(13.6), 14);
    });

    test('default size is in range', () {
      expect(
        terminalFontDefaultSize,
        inInclusiveRange(terminalFontSizeMin, terminalFontSizeMax),
      );
    });
  });

  group('clampTerminalLineHeight', () {
    test('clamps to the settings range', () {
      expect(clampTerminalLineHeight(200), terminalLineHeightMax);
      expect(clampTerminalLineHeight(-50), terminalLineHeightMin);
    });

    test('snaps to the step', () {
      expect(clampTerminalLineHeight(23), 25);
      expect(clampTerminalLineHeight(22), 20);
    });
  });
}
