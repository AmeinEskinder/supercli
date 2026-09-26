/// Startup presentation cache for instant sidebar.
///
/// Port of `sidebarskeleton.swift`. Covers checklist row 219:
/// "Startup presentation cache for instant sidebar".
///
/// On quit, the app snapshots the sidebar presentation (pinned order,
/// group expansion, selected session) to JSON. On launch, the cached
/// snapshot renders instantly (skeleton with real labels) while the
/// live Host bootstrap loads in the background, then the live data
/// replaces it. A stale cache (older than [maxAge]) is ignored.
library;

import 'dart:convert';

/// How long a cached sidebar snapshot is trusted.
const Duration sidebarCacheMaxAge = Duration(hours: 24);

/// Serializable snapshot of the sidebar presentation.
final class SidebarSnapshot {
  const SidebarSnapshot({
    this.pinnedSessionIds = const [],
    this.expandedGroupIds = const [],
    this.selectedSessionId,
    this.capturedAt,
  });

  final List<String> pinnedSessionIds;
  final List<String> expandedGroupIds;
  final String? selectedSessionId;
  final DateTime? capturedAt;

  Map<String, Object?> toJson() => {
        'pinnedSessionIds': pinnedSessionIds,
        'expandedGroupIds': expandedGroupIds,
        'selectedSessionId': selectedSessionId,
        'capturedAt': capturedAt?.toIso8601String(),
      };

  factory SidebarSnapshot.fromJson(Map<String, dynamic> json) =>
      SidebarSnapshot(
        pinnedSessionIds:
            (json['pinnedSessionIds'] as List?)?.cast<String>() ?? const [],
        expandedGroupIds:
            (json['expandedGroupIds'] as List?)?.cast<String>() ?? const [],
        selectedSessionId: json['selectedSessionId'] as String?,
        capturedAt: json['capturedAt'] == null
            ? null
            : DateTime.tryParse(json['capturedAt'] as String),
      );

  /// True if the snapshot is missing or older than [maxAge].
  bool isStale({Duration maxAge = sidebarCacheMaxAge, DateTime? now}) {
    final at = capturedAt;
    if (at == null) return true;
    return (now ?? DateTime.now()).difference(at) > maxAge;
  }
}

/// Presentation cache: serialize on quit, restore on launch.
final class SidebarPresentationCache {
  SidebarPresentationCache({this.maxAge = sidebarCacheMaxAge});

  final Duration maxAge;

  SidebarSnapshot? _cached;

  /// Snapshot to render instantly at startup, or null when there is
  /// no usable cache (first launch or stale).
  SidebarSnapshot? restore(String? json, {DateTime? now}) {
    if (json == null || json.isEmpty) return null;
    try {
      final snap =
          SidebarSnapshot.fromJson(jsonDecode(json) as Map<String, dynamic>);
      if (snap.isStale(maxAge: maxAge, now: now)) return null;
      _cached = snap;
      return snap;
    } on FormatException {
      return null;
    }
  }

  /// Serialize the current presentation for the next launch.
  String capture(SidebarSnapshot snapshot, {DateTime? now}) {
    final stamped = SidebarSnapshot(
      pinnedSessionIds: snapshot.pinnedSessionIds,
      expandedGroupIds: snapshot.expandedGroupIds,
      selectedSessionId: snapshot.selectedSessionId,
      capturedAt: now ?? DateTime.now(),
    );
    _cached = stamped;
    return jsonEncode(stamped.toJson());
  }

  SidebarSnapshot? get cached => _cached;
}
