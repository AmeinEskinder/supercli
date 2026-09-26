/// Session screenshot capture: capture/share session screenshots.
///
/// Port of `SessionScreenshotCapture.swift`.
/// Row 189 — [DESKTOP] parity: ⇧⌘S captures into the session and attaches
/// the image to the prompt.
/// GAP (P0-6): No texture capture widget.
/// GAP: no native-window screenshot proof yet (screenshot proof pending).
library;

import 'package:gpuidart/gpuidart.dart';

import '../platform_keys.dart';

final class SessionScreenshotCapture {
  const SessionScreenshotCapture({
    this.sessionId = '',
    this.attachedToPrompt = false,
  });

  final String sessionId;
  final bool attachedToPrompt;

  UiNode build() {
    return UiRow('screenshot-capture', [
      const UiButton('screenshot-take', 'Capture Screenshot'),
      const UiButton('screenshot-copy', 'Copy'),
      const UiButton('screenshot-save', 'Save…'),
      UiButton('screenshot-attach',
          attachedToPrompt ? '☑ Attached to prompt' : 'Attach to prompt'),
    ]);
  }

  /// Row 189: ⇧⌘S takes a screenshot into the session.
  List<UiAction> actions() {
    final mod = currentPrimaryModifier;
    return [
      UiAction(
          name: 'screenshot.take',
          keys: 'shift+$mod+s',
          context: UiActionContext.node('screenshot-capture')),
    ];
  }
}
