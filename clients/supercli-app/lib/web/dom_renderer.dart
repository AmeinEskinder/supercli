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
/// upstream gpuidart (`clients/gpuidart` @ 135d300), plus `heading`,
/// `list`/`listitem`, `image` and `link` for richer app content. Unknown
/// kinds render as a labelled placeholder instead of throwing, so the
/// renderer never breaks on newer node types.
///
/// Event wiring: buttons may carry an `action` field; the renderer emits
/// it as `data-action` and every interactive node keeps `data-node-id`.
/// `web_events.dart` provides the JS bridge that forwards DOM events back
/// to the host in the same shape as `GpuiEvent` (`{type, id, value}`).
/// Tables reference their dataset id; `web_hydration.dart` fills them from
/// live host datasets without re-rendering the tree.
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
      'heading' => _heading(json),
      'button' => _button(json),
      'input' => _input(json),
      'table' => _table(json),
      'list' => _list(json),
      'listitem' => _listItem(json),
      'image' => _image(json),
      'link' => _link(json),
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
    final id = _attr(json['id']);
    // `action` names the app action this button triggers (see UiAction in
    // app.dart, e.g. `mcp.approve`). The JS event bridge (web_events.dart)
    // forwards it as `{type: 'action', id, value: action}`.
    final action = _attr(json['action']);
    final actionAttr =
        action.isEmpty ? '' : ' data-action="${_escape(action)}"';
    final disabled = json['disabled'] == true;
    final disabledAttr = disabled ? ' disabled aria-disabled="true"' : '';
    // Icon-only buttons (empty label) need an explicit accessible name or
    // screen readers announce them as just "button".
    final ariaLabel = _escape(json['aria_label']?.toString() ?? '');
    final ariaLabelAttr =
        ariaLabel.isEmpty ? '' : ' aria-label="$ariaLabel"';
    return '<button type="button" class="sc-button"'
        '${_idAttr(id)}$actionAttr$disabledAttr$ariaLabelAttr'
        '${_styleAttr(json['style'])}>$label</button>';
  }

  String _input(Map<String, Object?> json) {
    final placeholder = _escape(json['placeholder']?.toString() ?? '');
    final id = _attr(json['id']);
    final value = _escape(json['value']?.toString() ?? '');
    final valueAttr = value.isEmpty ? '' : ' value="$value"';
    final disabled = json['disabled'] == true;
    final disabledAttr = disabled ? ' disabled aria-disabled="true"' : '';
    // Accessible name: explicit `label` wins, then the placeholder. An
    // empty aria-label is worse than none (it masks the accessible name
    // computation), so the attribute is omitted when there is no name.
    final name = _escape(
        json['label']?.toString() ?? json['placeholder']?.toString() ?? '');
    final nameAttr = name.isEmpty ? '' : ' aria-label="$name"';
    // aria-label falls back to the placeholder so screen readers always
    // have a name for the field. The JS bridge forwards input events as
    // `{type: 'input', id, value}` (debounced).
    return '<input type="text" class="sc-input"${_idAttr(id)}'
        ' placeholder="$placeholder"$nameAttr'
        '$valueAttr$disabledAttr${_styleAttr(json['style'])}>';
  }

  /// Heading text: `level` 1-6, defaults to 2. Renders a real `<hN>` so
  /// screen readers get document structure, not just styled spans.
  String _heading(Map<String, Object?> json) {
    final text = _escape(json['text']?.toString() ?? '');
    final level = switch (json['level']) {
      final int l when l >= 1 && l <= 6 => l,
      _ => 2,
    };
    return '<h$level class="sc-heading"${_idAttr(_attr(json['id']))}'
        '${_styleAttr(json['style'])}>$text</h$level>';
  }

  /// Semantic list. Children should be `listitem` nodes; anything else is
  /// still rendered (graceful degradation).
  String _list(Map<String, Object?> json) {
    final children = json['children'];
    final buf = StringBuffer();
    for (final child in children is List ? children : const []) {
      if (child is Map<String, Object?>) buf.write(renderNode(child));
    }
    final ordered = json['ordered'] == true;
    final tag = ordered ? 'ol' : 'ul';
    return '<$tag class="sc-list"${_idAttr(_attr(json['id']))}'
        '${_styleAttr(json['style'])}>${buf.toString()}</$tag>';
  }

  String _listItem(Map<String, Object?> json) {
    final children = json['children'];
    final buf = StringBuffer();
    if (children is List && children.isNotEmpty) {
      for (final child in children) {
        if (child is Map<String, Object?>) buf.write(renderNode(child));
      }
    } else {
      // Shorthand: listitem with inline text.
      buf.write(_escape(json['text']?.toString() ?? ''));
    }
    // aria-selected is only valid on option/row/tab roles. A plain <li>
    // with aria-selected is invalid ARIA, so selection is only exposed
    // when the item opts into role="option" (listbox pattern).
    final role = json['role']?.toString();
    final roleAttr =
        (role == 'option') ? ' role="option"' : '';
    final selected = json['selected'] == true && role == 'option';
    final selectedAttr = selected ? ' aria-selected="true"' : '';
    return '<li class="sc-listitem"${_idAttr(_attr(json['id']))}'
        '$roleAttr$selectedAttr${_styleAttr(json['style'])}>'
        '${buf.toString()}</li>';
  }

  /// Image with mandatory alt text (empty alt = decorative).
  String _image(Map<String, Object?> json) {
    final src = _escape(json['src']?.toString() ?? '');
    final alt = _escape(json['alt']?.toString() ?? '');
    final width = json['width'];
    final height = json['height'];
    final dims = StringBuffer();
    if (width is num) dims.write(' width="${width.toInt()}"');
    if (height is num) dims.write(' height="${height.toInt()}"');
    return '<img class="sc-image"${_idAttr(_attr(json['id']))}'
        ' src="$src" alt="$alt"$dims${_styleAttr(json['style'])}>';
  }

  /// Hyperlink. `url` is the href; external links get
  /// rel="noopener noreferrer".
  String _link(Map<String, Object?> json) {
    final url = _escape(json['url']?.toString() ?? '#');
    final label = _escape(json['label']?.toString() ?? url);
    final external = json['external'] == true;
    final rel = external ? ' rel="noopener noreferrer"' : '';
    return '<a class="sc-link"${_idAttr(_attr(json['id']))}'
        ' href="$url"$rel${_styleAttr(json['style'])}>$label</a>';
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
  /// Keys follow the gpuidart wire format (snake_case); camelCase aliases
  /// are accepted for forward compatibility.
  String _styleAttr(Object? styleJson) {
    if (styleJson is! Map<String, Object?>) return '';
    final css = StringBuffer();
    void prop(String name, Object? value) {
      if (value == null) return;
      css.write('$name:${_escape(value.toString())};');
    }

    /// Numeric CSS lengths need `px`; strings pass through (they may carry
    /// their own units or keywords).
    void lengthProp(String name, Object? value) {
      if (value == null) return;
      if (value is num) {
        css.write('$name:${value}px;');
      } else {
        prop(name, value);
      }
    }

    final fg = styleJson['foreground'];
    if (fg is String) css.write('color:${_cssColor(fg)};');
    final bg = styleJson['background'];
    if (bg is String) css.write('background-color:${_cssColor(bg)};');
    final borderColor =
        styleJson['border_color'] ?? styleJson['borderColor'];
    if (borderColor is String) {
      css.write('border-color:${_cssColor(borderColor)};');
    }
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
    final gap = styleJson['gap'];
    if (gap is num) css.write('gap:${gap}px;');
    lengthProp('width', _cssSize(styleJson['width']));
    lengthProp('height', _cssSize(styleJson['height']));
    final align = styleJson['align'];
    if (align is String) prop('align-items', align);
    final justify = styleJson['justify'];
    if (justify is String) {
      prop('justify-content',
          justify == 'space_between' ? 'space-between' : justify);
    }
    lengthProp(
        'border-radius', styleJson['border_radius'] ?? styleJson['borderRadius']);
    final out = css.toString();
    return out.isEmpty ? '' : ' style="$out"';
  }

  /// Converts a UiSize wire value to CSS: `{'px': n}` → n (px added by
  /// caller), `'full'` → `100%`, `'fit'` → `auto`.
  Object? _cssSize(Object? v) {
    if (v is Map<String, Object?>) {
      final px = v['px'];
      if (px is num) return px;
      return null;
    }
    return switch (v) {
      'full' => '100%',
      'fit' => 'auto',
      _ => null,
    };
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
