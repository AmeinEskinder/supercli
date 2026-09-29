/// Design tokens ported from the legacy native client
/// (`clients/legacy/native/SupercliNative/Sources/SupercliNative/Theme.swift`).
///
/// Only the portable parts are here: the theme preference enum, the folder
/// color / app tint enums with their exact hex values, layout constants, the
/// semantic color tokens, the terminal pane style data, and the pure color
/// math (alpha compositing, surface lerp). AppKit/SwiftUI-specific rendering
/// (dynamic NSColor, vibrancy, NSAppearance) has no Dart equivalent and is
/// intentionally not ported.
library;

/// The Appearance tab's `theme` value: "light" / "dark" / "system".
enum ThemePreference {
  system,
  light,
  dark;

  String get id => name;

  String get title {
    switch (this) {
      case ThemePreference.system:
        return 'System';
      case ThemePreference.light:
        return 'Light';
      case ThemePreference.dark:
        return 'Dark';
    }
  }

  static ThemePreference? fromString(String? value) {
    switch (value?.trim().toLowerCase()) {
      case 'system':
        return ThemePreference.system;
      case 'light':
        return ThemePreference.light;
      case 'dark':
        return ThemePreference.dark;
      default:
        return null;
    }
  }
}

/// Sidebar folder palette. The accent hex values are the exact sRGB
/// `#RRGGBB` contract exported to hosted terminal Apps (never an
/// appearance-dependent color archive).
enum ProjectFolderColor {
  sky,
  blue,
  violet,
  rose,
  amber,
  moss,
  teal,
  graphite;

  String get id => name;

  String get title {
    switch (this) {
      case ProjectFolderColor.sky:
        return 'Sky';
      case ProjectFolderColor.blue:
        return 'Blue';
      case ProjectFolderColor.violet:
        return 'Violet';
      case ProjectFolderColor.rose:
        return 'Rose';
      case ProjectFolderColor.amber:
        return 'Amber';
      case ProjectFolderColor.moss:
        return 'Moss';
      case ProjectFolderColor.teal:
        return 'Teal';
      case ProjectFolderColor.graphite:
        return 'Graphite';
    }
  }

  /// Stable terminal accent exported to Apps hosted in this project.
  String accentHex({required bool isDark}) {
    switch (this) {
      case ProjectFolderColor.sky:
        return isDark ? '#7DD3FC' : '#2095C9';
      case ProjectFolderColor.blue:
        return isDark ? '#7EA6FF' : '#4F73E6';
      case ProjectFolderColor.violet:
        return isDark ? '#B79CFF' : '#7B5BDA';
      case ProjectFolderColor.rose:
        return isDark ? '#F79AC0' : '#D75F8F';
      case ProjectFolderColor.amber:
        return isDark ? '#F8C86A' : '#B87511';
      case ProjectFolderColor.moss:
        return isDark ? '#9DD67A' : '#5F9A3D';
      case ProjectFolderColor.teal:
        return isDark ? '#64DCCB' : '#159B91';
      case ProjectFolderColor.graphite:
        return isDark ? '#B8BCC8' : '#687083';
    }
  }

  static ProjectFolderColor? fromString(String? value) {
    for (final c in ProjectFolderColor.values) {
      if (c.name == value?.trim().toLowerCase()) return c;
    }
    return null;
  }
}

/// Workspace-wide chrome tint (Settings ▸ Appearance ▸ App color).
enum AppTint {
  none,
  peel,
  amber,
  green,
  teal,
  blue,
  indigo,
  violet;

  String get id => name;

  String get title {
    switch (this) {
      case AppTint.none:
        return 'Default';
      case AppTint.peel:
        return 'Peel';
      case AppTint.amber:
        return 'Amber';
      case AppTint.green:
        return 'Green';
      case AppTint.teal:
        return 'Teal';
      case AppTint.blue:
        return 'Blue';
      case AppTint.indigo:
        return 'Indigo';
      case AppTint.violet:
        return 'Violet';
    }
  }

