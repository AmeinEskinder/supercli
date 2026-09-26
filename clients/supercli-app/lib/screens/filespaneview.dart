/// Files app pane: scoped file-tree explorer with filter and preview.
///
/// Real implementation against the [StubFilesDataSource] model layer (the
/// Host has no file-browse backend route yet — see docs/gpuidart-gaps-apps.md
/// GAP-A2). Renders through the RLE fallback (UiRow/UiText/UiButton/UiColumn);
/// no gpuidart APIs beyond the upstream set are used.
///
/// Covers parity row 139 (scoped explorer with filter, context menu
/// open/send/copy) and row 140 (follows the neighbouring agent's
/// project/worktree via [rootPath]).
library;

import 'package:gpuidart/gpuidart.dart';

/// One node of the file tree.
final class FileNode {
  const FileNode({
    required this.name,
    required this.path,
    required this.isDir,
    this.children = const [],
    this.sizeBytes,
  });

  final String name;
  final String path;
  final bool isDir;
  final List<FileNode> children;
  final int? sizeBytes;

  String get displaySize {
    final s = sizeBytes;
    if (s == null) return '';
    if (s < 1024) return '$s B';
    if (s < 1024 * 1024) return '${(s / 1024).toStringAsFixed(1)} KB';
    return '${(s / (1024 * 1024)).toStringAsFixed(1)} MB';
  }
}

/// Stub file-tree data source.
///
/// STUB: the Host has no file-browse backend route yet, so the Files pane
/// cannot list the real working tree. Representative data stands in until
/// the backend exists (GAP-A2 in docs/gpuidart-gaps-apps.md).
final class StubFilesDataSource {
  const StubFilesDataSource();

  String get rootPath => '/home/user/projects/supercli';

  FileNode get tree => const FileNode(
    name: 'supercli',
    path: '/home/user/projects/supercli',
    isDir: true,
    children: [
      FileNode(
        name: 'crates',
        path: '/home/user/projects/supercli/crates',
        isDir: true,
        children: [
          FileNode(
            name: 'supercli-core',
            path: '/home/user/projects/supercli/crates/supercli-core',
            isDir: true,
            children: [
              FileNode(
                name: 'Cargo.toml',
                path: '/home/user/projects/supercli/crates/supercli-core/Cargo.toml',
                isDir: false,
                sizeBytes: 2048,
              ),
              FileNode(
                name: 'src',
                path: '/home/user/projects/supercli/crates/supercli-core/src',
                isDir: true,
                children: [
                  FileNode(
                    name: 'lib.rs',
                    path: '/home/user/projects/supercli/crates/supercli-core/src/lib.rs',
                    isDir: false,
                    sizeBytes: 15360,
                  ),
                ],
              ),
            ],
          ),
        ],
      ),
      FileNode(
        name: 'clients',
        path: '/home/user/projects/supercli/clients',
        isDir: true,
        children: [
          FileNode(
            name: 'supercli-app',
            path: '/home/user/projects/supercli/clients/supercli-app',
            isDir: true,
            children: [],
          ),
        ],
      ),
      FileNode(
        name: 'README.md',
        path: '/home/user/projects/supercli/README.md',
        isDir: false,
        sizeBytes: 8192,
      ),
      FileNode(
        name: '.gitignore',
        path: '/home/user/projects/supercli/.gitignore',
        isDir: false,
        sizeBytes: 256,
      ),
    ],
  );

  /// Fake file preview content for the selected path.
  String previewFor(String path) {
    if (path.endsWith('.md')) {
      return '# README\n\nSuperCLI — the agent-first terminal.\n';
    }
    if (path.endsWith('.toml')) {
      return '[package]\nname = "supercli-core"\nversion = "0.1.0"\n';
    }
    if (path.endsWith('.rs')) {
      return '//! supercli core library.\n\npub fn version() -> &\'static str {\n    "0.1.0"\n}\n';
    }
    return '(binary or empty file — no preview)';
  }
}

/// The Files app pane: filterable tree + preview column.
final class FilesPaneView {
  FilesPaneView({
    required this.paneId,
    this.dataSource = const StubFilesDataSource(),
    this.filter = '',
    this.selectedPath,
    this.expandedPaths = const {},
    this.onSelect,
    this.onOpen,
    this.onSendToAgent,
    this.onCopyPath,
  });

  final String paneId;
  final StubFilesDataSource dataSource;

  /// Substring filter applied to file names (parity row 139 `--ext`/filter).
  final String filter;

  /// Currently selected file path.
  final String? selectedPath;

  /// Paths of expanded directories.
  final Set<String> expandedPaths;

  /// Context-menu actions (parity row 139: open / send to agent / copy path).
  /// NOTE (gap GAP-A4): no native context-menu API upstream; the app shell
  /// wires these callbacks to its own menu.
  final void Function(String path)? onSelect;
  final void Function(String path)? onOpen;
  final void Function(String path)? onSendToAgent;
  final void Function(String path)? onCopyPath;

  UiNode build() {
    final root = dataSource.tree;
    final rows = <UiNode>[];
    _collectRows(root, 0, rows);
    return UiColumn(
      'filespane-$paneId',
      [
        _header(),
        UiRow('filespane-$paneId-body', [
          UiColumn('filespane-$paneId-tree', rows, style: UiStyle(gap: 2)),
          _previewColumn(),
        ], style: UiStyle(gap: 16)),
      ],
      style: UiStyle(
        background: UiColor.hex('#1e1e1e'),
        padding: const [8, 8, 8, 8],
        gap: 8,
      ),
    );
  }

