/// Workspace open targets: external apps for the "open workspace" menu.
///
/// Port of `WorkspaceOpenTarget.swift`
/// (`clients/legacy/native/SupercliNative/Sources/SupercliNative/`).
///
/// Targets for the titlebar "open workspace" menu. The actual launching
/// happens in the store so failures can use the app's shared alert path.
///
/// Only the portable data is here (enum cases, titles, bundle IDs, app
/// names, symbols, editor IDs). macOS-specific availability checks
/// (NSWorkspace, Launch Services) and icon loading are platform code.
library;

/// An external application that can open a workspace.
enum WorkspaceOpenTarget {
  vscode,
  cursor,
  zed,
  idea,
  webstorm,
  githubDesktop,
  fork,
  tower,
  sourcetree,
  gitkraken,
  sublimeMerge,
  finder,
  terminal,
  iterm2,
  ghostty,
  warp,
  wezterm,
  kitty,
  alacritty,
  tabby,
  hyper,
  rio,
  wave,
  xcode;

  String get id => name;

  String get title {
    switch (this) {
      case WorkspaceOpenTarget.vscode:
        return 'VS Code';
      case WorkspaceOpenTarget.cursor:
        return 'Cursor';
      case WorkspaceOpenTarget.zed:
        return 'Zed';
      case WorkspaceOpenTarget.idea:
        return 'IntelliJ';
      case WorkspaceOpenTarget.webstorm:
        return 'WebStorm';
      case WorkspaceOpenTarget.githubDesktop:
        return 'GitHub Desktop';
      case WorkspaceOpenTarget.fork:
        return 'Fork';
      case WorkspaceOpenTarget.tower:
        return 'Tower';
      case WorkspaceOpenTarget.sourcetree:
        return 'Sourcetree';
      case WorkspaceOpenTarget.gitkraken:
        return 'GitKraken';
      case WorkspaceOpenTarget.sublimeMerge:
        return 'Sublime Merge';
      case WorkspaceOpenTarget.finder:
        return 'Finder';
      case WorkspaceOpenTarget.terminal:
        return 'Terminal';
      case WorkspaceOpenTarget.iterm2:
        return 'iTerm2';
      case WorkspaceOpenTarget.ghostty:
        return 'Ghostty';
      case WorkspaceOpenTarget.warp:
        return 'Warp';
      case WorkspaceOpenTarget.wezterm:
        return 'WezTerm';
      case WorkspaceOpenTarget.kitty:
        return 'kitty';
      case WorkspaceOpenTarget.alacritty:
        return 'Alacritty';
      case WorkspaceOpenTarget.tabby:
        return 'Tabby';
      case WorkspaceOpenTarget.hyper:
        return 'Hyper';
      case WorkspaceOpenTarget.rio:
        return 'Rio';
      case WorkspaceOpenTarget.wave:
        return 'Wave';
      case WorkspaceOpenTarget.xcode:
        return 'Xcode';
    }
  }

  List<String> get bundleIdentifiers {
    switch (this) {
      case WorkspaceOpenTarget.vscode:
        return ['com.microsoft.VSCode'];
      case WorkspaceOpenTarget.cursor:
        return ['com.todesktop.230313mzl4w4u92'];
      case WorkspaceOpenTarget.zed:
        return [
          'dev.zed.Zed',
          'dev.zed.Zed-Preview',
          'dev.zed.Zed-Nightly',
          'dev.zed.Zed-Dev'
        ];
      case WorkspaceOpenTarget.idea:
        return ['com.jetbrains.intellij', 'com.jetbrains.intellij.ce'];
      case WorkspaceOpenTarget.webstorm:
        return ['com.jetbrains.WebStorm'];
      case WorkspaceOpenTarget.githubDesktop:
        return ['com.github.GitHubClient'];
      case WorkspaceOpenTarget.fork:
        return ['com.DanPristupov.Fork'];
      case WorkspaceOpenTarget.tower:
        return ['com.fournova.Tower3', 'com.fournova.Tower'];
      case WorkspaceOpenTarget.sourcetree:
        return ['com.torusknot.SourceTreeNotMAS'];
      case WorkspaceOpenTarget.gitkraken:
        return ['com.axosoft.gitkraken'];
      case WorkspaceOpenTarget.sublimeMerge:
        return ['com.sublimemerge'];
      case WorkspaceOpenTarget.finder:
        return ['com.apple.finder'];
      case WorkspaceOpenTarget.terminal:
        return ['com.apple.Terminal'];
      case WorkspaceOpenTarget.iterm2:
        return ['com.googlecode.iterm2'];
      case WorkspaceOpenTarget.ghostty:
        return ['com.mitchellh.ghostty'];
      case WorkspaceOpenTarget.warp:
        return ['dev.warp.Warp-Stable', 'dev.warp.Warp'];
      case WorkspaceOpenTarget.wezterm:
        return ['com.github.wez.wezterm'];
      case WorkspaceOpenTarget.kitty:
        return ['net.kovidgoyal.kitty'];
      case WorkspaceOpenTarget.alacritty:
        return ['org.alacritty'];
      case WorkspaceOpenTarget.tabby:
        return ['org.tabby'];
      case WorkspaceOpenTarget.hyper:
        return ['co.zeit.hyper'];
      case WorkspaceOpenTarget.rio:
        return ['com.raphaelamorim.rio'];
      case WorkspaceOpenTarget.wave:
        return ['dev.commandline.waveterm'];
      case WorkspaceOpenTarget.xcode:
        return ['com.apple.dt.Xcode'];
    }
  }

