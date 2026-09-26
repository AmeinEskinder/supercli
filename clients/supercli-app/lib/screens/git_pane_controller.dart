/// Binds [GitPaneView]'s action callbacks to a live [HostClient].
///
/// The view itself is callback-driven; this controller owns the repo path,
/// forwards fetch/pull/push/stage/unstage/commit to the Host's git routes,
/// and refreshes the view state afterwards. The app shell constructs one
/// per git pane and hands the resulting callbacks to the view.
library;

import '../host_client.dart';
import '../host_models.dart';
import 'gitpaneview.dart';

/// Controller wiring GitPaneView callbacks to Host git routes.
final class GitPaneController {
  GitPaneController({required this.client, required this.repoPath});

  final HostClient client;
  final String repoPath;

  /// Last status fetched from the Host. Null until [refresh] succeeds.
  GitStatus? status;

  /// Last error from a Host call, for the shell to surface.
  String? lastError;

  /// Refresh the cached status from `GET /mobile/git/status`.
  Future<void> refresh() async {
    try {
      status = await client.gitStatus(repoPath);
      lastError = null;
    } on HostException catch (e) {
      lastError = e.message;
    }
  }

  Future<void> _run(Future<void> Function() op) async {
    try {
      await op();
      lastError = null;
      await refresh();
    } on HostException catch (e) {
      lastError = e.message;
    }
  }

  /// Build a [GitPaneView] with all callbacks wired to the Host.
  GitPaneView view({
    required String paneId,
    GitPaneTab tab = GitPaneTab.changes,
  }) {
    return GitPaneView(
      paneId: paneId,
      tab: tab,
      onStage: (path) => _run(() => client.gitStage(repoPath, [path])),
      onUnstage: (path) => _run(() => client.gitUnstage(repoPath, [path])),
      onCommit: (message) => _run(() => client.gitCommit(repoPath, message)),
      onFetch: () => _run(() => client.gitFetch(repoPath)),
      onPull: () => _run(() => client.gitPull(repoPath)),
      onPush: () => _run(() => client.gitPush(repoPath)),
    );
  }
}
