/// Behaviour tests for the TerminalPaneView surface-logic port.
///
/// Covers the newly ported portable logic from `TerminalPaneView.swift`
/// (native/SupercliNative, 2,224 lines) that does not depend on SwiftUI:
///
/// - `terminal_pane_rename.dart` — title-chip rename: tooltips, commit
///   trimming, empty/unchanged draft ends without commit, Esc cancels,
///   drag blocked while editing.
/// - `terminal_pane_focus.dart` — active-pane resolution
///   (effectiveActivePaneID), paneIsFocused across main area / project
///   sidebar, the white active-border rule, split-button visibility, the
///   working-spinner gate.
/// - `terminal_pane_archive.dart` — archive verb decision by status;
///   agent-terminal detection for corner gallery controls.
/// - `terminal_pane_menu.dart` — the pane "more" menu model: session menu
///   ordering and visibility conditions, launcher menu sections,
///   transcript submenu ranges, copy-session-id text.
/// - `terminal_pane_split.dart` — split extent math, divider path key,
///   group-edge session-leaf count, launcher/unavailable titles.
library;

import 'package:test/test.dart';

import 'package:supercli_app/terminal/terminal_pane_archive.dart';
import 'package:supercli_app/terminal/terminal_pane_focus.dart';
import 'package:supercli_app/terminal/terminal_pane_menu.dart';
import 'package:supercli_app/terminal/terminal_pane_rename.dart';
import 'package:supercli_app/terminal/terminal_pane_split.dart';

const _fullSessionInput = PaneSessionMenuInput(
  supportsTranscriptCopy: true,
  isAuxiliaryRegion: false,
  sessionIsInProjectSidebar: false,
  canMoveSessionToProjectSidebar: true,
  canResumeAgent: true,
  canRestart: true,
  canRestartApp: true,
  canNotifyWhenDone: true,
  notifyWhenDoneEnabled: false,
  statusIsAttention: true,
  canClearAttention: true,
  canArchive: true,
  isLive: true,
  inGroup: true,
);

String? _label(PaneMenuItem item) => item.label;

List<String?> _labels(List<PaneMenuItem> items) =>
    items.map(_label).toList();

