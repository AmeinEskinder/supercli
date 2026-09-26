/// Viewer presence avatars: who's watching this session.
///
/// Port of `ViewerAvatarsView.swift`. Row 185 — [DESKTOP] parity.
/// GAP: no native-window screenshot proof yet (screenshot proof pending).
library;

import 'package:gpuidart/gpuidart.dart';

final class ViewerAvatarsView {
  const ViewerAvatarsView({this.viewers = const []});

  final List<String> viewers;

  UiNode build() {
    if (viewers.isEmpty) return UiRow('viewers-empty', []);
    return UiRow('viewer-avatars', [
      for (var i = 0; i < viewers.length; i++)
        UiText('viewer-$i', viewers[i].isNotEmpty ? viewers[i][0] : '?'),
    ]);
  }
}

/// Row 185: "Fit to desktop" control — scales the shared session view so a
/// remote viewer sees the whole desktop surface instead of scrolling.
final class FitToDesktopControl {
  const FitToDesktopControl({this.enabled = false});

  final bool enabled;

  UiNode build() {
    return UiRow('fit-to-desktop', [
      UiButton('fit-desktop-toggle', enabled ? '☑ Fit to desktop' : '☐ Fit to desktop'),
    ]);
  }
}
