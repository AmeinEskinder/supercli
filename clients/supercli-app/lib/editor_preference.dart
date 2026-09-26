/// Default editor / opener preference.
///
/// Covers checklist row 215: "Default editor / opener preference".
///
/// Used by "Open in editor" actions (session context menu, clickable
/// paths, worktree reveal). The preference stores a preferred editor
/// identifier plus an ordered fallback chain; [resolve] picks the first
/// available editor so a missing app never dead-ends the action.
library;

/// A known editor/opener.
final class EditorInfo {
  const EditorInfo({
    required this.id,
    required this.name,
    this.bundleId,
    this.executableHint = '',
  });

  /// Stable identifier, e.g. `vscode`, `zed`, `xcode`, `finder`.
  final String id;
  final String name;

  /// macOS bundle identifier for `open -b`, if applicable.
  final String? bundleId;

  /// Executable name searched on PATH (Linux/other).
  final String executableHint;
}

/// Well-known editors, in fallback priority order.
const List<EditorInfo> knownEditors = [
  EditorInfo(
      id: 'vscode',
      name: 'Visual Studio Code',
      bundleId: 'com.microsoft.VSCode',
      executableHint: 'code'),
  EditorInfo(
      id: 'zed',
      name: 'Zed',
      bundleId: 'dev.zed.Zed',
      executableHint: 'zed'),
  EditorInfo(
      id: 'xcode',
      name: 'Xcode',
      bundleId: 'com.apple.dt.Xcode',
      executableHint: 'xed'),
  EditorInfo(
      id: 'finder',
      name: 'Finder',
      bundleId: 'com.apple.finder',
      executableHint: 'open'),
];

/// Default editor preference with fallback resolution.
final class EditorPreference {
  const EditorPreference({
    this.preferredEditorId,
    this.fallbackIds = const ['vscode', 'zed', 'finder'],
  });

  /// Editor id chosen by the user, or null for "system default".
  final String? preferredEditorId;

  /// Ordered fallback chain tried when the preferred editor is missing.
  final List<String> fallbackIds;

  /// Resolve against [availableIds] (editors actually installed).
  /// Returns the preferred editor if available, else the first
  /// available fallback, else null.
  String? resolve(Set<String> availableIds) {
    if (preferredEditorId != null &&
        availableIds.contains(preferredEditorId)) {
      return preferredEditorId;
    }
    for (final id in fallbackIds) {
      if (availableIds.contains(id)) return id;
    }
    return null;
  }

  EditorInfo? infoFor(String id) {
    for (final e in knownEditors) {
      if (e.id == id) return e;
    }
    return null;
  }

  Map<String, Object?> toJson() => {
        'preferredEditorId': preferredEditorId,
        'fallbackIds': fallbackIds,
      };

  factory EditorPreference.fromJson(Map<String, dynamic> json) =>
      EditorPreference(
        preferredEditorId: json['preferredEditorId'] as String?,
        fallbackIds:
            (json['fallbackIds'] as List?)?.cast<String>() ?? const [],
      );
}
