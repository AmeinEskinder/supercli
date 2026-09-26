/// Folder color palette (8 colors) for projects and groups.
///
/// Port of the unpeel folder-color picker: 8 named colors assignable to a
/// project folder or session group, rendered as a swatch row with a "None"
/// clear option. The color is stored as `#RRGGBB` on [SidebarProject.folderColor]
/// and [SidebarGroup.color] (both already exist in sidebarview.dart).
library;

import 'package:gpuidart/gpuidart.dart';

/// One entry in the 8-color folder palette.
final class FolderColor {
  const FolderColor({required this.name, required this.hex});

  final String name;
  final String hex;
}

/// The 8 folder colors (same hues as unpeel).
const List<FolderColor> folderColors = [
  FolderColor(name: 'Red', hex: '#E5484D'),
  FolderColor(name: 'Orange', hex: '#F76B15'),
  FolderColor(name: 'Yellow', hex: '#FFB224'),
  FolderColor(name: 'Green', hex: '#30A46C'),
  FolderColor(name: 'Blue', hex: '#3E63DD'),
  FolderColor(name: 'Purple', hex: '#8E4EC6'),
  FolderColor(name: 'Pink', hex: '#D6409F'),
  FolderColor(name: 'Gray', hex: '#8B8D98'),
];

/// Swatch picker for a folder color. `selected` is the current `#RRGGBB`
/// or null for no color. `onSelect` receives the picked hex or null when
/// the user clears the color.
final class FolderColorPalette {
  const FolderColorPalette({this.selected, this.onSelect});

  final String? selected;
  final void Function(String? hex)? onSelect;

  UiNode build() {
    return UiColumn('folder-color-palette', [
      const UiText('folder-color-title', 'Folder color'),
      UiRow('folder-color-swatches', [
        for (final c in folderColors)
          UiButton(
            'folder-color-${c.hex}',
            '${c.name}${selected == c.hex ? ' ✓' : ''}',
          ),
        UiButton('folder-color-none', 'None${selected == null ? ' ✓' : ''}'),
      ]),
    ]);
  }

  /// Resolve the tap action for a swatch button id back to a color hex
  /// (or null for the clear button). Returns null for unknown ids.
  static String? colorForAction(String actionId) {
    if (actionId == 'folder-color-none') return null;
    for (final c in folderColors) {
      if (actionId == 'folder-color-${c.hex}') return c.hex;
    }
    return null;
  }

  /// True when [actionId] is one of this palette's buttons.
  static bool isPaletteAction(String actionId) {
    if (actionId == 'folder-color-none') return true;
    return folderColors.any((c) => actionId == 'folder-color-${c.hex}');
  }
}
