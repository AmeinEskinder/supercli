/// Global activity menu: menu-bar-extra style activity list.
///
/// Port of `GlobalActivityMenu.swift`. Shows active sessions/tasks across
/// all workspaces in a dropdown menu.
library;

import 'package:gpuidart/gpuidart.dart';

import '../models.dart';

final class GlobalActivityMenu {
  const GlobalActivityMenu({this.sessions = const []});

  final List<SessionSummary> sessions;

  UiNode build() {
    return UiColumn('global-activity-menu', [
      const UiText('activity-menu-title', 'Activity'),
      if (sessions.isEmpty)
        const UiText('activity-menu-empty', 'No active sessions.')
      else
        for (final s in sessions)
          UiButton('activity-${s.id}', s.title),
    ]);
  }
}
