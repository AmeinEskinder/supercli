/// Proof-of-concept DOM/ARIA renderer for the supercli App tree.
///
/// The App tree is the `UiNode` tree produced by the App Kit widgets in
/// `lib/widgets/` (Page/List/Input/TextBox/Tree/Menu/...). Every node
/// serializes with `UiNode.toJson()` to a JSON map with a `kind` field
/// (`column`, `row`, `text`, `button`, `input`, `table`). This renderer
/// consumes that same JSON and produces an HTML fragment with ARIA roles,
/// so the *same* App tree can be presented in a browser without a native
/// gpuidart host.
///
/// Status: proof of concept. It covers the six node kinds that exist in
/// upstream gpuidart (`clients/gpuidart` @ 135d300). Unknown kinds render
/// as a labelled placeholder instead of throwing, so the renderer never
/// breaks on newer node types. Live event wiring (clicks -> host actions,
/// dataset resolution for tables) is out of scope: tables reference their
/// dataset id and the host is expected to hydrate them.
library;

/// Renders a `UiNode` JSON tree to an HTML fragment with ARIA roles.
final class DomRenderer {
  const DomRenderer();

  /// Render the JSON map produced by `UiNode.toJson()`.
  String renderNode(Map<String, Object?> json) {
    final kind = json['kind'] as String? ?? 'unknown';
    return switch (kind) {
      'column' => _container(json, vertical: true),
      'row' => _container(json, vertical: false),
      'text' => _text(json),
      'button' => _button(json),
      'input' => _input(json),
      'table' => _table(json),
      _ => _unknown(json, kind),
    };
  }

  String _container(Map<String, Object?> json, {required bool vertical}) {
    final id = _attr(json['id']);
    final children = json['children'];
    final buf = StringBuffer();
    for (final child in children is List ? children : const []) {
      if (child is Map<String, Object?>) buf.write(renderNode(child));
    }
    final cls = vertical ? 'sc-column' : 'sc-row';
    return '<div role="group" class="$cls"${_idAttr(id)}'
        '${_styleAttr(json['style'])}>${buf.toString()}</div>';
  }

  String _text(Map<String, Object?> json) {
    final text = _escape(json['text']?.toString() ?? '');
    return '<span class="sc-text"${_idAttr(_attr(json['id']))}'
        '${_styleAttr(json['style'])}>$text</span>';
  }

  String _button(Map<String, Object?> json) {
    final label = _escape(json['label']?.toString() ?? '');
    return '<button type="button" class="sc-button"'
        '${_idAttr(_attr(json['id']))}${_styleAttr(json['style'])}>'
        '$label</button>';
  }

  String _input(Map<String, Object?> json) {
    final placeholder = _escape(json['placeholder']?.toString() ?? '');
    final id = _attr(json['id']);
    // aria-label falls back to the placeholder so screen readers always
    // have a name for the field.
    return '<input type="text" class="sc-input"${_idAttr(id)}'
        ' placeholder="$placeholder" aria-label="$placeholder"'
        '${_styleAttr(json['style'])}>';
  }

  String _table(Map<String, Object?> json) {
    final dataset = _escape(json['dataset']?.toString() ?? '');
    // Datasets live on the host; the DOM carries the dataset id so a
    // hydration pass can fill rows without re-rendering the tree.
    return '<table class="sc-table"${_idAttr(_attr(json['id']))}'
        ' data-dataset="$dataset" aria-label="Data table: $dataset">'
        '<caption>Dataset: $dataset (resolved by host)</caption>'
        '<tbody><tr><td aria-live="polite">Loading…</td></tr></tbody>'
        '</table>';
  }

  String _unknown(Map<String, Object?> json, String kind) {
    final safe = _escape(kind);
    return '<div class="sc-unknown"${_idAttr(_attr(json['id']))} '
        'data-kind="$safe" role="note">'
        'Unsupported node kind: $safe</div>';
  }

  String _idAttr(String? id) =>
      (id == null || id.isEmpty) ? '' : ' data-node-id="${_escape(id)}"';

  /// Maps the subset of UiStyle JSON we can express in CSS. Token colors
  /// (`token:<name>`) map to CSS variables so the page theme controls them.
  String _styleAttr(Object? styleJson) {
    if (styleJson is! Map<String, Object?>) return '';
    final css = StringBuffer();
    void prop(String name, Object? value) {
      if (value == null) return;
      css.write('$name:${_escape(value.toString())};');
    }

    final fg = styleJson['foreground'];
    if (fg is String) css.write('color:${_cssColor(fg)};');
    final bg = styleJson['background'];
    if (bg is String) css.write('background-color:${_cssColor(bg)};');
    final fontSize = styleJson['font_size'] ?? styleJson['fontSize'];
    if (fontSize is num) css.write('font-size:${fontSize}px;');
    final fontWeight = styleJson['font_weight'] ?? styleJson['fontWeight'];
    if (fontWeight is String) {
      css.write('font-weight:${switch (fontWeight) {
        'bold' => '700',
        'semibold' => '600',
        'medium' => '500',
        _ => '400',
      }};');
    }
    final padding = styleJson['padding'];
    if (padding is List && padding.length == 4) {
      css.write('padding:${padding.map((e) => '${e}px').join(' ')};');
    }
    prop('border-radius', styleJson['border_radius'] ?? styleJson['borderRadius']);
    final out = css.toString();
    return out.isEmpty ? '' : ' style="$out"';
  }

  String _cssColor(String v) {
    if (v.startsWith('#')) return _escape(v);
    if (v.startsWith('token:')) {
      final name = _escape(v.substring('token:'.length));
      return 'var(--sc-$name)';
    }
    return 'inherit';
  }

  String _attr(Object? v) => v?.toString() ?? '';

  /// Escapes text for HTML element content and double-quoted attributes.
  String _escape(String s) => s
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;');
}
