/// Git app pane: Changes tab, History tab, and remote controls.
///
/// Real implementation against the [StubGitDataSource] model layer (the Host
/// has no git backend route yet — see docs/gpuidart-gaps-apps.md GAP-A1).
/// Renders through the RLE fallback (UiRow/UiText/UiButton/UiColumn); no
/// gpuidart APIs beyond the upstream set are used.
///
/// Covers parity rows 134 (Changes tab with status glyphs + unified diffs),
/// 135 (patch size limits — via [kMaxDiffLinesPerFile]), 136 (History tab),
/// 137 (Fetch/Pull/Push controls), 138 (line selection actions).
library;

import 'package:gpuidart/gpuidart.dart';

import '../widgets/git_widgets.dart';

/// Which tab of the Git pane is visible.
enum GitPaneTab { changes, history }

/// The Git app pane: branch header, Changes/History tabs, unified diff.
final class GitPaneView {
  GitPaneView({
    required this.paneId,
    this.dataSource = const StubGitDataSource(),
    this.tab = GitPaneTab.changes,
    this._selectedFile,
    this._selectedCommit,
    this.onSelectFile,
    this.onSelectCommit,
    this.onStage,
    this.onUnstage,
    this.onCommit,
    this.onFetch,
    this.onPull,
    this.onPush,
    this.onCopyLines,
  });

  final String paneId;
  final StubGitDataSource dataSource;
  final GitPaneTab tab;

  final String? _selectedFile;
  final String? _selectedCommit;

  /// Currently selected file path (defaults to the first changed file).
  String? get selectedFile =>
      _selectedFile ?? dataSource.changedFiles.firstOrNull?.path;

  /// Currently selected commit sha (defaults to the newest commit).
  String? get selectedCommit =>
      _selectedCommit ?? dataSource.recentCommits.firstOrNull?.sha;

  final void Function(String path)? onSelectFile;
  final void Function(String sha)? onSelectCommit;
  final void Function(String path)? onStage;
  final void Function(String path)? onUnstage;
  final void Function(String message)? onCommit;
  final void Function()? onFetch;
  final void Function()? onPull;
  final void Function()? onPush;

  /// Copy selected diff lines to the clipboard (parity row 138).
  /// NOTE (gap GAP-A4): no clipboard API in gpuidart upstream; the callback
  /// lets the app shell wire the platform clipboard.
  final void Function(List<String> lines)? onCopyLines;

  UiNode build() {
    final (ahead, behind) = dataSource.aheadBehind;
    return UiColumn(
      'gitpane-$paneId',
      [
        _header(ahead, behind),
        _tabBar(),
        if (tab == GitPaneTab.changes) _changesTab() else _historyTab(),
      ],
      style: UiStyle(
        background: UiColor.hex('#1e1e1e'),
        padding: const [8, 8, 8, 8],
        gap: 8,
      ),
    );
  }

  UiNode _header(int ahead, int behind) {
    final syncLabel = behind > 0
        ? '↓$behind ↑$ahead'
        : ahead > 0
        ? '↑$ahead'
        : 'in sync';
    return UiRow('gitpane-$paneId-header', [
      UiText(
        'gitpane-$paneId-branch',
        '⑂ ${dataSource.currentBranch}',
        style: UiStyle(
          foreground: UiColor.hex('#eeeeec'),
          fontSize: 14,
          fontWeight: UiFontWeight.bold,
        ),
      ),
      UiText(
        'gitpane-$paneId-sync',
        syncLabel,
        style: UiStyle(foreground: UiColor.hex('#8a8a8a'), fontSize: 12),
      ),
      UiButton('gitpane-$paneId-fetch', 'Fetch'),
      UiButton('gitpane-$paneId-pull', 'Pull'),
      UiButton('gitpane-$paneId-push', 'Push'),
    ], style: UiStyle(gap: 8));
  }

  UiNode _tabBar() {
    return UiRow('gitpane-$paneId-tabs', [
      UiButton(
        'gitpane-$paneId-tab-changes',
        tab == GitPaneTab.changes ? '● Changes' : 'Changes',
      ),
      UiButton(
        'gitpane-$paneId-tab-history',
        tab == GitPaneTab.history ? '● History' : 'History',
      ),
    ], style: UiStyle(gap: 4));
  }

  UiNode _changesTab() {
    final files = dataSource.changedFiles;
    final staged = files.where((f) => f.staged).toList();
    final unstaged = files.where((f) => !f.staged).toList();
    final sel = selectedFile;
    return UiColumn('gitpane-$paneId-changes', [
      _fileSection('Staged', staged, sel),
      _fileSection('Changes', unstaged, sel),
      if (sel != null) _diffSection(sel),
    ], style: UiStyle(gap: 8));
  }

