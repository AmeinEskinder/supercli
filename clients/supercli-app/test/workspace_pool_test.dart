/// Port of `WorkspacePoolTests.swift`
/// (`clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/`).
///
/// Background workspace pool (workspaces-unification phase 7): poll caching,
/// unreachable-host backoff, the remote concurrency cap, attention edge
/// detection + notification dedup, and the lend/adopt handoff that guarantees
/// the pool never keeps a live connection to a runtime-served workspace.
///
/// The Swift tests drive real async entry loops (`Task` poll loops,
/// `CheckedContinuation` wakes); those have no portable Dart equivalent and
/// are not modeled. Every test below targets the portable kernel in
/// `workspace_pool.dart`: the same pure functions and synchronous state
/// machines the async loops are built from, with the same assertions the
/// XCTest cases make about their decisions.
library;

import 'package:supercli_app/screens/workspace_pool.dart';
import 'package:test/test.dart';

PooledSession session(
  String id, {
  String projectId = 'project',
  String status = 'running',
  String activity = 'idle',
  bool archived = false,
  String? title,
  String command = 'claude',
}) => PooledSession(
  id: id,
  projectId: projectId,
  title: title ?? id,
  command: command,
  status: status,
  activity: activity,
  archived: archived,
);

PooledSnapshot snapshot({
  List<PooledSession> sessions = const [],
  List<PooledProject> projects = const [],
  int capturedAtUnixMs = 1000,
}) => PooledSnapshot(
  sessions: sessions,
  projects: projects,
  capturedAtUnixMs: capturedAtUnixMs,
);

WorkspacePoolTarget target(String key, {bool remote = false}) =>
    WorkspacePoolTarget(
      key: key,
      name: 'Workspace $key',
      transportKind: remote ? 'ssh' : 'local',
      isRemote: remote,
      fingerprint: 'fp:$key',
    );

