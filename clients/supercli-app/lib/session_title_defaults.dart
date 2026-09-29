/// Session title defaults: initial labels and path abbreviation.
///
/// Port of `SessionTitleDefaults` (enum) in `Models.swift`
/// (`clients/legacy/native/SupercliNative/Sources/SupercliNative/`).
///
/// Agent sessions begin with their command. A blank shell begins with its
/// working folder and is replaced by the Host after the first submitted
/// command, just like the former provisional "Terminal" label.
library;

import 'dart:io';

/// Default title-label rules for terminal sessions.
class SessionTitleDefaults {
  /// The initial label for a session.
  ///
  /// If [command] is non-blank, it wins. Otherwise the working folder,
  /// abbreviated via [abbreviatedPath], is used; if that is blank,
  /// falls back to "Terminal" for compatibility.
  static String initialLabel({
    required String command,
    required String cwd,
    String? home,
  }) {
    if (command.trim().isNotEmpty) return command;
    final folder = abbreviatedPath(cwd, home: home);
    return folder.trim().isEmpty ? 'Terminal' : folder;
  }

  /// Abbreviates [path] by replacing the [home] prefix with "~".
  ///
  /// The prefix must end at a path boundary: "/Users/testing/project"
  /// with home "/Users/test" is NOT abbreviated.
  static String abbreviatedPath(String path, {String? home}) {
    final h = home ?? Platform.environment['HOME'] ?? '';
    if (h.isEmpty) return path;
    if (path == h) return '~';
    if (path.startsWith('$h/')) return '~${path.substring(h.length)}';
    return path;
  }
}
