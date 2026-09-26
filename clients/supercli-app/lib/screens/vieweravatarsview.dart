/// Viewer presence avatars: who's watching this session.
///
/// Port of `ViewerAvatarsView.swift`.
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
