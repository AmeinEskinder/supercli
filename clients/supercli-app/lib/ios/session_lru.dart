/// Pure LRU bookkeeping for the per-session terminal cache, plus the
/// token-gated lease trackers.
///
/// Port of the UI-agnostic structs in `TerminalSessionCache.swift`
/// (`clients/legacy/ios/SupercliIOS`): `SessionLRUIndex`,
/// `TerminalVisibilityLeaseTracker`, `TerminalStreamLeaseTracker`.
///
/// The `@MainActor` cache + entry themselves (ghostty surfaces, UIKit views,
/// memory-warning observer) are platform-bound and stay out — what ports is
/// the ordering/eviction/prune logic, generic over the payload so it is
/// unit-testable without ever touching renderers.
library;

/// One entry evicted from (or removed from) the index.
final class LruEviction<E> {
  const LruEviction(this.id, this.entry);
  final String id;
  final E entry;

  @override
  bool operator ==(Object other) =>
      other is LruEviction<E> && other.id == id && other.entry == entry;

  @override
  int get hashCode => Object.hash(id, entry);
}

/// Pure LRU bookkeeping, generic over the payload. Least-recently-used
/// first, most-recently-used last.
final class SessionLruIndex<E> {
  SessionLruIndex({required int capacity})
    : capacity = capacity < 1 ? 1 : capacity;

  final int capacity;

  /// Least-recently-used first, most-recently-used last.
  final List<String> _order = <String>[];
  final Map<String, E> _storage = <String, E>{};

  int get count => _order.length;
  List<String> get keys => List<String>.unmodifiable(_order);

  /// Returns the entry and marks it most-recently-used.
  E? lookup(String id) {
    final entry = _storage[id];
    if (entry == null) return null;
    _touch(id);
    return entry;
  }

  /// Reads without touching recency (for inspection/iteration).
  E? peek(String id) => _storage[id];

  /// Inserts (or replaces) an entry as most-recently-used. Returns the
  /// entries evicted to stay within capacity — never the one just inserted.
  List<LruEviction<E>> insert(E entry, {required String id}) {
    _storage[id] = entry;
    _touch(id);
    final evicted = <LruEviction<E>>[];
    while (_order.length > capacity) {
      final oldest = _order.removeAt(0);
      final dropped = _storage.remove(oldest);
      if (dropped != null) evicted.add(LruEviction<E>(oldest, dropped));
    }
    return evicted;
  }

  E? remove(String id) {
    _order.removeWhere((k) => k == id);
    return _storage.remove(id);
  }

  List<LruEviction<E>> removeAll() {
    final removed = <LruEviction<E>>[];
    for (final id in _order) {
      final entry = _storage[id];
      if (entry != null) removed.add(LruEviction<E>(id, entry));
    }
    _order.clear();
    _storage.clear();
    return removed;
  }

  /// Drops every entry whose id is not in [ids] (session killed on the
  /// Mac), except [keeping] (the on-screen session must not be torn down
  /// under a transiently stale/empty session list).
  List<LruEviction<E>> retainOnly(Set<String> ids, {String? keeping}) {
    final doomed = _order
        .where((id) => id != keeping && !ids.contains(id))
        .toList();
    final removed = <LruEviction<E>>[];
    for (final id in doomed) {
      final entry = remove(id);
      if (entry != null) removed.add(LruEviction<E>(id, entry));
    }
    return removed;
  }

  /// Drops everything except [id] (memory pressure: keep only the visible
  /// session's terminal).
  List<LruEviction<E>> removeAllExcept(String? id) {
    final doomed = _order.where((k) => k != id).toList();
    final removed = <LruEviction<E>>[];
    for (final k in doomed) {
      final entry = remove(k);
      if (entry != null) removed.add(LruEviction<E>(k, entry));
    }
    return removed;
  }

  void _touch(String id) {
    _order.removeWhere((k) => k == id);
    _order.add(id);
  }
}

/// Token-gated ownership for the one terminal currently on screen. The UI
/// may mount a replacement before dismantling the previous view. The token
/// distinguishes those two lifetimes even when they render the same session
/// (notably a connection-epoch remount), so the old disappear cannot hide
/// the replacement from prune/memory-pressure protection.
///
/// Port of `TerminalVisibilityLeaseTracker`.
final class TerminalVisibilityLeaseTracker {
  String? _sessionID;
  Object? _owner;

  String? get sessionID => _sessionID;

  void acquire({required String sessionID, required Object owner}) {
    _sessionID = sessionID;
    _owner = owner;
  }

  /// Returns true only when the releasing owner actually holds the lease.
  bool release({required String sessionID, required Object owner}) {
    if (_sessionID != sessionID || !identical(_owner, owner)) return false;
    _sessionID = null;
    _owner = null;
    return true;
  }
}

/// Reference-counts renderer streaming by mounted view lifetime. A Set
/// makes repeated appear callbacks idempotent. More importantly, a late
/// disappear from an old mount only releases its own token; it cannot
/// stop a renderer already acquired by its replacement.
///
/// Port of `TerminalStreamLeaseTracker`.
final class TerminalStreamLeaseTracker {
  final Set<Object> _owners = <Object>{};

  bool get isEmpty => _owners.isEmpty;

  /// True only for the transition that should start the renderer.
  bool acquire(Object owner) {
    final inserted = _owners.add(owner);
    return inserted && _owners.length == 1;
  }

  /// True only for the transition that should stop the renderer.
  bool release(Object owner) {
    if (!_owners.remove(owner)) return false;
    return _owners.isEmpty;
  }

  void removeAll() => _owners.clear();
}
