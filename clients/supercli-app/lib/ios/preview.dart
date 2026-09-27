/// UI-agnostic state logic from the iOS preview store.
///
/// Port of the portable parts of `RemotePreviewStore.swift`
/// (`clients/legacy/ios/SupercliIOS`):
/// - Host capability gates (`supportsResumableArtifactUpload`,
///   `supportsSessionReorder`);
/// - the MCP approval ID bookkeeping (answered/revealed sets, filtering,
///   pruning, per-session presentation resolution);
/// - workspace resolution (current workspace from the advertised list).
///
/// The `@MainActor @Observable` class itself is SwiftUI-bound and stays out.
/// Everything here is side-effect-free and unit-tested.
///
/// Wire capability names mirror `RemoteControlProtocol` in the legacy
/// shared Swift package (`artifact.upload.resumable`, `session.order.set`).
library;

/// Host capability for resumable artifact upload.
const String kResumableArtifactUploadCapability = 'artifact.upload.resumable';

/// Host capability for hold-to-reorder of sidebar sessions.
const String kSessionOrderCapability = 'session.order.set';

/// Minimal host-protocol descriptor for capability gating (mirrors the
/// fields `RemotePreviewStore` reads off its `snapshot.hostProtocol`).
final class HostProtocolDescriptor {
  const HostProtocolDescriptor({
    required this.isCompatible,
    required this.capabilities,
  });

  final bool isCompatible;
  final Set<String> capabilities;

  bool supports(String capability) => capabilities.contains(capability);
}

/// Whether this Host supports capability-gated resumable image upload.
/// A missing descriptor is a legacy Host and must keep using the shipped
/// one-shot upload route.
///
/// Port of `RemotePreviewStore.supportsResumableArtifactUpload`.
bool supportsResumableArtifactUpload(HostProtocolDescriptor? descriptor) {
  if (descriptor == null) return false;
  return descriptor.isCompatible &&
      descriptor.supports(kResumableArtifactUploadCapability);
}

/// Whether this Host supports capability-gated hold-to-reorder for sidebar
/// sessions. A missing descriptor is a legacy Host with no session-order
/// route.
///
/// Port of `RemotePreviewStore.supportsSessionReorder`.
bool supportsSessionReorder(HostProtocolDescriptor? descriptor) {
  if (descriptor == null) return false;
  return descriptor.isCompatible &&
      descriptor.supports(kSessionOrderCapability);
}

/// Minimal approval summary for the tracker (mirrors the fields
/// `RemotePreviewStore` reads off `RemotePendingApproval`).
final class ApprovalSummary {
  const ApprovalSummary({
    required this.id,
    required this.callerSessionId,
    this.targetSessionId,
  });

  final String id;
  final String callerSessionId;
  final String? targetSessionId;

  @override
  bool operator ==(Object other) =>
      other is ApprovalSummary &&
      other.id == id &&
      other.callerSessionId == callerSessionId &&
      other.targetSessionId == targetSessionId;

  @override
  int get hashCode => Object.hash(id, callerSessionId, targetSessionId);
}

/// Session that should show the in-pane prompt and the attention badge.
/// Write grants present on the destination so the user sees where input
/// would land; other kinds have no destination and present on the caller.
/// A missing/unknown destination falls back to the caller.
///
/// Port of `RemotePendingApproval.presentationSessionID(knownIDs:)`.
String presentationSessionId({
  String? targetSessionId,
  required String callerSessionId,
  required Set<String> knownIds,
}) {
  if (targetSessionId != null && knownIds.contains(targetSessionId)) {
    return targetSessionId;
  }
  return callerSessionId;
}

/// Tracks which MCP approval prompts this client has already answered
/// or revealed, so polling never re-shows them.
///
/// Port of the `answeredApprovalIDs` / `revealedApprovalIDs` bookkeeping in
/// `RemotePreviewStore`.
final class ApprovalTracker {
  /// Approval ids answered from this client. The answer POST wins the race
  /// against the next bootstrap poll, so answered prompts hide immediately.
  final Set<String> _answered = <String>{};

