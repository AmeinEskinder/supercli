/// Full HTML document wrapper for [DomRenderer] output.
library;

import 'dom_renderer.dart';
import 'web_events.dart';

/// Renders a complete standalone HTML page for an App tree.
///
/// [rootJson] is the `UiNode.toJson()` map of the tree root. [title] sets
/// `<title>` and the page heading. The page includes a minimal stylesheet
/// so the structure is legible without external assets.
///
/// When [eventEndpoint] is non-null, the page embeds the JS event bridge
/// ([webEventBridgeJs]) so clicks/inputs are POSTed back to the host and
/// the page becomes interactive instead of a static snapshot.
String renderWebPage({
  required Map<String, Object?> rootJson,
  String title = 'supercli',
  String lang = 'en',
  String? eventEndpoint,
}) {
  final renderer = const DomRenderer();
  final body = renderer.renderNode(rootJson);
  final safeTitle = _escape(title);
  final bridge = eventEndpoint == null
      ? ''
      : '<script>${webEventBridgeJs(endpoint: eventEndpoint)}</script>';
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
.sc-heading { margin: 0.5em 0; }
.sc-button { padding: 6px 12px; border: 1px solid #888; border-radius: 6px; background: #f0f0f0; cursor: pointer; }
.sc-button[disabled] { opacity: 0.5; cursor: not-allowed; }
.sc-input { padding: 6px 10px; border: 1px solid #888; border-radius: 6px; min-width: 200px; }
.sc-table { border-collapse: collapse; }
.sc-table td, .sc-table th { border: 1px solid #888; padding: 4px 10px; }
.sc-table tr[data-row] { cursor: pointer; }
.sc-table tr[data-row]:hover { background: #f5f5f5; }
.sc-list { margin: 0; padding-left: 1.5em; }
.sc-listitem { margin: 2px 0; }
.sc-link { color: #0b5fff; }
.sc-image { max-width: 100%; height: auto; }
.sc-unknown { padding: 8px; border: 1px dashed #c00; border-radius: 6px; color: #c00; }
</style>
</head>
<body>
<main aria-label="$safeTitle">
$body
</main>
$bridge
</body>
</html>
''';
}

String _escape(String s) => s
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
