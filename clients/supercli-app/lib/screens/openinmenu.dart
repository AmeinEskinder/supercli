/// Titlebar "Open in" menu (row 162).
///
/// The 24 external editors, terminals and git clients unpeel's titlebar
/// menu offers for opening the current project/session working directory.
/// Each entry maps to a host `open-in` action; the host resolves the real
/// application path per platform (macOS .app bundle, Linux desktop entry,
/// Windows start-menu id) and reports availability so unavailable entries
/// render disabled.
library;

import 'package:gpuidart/gpuidart.dart';

/// One "Open in" target.
final class OpenInTarget {
  const OpenInTarget({
    required this.id,
    required this.name,
    required this.kind,
  });

  final String id;
  final String name;

  /// editor | terminal | git
  final String kind;
}

/// The 24 targets, in unpeel's menu order.
const List<OpenInTarget> openInTargets = [
  // Editors
  OpenInTarget(id: 'vscode', name: 'Visual Studio Code', kind: 'editor'),
  OpenInTarget(id: 'cursor', name: 'Cursor', kind: 'editor'),
  OpenInTarget(id: 'zed', name: 'Zed', kind: 'editor'),
  OpenInTarget(id: 'sublime', name: 'Sublime Text', kind: 'editor'),
  OpenInTarget(id: 'nova', name: 'Nova', kind: 'editor'),
  OpenInTarget(id: 'bbedit', name: 'BBEdit', kind: 'editor'),
  OpenInTarget(id: 'textmate', name: 'TextMate', kind: 'editor'),
  OpenInTarget(id: 'idea', name: 'IntelliJ IDEA', kind: 'editor'),
  OpenInTarget(id: 'pycharm', name: 'PyCharm', kind: 'editor'),
  OpenInTarget(id: 'webstorm', name: 'WebStorm', kind: 'editor'),
  OpenInTarget(id: 'xcode', name: 'Xcode', kind: 'editor'),
  OpenInTarget(id: 'neovim', name: 'Neovim', kind: 'editor'),
  OpenInTarget(id: 'emacs', name: 'Emacs', kind: 'editor'),
  // Terminals
  OpenInTarget(id: 'terminal-app', name: 'Terminal', kind: 'terminal'),
  OpenInTarget(id: 'iterm2', name: 'iTerm2', kind: 'terminal'),
  OpenInTarget(id: 'warp', name: 'Warp', kind: 'terminal'),
  OpenInTarget(id: 'kitty', name: 'Kitty', kind: 'terminal'),
  OpenInTarget(id: 'alacritty', name: 'Alacritty', kind: 'terminal'),
  OpenInTarget(id: 'wezterm', name: 'WezTerm', kind: 'terminal'),
  OpenInTarget(id: 'ghostty-term', name: 'Ghostty', kind: 'terminal'),
  // Git clients
  OpenInTarget(id: 'tower', name: 'Tower', kind: 'git'),
  OpenInTarget(id: 'fork', name: 'Fork', kind: 'git'),
  OpenInTarget(id: 'sublime-merge', name: 'Sublime Merge', kind: 'git'),
  OpenInTarget(id: 'gitkraken', name: 'GitKraken', kind: 'git'),
];

/// The titlebar "Open in" menu for a working directory.
final class OpenInMenu {
  const OpenInMenu({this.cwd = '', this.available = const {}, this.onOpen});

  /// Working directory to open.
  final String cwd;

  /// Target ids the host reports as installed. Entries not in this set
  /// render disabled (still listed, so the user sees what's missing).
  final Set<String> available;

  final void Function(String targetId, String cwd)? onOpen;

  UiNode build() {
    UiNode section(String kind, String title) {
      final items = openInTargets
          .where((t) => t.kind == kind)
          .toList(growable: false);
      return UiColumn('openin-$kind', [
        UiText('openin-$kind-title', title),
        for (final t in items)
          UiButton(
            'openin-${t.id}',
            available.contains(t.id) ? t.name : '${t.name} (not installed)',
          ),
      ]);
    }

    return UiColumn('openin-menu', [
      UiText('openin-title', 'Open $cwd in…'),
      section('editor', 'Editors'),
      section('terminal', 'Terminals'),
      section('git', 'Git clients'),
    ]);
  }

  /// Host action payload for opening [targetId] at [cwd].
  Map<String, String> openAction(String targetId) => {
    'action': 'open-in',
    'target': targetId,
    'cwd': cwd,
  };

  /// True when [actionId] is one of this menu's buttons.
  static bool isOpenInAction(String actionId) =>
      actionId.startsWith('openin-') &&
      openInTargets.any((t) => actionId == 'openin-${t.id}');

  /// Target id for an `openin-<id>` action id, or null.
  static String? targetForAction(String actionId) {
    for (final t in openInTargets) {
      if (actionId == 'openin-${t.id}') return t.id;
    }
    return null;
  }
}