  /// Hue (degrees) the neutral chrome is washed toward; null = no wash.
  double? get hue {
    switch (this) {
      case AppTint.none:
        return null;
      case AppTint.peel:
        return 17;
      case AppTint.amber:
        return 45;
      case AppTint.green:
        return 140;
      case AppTint.teal:
        return 187;
      case AppTint.blue:
        return 212;
      case AppTint.indigo:
        return 243;
      case AppTint.violet:
        return 285;
    }
  }

  /// Exact website/mascot-family accent exported to hosted terminal Apps.
  /// Null for the neutral workspace (each App keeps its own default).
  String? get accentHex {
    switch (this) {
      case AppTint.none:
        return null;
      case AppTint.peel:
        return '#D97757';
      case AppTint.amber:
        return '#E3A63B';
      case AppTint.green:
        return '#3FBF63';
      case AppTint.teal:
        return '#4EC3C9';
      case AppTint.blue:
        return '#4FA8FF';
      case AppTint.indigo:
        return '#7A7EF2';
      case AppTint.violet:
        return '#B166E8';
    }
  }

  static AppTint? fromString(String? value) {
    for (final t in AppTint.values) {
      if (t.name == value?.trim().toLowerCase()) return t;
    }
    return null;
  }
}

/// Chrome / layout constants from `Theme`.
abstract final class ThemeLayout {
  static const double titlebarHeight = 38;
  static const double titleStripHeight = 30;
  static const double sessionRowHeight = 28;
  static const double sidebarDefaultWidth = 300;
  static const double sidebarMinWidth = 220;
  static const double sidebarMaxWidth = 520;
  static const double windowCornerRadius = 16;
  static const double surfaceInset = 8;
  static const double contentCornerRadius = 10;
}

/// Semantic color tokens (0xRRGGBB) from `Theme`.
abstract final class ThemeTokens {
  /// Attention dot (session).
  static const int attention = 0xF59E0B;
  /// Unread badge.
  static const int unread = 0x60A5FA;
  /// Danger.
  static const int danger = 0xEF4444;
  /// Control accent for native form controls and the Quick badge.
  static const int accent = 0x34C759;
  /// The two dark planes: background frames the window.
  static const int darkBackgroundHex = 0x121314;
  /// Surface carries terminal canvases and pages above it.
  static const int darkSurfaceHex = 0x1A1B1D;
  static const String darkSurfaceHexString = '#1A1B1D';
  /// Dark surface base while an App color is active.
  static const int darkTintedSurfaceHex = 0x1F2023;

  /// The dark surface base for the current tint state, brightness-lerped by
  /// the tint strength ramp so enabling an App color never pops the canvas.
  /// `tintStrength` is 0 (no tint) to 1 (full tint); null hue = neutral.
  static int currentDarkSurfaceHex({double? tintStrength}) {
    if (tintStrength == null) return darkSurfaceHex;
    final t = tintStrength.clamp(0.0, 1.0);
    int channel(int shift) {
      final a = ((darkSurfaceHex >> shift) & 0xFF).toDouble();
      final b = ((darkTintedSurfaceHex >> shift) & 0xFF).toDouble();
      return ((a + (b - a) * t).round() << shift);
    }
    return channel(16) | channel(8) | channel(0);
  }

  static String currentDarkSurfaceHexString({double? tintStrength}) {
    final hex = currentDarkSurfaceHex(tintStrength: tintStrength);
    return '#${hex.toRadixString(16).padLeft(6, '0')}';
  }
}

/// Alpha-composite `top` over `bottom` (straight alpha, sRGB), so a merged
/// wash renders identically to a two-layer stack.
/// Colors are 0xAARRGGBB; returns 0xAARRGGBB.
int flattenedColor(int bottom, int top) {
  double comp(int c, int shift) => ((c >> shift) & 0xFF) / 255.0;
  final ab = comp(bottom, 24);
  final at = comp(top, 24);
  final outA = at + ab * (1 - at);
  if (outA <= 0) return 0x00000000;
  double mix(int shift) {
    final tc = comp(top, shift);
    final bc = comp(bottom, shift);
    return (tc * at + bc * ab * (1 - at)) / outA;
  }
  final a = (outA * 255).round().clamp(0, 255);
  final r = (mix(16) * 255).round().clamp(0, 255);
  final g = (mix(8) * 255).round().clamp(0, 255);
  final b = (mix(0) * 255).round().clamp(0, 255);
  return (a << 24) | (r << 16) | (g << 8) | b;
}

