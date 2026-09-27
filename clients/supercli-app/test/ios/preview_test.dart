/// Tests for the preview-store logic: capability gates, approval
/// bookkeeping, and workspace resolution (port of the portable parts of
/// RemotePreviewStore.swift).
library;

import 'package:supercli_app/ios/preview.dart';
import 'package:test/test.dart';

const _descriptor = HostProtocolDescriptor(
  isCompatible: true,
  capabilities: {kResumableArtifactUploadCapability, kSessionOrderCapability},
);

List<ApprovalSummary> approvals() => const [
  ApprovalSummary(id: 'a1', callerSessionId: 's1', targetSessionId: 's-write'),
  ApprovalSummary(id: 'a2', callerSessionId: 's2'),
  ApprovalSummary(
    id: 'a3',
    callerSessionId: 's1',
    targetSessionId: 's-unknown',
  ),
];

void main() {
  group('capability gates', () {
    test('missing descriptor disables both gates', () {
      expect(supportsResumableArtifactUpload(null), isFalse);
      expect(supportsSessionReorder(null), isFalse);
    });

    test('incompatible host disables both gates', () {
      const descriptor = HostProtocolDescriptor(
        isCompatible: false,
        capabilities: {
          kResumableArtifactUploadCapability,
          kSessionOrderCapability,
        },
      );
      expect(supportsResumableArtifactUpload(descriptor), isFalse);
      expect(supportsSessionReorder(descriptor), isFalse);
    });

    test('gates follow advertised capabilities', () {
      expect(supportsResumableArtifactUpload(_descriptor), isTrue);
      expect(supportsSessionReorder(_descriptor), isTrue);
      const partial = HostProtocolDescriptor(
        isCompatible: true,
        capabilities: {kSessionOrderCapability},
      );
      expect(supportsResumableArtifactUpload(partial), isFalse);
      expect(supportsSessionReorder(partial), isTrue);
    });
  });

  group('presentationSessionId', () {
    test('prefers known target, falls back to caller', () {
      expect(
        presentationSessionId(
          targetSessionId: 's-write',
          callerSessionId: 's1',
          knownIds: {'s-write', 's1'},
        ),
        's-write',
      );
      // Unknown target falls back to the caller.
      expect(
        presentationSessionId(
          targetSessionId: 's-gone',
          callerSessionId: 's1',
          knownIds: {'s1'},
        ),
        's1',
      );
      // No target: the caller.
      expect(
        presentationSessionId(callerSessionId: 's2', knownIds: {'s2'}),
        's2',
      );
    });
  });

  group('ApprovalTracker', () {
    test('pending hides answered approvals', () {
      final tracker = ApprovalTracker();
      expect(tracker.markAnswered('a1'), isTrue);
      expect(tracker.markAnswered('a1'), isFalse); // already answered
      final pending = tracker.pending(approvals());
      expect(pending.map((a) => a.id), ['a2', 'a3']);
    });

    test('pruneAnswered drops ids the host stopped reporting', () {
      final tracker = ApprovalTracker();
      tracker.markAnswered('a1');
      tracker.markAnswered('stale');
      final pruned = tracker.pruneAnswered({'a1', 'a2', 'a3'});
      expect(pruned, 1);
      expect(tracker.isAnswered('a1'), isTrue);
      expect(tracker.isAnswered('stale'), isFalse);
    });

    test('reveal tracking', () {
      final tracker = ApprovalTracker();
      expect(tracker.markRevealed('a1'), isTrue);
      expect(tracker.markRevealed('a1'), isFalse);
      expect(tracker.wasRevealed('a1'), isTrue);
      expect(tracker.wasRevealed('a2'), isFalse);
    });

    test('session attention and lookup use presentation session', () {
      final tracker = ApprovalTracker();
      const knownIds = {'s1', 's2', 's-write'};
      // a1 presents on s-write (known target); a2 on s2 (caller); a3's
      // target is unknown so it presents on its caller s1.
      expect(
        tracker.sessionNeedsAttention(
          sessionId: 's-write',
          advertised: approvals(),
          knownIds: knownIds,
        ),
        isTrue,
      );
      expect(
        tracker.sessionNeedsAttention(
          sessionId: 's2',
          advertised: approvals(),
          knownIds: knownIds,
        ),
        isTrue,
      );
      expect(
        tracker.sessionNeedsAttention(
          sessionId: 's1',
          advertised: approvals(),
          knownIds: knownIds,
        ),
        isTrue,
      );
      expect(
        tracker.sessionNeedsAttention(
          sessionId: 's9',
          advertised: approvals(),
          knownIds: knownIds,
        ),
        isFalse,
      );

      final forWrite = tracker.pendingApprovalFor(
        sessionId: 's-write',
        advertised: approvals(),
        knownIds: knownIds,
      );
      expect(forWrite?.id, 'a1');
      expect(
        tracker.pendingApprovalFor(
          sessionId: 's9',
          advertised: approvals(),
          knownIds: knownIds,
        ),
        isNull,
      );
      expect(
        tracker.pendingApprovalCount(
          sessionId: 's1',
          advertised: approvals(),
          knownIds: knownIds,
        ),
        1,
      );
    });

    test('answered approvals need no attention', () {
      final tracker = ApprovalTracker();
      tracker.markAnswered('a1');
      const knownIds = {'s1', 's2', 's-write'};
      expect(
        tracker.sessionNeedsAttention(
          sessionId: 's-write',
          advertised: approvals(),
          knownIds: knownIds,
        ),
        isFalse,
      );
    });
  });

  group('workspace resolution', () {
    test('currentWorkspace prefers isCurrent, falls back to first', () {
      const workspaces = [
        WorkspaceSummary(id: 'w1', name: 'one', isCurrent: false),
        WorkspaceSummary(id: 'w2', name: 'two', isCurrent: true),
      ];
      expect(currentWorkspace(workspaces)?.id, 'w2');

      const legacy = [
        WorkspaceSummary(id: 'w1', name: 'one', isCurrent: false),
      ];
      expect(currentWorkspace(legacy)?.id, 'w1');
      expect(currentWorkspace(const []), isNull);
    });

    test('hasMultipleWorkspaces', () {
      expect(
        hasMultipleWorkspaces(const [
          WorkspaceSummary(id: 'w1', name: 'one', isCurrent: true),
        ]),
        isFalse,
      );
      expect(
        hasMultipleWorkspaces(const [
          WorkspaceSummary(id: 'w1', name: 'one', isCurrent: true),
          WorkspaceSummary(id: 'w2', name: 'two', isCurrent: false),
        ]),
        isTrue,
      );
    });
  });
}
