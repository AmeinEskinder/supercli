/// Tests for plugin_list_drag.dart
/// 
/// Port of PluginListDragTests.swift pure function behaviors.
library;
import 'package:test/test.dart';
import 'package:supercli_app/plugin_list_drag.dart';

void main() {
  group('PluginListDrag.reordered', () {
    test('moves item from source to target', () {
      final ids = ['claude', 'codex', 'markdown', 'git'];
      expect(
        PluginListDrag.reordered(ids, 0, 2),
        ['codex', 'markdown', 'claude', 'git'],
      );
    });

    test('returns original if source out of bounds', () {
      final ids = ['a', 'b'];
      expect(PluginListDrag.reordered(ids, 5, 0), ['a', 'b']);
    });

    test('returns original if target out of bounds', () {
      final ids = ['a', 'b'];
      expect(PluginListDrag.reordered(ids, 0, 5), ['a', 'b']);
    });
  });

  group('PluginListDrag.slotOffset', () {
    test('rows between source and target shift', () {
      // Dragging from 0 to 2: indices 1 and 2 shift up by stride
      expect(
        PluginListDrag.slotOffset(index: 1, source: 0, target: 2, stride: 128),
        -128,
      );
      expect(
        PluginListDrag.slotOffset(index: 2, source: 0, target: 2, stride: 128),
        -128,
      );
      expect(
        PluginListDrag.slotOffset(index: 3, source: 0, target: 2, stride: 128),
        0,
      );
    });

    test('no offset when source == target', () {
      expect(
        PluginListDrag.slotOffset(index: 1, source: 0, target: 0, stride: 128),
        0,
      );
    });

    test('rows shift down when dragging upward', () {
      // Dragging from 2 to 0: indices 0 and 1 shift down by stride
      expect(
        PluginListDrag.slotOffset(index: 0, source: 2, target: 0, stride: 128),
        128,
      );
      expect(
        PluginListDrag.slotOffset(index: 1, source: 2, target: 0, stride: 128),
        128,
      );
    });
  });

  group('PluginListDrag.destinationFrame', () {
    test('calculates destination for downward drag', () {
      final frames = [
        {'minY': 0.0, 'maxY': 100.0, 'height': 100.0},
        {'minY': 100.0, 'maxY': 200.0, 'height': 100.0},
        {'minY': 200.0, 'maxY': 300.0, 'height': 100.0},
      ];
      final result = PluginListDrag.destinationFrame(
        frames: frames,
        source: 0,
        target: 2,
      );
      // Source < target: y = target.maxY - height = 300 - 100 = 200
      expect(result?['y'], 200);
      expect(result?['height'], 100);
    });

    test('returns null for out of bounds', () {
      final frames = [
        {'minY': 0.0, 'maxY': 100.0, 'height': 100.0},
      ];
      expect(
        PluginListDrag.destinationFrame(frames: frames, source: 0, target: 5),
        isNull,
      );
    });
  });
}
