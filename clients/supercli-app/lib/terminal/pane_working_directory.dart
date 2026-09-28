/// Pane working-directory seeding rule.
///
/// Port of `SupercliStore.paneWorkingDirectory(sessionCwd:projectPath:)`
/// from the native SupercliStore.
///
/// The pane cwd seed behind cmd-click path resolution: a Session's own
/// launch cwd wins, the scope-aware project path is the fallback for Hosts
/// that publish no cwd, and the seed only matters until the shell's OSC 7
/// report replaces it inside the pane.
///
/// Note: `ClickablePath.resolveFile` (used with this seed) was ported to
/// Rust by another worker (`crates/supercli-core/src/clickable_path.rs`);
/// the JSON `cwd` decoding (`RemoteSessionSummary`, `HostedSessionManifest`)
/// and `sessionEntry(fromRemote:)` belong to the DTO/store workers.
library;

/// Pure seeding rule: a non-empty Session cwd wins; an absent or blank one
/// falls back to the project path (older Hosts publish no cwd).
String? paneWorkingDirectory({String? sessionCwd, String? projectPath}) {
  final cwd = sessionCwd?.trim();
  if (cwd != null && cwd.isNotEmpty) return cwd;
  return projectPath;
}
