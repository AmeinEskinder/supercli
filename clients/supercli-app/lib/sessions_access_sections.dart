/// Sessions access sections data logic.
/// 
/// Port of the portable data logic from `SessionsAccessSections.swift`.
/// The SwiftUI View itself is not portable to Dart; only the approved
/// pairs/apps computations are ported.
library;

/// An approved session pair (caller can write to target without asking).
final class ApprovedPair {
  const ApprovedPair({
    required this.id,
    required this.caller,
    required this.target,
  });

  final String id;
  final String caller;
  final String target;
}

/// An approved app launch (caller can start the app without asking).
final class ApprovedApp {
  const ApprovedApp({
    required this.id,
    required this.caller,
    required this.appID,
  });

  final String id;
  final String caller;
  final String appID;
}

/// Computes the approved pairs and apps from the store's approval maps.
abstract final class SessionsAccessData {
  /// Flattens the caller→targets map into a sorted list of approved pairs.
  static List<ApprovedPair> approvedPairs(Map<String, List<String>> approvals) {
    final pairs = <ApprovedPair>[];
    for (final entry in approvals.entries) {
      final caller = entry.key;
      for (final target in entry.value) {
        pairs.add(ApprovedPair(
          id: '$caller→$target',
          caller: caller,
          target: target,
        ));
      }
    }
    pairs.sort((a, b) => a.id.compareTo(b.id));
    return pairs;
  }

  /// Flattens the caller→appIDs map into a sorted list of approved apps.
  static List<ApprovedApp> approvedApps(Map<String, List<String>> approvals) {
    final apps = <ApprovedApp>[];
    for (final entry in approvals.entries) {
      final caller = entry.key;
      for (final appID in entry.value) {
        apps.add(ApprovedApp(
          id: '$caller→$appID',
          caller: caller,
          appID: appID,
        ));
      }
    }
    apps.sort((a, b) => a.id.compareTo(b.id));
    return apps;
  }
}
