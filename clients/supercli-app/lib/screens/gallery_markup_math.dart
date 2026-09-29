/// Session gallery markup: arrow geometry math.
///
/// Port of the portable math in `SessionGalleryMarkup.swift`
/// (`clients/legacy/native/SupercliNative/Sources/SupercliNative/Views/`).
///
/// Arrow + crop markup for the desktop session gallery's detail view — the
/// desktop twin of the iOS gallery markup (keep the palette, stroke
/// formulas, and arrow geometry in step so annotations look the same from
/// both apps). Arrows are stored in NORMALIZED image coordinates (0…1,
/// top-left origin) so they're resolution-independent.
///
/// Only the pure math is here. The SwiftUI views (GalleryAnnotatableImage,
/// GalleryCropOverlay) and CoreGraphics flatten/crop are platform code.
library;

import 'dart:math' as math;

/// A single arrow annotation in normalized image coordinates.
final class GalleryArrow {
  const GalleryArrow({
    required this.id,
    required this.startX,
    required this.startY,
    required this.endX,
    required this.endY,
    required this.colorHex,
  });

  final String id;
  final double startX;
  final double startY;
  final double endX;
  final double endY;
  final int colorHex;

  @override
  bool operator ==(Object other) =>
      other is GalleryArrow &&
      id == other.id &&
      startX == other.startX &&
      startY == other.startY &&
      endX == other.endX &&
      endY == other.endY &&
      colorHex == other.colorHex;

  @override
  int get hashCode =>
      Object.hash(id, startX, startY, endX, endY, colorHex);
}

/// Pure geometry for gallery arrow markup.
abstract final class GalleryMarkup {
  const GalleryMarkup._();

  /// Same palette as the iOS gallery markup.
  static const List<int> palette = [
    0xEF4444,
    0xEAB308,
    0x22C55E,
    0x3B82F6,
    0xFFFFFF,
  ];

  /// Stroke metrics relative to the surface being drawn on — identical
  /// formulas to iOS ArrowMarkup.
  static double lineWidth({required double forWidth}) =>
      math.max(2.5, forWidth * 0.009);

  static double headLength({required double forWidth}) =>
      math.max(12, forWidth * 0.038);

  /// Split a hex color into RGB components (0…1).
  static (double, double, double) rgb(int hex) => (
        ((hex >> 16) & 0xFF) / 255,
        ((hex >> 8) & 0xFF) / 255,
        (hex & 0xFF) / 255,
      );

  /// Arrowhead geometry: line + two head strokes.
  ///
  /// Returns the three line segments as (x1, y1, x2, y2) tuples:
  /// the shaft, then the two head strokes.
  static List<(double, double, double, double)> arrowSegments({
    required double startX,
    required double startY,
    required double endX,
    required double endY,
    required double headLength,
  }) {
    final angle = math.atan2(endY - startY, endX - startX);
    const spread = math.pi / 7;
    final leftX = endX - headLength * math.cos(angle - spread);
    final leftY = endY - headLength * math.sin(angle - spread);
    final rightX = endX - headLength * math.cos(angle + spread);
    final rightY = endY - headLength * math.sin(angle + spread);
    return [
      (startX, startY, endX, endY),
      (endX, endY, leftX, leftY),
      (endX, endY, rightX, rightY),
    ];
  }

  /// Aspect-fit of an image inside bounds. Returns (width, height).
  static (double, double) fittedSize({
    required double imageWidth,
    required double imageHeight,
    required double boundsWidth,
    required double boundsHeight,
  }) {
    if (imageWidth <= 0 ||
        imageHeight <= 0 ||
        boundsWidth <= 0 ||
        boundsHeight <= 0) {
      return (boundsWidth, boundsHeight);
    }
    final scale = math.min(
        boundsWidth / imageWidth, boundsHeight / imageHeight);
    return (imageWidth * scale, imageHeight * scale);
  }

  /// Normalize a drag point to 0…1 image coordinates, clamped.
  static (double, double) normalize({
    required double x,
    required double y,
    required double fittedWidth,
    required double fittedHeight,
  }) {
    final nx = (x / math.max(fittedWidth, 1)).clamp(0.0, 1.0);
    final ny = (y / math.max(fittedHeight, 1)).clamp(0.0, 1.0);
    return (nx, ny);
  }

  /// Convert a normalized crop rect to pixel coordinates.
  /// Returns (x, y, width, height) in pixels.
  static (int, int, int, int) cropPixelRect({
    required double rectX,
    required double rectY,
    required double rectWidth,
    required double rectHeight,
    required int imageWidth,
    required int imageHeight,
  }) {
    final x = (rectX * imageWidth).floor();
    final y = (rectY * imageHeight).floor();
    final w = math.max(1, (rectWidth * imageWidth).round());
    final h = math.max(1, (rectHeight * imageHeight).round());
    return (x, y, w, h);
  }
}
