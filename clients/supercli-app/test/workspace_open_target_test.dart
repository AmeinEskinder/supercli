/// Tests for `workspace_open_target.dart` — port of `WorkspaceOpenTarget.swift`.
library;

import 'package:supercli_app/screens/workspace_open_target.dart';
import 'package:test/test.dart';

void main() {
  group('WorkspaceOpenTarget (WorkspaceOpenTarget.swift)', () {
    test('all 24 targets exist', () {
      expect(WorkspaceOpenTarget.values.length, 24);
    });

    test('titles match Swift', () {
      expect(WorkspaceOpenTarget.vscode.title, 'VS Code');
      expect(WorkspaceOpenTarget.cursor.title, 'Cursor');
      expect(WorkspaceOpenTarget.zed.title, 'Zed');
      expect(WorkspaceOpenTarget.idea.title, 'IntelliJ');
      expect(WorkspaceOpenTarget.webstorm.title, 'WebStorm');
      expect(WorkspaceOpenTarget.githubDesktop.title, 'GitHub Desktop');
      expect(WorkspaceOpenTarget.finder.title, 'Finder');
      expect(WorkspaceOpenTarget.terminal.title, 'Terminal');
      expect(WorkspaceOpenTarget.iterm2.title, 'iTerm2');
      expect(WorkspaceOpenTarget.ghostty.title, 'Ghostty');
      expect(WorkspaceOpenTarget.xcode.title, 'Xcode');
    });

    test('bundle identifiers match Swift', () {
      expect(WorkspaceOpenTarget.vscode.bundleIdentifiers,
          ['com.microsoft.VSCode']);
      expect(WorkspaceOpenTarget.zed.bundleIdentifiers,
          ['dev.zed.Zed', 'dev.zed.Zed-Preview', 'dev.zed.Zed-Nightly', 'dev.zed.Zed-Dev']);
      expect(WorkspaceOpenTarget.finder.bundleIdentifiers,
          ['com.apple.finder']);
    });

    test('codeEditorId only for editors', () {
      expect(WorkspaceOpenTarget.vscode.codeEditorId, 'code');
      expect(WorkspaceOpenTarget.cursor.codeEditorId, 'cursor');
      expect(WorkspaceOpenTarget.zed.codeEditorId, 'zed');
      expect(WorkspaceOpenTarget.idea.codeEditorId, 'idea');
      expect(WorkspaceOpenTarget.webstorm.codeEditorId, 'webstorm');
      expect(WorkspaceOpenTarget.xcode.codeEditorId, 'xcode');
      expect(WorkspaceOpenTarget.finder.codeEditorId, isNull);
      expect(WorkspaceOpenTarget.terminal.codeEditorId, isNull);
      expect(WorkspaceOpenTarget.githubDesktop.codeEditorId, isNull);
    });

    test('editorTargets in menu order', () {
      expect(WorkspaceOpenTarget.editorTargets, [
        WorkspaceOpenTarget.vscode,
        WorkspaceOpenTarget.cursor,
        WorkspaceOpenTarget.zed,
        WorkspaceOpenTarget.idea,
        WorkspaceOpenTarget.webstorm,
        WorkspaceOpenTarget.xcode,
      ]);
    });

    test('gitAppCases has six git apps', () {
      expect(WorkspaceOpenTarget.gitAppCases.length, 6);
    });

    test('preferred resolves editor id, defaults to vscode', () {
      expect(WorkspaceOpenTarget.preferred(forEditor: 'cursor'),
          WorkspaceOpenTarget.cursor);
      expect(WorkspaceOpenTarget.preferred(forEditor: '  ZED  '),
          WorkspaceOpenTarget.zed);
      expect(WorkspaceOpenTarget.preferred(forEditor: 'unknown'),
          WorkspaceOpenTarget.vscode);
    });

    test('fallback symbols match Swift groups', () {
      expect(WorkspaceOpenTarget.finder.fallbackSymbol, 'folder');
      expect(WorkspaceOpenTarget.terminal.fallbackSymbol, 'terminal');
      expect(WorkspaceOpenTarget.ghostty.fallbackSymbol, 'terminal');
      expect(WorkspaceOpenTarget.xcode.fallbackSymbol, 'hammer.fill');
      expect(WorkspaceOpenTarget.vscode.fallbackSymbol,
          'chevron.left.forwardslash.chevron.right');
      expect(WorkspaceOpenTarget.fork.fallbackSymbol, 'arrow.triangle.branch');
    });
  });
}
