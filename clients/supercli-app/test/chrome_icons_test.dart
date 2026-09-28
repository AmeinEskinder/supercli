/// Tests for chrome_icons.dart
/// 
/// Port of ChromeIcons behavior verification.
import 'package:test/test.dart';
import '../lib/chrome_icons.dart';

void main() {
  group('SupercliChromeIcon', () {
    test('has 8 cases', () {
      expect(SupercliChromeIcon.values.length, 8);
    });

    test('assetName matches enum name', () {
      expect(SupercliChromeIcon.folderClosed.assetName, 'folderClosed');
      expect(SupercliChromeIcon.branch.assetName, 'branch');
    });

    test('branch rotates 90 degrees, others 0', () {
      expect(SupercliChromeIcon.branch.rotationDegrees, 90);
      expect(SupercliChromeIcon.folderClosed.rotationDegrees, 0);
      expect(SupercliChromeIcon.pin.rotationDegrees, 0);
    });

    test('svgSource is non-empty for all icons', () {
      for (final icon in SupercliChromeIcon.values) {
        expect(icon.svgSource.isNotEmpty, true, reason: '${icon.name} svgSource');
        expect(icon.svgSource.contains('<svg'), true, reason: '${icon.name} is SVG');
      }
    });

    test('glass icons use gradient', () {
      final svg = SupercliChromeIcon.sidebarToggle.svgSource;
      expect(svg.contains('linearGradient'), true);
      expect(svg.contains('sidebarToggleGlass'), true);
    });
  });
}
