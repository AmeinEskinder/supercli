/// Builds a sample split-pane layout and dumps its JSON snapshot.
library;

import 'dart:convert';
import 'dart:io';


import 'package:supercli_app/screens/terminalarea.dart';
import 'package:supercli_app/screens/terminalfindbar.dart';

void main() {
  // Build a 3-pane layout: left pane + right side split vertically.
  var layout = PaneLayout.single(paneId: 'p1', title: 'zsh — api-server');
  layout = layout.split(
    direction: SplitDirection.horizontal,
    newPaneId: 'p2',
    newTitle: 'vim — main.dart',
  )!;
  layout = layout.focus('p2').split(
    direction: SplitDirection.vertical,
    newPaneId: 'p3',
    newTitle: 'zsh — tests',
  )!;
  // Focus the middle pane to show the focus marker.
  layout = layout.focus('p2');

  final area = TerminalArea(
    layout: layout,
    findBar: const TerminalFindBar(
      query: 'test',
      matchIndex: 2,
      matchCount: 7,
    ),
    statusText: '3 panes · p2 focused · ⌘D split · ⇧⌘↩ zoom',
  );
  final node = area.build();
  final out = File('/tmp/pane-layout-snapshot.json');
  out.writeAsStringSync(
      const JsonEncoder.withIndent('  ').convert(node.toJson()));
  print('wrote ${out.path}');
}