/// Canvas wash scale: the terminal canvas takes a much lighter wash than the
/// chrome (2026-09-01) — gray with only a hint of the workspace color.
const double canvasWashScale = 0.35;

/// Workspace tint wash over a "#RRGGBB" string (the Ghostty theme path).
/// Mirrors `Theme.appTintedHexString(_:washScale:)`:
/// keep the base color's brightness, push its hue to the tint with a
/// brightness-scaled saturation. The curve is steep in the darks: perceived
/// chroma ≈ saturation × brightness, so near-black needs s ≈ 0.5 for a
/// visible cast while near-white takes only ~2.5%.
/// `hueDegrees` null = no tint active → returns `hex` unchanged.
String appTintedHexString(
  String hex, {
  int? hueDegrees,
  double strength = 1.0,
  double washScale = 1.0,
}) {
  var body = hex;
  if (body.startsWith('#')) body = body.substring(1);
  if (hueDegrees == null || body.length != 6) return hex;
  final rgb = int.tryParse(body, radix: 16);
  if (rgb == null) return hex;

  final r = ((rgb >> 16) & 0xFF) / 255.0;
  final g = ((rgb >> 8) & 0xFF) / 255.0;
  final b = (rgb & 0xFF) / 255.0;
  final hsb = _rgbToHsb(r, g, b);
  final s = hsb[1], brightness = hsb[2];
  // min(0.5, 0.615 - 0.59 * b): darks keep their ~50% cap, near-white
  // takes only ~2.5% cast so light-mode surfaces read as white.
  final cap = 0.615 - 0.59 * brightness;
  final capped = cap < 0.5 ? cap : 0.5;
  final scaled = capped * strength * washScale;
  final wash = s > scaled ? s : scaled; // max(s, min(0.5, cap) * strength * washScale)
  final out = _hsbToRgb((hueDegrees % 360) / 360.0, wash, brightness);
  String byte(double v) =>
      (v * 255).round().clamp(0, 255).toRadixString(16).padLeft(2, '0');
  return '#${byte(out[0])}${byte(out[1])}${byte(out[2])}';
}

/// Canvas-wash twin: must stay in lockstep with the chrome wash.
String appTintedCanvasHexString(
  String hex, {
  int? hueDegrees,
  double strength = 1.0,
}) =>
    appTintedHexString(
      hex,
      hueDegrees: hueDegrees,
      strength: strength,
      washScale: canvasWashScale,
    );

/// RGB (0..1) → HSB (hue 0..1, saturation 0..1, brightness 0..1).
List<double> _rgbToHsb(double r, double g, double b) {
  final maxC = r > g ? (r > b ? r : b) : (g > b ? g : b);
  final minC = r < g ? (r < b ? r : b) : (g < b ? g : b);
  final delta = maxC - minC;
  double h = 0;
  if (delta > 0) {
    if (maxC == r) {
      h = ((g - b) / delta) % 6 / 6;
    } else if (maxC == g) {
      h = ((b - r) / delta + 2) / 6;
    } else {
      h = ((r - g) / delta + 4) / 6;
    }
    if (h < 0) h += 1;
  }
  final s = maxC == 0 ? 0.0 : delta / maxC;
  return [h, s, maxC];
}

/// HSB → sRGB by hand so tinted grays stay in the same color space as the
/// hex tokens. Mirrors `Theme.srgbColor`.
List<double> _hsbToRgb(double hue, double saturation, double brightness) {
  final h = (hue - hue.floor()) * 6;
  final f = h - h.floor();
  final p = brightness * (1 - saturation);
  final q = brightness * (1 - saturation * f);
  final t = brightness * (1 - saturation * (1 - f));
  switch (h.floor() % 6) {
    case 0:
      return [brightness, t, p];
    case 1:
      return [q, brightness, p];
    case 2:
      return [p, brightness, t];
    case 3:
      return [p, q, brightness];
    case 4:
      return [t, p, brightness];
    default:
      return [brightness, p, q];
  }
}

