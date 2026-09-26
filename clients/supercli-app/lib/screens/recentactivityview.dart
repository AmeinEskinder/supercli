/// Recent activity feed.
///
/// Port of `RecentActivityView.swift`.
library;

import 'package:gpuidart/gpuidart.dart';

final class ActivityItem {
  const ActivityItem({
    required this.id,
    required this.title,
    required this.timestamp,
  });

  final String id;
  final String title;
  final DateTime timestamp;
}

final class RecentActivityView {
  const RecentActivityView({this.items = const []});

  final List<ActivityItem> items;

  UiNode build() {
    return UiColumn('recent-activity', [
      const UiText('activity-title', 'Recent Activity'),
      UiTable('activity-table', dataset: 'recent-activity'),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'recent-activity',
        columns: const ['Activity', 'When'],
        rows: items.map((i) => [i.title, '']).toList(),
      );
}