void main() {
  group('TerminalPaneRename.tooltip', () {
    test('editing shows "Edit session title"', () {
      expect(TerminalPaneRename.tooltip(isEditing: true),
          'Edit session title');
    });

    test('idle shows double-click/drag hint', () {
      expect(TerminalPaneRename.tooltip(isEditing: false),
          'Double-click to rename; drag to move');
    });
  });

  group('TerminalPaneRename.endEdit', () {
    test('submit commits a trimmed, changed title', () {
      expect(
        TerminalPaneRename.endEdit(
            draft: '  new title  ', currentLabel: 'old', cancelled: false),
        const RenameCommit('new title'),
      );
    });

    test('unchanged draft just ends editing', () {
      expect(
        TerminalPaneRename.endEdit(
            draft: 'same', currentLabel: 'same', cancelled: false),
        const RenameJustEnd(),
      );
    });

    test('empty draft just ends editing', () {
      expect(
        TerminalPaneRename.endEdit(
            draft: '   \n', currentLabel: 'old', cancelled: false),
        const RenameJustEnd(),
      );
    });

    test('cancel (Esc) ends without committing', () {
      expect(
        TerminalPaneRename.endEdit(
            draft: 'new title', currentLabel: 'old', cancelled: true),
        const RenameJustEnd(),
      );
    });

    test('draft that trims to the current label just ends', () {
      expect(
        TerminalPaneRename.endEdit(
            draft: ' old ', currentLabel: 'old', cancelled: false),
        const RenameJustEnd(),
      );
    });
  });

  group('TerminalPaneRename.dragAllowed', () {
    test('drag allowed when not editing', () {
      expect(TerminalPaneRename.dragAllowed(isEditing: false), isTrue);
    });

    test('drag blocked while editing', () {
      expect(TerminalPaneRename.dragAllowed(isEditing: true), isFalse);
    });
  });

  group('TerminalPaneFocus.effectiveActivePaneID', () {
    test('local active pane wins when presented', () {
      expect(
        TerminalPaneFocus.effectiveActivePaneID(
          activePaneID: 'p2',
          presentedPaneIDs: const ['p1', 'p2'],
          storeActivePaneID: 'p1',
          storeActiveGroupID: 'g',
          groupID: 'g',
          defaultActivePaneID: 'p1',
        ),
        'p2',
      );
    });

    test('falls back to the store pane for the matching group', () {
      expect(
        TerminalPaneFocus.effectiveActivePaneID(
          activePaneID: null,
          presentedPaneIDs: const ['p1', 'p2'],
          storeActivePaneID: 'p2',
          storeActiveGroupID: 'g',
          groupID: 'g',
          defaultActivePaneID: 'p1',
        ),
        'p2',
      );
    });

    test('ignores the store pane when the group does not match', () {
      expect(
        TerminalPaneFocus.effectiveActivePaneID(
          activePaneID: null,
          presentedPaneIDs: const ['p1', 'p2'],
          storeActivePaneID: 'p2',
          storeActiveGroupID: 'other',
          groupID: 'g',
          defaultActivePaneID: 'p1',
        ),
        'p1',
      );
    });

    test('ignores a local active pane that left the group', () {
      expect(
        TerminalPaneFocus.effectiveActivePaneID(
          activePaneID: 'gone',
          presentedPaneIDs: const ['p1'],
          storeActivePaneID: null,
          storeActiveGroupID: null,
          groupID: 'g',
          defaultActivePaneID: 'p1',
        ),
        'p1',
      );
    });
  });

  group('TerminalPaneFocus.paneIsFocused', () {
    test('multi-pane split follows the effective active pane', () {
      expect(
        TerminalPaneFocus.paneIsFocused(
          presentedPaneCount: 2,
          paneID: 'p2',
          effectiveActivePaneID: 'p2',
          storeHoldsActiveTerminalPane: false,
          focusedSoloSessionID: null,
          contentSessionID: 's2',
          isAuxiliaryRegion: false,
        ),
        isTrue,
      );
      expect(
        TerminalPaneFocus.paneIsFocused(
          presentedPaneCount: 2,
          paneID: 'p1',
          effectiveActivePaneID: 'p2',
          storeHoldsActiveTerminalPane: false,
          focusedSoloSessionID: null,
          contentSessionID: 's1',
          isAuxiliaryRegion: false,
        ),
        isFalse,
      );
    });

    test('solo pane is not focused while a split group holds focus', () {
      expect(
        TerminalPaneFocus.paneIsFocused(
          presentedPaneCount: 1,
          paneID: 'p1',
          effectiveActivePaneID: 'p1',
          storeHoldsActiveTerminalPane: true,
          focusedSoloSessionID: null,
          contentSessionID: 's1',
          isAuxiliaryRegion: false,
        ),
        isFalse,
      );
    });

    test('solo pane follows the last clicked solo session', () {
      expect(
        TerminalPaneFocus.paneIsFocused(
          presentedPaneCount: 1,
          paneID: 'p1',
          effectiveActivePaneID: 'p1',
          storeHoldsActiveTerminalPane: false,
          focusedSoloSessionID: 's1',
          contentSessionID: 's1',
          isAuxiliaryRegion: true,
        ),
        isTrue,
      );
    });

    test('solo pane defaults to the main area', () {
      expect(
        TerminalPaneFocus.paneIsFocused(
          presentedPaneCount: 1,
          paneID: 'p1',
          effectiveActivePaneID: 'p1',
          storeHoldsActiveTerminalPane: false,
          focusedSoloSessionID: null,
          contentSessionID: 's1',
          isAuxiliaryRegion: false,
        ),
        isTrue,
      );
      expect(
        TerminalPaneFocus.paneIsFocused(
          presentedPaneCount: 1,
          paneID: 'p1',
          effectiveActivePaneID: 'p1',
          storeHoldsActiveTerminalPane: false,
          focusedSoloSessionID: null,
          contentSessionID: 's1',
          isAuxiliaryRegion: true,
        ),
        isFalse,
      );
    });
  });

  group('TerminalPaneFocus.paneShowsActiveBorder', () {
    test('border follows the focused pane in a multi-pane window', () {
      expect(
        TerminalPaneFocus.paneShowsActiveBorder(
            zoomedPresent: false,
            panePresented: true,
            multiPane: true,
            focused: true),
        isTrue,
      );
    });

    test('no border on unfocused panes', () {
      expect(
        TerminalPaneFocus.paneShowsActiveBorder(
            zoomedPresent: false,
            panePresented: true,
            multiPane: true,
            focused: false),
        isFalse,
      );
    });

    test('no border for a single pane window', () {
      expect(
        TerminalPaneFocus.paneShowsActiveBorder(
            zoomedPresent: false,
            panePresented: true,
            multiPane: false,
            focused: true),
        isFalse,
      );
    });

    test('no border while zoomed', () {
      expect(
        TerminalPaneFocus.paneShowsActiveBorder(
            zoomedPresent: true,
            panePresented: true,
            multiPane: true,
            focused: true),
        isFalse,
      );
    });
  });

  group('TerminalPaneFocus.showsPaneSplitControls', () {
    test('visible on the focused pane', () {
      expect(
        TerminalPaneFocus.showsPaneSplitControls(
            hoveredPaneID: null, paneID: 'p1', focused: true),
        isTrue,
      );
    });

    test('visible on the hovered pane', () {
      expect(
        TerminalPaneFocus.showsPaneSplitControls(
            hoveredPaneID: 'p2', paneID: 'p2', focused: false),
        isTrue,
      );
    });

    test('hidden otherwise', () {
      expect(
        TerminalPaneFocus.showsPaneSplitControls(
            hoveredPaneID: 'p1', paneID: 'p2', focused: false),
        isFalse,
      );
    });
  });

  group('TerminalPaneFocus.paneIsWorking', () {
    test('starting and busy are working', () {
      expect(
        TerminalPaneFocus.paneIsWorking(
            status: PaneSessionStatus.starting,
            restarting: false,
            resumingAgent: false),
        isTrue,
      );
      expect(
        TerminalPaneFocus.paneIsWorking(
            status: PaneSessionStatus.busy,
            restarting: false,
            resumingAgent: false),
        isTrue,
      );
    });

    test('restarting or resuming makes an idle pane working', () {
      expect(
        TerminalPaneFocus.paneIsWorking(
            status: PaneSessionStatus.idle,
            restarting: true,
            resumingAgent: false),
        isTrue,
      );
      expect(
        TerminalPaneFocus.paneIsWorking(
            status: PaneSessionStatus.idle,
            restarting: false,
            resumingAgent: true),
        isTrue,
      );
    });

    test('idle pane is not working', () {
      expect(
        TerminalPaneFocus.paneIsWorking(
            status: PaneSessionStatus.idle,
            restarting: false,
            resumingAgent: false),
        isFalse,
      );
    });
  });

  group('TerminalPaneArchive.paneArchiveDecision', () {
    test('starting/busy/attention need the confirmation card', () {
      for (final status in [
        PaneSessionStatus.starting,
        PaneSessionStatus.busy,
        PaneSessionStatus.attention,
      ]) {
        expect(TerminalPaneArchive.paneArchiveDecision(status),
            PaneArchiveDecision.requestConfirmation,
            reason: '$status');
      }
    });

    test('idle/exited archive directly', () {
      expect(TerminalPaneArchive.paneArchiveDecision(PaneSessionStatus.idle),
          PaneArchiveDecision.requestDirect);
      expect(TerminalPaneArchive.paneArchiveDecision(PaneSessionStatus.exited),
          PaneArchiveDecision.requestDirect);
    });
  });

  group('TerminalPaneArchive.isAgentTerminal', () {
    test('active runtime id marks an agent terminal', () {
      expect(
        TerminalPaneArchive.isAgentTerminal(
            activeRuntimeID: 'claude',
            activeAppName: null,
            setupToolDetected: false),
        isTrue,
      );
    });

    test('active app name marks an agent terminal', () {
      expect(
        TerminalPaneArchive.isAgentTerminal(
            activeRuntimeID: null,
            activeAppName: 'SomeApp',
            setupToolDetected: false),
        isTrue,
      );
    });

    test('setup tool in the command marks an agent terminal', () {
      expect(
        TerminalPaneArchive.isAgentTerminal(
            activeRuntimeID: null,
            activeAppName: null,
            setupToolDetected: true),
        isTrue,
      );
    });

    test('plain shell is not an agent terminal', () {
      expect(
        TerminalPaneArchive.isAgentTerminal(
            activeRuntimeID: '',
            activeAppName: '',
            setupToolDetected: false),
        isFalse,
      );
    });
  });

  group('buildPaneSessionMenu', () {
    test('full menu order matches the Swift paneMenu', () {
      final labels = _labels(buildPaneSessionMenu(_fullSessionInput));
      expect(
        labels,
        [
          'Copy transcript',
          'Copy session ID',
          null, // separator before pin verb
          'Pin to global project sidebar',
          null, // separator before lifecycle
          'Resume Agent',
          'Restart App',
          'Notify when done',
          'Clear attention',
          null, // separator before archive/remove
          'Stop and archive',
          'Remove session',
          null, // separator before group verbs
          'Detach Pane',
          'Exit Multi-Pane View',
        ],
      );
    });

    test('transcript submenu carries the three ranges', () {
      final menu = buildPaneSessionMenu(_fullSessionInput);
      final transcript = menu.first;
      expect(transcript.label, 'Copy transcript');
      expect(
        transcript.submenu,
        [TranscriptCopyRange.last20, TranscriptCopyRange.last50, TranscriptCopyRange.whole],
      );
      expect(TranscriptCopyRange.last20.entryCount, 20);
      expect(TranscriptCopyRange.last50.entryCount, 50);
      expect(TranscriptCopyRange.whole.entryCount, 0);
      expect(TranscriptCopyRange.last20.label, 'Last 20 entries');
      expect(TranscriptCopyRange.whole.label, 'Whole conversation');
    });

    test('notify toggle reflects the enabled state', () {
      final menu = buildPaneSessionMenu(_fullSessionInput);
      final toggle =
          menu.firstWhere((i) => i.label == 'Notify when done');
      expect(toggle.action, PaneMenuAction.toggleNotifyWhenDone);
      expect(toggle.checked, isFalse);
    });

    test('minimal menu: no transcript, no pin, resume fallback, not live', () {
      const input = PaneSessionMenuInput(
        supportsTranscriptCopy: false,
        isAuxiliaryRegion: false,
        sessionIsInProjectSidebar: false,
        canMoveSessionToProjectSidebar: false,
        canResumeAgent: false,
        canRestart: true,
        canRestartApp: false,
        canNotifyWhenDone: false,
        notifyWhenDoneEnabled: false,
        statusIsAttention: false,
        canClearAttention: false,
        canArchive: false,
        isLive: false,
        inGroup: false,
      );
      final labels = _labels(buildPaneSessionMenu(input));
      expect(
        labels,
        [
          'Copy session ID',
          null,
          'Resume',
          null,
          'Remove from list',
        ],
      );
    });

    test('non-archivable live session removes directly', () {
      const input = PaneSessionMenuInput(
        supportsTranscriptCopy: false,
        isAuxiliaryRegion: false,
        sessionIsInProjectSidebar: false,
        canMoveSessionToProjectSidebar: false,
        canResumeAgent: false,
        canRestart: false,
        canRestartApp: false,
        canNotifyWhenDone: false,
        notifyWhenDoneEnabled: false,
        statusIsAttention: false,
        canClearAttention: false,
        canArchive: false,
        isLive: true,
        inGroup: false,
      );
      final menu = buildPaneSessionMenu(input);
      final remove =
          menu.firstWhere((i) => i.label == 'Remove session');
      expect(remove.action, PaneMenuAction.removeSessionDirect);
    });

    test('archivable session routes remove through confirmation', () {
      final menu = buildPaneSessionMenu(_fullSessionInput);
      final remove =
          menu.firstWhere((i) => i.label == 'Remove session');
      expect(remove.action, PaneMenuAction.removeSessionConfirm);
    });

    test('auxiliary pane in the sidebar offers unpin', () {
      const input = PaneSessionMenuInput(
        supportsTranscriptCopy: false,
        isAuxiliaryRegion: true,
        sessionIsInProjectSidebar: true,
        canMoveSessionToProjectSidebar: false,
        canResumeAgent: false,
        canRestart: false,
        canRestartApp: false,
        canNotifyWhenDone: false,
        notifyWhenDoneEnabled: false,
        statusIsAttention: false,
        canClearAttention: false,
        canArchive: false,
        isLive: false,
        inGroup: false,
      );
      final labels = _labels(buildPaneSessionMenu(input));
      expect(labels, contains('Unpin from global project sidebar'));
      expect(labels, isNot(contains('Pin to global project sidebar')));
    });

    test('not-live archivable session says "Archive"', () {
      final menu = buildPaneSessionMenu(const PaneSessionMenuInput(
        supportsTranscriptCopy: false,
        isAuxiliaryRegion: false,
        sessionIsInProjectSidebar: false,
        canMoveSessionToProjectSidebar: false,
        canResumeAgent: false,
        canRestart: false,
        canRestartApp: false,
        canNotifyWhenDone: false,
        notifyWhenDoneEnabled: false,
        statusIsAttention: false,
        canClearAttention: false,
        canArchive: true,
        isLive: false,
        inGroup: false,
      ));
      expect(menu.map(_label), contains('Archive'));
      expect(menu.map(_label), contains('Remove from list'));
    });
  });

  group('buildPaneLauncherMenu', () {
    test('launcher menu sections match the Swift menu', () {
      final menu = buildPaneLauncherMenu(
        agentPresetLabels: const ['Claude'],
        pluginPresetLabels: const ['Git'],
      );
      expect(
        _labels(menu),
        [
          'New Terminal',
          null,
          'Agents',
          'Claude',
          'Manage Agents…',
          null,
          'Plugins',
          'Git',
          'Manage Plugins…',
        ],
      );
    });
  });

  group('copySessionIdText', () {
    test('formats the clipboard text like Swift', () {
      expect(copySessionIdText('abc-123'), 'Supercli Session ID: abc-123');
    });
  });

  group('TerminalPaneSplitLayout.splitPaneExtents', () {
    test('extent minus divider splits by ratio', () {
      // total 1000, divider 8 -> extent 992; ratio 0.5.
      final e = TerminalPaneSplitLayout.splitPaneExtents(
          totalExtent: 1000, dividerWidth: 8, ratio: 0.5);
      expect(e.first, closeTo(496, 1e-9));
      expect(e.second, closeTo(496, 1e-9));
    });

    test('children sum to the available extent', () {
      final e = TerminalPaneSplitLayout.splitPaneExtents(
          totalExtent: 640, dividerWidth: 8, ratio: 0.25);
      expect(e.first + e.second, closeTo(632, 1e-9));
    });

    test('negative extent clamps to zero', () {
      final e = TerminalPaneSplitLayout.splitPaneExtents(
          totalExtent: 4, dividerWidth: 8, ratio: 0.5);
      expect(e, const SplitPaneExtents(first: 0, second: 0));
    });
  });

  group('TerminalPaneSplitLayout.splitPathKey', () {
    test('joins path components with commas', () {
      expect(
        TerminalPaneSplitLayout.splitPathKey(const ['left', 'right']),
        'left,right',
      );
      expect(TerminalPaneSplitLayout.splitPathKey(const []), '');
    });
  });

  group('TerminalPaneSplitLayout.existingSessionLeafCount', () {
    test('solo terminal counts as one', () {
      expect(
        TerminalPaneSplitLayout.existingSessionLeafCount(
            hasSelectedSessionGroup: false, groupSessionLeafCount: 7),
        1,
      );
    });

    test('group uses its durable session-leaf count', () {
      expect(
        TerminalPaneSplitLayout.existingSessionLeafCount(
            hasSelectedSessionGroup: true, groupSessionLeafCount: 3),
        3,
      );
    });

    test('floors at one', () {
      expect(
        TerminalPaneSplitLayout.existingSessionLeafCount(
            hasSelectedSessionGroup: true, groupSessionLeafCount: 0),
        1,
      );
    });
  });

  group('pane titles', () {
    test('launcher title depends on the pending state', () {
      expect(TerminalPaneSplitLayout.launcherPaneTitle(starting: true),
          'Starting…');
      expect(TerminalPaneSplitLayout.launcherPaneTitle(starting: false),
          'New pane');
    });

    test('unavailable pane title', () {
      expect(TerminalPaneSplitLayout.unavailablePaneTitle,
          'Session unavailable');
    });
  });
}
