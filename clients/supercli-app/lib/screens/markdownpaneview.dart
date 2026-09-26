/// Markdown app pane: read-only rendered preview.
///
/// Distinct from `lib/widgets/markdowneditorview.dart` (the live editable
/// surface): this pane renders a note read-only with heading/list/code/
/// quote styling through the RLE fallback (UiRow/UiText/UiColumn).
///
/// STUB DATA SOURCE: the Host has no notes backend route yet — see
/// docs/gpuidart-gaps-apps.md GAP-A2. [StubMarkdownDataSource] stands in.
///
/// Covers parity rows 141 (rendered markdown — the editor itself lives in
/// markdowneditorview.dart) and 142 (Open/Save/New note, vault explorer,
/// remembered notes folder).
library;

import 'package:gpuidart/gpuidart.dart';

/// One vault note.
final class MarkdownNote {
  const MarkdownNote({
    required this.title,
    required this.path,
    required this.body,
    required this.updated,
  });

  final String title;
  final String path;
  final String body;
  final String updated;
}

/// Stub notes data source.
///
/// STUB: no Host notes backend exists yet; representative notes stand in
/// until it does (GAP-A2 in docs/gpuidart-gaps-apps.md).
final class StubMarkdownDataSource {
  const StubMarkdownDataSource();

  String get vaultPath => '/home/user/notes';

  List<MarkdownNote> get notes => const [
    MarkdownNote(
      title: 'supercli roadmap',
      path: '/home/user/notes/roadmap.md',
      updated: '2026-09-26',
      body:
          '# supercli roadmap\n\n'
          'Parity with **unpeel**, then *beyond*.\n\n'
          '## Milestones\n\n'
          '- [x] M1: rename + guard\n'
          '- [ ] M2: gpuidart desktop\n'
          '- [ ] M3: mobile parity\n\n'
          '> Ship every feature unpeel had, with improvements.\n\n'
          '```sh\ncargo test --workspace --locked\n```\n\n'
          'See the [parity checklist](docs/parity/checklist.md).',
    ),
    MarkdownNote(
      title: 'meeting notes',
      path: '/home/user/notes/meeting.md',
      updated: '2026-09-25',
      body:
          '# Meeting notes\n\n'
          '1. Approve the P0-8 proposal\n'
          '2. Review `UiTerminal` gaps\n'
          '3. Plan the v2 branch\n',
    ),
  ];
}

/// Inline-styled text run produced by the mini markdown parser.
final class MdRun {
  const MdRun(
    this.text, {
    this.bold = false,
    this.italic = false,
    this.code = false,
  });
  final String text;
  final bool bold;
  final bool italic;
  final bool code;
}

/// Minimal block parser: headings, fenced code, blockquotes, lists
/// (bulleted, numbered, task), horizontal rules, paragraphs. Inline:
/// `**bold**`, `*italic*`, `` `code` ``, `[text](url)`.
final class MarkdownPreview {
  const MarkdownPreview();

  /// Parse [source] into renderable blocks. Each block is a list of
  /// (indent, runs) rows; code blocks carry raw lines instead.
  List<MdBlock> parse(String source) {
    final blocks = <MdBlock>[];
    final lines = source.split('\n');
    var i = 0;
    while (i < lines.length) {
      final line = lines[i];
      final trimmed = line.trimLeft();
      if (trimmed.startsWith('```')) {
        final code = <String>[];
        i++;
        while (i < lines.length && !lines[i].trimLeft().startsWith('```')) {
          code.add(lines[i]);
          i++;
        }
        i++; // consume closing fence
        blocks.add(MdCodeBlock(code));
        continue;
      }
      if (trimmed.startsWith('#')) {
        final level = RegExp(r'^#+').firstMatch(trimmed)![0]!.length;
        blocks.add(
          MdHeading(
            level.clamp(1, 6),
            parseInline(trimmed.substring(level).trim()),
          ),
        );
        i++;
        continue;
      }
      if (trimmed.startsWith('>')) {
        blocks.add(MdQuote(parseInline(trimmed.substring(1).trim())));
        i++;
        continue;
      }
      if (RegExp(r'^---+$').hasMatch(trimmed)) {
        blocks.add(const MdRule());
        i++;
        continue;
      }
      final bullet = RegExp(r'^([-*]|\d+[.)])\s+(.*)$').firstMatch(trimmed);
      if (bullet != null) {
        final indent = (line.length - trimmed.length) ~/ 2;
        final task = RegExp(r'^\[([ x])\]\s+(.*)$')
            .firstMatch(bullet.group(2)!);
        if (task != null) {
          blocks.add(
            MdTaskItem(
              task.group(1) == 'x',
              parseInline(task.group(2)!),
              indent,
            ),
          );
        } else {
          final ordered = RegExp(r'^\d+[.)]$').hasMatch(bullet.group(1)!);
          blocks.add(
            MdListItem(parseInline(bullet.group(2)!), indent, ordered: ordered),
          );
        }
        i++;
        continue;
      }
      if (trimmed.isEmpty) {
        i++;
        continue;
      }
      blocks.add(MdParagraph(parseInline(trimmed)));
      i++;
    }
    return blocks;
  }