  List<String> get appNames {
    switch (this) {
      case WorkspaceOpenTarget.vscode:
        return ['Visual Studio Code'];
      case WorkspaceOpenTarget.cursor:
        return ['Cursor'];
      case WorkspaceOpenTarget.zed:
        return ['Zed', 'Zed Preview', 'Zed Nightly'];
      case WorkspaceOpenTarget.idea:
        return ['IntelliJ IDEA', 'IntelliJ IDEA CE'];
      case WorkspaceOpenTarget.webstorm:
        return ['WebStorm'];
      case WorkspaceOpenTarget.githubDesktop:
        return ['GitHub Desktop'];
      case WorkspaceOpenTarget.fork:
        return ['Fork'];
      case WorkspaceOpenTarget.tower:
        return ['Tower'];
      case WorkspaceOpenTarget.sourcetree:
        return ['Sourcetree', 'SourceTree'];
      case WorkspaceOpenTarget.gitkraken:
        return ['GitKraken'];
      case WorkspaceOpenTarget.sublimeMerge:
        return ['Sublime Merge'];
      case WorkspaceOpenTarget.finder:
        return ['Finder'];
      case WorkspaceOpenTarget.terminal:
        return ['Terminal'];
      case WorkspaceOpenTarget.iterm2:
        return ['iTerm', 'iTerm2'];
      case WorkspaceOpenTarget.ghostty:
        return ['Ghostty'];
      case WorkspaceOpenTarget.warp:
        return ['Warp'];
      case WorkspaceOpenTarget.wezterm:
        return ['WezTerm'];
      case WorkspaceOpenTarget.kitty:
        return ['kitty'];
      case WorkspaceOpenTarget.alacritty:
        return ['Alacritty'];
      case WorkspaceOpenTarget.tabby:
        return ['Tabby'];
      case WorkspaceOpenTarget.hyper:
        return ['Hyper'];
      case WorkspaceOpenTarget.rio:
        return ['Rio', 'rio'];
      case WorkspaceOpenTarget.wave:
        return ['Wave Terminal', 'Wave'];
      case WorkspaceOpenTarget.xcode:
        return ['Xcode'];
    }
  }

  /// SF Symbol name for the fallback icon.
  String get fallbackSymbol {
    switch (this) {
      case WorkspaceOpenTarget.finder:
        return 'folder';
      case WorkspaceOpenTarget.terminal:
      case WorkspaceOpenTarget.iterm2:
      case WorkspaceOpenTarget.ghostty:
      case WorkspaceOpenTarget.warp:
      case WorkspaceOpenTarget.wezterm:
      case WorkspaceOpenTarget.kitty:
      case WorkspaceOpenTarget.alacritty:
      case WorkspaceOpenTarget.tabby:
      case WorkspaceOpenTarget.hyper:
      case WorkspaceOpenTarget.rio:
      case WorkspaceOpenTarget.wave:
        return 'terminal';
      case WorkspaceOpenTarget.xcode:
        return 'hammer.fill';
      case WorkspaceOpenTarget.vscode:
      case WorkspaceOpenTarget.cursor:
      case WorkspaceOpenTarget.zed:
      case WorkspaceOpenTarget.idea:
      case WorkspaceOpenTarget.webstorm:
        return 'chevron.left.forwardslash.chevron.right';
      case WorkspaceOpenTarget.githubDesktop:
      case WorkspaceOpenTarget.fork:
      case WorkspaceOpenTarget.tower:
      case WorkspaceOpenTarget.sourcetree:
      case WorkspaceOpenTarget.gitkraken:
      case WorkspaceOpenTarget.sublimeMerge:
        return 'arrow.triangle.branch';
    }
  }

  /// The `codeEditor` id this target maps to (the value persisted as the
  /// default editor). Only the editor-group targets have one; everything
  /// else (git apps, terminals, Finder) returns null.
  String? get codeEditorId {
    switch (this) {
      case WorkspaceOpenTarget.vscode:
        return 'code';
      case WorkspaceOpenTarget.cursor:
        return 'cursor';
      case WorkspaceOpenTarget.zed:
        return 'zed';
      case WorkspaceOpenTarget.idea:
        return 'idea';
      case WorkspaceOpenTarget.webstorm:
        return 'webstorm';
      case WorkspaceOpenTarget.xcode:
        return 'xcode';
      default:
        return null;
    }
  }

  /// The editor targets, in menu order, used by both the titlebar dropdown
  /// and the Settings "Default editor" picker.
  static List<WorkspaceOpenTarget> get editorTargets => const [
        WorkspaceOpenTarget.vscode,
        WorkspaceOpenTarget.cursor,
        WorkspaceOpenTarget.zed,
        WorkspaceOpenTarget.idea,
        WorkspaceOpenTarget.webstorm,
        WorkspaceOpenTarget.xcode,
      ];

  static const List<WorkspaceOpenTarget> gitAppCases = [
    WorkspaceOpenTarget.githubDesktop,
    WorkspaceOpenTarget.fork,
    WorkspaceOpenTarget.tower,
    WorkspaceOpenTarget.sourcetree,
    WorkspaceOpenTarget.gitkraken,
    WorkspaceOpenTarget.sublimeMerge,
  ];

  /// Preferred editor for a persisted `codeEditor` id string.
  static WorkspaceOpenTarget preferred({required String forEditor}) {
    final id = forEditor.trim().toLowerCase();
    return WorkspaceOpenTarget.values.firstWhere(
      (t) => t.codeEditorId == id,
      orElse: () => WorkspaceOpenTarget.vscode,
    );
  }
}
