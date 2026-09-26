/// Clickable path: file path rendered as tappable segments.
///
/// Port of `ClickablePath.swift`. Each path segment is a button; clicking
/// reveals the file in the workspace.
library;

import 'package:gpuidart/gpuidart.dart';

final class ClickablePath {
  const ClickablePath({required this.path});

  final String path;

  UiNode build() {
    final segments = path.split('/').where((s) => s.isNotEmpty).toList();
    return UiRow('clickable-path', [
      for (var i = 0; i < segments.length; i++) ...[
        if (i > 0) const UiText('path-sep', '/'),
        UiButton('path-seg-$i', segments[i]),
      ],
    ]);
  }
}
