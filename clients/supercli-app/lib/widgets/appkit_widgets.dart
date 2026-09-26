/// app-kit UI widgets ported to gpuidart.
///
/// Port of `app-kit/swift/Sources/SupercliAppKitUI/`:
/// CanvasPageView, FooterActionsView, ListNavigation, MarkdownEditorView,
/// MarkdownInsertMenu, MediaView, PageView, ReadOnlyContentView,
/// SemanticMenuView, SurfaceComponentView, TextBoxView, TreeView.
///
/// GAPs (logged in docs/gpuidart-requirements.md):
/// - P0-12: No canvas drawing primitive (CanvasPageView, SurfaceComponentView)
/// - P2: No rich text / markdown rendering (MarkdownEditorView, ReadOnlyContentView)
/// - P0-2: No tree view primitive (TreeView)
library;

import 'package:gpuidart/gpuidart.dart';

/// Canvas page: freeform canvas with positioned components.
/// GAP (P0-12): No canvas primitive. Renders as vertical stack.
final class CanvasPageView {
  const CanvasPageView({this.children = const []});

  final List<UiNode> children;

  UiNode build() {
    return UiColumn('canvas-page', children);
  }
}

/// Footer actions bar: action buttons at the bottom of a page.
final class FooterActionsView {
  const FooterActionsView({this.actions = const []});

  final List<String> actions;

  UiNode build() {
    return UiRow('footer-actions', [
      for (var i = 0; i < actions.length; i++)
        UiButton('footer-action-$i', actions[i]),
    ]);
  }
}

/// List navigation: keyboard-navigable list.
/// GAP (P0-2): Uses UiTable until UiList lands.
final class ListNavigation {
  const ListNavigation({
    this.items = const [],
    this.selectedIndex = 0,
  });

  final List<String> items;
  final int selectedIndex;

  UiNode build() {
    return UiTable('list-navigation', dataset: 'list-navigation');
  }

  TableDataset dataset() => TableDataset(
        'list-navigation',
        columns: const ['Item'],
        rows: items.map((i) => [i]).toList(),
      );

  List<UiAction> actions() => const [
        UiAction(name: 'list.up', keys: 'up',
            context: UiActionContext.node('list-navigation')),
        UiAction(name: 'list.down', keys: 'down',
            context: UiActionContext.node('list-navigation')),
      ];
}

/// Markdown editor: rich text editing.
/// GAP (P2): No rich-text editing in gpuidart. Falls back to UiInput.
final class MarkdownEditorView {
  const MarkdownEditorView({this.initialText = ''});

  final String initialText;

  UiNode build() {
    return UiColumn('markdown-editor', [
      const UiInput('markdown-input', placeholder: 'Write markdown…'),
      const UiText('markdown-gap',
          '(rich markdown editing needs gpuidart rich text, P2)'),
    ]);
  }
}

/// Markdown insert menu: slash-menu for markdown blocks.
final class MarkdownInsertMenu {
  const MarkdownInsertMenu();

  UiNode build() {
    return UiColumn('markdown-insert-menu', [
      for (final item in [
        'Heading',
        'Bold',
        'Code block',
        'Link',
        'Image',
        'Table'
      ])
        UiButton('md-insert-$item', item),
    ]);
  }
}

/// Media view: image/video display.
/// GAP (P0-6): No image/texture widget. Shows placeholder.
final class MediaView {
  const MediaView({this.source = '', this.caption = ''});

  final String source;
  final String caption;

  UiNode build() {
    return UiColumn('media-view', [
      UiText('media-source', '(media: $source — needs UiImage, P0-6)'),
      if (caption.isNotEmpty) UiText('media-caption', caption),
    ]);
  }
}

/// Page view: paged document container.
final class PageView {
  const PageView({
    this.pages = const [],
    this.currentPage = 0,
  });

  final List<UiNode> pages;
  final int currentPage;

  UiNode build() {
    return UiColumn('page-view', [
      if (pages.isNotEmpty) pages[currentPage.clamp(0, pages.length - 1)],
      UiRow('page-nav', [
        const UiButton('page-prev', '←'),
        UiText('page-indicator', '${currentPage + 1} / ${pages.length}'),
        const UiButton('page-next', '→'),
      ]),
    ]);
  }
}

/// Read-only content view: rendered markdown/text.
/// GAP (P2): No markdown rendering. Shows plain text.
final class ReadOnlyContentView {
  const ReadOnlyContentView({this.content = ''});

  final String content;

  UiNode build() {
    return UiColumn('readonly-content', [
      UiText('readonly-text', content),
    ]);
  }
}

/// Semantic menu: context menu with grouped actions.
final class SemanticMenuView {
  const SemanticMenuView({this.groups = const []});

  final List<SemanticMenuGroup> groups;

  UiNode build() {
    return UiColumn('semantic-menu', [
      for (final g in groups) ...[
        UiText('sem-group-${g.title}', g.title),
        for (final item in g.items) UiButton('sem-item-$item', item),
      ],
    ]);
  }
}

final class SemanticMenuGroup {
  const SemanticMenuGroup({required this.title, this.items = const []});

  final String title;
  final List<String> items;
}

/// Surface component view: embeddable component surface.
/// GAP (P0-12): No canvas primitive.
final class SurfaceComponentView {
  const SurfaceComponentView({this.child});

  final UiNode? child;

  UiNode build() {
    final c = child;
    return UiColumn('surface-component', [
      ?c,
    ]);
  }
}

/// Text box view: bordered text display.
final class TextBoxView {
  const TextBoxView({this.text = ''});

  final String text;

  UiNode build() {
    return UiColumn('text-box', [
      UiText('text-box-content', text),
    ]);
  }
}

/// Tree view: collapsible hierarchical list.
/// GAP (P0-2/P0-15): No tree widget. Renders flattened with indentation.
final class TreeView {
  const TreeView({this.nodes = const []});

  final List<TreeNode> nodes;

  UiNode build() {
    return UiColumn('tree-view', [
      for (final n in nodes) _nodeRow(n, 0),
    ]);
  }

  UiNode _nodeRow(TreeNode node, int depth) {
    final indent = '  ' * depth;
    return UiColumn('tree-node-${node.id}', [
      UiRow('tree-row-${node.id}', [
        UiButton('tree-toggle-${node.id}', node.expanded ? '▼' : '▶'),
        UiText('tree-label-${node.id}', '$indent${node.label}'),
      ]),
      if (node.expanded)
        for (final child in node.children) _nodeRow(child, depth + 1),
    ]);
  }
}

final class TreeNode {
  const TreeNode({
    required this.id,
    required this.label,
    this.children = const [],
    this.expanded = false,
  });

  final String id;
  final String label;
  final List<TreeNode> children;
  final bool expanded;
}
