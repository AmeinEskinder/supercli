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
    this.projectId = '',
    this.status = '',
    this.activity = '',
    this.pinned = false,
  });

  final String id;
  final String title;
  final DateTime updatedAt;
  final int unreadCount;

  /// Host project this session belongs to (`projectID` on the wire).
  final String projectId;

  /// Host lifecycle status: "running" | "exited".
  final String status;

  /// Host activity: "starting" | "working" | "blocked" | "idle" | "done".
  final String activity;

  /// Pinned on the Host.
  final bool pinned;

  /// True when the Host reports unread activity (`unread: true`).
  bool get unread => unreadCount > 0;

  /// True when the agent is actively working (drives the busy spinner).
  bool get isBusy => activity == 'working' || activity == 'starting';

  /// True when the session needs attention (drives the attention dot).
  bool get needsAttention => activity == 'blocked';

  factory SessionSummary.fromJson(Map<String, dynamic> json) {
    // Real Host wire format first (camelCase + Unix ms), then legacy
    // snake_case fallbacks.
    final updatedMs = (json['updatedAtUnixMs'] as num?)?.toInt();
    final unreadBool = json['unread'] as bool?;
    final unreadCountLegacy = (json['unread_count'] as num?)?.toInt();
    return SessionSummary(
      id: json['id'] as String,
      title: (json['title'] as String?) ?? 'Untitled',
      updatedAt: updatedMs != null
          ? DateTime.fromMillisecondsSinceEpoch(updatedMs, isUtc: true)
          : DateTime.tryParse(json['updated_at'] as String? ?? '') ??
                DateTime.fromMillisecondsSinceEpoch(0),
      unreadCount: unreadBool != null
          ? (unreadBool ? 1 : 0)
          : (unreadCountLegacy ?? 0),
      projectId:
          (json['projectID'] as String?) ??
          (json['project_id'] as String?) ??
          '',
      status: (json['status'] as String?) ?? '',
      activity: (json['activity'] as String?) ?? '',
      pinned: (json['pinned'] as bool?) ?? false,
    );
  }

  Map<String, Object> toJson() => {
    'id': id,
    'title': title,
    'updatedAtUnixMs': updatedAt.millisecondsSinceEpoch,
    'unread': unreadCount > 0,
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
