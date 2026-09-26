/// Shared rendering helpers for the Git app pane.
///
/// Status glyphs, diff-line classification, and the RLE-style diff renderer
/// used by [GitPaneView]. These mirror the Rust `unpeel-apps` diffs app
/// (`crates/apps/diffs/src/ui.rs`, `git.rs`): status glyphs, unified diffs
/// with size limits, and syntax-agnostic line coloring.
///
/// STUB DATA SOURCE: the host has no git backend route yet (no
/// `HostClient.gitStatus` / `gitDiff`); the pane ships with
/// [StubGitDataSource] until the backend exists. See
/// docs/gpuidart-gaps-apps.md (GAP-A1).
library;

import 'package:gpuidart/gpuidart.dart';

/// Status of one file in the working tree, mirroring `git status --porcelain`.
enum GitFileStatus {
  modified('M', '#fce94f'),
  added('A', '#8ae234'),
  deleted('D', '#ef2929'),
  renamed('R', '#729fcf'),
  untracked('??', '#ad7fa8'),
  conflicted('UU', '#ff0000');

  const GitFileStatus(this.glyph, this.hex);
  final String glyph;
  final String hex;
}

/// One changed file: path plus its index/worktree status.
final class GitChangedFile {
  const GitChangedFile({
    required this.path,
    required this.status,
    this.staged = false,
  });

  final String path;
  final GitFileStatus status;
  final bool staged;
}

/// One commit in the history list.
final class GitCommit {
  const GitCommit({
    required this.sha,
    required this.author,
    required this.date,
    required this.message,
  });

  final String sha;
  final String author;
  final String date;
  final String message;

  String get shortSha => sha.length > 7 ? sha.substring(0, 7) : sha;
}

/// Classification of one unified-diff line.
enum DiffLineKind {
  fileHeader, // diff --git / index / --- / +++
  hunkHeader, // @@ ... @@
  addition, // +
  deletion, // -
  context, // ' ' or anything else
}

/// One line of a unified diff.
final class DiffLine {
  const DiffLine(this.kind, this.text);
  final DiffLineKind kind;
  final String text;
}

/// Classify a raw unified-diff line. Mirrors `crates/apps/diffs/src/ui.rs`.
DiffLine classifyDiffLine(String raw) {
  if (raw.startsWith('diff --git') ||
      raw.startsWith('index ') ||
      raw.startsWith('--- ') ||
      raw.startsWith('+++ ')) {
    return DiffLine(DiffLineKind.fileHeader, raw);
  }
  if (raw.startsWith('@@')) {
    return DiffLine(DiffLineKind.hunkHeader, raw);
  }
  if (raw.startsWith('+')) {
    return DiffLine(DiffLineKind.addition, raw);
  }
  if (raw.startsWith('-')) {
    return DiffLine(DiffLineKind.deletion, raw);
  }
  return DiffLine(DiffLineKind.context, raw);
}

/// Maximum diff lines rendered per file before truncation, mirroring the
/// size limits in `crates/apps/diffs/src/highlight.rs`.
const int kMaxDiffLinesPerFile = 200;

/// Render one diff line as a [UiText] run (RLE fallback: one node per line).
///
/// Colors: additions green, deletions red, hunk headers blue, file headers
/// bold white, context dim gray. Syntax highlighting of patch tokens is a
/// gap (see docs/gpuidart-gaps-apps.md GAP-A3).
UiText diffLineNode(String nodeId, DiffLine line) {
  final (fg, bold) = switch (line.kind) {
    DiffLineKind.addition => ('#8ae234', false),
    DiffLineKind.deletion => ('#ef2929', false),
    DiffLineKind.hunkHeader => ('#729fcf', true),
    DiffLineKind.fileHeader => ('#eeeeec', true),
    DiffLineKind.context => ('#8a8a8a', false),
  };
  return UiText(
    nodeId,
    line.text.isEmpty ? ' ' : line.text,
    style: UiStyle(
      foreground: UiColor.hex(fg),
      fontSize: 13,
      fontWeight: bold ? UiFontWeight.bold : UiFontWeight.normal,
    ),
  );
}

