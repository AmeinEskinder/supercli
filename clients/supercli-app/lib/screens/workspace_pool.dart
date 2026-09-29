/// Background workspace pool: policy and state-machine logic.
///
/// Port of `WorkspacePool.swift`
/// (`clients/legacy/native/SupercliNative/Sources/SupercliNative/`).
///
/// The Swift `WorkspacePool` is a `@MainActor ObservableObject` that keeps
/// one live, read-only background connection per known workspace with a
/// cached latest bootstrap snapshot, so the swipe pager / footer dots /
/// selector can peek real sidebar content before a scope switch and scoping
/// into a pooled workspace renders instantly from the cached snapshot.
///
/// Only the portable policy is here: every pure function and every
/// synchronous state machine (reconciliation, remote-slot accounting,
/// attention-edge latches, optimistic organization holds, exponential
/// backoff). The async entry loops (`Task` poll loops, `CheckedContinuation`
/// wake/slot primitives, `@Published` publishing) are platform runtime
/// machinery with no portable equivalent and are intentionally not modeled:
/// the Dart host drives the same decisions through these functions.
///
/// Separation from the foreground is structural: pool connections are
/// bootstrap READS only — no terminal writes, fits, mark-reads, or
/// organization verbs — so the foreground runtime's generation-bound
/// non-replay semantics are untouched by construction. The workspace the
/// runtime currently serves is EXCLUDED from pooling; its snapshots are
/// mirrored in through [PoolStore.noteExternalSnapshot] instead, so exactly
/// one live connection per workspace exists at any time.
library;

/// Tunables mirroring `WorkspacePool`'s initializer defaults.
///
/// Kept as constants (rather than constructor parameters) because the Dart
/// host configures them once per app launch.
abstract final class WorkspacePoolPolicy {
  /// Low-cadence poll interval: ~25s. Mirrors `pollIntervalNanoseconds`.
  static const int pollIntervalMs = 25000;

  /// First backoff step for unreachable hosts. Mirrors
  /// `backoffBaseNanoseconds`.
  static const int backoffBaseMs = 5000;

  /// Backoff ceiling. Mirrors `backoffCapNanoseconds`.
  static const int backoffCapMs = 300000;

  /// Target-list reconcile cadence. Mirrors `maintenanceIntervalNanoseconds`.
  static const int maintenanceIntervalMs = 30000;

  /// Minimum gap between caller-initiated immediate refreshes, so
  /// hover-adjacent call sites cannot hammer remote hosts. Mirrors
  /// `immediateRefreshThrottleSeconds`.
  static const double immediateRefreshThrottleSeconds = 2;

  /// Cap on concurrent live remote (ssh/paired) connections. Local gateway
  /// children are cheap and stay always-on. Mirrors
  /// `maxLiveRemoteConnections` (clamped to >= 1 in Swift).
  static const int maxLiveRemoteConnections = 4;

  /// How long an optimistic reorder hold pins the cached snapshot over stale
  /// host bootstraps before host truth wins again. Mirrors
  /// `organizationHoldSeconds`.
  static const double organizationHoldSeconds = 15;
}

/// One pooled workspace. Mirrors `WorkspacePool.Target`.
///
/// The stable [key] IS the shared workspace-order key
/// (`WorkspaceListOrder.localKey/pairedKey/sshKey`), so views can join pool
/// state to row-model ids directly.
final class WorkspacePoolTarget {
  /// Creates a pooled workspace target.
  const WorkspacePoolTarget({
    required this.key,
    required this.name,
    required this.transportKind,
    required this.isRemote,
    this.expectedHostId,
    required this.fingerprint,
  });

  /// Stable workspace-order key.
  final String key;

  /// Display name.
  final String name;

  /// Transport discriminator: one of `local`, `ssh`, `direct`, `link`.
  /// Mirrors the `RemoteHostTransport` case without its payloads; the pool's
  /// portable logic only branches on [isRemote].
  final String transportKind;

