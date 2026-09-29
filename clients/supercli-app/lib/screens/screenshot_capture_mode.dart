/// Session screenshot capture modes.
///
/// Port of the portable `SessionScreenshotCapture.Mode` enum in
/// `SessionScreenshotCapture.swift`
/// (`clients/legacy/native/SupercliNative/Sources/SupercliNative/Views/`).
///
/// User-taken screenshots for a session, launched from the gallery chip's
/// dropdown in the terminal title bar. Uses the system `screencapture` CLI
/// rather than ScreenCaptureKit: the CLI provides the native crosshair /
/// window-picker UI for free.
///
/// Only the mode data is here. The AppKit capture flow (Process,
/// CGPreflightScreenCaptureAccess, NSMenu) is macOS platform code.
library;

/// Screenshot capture mode.
enum ScreenshotCaptureMode {
  area,
  window,
  screen;

  String get title {
    switch (this) {
      case ScreenshotCaptureMode.area:
        return 'Capture area';
      case ScreenshotCaptureMode.window:
        return 'Capture window';
      case ScreenshotCaptureMode.screen:
        return 'Capture full screen';
    }
  }

  String get symbol {
    switch (this) {
      case ScreenshotCaptureMode.area:
        return 'rectangle.dashed';
      case ScreenshotCaptureMode.window:
        return 'macwindow';
      case ScreenshotCaptureMode.screen:
        return 'display';
    }
  }

  /// `screencapture` argv. `-i` is the interactive crosshair (space
  /// toggles to window picking anyway); `-W` starts interaction in
  /// window mode with `-o` dropping the drop shadow; no flags captures
  /// the whole screen immediately.
  List<String> get flags {
    switch (this) {
      case ScreenshotCaptureMode.area:
        return ['-i'];
      case ScreenshotCaptureMode.window:
        return ['-i', '-W', '-o'];
      case ScreenshotCaptureMode.screen:
        return [];
    }
  }

  /// Milliseconds to wait before a full-screen capture, giving the
  /// dropdown's fade-out a beat so the menu isn't in the shot.
  int get preCaptureDelayMs {
    switch (this) {
      case ScreenshotCaptureMode.screen:
        return 400;
      default:
        return 0;
    }
  }

  /// Notification name for Session ▸ Take Screenshot… (⌘⇧S).
  static const String takeScreenshotNotification =
      'supercli.native.takeSessionScreenshot';
}