  /// Parse inline markup into styled runs.
  List<MdRun> parseInline(String text) {
    final runs = <MdRun>[];
    final pattern = RegExp(
      r'(\*\*.+?\*\*|\*[^*]+?\*|`[^`]+?`|\[[^\]]+?\]\([^)]+?\))',
    );
    var pos = 0;
    for (final m in pattern.allMatches(text)) {
      if (m.start > pos) {
        runs.add(MdRun(text.substring(pos, m.start)));
      }
      final token = m.group(0)!;
      if (token.startsWith('**')) {
        runs.add(MdRun(token.substring(2, token.length - 2), bold: true));
      } else if (token.startsWith('`')) {
        runs.add(MdRun(token.substring(1, token.length - 1), code: true));
      } else if (token.startsWith('[')) {
        final label = token.substring(1, token.indexOf(']'));
        runs.add(MdRun(label, bold: true));
      } else {
        runs.add(MdRun(token.substring(1, token.length - 1), italic: true));
      }
      pos = m.end;
    }
    if (pos < text.length) {
      runs.add(MdRun(text.substring(pos)));
    }
    return runs;
  }
}

/// Rendered block types.
sealed class MdBlock {
  const MdBlock();
}

final class MdHeading extends MdBlock {
  const MdHeading(this.level, this.runs);
  final int level;
  final List<MdRun> runs;
}

final class MdParagraph extends MdBlock {
  const MdParagraph(this.runs);
  final List<MdRun> runs;
}

final class MdQuote extends MdBlock {
  const MdQuote(this.runs);
  final List<MdRun> runs;
}

final class MdListItem extends MdBlock {
  const MdListItem(this.runs, this.indent, {this.ordered = false});
  final List<MdRun> runs;
  final int indent;
  final bool ordered;
}

final class MdTaskItem extends MdBlock {
  const MdTaskItem(this.done, this.runs, this.indent);
  final bool done;
  final List<MdRun> runs;
  final int indent;
}

final class MdCodeBlock extends MdBlock {
  const MdCodeBlock(this.lines);
  final List<String> lines;
}

final class MdRule extends MdBlock {
  const MdRule();
}

/// The Markdown app pane: vault explorer + read-only preview.
final class MarkdownPaneView {
  MarkdownPaneView({
    required this.paneId,
    this.dataSource = const StubMarkdownDataSource(),
    this.selectedPath,
    this.onSelectNote,
    this.onNewNote,
    this.onOpenNote,
    this.onSaveNote,
  });

  final String paneId;
  final StubMarkdownDataSource dataSource;
  final String? selectedPath;

  /// Vault actions (parity row 142: Open/Save/New note).
  /// NOTE (gap GAP-A4): no native file dialogs upstream; the app shell
  /// wires these callbacks to its own dialogs.
  final void Function(String path)? onSelectNote;
  final void Function()? onNewNote;
  final void Function(String path)? onOpenNote;
  final void Function(String path)? onSaveNote;

  static const _parser = MarkdownPreview();

  MarkdownNote? get selectedNote {
    final notes = dataSource.notes;
    if (selectedPath != null) {
      for (final n in notes) {
        if (n.path == selectedPath) return n;
      }
    }
    return notes.isEmpty ? null : notes.first;
  }

