/// Terminal font model pure logic.
///
/// Port of the pure functions from `TerminalFontModel` in
/// `native/SupercliNative/Sources/SupercliNative/Theme.swift`.
/// The `ObservableObject`/`@Published`/`UserDefaults`/`AppKit` surface
/// (persistence, live overlay, installed-family enumeration) is platform
/// bound and not portable to Dart unit tests.
library;

/// Shipped default terminal font size.
const double terminalFontDefaultSize = 13;

/// Allowed terminal font size range (points).
const double terminalFontSizeMin = 8;
const double terminalFontSizeMax = 32;

/// Font size step for ⌘+/⌘-.
const double terminalFontSizeStep = 1;

/// Default line-height adjustment (percent of natural cell height).
const double terminalLineHeightDefault = 0;

/// Allowed line-height adjustment range (percent).
const double terminalLineHeightMin = -20;
const double terminalLineHeightMax = 100;

/// Line-height step.
const double terminalLineHeightStep = 5;

/// Normalizes a font family name: trims whitespace, empty → null.
/// Port of `TerminalFontModel.normalize(_:)`.
String? normalizeTerminalFontFamily(String? family) {
  if (family == null) return null;
  final trimmed = family.trim();
  return trimmed.isEmpty ? null : trimmed;
}

/// Clamps a font size to the settings range, snapping to the step.
/// Port of `TerminalFontModel.clamp(_:)`.
double clampTerminalFontSize(double size) {
  final stepped = (size / terminalFontSizeStep).round() * terminalFontSizeStep;
  return stepped.clamp(terminalFontSizeMin, terminalFontSizeMax);
}

/// Clamps a line-height adjustment to the settings range, snapping to step.
/// Port of `TerminalFontModel.clampLineHeight(_:)`.
double clampTerminalLineHeight(double value) {
  final stepped =
      (value / terminalLineHeightStep).round() * terminalLineHeightStep;
  return stepped.clamp(terminalLineHeightMin, terminalLineHeightMax);
}
