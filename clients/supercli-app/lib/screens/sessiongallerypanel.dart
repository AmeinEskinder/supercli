/// Session gallery: screenshot/card gallery of sessions.
///
/// Port of `SessionGalleryPanel.swift`.
/// The Swift version shows session screenshots as cards with annotation
/// markup tools. The gpuidart port lists artifacts; image rendering needs
/// P0-6 (no image/texture widget), markup canvas needs P0-12.
///
/// The grid/detail state machine (selected artifact, markup mode) is
/// ported. SwiftUI rendering (GalleryTile, GalleryAnnotatableImage,
/// NSImage/CGImage loading, the title-bar SessionGalleryButton chip) is
/// AppKit-only and dropped.
///
/// GAP (P0-6): No image/texture widget. Gallery shows titles only.
library;

import 'package:gpuidart/gpuidart.dart';

import 'session_artifacts.dart';

/// Which view the gallery panel is showing.
enum GalleryView {
  grid,
  detail,
}

final class SessionGalleryPanel {
  const SessionGalleryPanel({
    this.artifacts = const [],
    this.selectedId,
    this.markupMode = false,
  });

  final List<SessionArtifact> artifacts;

  /// ID of the artifact in the detail view. Null = grid view.
  final String? selectedId;

  /// True when the markup toolbar is active on the detail view.
  final bool markupMode;

  GalleryView get view => selectedId == null ? GalleryView.grid : GalleryView.detail;

  SessionArtifact? get selected {
    final id = selectedId;
    if (id == null) return null;
    for (final a in artifacts) {
      if (a.id == id) return a;
    }
    return null;
  }

  UiNode build() {
    final sel = selected;
    if (sel != null) {
      return _detail(sel);
    }
    return _grid();
  }

  UiNode _grid() {
    return UiColumn('session-gallery', [
      const UiText('gallery-title', 'Session Gallery'),
      if (artifacts.isNotEmpty)
        UiText('gallery-count', '${artifacts.length}'),
      if (artifacts.isEmpty)
        const UiText('gallery-empty', 'No images yet'),
      for (final a in artifacts)
        UiRow('gallery-tile-${a.id}', [
          UiText('gallery-tile-name-${a.id}', a.name),
          UiText('gallery-tile-kind-${a.id}', a.kind),
        ]),
    ]);
  }

  UiNode _detail(SessionArtifact artifact) {
    return UiColumn('gallery-detail', [
      UiRow('gallery-detail-header', [
        const UiButton('gallery-back', 'Back'),
        UiText('gallery-detail-name', artifact.name),
      ]),
      UiText('gallery-detail-kind', artifact.kind),
      if (markupMode)
        const SessionGalleryMarkup().build(),
      UiRow('gallery-detail-actions', [
        const UiButton('gallery-add-prompt', 'Add to prompt'),
        const UiButton('gallery-reveal', 'Reveal'),
        const UiButton('gallery-delete', 'Delete'),
      ]),
    ]);
  }
}

/// Annotation markup toolbar for gallery screenshots.
///
/// Port of `SessionGalleryMarkup.swift`. Tools: pen, arrow, box, text.
/// GAP: No canvas drawing primitive in gpuidart (P0-12).
final class SessionGalleryMarkup {
  const SessionGalleryMarkup({this.activeTool = 'pen'});

  final String activeTool;

  UiNode build() {
    return UiRow('gallery-markup', [
      for (final tool in ['pen', 'arrow', 'box', 'text'])
        UiButton('markup-$tool', '${tool == activeTool ? '● ' : ''}$tool'),
      const UiButton('markup-clear', 'Clear'),
      const UiButton('markup-save', 'Save'),
    ]);
  }
}
