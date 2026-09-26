/// Archived sessions browser: search, restore, delete.
///
/// Port of `ArchivedSessionsView.swift`.
library;

import 'package:gpuidart/gpuidart.dart';

import '../models.dart';

final class ArchivedSessionsView {
  const ArchivedSessionsView(
      {this.sessions = const [], this.filterText = ''});

  final List<SessionSummary> sessions;
  final String filterText;

  List<SessionSummary> get visible => filterText.isEmpty
      ? sessions
      : sessions
          .where((s) =>
              s.title.toLowerCase().contains(filterText.toLowerCase()))
          .toList();

  UiNode build() {
    return UiColumn('archived-sessions', [
      UiText('archived-title', 'Archived Sessions (${sessions.length})',
          style: const UiStyle(
              fontSize: 13, fontWeight: UiFontWeight.semibold)),
      const UiInput('archived-filter', placeholder: 'Search archived…'),
      if (visible.isEmpty)
        const UiText('archived-empty', 'Nothing archived.',
            style: UiStyle(fontSize: 12))
      else
        for (final s in visible)
          UiRow('archived-${s.id}', [
            UiText('archived-name-${s.id}', s.title,
                style: const UiStyle(fontSize: 12)),
            UiText('archived-date-${s.id}',
                s.updatedAt.toIso8601String().substring(0, 10),
                style: const UiStyle(fontSize: 11)),
            UiButton('archived-restore-${s.id}', 'Restore'),
          ], style: const UiStyle(gap: 8)),
      UiRow('archived-actions', [
        const UiButton('archived-delete-selected', 'Delete Selected'),
        const UiButton('archived-close', 'Close'),
      ], style: const UiStyle(gap: 8)),
    ]);
  }
}
