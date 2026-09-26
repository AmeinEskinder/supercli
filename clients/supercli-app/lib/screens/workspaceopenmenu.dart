/// Workspace open menu.
///
/// Port of `WorkspaceOpenMenu.swift`.
library;

import 'package:gpuidart/gpuidart.dart';

/// Menu for opening a workspace (local / remote / recent).
final class WorkspaceOpenMenu {
  const WorkspaceOpenMenu({this.recentPaths = const []});

  final List<String> recentPaths;

  UiNode build() {
    return UiColumn('workspace-open-menu', [
      const UiButton('open-local', 'Open Local Folder…'),
      const UiButton('open-remote', 'Open Remote Host…'),
      const UiText('recent-title', 'Recent'),
      for (final p in recentPaths) UiButton('recent-$p', p),
    ]);
  }
}

/// Remote host workspace browser and folder picker now live in their
/// dedicated files:
/// - `remotehostworkspaceview.dart` (RemoteHostWorkspaceView +
///   RemotePairingFlow, row 208)
/// - `remotefolderpicker.dart` (RemoteFolderPicker + RemoteFolderBrowser,
///   row 199)
