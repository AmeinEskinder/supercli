/// Archived sessions browser.
///
/// Port of `ArchivedSessionsView.swift`.
library;

import 'package:gpuidart/gpuidart.dart';

import '../models.dart';

final class ArchivedSessionsView {
  const ArchivedSessionsView({this.sessions = const []});

  final List<SessionSummary> sessions;

  UiNode build() {
    return UiColumn('archived-sessions', [
      const UiText('archived-title', 'Archived Sessions'),
      const UiInput('archived-filter', placeholder: 'Search archived…'),
      UiTable('archived-table', dataset: 'archived-sessions'),
      UiRow('archived-actions', [
        const UiButton('archived-restore', 'Restore'),
        const UiButton('archived-delete', 'Delete'),
      ]),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'archived-sessions',
        columns: const ['Title', 'Archived'],
        rows: sessions
            .map((s) => [s.title, s.updatedAt.toIso8601String().substring(0, 10)])
            .toList(),
      );
}
