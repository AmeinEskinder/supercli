/// Mobile pane-group projection: summaries and scope IDs for mobile clients.
/// Port of `MobilePaneGroupProjection` from
/// `native/SupercliNative/Sources/SupercliNative/RemoteDTOAdapters.swift`.
library;

import '../pane_layout_state.dart';

/// A summary of a multi-session pane group, as advertised to mobile clients.
/// Port of `RemotePaneGroupSummary` from
/// `shared/SupercliShared/Sources/SupercliShared/RemoteControlProtocol.swift`.
final class RemotePaneGroupSummary {
  const RemotePaneGroupSummary({
    required this.id,
    required this.representativeSessionID,
    required this.sessionIDs,
  });

  final String id;
  final String representativeSessionID;
  final List<String> sessionIDs;

  @override
  bool operator ==(Object other) =>
      other is RemotePaneGroupSummary &&
      other.id == id &&
      other.representativeSessionID == representativeSessionID &&
      _listEquals(other.sessionIDs, sessionIDs);

  @override
  int get hashCode =>
      Object.hash(id, representativeSessionID, Object.hashAll(sessionIDs));

  @override
  String toString() =>
      'RemotePaneGroupSummary(id: $id, representative: $representativeSessionID, sessions: $sessionIDs)';
}

bool _listEquals(List<String> a, List<String> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

/// Projects pane-layout state into mobile-consumable group summaries and
/// maps mobile workspace selection keys to controller pane scopes.
/// Port of `MobilePaneGroupProjection` (all static) from
/// `native/SupercliNative/Sources/SupercliNative/RemoteDTOAdapters.swift`.
abstract final class MobilePaneGroupProjection {
  /// Returns one summary per group that has at least two sessions and a
  /// representative session that is actually in the group, or `null` when
  /// no group qualifies (including an empty layout).
  static List<RemotePaneGroupSummary>? summaries(LayoutPaneLayoutState state) {
    final result = <RemotePaneGroupSummary>[];
    for (final group in state.groups) {
      final sessionIDs = group.sessionIDs;
      final representative = group.representativeSessionID;
      if (sessionIDs.length >= 2 &&
          representative != null &&
          sessionIDs.contains(representative)) {
        result.add(RemotePaneGroupSummary(
          id: group.id,
          representativeSessionID: representative,
          sessionIDs: sessionIDs,
        ));
      }
    }
    return result.isEmpty ? null : result;
  }

  /// Maps a mobile workspace mux selection key to a controller pane scope ID.
  /// `null` (no selection) means the local machine. Returns `null` for
  /// malformed keys (empty home/host, unknown scheme).
  static String? scopeIDForSelectionKey(String? selectionKey) {
    if (selectionKey == null) return 'local';
    if (selectionKey.startsWith('local:')) {
      final home = selectionKey.substring('local:'.length);
      return home.isEmpty ? null : 'workspace:$home';
    }
    if (selectionKey.startsWith('ssh:')) {
      final hostID = selectionKey.substring('ssh:'.length);
      return hostID.isEmpty ? null : 'host:$hostID';
    }
    if (selectionKey.startsWith('host:')) {
      return selectionKey.length > 'host:'.length ? selectionKey : null;
    }
    return null;
  }
}
