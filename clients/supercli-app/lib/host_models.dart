/// Git, files, and usage models for the supercli desktop app.
///
/// These mirror the Host's JSON wire format
/// (`crates/supercli-core/src/host_git.rs`). Keep in sync with the Rust side.
library;

/// One changed file in `git status --porcelain` output.
final class GitFileChange {
  const GitFileChange({
    required this.path,
    required this.indexStatus,
    required this.worktreeStatus,
    required this.staged,
  });

  final String path;
  final String indexStatus;
  final String worktreeStatus;
  final bool staged;

  factory GitFileChange.fromJson(Map<String, dynamic> json) {
    return GitFileChange(
      path: json['path'] as String,
      indexStatus: (json['indexStatus'] as String?) ?? ' ',
      worktreeStatus: (json['worktreeStatus'] as String?) ?? ' ',
      staged: (json['staged'] as bool?) ?? false,
    );
  }
}

/// `GET /mobile/git/status` result.
final class GitStatus {
  const GitStatus({
    required this.repoRoot,
    this.branch,
    this.ahead = 0,
    this.behind = 0,
    this.files = const [],
  });

  final String repoRoot;
  final String? branch;
  final int ahead;
  final int behind;
  final List<GitFileChange> files;

  factory GitStatus.fromJson(Map<String, dynamic> json) {
    final files = (json['files'] as List?) ?? const [];
    return GitStatus(
      repoRoot: (json['repoRoot'] as String?) ?? '',
      branch: json['branch'] as String?,
      ahead: (json['ahead'] as num?)?.toInt() ?? 0,
      behind: (json['behind'] as num?)?.toInt() ?? 0,
      files: files
          .map((f) => GitFileChange.fromJson(f as Map<String, dynamic>))
          .toList(),
    );
  }
}

/// One commit in `GET /mobile/git/history`.
final class GitHistoryCommit {
  const GitHistoryCommit({
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

  factory GitHistoryCommit.fromJson(Map<String, dynamic> json) {
    return GitHistoryCommit(
      sha: (json['sha'] as String?) ?? '',
      author: (json['author'] as String?) ?? '',
      date: (json['date'] as String?) ?? '',
      message: (json['message'] as String?) ?? '',
    );
  }
}

/// One entry of `GET /mobile/files/list`.
final class HostFileEntry {
  const HostFileEntry({
    required this.name,
    required this.isDir,
    this.size = 0,
  });

  final String name;
  final bool isDir;
  final int size;

  factory HostFileEntry.fromJson(Map<String, dynamic> json) {
    return HostFileEntry(
      name: json['name'] as String,
      isDir: (json['isDir'] as bool?) ?? false,
      size: (json['size'] as num?)?.toInt() ?? 0,
    );
  }
}

/// One provider row of `GET /mobile/usage/stats`.
final class UsageProvider {
  const UsageProvider({
    required this.provider,
    required this.transcriptDir,
    required this.transcriptSessions,
  });

  final String provider;
  final String transcriptDir;
  final int transcriptSessions;

  factory UsageProvider.fromJson(Map<String, dynamic> json) {
    return UsageProvider(
      provider: (json['provider'] as String?) ?? '',
      transcriptDir: (json['transcriptDir'] as String?) ?? '',
      transcriptSessions: (json['transcriptSessions'] as num?)?.toInt() ?? 0,
    );
  }
}

/// `GET /mobile/usage/stats` result.
final class UsageStats {
  const UsageStats({
    required this.hostVersion,
    required this.sessionsTotal,
    required this.sessionsRunning,
    this.providers = const [],
  });

  final String hostVersion;
  final int sessionsTotal;
  final int sessionsRunning;
  final List<UsageProvider> providers;

  factory UsageStats.fromJson(Map<String, dynamic> json) {
    final providers = (json['providers'] as List?) ?? const [];
    return UsageStats(
      hostVersion: (json['hostVersion'] as String?) ?? '',
      sessionsTotal: (json['sessionsTotal'] as num?)?.toInt() ?? 0,
      sessionsRunning: (json['sessionsRunning'] as num?)?.toInt() ?? 0,
      providers: providers
          .map((p) => UsageProvider.fromJson(p as Map<String, dynamic>))
          .toList(),
    );
  }
}