void main() {
  group('WorkspacePool (WorkspacePool.swift)', () {
    group('poll caching', () {
      test('accepting a snapshot caches it and publishes', () {
        final incoming = snapshot(
          sessions: [session('s1', activity: 'blocked')],
        );
        final result = acceptPooledSnapshot(
          cached: null,
          incoming: incoming,
          holds: const OrganizationHolds(),
          latch: const AttentionLatch(),
          isForeground: false,
          nowMs: 2000,
        );
        expect(result.published, isTrue);
        expect(result.snapshot.sessions.map((s) => s.id), ['s1']);
        expect(result.attention.hasAttention, isTrue);
      });

      test('equivalent snapshot polls never republish', () {
        // Mirrors testEquivalentSnapshotPollsNeverRepublish: identical
        // content with a fresh capture timestamp — exactly what an idle
        // host's poll returns forever — must not wake observers.
        final cached = snapshot(
          sessions: [session('s1')],
          capturedAtUnixMs: 1000,
        );
        final incoming = snapshot(
          sessions: [session('s1')],
          capturedAtUnixMs: 2000,
        );
        final result = acceptPooledSnapshot(
          cached: cached,
          incoming: incoming,
          holds: const OrganizationHolds(),
          latch: const AttentionLatch(
            seeded: true,
            notifiedSessionIds: <String>{},
          ),
          isForeground: false,
          nowMs: 3000,
        );
        expect(
          result.published,
          isFalse,
          reason: 'unchanged workspace must never republish',
        );
        expect(
          identical(result.snapshot, cached),
          isTrue,
          reason: 'the cached snapshot (and its timestamp-keyed derived caches) stays put',
        );
        expect(result.attention.notifySessionIds, isEmpty);

        // Real content change publishes again.
        final changed = acceptPooledSnapshot(
          cached: cached,
          incoming: snapshot(
            sessions: [session('s1', activity: 'blocked')],
            capturedAtUnixMs: 3000,
          ),
          holds: const OrganizationHolds(),
          latch: const AttentionLatch(
            seeded: true,
            notifiedSessionIds: <String>{},
          ),
          isForeground: false,
          nowMs: 4000,
        );
        expect(changed.published, isTrue);
        expect(changed.attention.hasAttention, isTrue);
      });

      test('equivalence normalizes only the capture timestamp', () {
        final a = snapshot(sessions: [session('s1')], capturedAtUnixMs: 1);
        final b = snapshot(sessions: [session('s1')], capturedAtUnixMs: 999);
        expect(a.isEquivalentTo(b), isTrue);
        final c = snapshot(
          sessions: [session('s1', activity: 'blocked')],
          capturedAtUnixMs: 1,
        );
        expect(a.isEquivalentTo(c), isFalse);
      });
    });

    group('backoff', () {
      test('unreachable host backs off exponentially and recovers', () {
        // Mirrors testUnreachableHostBacksOffExponentiallyAndRecovers with
        // the test's 30ms base / 240ms cap: ~30ms then ~120ms, doubling.
        final delays = [
          for (var failures = 1; failures <= 5; failures++)
            backoffDelayMs(
              consecutiveFailures: failures,
              baseMs: 30,
              capMs: 240,
            ),
        ];
        expect(delays, [30, 60, 120, 240, 240]);
        expect(
          delays[2] > delays[0] * 1.5,
          isTrue,
          reason: 'backoff grows faster than linear',
        );
      });

      test('exponent clamps at 16 and result caps', () {
        expect(
          backoffDelayMs(consecutiveFailures: 100),
          WorkspacePoolPolicy.backoffCapMs,
        );
        expect(
          backoffDelayMs(consecutiveFailures: 17),
          backoffDelayMs(consecutiveFailures: 100),
          reason: 'exponent clamped at 16',
        );
        expect(
          backoffDelayMs(consecutiveFailures: 1),
          WorkspacePoolPolicy.backoffBaseMs,
        );
      });
    });

    group('remote concurrency cap', () {
      test('remote connections are capped and slots recycle', () {
        // Mirrors testRemoteConnectionsAreCappedAndSlotsRecycle with
        // maxRemote 2: the third remote host waits for a slot.
        final slots = RemoteSlotPool(maxSlots: 2);
        expect(slots.tryAcquire('a'), isTrue);
        expect(slots.tryAcquire('b'), isTrue);
        expect(
          slots.tryAcquire('c'),
          isFalse,
          reason: 'third remote host must wait for a slot',
        );
        expect(slots.waiters, ['c']);

        // A failing host releases its slot during backoff; the waiter runs.
        final granted = slots.release('a');
        expect(granted, 'c');
        expect(slots.holdsSlot('c'), isTrue);
        expect(slots.holdsSlot('a'), isFalse);
      });

      test('local targets are not subject to the remote cap', () {
        // Mirrors runEntryLoop's `entry.target.isRemote` guard: only remote
        // targets acquire from the slot pool. Local gateway children are
        // cheap child processes that stay always-on and never queue behind
        // the cap.
        final slots = RemoteSlotPool(maxSlots: 1);
        bool acquireIfRemote(WorkspacePoolTarget t) =>
            t.isRemote ? slots.tryAcquire(t.key) : true;
        expect(
          slots.tryAcquire('ssh-b'),
          isTrue,
          reason: 'one remote holds the only slot',
        );
        expect(
          acquireIfRemote(target('local-a')),
          isTrue,
          reason: 'local targets never queue behind the remote cap',
        );
        expect(
          acquireIfRemote(target('ssh-c', remote: true)),
          isFalse,
          reason: 'a second remote waits for the slot',
        );
      });

      test('retired waiter resumes without a slot', () {
        final slots = RemoteSlotPool(maxSlots: 1);
        expect(slots.tryAcquire('a'), isTrue);
        expect(slots.tryAcquire('b'), isFalse);
        slots.cancelWaiter('b');
        expect(slots.release('a'), isNull);
        expect(slots.holdsSlot('a'), isFalse);
      });

      test('max slots clamps to at least one', () {
        expect(RemoteSlotPool(maxSlots: 0).maxSlots, 1);
      });
    });

    group('attention edges and notification dedup', () {
      test('attention edge notifies once per rise', () {
        // Mirrors testAttentionEdgeNotifiesOncePerRise.
        var latch = const AttentionLatch();

        // First contact seeds silently.
        var advance = advanceAttention(
          latch: latch,
          blockedIds: {'s1'},
          isForeground: false,
        );
        latch = advance.latch;
        expect(advance.notifySessionIds, isEmpty);
        expect(advance.hasAttention, isTrue);

        // Rise: blocked after idle → exactly one notification.
        advance = advanceAttention(
          latch: const AttentionLatch(),
          blockedIds: const <String>{},
          isForeground: false,
        );
        advance = advanceAttention(
          latch: advance.latch,
          blockedIds: {'s1'},
          isForeground: false,
        );
        expect(advance.notifySessionIds, ['s1']);

        // Repeated blocked snapshots stay deduplicated.
        advance = advanceAttention(
          latch: advance.latch,
          blockedIds: {'s1'},
          isForeground: false,
        );
        expect(advance.notifySessionIds, isEmpty);

        // Falling edge clears the badge and re-arms the latch.
        advance = advanceAttention(
          latch: advance.latch,
          blockedIds: const <String>{},
          isForeground: false,
        );
        expect(advance.hasAttention, isFalse);
        advance = advanceAttention(
          latch: advance.latch,
          blockedIds: {'s1'},
          isForeground: false,
        );
        expect(advance.notifySessionIds, ['s1']);
      });

      test('already-blocked first contact seeds badge without notification', () {
        // Mirrors testAlreadyBlockedFirstContactSeedsBadgeWithoutNotification:
        // stale attention at first contact is a badge, never a banner.
        final advance = advanceAttention(
          latch: const AttentionLatch(),
          blockedIds: {'s1'},
          isForeground: false,
        );
        expect(
          advance.notifySessionIds,
          isEmpty,
          reason: 'stale attention at first contact is a badge, never a banner',
        );
        expect(advance.hasAttention, isTrue);
        expect(advance.latch.seeded, isTrue);
        expect(advance.latch.notifiedSessionIds, {'s1'});
      });

      test('foreground workspace never notifies', () {
        // Mirrors testForegroundWorkspaceNeverNotifies.
        final advance = advanceAttention(
          latch: const AttentionLatch(
            seeded: true,
            notifiedSessionIds: <String>{},
          ),
          blockedIds: {'s1'},
          isForeground: true,
        );
        expect(advance.notifySessionIds, isEmpty);
        expect(advance.hasAttention, isTrue);
        // The foreground latch still records blocked, so leaving the scope
        // does not replay it as new.
        expect(advance.latch.notifiedSessionIds, {'s1'});
      });

      test('archived sessions never raise attention', () {
        expect(
          session('s1', activity: 'blocked', archived: true).needsAttention,
          isFalse,
        );
        expect(session('s1', activity: 'blocked').needsAttention, isTrue);
        expect(session('s1', activity: 'idle').needsAttention, isFalse);
        expect(
          session('s1', activity: 'blocked', status: 'exited').needsAttention,
          isFalse,
        );
      });

      test('empty title falls back to command for notification text', () {
        expect(
          session('s1', title: '', command: 'claude').attentionTitle,
          'claude',
        );
        expect(session('s1', title: 'My title').attentionTitle, 'My title');
      });
    });

    group('reconciliation and lend handoff', () {
      test('lend retires the pool entry but keeps the cache', () {
        // Mirrors testLendRetiresPoolConnectionAndNeverDuplicatesWhileExcluded:
        // scope entry retires the pool's read-only connection; while the
        // runtime serves the key it is never re-polled, and the cache
        // remains for peeks.
        final result = reconcilePoolTargets(
          entryFingerprints: {'a': 'fp:a'},
          targets: [target('a')],
          excludedKeys: {'a'},
          failedIdentityFingerprints: const <String>{},
          cachedKeys: {'a'},
        );
        expect(result.retireKeys, ['a']);
        expect(
          result.dropCacheKeys,
          isEmpty,
          reason: 'excluded (runtime-served) keys keep their cache',
        );
        expect(
          result.startTargets,
          isEmpty,
          reason: 'lent workspace must not be re-polled',
        );

        // The runtime lets go: pooling resumes on the next reconcile.
        final resumed = reconcilePoolTargets(
          entryFingerprints: const {},
          targets: [target('a')],
          excludedKeys: const <String>{},
          failedIdentityFingerprints: const <String>{},
          cachedKeys: {'a'},
        );
        expect(resumed.startTargets.map((t) => t.key), ['a']);
      });

      test('forgotten workspace drops cache and connection', () {
        // Mirrors testForgottenWorkspaceDropsCacheAndConnection.
        final result = reconcilePoolTargets(
          entryFingerprints: {'a': 'fp:a'},
          targets: const [],
          excludedKeys: const <String>{},
          failedIdentityFingerprints: const <String>{},
          cachedKeys: {'a'},
        );
        expect(result.retireKeys, ['a']);
        expect(result.dropCacheKeys, ['a']);
      });

      test('changed fingerprint retires and reopens the entry', () {
        final result = reconcilePoolTargets(
          entryFingerprints: {'a': 'fp:old'},
          targets: [
            WorkspacePoolTarget(
              key: 'a',
              name: 'Workspace a',
              transportKind: 'local',
              isRemote: false,
              fingerprint: 'fp:new',
            ),
          ],
          excludedKeys: const <String>{},
          failedIdentityFingerprints: const <String>{},
          cachedKeys: {'a'},
        );
        expect(result.retireKeys, ['a']);
        expect(result.startTargets.map((t) => t.key), ['a']);
        expect(result.dropCacheKeys, isEmpty);
      });

      test('identity-latched fingerprint is not re-polled', () {
        // Mirrors the identity fail-closed gate: the fingerprint latch stops
        // re-opens until the record itself changes.
        final result = reconcilePoolTargets(
          entryFingerprints: const {},
          targets: [target('a')],
          excludedKeys: const <String>{},
          failedIdentityFingerprints: {'fp:a'},
          cachedKeys: const <String>{},
        );
        expect(result.startTargets, isEmpty);
      });

      test('identity mismatch latches the fingerprint', () {
        expect(
          identityMismatchFingerprint(
            expectedHostId: 'h1',
            reportedHostId: 'h2',
            fingerprint: 'fp:a',
          ),
          'fp:a',
        );
        expect(
          identityMismatchFingerprint(
            expectedHostId: 'h1',
            reportedHostId: 'h1',
            fingerprint: 'fp:a',
          ),
          isNull,
        );
        expect(
          identityMismatchFingerprint(
            expectedHostId: null,
            reportedHostId: 'h2',
            fingerprint: 'fp:a',
          ),
          isNull,
        );
      });
    });

    group('optimistic organization holds', () {
      PooledSnapshot organizationSnapshot({int capturedAtUnixMs = 1000}) {
        return PooledSnapshot(
          projects: [
            const PooledProject(
              id: 'p1',
              sessionOrder: ['g1', 's1', 's2', 's3'],
            ),
            const PooledProject(id: 'g1', parentProjectId: 'p1'),
            const PooledProject(id: 'p2'),
            const PooledProject(id: 'p3'),
          ],
          sessions: [
            session('s1', projectId: 'p1'),
            session('s2', projectId: 'p1'),
            session('s3', projectId: 'p1'),
          ],
          capturedAtUnixMs: capturedAtUnixMs,
        );
      }

      List<String> rootOrder(PooledSnapshot s) => [
        for (final p in s.projects)
          if (p.parentProjectId == null) p.id,
      ];

      test('holds keep committed order until the host confirms', () {
        // Mirrors
        // testOrganizationHoldsKeepCarouselSnapshotInCommittedOrderUntilHostConfirms.
        final initial = organizationSnapshot();
        var accepted = acceptPooledSnapshot(
          cached: null,
          incoming: initial,
          holds: const OrganizationHolds(),
          latch: const AttentionLatch(),
          isForeground: false,
          nowMs: 1000,
        );
        var cached = accepted.snapshot;
        var holds = const OrganizationHolds();

        var held = holds.holdProjectOrder(
          snapshot: cached,
          parentId: null,
          orderedIds: ['p3', 'p1', 'p2'],
          nowMs: 1100,
        );
        holds = held.holds;
        cached = held.snapshot;
        held = holds.holdSessionOrder(
          snapshot: cached,
          projectId: 'p1',
          orderedIds: ['s3', 's1', 's2'],
          nowMs: 1100,
        );
        holds = held.holds;
        cached = held.snapshot;

        expect(rootOrder(cached), ['p3', 'p1', 'p2']);
        expect(cached.sessions.map((s) => s.id), ['s3', 's1', 's2']);
        expect(cached.projects.firstWhere((p) => p.id == 'p1').sessionOrder, [
          'g1',
          's3',
          's1',
          's2',
        ], reason: 'the child-group slot must not move with session ranks');

        // A stale bootstrap arriving right after the effect must not roll
        // the carousel's ghost page back to its pre-drop order.
        accepted = acceptPooledSnapshot(
          cached: cached,
          incoming: organizationSnapshot(capturedAtUnixMs: 2000),
          holds: holds,
          latch: const AttentionLatch(seeded: true),
          isForeground: false,
          nowMs: 1200,
        );
        holds = accepted.holds;
        expect(
          holds.isEmpty,
          isFalse,
          reason: 'stale host truth must not release the holds',
        );
        expect(rootOrder(accepted.snapshot), ['p3', 'p1', 'p2']);
        expect(accepted.snapshot.sessions.map((s) => s.id), ['s3', 's1', 's2']);

        // A naturally matching bootstrap releases both holds; a later host
        // order is then authoritative again rather than pinned forever.
        final confirmed = PooledSnapshot(
          projects: [
            const PooledProject(id: 'p3'),
            const PooledProject(
              id: 'p1',
              sessionOrder: ['g1', 's3', 's1', 's2'],
            ),
            const PooledProject(id: 'g1', parentProjectId: 'p1'),
            const PooledProject(id: 'p2'),
          ],
          sessions: [
            session('s3', projectId: 'p1'),
            session('s1', projectId: 'p1'),
            session('s2', projectId: 'p1'),
          ],
          capturedAtUnixMs: 3000,
        );
        accepted = acceptPooledSnapshot(
          cached: accepted.snapshot,
          incoming: confirmed,
          holds: holds,
          latch: const AttentionLatch(seeded: true),
          isForeground: false,
          nowMs: 1300,
        );
        holds = accepted.holds;
        expect(
          holds.isEmpty,
          isTrue,
          reason: 'a host-confirming bootstrap releases both holds',
        );
        expect(rootOrder(accepted.snapshot), [
          'p3',
          'p1',
          'p2',
        ], reason: 'the confirming snapshot publishes as-is');
        expect(accepted.snapshot.sessions.map((s) => s.id), ['s3', 's1', 's2']);

        // Holds were released: the follow-up stale bootstrap is NOT
        // re-projected through them — host truth wins again.
        final stale = acceptPooledSnapshot(
          cached: accepted.snapshot,
          incoming: organizationSnapshot(capturedAtUnixMs: 4000),
          holds: holds,
          latch: const AttentionLatch(seeded: true),
          isForeground: false,
          nowMs: 1400,
        );
        expect(rootOrder(stale.snapshot), [
          'p1',
          'p2',
          'p3',
        ], reason: 'released holds never pin the old order again');
        expect(stale.snapshot.sessions.map((s) => s.id), ['s1', 's2', 's3']);
      });

      test('holds expire after the bounded timeout', () {
        var holds = const OrganizationHolds();
        final cached = organizationSnapshot();
        final held = holds.holdProjectOrder(
          snapshot: cached,
          parentId: null,
          orderedIds: ['p3', 'p1', 'p2'],
          nowMs: 0,
        );
        holds = held.holds;
        final stale = organizationSnapshot(capturedAtUnixMs: 9999);
        final applied = holds.applyTo(
          incoming: stale,
          nowMs:
              (WorkspacePoolPolicy.organizationHoldSeconds * 1000).round() + 1,
        );
        expect(rootOrder(applied.snapshot), [
          'p1',
          'p2',
          'p3',
        ], reason: 'expired hold releases host truth again');
        expect(applied.holds.projectHold, isNull);
      });

      test('unknown ids in a hold are ignored, siblings keep slots', () {
        const snap = PooledSnapshot(
          projects: [
            PooledProject(id: 'p1'),
            PooledProject(id: 'p2'),
            PooledProject(id: 'p3'),
          ],
        );
        final reordered = snap.applyingProjectOrder(
          orderedIds: ['p3', 'ghost', 'p1'],
        );
        expect(reordered.projects.map((p) => p.id), [
          'p3',
          'p2',
          'p1',
        ], reason: 'only the named positions are re-ranked; p2 keeps its slot');
      });
    });

    group('policy constants', () {
      test('defaults mirror the Swift initializer', () {
        expect(WorkspacePoolPolicy.pollIntervalMs, 25000);
        expect(WorkspacePoolPolicy.backoffBaseMs, 5000);
        expect(WorkspacePoolPolicy.backoffCapMs, 300000);
        expect(WorkspacePoolPolicy.maintenanceIntervalMs, 30000);
        expect(WorkspacePoolPolicy.immediateRefreshThrottleSeconds, 2);
        expect(WorkspacePoolPolicy.maxLiveRemoteConnections, 4);
        expect(WorkspacePoolPolicy.organizationHoldSeconds, 15);
      });
    });
  });
}
