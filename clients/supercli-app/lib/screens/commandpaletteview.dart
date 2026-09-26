/// Command palette: fuzzy search over commands.
///
/// Port of `CommandPaletteView.swift`. The Swift version is a floating
/// panel with a filter field and results list (keyboard navigable).
/// The gpuidart port uses UiInput + UiTable.
library;

import 'package:gpuidart/gpuidart.dart';

/// A command in the palette.
final class PaletteCommand {
  const PaletteCommand({
    required this.id,
    required this.title,
    this.subtitle = '',
    this.shortcut = '',
  });

  final String id;
  final String title;
  final String subtitle;
  final String shortcut;
}

/// The command palette overlay.
final class CommandPaletteView {
  CommandPaletteView({
    this.commands = const [],
    this.filter = '',
  });

  final List<PaletteCommand> commands;
  final String filter;

  UiNode build() {
    return UiColumn('command-palette', [
      const UiInput('palette-filter', placeholder: 'Type a command…'),
      UiTable('palette-results',
          dataset: 'palette-results'),
    ]);
  }

  TableDataset dataset() {
    final filtered = commands
        .where((c) =>
            filter.isEmpty ||
            c.title.toLowerCase().contains(filter.toLowerCase()))
        .toList();
    return TableDataset(
      'palette-results',
      columns: const ['Command', 'Shortcut'],
      rows: filtered.map((c) => [c.title, c.shortcut]).toList(),
    );
  }

  List<UiAction> actions() => const [
        UiAction(name: 'palette.open', keys: 'cmd+shift+p'),
        UiAction(name: 'palette.close', keys: 'escape',
            context: UiActionContext.node('command-palette')),
      ];
}