/// Plain description of the terminal surface theme, one variant per
/// appearance. Values: DESIGN.md §3 (terminal/theme.ts light + dark,
/// default scheme).
final class TerminalPaneStyleVariant {
  const TerminalPaneStyleVariant({
    required this.background,
    required this.foreground,
    required this.selectionBackground,
    required this.cursorColor,
    required this.palette,
  });

  final String background;
  final String foreground;
  final String selectionBackground;
  final String cursorColor;

  /// ANSI 0–15.
  final List<String> palette;
}

/// Terminal surface theme data from `Theme.swift`'s `TerminalPaneStyle`.
final class TerminalPaneStyle {
  const TerminalPaneStyle({
    this.dark = const TerminalPaneStyleVariant(
      background: '#1A1B1D',
      foreground: '#fafafa',
      selectionBackground: '#3a3a40',
      cursorColor: '#fafafa',
      palette: [
        '#1c1c22', '#ef4444', '#22c55e', '#eab308',
        '#3b82f6', '#a855f7', '#06b6d4', '#a1a1aa',
        '#6e6e76', '#f87171', '#4ade80', '#facc15',
        '#60a5fa', '#c084fc', '#22d3ee', '#fafafa',
      ],
    ),
    this.light = const TerminalPaneStyleVariant(
      background: '#ffffff',
      foreground: '#09090b',
      selectionBackground: '#d4d4d8',
      cursorColor: '#09090b',
      palette: [
        '#09090b', '#dc2626', '#16a34a', '#ca8a04',
        '#2563eb', '#9333ea', '#0891b2', '#e4e4e7',
        '#71717a', '#ef4444', '#22c55e', '#eab308',
        '#3b82f6', '#a855f7', '#06b6d4', '#fafafa',
      ],
    ),
    this.fontSize = 13,
    this.lineHeightPercent = 0,
    this.windowPaddingX = 0,
    this.windowPaddingY = 0,
    this.windowPaddingBalanced = false,
    this.mouseScrollMultiplier = 3,
    this.fontFamily,
    this.backgroundOpacity = 1,
  });

  final TerminalPaneStyleVariant dark;
  final TerminalPaneStyleVariant light;

  /// Settings ▸ Appearance ▸ Terminal font; 13 is the shipped default.
  final double fontSize;
  /// Settings ▸ Appearance ▸ Terminal font ▸ Line height, as Ghostty's
  /// `adjust-cell-height` percentage (0 = the face's own metrics).
  final int lineHeightPercent;
  /// Runtime descriptors opt into horizontal padding. The neutral terminal
  /// style is edge-to-edge so full-bleed TUIs can own their whole canvas.
  final int windowPaddingX;
  final int windowPaddingY;
  /// Ghostty `window-padding-balance`. Keep false: balanced padding shifts
  /// the whole text block by a few pixels per frame during a window drag.
  final bool windowPaddingBalanced;
  /// Ghostty `mouse-scroll-multiplier`, discrete (wheel-tick) field only.
  final int mouseScrollMultiplier;
  /// The chosen family or the shipped stack's first installed face; null =
  /// leave Ghostty's bundled default (JetBrains Mono).
  final String? fontFamily;
  /// Ghostty `background-opacity` (Settings ▸ Appearance ▸ Transparency).
  final double backgroundOpacity;
}

/// First whitespace token's path basename, lowercased — the same
/// normalization the Host's App detection index uses.
String commandBasename(String command) {
  final head = command.split(RegExp(r'[ \t]')).firstWhere(
        (t) => t.isNotEmpty,
        orElse: () => '',
      );
  if (head.isEmpty) return '';
  final base = head.split('/').last;
  return base.toLowerCase();
}

/// Busy spinner frames (DESIGN.md §5).
const List<String> spinnerFrames = [
  '⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏',
];

/// Seconds per spinner frame.
const double spinnerInterval = 0.12;
