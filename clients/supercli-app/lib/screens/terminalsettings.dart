/// Terminal display settings: font size controls.
///
/// Port of the font-size handling in `TerminalSettings.swift`.
/// Row 178 — [DESKTOP] parity: ⌘+ / ⌘- / ⌘0 (platform primary modifier).
/// GAP: no native-window screenshot proof yet (screenshot proof pending).
library;

import 'package:gpuidart/gpuidart.dart';

import '../platform_keys.dart';

/// Bounds for terminal font size (points).
const double minTerminalFontSize = 8;
const double maxTerminalFontSize = 32;
const double defaultTerminalFontSize = 13;

/// Controller for terminal font size with increase/decrease/reset.
final class TerminalFontSize {
  TerminalFontSize([this.size = defaultTerminalFontSize]);

  double size;

  void increase() {
    size = (size + 1).clamp(minTerminalFontSize, maxTerminalFontSize);
  }

  void decrease() {
    size = (size - 1).clamp(minTerminalFontSize, maxTerminalFontSize);
  }

  void reset() {
    size = defaultTerminalFontSize;
  }

  UiNode build() {
    return UiRow('terminal-font-size', [
      const UiButton('font-decrease', 'A−'),
      UiText('font-size-label', '${size.toStringAsFixed(0)}pt'),
      const UiButton('font-increase', 'A+'),
      const UiButton('font-reset', 'Reset'),
    ]);
  }

  List<UiAction> actions() {
    final mod = currentPrimaryModifier;
    return [
      UiAction(
          name: 'font.increase',
          keys: '$mod+plus',
          context: const UiActionContext.node('terminal-font-size')),
      UiAction(
          name: 'font.decrease',
          keys: '$mod+minus',
          context: const UiActionContext.node('terminal-font-size')),
      UiAction(
          name: 'font.reset',
          keys: '$mod+0',
          context: const UiActionContext.node('terminal-font-size')),
    ];
  }
}
