/// Local persistence for desktop-only settings.
///
/// The Host owns the workspace settings served at `GET/POST
/// /mobile/workspace-settings` (see [AppSettings.toHostJson]). Everything
/// else — theme overrides, appearance, notifications, advanced prefs — is
/// desktop-only and must survive app restarts without a Host round-trip, so
/// it persists through the app's own config file.
///
/// The controller keeps a full local snapshot ([AppSettings.toLocalJson]):
/// on load the local snapshot is applied first, then the Host response
/// overrides only the Host-owned fields; on save the local snapshot is
/// rewritten after the Host save.
///
/// IO failures are graceful by contract: [FileSettingsLocalStore.load]
/// returns an empty map when the file is missing or corrupt, and
/// [SettingsController] reports save failures through `onError` without
/// throwing.
library;

import 'dart:convert';
import 'dart:io';

/// Persistence backend for the local settings snapshot. Injected into
/// [SettingsController] so tests can substitute a fake.
abstract class SettingsLocalStore {
  /// Read the last-saved snapshot. Returns an empty map when nothing was
  /// saved yet or the stored data is unusable; never throws.
  Future<Map<String, Object?>> load();

  /// Persist a snapshot. May throw on IO failure; the controller catches
  /// and reports it through `onError`.
  Future<void> save(Map<String, Object?> json);
}

/// JSON-file implementation of [SettingsLocalStore].
///
/// Writes are atomic (temp file + rename) so a crash mid-save cannot leave
/// a torn config behind. Reads tolerate a missing file and corrupt JSON.
final class FileSettingsLocalStore implements SettingsLocalStore {
  FileSettingsLocalStore(this.path);

  final String path;

  /// Default config path for the desktop app: `$SUPERCLI_HOME/desktop/
  /// settings.json`, falling back to `~/.supercli/desktop/settings.json`.
  /// Mirrors the Rust `app_paths` convention (see
  /// `crates/supercli-core/src/app_paths.rs`).
  static String defaultPath() {
    final home = Platform.environment['SUPERCLI_HOME'] ??
        '${Platform.environment['HOME'] ?? '~'}/.supercli';
    return '$home/desktop/settings.json';
  }

  @override
  Future<Map<String, Object?>> load() async {
    try {
      final file = File(path);
      if (!await file.exists()) return <String, Object?>{};
      final text = await file.readAsString();
      final decoded = jsonDecode(text);
      if (decoded is Map<String, Object?>) return decoded;
      if (decoded is Map) {
        return Map<String, Object?>.from(decoded);
      }
      return <String, Object?>{};
    } catch (_) {
      // Missing file, unreadable file, or corrupt JSON: start from defaults.
      return <String, Object?>{};
    }
  }

  @override
  Future<void> save(Map<String, Object?> json) async {
    final file = File(path);
    await file.parent.create(recursive: true);
    final tmp = File('$path.tmp.$pid');
    try {
      await tmp.writeAsString(jsonEncode(json), flush: true);
      await tmp.rename(path);
    } catch (_) {
      try {
        await tmp.delete();
      } catch (_) {}
      rethrow;
    }
  }
}