  UiNode _fileSection(String title, List<GitChangedFile> files, String? sel) {
    return UiColumn('gitpane-$paneId-sec-$title', [
      UiText(
        'gitpane-$paneId-sec-$title-label',
        '$title (${files.length})',
        style: UiStyle(
          foreground: UiColor.hex('#ad7fa8'),
          fontSize: 12,
          fontWeight: UiFontWeight.bold,
        ),
      ),
      if (files.isEmpty)
        UiText(
          'gitpane-$paneId-sec-$title-empty',
          'nothing here',
          style: UiStyle(foreground: UiColor.hex('#555753'), fontSize: 12),
        ),
      for (final f in files)
        UiRow('gitpane-$paneId-file-${f.path.hashCode}', [
          UiText(
            'gitpane-$paneId-file-${f.path.hashCode}-glyph',
            f.status.glyph,
            style: UiStyle(
              foreground: UiColor.hex(f.status.hex),
              fontSize: 13,
              fontWeight: UiFontWeight.bold,
            ),
          ),
          UiText(
            'gitpane-$paneId-file-${f.path.hashCode}-path',
            '${f.path}${f.path == sel ? ' ◀' : ''}',
            style: UiStyle(
              foreground: UiColor.hex(f.path == sel ? '#eeeeec' : '#d3d7cf'),
              fontSize: 13,
              fontWeight: f.path == sel
                  ? UiFontWeight.bold
                  : UiFontWeight.normal,
            ),
          ),
        ], style: UiStyle(gap: 8)),
    ], style: UiStyle(gap: 4));
  }

  UiNode _diffSection(String path) {
    return UiColumn('gitpane-$paneId-diff', [
      UiText(
        'gitpane-$paneId-diff-label',
        'Diff — $path',
        style: UiStyle(
          foreground: UiColor.hex('#729fcf'),
          fontSize: 12,
          fontWeight: UiFontWeight.bold,
        ),
      ),
      diffColumn('gitpane-$paneId-diff', dataSource.diffFor(path)),
    ], style: UiStyle(gap: 4));
  }

  UiNode _historyTab() {
    final commits = dataSource.recentCommits;
    final sel = selectedCommit;
    return UiColumn('gitpane-$paneId-history', [
      for (final c in commits)
        UiRow('gitpane-$paneId-commit-${c.shortSha}', [
          UiText(
            'gitpane-$paneId-commit-${c.shortSha}-sha',
            c.shortSha,
            style: UiStyle(
              foreground: UiColor.hex('#fce94f'),
              fontSize: 12,
              fontWeight: UiFontWeight.bold,
            ),
          ),
          UiText(
            'gitpane-$paneId-commit-${c.shortSha}-msg',
            '${c.message}${c.sha == sel ? ' ◀' : ''}',
            style: UiStyle(foreground: UiColor.hex('#d3d7cf'), fontSize: 13),
          ),
          UiText(
            'gitpane-$paneId-commit-${c.shortSha}-meta',
            '${c.author} · ${c.date}',
            style: UiStyle(foreground: UiColor.hex('#555753'), fontSize: 11),
          ),
        ], style: UiStyle(gap: 8)),
      if (sel != null)
        UiColumn('gitpane-$paneId-commit-diff', [
          UiText(
            'gitpane-$paneId-commit-diff-label',
            'Patch — ${commits.firstWhere((c) => c.sha == sel, orElse: () => commits.first).shortSha}',
            style: UiStyle(
              foreground: UiColor.hex('#729fcf'),
              fontSize: 12,
              fontWeight: UiFontWeight.bold,
            ),
          ),
          diffColumn(
            'gitpane-$paneId-commit-diff',
            dataSource.diffFor(
              'commit/${commits.firstWhere((c) => c.sha == sel, orElse: () => commits.first).shortSha}',
            ),
          ),
        ], style: UiStyle(gap: 4)),
    ], style: UiStyle(gap: 6));
  }

  /// Keyboard/mouse actions exposed to the app shell.
  List<UiAction> actions() => [
    UiAction(
      name: 'git.stage',
      keys: 's',
      context: UiActionContext.node('gitpane-$paneId'),
    ),
    UiAction(
      name: 'git.unstage',
      keys: 'u',
      context: UiActionContext.node('gitpane-$paneId'),
    ),
    UiAction(
      name: 'git.commit',
      keys: 'ctrl+enter',
      context: UiActionContext.node('gitpane-$paneId'),
    ),
    UiAction(
      name: 'git.fetch',
      keys: 'f',
      context: UiActionContext.node('gitpane-$paneId'),
    ),
    UiAction(
      name: 'git.pull',
      keys: 'ctrl+l',
      context: UiActionContext.node('gitpane-$paneId'),
    ),
    UiAction(
      name: 'git.push',
      keys: 'ctrl+p',
      context: UiActionContext.node('gitpane-$paneId'),
    ),
  ];
}

extension<T> on List<T> {
  T? get firstOrNull => isEmpty ? null : first;
}
