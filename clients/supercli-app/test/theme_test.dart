import 'package:test/test.dart';

import 'package:supercli_app/theme.dart';

void main() {
  group('ThemePreference', () {
    test('titles match the Appearance tab', () {
      expect(ThemePreference.system.title, 'System');
      expect(ThemePreference.light.title, 'Light');
      expect(ThemePreference.dark.title, 'Dark');
    });

    test('fromString parses the Tauri theme values', () {
      expect(ThemePreference.fromString('system'), ThemePreference.system);
      expect(ThemePreference.fromString('light'), ThemePreference.light);
      expect(ThemePreference.fromString('dark'), ThemePreference.dark);
      expect(ThemePreference.fromString('DARK'), ThemePreference.dark);
      expect(ThemePreference.fromString('neon'), isNull);
      expect(ThemePreference.fromString(null), isNull);
    });
  });

  group('ProjectFolderColor', () {
    test('has eight cases with titles', () {
      expect(ProjectFolderColor.values.length, 8);
      expect(ProjectFolderColor.sky.title, 'Sky');
      expect(ProjectFolderColor.graphite.title, 'Graphite');
    });

    test('accentHex matches the Swift contract exactly', () {
      expect(ProjectFolderColor.sky.accentHex(isDark: false), '#2095C9');
      expect(ProjectFolderColor.sky.accentHex(isDark: true), '#7DD3FC');
      expect(ProjectFolderColor.blue.accentHex(isDark: false), '#4F73E6');
      expect(ProjectFolderColor.blue.accentHex(isDark: true), '#7EA6FF');
      expect(ProjectFolderColor.violet.accentHex(isDark: false), '#7B5BDA');
      expect(ProjectFolderColor.violet.accentHex(isDark: true), '#B79CFF');
      expect(ProjectFolderColor.rose.accentHex(isDark: false), '#D75F8F');
      expect(ProjectFolderColor.rose.accentHex(isDark: true), '#F79AC0');
      expect(ProjectFolderColor.amber.accentHex(isDark: false), '#B87511');
      expect(ProjectFolderColor.amber.accentHex(isDark: true), '#F8C86A');
      expect(ProjectFolderColor.moss.accentHex(isDark: false), '#5F9A3D');
      expect(ProjectFolderColor.moss.accentHex(isDark: true), '#9DD67A');
      expect(ProjectFolderColor.teal.accentHex(isDark: false), '#159B91');
      expect(ProjectFolderColor.teal.accentHex(isDark: true), '#64DCCB');
      expect(ProjectFolderColor.graphite.accentHex(isDark: false), '#687083');
      expect(ProjectFolderColor.graphite.accentHex(isDark: true), '#B8BCC8');
    });

    test('fromString round-trips', () {
      expect(ProjectFolderColor.fromString('teal'), ProjectFolderColor.teal);
      expect(ProjectFolderColor.fromString('nope'), isNull);
    });
  });

  group('AppTint', () {
    test('hues match the website agent-accent palette', () {
      expect(AppTint.none.hue, isNull);
      expect(AppTint.peel.hue, 17);
      expect(AppTint.amber.hue, 45);
      expect(AppTint.green.hue, 140);
      expect(AppTint.teal.hue, 187);
      expect(AppTint.blue.hue, 212);
      expect(AppTint.indigo.hue, 243);
      expect(AppTint.violet.hue, 285);
    });

    test('accentHex matches the Swift contract exactly', () {
      expect(AppTint.none.accentHex, isNull);
      expect(AppTint.peel.accentHex, '#D97757');
      expect(AppTint.amber.accentHex, '#E3A63B');
      expect(AppTint.green.accentHex, '#3FBF63');
      expect(AppTint.teal.accentHex, '#4EC3C9');
      expect(AppTint.blue.accentHex, '#4FA8FF');
      expect(AppTint.indigo.accentHex, '#7A7EF2');
      expect(AppTint.violet.accentHex, '#B166E8');
    });
  });

  group('ThemeLayout', () {
    test('constants match the Swift values', () {
      expect(ThemeLayout.titlebarHeight, 38);
      expect(ThemeLayout.titleStripHeight, 30);
      expect(ThemeLayout.sessionRowHeight, 28);
      expect(ThemeLayout.sidebarDefaultWidth, 300);
      expect(ThemeLayout.sidebarMinWidth, 220);
      expect(ThemeLayout.sidebarMaxWidth, 520);
      expect(ThemeLayout.surfaceInset, 8);
      expect(ThemeLayout.contentCornerRadius, 10);
    });
  });

  group('ThemeTokens', () {
    test('semantic tokens match the Swift hex values', () {
      expect(ThemeTokens.attention, 0xF59E0B);
      expect(ThemeTokens.unread, 0x60A5FA);
      expect(ThemeTokens.danger, 0xEF4444);
      expect(ThemeTokens.accent, 0x34C759);
      expect(ThemeTokens.darkBackgroundHex, 0x121314);
      expect(ThemeTokens.darkSurfaceHex, 0x1A1B1D);
      expect(ThemeTokens.darkSurfaceHexString, '#1A1B1D');
      expect(ThemeTokens.darkTintedSurfaceHex, 0x1F2023);
    });

    test('currentDarkSurfaceHex lerps between the surface bases', () {
      expect(
        ThemeTokens.currentDarkSurfaceHex(),
        ThemeTokens.darkSurfaceHex,
      );
      expect(
        ThemeTokens.currentDarkSurfaceHex(tintStrength: 0),
        ThemeTokens.darkSurfaceHex,
      );
      expect(
        ThemeTokens.currentDarkSurfaceHex(tintStrength: 1),
        ThemeTokens.darkTintedSurfaceHex,
      );
      // Midpoint: 0x1A1B1D -> 0x1F2023 lerps each channel halfway.
      final mid = ThemeTokens.currentDarkSurfaceHex(tintStrength: 0.5);
      expect(mid, 0x1D1E20);
      expect(
        ThemeTokens.currentDarkSurfaceHexString(tintStrength: 1),
        '#1f2023',
      );
    });
  });

  group('flattenedColor', () {
    test('opaque top wins', () {
      expect(flattenedColor(0xFF000000, 0xFFFFFFFF), 0xFFFFFFFF);
    });

    test('transparent top keeps bottom', () {
      expect(flattenedColor(0xFF112233, 0x00000000), 0xFF112233);
    });

    test('half alpha blends toward top', () {
      // White @ 50% over black = mid gray.
      final out = flattenedColor(0xFF000000, 0x80FFFFFF);
      final r = (out >> 16) & 0xFF;
      expect(r, inInclusiveRange(126, 130));
      expect((out >> 24) & 0xFF, 0xFF);
    });
  });

  group('TerminalPaneStyle', () {
    test('dark variant matches the default-scheme values', () {
      const style = TerminalPaneStyle();
      expect(style.dark.background, '#1A1B1D');
      expect(style.dark.foreground, '#fafafa');
      expect(style.dark.selectionBackground, '#3a3a40');
      expect(style.dark.cursorColor, '#fafafa');
      expect(style.dark.palette.length, 16);
      expect(style.dark.palette[0], '#1c1c22');
      expect(style.dark.palette[8], '#6e6e76'); // brightBlack override
      expect(style.dark.palette[15], '#fafafa');
    });

    test('light variant matches terminal/theme.ts', () {
      const style = TerminalPaneStyle();
      expect(style.light.background, '#ffffff');
      expect(style.light.foreground, '#09090b');
      expect(style.light.palette.length, 16);
      expect(style.light.palette[1], '#dc2626');
    });

    test('Ghostty defaults match the Swift struct', () {
      const style = TerminalPaneStyle();
      expect(style.fontSize, 13);
      expect(style.lineHeightPercent, 0);
      expect(style.windowPaddingX, 0);
      expect(style.windowPaddingY, 0);
      expect(style.windowPaddingBalanced, isFalse);
      expect(style.mouseScrollMultiplier, 3);
      expect(style.fontFamily, isNull);
      expect(style.backgroundOpacity, 1);
    });
  });

  group('commandBasename', () {
    test('takes the first token path basename lowercased', () {
      expect(commandBasename('claude --dangerously-skip-permissions'), 'claude');
      expect(commandBasename('/usr/local/bin/OpenCode --help'), 'opencode');
      expect(commandBasename('  GROK  --light'), 'grok');
      expect(commandBasename(''), '');
      expect(commandBasename('   '), '');
    });
  });

  group('spinner', () {
    test('frames and interval match DESIGN.md §5', () {
      expect(spinnerFrames.length, 10);
      expect(spinnerFrames.first, '⠋');
      expect(spinnerFrames.last, '⠏');
      expect(spinnerInterval, 0.12);
    });
  });

  group('appTintedHexString', () {
    test('no tint returns the hex unchanged', () {
      expect(appTintedHexString('#1A1B1D'), '#1A1B1D');
      expect(appTintedHexString('#1A1B1D', hueDegrees: null), '#1A1B1D');
    });

    test('invalid hex returns unchanged', () {
      expect(appTintedHexString('xyz', hueDegrees: 212), 'xyz');
      expect(appTintedHexString('#12345', hueDegrees: 212), '#12345');
    });

    test('mid gray takes the tint hue at the brightness-scaled wash', () {
      // #808080, blue hue 212: wash = min(0.5, 0.615-0.59*0.502) ≈ 0.319.
      expect(
        appTintedHexString('#808080', hueDegrees: 212),
        '#576a80',
      );
    });

    test('canvas wash is lighter than the chrome wash', () {
      expect(
        appTintedCanvasHexString('#808080', hueDegrees: 212),
        '#727880',
      );
      expect(
        appTintedHexString('#808080', hueDegrees: 212, washScale: canvasWashScale),
        appTintedCanvasHexString('#808080', hueDegrees: 212),
      );
    });

    test('already-saturated colors keep their saturation', () {
      // Pure red keeps s=1 (wash = max(1, tiny cap)) but takes the blue hue.
      expect(
        appTintedHexString('#FF0000', hueDegrees: 212),
        '#0077ff',
      );
    });

    test('strength 0 leaves only the original saturation', () {
      // wash = max(s, cap*0) = max(0, 0) = 0 → neutral gray, same brightness.
      expect(
        appTintedHexString('#808080', hueDegrees: 212, strength: 0),
        '#808080',
      );
    });
  });
}
