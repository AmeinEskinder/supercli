/// Session gallery: screenshot/card gallery of sessions.
///
/// Port of `SessionGalleryPanel.swift` and `SessionGalleryMarkup.swift`.
/// The Swift version shows session screenshots as cards with annotation
/// markup tools. The gpuidart port lists sessions; screenshots need P0-6.
///
/// GAP (P0-6): No image/texture widget. Gallery shows titles only.
library;

import 'package:gpuidart/gpuidart.dart';

import '../models.dart';

final class SessionGalleryPanel {
  const SessionGalleryPanel({this.sessions = const []});

  final List<SessionSummary> sessions;

  UiNode build() {
    return UiColumn('session-gallery', [
      const UiText('gallery-title', 'Session Gallery'),
      UiTable('gallery-table', dataset: 'session-gallery'),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'session-gallery',
        columns: const ['Session', 'Updated'],
        rows: sessions.map((s) => [s.title, '']).toList(),
      );
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
