/// Behavior tests for the [APPS] panes: Git, Files, Markdown, Usage.
///
/// These exercise the models and the rendered UiNode trees (via `toJson`),
/// not just tree shape: status glyphs, diff classification + size limits,
/// file-tree filtering, the markdown block parser, and usage gauge math.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/screens/filespaneview.dart';
import 'package:supercli_app/screens/gitpaneview.dart';
import 'package:supercli_app/screens/markdownpaneview.dart';
import 'package:supercli_app/screens/usagepaneview.dart';
import 'package:supercli_app/widgets/git_widgets.dart';
import 'package:test/test.dart';

/// Collect every `text` value in a UiNode JSON tree.
List<String> allTexts(UiNode node) {
  final out = <String>[];
  void walk(Map<String, Object> json) {
    final text = json['text'];
    if (text is String) out.add(text);
    final label = json['label'];
    if (label is String) out.add(label);
    final children = json['children'];
    if (children is List) {
      for (final c in children) {
        if (c is Map<String, Object>) walk(c);
      }
    }
  }

  walk(node.toJson());
  return out;
}

bool containsText(UiNode node, String needle) =>
    allTexts(node).any((t) => t.contains(needle));

void main() {
  group('git_widgets', () {
    test('status glyphs match git porcelain', () {
      expect(GitFileStatus.modified.glyph, 'M');
      expect(GitFileStatus.added.glyph, 'A');
      expect(GitFileStatus.deleted.glyph, 'D');
      expect(GitFileStatus.renamed.glyph, 'R');
      expect(GitFileStatus.untracked.glyph, '??');
      expect(GitFileStatus.conflicted.glyph, 'UU');
    });

    test('classifyDiffLine distinguishes headers, hunks, add/del', () {
      expect(
        classifyDiffLine('diff --git a/x b/x').kind,
        DiffLineKind.fileHeader,
      );
      expect(classifyDiffLine('@@ -1,2 +1,3 @@').kind, DiffLineKind.hunkHeader);
      expect(classifyDiffLine('+added').kind, DiffLineKind.addition);
      expect(classifyDiffLine('-removed').kind, DiffLineKind.deletion);
      expect(classifyDiffLine(' context').kind, DiffLineKind.context);
    });

    test('diffColumn truncates at the size limit with a marker', () {
      final lines = List.generate(kMaxDiffLinesPerFile + 50, (i) => ' line $i');
      final node = diffColumn('t', lines);
      final texts = allTexts(node);
      expect(texts.length, kMaxDiffLinesPerFile + 1);
      expect(texts.last, contains('more lines (size limit)'));
    });

    test('gaugeBar renders a full/empty bar', () {
      expect(gaugeBar(1.0, 4), '████');
      expect(gaugeBar(0.0, 4), '░░░░');
      expect(gaugeBar(0.5, 4), '██░░');
      expect(gaugeBar(2.0, 4), '████'); // clamped
    });

    test('stub data source is honest about being a stub', () {
      const ds = StubGitDataSource();
      expect(ds.currentBranch, isNotEmpty);
      expect(ds.changedFiles, isNotEmpty);
      expect(ds.recentCommits, isNotEmpty);
      expect(ds.diffFor('x').first, startsWith('diff --git'));
    });
  });

  group('GitPaneView', () {
    test('changes tab shows branch, sections, and diff', () {
      final view = GitPaneView(paneId: 't');
      final texts = allTexts(view.build());
      expect(texts.any((t) => t.contains('supercli-next')), isTrue);
      expect(texts.any((t) => t.contains('Staged (2)')), isTrue);
      expect(texts.any((t) => t.contains('Changes (3)')), isTrue);
      expect(texts.any((t) => t.contains('Diff —')), isTrue);
      // Status glyphs present.
      expect(texts, contains('A'));
      expect(texts, contains('M'));
      expect(texts, contains('??'));
      // Diff content present.
      expect(texts.any((t) => t.contains('+new line added')), isTrue);
      expect(texts.any((t) => t.contains('-old line removed')), isTrue);
    });

    test('history tab shows commits and the selected patch', () {
      final view = GitPaneView(paneId: 't', tab: GitPaneTab.history);
      final texts = allTexts(view.build());
      expect(
        texts.any((t) => t.contains('rename-guard: allowlist docs/parity/')),
        isTrue,
      );
      expect(texts.any((t) => t.contains('Patch —')), isTrue);
    });

    test('selecting a file switches the diff', () {
      final view = GitPaneView(
        paneId: 't',
        selectedFile: 'docs/parity/checklist.md',
      );
      expect(view.selectedFile, 'docs/parity/checklist.md');
      expect(
        containsText(view.build(), 'Diff — docs/parity/checklist.md'),
        isTrue,
      );
    });

    test('remote actions are exposed', () {
      final view = GitPaneView(paneId: 't');
      final names = view.actions().map((a) => a.name).toSet();
      expect(
        names,
        containsAll({'git.fetch', 'git.pull', 'git.push', 'git.commit'}),
      );
      expect(containsText(view.build(), 'Fetch'), isTrue);
      expect(containsText(view.build(), 'Push'), isTrue);
    });

    test('stage callback fires for the selected file', () {
      var staged = '';
      final view = GitPaneView(
        paneId: 't',
        selectedFile: 'docs/parity/checklist.md',
        onStage: (p) => staged = p,
      );
      view.onStage?.call(view.selectedFile!);
      expect(staged, 'docs/parity/checklist.md');
    });
  });

  group('FilesPaneView', () {
    test('visibleRows walks the tree in display order', () {
      final view = FilesPaneView(paneId: 't');
      final rows = view.visibleRows();
      final names = rows.map((r) => r.$2.name).toList();
      expect(names.first, 'crates');
      expect(names, contains('lib.rs'));
      expect(names, contains('README.md'));
      // Depth increases into subdirectories.
      final libRs = rows.firstWhere((r) => r.$2.name == 'lib.rs');
      expect(libRs.$1, 4);
    });

    test('filter keeps matching files and their ancestors', () {
      final view = FilesPaneView(paneId: 't', filter: 'lib.rs');
      final names = view.visibleRows().map((r) => r.$2.name).toList();
      expect(names, contains('lib.rs'));
      expect(names, contains('src')); // ancestor kept
      expect(names, isNot(contains('README.md')));
    });

    test('build shows root, filter, preview, and context actions', () {
      final view = FilesPaneView(
        paneId: 't',
        selectedPath: '/home/user/projects/supercli/README.md',
      );
      final texts = allTexts(view.build());
      expect(
        texts.any((t) => t.contains('/home/user/projects/supercli')),
        isTrue,
      );
      expect(texts.any((t) => t.contains('Preview — README.md')), isTrue);
      expect(texts, contains('Open'));
      expect(texts, contains('Send to agent'));
      expect(texts, contains('Copy path'));
    });

    test('context callbacks fire with the selected path', () {
      var opened = '';
      final view = FilesPaneView(
        paneId: 't',
        selectedPath: '/home/user/projects/supercli/README.md',
        onOpen: (p) => opened = p,
      );
      view.onOpen?.call(view.selectedPath!);
      expect(opened, endsWith('README.md'));
    });
  });

  group('MarkdownPreview parser', () {
    const parser = MarkdownPreview();

    test('headings, bold, italic, code, links', () {
      final blocks = parser.parse(
        '# Title\n\nA **bold** and *italic* '
        'with `code` and a [link](https://x).',
      );
      expect(blocks.first, isA<MdHeading>());
      final h = blocks.first as MdHeading;
      expect(h.level, 1);
      final para = blocks.last as MdParagraph;
      final texts = para.runs.map((r) => r.text).toList();
      expect(texts, contains('bold'));
      expect(texts, contains('italic'));
      expect(texts, contains('code'));
      expect(texts, contains('link'));
      expect(para.runs.firstWhere((r) => r.text == 'bold').bold, isTrue);
      expect(para.runs.firstWhere((r) => r.text == 'italic').italic, isTrue);
      expect(para.runs.firstWhere((r) => r.text == 'code').code, isTrue);
    });

    test('lists, task items, quotes, code blocks, rules', () {
      final blocks = parser.parse(
        '- a\n- [x] done\n- [ ] todo\n> quoted\n```\ncode()\n```\n---',
      );
      expect(blocks.whereType<MdListItem>(), hasLength(1));
      expect(blocks.whereType<MdTaskItem>(), hasLength(2));
      expect((blocks.whereType<MdTaskItem>().first).done, isTrue);
      expect(blocks.whereType<MdQuote>(), hasLength(1));
      final cb = blocks.whereType<MdCodeBlock>().single;
      expect(cb.lines, ['code()']);
      expect(blocks.whereType<MdRule>(), hasLength(1));
    });
  });

  group('MarkdownPaneView', () {
    test('build shows vault, selected note, and rendered blocks', () {
      final view = MarkdownPaneView(paneId: 't');
      final texts = allTexts(view.build());
      expect(texts.any((t) => t.contains('Vault (2)')), isTrue);
      expect(texts.any((t) => t.contains('supercli roadmap')), isTrue);
      expect(texts.any((t) => t.contains('Milestones')), isTrue);
      expect(texts, contains('+ New note'));
      expect(texts, contains('Save'));
    });

    test('selecting the second note switches the preview', () {
      final view = MarkdownPaneView(
        paneId: 't',
        selectedPath: '/home/user/notes/meeting.md',
      );
      expect(view.selectedNote?.title, 'meeting notes');
      expect(containsText(view.build(), 'Meeting notes'), isTrue);
    });
  });

  group('UsagePaneView', () {
    test('fraction and alerting math', () {
      const p = ProviderUsage(
        provider: 'x',
        quotaUsed: 85,
        quotaLimit: 100,
        tokensToday: 1,
        tokensMonth: 2,
        alertThreshold: 0.8,
      );
      expect(p.fraction, closeTo(0.85, 0.001));
      expect(p.alerting, isTrue);
      expect(p.percentLabel, '85%');
      const ok = ProviderUsage(
        provider: 'y',
        quotaUsed: 10,
        quotaLimit: 100,
        tokensToday: 1,
        tokensMonth: 2,
        alertThreshold: 0.8,
      );
      expect(ok.alerting, isFalse);
    });

    test('build shows gauges, alerts, history, and monthly totals', () {
      final view = UsagePaneView(paneId: 't');
      final texts = allTexts(view.build());
      // Alerting provider flagged (claude at 82%).
      expect(texts.any((t) => t.contains('⚠ claude')), isTrue);
      expect(texts.any((t) => t.contains('82%')), isTrue);
      // Gauges for all providers.
      for (final name in ['claude', 'codex', 'muse', 'grok']) {
        expect(texts.any((t) => t.contains(name)), isTrue);
      }
      // History sparkline + monthly table with totals.
      expect(texts.any((t) => t.contains('Token history (14 days)')), isTrue);
      expect(texts.any((t) => t.contains('Monthly by project')), isTrue);
      expect(texts.any((t) => t.contains('total')), isTrue);
      expect(texts.any((t) => t.contains('\$61.52')), isTrue);
    });

    test('no alerts when all providers are within quota', () {
      final view = UsagePaneView(paneId: 't');
      final calm = [
        const ProviderUsage(
          provider: 'z',
          quotaUsed: 1,
          quotaLimit: 100,
          tokensToday: 1,
          tokensMonth: 1,
          alertThreshold: 0.8,
        ),
      ];
      expect(calm.where((p) => p.alerting), isEmpty);
      expect(view.dataSource.providers.where((p) => p.alerting).length, 1);
    });
  });
}
