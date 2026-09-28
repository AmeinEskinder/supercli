/// Behaviour tests for the SidebarView completion port.
///
/// Covers the SidebarView.swift components ported in
/// `lib/screens/sidebarview.dart` beyond the #151-154 baseline:
///
/// - `SidebarBranchLabel` — hidden when branch == projectName.
/// - `ActiveProjectBranchLabel` — only for the active project.
/// - `SidebarFooter` — collapse-all disabled state, remote scope verbs.
/// - `RowActionButtons` — archive vs remove affordances.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:test/test.dart';

import 'package:supercli_app/screens/sidebarview.dart';

/// Collect all UiText texts in a node tree (depth-first).
List<String> textsOf(UiNode node) {
  final out = <String>[];
  void walk(UiNode n) {
    if (n is UiText) out.add(n.text);
    if (n is UiColumn) {
      for (final c in n.children) {
        walk(c);
      }
    }
    if (n is UiRow) {
      for (final c in n.children) {
        walk(c);
      }
    }
  }

  walk(node);
  return out;
}

/// Collect all UiButton labels in a node tree (depth-first).
List<String> buttonsOf(UiNode node) {
  final out = <String>[];
  void walk(UiNode n) {
    if (n is UiButton) out.add(n.label);
    if (n is UiColumn) {
      for (final c in n.children) {
        walk(c);
      }
    }
    if (n is UiRow) {
      for (final c in n.children) {
        walk(c);
      }
    }
  }

  walk(node);
  return out;
}

void main() {
  group('SidebarBranchLabel', () {
    test('visible when branch differs from project name', () {
      const l = SidebarBranchLabel(
        branch: 'feature/x',
        projectName: 'myproject',
      );
      expect(l.visible, isTrue);
      expect(l.text, 'feature/x');
      expect(textsOf(l.build('b1')), contains('⎇ feature/x'));
    });

    test('hidden when branch equals project name', () {
      const l = SidebarBranchLabel(
        branch: 'myproject',
        projectName: 'myproject',
      );
      expect(l.visible, isFalse);
      expect(l.text, isEmpty);
    });
  });

  group('ActiveProjectBranchLabel', () {
    test('shows for the project holding the selected session', () {
      const l = ActiveProjectBranchLabel(
        projectId: 'p1',
        selectedSessionProjectId: 'p1',
        branchName: 'main',
        projectName: 'proj',
      );
      expect(l.isActiveProject, isTrue);
      expect(l.visible, isTrue);
      expect(textsOf(l.build()), contains('⎇ main'));
    });

    test('hidden for other projects', () {
      const l = ActiveProjectBranchLabel(
        projectId: 'p2',
        selectedSessionProjectId: 'p1',
        branchName: 'main',
        projectName: 'proj',
      );
      expect(l.isActiveProject, isFalse);
      expect(l.visible, isFalse);
    });

    test('hidden for worktrees and when branch is null', () {
      const worktree = ActiveProjectBranchLabel(
        projectId: 'p1',
        selectedSessionProjectId: 'p1',
        branchName: 'main',
        projectName: 'proj',
        isWorktree: true,
      );
      expect(worktree.visible, isFalse);

      const noBranch = ActiveProjectBranchLabel(
        projectId: 'p1',
        selectedSessionProjectId: 'p1',
        branchName: null,
        projectName: 'proj',
      );
      expect(noBranch.visible, isFalse);
    });
  });

  group('SidebarFooter', () {
    test('collapse-all disabled while nothing expanded', () {
      const f = SidebarFooter(expandedProjectCount: 0);
      expect(f.collapseAllEnabled, isFalse);
      const f2 = SidebarFooter(expandedProjectCount: 3);
      expect(f2.collapseAllEnabled, isTrue);
    });

    test('add-project hidden while remote Host scoped', () {
      const local = SidebarFooter(localVerbsVisible: true);
      const remote = SidebarFooter(localVerbsVisible: false);
      expect(buttonsOf(local.build()), contains('＋'));
      expect(buttonsOf(remote.build()), isNot(contains('＋')));
      // Settings gear always present.
      expect(buttonsOf(remote.build()), contains('⚙'));
    });

    test('actions include settings, add, collapse-all', () {
      const f = SidebarFooter();
      final names = f.actions().map((a) => a.name).toList();
      expect(names, contains('settings.open'));
      expect(names, contains('project.add'));
      expect(names, contains('sidebar.collapse-all'));
    });
  });

  group('RowActionButtons', () {
    test('archive shows for resumable, remove for non-resumable', () {
      expect(RowActionButtons.showsArchive(true), isTrue);
      expect(RowActionButtons.showsRemove(true), isFalse);
      expect(RowActionButtons.showsArchive(false), isFalse);
      expect(RowActionButtons.showsRemove(false), isTrue);
    });
  });
}