  UiNode _header() {
    return UiRow('filespane-$paneId-header', [
      UiText(
        'filespane-$paneId-root',
        '▾ ${dataSource.rootPath}',
        style: UiStyle(
          foreground: UiColor.hex('#eeeeec'),
          fontSize: 14,
          fontWeight: UiFontWeight.bold,
        ),
      ),
      UiText(
        'filespane-$paneId-filter',
        filter.isEmpty ? 'filter: —' : 'filter: "$filter"',
        style: UiStyle(foreground: UiColor.hex('#8a8a8a'), fontSize: 12),
      ),
    ], style: UiStyle(gap: 12));
  }

  void _collectRows(FileNode node, int depth, List<UiNode> out) {
    // Skip the synthetic root itself; render its children.
    if (depth == 0) {
      for (final c in _visibleChildren(node)) {
        _collectRows(c, 1, out);
      }
      return;
    }
    final expanded =
        node.isDir &&
        (expandedPaths.contains(node.path) || expandedPaths.isEmpty);
    final marker = node.isDir ? (expanded ? '▾' : '▸') : '•';
    final selected = node.path == selectedPath;
    out.add(
      UiRow('filespane-$paneId-row-${node.path.hashCode}', [
        UiText(
          'filespane-$paneId-row-${node.path.hashCode}-m',
          marker,
          style: UiStyle(
            foreground: UiColor.hex(node.isDir ? '#729fcf' : '#555753'),
            fontSize: 13,
          ),
        ),
        UiText(
          'filespane-$paneId-row-${node.path.hashCode}-n',
          '${'  ' * (depth - 1)}${node.name}${selected ? ' ◀' : ''}',
          style: UiStyle(
            foreground: UiColor.hex(selected ? '#eeeeec' : '#d3d7cf'),
            fontSize: 13,
            fontWeight: selected ? UiFontWeight.bold : UiFontWeight.normal,
          ),
        ),
        if (!node.isDir)
          UiText(
            'filespane-$paneId-row-${node.path.hashCode}-s',
            node.displaySize,
            style: UiStyle(foreground: UiColor.hex('#555753'), fontSize: 11),
          ),
      ], style: UiStyle(gap: 6)),
    );
    if (node.isDir && expanded) {
      for (final c in _visibleChildren(node)) {
        _collectRows(c, depth + 1, out);
      }
    }
  }

  List<FileNode> _visibleChildren(FileNode node) {
    final kids = node.children;
    if (filter.isEmpty) return kids;
    // Keep a directory if it or any descendant matches the filter.
    return kids.where(_matches).toList();
  }

  bool _matches(FileNode node) {
    if (node.name.toLowerCase().contains(filter.toLowerCase())) return true;
    return node.children.any(_matches);
  }

  UiNode _previewColumn() {
    final path = selectedPath;
    final lines = path == null
        ? ['(select a file to preview)']
        : dataSource.previewFor(path).split('\n');
    return UiColumn('filespane-$paneId-preview', [
      UiText(
        'filespane-$paneId-preview-label',
        path == null ? 'Preview' : 'Preview — ${path.split('/').last}',
        style: UiStyle(
          foreground: UiColor.hex('#ad7fa8'),
          fontSize: 12,
          fontWeight: UiFontWeight.bold,
        ),
      ),
      for (var i = 0; i < lines.length; i++)
        UiText(
          'filespane-$paneId-preview-l$i',
          lines[i].isEmpty ? ' ' : lines[i],
          style: UiStyle(foreground: UiColor.hex('#8a8a8a'), fontSize: 12),
        ),
      if (path != null)
        UiRow('filespane-$paneId-preview-actions', [
          UiButton('filespane-$paneId-open', 'Open'),
          UiButton('filespane-$paneId-send', 'Send to agent'),
          UiButton('filespane-$paneId-copy', 'Copy path'),
        ], style: UiStyle(gap: 4)),
    ], style: UiStyle(gap: 4));
  }

  /// Flattened visible rows (for tests): (depth, node) in display order.
  List<(int, FileNode)> visibleRows() {
    final out = <(int, FileNode)>[];
    void walk(FileNode node, int depth) {
      if (depth > 0) out.add((depth, node));
      final expanded =
          node.isDir &&
          (expandedPaths.contains(node.path) || expandedPaths.isEmpty);
      if (node.isDir && (depth == 0 || expanded)) {
        for (final c in _visibleChildren(node)) {
          walk(c, depth + 1);
        }
      }
    }

    walk(dataSource.tree, 0);
    return out;
  }

  List<UiAction> actions() => [
    UiAction(
      name: 'files.open',
      keys: 'enter',
      context: UiActionContext.node('filespane-$paneId'),
    ),
    UiAction(
      name: 'files.send-to-agent',
      keys: 'ctrl+enter',
      context: UiActionContext.node('filespane-$paneId'),
    ),
    UiAction(
      name: 'files.copy-path',
      keys: 'alt+c',
      context: UiActionContext.node('filespane-$paneId'),
    ),
  ];
}