  /// True for ssh/paired transports: subject to the concurrency cap and the
  /// reachability backoff slot release. Local gateways are cheap child
  /// processes and stay always-on.
  final bool isRemote;

  /// Expected host identity; a bootstrap whose host id differs retires the
  /// entry and latches the fingerprint (fail closed).
  final String? expectedHostId;

  /// Change detection token. A changed fingerprint retires and reopens the
  /// entry, exactly like the Swift `Target.fingerprint` comparison.
  final String fingerprint;

  @override
  bool operator ==(Object other) =>
      other is WorkspacePoolTarget &&
      key == other.key &&
      name == other.name &&
      transportKind == other.transportKind &&
      isRemote == other.isRemote &&
      expectedHostId == other.expectedHostId &&
      fingerprint == other.fingerprint;

  @override
  int get hashCode => Object.hash(
    key,
    name,
    transportKind,
    isRemote,
    expectedHostId,
    fingerprint,
  );
}

/// Minimal project projection the pool's pure logic reads.
///
/// Mirrors the `RemoteProjectSummary` fields consumed by
/// `applyingProjectOrder` / `applyingSessionOrder` / organization holds.
final class PooledProject {
  /// Creates a pooled project projection.
  const PooledProject({
    required this.id,
    this.parentProjectId,
    this.sessionOrder,
  });

  /// Project id.
  final String id;

  /// Parent project id; null for roots.
  final String? parentProjectId;

  /// Mixed session order (may contain child-group ids that must keep their
  /// slots when sessions reorder).
  final List<String>? sessionOrder;

  /// Returns a copy with [sessionOrder] replaced.
  PooledProject withSessionOrder(List<String> order) => PooledProject(
    id: id,
    parentProjectId: parentProjectId,
    sessionOrder: List<String>.unmodifiable(order),
  );

  @override
  bool operator ==(Object other) =>
      other is PooledProject &&
      id == other.id &&
      parentProjectId == other.parentProjectId &&
      _listEquals(sessionOrder, other.sessionOrder);

  @override
  int get hashCode => Object.hash(id, parentProjectId, sessionOrder);
}

/// Minimal session projection the pool's pure logic reads.
///
/// Mirrors the `RemoteSessionSummary` fields consumed by attention detection
/// and organization holds.
final class PooledSession {
  /// Creates a pooled session projection.
  const PooledSession({
    required this.id,
    required this.projectId,
    this.title = '',
    this.command = '',
    this.status = '',
    this.activity = '',
    this.archived = false,
  });

  /// Session id.
  final String id;

  /// Owning project id.
  final String projectId;

  /// Display title (falls back to [command] when empty).
  final String title;

  /// Raw launch command.
  final String command;

  /// Lifecycle status: `running` | `exited`.
  final String status;

  /// Activity: `starting` | `working` | `blocked` | `idle` | `done`.
  final String activity;

  /// Archived sessions never raise attention.
  final bool archived;

  /// Swift `acceptSnapshot` attention predicate: running, blocked, live.
  bool get needsAttention =>
      status == 'running' && activity == 'blocked' && !archived;

  /// Swift `acceptSnapshot` notification title: empty title falls back to
  /// the raw command.
  String get attentionTitle => title.isEmpty ? command : title;

  @override
  bool operator ==(Object other) =>
      other is PooledSession &&
      id == other.id &&
      projectId == other.projectId &&
      title == other.title &&
      command == other.command &&
      status == other.status &&
      activity == other.activity &&
      archived == other.archived;

  @override
  int get hashCode =>
      Object.hash(id, projectId, title, command, status, activity, archived);
}

/// Minimal bootstrap projection for the pool cache.
///
/// Mirrors the `RemoteBootstrapSnapshot` fields the pool reads. Full wire
/// decoding lives in `host_models.dart` (`BootstrapSnapshot`); this is the
/// normalized shape the pure pool functions operate on.
final class PooledSnapshot {
  /// Creates a pooled snapshot projection.
  const PooledSnapshot({
    this.projects = const [],
    this.sessions = const [],
    this.capturedAtUnixMs = 0,
  });

