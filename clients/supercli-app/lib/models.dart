/// Domain models for the supercli desktop app.
///
/// These mirror the Host's JSON wire format (see supercli-serve/src/mobile.rs
/// and supercli-serve/src/approvals.rs). Keep in sync with the Rust side.
library;

/// A chat/session thread on the Host.
///
/// Wire format is the real Host dialect (supercli-serve/src/sessions.rs
/// `session_json`): `{id, projectID, title, command, createdAtUnixMs,
/// updatedAtUnixMs, status, activity, unread, pinned, ...}`.
/// The legacy snake_case names (`updated_at`, `unread_count`) are still
/// accepted for old fixtures.
final class SessionSummary {
  const SessionSummary({
    required this.id,
    required this.title,
    required this.updatedAt,
    this.unreadCount = 0,
    this.command = '',
    this.cwd = '',
    this.agentId = '',
    this.appName = '',
    this.projectId = '',
    this.status = '',
    this.activity = '',
    this.pinned = false,
  });

  final String id;
  final String title;
  final DateTime updatedAt;
  final int unreadCount;

  /// Raw launch command (wire `command`). Shown as secondary text, never
  /// as the primary label — see [displayTitle].
  final String command;

  /// Launch working directory (wire `cwd`).
  final String cwd;

  /// Host-observed foreground runtime id (wire `activeRuntimeID`),
  /// e.g. "claude". Empty when the session is a plain terminal.
  final String agentId;

  /// Host-resolved installed App name (wire `activeAppName`).
  final String appName;

  /// Host project this session belongs to (`projectID` on the wire).
  final String projectId;

  /// Host lifecycle status: "running" | "exited".
  final String status;

  /// Host activity: "starting" | "working" | "blocked" | "idle" | "done".
  final String activity;

  /// Pinned on the Host.
  final bool pinned;

  /// Human-friendly primary label for the sidebar row.
  ///
  /// Prefers the Host label when it carries meaning beyond the raw
  /// command (custom title, agent terminal title, app title marker).
  /// Otherwise derives an "agent · folder" style label like the native
  /// client, so rows never show a raw command line as their title.
  String get displayTitle {
    final t = title.trim();
    final c = command.trim();
    if (t.isNotEmpty && t != 'Untitled' && t != c) return t;
    final agent = agentLabel;
    final folder = shortCwd;
    if (agent.isNotEmpty && folder.isNotEmpty) return '$agent · $folder';
    if (agent.isNotEmpty) return agent;
    if (folder.isNotEmpty) return folder;
    if (c.isNotEmpty) return c;
    return 'Terminal';
  }

  /// Secondary line for the sidebar row: agent and working directory,
  /// with the raw command appended when it differs from [displayTitle].
  /// This is the secondary-text carrier for the command (gpuidart has
  /// no tooltip primitive).
  String get subtitle {
    final parts = <String>[];
    if (agentLabel.isNotEmpty) parts.add(agentLabel);
    if (shortCwd.isNotEmpty) parts.add(shortCwd);
    var sub = parts.join(' · ');
    final c = command.trim();
    if (c.isNotEmpty && c != displayTitle) {
      sub = sub.isEmpty ? c : '$sub — $c';
    }
    return sub;
  }

  /// Display name for the agent: the App name when an installed App owns
  /// the session, else the runtime id capitalized ("claude" → "Claude").
  String get agentLabel {
    if (appName.trim().isNotEmpty) return appName.trim();
    final id = agentId.trim();
    if (id.isEmpty) return '';
    return id[0].toUpperCase() + id.substring(1);
  }

  /// Cwd abbreviated for display: the home dir becomes "~/…", otherwise
  /// the last two path segments.
  String get shortCwd {
    final raw = cwd.trim();
    if (raw.isEmpty) return '';
    var prefix = '';
    var path = raw;
    const homePrefix = '/home/';
    if (path.startsWith(homePrefix)) {
      final rest = path.substring(homePrefix.length);
      final slash = rest.indexOf('/');
      if (slash < 0) return '~'; // exactly /home/<user>
      prefix = '~/';
      path = rest.substring(slash + 1);
    }
    final segments = path.split('/').where((s) => s.isNotEmpty).toList();
    if (segments.length <= 2) {
      // Short non-home paths keep their original form (leading slash).
      return prefix.isEmpty ? raw : '$prefix${segments.join('/')}';
    }
    return '$prefix…/${segments.sublist(segments.length - 2).join('/')}';
  }

  /// True when the Host reports unread activity (`unread: true`).
  bool get unread => unreadCount > 0;

  /// True when the agent is actively working (drives the busy spinner).
  bool get isBusy => activity == 'working' || activity == 'starting';

  /// True when the session needs attention (drives the attention dot).
  bool get needsAttention => activity == 'blocked';

