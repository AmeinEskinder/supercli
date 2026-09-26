/// Workspace menus: move-project (row 159), local-site globe (row 161),
/// and open-in-new-window (row 158).
///
/// - [MoveProjectMenu]: pick a destination workspace for a project. Fires
///   the `workspace.project.move` host action; the host re-parents the
///   project and the sidebar re-renders it under the new workspace tree.
/// - [LocalSiteGlobe]: the titlebar globe button for a project's local dev
///   server: open URL, copy URL, stop server. Wired to the project's
///   `localSiteUrl` when the host reports one.
/// - [OpenWorkspaceInNewWindow]: emit `workspace.openInNewWindow`. The
///   actual second window needs gpuidart P0-14 (multi-window), so this
///   stays component-level until the framework supports it.
library;

import 'package:gpuidart/gpuidart.dart';

import 'sidebarview.dart';
import 'workspacessettingspanel.dart';

/// Menu to move a project to another local workspace (row 159).
final class MoveProjectMenu {
  const MoveProjectMenu({
    required this.project,
    this.workspaces = const [],
    this.onMove,
  });

  final SidebarProject project;
  final List<WorkspaceEntry> workspaces;

  /// Called with (projectId, destinationWorkspaceId).
  final void Function(String projectId, String workspaceId)? onMove;

  /// Destination workspaces: all except the project's current one.
  List<WorkspaceEntry> get destinations =>
      workspaces.where((w) => w.id != project.workspaceId).toList();

  UiNode build() {
    return UiColumn('move-project-menu', [
      UiText('move-project-title', 'Move "${project.name}" to…'),
      for (final w in destinations) UiButton('move-project-${w.id}', w.name),
      if (destinations.isEmpty)
        const UiText(
          'move-project-empty',
          'No other workspaces. Add one in Settings ▸ Workspaces.',
        ),
    ]);
  }

  /// Host action payload for the move.
  Map<String, String> moveAction(String workspaceId) => {
    'action': 'workspace.project.move',
    'projectId': project.id,
    'workspaceId': workspaceId,
  };
}

/// Local-site globe button + menu (row 161).
///
/// Shows when the host reports a `localSiteUrl` for the project (e.g. a
/// dev server the agent started). Actions: open URL, copy URL, stop server.
final class LocalSiteGlobe {
  const LocalSiteGlobe({this.siteUrl, this.onOpen, this.onCopy, this.onStop});

  /// The site URL from the host, or null when no local site is running.
  final String? siteUrl;

  final void Function(String url)? onOpen;
  final void Function(String url)? onCopy;
  final void Function()? onStop;

  bool get hasSite => siteUrl != null && siteUrl!.isNotEmpty;

  UiNode build() {
    return UiColumn('local-site-menu', [
      UiButton('local-site-globe', hasSite ? '🌐' : '🌐 (no site)'),
      if (hasSite) ...[
        UiButton('local-site-open', 'Open $siteUrl'),
        UiButton('local-site-copy', 'Copy URL'),
        const UiButton('local-site-stop', 'Stop server'),
      ],
    ]);
  }

  /// Host action payloads.
  Map<String, String> openAction() => {
    'action': 'site.open',
    'url': siteUrl ?? '',
  };
  Map<String, String> stopAction() => {'action': 'site.stop'};
}

/// Row 158: open a workspace in a new window.
///
/// Component only: emits the `workspace.openInNewWindow` host action for
/// the workspace selector. Rendering an actual second window needs gpuidart
/// multi-window support (P0-14, missing — one host, one window per
/// process), so this stays `partial (component)` until P0-14 lands.
final class OpenWorkspaceInNewWindow {
  const OpenWorkspaceInNewWindow({required this.workspace, this.onAction});

  final WorkspaceEntry workspace;

  /// Called with the host action payload.
  final void Function(Map<String, String> payload)? onAction;

  UiNode build() {
    return UiButton(
      'workspace-new-window-${workspace.id}',
      'Open “${workspace.name}” in New Window',
    );
  }

  /// Host action payload for opening [workspace] in a new window.
  Map<String, String> actionPayload() => {
    'action': 'workspace.openInNewWindow',
    'workspaceId': workspace.id,
  };

  void fire() => onAction?.call(actionPayload());
}
