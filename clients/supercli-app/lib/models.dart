/// Domain models for the supercli desktop app.
///
/// These mirror the Host's JSON wire format (see supercli-serve/src/mobile.rs
/// and supercli-serve/src/approvals.rs). Keep in sync with the Rust side.
library;

/// A chat/session thread on the Host.
final class SessionSummary {
  const SessionSummary({
    required this.id,
    required this.title,
    required this.updatedAt,
    this.unreadCount = 0,
  });

  final String id;
  final String title;
  final DateTime updatedAt;
  final int unreadCount;

  factory SessionSummary.fromJson(Map<String, dynamic> json) {
    return SessionSummary(
      id: json['id'] as String,
      title: (json['title'] as String?) ?? 'Untitled',
      updatedAt: DateTime.tryParse(json['updated_at'] as String? ?? '') ??
          DateTime.fromMillisecondsSinceEpoch(0),
      unreadCount: (json['unread_count'] as num?)?.toInt() ?? 0,
    );
  }

  Map<String, Object> toJson() => {
        'id': id,
        'title': title,
        'updated_at': updatedAt.toIso8601String(),
        'unread_count': unreadCount,
      };
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
      requestedAtUnixMs:
          (json['requestedAtUnixMs'] as num?)?.toInt() ?? 0,
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
