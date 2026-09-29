/// Session gallery artifacts: shared on-disk model.
///
/// Port of `SessionArtifacts.swift`
/// (`clients/legacy/native/SupercliNative/Sources/SupercliNative/`).
///
/// Shared on-disk model for a session's gallery artifacts under
/// `~/.supercli/app-sessions/<id>/artifacts/`. One kind list + directory
/// mapping for every gallery surface — the desktop title-bar gallery
/// (SessionGalleryPanel) and the phone's /mobile/artifacts routes
/// (MobileSessionControl) — so the two can never drift.
library;

import 'dart:io';

/// A single gallery artifact file.
final class SessionArtifact {
  const SessionArtifact({
    required this.kind,
    required this.name,
    required this.path,
    required this.size,
    required this.modifiedAt,
  });

  final String kind;
  final String name;
  final String path;
  final int size;
  final DateTime modifiedAt;

  /// Stable within a session: kind + filename is the on-disk address.
  String get id => '$kind/$name';

  bool get isImage {
    final ext = name.split('.').last.toLowerCase();
    return const {'png', 'jpg', 'jpeg', 'gif', 'webp'}.contains(ext);
  }

  @override
  bool operator ==(Object other) =>
      other is SessionArtifact &&
      kind == other.kind &&
      name == other.name &&
      path == other.path &&
      size == other.size &&
      modifiedAt == other.modifiedAt;

  @override
  int get hashCode => Object.hash(kind, name, path, size, modifiedAt);
}

/// Gallery artifact store: kind list + directory mapping + file operations.
///
/// All functions are pure except those that touch the filesystem, which are
/// static for testability. The `appSessionsDir` must be provided by the
/// caller (injected for testing).
abstract final class SessionArtifactStore {
  const SessionArtifactStore._();

  /// Gallery kinds. `screenshots` and `downloads` are browser-MCP output
  /// (under `artifacts/browser/`), `computer` is computer-MCP captures,
  /// and `uploads` is images the user added or edited from the phone — so
  /// the gallery is a unified per-session image view, not just one
  /// source's captures.
  static const List<String> listedKinds = [
    'screenshots',
    'downloads',
    'uploads',
    'computer',
  ];

  /// Kinds that are agent captures (browser/computer screenshots) — the
  /// ones whose arrival pulses the gallery button on desktop and phone.
  /// Uploads/downloads are excluded: the user put those there themselves.
  static const List<String> captureKinds = ['screenshots', 'computer'];

  /// Root artifacts dir for a session.
  static String root(String appSessionsDir, String sessionId) =>
      '$appSessionsDir/$sessionId/artifacts';

  /// Resolve a gallery kind to its on-disk dir. `uploads` sits directly
  /// under `artifacts/`; the browser kinds live under `artifacts/browser/`.
  /// Returns null for an unknown kind (callers 404 / skip).
  static String? kindDir(
      String appSessionsDir, String sessionId, String kind) {
    final base = root(appSessionsDir, sessionId);
    switch (kind) {
      case 'screenshots':
        return '$base/browser/screenshots';
      case 'downloads':
        return '$base/browser/downloads';
      case 'uploads':
        return '$base/uploads';
      case 'computer':
        // Computer MCP captures (`see`/`screenshot`) — the Rust server
        // writes here, same contract as the browser kinds.
        return '$base/computer/screenshots';
      default:
        return null;
    }
  }

  /// Downscaled phone-gallery variants live here — a sibling of the kind
  /// dirs, deliberately not a listed kind so it never shows in a gallery.
  static String thumbsDir(String appSessionsDir, String sessionId) =>
      '${root(appSessionsDir, sessionId)}/thumbs';

  /// Newest capture mtime (ms epoch) for the pulse signal; 0 when none.
  static int latestCaptureUnixMs(String appSessionsDir, String sessionId) {
    var latest = 0;
    for (final kind in captureKinds) {
      final dir = kindDir(appSessionsDir, sessionId, kind);
      if (dir == null) continue;
      final entries = _listDir(dir);
      for (final entry in entries) {
        final mtime = entry.statSync().modified.millisecondsSinceEpoch;
        if (mtime > latest) latest = mtime;
      }
    }
    return latest;
  }

  /// Every artifact across the listed kinds, newest-first.
  static List<SessionArtifact> list(
      String appSessionsDir, String sessionId) {
    final artifacts = <SessionArtifact>[];
    for (final kind in listedKinds) {
      final dir = kindDir(appSessionsDir, sessionId, kind);
      if (dir == null) continue;
      for (final entry in _listDir(dir)) {
        final stat = entry.statSync();
        artifacts.add(SessionArtifact(
          kind: kind,
          name: entry.uri.pathSegments.last,
          path: entry.path,
          size: stat.size,
          modifiedAt: stat.modified,
        ));
      }
    }
    artifacts.sort((a, b) => b.modifiedAt.compareTo(a.modifiedAt));
    return artifacts;
  }

  /// Remove one artifact from disk, plus any cached thumbnail variants.
  /// Idempotent: a missing file is a no-op success.
  static void delete(
      String appSessionsDir, String sessionId, String kind, String name) {
    final dir = kindDir(appSessionsDir, sessionId, kind);
    if (dir == null) return;
    final file = File('$dir/$name');
    if (file.existsSync()) {
      file.deleteSync();
    }
    for (final thumb in thumbnailVariants(
        appSessionsDir, sessionId, kind: kind, name: name)) {
      try {
        File(thumb).deleteSync();
      } catch (_) {
        // Best-effort cleanup.
      }
    }
  }

  /// Cached thumbnail files for one artifact, optionally excluding the
  /// freshly-generated variant being kept.
  static List<String> thumbnailVariants(
    String appSessionsDir,
    String sessionId, {
    required String kind,
    required String name,
    String? keeping,
  }) {
    final suffix = '-$kind-$name.jpg';
    final dir = Directory(thumbsDir(appSessionsDir, sessionId));
    if (!dir.existsSync()) return const [];
    return dir
        .listSync()
        .whereType<File>()
        .map((f) => f.path)
        .where((p) =>
            p.endsWith(suffix) &&
            (keeping == null || !p.endsWith('/$keeping')))
        .toList();
  }

  static List<FileSystemEntity> _listDir(String dir) {
    final d = Directory(dir);
    if (!d.existsSync()) return const [];
    return d.listSync().where((e) {
      if (e is! File) return false;
      final name = e.uri.pathSegments.last;
      return !name.startsWith('.');
    }).toList();
  }
}