  /// Projects in bootstrap display order.
  final List<PooledProject> projects;

  /// Sessions in bootstrap display order.
  final List<PooledSession> sessions;

  /// Capture timestamp. Advances on every poll even when nothing changed;
  /// [isEquivalentTo] normalizes it away.
  final int capturedAtUnixMs;

  /// Returns a copy with [capturedAtUnixMs] replaced. Mirrors
  /// `withCapturedAt(_:)`.
  PooledSnapshot withCapturedAt(int capturedAtUnixMs) => PooledSnapshot(
    projects: projects,
    sessions: sessions,
    capturedAtUnixMs: capturedAtUnixMs,
  );

  /// Published-cache equivalence: full equality with the capture timestamp
  /// normalized away (it advances on every poll even when nothing else
  /// changed). Built on `==` over a re-stamped copy — never a field subset
  /// — so a future snapshot field can't silently fall out of the comparison
  /// and pin stale content in the cache. Mirrors `isEquivalent(to:)`.
  bool isEquivalentTo(PooledSnapshot other) =>
      withCapturedAt(other.capturedAtUnixMs) == other;

  /// Replace only the relative positions named by [orderedIds]; unknown or
  /// newly-created siblings keep their slots. Array order is the bootstrap
  /// display-order contract consumed by both live and pooled projections.
  /// Mirrors `applyingProjectOrder(parentID:orderedIDs:)`.
  PooledSnapshot applyingProjectOrder({
    String? parentId,
    required List<String> orderedIds,
  }) {
    final siblingIds = <String>{
      for (final p in projects)
        if (p.parentProjectId == parentId) p.id,
    };
    final preferred = orderedIds.where(siblingIds.contains).toList();
    final reordered = applyingRelativeOrder<String, PooledProject>(
      preferred,
      projects,
      (p) => p.id,
    );
    return PooledSnapshot(
      projects: reordered,
      sessions: sessions,
      capturedAtUnixMs: capturedAtUnixMs,
    );
  }

