/// Full HTML document wrapper for [DomRenderer] output.
library;

import 'dom_renderer.dart';

/// Renders a complete standalone HTML page for an App tree.
///
/// [rootJson] is the `UiNode.toJson()` map of the tree root. [title] sets
/// `<title>` and the page heading. The page includes a minimal stylesheet
/// so the structure is legible without external assets.
String renderWebPage({
  required Map<String, Object?> rootJson,
  String title = 'supercli',
  String lang = 'en',
}) {
  final renderer = const DomRenderer();
  final body = renderer.renderNode(rootJson);
  final safeTitle = _escape(title);
  return '''<!DOCTYPE html>
<html lang="${_escape(lang)}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>$safeTitle</title>
<style>
:root { color-scheme: light dark; }
body { font-family: system-ui, -apple-system, sans-serif; margin: 0; padding: 16px; }
.sc-column { display: flex; flex-direction: column; gap: 8px; }
.sc-row { display: flex; flex-direction: row; flex-wrap: wrap; gap: 8px; align-items: center; }
.sc-text { line-height: 1.4; }
.sc-button { padding: 6px 12px; border: 1px solid #888; border-radius: 6px; background: #f0f0f0; cursor: pointer; }
.sc-input { padding: 6px 10px; border: 1px solid #888; border-radius: 6px; min-width: 200px; }
.sc-table { border-collapse: collapse; }
.sc-table td, .sc-table th { border: 1px solid #888; padding: 4px 10px; }
.sc-unknown { padding: 8px; border: 1px dashed #c00; border-radius: 6px; color: #c00; }
</style>
</head>
<body>
<main aria-label="$safeTitle">
$body
</main>
</body>
</html>
''';
}

String _escape(String s) => s
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
