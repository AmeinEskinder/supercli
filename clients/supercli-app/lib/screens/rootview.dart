/// App shell: resizable sidebar + content area.
///
/// Port of `RootView.swift` (SupercliNative/Views). The Swift version uses
/// SwiftUI with @AppStorage for sidebar width persistence, vibrancy materials,
/// and a detached session-drag overlay. The gpuidart port uses UiRow/UiColumn
/// with the sidebar as a collapsible pane.
///
/// GAP: No resizable split-pane primitive in gpuidart. Sidebar width is fixed;
/// collapse/expand via button. Logged as P0-9 in docs/gpuidart-requirements.md.
library;

import 'package:gpuidart/gpuidart.dart';

import 'sidebarview.dart';
import 'terminalarea.dart';

/// The root application layout: sidebar + main content.
final class RootView {
  RootView({
    required this.sidebar,
    required this.content,
    this.sidebarCollapsed = false,
    this.settingsVisible = false,
  });

  final SidebarView sidebar;
  final TerminalArea content;
  final bool sidebarCollapsed;
  final bool settingsVisible;

  UiNode build() {
    return UiRow('root-layout', [
      if (!sidebarCollapsed)
        sidebar.build()
      else
        UiColumn('sidebar-collapsed', [
          const UiButton('expand-sidebar', '+'),
          const UiButton('new-session-collapsed', 'New'),
        ]),
      UiColumn('content-area', [
        content.build(),
      ]),
    ]);
  }

  List<UiAction> actions() => const [
        UiAction(name: 'sidebar.toggle', keys: 'cmd+b'),
      ];
}
