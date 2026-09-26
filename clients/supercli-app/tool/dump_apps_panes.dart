/// Dumps JSON snapshots of the four [APPS] panes for screenshot rendering.
library;

import 'dart:convert';
import 'dart:io';

import 'package:supercli_app/screens/filespaneview.dart';
import 'package:supercli_app/screens/gitpaneview.dart';
import 'package:supercli_app/screens/markdownpaneview.dart';
import 'package:supercli_app/screens/usagepaneview.dart';

void dump(String name, Object node) {
  final out = File('/tmp/apps-pane-$name.json');
  out.writeAsStringSync(
    const JsonEncoder.withIndent('  ').convert((node as dynamic).toJson()),
  );
  print('wrote ${out.path}');
}

void main() {
  dump('git', GitPaneView(paneId: 'shot', tab: GitPaneTab.changes).build());
  dump(
    'files',
    FilesPaneView(
      paneId: 'shot',
      selectedPath: '/home/user/projects/supercli/README.md',
    ).build(),
  );
  dump('markdown', MarkdownPaneView(paneId: 'shot').build());
  dump('usage', UsagePaneView(paneId: 'shot').build());
}
