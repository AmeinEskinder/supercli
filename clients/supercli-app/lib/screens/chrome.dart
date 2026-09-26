/// App chrome: title bar, traffic-light-adjacent controls, status bar.
///
/// Port of `Chrome.swift`. The Swift version draws the 38px custom titlebar,
/// window controls, and the bottom status bar with sync state.
library;

import 'package:gpuidart/gpuidart.dart';

final class Chrome {
  const Chrome({
    this.title = 'supercli',
    this.statusText = '',
    this.syncState = '',
  });

  final String title;
  final String statusText;
  final String syncState;

  UiNode titleBar() {
    return UiRow('chrome-titlebar', [
      const UiButton('sidebar-toggle', '☰'),
      UiText('chrome-title', title),
    ]);
  }

  UiNode statusBar() {
    return UiRow('chrome-statusbar', [
      UiText('chrome-status', statusText),
      UiText('chrome-sync', syncState),
    ]);
  }
}
