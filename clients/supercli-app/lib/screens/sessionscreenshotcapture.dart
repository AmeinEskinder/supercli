/// Session screenshot capture: capture/share session screenshots.
///
/// Port of `SessionScreenshotCapture.swift`.
/// GAP (P0-6): No texture capture widget.
library;

import 'package:gpuidart/gpuidart.dart';

final class SessionScreenshotCapture {
  const SessionScreenshotCapture({this.sessionId = ''});

  final String sessionId;

  UiNode build() {
    return UiRow('screenshot-capture', [
      const UiButton('screenshot-take', 'Capture Screenshot'),
      const UiButton('screenshot-copy', 'Copy'),
      const UiButton('screenshot-save', 'Save…'),
    ]);
  }
}
