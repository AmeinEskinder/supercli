/// Data hydration for the web renderer: fills tables from live host data.
///
/// The renderer (`dom_renderer.dart`) emits tables as placeholders carrying
/// `data-dataset="<id>"`, because datasets live on the host (see
/// `Host.replaceDataset` / `app.sessionDataset` in `bin/main.dart`). This
/// file provides the hydration pass that turns those placeholders into real
/// rows without re-rendering the whole tree:
///
/// 1. [TableDataset] — columns + rows, the same shape the host publishes.
/// 2. [hydrateTable] — renders a `<tbody>` for one dataset.
/// 3. [hydrateTables] — rewrites every `<table … data-dataset="…">`
///    placeholder in a rendered page with its live rows. Tables whose
///    dataset is missing keep the "Loading…" placeholder.
///
/// Dataset protocol (host → web):
/// ```json
/// { "sessions": { "columns": ["Title", "Updated"],
///                 "rows": [["a", "1m"], ["b", "2m"]] } }
/// ```
/// Values are plain strings; HTML escaping is applied at render time.
/// Row selection: each `<tr>` gets `data-row="<index>"` so the JS event
/// bridge (`web_events.dart`) can report `table_selection` events.
///
/// No fixtures: the caller passes the datasets it fetched from the live
/// host (e.g. via `HostClient` or the `/mobile/bootstrap` payload).
library;

/// A host dataset: column headers plus string rows.
final class TableDataset {
  const TableDataset({required this.columns, required this.rows});

  /// Builds from the host's dataset JSON
  /// (`{columns: [...], rows: [[...], ...]}`).
  static TableDataset? fromJson(Map<String, Object?> json) {
    final columns = json['columns'];
    final rows = json['rows'];
    if (columns is! List || rows is! List) return null;
    return TableDataset(
      columns: [for (final c in columns) c.toString()],
      rows: [
        for (final r in rows)
          if (r is List) [for (final cell in r) cell.toString()],
      ],
    );
  }

  final List<String> columns;
  final List<List<String>> rows;

  bool get isEmpty => rows.isEmpty;
}

/// Renders the `<tbody>` for [dataset], including the header row.
/// [selectedRow] marks one row `aria-selected="true"`.
String hydrateTable(TableDataset dataset, {int? selectedRow}) {
  final buf = StringBuffer()..write('<tbody>');
  buf.write('<tr>');
  for (final col in dataset.columns) {
    buf.write('<th scope="col">${_escape(col)}</th>');
  }
  buf.write('</tr>');
  if (dataset.isEmpty) {
    final span = dataset.columns.isEmpty ? 1 : dataset.columns.length;
    buf.write('<tr><td colspan="$span" aria-live="polite">No rows</td></tr>');
  } else {
    for (var i = 0; i < dataset.rows.length; i++) {
      final selected =
          selectedRow == i ? ' aria-selected="true"' : '';
      buf.write('<tr data-row="$i"$selected>');
      for (final cell in dataset.rows[i]) {
        buf.write('<td>${_escape(cell)}</td>');
      }
      buf.write('</tr>');
    }
  }
  return (buf..write('</tbody>')).toString();
}

/// Replaces every table placeholder in [html] with live rows.
///
/// A placeholder looks like the output of `DomRenderer._table`:
/// `<table … data-dataset="<id>" …><caption>…</caption><tbody>…</tbody></table>`.
/// Only the `<tbody>` is replaced; the table element (and its
/// `data-node-id`) is preserved so the event bridge keeps working.
/// Tables with no matching dataset are left untouched.
String hydrateTables(
  String html,
  Map<String, TableDataset> datasets, {
  Map<String, int>? selectedRows,
}) {
  return html.replaceAllMapped(
    RegExp(
      r'(<table\b[^>]*\bdata-dataset="([^"]+)"[^>]*>.*?<caption>.*?</caption>)'
      r'<tbody>.*?</tbody>',
      dotAll: true,
    ),
    (match) {
      final datasetId = _unescape(match.group(2) ?? '');
      final dataset = datasets[datasetId];
      if (dataset == null) return match.group(0)!;
      final selected = selectedRows?[datasetId];
      return '${match.group(1)}${hydrateTable(dataset, selectedRow: selected)}';
    },
  );
}

String _escape(String s) => s
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');

/// Reverses the attribute escaping the renderer applied to dataset ids.
String _unescape(String s) => s
    .replaceAll('&quot;', '"')
    .replaceAll('&gt;', '>')
    .replaceAll('&lt;', '<')
    .replaceAll('&amp;', '&');
