/// Plugin list drag pure functions.
/// 
/// Port of the portable logic from `PluginListDrag.swift`.
/// The AppKit/SwiftUI drag controller (NSView, NSEvent, NSPanel) is not
/// portable to Dart; only the pure reorder/offset/geometry functions are ported.
library;

/// Pure drag-reorder calculations for the plugin settings list.
abstract final class PluginListDrag {
  /// Calculates the vertical offset for a row during drag.
  /// 
  /// Rows between source and target shift by one stride to make room.
  static double slotOffset({
    required int index,
    required int source,
    required int target,
    required double stride,
  }) {
    if (source < target && index > source && index <= target) return -stride;
    if (source > target && index >= target && index < source) return stride;
    return 0;
  }

  /// Returns a new list with the item moved from source to target index.
  static List<String> reordered(List<String> ids, int source, int target) {
    if (source < 0 || source >= ids.length) return ids;
    if (target < 0 || target >= ids.length) return ids;
    final result = List<String>.from(ids);
    final id = result.removeAt(source);
    result.insert(target, id);
    return result;
  }

  /// Calculates the destination frame for the drag card.
  /// 
  /// Returns null if source or target are out of bounds.
  /// (Frame is represented as a map with 'y' and 'height' for portability.)
  static Map<String, double>? destinationFrame({
    required List<Map<String, double>> frames,
    required int source,
    required int target,
  }) {
    if (source < 0 || source >= frames.length) return null;
    if (target < 0 || target >= frames.length) return null;
    
    final sourceFrame = frames[source];
    final targetFrame = frames[target];
    final height = sourceFrame['height'] ?? 0;
    final y = source < target
        ? (targetFrame['maxY'] ?? 0) - height
        : (targetFrame['minY'] ?? 0);
    
    return {'y': y, 'height': height};
  }
}
