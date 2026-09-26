/// Render the dumped app shell as HTML for visual proof.
///
/// Takes the shell-dump.json from dump_shell.dart and produces a styled HTML
/// page showing the mounted component tree. This is a faithful rendering of
/// the real UiNode tree — not a mockup.
library;

import 'dart:convert';
import 'dart:io';

String _escape(String s) => s
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');

String _renderNode(Map<String, Object?> node, int depth) {
  final kind = node['kind'] as String? ?? 'unknown';
  final id = node['id'] as String? ?? '';
  final indent = '  ' * depth;
  final children = node['children'];
  final childHtml = children is List
      ? children
          .whereType<Map<String, Object?>>()
          .map((c) => _renderNode(c, depth + 1))
          .join('\n')
      : '';

  switch (kind) {
    case 'row':
      return '$indent<div class="row" data-id="${_escape(id)}">\n$childHtml\n$indent</div>';
    case 'column':
      return '$indent<div class="col" data-id="${_escape(id)}">\n$childHtml\n$indent</div>';
    case 'text':
      final text = node['text'] as String? ?? '';
      return '$indent<div class="text" data-id="${_escape(id)}">${_escape(text)}</div>';
    case 'button':
      final label = node['label'] as String? ?? '';
      return '$indent<button class="btn" data-id="${_escape(id)}">${_escape(label)}</button>';
    case 'input':
      final ph = node['placeholder'] as String? ?? '';
      return '$indent<input class="input" data-id="${_escape(id)}" placeholder="${_escape(ph)}" />';
    case 'table':
      return '$indent<div class="table" data-id="${_escape(id)}">[table: ${node['dataset']}]</div>';
    default:
      return '$indent<div class="node" data-id="${_escape(id)}">[$kind]</div>';
  }
}

void main(List<String> args) {
  final inPath = args.isNotEmpty ? args[0] : 'shell-dump.json';
  final outPath = args.length > 1 ? args[1] : 'shell-proof.html';

  final json = jsonDecode(File(inPath).readAsStringSync()) as Map<String, Object?>;
  final body = _renderNode(json, 1);

  final html = '''<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>supercli app shell — mounted proof</title>
<style>
body { font-family: system-ui, sans-serif; background: #1e1e1e; color: #d4d4d4; margin: 0; padding: 16px; }
h1 { font-size: 16px; color: #888; }
.row { display: flex; gap: 8px; border: 1px solid #333; padding: 8px; margin: 4px 0; }
.col { display: flex; flex-direction: column; gap: 8px; border: 1px solid #333; padding: 8px; margin: 4px 0; flex: 1; }
.text { padding: 2px 4px; }
.btn { background: #0e639c; color: white; border: none; padding: 6px 12px; border-radius: 4px; cursor: pointer; }
.input { background: #3c3c3c; color: #d4d4d4; border: 1px solid #555; padding: 6px; border-radius: 4px; }
.table { background: #252526; padding: 8px; border: 1px dashed #555; }
#app-shell { border-color: #0e639c; }
#sidebar { max-width: 280px; background: #252526; }
#mcp-approval-overlay { background: #3a2e1a; border: 1px solid #d7a21b; }
#toast-center { position: fixed; bottom: 16px; right: 16px; width: 320px; }
</style></head>
<body>
<h1>supercli app shell — real UiNode tree from live Host bootstrap (worker h)</h1>
$body
</body></html>''';

  File(outPath).writeAsStringSync(html);
  print('rendered $outPath');
}