  /// Approval ids this client has already opened the presentation
  /// session for. New ids reveal once; a later poll must not yank the user
  /// back if they navigated away while the prompt is still pending.
  final Set<String> _revealed = <String>{};

  /// Record an answer. Returns true if this id was newly answered.
  bool markAnswered(String id) => _answered.add(id);

  /// Record a reveal. Returns true if this id was newly revealed.
  bool markRevealed(String id) => _revealed.add(id);

  bool isAnswered(String id) => _answered.contains(id);
  bool wasRevealed(String id) => _revealed.contains(id);

  /// IDs waiting on the Host, minus ones already answered from this client.
  ///
  /// Port of `RemotePreviewStore.pendingApprovals`.
  List<ApprovalSummary> pending(List<ApprovalSummary> advertised) {
    return advertised.where((a) => !_answered.contains(a.id)).toList();
  }

  /// Prune answered ids once the Host's bootstrap stops reporting them.
  /// Returns the number of ids pruned.
  int pruneAnswered(Set<String> advertisedIds) {
    final before = _answered.length;
    _answered.retainWhere(advertisedIds.contains);
    return before - _answered.length;
  }

  /// Does [sessionId] need MCP approval attention (badge / in-pane prompt)?
  bool sessionNeedsAttention({
    required String sessionId,
    required List<ApprovalSummary> advertised,
    required Set<String> knownIds,
  }) {
    return pending(advertised).any(
      (a) =>
          presentationSessionId(
            targetSessionId: a.targetSessionId,
            callerSessionId: a.callerSessionId,
            knownIds: knownIds,
          ) ==
          sessionId,
    );
  }

  /// First pending approval presenting on [sessionId], if any.
  ApprovalSummary? pendingApprovalFor({
    required String sessionId,
    required List<ApprovalSummary> advertised,
    required Set<String> knownIds,
  }) {
    for (final a in pending(advertised)) {
      if (presentationSessionId(
            targetSessionId: a.targetSessionId,
            callerSessionId: a.callerSessionId,
            knownIds: knownIds,
          ) ==
          sessionId) {
        return a;
      }
    }
    return null;
  }

  /// Number of pending approvals presenting on [sessionId].
  int pendingApprovalCount({
    required String sessionId,
    required List<ApprovalSummary> advertised,
    required Set<String> knownIds,
  }) {
    return pending(advertised)
        .where(
          (a) =>
              presentationSessionId(
                targetSessionId: a.targetSessionId,
                callerSessionId: a.callerSessionId,
                knownIds: knownIds,
              ) ==
              sessionId,
        )
        .length;
  }
}

/// Minimal workspace summary for resolution (mirrors the fields
/// `RemotePreviewStore` reads off `RemoteWorkspaceSummary`).
final class WorkspaceSummary {
  const WorkspaceSummary({
    required this.id,
    required this.name,
    required this.isCurrent,
  });

  final String id;
  final String name;
  final bool isCurrent;

  @override
  bool operator ==(Object other) =>
      other is WorkspaceSummary &&
      other.id == id &&
      other.name == name &&
      other.isCurrent == isCurrent;

  @override
  int get hashCode => Object.hash(id, name, isCurrent);
}

/// The workspace currently being served over this connection — the Host's
/// own workspace by default, or the one this device switched to. Resolved
/// from the Host's authoritative `isCurrent` flag in the latest bootstrap.
///
/// Port of `RemotePreviewStore.currentWorkspace`.
WorkspaceSummary? currentWorkspace(List<WorkspaceSummary> workspaces) {
  for (final w in workspaces) {
    if (w.isCurrent) return w;
  }
  return workspaces.isEmpty ? null : workspaces.first;
}

/// Whether the connected Host advertises more than one local workspace,
/// so a workspace picker is worth showing.
///
/// Port of `RemotePreviewStore.hasMultipleWorkspaces`.
bool hasMultipleWorkspaces(List<WorkspaceSummary> workspaces) =>
    workspaces.length > 1;
