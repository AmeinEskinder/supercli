/// Session gallery: screenshot/card gallery of sessions.
///
/// Port of `SessionGalleryPanel.swift` and `SessionGalleryMarkup.swift`.
/// The Swift version shows session screenshots as cards with annotation
/// markup tools. The gpuidart port lists sessions; screenshots need P0-6.
///
/// Rows 187, 188 — [DESKTOP] parity.
/// GAP (P0-6): No image/texture widget. Gallery shows titles only.
/// GAP: no native-window screenshot proof yet (screenshot proof pending).
library;

import 'package:gpuidart/gpuidart.dart';

import '../models.dart';

/// Gallery tabs: screenshots, downloads, uploads.
enum GalleryTab { screenshots, downloads, uploads }

final class SessionGalleryPanel {
  const SessionGalleryPanel({
    this.sessions = const [],
    this.tab = GalleryTab.screenshots,
  });

  final List<SessionSummary> sessions;
  final GalleryTab tab;

  UiNode build() {
    return UiColumn('session-gallery', [
      const UiText('gallery-title', 'Session Gallery'),
      UiRow('gallery-tabs', [
        for (final t in GalleryTab.values)
          UiButton('gallery-tab-${t.name}',
              '${t == tab ? '● ' : ''}${_tabLabel(t)}'),
      ]),
      UiTable('gallery-table', dataset: 'session-gallery'),
    ]);
  }

  static String _tabLabel(GalleryTab t) => switch (t) {
        GalleryTab.screenshots => 'Screenshots',
        GalleryTab.downloads => 'Downloads',
        GalleryTab.uploads => 'Uploads',
      };

  TableDataset dataset() => TableDataset(
        'session-gallery',
        columns: const ['Session', 'Updated'],
        rows: sessions.map((s) => [s.title, '']).toList(),
      );
}

/// Annotation markup toolbar for gallery screenshots.
///
/// Port of `SessionGalleryMarkup.swift`.
/// Row 188 — [DESKTOP] parity: arrow + crop markup tools and "Add to prompt".
/// GAP: No canvas drawing primitive in gpuidart (P0-12); the toolbar records
/// the selected tool and pending annotations as data, rendered once a canvas
/// primitive exists.
final class SessionGalleryMarkup {
  const SessionGalleryMarkup({
    this.activeTool = 'pen',
    this.annotations = const [],
  });

  final String activeTool;

  /// Pending annotations as data: {tool, x1, y1, x2, y2}.
  final List<Map<String, Object>> annotations;

  UiNode build() {
    return UiColumn('gallery-markup', [
      UiRow('gallery-markup-tools', [
        for (final tool in ['pen', 'arrow', 'box', 'text', 'crop'])
          UiButton('markup-$tool', '${tool == activeTool ? '● ' : ''}$tool'),
        const UiButton('markup-clear', 'Clear'),
        const UiButton('markup-save', 'Save'),
      ]),
      UiText('markup-count', '${annotations.length} annotations'),
      const UiButton('markup-add-to-prompt', 'Add to prompt'),
    ]);
  }
}
