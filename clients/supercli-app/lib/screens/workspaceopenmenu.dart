/// Workspace open menu and remote host workspace view.
///
/// Port of `WorkspaceOpenMenu.swift`, `RemoteHostWorkspaceView.swift`,
/// and `RemoteFolderPicker.swift`.
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

/// Remote host workspace browser.
final class RemoteHostWorkspaceView {
  const RemoteHostWorkspaceView({
    this.hostName = '',
    this.currentPath = '',
    this.entries = const [],
  });

  final String hostName;
  final String currentPath;
  final List<String> entries;

  UiNode build() {
    return UiColumn('remote-workspace', [
      UiText('remote-ws-title', 'Workspace on $hostName'),
      UiText('remote-ws-path', currentPath),
      UiTable('remote-ws-entries', dataset: 'remote-ws-entries'),
      UiRow('remote-ws-actions', [
        const UiButton('remote-ws-open', 'Open'),
        const UiButton('remote-ws-cancel', 'Cancel'),
      ]),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'remote-ws-entries',
        columns: const ['Name'],
        rows: entries.map((e) => [e]).toList(),
      );
}

/// Remote folder picker sheet.
final class RemoteFolderPicker {
  const RemoteFolderPicker({
    this.hostName = '',
    this.currentPath = '',
    this.entries = const [],
  });

  final String hostName;
  final String currentPath;
  final List<String> entries;

  UiNode build() {
    return UiColumn('remote-folder-picker', [
      UiText('rfp-title', 'Choose Folder on $hostName'),
      UiText('rfp-path', currentPath),
      UiTable('rfp-entries', dataset: 'rfp-entries'),
      UiRow('rfp-actions', [
        const UiButton('rfp-choose', 'Choose'),
        const UiButton('rfp-cancel', 'Cancel'),
      ]),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'rfp-entries',
        columns: const ['Name'],
        rows: entries.map((e) => [e]).toList(),
      );
}
