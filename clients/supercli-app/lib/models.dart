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
final class PendingApproval {
  const PendingApproval({
    required this.id,
    required this.tool,
    required this.summary,
    required this.detail,
    this.generation = 0,
  });

  final String id;
  final String tool;
  final String summary;
  final String detail;
  final int generation;

  factory PendingApproval.fromJson(Map<String, dynamic> json) {
    return PendingApproval(
      id: json['id'] as String,
      tool: (json['tool'] as String?) ?? 'unknown',
      summary: (json['summary'] as String?) ?? '',
      detail: (json['detail'] as String?) ?? '',
      generation: (json['generation'] as num?)?.toInt() ?? 0,
    );
  }

  Map<String, Object> toJson() => {
        'id': id,
        'tool': tool,
        'summary': summary,
        'detail': detail,
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