  UiNode build() {
    final note = selectedNote;
    return UiColumn(
      'mdpane-$paneId',
      [
        _header(),
        UiRow('mdpane-$paneId-body', [
          _vaultColumn(),
          _previewColumn(note),
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
    return UiRow('mdpane-$paneId-header', [
      UiText(
        'mdpane-$paneId-title',
        'Notes — ${dataSource.vaultPath}',
        style: UiStyle(
          foreground: UiColor.hex('#eeeeec'),
          fontSize: 14,
          fontWeight: UiFontWeight.bold,
        ),
      ),
      UiButton('mdpane-$paneId-new', '+ New note'),
    ], style: UiStyle(gap: 12));
  }

  UiNode _vaultColumn() {
    final notes = dataSource.notes;
    final sel = selectedNote?.path;
    return UiColumn('mdpane-$paneId-vault', [
      UiText(
        'mdpane-$paneId-vault-label',
        'Vault (${notes.length})',
        style: UiStyle(
          foreground: UiColor.hex('#ad7fa8'),
          fontSize: 12,
          fontWeight: UiFontWeight.bold,
        ),
      ),
      for (final n in notes)
        UiRow('mdpane-$paneId-note-${n.path.hashCode}', [
          UiText(
            'mdpane-$paneId-note-${n.path.hashCode}-t',
            '▤ ${n.title}${n.path == sel ? ' ◀' : ''}',
            style: UiStyle(
              foreground: UiColor.hex(n.path == sel ? '#eeeeec' : '#d3d7cf'),
              fontSize: 13,
              fontWeight: n.path == sel
                  ? UiFontWeight.bold
                  : UiFontWeight.normal,
            ),
          ),
          UiText(
            'mdpane-$paneId-note-${n.path.hashCode}-u',
            n.updated,
            style: UiStyle(foreground: UiColor.hex('#555753'), fontSize: 11),
          ),
        ], style: UiStyle(gap: 8)),
    ], style: UiStyle(gap: 4));
  }

  UiNode _previewColumn(MarkdownNote? note) {
    if (note == null) {
      return UiText(
        'mdpane-$paneId-empty',
        '(no notes in vault)',
        style: UiStyle(foreground: UiColor.hex('#555753'), fontSize: 13),
      );
    }
    final blocks = _parser.parse(note.body);
    return UiColumn('mdpane-$paneId-preview', [
      UiText(
        'mdpane-$paneId-preview-title',
        note.title,
        style: UiStyle(
          foreground: UiColor.hex('#eeeeec'),
          fontSize: 16,
          fontWeight: UiFontWeight.bold,
        ),
      ),
      for (var i = 0; i < blocks.length; i++)
        _blockNode('mdpane-$paneId-b$i', blocks[i], i),
      UiRow('mdpane-$paneId-actions', [
        UiButton('mdpane-$paneId-open', 'Open'),
        UiButton('mdpane-$paneId-save', 'Save'),
      ], style: UiStyle(gap: 4)),
    ], style: UiStyle(gap: 6));
  }

  UiNode _blockNode(String id, MdBlock block, int index) {
    return switch (block) {
      MdHeading(level: final l, runs: final r) => UiRow('$id-h', [
        UiText('$id-h-t', '${'#' * l} ', style: _dimStyle(12)),
        ..._runNodes('$id-h', r, 20.0 - l, bold: true),
      ]),
      MdParagraph(runs: final r) => UiRow('$id-p', _runNodes('$id-p', r, 13)),
      MdQuote(runs: final r) => UiRow('$id-q', [
        UiText('$id-q-bar', '▏ ', style: _fgStyle('#729fcf', 13, bold: true)),
        ..._runNodes('$id-q', r, 13, dim: true),
      ]),
      MdListItem(runs: final r, indent: final d, ordered: final o) => UiRow(
        '$id-li',
        [
          UiText(
            '$id-li-b',
            '${'  ' * d}${o ? '${index + 1}.' : '•'} ',
            style: _fgStyle('#8ae234', 13, bold: true),
          ),
          ..._runNodes('$id-li', r, 13),
        ],
      ),
      MdTaskItem(done: final done, runs: final r, indent: final d) => UiRow(
        '$id-t',
        [
          UiText(
            '$id-t-c',
            '${'  ' * d}${done ? '☑' : '☐'} ',
            style: _fgStyle(done ? '#8ae234' : '#8a8a8a', 13, bold: true),
          ),
          ..._runNodes('$id-t', r, 13, dim: done),
        ],
      ),
      MdCodeBlock(lines: final lines) => UiColumn('$id-cb', [
        for (var j = 0; j < lines.length; j++)
          UiText(
            '$id-cb-l$j',
            lines[j].isEmpty ? ' ' : lines[j],
            style: UiStyle(
              foreground: UiColor.hex('#fce94f'),
              background: UiColor.hex('#2d2d2d'),
              fontSize: 12,
            ),
          ),
      ], style: UiStyle(gap: 1)),
      MdRule() => UiText('$id-hr', '─' * 40, style: _dimStyle(12)),
    };
  }

  List<UiNode> _runNodes(
    String id,
    List<MdRun> runs,
    double size, {
    bool bold = false,
    bool dim = false,
  }) {
    return [
      for (var j = 0; j < runs.length; j++)
        UiText(
          '$id-r$j',
          runs[j].text,
          style: _runStyle(runs[j], size, bold: bold, dim: dim),
        ),
    ];
  }

  UiStyle _runStyle(
    MdRun run,
    double size, {
    bool bold = false,
    bool dim = false,
  }) {
    var fg = '#d3d7cf';
    if (run.code) fg = '#fce94f';
    if (dim) fg = '#8a8a8a';
    return UiStyle(
      foreground: UiColor.hex(fg),
      background: run.code ? UiColor.hex('#2d2d2d') : null,
      fontSize: size,
      fontWeight: (run.bold || bold) ? UiFontWeight.bold : UiFontWeight.normal,
    );
  }

  UiStyle _fgStyle(String hex, double size, {bool bold = false}) => UiStyle(
    foreground: UiColor.hex(hex),
    fontSize: size,
    fontWeight: bold ? UiFontWeight.bold : UiFontWeight.normal,
  );

  UiStyle _dimStyle(double size) =>
      UiStyle(foreground: UiColor.hex('#555753'), fontSize: size);

  List<UiAction> actions() => [
    UiAction(
      name: 'notes.new',
      keys: 'n',
      context: UiActionContext.node('mdpane-$paneId'),
    ),
    UiAction(
      name: 'notes.open',
      keys: 'enter',
      context: UiActionContext.node('mdpane-$paneId'),
    ),
    UiAction(
      name: 'notes.save',
      keys: 'ctrl+s',
      context: UiActionContext.node('mdpane-$paneId'),
    ),
  ];
}
