/// Global activity menu: menu-bar-extra style activity list.
///
/// Port of `GlobalActivityMenu.swift`. Shows active sessions/tasks across
/// all workspaces in a dropdown menu. Busy sessions get a spinner glyph,
/// attention sessions a red dot.
library;

import 'package:gpuidart/gpuidart.dart';

import 'sidebarview.dart';

final class GlobalActivityMenu {
  const GlobalActivityMenu({this.sessions = const []});

  final List<SidebarSession> sessions;

  /// Only sessions that are busy or need attention.
  List<SidebarSession> get active =>
      sessions.where((s) => s.busy || s.attention).toList();

  UiNode build() {
    return UiColumn('global-activity-menu', [
      const UiText('activity-menu-title', 'Activity',
          style: UiStyle(fontSize: 13, fontWeight: UiFontWeight.semibold)),
      if (active.isEmpty)
        const UiText('activity-menu-empty', 'No active sessions.',
            style: UiStyle(fontSize: 12))
      else
        for (final s in active)
          UiRow('gact-${s.id}', [
            UiText('gact-glyph-${s.id}',
                s.attention ? '●' : (s.busy ? '◌' : '·'),
                style: const UiStyle(fontSize: 11)),
            UiButton('activity-${s.id}', s.title),
          ], style: const UiStyle(gap: 6)),
    ]);
  }
}