  factory SessionSummary.fromJson(Map<String, dynamic> json) {
    // Wire format (camelCase) takes precedence; older snake_case spellings
    // are kept for backward compat. `unread` may arrive as a bool or a
    // number from the Host; fall back to `unread_count`.
    int unreadCount;
    final unread = json['unread'];
    if (unread is bool) {
      unreadCount = unread ? 1 : 0;
    } else if (unread is num) {
      unreadCount = unread.toInt();
    } else {
      unreadCount = (json['unread_count'] as num?)?.toInt() ?? 0;
    }
    return SessionSummary(
      id: json['id'] as String,
      title: (json['title'] as String?) ?? 'Untitled',
      updatedAt: _parseUpdatedAt(json),
      unreadCount: unreadCount,
      command: (json['command'] as String?) ?? '',
      cwd: (json['cwd'] as String?) ?? '',
      agentId: (json['activeRuntimeID'] as String?) ?? '',
      appName: (json['activeAppName'] as String?) ?? '',
      projectId:
          (json['projectID'] as String?) ??
          (json['project_id'] as String?) ??
          '',
      status: (json['status'] as String?) ?? '',
      activity: (json['activity'] as String?) ?? '',
      pinned: (json['pinned'] as bool?) ?? false,
    );
  }

  /// The Host wire format carries `updatedAtUnixMs` (int ms). Tolerate the
  /// legacy `updated_at` ISO string too.
  static DateTime _parseUpdatedAt(Map<String, dynamic> json) {
    final ms = json['updatedAtUnixMs'];
    if (ms is num) {
      return DateTime.fromMillisecondsSinceEpoch(ms.toInt(), isUtc: true);
    }
    return DateTime.tryParse(json['updated_at'] as String? ?? '') ??
        DateTime.fromMillisecondsSinceEpoch(0);
  }

  Map<String, Object> toJson() => {
    'id': id,
    'title': title,
    'updated_at': updatedAt.toIso8601String(),
    'updatedAtUnixMs': updatedAt.millisecondsSinceEpoch,
    'unread_count': unreadCount,
    'unread': unreadCount > 0,
    'command': command,
    'cwd': cwd,
    'activeRuntimeID': agentId,
    'activeAppName': appName,
    'projectID': projectId,
    'status': status,
    'activity': activity,
    'pinned': pinned,
  };
}

/// A project on the Host, from the bootstrap `projects` array.
///
/// Wire format (supercli-serve/src/sessions.rs): `{id, name, path,
/// mcpBlocked, archivedSessionCount, ...}`.
final class HostProject {
  const HostProject({required this.id, required this.name, this.path = ''});

  final String id;
  final String name;
  final String path;

  factory HostProject.fromJson(Map<String, dynamic> json) {
    return HostProject(
      id: json['id'] as String,
      name: (json['name'] as String?) ?? (json['id'] as String),
      path: (json['path'] as String?) ?? '',
    );
  }
}

/// A pending approval request from the Host.
///
/// Wire format is the real Host dialect (supercli-serve/src/approvals.rs
/// `list_json`, surfaced via `GET /mobile/bootstrap` as `pendingApprovals`):
/// `{id, kind, title, body, callerSessionID, requestedAtUnixMs,
/// targetSessionID?}`.
final class PendingApproval {
  const PendingApproval({
    required this.id,
    required this.tool,
    required this.summary,
    required this.detail,
    this.callerSessionId = '',
    this.requestedAtUnixMs = 0,
    this.generation = 0,
  });

  final String id;

  /// The approval kind (e.g. "tool"); shown as the tool name.
  final String tool;

  /// The approval title; shown as the summary line.
  final String summary;

  /// The approval body; shown as the detail text.
  final String detail;

  final String callerSessionId;
  final int requestedAtUnixMs;
  final int generation;

  factory PendingApproval.fromJson(Map<String, dynamic> json) {
    return PendingApproval(
      id: json['id'] as String,
      tool: (json['kind'] as String?) ?? (json['tool'] as String?) ?? 'unknown',
      summary: (json['title'] as String?) ?? (json['summary'] as String?) ?? '',
      detail: (json['body'] as String?) ?? (json['detail'] as String?) ?? '',
      callerSessionId: (json['callerSessionID'] as String?) ?? '',
      requestedAtUnixMs: (json['requestedAtUnixMs'] as num?)?.toInt() ?? 0,
      generation: (json['generation'] as num?)?.toInt() ?? 0,
    );
  }

  Map<String, Object> toJson() => {
    'id': id,
    'kind': tool,
    'title': summary,
    'body': detail,
    'callerSessionID': callerSessionId,
    'requestedAtUnixMs': requestedAtUnixMs,
    'generation': generation,
  };
}

/// The answer to an approval. Idempotency is enforced by the Host on the
/// approval id; the client must not send the same id twice.
final class ApprovalAnswer {
  const ApprovalAnswer.approve(this.id) : approved = true;
  const ApprovalAnswer.deny(this.id) : approved = false;

  final String id;
  final bool approved;

  Map<String, Object> toJson() => {'id': id, 'approved': approved};
}