/// Render a unified diff (already classified or raw) as a [UiColumn] of
/// [UiText] lines, truncated at [kMaxDiffLinesPerFile] with a marker row.
UiNode diffColumn(String nodeId, List<String> rawLines) {
  final lines = rawLines.length > kMaxDiffLinesPerFile
      ? rawLines.sublist(0, kMaxDiffLinesPerFile)
      : rawLines;
  final nodes = <UiNode>[
    for (var i = 0; i < lines.length; i++)
      diffLineNode('$nodeId-l$i', classifyDiffLine(lines[i])),
  ];
  if (rawLines.length > kMaxDiffLinesPerFile) {
    nodes.add(
      UiText(
        '$nodeId-truncated',
        '… ${rawLines.length - kMaxDiffLinesPerFile} more lines (size limit)',
        style: UiStyle(
          foreground: UiColor.hex('#ad7fa8'),
          fontSize: 12,
          fontWeight: UiFontWeight.medium,
        ),
      ),
    );
  }
  return UiColumn(nodeId, nodes);
}

/// Text gauge bar: `fraction` 0..1 over [width] cells.
/// Uses block characters so it renders in the RLE text fallback.
String gaugeBar(double fraction, int width) {
  final f = fraction.clamp(0.0, 1.0);
  final filled = (f * width).round().clamp(0, width);
  return '${'█' * filled}${'░' * (width - filled)}';
}

/// Stub git data source.
///
/// STUB: the Host has no git backend route yet, so the Git pane cannot show
/// live repository state. This stub provides representative data for the UI.
/// Replace with `HostClient.gitStatus()` / `gitDiff()` when the backend
/// exists (GAP-A1 in docs/gpuidart-gaps-apps.md).
final class StubGitDataSource {
  const StubGitDataSource();

  String get currentBranch => 'supercli-next';

  List<String> get branches => const [
    'supercli-next',
    'track-b-parity-apps',
    'supercli-main',
  ];

  (int, int) get aheadBehind => (2, 0);

  List<GitChangedFile> get changedFiles => const [
    GitChangedFile(
      path: 'clients/supercli-app/lib/screens/gitpaneview.dart',
      status: GitFileStatus.added,
      staged: true,
    ),
    GitChangedFile(
      path: 'clients/supercli-app/lib/screens/filespaneview.dart',
      status: GitFileStatus.added,
      staged: true,
    ),
    GitChangedFile(
      path: 'docs/parity/checklist.md',
      status: GitFileStatus.modified,
    ),
    GitChangedFile(
      path: 'crates/supercli-core/src/lib.rs',
      status: GitFileStatus.modified,
    ),
    GitChangedFile(path: 'notes/scratch.txt', status: GitFileStatus.untracked),
  ];

  List<GitCommit> get recentCommits => const [
    GitCommit(
      sha: 'd0145789abcdef0123456789abcdef0123456789',
      author: 'Amein Eskinder',
      date: '2026-09-26',
      message: 'rename-guard: allowlist docs/parity/',
    ),
    GitCommit(
      sha: '33c204dabcdef0123456789abcdef0123456789ab',
      author: 'Amein Eskinder',
      date: '2026-09-26',
      message: 'Merge track-b-terminal-pane@2385779',
    ),
    GitCommit(
      sha: 'ac2b8d3abcdef0123456789abcdef0123456789ab',
      author: 'Amein Eskinder',
      date: '2026-09-26',
      message: 'Merge track-b-parity-docs@116c59a',
    ),
  ];

  List<String> diffFor(String path) => [
    'diff --git a/$path b/$path',
    'index 1234567..89abcde 100644',
    '--- a/$path',
    '+++ b/$path',
    '@@ -1,4 +1,5 @@',
    ' context line one',
    '-old line removed',
    '+new line added',
    '+another added line',
    ' context line two',
  ];
}
