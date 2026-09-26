/// Recent activity feed: sessions and tasks across workspaces with
/// relative timestamps.
///
/// Port of `RecentActivityView.swift`.
library;

import 'package:gpuidart/gpuidart.dart';

final class ActivityItem {
  const ActivityItem({
    required this.id,
    required this.title,
    required this.timestamp,
    this.kind = ActivityKind.session,
  });

  final String id;
  final String title;
  final DateTime timestamp;
  final ActivityKind kind;

  /// Human relative time: "just now", "5m", "2h", "3d".
  static String relativeTime(DateTime ts, {DateTime? now}) {
    final diff = (now ?? DateTime.now()).difference(ts);
    if (diff.inMinutes < 1) return 'just now';
    if (diff.inMinutes < 60) return '${diff.inMinutes}m';
    if (diff.inHours < 24) return '${diff.inHours}h';
    return '${diff.inDays}d';
  }
}

enum ActivityKind {
  session('💬'),
  approval('✋'),
  task('⚙️'),
  notification('🔔');

  const ActivityKind(this.glyph);
  final String glyph;
}

final class RecentActivityView {
  const RecentActivityView({this.items = const []});

  final List<ActivityItem> items;

  UiNode build() {
    return UiColumn('recent-activity', [
      const UiText('activity-title', 'Recent Activity',
          style: UiStyle(fontSize: 13, fontWeight: UiFontWeight.semibold)),
      if (items.isEmpty)
        const UiText('activity-empty', 'No recent activity.',
            style: UiStyle(fontSize: 12))
      else
        for (final i in items)
          UiRow('activity-${i.id}', [
            UiText('activity-glyph-${i.id}', i.kind.glyph,
                style: const UiStyle(fontSize: 12)),
            UiText('activity-title-${i.id}', i.title,
                style: const UiStyle(fontSize: 12)),
            UiText('activity-when-${i.id}',
                ActivityItem.relativeTime(i.timestamp),
                style: const UiStyle(fontSize: 11)),
          ], style: const UiStyle(gap: 8)),
    ]);
  }
}
