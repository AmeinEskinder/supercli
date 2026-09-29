/// Behavior tests for the session gallery panel: grid/detail state.
///
/// Ports the portable state machine from `SessionGalleryPanel.swift`:
/// grid vs detail view, artifact selection, markup mode.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/screens/session_artifacts.dart';
import 'package:supercli_app/screens/sessiongallerypanel.dart';
import 'package:test/test.dart';

SessionArtifact makeArtifact({String name = 'shot.png'}) => SessionArtifact(
      kind: 'screenshots',
      name: name,
      path: '/tmp/$name',
      size: 100,
      modifiedAt: DateTime(2026, 1, 1),
    );

void main() {
  group('SessionGalleryPanel', () {
    test('grid view when nothing selected', () {
      final panel = SessionGalleryPanel(artifacts: [makeArtifact()]);
      expect(panel.view, GalleryView.grid);
      final node = panel.build() as UiColumn;
      expect(node.id, 'session-gallery');
    });

    test('empty grid shows empty state', () {
      const panel = SessionGalleryPanel();
      final node = panel.build() as UiColumn;
      final texts = node.children.whereType<UiText>().map((t) => t.text);
      expect(texts, contains('No images yet'));
    });

    test('detail view when artifact selected', () {
      final a = makeArtifact();
      final panel = SessionGalleryPanel(
        artifacts: [a],
        selectedId: a.id,
      );
      expect(panel.view, GalleryView.detail);
      expect(panel.selected?.id, a.id);
      final node = panel.build() as UiColumn;
      expect(node.id, 'gallery-detail');
    });

    test('unknown selectedId falls back to grid', () {
      final panel = SessionGalleryPanel(
        artifacts: [makeArtifact()],
        selectedId: 'nope/missing.png',
      );
      expect(panel.view, GalleryView.grid);
      expect(panel.selected, isNull);
    });

    test('detail shows markup when markupMode', () {
      final a = makeArtifact();
      final panel = SessionGalleryPanel(
        artifacts: [a],
        selectedId: a.id,
        markupMode: true,
      );
      final node = panel.build() as UiColumn;
      // Markup toolbar is present (gallery-markup row).
      bool found = false;
      void walk(UiNode n) {
        if (n.id == 'gallery-markup') found = true;
        if (n is UiColumn) {
          for (final c in n.children) {
            walk(c);
          }
        }
        if (n is UiRow) {
          for (final c in n.children) {
            walk(c);
          }
        }
      }

      walk(node);
      expect(found, isTrue);
    });
  });
}