  /// Optimistically reorder one project's session summaries and its mixed
  /// `sessionOrder` field (when present), preserving child-folder slots.
  /// Mirrors `applyingSessionOrder(projectID:orderedIDs:)`.
  PooledSnapshot applyingSessionOrder({
    required String projectId,
    required List<String> orderedIds,
  }) {
    final memberIds = <String>{
      for (final s in sessions)
        if (s.projectId == projectId) s.id,
    };
    final preferred = orderedIds.where(memberIds.contains).toList();
    final reorderedSessions = applyingRelativeOrder<String, PooledSession>(
      preferred,
      sessions,
      (s) => s.id,
    );
    final reorderedProjects = [
      for (final p in projects)
        if (p.id == projectId && p.sessionOrder != null)
          p.withSessionOrder(
            applyingRelativeOrder<String, String>(
              preferred,
              p.sessionOrder!,
              (s) => s,
            ),
          )
        else
          p,
    ];
    return PooledSnapshot(
      projects: reorderedProjects,
      sessions: reorderedSessions,
      capturedAtUnixMs: capturedAtUnixMs,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is PooledSnapshot &&
      _listEquals(projects, other.projects) &&
      _listEquals(sessions, other.sessions) &&
      capturedAtUnixMs == other.capturedAtUnixMs;

  @override
  int get hashCode => Object.hash(projects, sessions, capturedAtUnixMs);
}

/// Replace only the relative positions named by [preferredIds]; values whose
/// ids are not preferred keep their slots. Unknown preferred ids are
/// ignored. Mirrors `RemoteBootstrapSnapshot.applyingRelativeOrder`.
List<T> applyingRelativeOrder<I, T>(
  List<I> preferredIds,
  List<T> values,
  I Function(T) idOf,
) {
  final byId = <I, T>{};
  for (final v in values) {
    byId[idOf(v)] = v;
  }
  final preferred = <T>[
    for (final id in preferredIds)
      if (byId.containsKey(id)) byId[id] as T,
  ];
  final replacingIds = preferred.map(idOf).toSet();
  final it = preferred.iterator;
  return [
    for (final v in values)
      replacingIds.contains(idOf(v)) ? (it.moveNext() ? it.current : v) : v,
  ];
}

/// Exponential backoff delay for an unreachable host: base doubling per
/// consecutive failure, exponent clamped to 16, result capped.
///
/// Mirrors `backoffDelayNanoseconds(_:)` (units converted to milliseconds).
/// `consecutiveFailures` is >= 1 at every real call site (the pool
/// increments before backing off).
int backoffDelayMs({
  required int consecutiveFailures,
  int baseMs = WorkspacePoolPolicy.backoffBaseMs,
  int capMs = WorkspacePoolPolicy.backoffCapMs,
}) {
  var exponent = consecutiveFailures - 1;
  if (exponent < 0) exponent = 0;
  if (exponent > 16) exponent = 16;
  final multiplier = 1 << exponent;
  final uncapped = baseMs * multiplier;
  // 64-bit overflow guard, mirroring `multipliedReportingOverflow`: on
  // overflow the delay is the cap.
  final delay = (multiplier != 0 && uncapped ~/ multiplier != baseMs)
      ? capMs
      : uncapped;
  return delay < capMs ? delay : capMs;
}

/// Per-(workspace, session) notification latch: a session notifies once per
/// blocked EDGE. Leaving blocked clears its latch; the first snapshot of a
/// workspace seeds silently so app launch never replays stale attention as
/// a banner storm. Mirrors `WorkspacePool.AttentionLatch`.
final class AttentionLatch {
  /// Creates an attention latch.
  const AttentionLatch({
    this.seeded = false,
    this.notifiedSessionIds = const <String>{},
  });

  /// True once the first snapshot has been observed.
  final bool seeded;

  /// Session ids currently latched as blocked (already notified or seeded).
  final Set<String> notifiedSessionIds;

  @override
  bool operator ==(Object other) =>
      other is AttentionLatch &&
      seeded == other.seeded &&
      notifiedSessionIds.length == other.notifiedSessionIds.length &&
      notifiedSessionIds.containsAll(other.notifiedSessionIds);

  @override
  int get hashCode => Object.hash(seeded, notifiedSessionIds);
}

/// Outcome of advancing one workspace's attention bookkeeping for an
/// accepted snapshot. Mirrors the latch/notification section of
/// `acceptSnapshot`.
final class AttentionAdvance {
  /// Creates an attention advance outcome.
  const AttentionAdvance({
    required this.latch,
    required this.notifySessionIds,
    required this.hasAttention,
  });

  /// Latch to store for the workspace.
  final AttentionLatch latch;

  /// Session ids to notify, in sorted order (deterministic). Empty when the
  /// workspace is unseeded or is the foreground workspace.
  final List<String> notifySessionIds;

  /// Whether the workspace currently carries any blocked live session
  /// (drives `attentionKeys`).
  final bool hasAttention;
}

/// Advances attention bookkeeping for one accepted snapshot.
///
/// [blockedIds] are the ids of live blocked sessions (running, blocked,
/// unarchived), mirroring the `blocked` set built in `acceptSnapshot`.
/// [isForeground] suppresses notifications for the scoped workspace: the
/// user is looking at it and its own per-session notification pipeline
/// already covers it.
///
/// - First contact seeds silently: stale attention at launch is a badge,
///   never a banner (`notifySessionIds` is empty, latch records [blockedIds]).
/// - Later polls notify exactly once per blocked edge: sessions that left
///   blocked re-arm; still-blocked stay latched.
AttentionAdvance advanceAttention({
  required AttentionLatch latch,
  required Set<String> blockedIds,
  required bool isForeground,
}) {
  if (!latch.seeded || isForeground) {
    return AttentionAdvance(
      latch: AttentionLatch(
        seeded: true,
        notifiedSessionIds: Set<String>.of(blockedIds),
      ),
      notifySessionIds: const [],
      hasAttention: blockedIds.isNotEmpty,
    );
  }
  final newlyBlocked = blockedIds.difference(latch.notifiedSessionIds).toList()
    ..sort();
  return AttentionAdvance(
    latch: AttentionLatch(
      seeded: true,
      notifiedSessionIds: Set<String>.of(blockedIds),
    ),
    notifySessionIds: newlyBlocked,
    hasAttention: blockedIds.isNotEmpty,
  );
}

/// A held optimistic project reorder for one workspace. Mirrors
/// `WorkspacePool.ProjectOrderHold`.
final class ProjectOrderHold {
  /// Creates a project order hold.
  const ProjectOrderHold({
    required this.parentId,
    required this.ids,
    required this.heldAtMs,
  });

  /// Parent whose children were reordered; null for roots.
  final String? parentId;

  /// Committed order to pin.
  final List<String> ids;

  /// Wall-clock hold time, milliseconds since epoch.
  final int heldAtMs;
}

/// A held optimistic session reorder for one project. Mirrors
/// `WorkspacePool.SessionOrderHold`.
final class SessionOrderHold {
  /// Creates a session order hold.
  const SessionOrderHold({required this.ids, required this.heldAtMs});

  /// Committed order to pin.
  final List<String> ids;

  /// Wall-clock hold time, milliseconds since epoch.
  final int heldAtMs;
}

/// Optimistic organization holds for one workspace, plus the pure
/// application function. Mirrors `projectOrderHolds` / `sessionOrderHolds`
/// and `applyingOrganizationHolds(to:key:)`.
final class OrganizationHolds {
  /// Creates an organization-holds record.
  const OrganizationHolds({this.projectHold, this.sessionHolds = const {}});

  /// Active project reorder hold, if any.
  final ProjectOrderHold? projectHold;

  /// Active session reorder holds, keyed by project id.
  final Map<String, SessionOrderHold> sessionHolds;

  /// Whether any hold is active.
  bool get isEmpty => projectHold == null && sessionHolds.isEmpty;

  /// Records a foreground project reorder and pins it over the cached
  /// snapshot immediately, so swiping away during the refresh window does
  /// not show the old "ghost" sidebar. Mirrors `holdProjectOrder`.
  ({OrganizationHolds holds, PooledSnapshot snapshot}) holdProjectOrder({
    required PooledSnapshot snapshot,
    String? parentId,
    required List<String> orderedIds,
    required int nowMs,
  }) {
    final next = OrganizationHolds(
      projectHold: ProjectOrderHold(
        parentId: parentId,
        ids: orderedIds,
        heldAtMs: nowMs,
      ),
      sessionHolds: sessionHolds,
    );
    final updated = snapshot.applyingProjectOrder(
      parentId: parentId,
      orderedIds: orderedIds,
    );
    return (holds: next, snapshot: updated);
  }

  /// Records a foreground session reorder and pins it over the cached
  /// snapshot immediately. Keyed per project because independent lists may
  /// be reordered before either confirming poll. Mirrors `holdSessionOrder`.
  ({OrganizationHolds holds, PooledSnapshot snapshot}) holdSessionOrder({
    required PooledSnapshot snapshot,
    required String projectId,
    required List<String> orderedIds,
    required int nowMs,
  }) {
    final nextHolds = Map<String, SessionOrderHold>.of(sessionHolds)
      ..[projectId] = SessionOrderHold(ids: orderedIds, heldAtMs: nowMs);
    final next = OrganizationHolds(
      projectHold: projectHold,
      sessionHolds: nextHolds,
    );
    final updated = snapshot.applyingSessionOrder(
      projectId: projectId,
      orderedIds: orderedIds,
    );
    return (holds: next, snapshot: updated);
  }

  /// Projects an incoming bootstrap through the active holds: a foreground
  /// reorder stays visible until the host confirms the same relative order
  /// or the bounded timeout releases host truth again. Mirrors
  /// `applyingOrganizationHolds(to:key:)`.
  ///
  /// Returns the projected snapshot and the surviving holds.
  ({PooledSnapshot snapshot, OrganizationHolds holds}) applyTo({
    required PooledSnapshot incoming,
    required int nowMs,
  }) {
    var snapshot = incoming;
    ProjectOrderHold? projectHold = this.projectHold;
    final sessionHolds = Map<String, SessionOrderHold>.of(this.sessionHolds);

    final holdSecondsMs = (WorkspacePoolPolicy.organizationHoldSeconds * 1000)
        .round();

    if (projectHold != null) {
      final memberIds = <String>{
        for (final p in incoming.projects)
          if (p.parentProjectId == projectHold.parentId) p.id,
      };
      final expected = projectHold.ids.where(memberIds.contains).toList();
      final expectedSet = expected.toSet();
      final natural = [
        for (final p in incoming.projects)
          if (expectedSet.contains(p.id)) p.id,
      ];
      if (_listEquals(natural, expected) ||
          nowMs - projectHold.heldAtMs > holdSecondsMs) {
        projectHold = null;
      } else {
        snapshot = snapshot.applyingProjectOrder(
          parentId: projectHold.parentId,
          orderedIds: projectHold.ids,
        );
      }
    }

    for (final entry in List.of(sessionHolds.entries)) {
      final projectId = entry.key;
      final hold = entry.value;
      final memberIds = <String>{
        for (final s in incoming.sessions)
          if (s.projectId == projectId) s.id,
      };
      final expected = hold.ids.where(memberIds.contains).toList();
      final expectedSet = expected.toSet();
      final natural = [
        for (final s in incoming.sessions)
          if (expectedSet.contains(s.id)) s.id,
      ];
      if (_listEquals(natural, expected) ||
          nowMs - hold.heldAtMs > holdSecondsMs) {
        sessionHolds.remove(projectId);
      } else {
        snapshot = snapshot.applyingSessionOrder(
          projectId: projectId,
          orderedIds: hold.ids,
        );
      }
    }

    return (
      snapshot: snapshot,
      holds: OrganizationHolds(
        projectHold: projectHold,
        sessionHolds: sessionHolds,
      ),
    );
  }
}

/// Outcome of one target-list reconciliation. Mirrors `refreshTargets()`.
final class PoolReconcileResult {
  /// Creates a reconciliation outcome.
  const PoolReconcileResult({
    required this.retireKeys,
    required this.startTargets,
    required this.dropCacheKeys,
  });

  /// Live entries to retire: vanished targets, runtime-excluded keys, and
  /// targets whose fingerprint changed. Mirrors the retire pass.
  final List<String> retireKeys;

  /// Targets to start polling: known, not excluded, not identity-latched,
  /// and without a live entry after the retire pass.
  final List<WorkspacePoolTarget> startTargets;

  /// Cached keys to drop entirely: the workspace is no longer known at all
  /// (a merely excluded — runtime-served — key keeps its cache for
  /// peek/seed). Mirrors the cache-drop pass.
  final List<String> dropCacheKeys;
}

/// Re-reads the known-workspace list and converges entries on it, exactly
/// like `refreshTargets()`:
///
/// - an entry whose target vanished, is runtime-excluded, or whose
///   fingerprint changed is retired;
/// - a workspace that is no longer known at all loses its cache (an
///   excluded one keeps it for peek/seed);
/// - remaining targets start polling unless excluded or identity-latched.
///
/// [entryFingerprints] maps live entry keys to their target fingerprints.
PoolReconcileResult reconcilePoolTargets({
  required Map<String, String> entryFingerprints,
  required List<WorkspacePoolTarget> targets,
  required Set<String> excludedKeys,
  required Set<String> failedIdentityFingerprints,
  required Set<String> cachedKeys,
}) {
  final byKey = {for (final t in targets) t.key: t};

  final retireKeys = <String>[];
  for (final entry in entryFingerprints.entries) {
    final target = byKey[entry.key];
    if (target == null ||
        excludedKeys.contains(entry.key) ||
        target.fingerprint != entry.value) {
      retireKeys.add(entry.key);
    }
  }
  final liveKeys = entryFingerprints.keys.toSet()..removeAll(retireKeys);

  final dropCacheKeys = <String>[
    for (final key in cachedKeys)
      if (!byKey.containsKey(key) && !excludedKeys.contains(key)) key,
  ];

  final startTargets = <WorkspacePoolTarget>[];
  for (final target in targets) {
    if (liveKeys.contains(target.key)) continue;
    if (excludedKeys.contains(target.key)) continue;
    if (failedIdentityFingerprints.contains(target.fingerprint)) continue;
    startTargets.add(target);
  }

  return PoolReconcileResult(
    retireKeys: retireKeys,
    startTargets: startTargets,
    dropCacheKeys: dropCacheKeys,
  );
}

/// Synchronous remote-connection slot accounting. Mirrors
/// `acquireRemoteSlot` / `releaseRemoteSlot` / `grantNextSlot`.
///
/// In Swift the waiter path suspends on a `CheckedContinuation`; here the
/// waiter simply queues and [tryAcquire] reports whether the slot was
/// granted immediately. The caller re-invokes [tryAcquire] when a slot frees
/// (the pool's immediate-refresh wake), which is the same grant order:
///
/// - a failing host releases its slot during backoff so one dead host never
///   starves a live one;
/// - waiters are granted FIFO; a waiter whose entry retired is dropped
///   without a slot (its loop's currency guard would exit immediately).
final class RemoteSlotPool {
  /// Creates a slot pool with [maxSlots] concurrent remote connections.
  RemoteSlotPool({required int maxSlots})
    : maxSlots = maxSlots < 1 ? 1 : maxSlots;

  /// Maximum concurrent remote connections (clamped to >= 1, like Swift).
  final int maxSlots;

  /// Currently granted slots.
  int get slotsInUse => _holders.length;
  final Set<String> _holders = {};

  /// FIFO waiter queue.
  List<String> get waiters => List<String>.unmodifiable(_waiters);
  final List<String> _waiters = [];

  /// Whether [key] currently holds a slot.
  bool holdsSlot(String key) => _holders.contains(key);

  /// Acquires a slot for [key]. Returns true when the slot was granted
  /// immediately; false when [key] was queued behind the cap.
  bool tryAcquire(String key) {
    if (_holders.contains(key)) return true;
    if (_holders.length < maxSlots) {
      _holders.add(key);
      return true;
    }
    if (!_waiters.contains(key)) _waiters.add(key);
    return false;
  }

  /// Releases [key]'s slot and grants the next waiter FIFO.
  ///
  /// Returns the waiter key that was granted a slot, or null when no waiter
  /// was waiting. Waiters not in [validKeys] are dropped without a slot
  /// (retired entries never resume into a slot they no longer own).
  String? release(String key, {Set<String> validKeys = const {}}) {
    if (!_holders.remove(key)) return null;
    while (_holders.length < maxSlots && _waiters.isNotEmpty) {
      final next = _waiters.removeAt(0);
      if (validKeys.isEmpty || validKeys.contains(next)) {
        _holders.add(next);
        return next;
      }
      // Stale waiter: resumes without a slot; its loop exits on the
      // currency guard. Kept out of _holders by construction.
    }
    return null;
  }

  /// Drops [key] from the waiter queue without granting (entry retired
  /// while waiting). Mirrors the retire path resuming the slot waiter.
  void cancelWaiter(String key) {
    _waiters.remove(key);
  }
}

/// Result of accepting one polled bootstrap into the pool cache. Mirrors
/// `acceptSnapshot(_:key:name:)` minus `@Published` publishing.
final class PoolAcceptResult {
  /// Creates a snapshot-acceptance outcome.
  const PoolAcceptResult({
    required this.snapshot,
    required this.published,
    required this.attention,
    required this.holds,
  });

  /// Snapshot to cache (post-hold projection).
  final PooledSnapshot snapshot;

  /// False when the incoming snapshot is equivalent to the cached one: an
  /// unchanged workspace must not wake observers every poll. Mirrors the
  /// early return in `acceptSnapshot`.
  final bool published;

  /// Attention bookkeeping for the accepted snapshot.
  final AttentionAdvance attention;

  /// Surviving organization holds after projection: a host-confirming
  /// snapshot releases the holds it confirms, exactly like the side effect
  /// inside Swift's `applyingOrganizationHolds(to:key:)`. Thread this into
  /// the next call.
  final OrganizationHolds holds;
}

/// Accepts one polled bootstrap for [key], exactly like
/// `acceptSnapshot(_:key:name:)`:
///
/// 1. projects the incoming snapshot through [holds] (a foreground reorder
///    stays visible before the host's confirming bootstrap arrives);
/// 2. when the projected snapshot is equivalent to [cached] (everything but
///    the capture timestamp), it is NOT republished — the cached snapshot
///    and its timestamp-keyed derived caches stay put, and latch/attention
///    bookkeeping is a no-op (identical content cannot change it);
/// 3. otherwise the projected snapshot replaces the cache and attention
///    advances (blocked-edge detection with per-session dedup).
///
/// [holds] is the workspace's current organization-holds record; the
/// returned [PoolAcceptResult.holds] must be threaded into the next call,
/// because a host-confirming snapshot releases the holds it confirms.
///
/// Contact bookkeeping (`lastSeenAt`) advances on every poll including
/// equivalent ones; it is deliberately not modeled here because nothing
/// renders from it (publishing it would defeat the unchanged-snapshot skip).
PoolAcceptResult acceptPooledSnapshot({
  required PooledSnapshot? cached,
  required PooledSnapshot incoming,
  required OrganizationHolds holds,
  required AttentionLatch latch,
  required bool isForeground,
  required int nowMs,
}) {
  final projection = holds.applyTo(incoming: incoming, nowMs: nowMs);
  final projected = projection.snapshot;
  if (cached != null && cached.isEquivalentTo(projected)) {
    return PoolAcceptResult(
      snapshot: cached,
      published: false,
      attention: AttentionAdvance(
        latch: latch,
        notifySessionIds: const [],
        hasAttention: latch.notifiedSessionIds.isNotEmpty,
      ),
      holds: projection.holds,
    );
  }
  final blockedIds = <String>{
    for (final s in projected.sessions)
      if (s.needsAttention) s.id,
  };
  final attention = advanceAttention(
    latch: latch,
    blockedIds: blockedIds,
    isForeground: isForeground,
  );
  return PoolAcceptResult(
    snapshot: projected,
    published: true,
    attention: attention,
    holds: projection.holds,
  );
}

/// Identity check for a freshly bootstrapped connection. Mirrors the
/// identity gate in `runEntryLoop`: a host whose reported id differs from
/// the saved one fails closed — never keep reading it — and the fingerprint
/// latches so the target is not re-polled until the record itself changes.
///
/// Returns the fingerprint to latch, or null when the identity matches.
String? identityMismatchFingerprint({
  required String? expectedHostId,
  required String reportedHostId,
  required String fingerprint,
}) {
  if (expectedHostId != null && reportedHostId != expectedHostId) {
    return fingerprint;
  }
  return null;
}

bool _listEquals<T>(List<T>? a, List<T>? b) {
  if (identical(a, b)) return true;
  if (a == null || b == null || a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}
