/// App-kit renderer: [AppKitNode] → gpuidart [UiNode].
///
/// Port of the Swift `SupercliAppKitUI` view layer
/// (`clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/`):
/// `PageView.swift` (1231 lines), `TreeView.swift` (370), `TextBoxView.swift`
/// (175), `MediaView.swift` (196), `SemanticMenuView.swift` (129),
/// `FooterActionsView.swift` (118), `CanvasPageView.swift` (106),
/// `SurfaceComponentView.swift` (84), `ReadOnlyContentView.swift` (131),
/// `MarkdownEditorView.swift` (989), `MarkdownInsertMenu.swift` (57),
/// `ListNavigation.swift` (47).
///
/// Each `render*` method mirrors the corresponding Swift `View.body`,
/// delegating to the widgets in `appkit_widgets.dart` where behavior already
/// exists. Unknown components render a terminal-fallback placeholder instead
/// of throwing (Swift `unsupported` behavior).
library;

import 'package:gpuidart/gpuidart.dart';

import 'appkit_protocol.dart';
import 'appkit_widgets.dart' as w;

/// Renders decoded app-kit nodes as native gpuidart widget trees.
abstract final class AppKitRenderer {
  /// Render a full node (dispatches on component kind).
  static UiNode render(AppKitNode node) {
    final c = node.component;
    return switch (c) {
      AppKitPage(:final spec) => renderPage(node.id, spec),
      AppKitList(:final spec) => renderList(node.id, spec),
      AppKitContent(:final spec) => renderContent(node.id, spec),
      AppKitTree(:final spec) => renderTree(node.id, spec),
      AppKitTextBox(:final spec) => renderTextBox(node.id, spec),
      AppKitMedia(:final spec) => renderMedia(node.id, spec),
      AppKitMenu(:final spec) => renderMenu(node.id, spec),
      AppKitMarkdownEditor(:final spec) => renderMarkdownEditor(node.id, spec),
      AppKitCanvasPage(:final spec) => renderCanvasPage(node.id, spec),
      AppKitSurface(:final spec) => renderSurface(node.id, spec),
      AppKitUnsupported(:final unsupportedKind) =>
        renderUnsupported(node.id, unsupportedKind),
    };
  }

  /// Mirrors Swift `PageView.body`.
  static UiNode renderPage(String id, PageSpec spec) {
    return UiColumn('appkit-page-$id', [
      UiText('appkit-page-title-$id', spec.title),
      if (spec.tabs.isNotEmpty)
        UiRow('appkit-page-tabs-$id', [
          for (final t in spec.tabs)
            UiButton('appkit-tab-${t.id}', t.label),
        ]),
      _renderPageBody(id, spec.body),
      if (!spec.footer.isEmpty)
        w.FooterActionsView(
          actions: [for (final a in spec.footer.actions) a.label],
        ).build(),
      if (spec.footer.status != null)
        UiText('appkit-page-status-$id', spec.footer.status!),
    ]);
  }

  static UiNode _renderPageBody(String id, PageBody body) {
    return switch (body) {
      PageBodyList(:final list) => renderList('$id-body', list),
      PageBodyContent(:final content) => renderContent('$id-body', content),
      PageBodyGauge(:final gauge) => renderGauge('$id-body', gauge),
      PageBodyUnsupported(:final bodyKind) =>
        renderUnsupported('$id-body', 'pageBody:$bodyKind'),
    };
  }

  /// Mirrors Swift `ListNavigation` + list item rendering.
  static UiNode renderList(String id, ListSpec spec) {
    if (spec.items.isEmpty) {
      return UiColumn('appkit-list-$id', [
        UiText('appkit-list-empty-$id',
            spec.emptyMessage.isEmpty ? '(empty)' : spec.emptyMessage),
      ]);
    }
    return UiColumn('appkit-list-$id', [
      for (final item in spec.items) _renderListItem(id, item),
    ]);
  }

  static UiNode _renderListItem(String listId, ListItemSpec item) {
    if (item.divider) {
      return UiText('appkit-divider-${item.id}',
          item.label.isEmpty ? '─' * 24 : '─ ${item.label} ─');
    }
    final row = <UiNode>[
      UiText('appkit-item-label-${item.id}',
          '${item.done ? '✓ ' : ''}${item.label}${item.busy ? ' …' : ''}'),
      if (item.detail != null)
        UiText('appkit-item-detail-${item.id}', item.detail!),
      if (item.value != null)
        UiText('appkit-item-value-${item.id}', item.value!),
      if (item.leading != null) _renderSlot('${item.id}-leading', item.leading!),
      if (item.trailing != null)
        _renderSlot('${item.id}-trailing', item.trailing!),
      if (item.accessory != null)
        _renderSlot('${item.id}-accessory', item.accessory!),
    ];
    final bands = <UiNode>[
      if (item.top != null) _renderBand('${item.id}-top', item.top!),
      UiRow('appkit-item-row-${item.id}', row),
      if (item.bottom != null) _renderBand('${item.id}-bottom', item.bottom!),
    ];
    if (item.media != null) {
      bands.add(UiText('appkit-item-media-${item.id}',
          '[${item.media!.glyph}] (${item.media!.side})'));
    }
    return UiColumn('appkit-item-${item.id}', bands);
  }

