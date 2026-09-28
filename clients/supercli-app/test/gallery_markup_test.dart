/// Tests for `gallery_markup_math.dart` and `screenshot_capture_mode.dart`.
library;

import 'package:supercli_app/screens/gallery_markup_math.dart';
import 'package:supercli_app/screens/screenshot_capture_mode.dart';
import 'package:test/test.dart';

void main() {
  group('GalleryMarkup (SessionGalleryMarkup.swift math)', () {
    test('palette matches iOS', () {
      expect(GalleryMarkup.palette,
          [0xEF4444, 0xEAB308, 0x22C55E, 0x3B82F6, 0xFFFFFF]);
    });

    test('lineWidth formula', () {
      expect(GalleryMarkup.lineWidth(forWidth: 100), 2.5);
      expect(GalleryMarkup.lineWidth(forWidth: 1000), closeTo(9, 0.01));
    });

    test('headLength formula', () {
      expect(GalleryMarkup.headLength(forWidth: 100), 12);
      expect(GalleryMarkup.headLength(forWidth: 1000), closeTo(38, 0.01));
    });

    test('rgb splits hex', () {
      final (r, g, b) = GalleryMarkup.rgb(0xFF0000);
      expect(r, 1.0);
      expect(g, 0.0);
      expect(b, 0.0);
    });

    test('arrowSegments returns shaft + two heads', () {
      final segs = GalleryMarkup.arrowSegments(
          startX: 0, startY: 0, endX: 10, endY: 0, headLength: 2);
      expect(segs.length, 3);
      expect(segs[0], (0.0, 0.0, 10.0, 0.0));
      // Heads go backwards from the tip.
      expect(segs[1].$3, lessThan(10.0));
      expect(segs[2].$3, lessThan(10.0));
    });

    test('fittedSize aspect-fits', () {
      final (w, h) = GalleryMarkup.fittedSize(
          imageWidth: 200,
          imageHeight: 100,
          boundsWidth: 100,
          boundsHeight: 100);
      expect(w, 100);
      expect(h, 50);
    });

    test('fittedSize guards zero dims', () {
      final (w, h) = GalleryMarkup.fittedSize(
          imageWidth: 0,
          imageHeight: 100,
          boundsWidth: 100,
          boundsHeight: 100);
      expect((w, h), (100.0, 100.0));
    });

    test('normalize clamps to 0…1', () {
      expect(
          GalleryMarkup.normalize(x: 50, y: 25, fittedWidth: 100, fittedHeight: 100),
          (0.5, 0.25));
      expect(
          GalleryMarkup.normalize(x: -10, y: 200, fittedWidth: 100, fittedHeight: 100),
          (0.0, 1.0));
    });

    test('cropPixelRect converts normalized to pixels', () {
      final (x, y, w, h) = GalleryMarkup.cropPixelRect(
          rectX: 0.25,
          rectY: 0.25,
          rectWidth: 0.5,
          rectHeight: 0.5,
          imageWidth: 100,
          imageHeight: 200);
      expect((x, y, w, h), (25, 50, 50, 100));
    });
  });

  group('ScreenshotCaptureMode (SessionScreenshotCapture.swift)', () {
    test('titles match Swift', () {
      expect(ScreenshotCaptureMode.area.title, 'Capture area');
      expect(ScreenshotCaptureMode.window.title, 'Capture window');
      expect(ScreenshotCaptureMode.screen.title, 'Capture full screen');
    });

    test('symbols match Swift', () {
      expect(ScreenshotCaptureMode.area.symbol, 'rectangle.dashed');
      expect(ScreenshotCaptureMode.window.symbol, 'macwindow');
      expect(ScreenshotCaptureMode.screen.symbol, 'display');
    });

    test('flags match screencapture argv', () {
      expect(ScreenshotCaptureMode.area.flags, ['-i']);
      expect(ScreenshotCaptureMode.window.flags, ['-i', '-W', '-o']);
      expect(ScreenshotCaptureMode.screen.flags, isEmpty);
    });

    test('screen has pre-capture delay, others do not', () {
      expect(ScreenshotCaptureMode.screen.preCaptureDelayMs, 400);
      expect(ScreenshotCaptureMode.area.preCaptureDelayMs, 0);
      expect(ScreenshotCaptureMode.window.preCaptureDelayMs, 0);
    });
  });
}
