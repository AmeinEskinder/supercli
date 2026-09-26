/// Session sidebar: list of sessions with search, groups, and drag-to-reorder.
///
/// Port of `SidebarView.swift` (SupercliNative/Views, 4406 lines). The Swift
/// version has: session rows with presence avatars, workspace groups,
/// drag-and-drop reordering ("Dia feel"), context menus, unread badges,
/// and a filter field. The gpuidart port uses UiTable with a dataset.
///
/// GAP (P0-2): No UiList primitive. Using UiTable with TableDataset as
/// stopgap — no per-row tap without table_selection events, no drag reorder.
library;

import 'package:gpuidart/gpuidart.dart';

import '../models.dart';

/// A sidebar section (e.g. "Pinned", workspace group, "Archived").
final class SidebarSection {
  const SidebarSection({
    required this.id,
    required this.title,
    required this.sessions,
    this.collapsed = false,
  });

  final String id;
  final String title;
  final List<SessionSummary> sessions;
  final bool collapsed;
}

/// The session sidebar.
final class SidebarView {
  SidebarView({
    this.sections = const [],
    this.filterText = '',
    this.selectedSessionId,
  });

  final List<SidebarSection> sections;
  final String filterText;
  final String? selectedSessionId;

  UiNode build() {
    final children = <UiNode>[
      const UiInput('sidebar-filter', placeholder: 'Filter sessions…'),
      const UiButton('new-session', '+ New Session'),
    ];
    for (final section in sections) {
      children.add(UiText('section-${section.id}', section.title));
      if (!section.collapsed) {
        children.add(UiTable('sidebar-table-${section.id}',
            dataset: 'sidebar-${section.id}'));
      }
    }
    return UiColumn('sidebar', children);
  }

  List<UiAction> actions() => const [
        UiAction(name: 'sidebar.filter', keys: 'cmd+f',
            context: UiActionContext.node('sidebar')),
        UiAction(name: 'session.new', keys: 'cmd+n'),
      ];
}