  static UiNode _renderSlot(String id, ListItemSlot slot) {
    return switch (slot) {
      SlotGauge(:final gauge) => renderGauge(id, gauge),
      SlotBadge(:final badge) =>
        UiText('appkit-badge-$id', '[${badge.text}]'),
      SlotStatus(:final status) =>
        UiText('appkit-status-$id', status.symbol),
      SlotToggle(:final on) =>
        UiText('appkit-toggle-$id', on ? '[x]' : '[ ]'),
      SlotSparkline(:final values) =>
        UiText('appkit-sparkline-$id', _sparkline(values)),
      SlotDisclosure() => UiText('appkit-disclosure-$id', '›'),
      SlotCheckmark(:final checked) =>
        UiText('appkit-checkmark-$id', checked ? '✓' : '○'),
      SlotUnsupported(:final slotKind) =>
        renderUnsupported(id, 'slot:$slotKind'),
    };
  }

  static UiNode _renderBand(String id, ListItemBand band) {
    if (band.gauge != null) return renderGauge(id, band.gauge!);
    return UiText('appkit-band-$id', band.text ?? '');
  }

  /// Gauge rendering mirrors Swift `UIGaugeSpec.percentageLabel`
  /// (`"\(label)  \(valueLabel)"`) with a text bar (gpuidart has no chart
  /// primitive yet — GAP-APPKIT-4).
  static UiNode renderGauge(String id, GaugeSpec gauge) {
    const width = 12;
    final filled = (gauge.ratio * width).round().clamp(0, width);
    final bar = '█' * filled + '░' * (width - filled);
    return UiRow('appkit-gauge-$id', [
      UiText('appkit-gauge-bar-$id', bar),
      UiText('appkit-gauge-label-$id', gauge.percentageLabel),
    ]);
  }

  static String _sparkline(List<num> values) {
    const glyphs = '▁▂▃▄▅▆▇█';
    if (values.isEmpty) return '';
    final max = values.fold<double>(0, (m, v) => v.toDouble() > m ? v.toDouble() : m);
    if (max <= 0) return glyphs[0] * values.length;
    return values
        .map((v) =>
            glyphs[((v.toDouble() / max) * (glyphs.length - 1)).round()])
        .join();
  }

  /// Mirrors Swift `ReadOnlyContentView`.
  static UiNode renderContent(String id, ContentSpec spec) {
    return w.ReadOnlyContentView(
      content: spec.lines.map((l) => l.text).join('\n'),
    ).build();
  }

  /// Mirrors Swift `TreeView`.
  static UiNode renderTree(String id, TreeSpec spec) {
    return w.TreeView(
      nodes: [for (final i in spec.items) _toTreeNode(i)],
    ).build();
  }

  static w.TreeNode _toTreeNode(TreeItemSpec item) => w.TreeNode(
        id: item.id,
        label: item.label,
        children: [for (final c in item.children) _toTreeNode(c)],
        expanded: item.expanded,
      );

  /// Mirrors Swift `TextBoxView`.
  static UiNode renderTextBox(String id, TextBoxSpec spec) {
    return w.TextBoxView(
      text: spec.text.isEmpty ? spec.placeholder : spec.text,
    ).build();
  }

  /// Mirrors Swift `MediaView` (placeholder until UiImage lands, P0-6).
  static UiNode renderMedia(String id, MediaSpec spec) {
    return w.MediaView(source: spec.source, caption: spec.caption ?? '')
        .build();
  }

  /// Mirrors Swift `SemanticMenuView`.
  static UiNode renderMenu(String id, MenuSpec spec) {
    return w.SemanticMenuView(
      groups: [
        w.SemanticMenuGroup(
          title: id,
          items: [for (final i in spec.items) i.label],
        ),
      ],
    ).build();
  }

  /// Mirrors Swift `MarkdownEditorView` (fallback until rich text, P2).
  static UiNode renderMarkdownEditor(String id, MarkdownEditorSpec spec) {
    return w.MarkdownEditorView(
      initialText: spec.text,
    ).build();
  }

  /// Mirrors Swift `CanvasPageView` (stack fallback until canvas, P0-12).
  static UiNode renderCanvasPage(String id, CanvasPageSpec spec) {
    return w.CanvasPageView(
      children: [for (final c in spec.children) render(c)],
    ).build();
  }

  /// Mirrors Swift `SurfaceComponentView`.
  static UiNode renderSurface(String id, SurfaceSpec spec) {
    final child = spec.child;
    return w.SurfaceComponentView(
      child: child == null ? null : render(child),
    ).build();
  }

  /// Terminal fallback for unknown components (Swift `unsupported` behavior).
  static UiNode renderUnsupported(String id, String kind) {
    return UiColumn('appkit-unsupported-$id', [
      UiText('appkit-unsupported-kind-$id', '(unsupported component: $kind)'),
      UiText('appkit-unsupported-hint-$id',
          'Render in terminal fallback.'),
    ]);
  }
}
