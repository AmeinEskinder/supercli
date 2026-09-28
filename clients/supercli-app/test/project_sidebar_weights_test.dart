/// Behavior tests for `project_sidebar_weights.dart`
/// (port of the height-weight math in `ProjectSidebarView.swift`).
library;

import 'package:supercli_app/screens/project_sidebar_weights.dart';
import 'package:test/test.dart';

void main() {
  group('ProjectSidebarView weights (ProjectSidebarView.swift)', () {
    test('missing weight counts as 1', () {
      expect(totalWeight(['a', 'b'], {}), 2.0);
      expect(totalWeight(['a', 'b'], {'a': 3.0}), 4.0);
    });

    test('resolvedHeights splits available space by weight', () {
      final heights = resolvedHeights(
        sessionIds: ['a', 'b'],
        available: 300,
        weights: {'a': 1.0, 'b': 2.0},
      );
      expect(heights['a'], closeTo(100.0, 1e-9));
      expect(heights['b'], closeTo(200.0, 1e-9));
    });

    test('resolvedHeights accounts for the launcher extra weight', () {
      // Swift: the launcher is an ordinary weight-1 slot.
      final heights = resolvedHeights(
        sessionIds: ['a'],
        available: 200,
        weights: {},
        extraWeight: 1.0,
      );
      expect(heights['a'], closeTo(100.0, 1e-9));
    });

    test('resolvedHeights is empty for no sessions', () {
      expect(resolvedHeights(sessionIds: [], available: 300, weights: {}),
          isEmpty);
    });

    test('pruneWeights drops stale ids', () {
      expect(
          pruneWeights({'a': 2.0, 'gone': 5.0}, ['a']), {'a': 2.0});
    });

    test('decode clamps to the Swift minimum of 0.05', () {
      expect(
          decodeProjectSidebarWeights({'a': 0.01, 'b': 2.0}),
          {'a': 0.05, 'b': 2.0});
    });

    test('divider drag transfers weight keeping the pair sum invariant', () {
      final r = dividerDragStep(
          above: 1.0, below: 1.0, deltaWeight: 0.5, minWeight: 0.2);
      expect(r.above, closeTo(1.5, 1e-9));
      expect(r.below, closeTo(0.5, 1e-9));
    });

    test('divider drag clamps at the minimum pane weight', () {
      final r = dividerDragStep(
          above: 1.0, below: 1.0, deltaWeight: 5.0, minWeight: 0.2);
      expect(r.below, closeTo(0.2, 1e-9));
      expect(r.above, closeTo(1.8, 1e-9));
    });
  });
}
