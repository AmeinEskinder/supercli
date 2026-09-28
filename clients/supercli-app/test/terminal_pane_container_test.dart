/// Behaviour tests for the TerminalPaneView container logic port.
///
/// Covers the portable state logic from `TerminalPaneView.swift`
/// (TerminalPaneContainer, 2,224 lines) that does not depend on SwiftUI:
///
/// - `terminal_pane_divider.dart` — divider drag ratio math (request =
///   start + translation/extent, clamped to [0.1, 0.9]; live ratio stays
///   local, model commits on release).
/// - `terminal_pane_model.dart` — PresentedPane stable identity
///   (`session:{id}` / `launcher:{paneID}`), synthetic solo pane,
///   defaultActivePaneID resolution.
/// - `terminal_pane_confirmation.dart` — PaneConfirmation titles, messages,
///   and confirm labels (archive vs remove, live vs not-live).
library;

import 'package:test/test.dart';

import 'package:supercli_app/terminal/terminal_pane_confirmation.dart';
import 'package:supercli_app/terminal/terminal_pane_divider.dart';
import 'package:supercli_app/terminal/terminal_pane_model.dart';

void main() {
  group('dividerDragRatio', () {
    test('adds translation/extent to the start ratio', () {
      // Drag right 100pt in a 1000pt extent: +0.10.
      expect(
        dividerDragRatio(startRatio: 0.5, translation: 100, extent: 1000),
        closeTo(0.6, 1e-9),
      );
    });

    test('negative translation shrinks the ratio', () {
      expect(
        dividerDragRatio(startRatio: 0.5, translation: -200, extent: 1000),
        closeTo(0.3, 1e-9),
      );
    });

    test('clamps to minimumSplitRatio (0.1)', () {
      // Requested 0.05 -> clamped to 0.1.
      expect(
        dividerDragRatio(startRatio: 0.5, translation: -900, extent: 1000),
        minimumSplitRatio,
      );
    });

    test('clamps to maximumSplitRatio (0.9)', () {
      // Requested 0.95 -> clamped to 0.9.
      expect(
        dividerDragRatio(startRatio: 0.5, translation: 900, extent: 1000),
        maximumSplitRatio,
      );
    });

    test('zero extent ignores the drag (returns start ratio)', () {
      // Matches Swift `guard extent > 0 else { return }`.
      expect(
        dividerDragRatio(startRatio: 0.4, translation: 500, extent: 0),
        0.4,
      );
    });

    test('continuing drag uses the live ratio as start', () {
      // Second onChanged with the same pathKey: start = live ratio.
      final first = dividerDragRatio(
          startRatio: 0.5, translation: 100, extent: 1000);
      final second = dividerDragRatio(
          startRatio: first, translation: 50, extent: 1000);
      expect(second, closeTo(0.65, 1e-9));
    });
  });

  group('shouldIgnoreDividerDrag', () {
    test('ignores when there is no group', () {
      expect(
          shouldIgnoreDividerDrag(hasGroup: false, extent: 1000), isTrue);
    });

    test('ignores when extent is zero', () {
      expect(shouldIgnoreDividerDrag(hasGroup: true, extent: 0), isTrue);
    });

    test('processes when group exists and extent is positive', () {
      expect(
          shouldIgnoreDividerDrag(hasGroup: true, extent: 1000), isFalse);
    });
  });

  group('DividerDrag equality', () {
    test('value equality', () {
      const a = DividerDrag(pathKey: 'k', startRatio: 0.5, ratio: 0.6);
      const b = DividerDrag(pathKey: 'k', startRatio: 0.5, ratio: 0.6);
      expect(a, equals(b));
      expect(a.hashCode, equals(b.hashCode));
    });
  });

  group('PresentedPane.id', () {
    test('session content uses the session id', () {
      const pane = PresentedPane(
        paneId: 'p1',
        content: SessionContent('abc123'),
        isSynthetic: false,
      );
      expect(pane.id, 'session:abc123');
    });

    test('launcher content uses the temporary pane id', () {
      const pane = PresentedPane(
        paneId: 'tmp-7',
        content: LauncherContent(),
        isSynthetic: false,
      );
      expect(pane.id, 'launcher:tmp-7');
    });
  });

  group('presentedPanes', () {
    test('nil group yields one synthetic solo pane', () {
      final panes = presentedPanes(representativeId: 's1');
      expect(panes, hasLength(1));
      expect(panes.first.isSynthetic, isTrue);
      expect(panes.first.paneId, 'solo:s1');
      expect(panes.first.id, 'session:s1');
    });

    test('group panes map with isSynthetic false', () {
      final panes = presentedPanes(
        representativeId: 's1',
        groupPanes: const [
          GroupPaneEntry(id: 'p1', content: SessionContent('s1')),
          GroupPaneEntry(id: 'p2', content: LauncherContent()),
        ],
      );
      expect(panes, hasLength(2));
      expect(panes.every((p) => !p.isSynthetic), isTrue);
      expect(panes[0].id, 'session:s1');
      expect(panes[1].id, 'launcher:p2');
    });
  });

  group('defaultActivePaneID', () {
    test('prefers the pane matching the representative session', () {
      expect(
        defaultActivePaneID(
          representativeId: 's2',
          groupPanes: const [
            GroupPaneEntry(id: 'p1', content: SessionContent('s1')),
            GroupPaneEntry(id: 'p2', content: SessionContent('s2')),
          ],
          groupRepresentativePaneID: 'p1',
        ),
        'p2',
      );
    });

    test('falls back to the group representative pane id', () {
      expect(
        defaultActivePaneID(
          representativeId: 's9',
          groupPanes: const [
            GroupPaneEntry(id: 'p1', content: SessionContent('s1')),
          ],
          groupRepresentativePaneID: 'p1',
        ),
        'p1',
      );
    });

    test('nil group yields the synthetic solo id', () {
      expect(
        defaultActivePaneID(representativeId: 's1'),
        'solo:s1',
      );
    });
  });

  group('PaneConfirmation', () {
    test('archive title, message, and confirm label', () {
      const c = PaneConfirmation(
        action: PaneConfirmationAction.archive,
        sessionID: 's1',
        label: 'my agent',
        isLive: true,
      );
      expect(c.title, 'Stop and archive session?');
      expect(c.message, contains('“my agent”'));
      expect(c.message, contains('restore and resume later'));
      expect(c.confirmLabel, 'Archive');
      expect(c.id, 's1:archive');
    });

    test('remove title differs for live vs not-live sessions', () {
      const live = PaneConfirmation(
        action: PaneConfirmationAction.remove,
        sessionID: 's1',
        label: 'my agent',
        isLive: true,
      );
      const dead = PaneConfirmation(
        action: PaneConfirmationAction.remove,
        sessionID: 's1',
        label: 'my agent',
        isLive: false,
      );
      expect(live.title, 'Remove session?');
      expect(dead.title, 'Remove from list?');
      expect(live.confirmLabel, 'Remove');
      expect(live.message, contains('does not delete the agent’s conversation'));
    });

    test('value equality', () {
      const a = PaneConfirmation(
        action: PaneConfirmationAction.archive,
        sessionID: 's1',
        label: 'x',
        isLive: true,
      );
      const b = PaneConfirmation(
        action: PaneConfirmationAction.archive,
        sessionID: 's1',
        label: 'x',
        isLive: true,
      );
      expect(a, equals(b));
    });
  });
}
