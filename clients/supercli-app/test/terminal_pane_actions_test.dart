/// Behaviour tests for the terminal pane write-path action decisions.
///
/// Covers `terminal_pane_actions.dart` — the portable mutation logic from
/// `TerminalPaneView.swift` (`TerminalPaneContainer`):
///
/// - `resolveCloseActivePane`: ⌘W resolves the focused leaf, applies the
///   stale-slot guard (unknown session → detach, never an unknown-id Host
///   mutation), and dispatches to detach / remove / confirm-archive.
/// - `resolveActivatePane`: membership guard; solo vs grouped store updates.
/// - `resolveSyncActivePane`: remount restore — re-affirm a valid
///   store-active pane, else fall back to the default.
/// - `resolveFocusRequest`: the five guards before a sidebar focus request
///   activates its pane.
/// - `resolveClaimPendingReveal`: one-shot reveal marker claim.
/// - `resolveZoomedPane`: zoom target resolution.
/// - `liveDividerRatio`: drag-local live ratio vs model ratio.
/// - `shouldDetachPane` / `shouldLaunchIntoPane`: synthetic-pane guards.
/// - `paneBanners`: local-only banner selection.
library;

import 'package:test/test.dart';

import 'package:supercli_app/terminal/terminal_pane_actions.dart';
import 'package:supercli_app/terminal/terminal_pane_model.dart';

PresentedPane _sessionPane(String paneId, String sessionId,
        {bool synthetic = false}) =>
    PresentedPane(
      paneId: paneId,
      content: SessionContent(sessionId),
      isSynthetic: synthetic,
    );

PresentedPane _launcherPane(String paneId) => PresentedPane(
      paneId: paneId,
      content: const LauncherContent(),
      isSynthetic: false,
    );

ClosePaneSessionEntry _entry(String id) =>
    ClosePaneSessionEntry(id: id, label: 'label-$id', isLive: true);

void main() {
  group('resolveCloseActivePane', () {
    final panes = [
      _sessionPane('p1', 's1'),
      _sessionPane('p2', 's2'),
      _launcherPane('p3'),
    ];

    test('no pane for the focused id is a no-op', () {
      final d = resolveCloseActivePane(
        presentedPanes: panes,
        effectiveActivePaneID: 'missing',
        canArchiveSession: (_) => true,
        entryFor: (id) => _entry(id),
      );
      expect(d, isA<CloseActivePaneNone>());
    });

    test('launcher pane detaches', () {
      final d = resolveCloseActivePane(
        presentedPanes: panes,
        effectiveActivePaneID: 'p3',
        canArchiveSession: (_) => true,
        entryFor: (id) => _entry(id),
      );
      expect(d, isA<CloseActivePaneDetach>());
      expect((d as CloseActivePaneDetach).paneId, 'p3');
    });

    test('archivable session raises the confirmation card', () {
      final d = resolveCloseActivePane(
        presentedPanes: panes,
        effectiveActivePaneID: 'p1',
        canArchiveSession: (_) => true,
        entryFor: (id) => _entry(id),
      );
      expect(d, isA<CloseActivePaneConfirmArchive>());
      final c = d as CloseActivePaneConfirmArchive;
      expect(c.sessionId, 's1');
      expect(c.label, 'label-s1');
      expect(c.isLive, isTrue);
    });

    test('disposable session removes directly', () {
      final d = resolveCloseActivePane(
        presentedPanes: panes,
        effectiveActivePaneID: 'p2',
        canArchiveSession: (_) => false,
        entryFor: (id) => _entry(id),
      );
      expect(d, isA<CloseActivePaneRemove>());
      expect((d as CloseActivePaneRemove).sessionId, 's2');
    });

    test('stale slot falls back to detach, never an unknown-id mutation',
        () {
      final d = resolveCloseActivePane(
        presentedPanes: panes,
        effectiveActivePaneID: 'p1',
        canArchiveSession: (_) => true,
        entryFor: (_) => null,
      );
      expect(d, isA<CloseActivePaneDetach>());
      expect((d as CloseActivePaneDetach).paneId, 'p1');
    });

    test('stale disposable slot also detaches', () {
      final d = resolveCloseActivePane(
        presentedPanes: panes,
        effectiveActivePaneID: 'p2',
        canArchiveSession: (_) => false,
        entryFor: (_) => null,
      );
      expect(d, isA<CloseActivePaneDetach>());
    });
  });

  group('resolveActivatePane', () {
    final panes = [_sessionPane('p1', 's1'), _launcherPane('p2')];

    test('unknown pane is ignored', () {
      expect(
        resolveActivatePane(
            paneId: 'nope', presentedPanes: panes, groupId: 'g'),
        isA<ActivatePaneIgnored>(),
      );
    });

    test('solo container remembers the focused solo session', () {
      final d = resolveActivatePane(
          paneId: 'p1', presentedPanes: panes, groupId: null);
      expect(d, isA<ActivatePaneSolo>());
      final s = d as ActivatePaneSolo;
      expect(s.paneId, 'p1');
      expect(s.sessionId, 's1');
    });

    test('solo launcher has no session to remember', () {
      final d = resolveActivatePane(
          paneId: 'p2', presentedPanes: panes, groupId: null);
      expect((d as ActivatePaneSolo).sessionId, isNull);
    });

    test('grouped container sets the store active pane', () {
      final d = resolveActivatePane(
          paneId: 'p1', presentedPanes: panes, groupId: 'g1');
      expect(d, isA<ActivatePaneGrouped>());
      final g = d as ActivatePaneGrouped;
      expect(g.paneId, 'p1');
      expect(g.groupId, 'g1');
      expect(g.sessionId, 's1');
    });
  });

  group('resolveSyncActivePane', () {
    test('solo container resets to the default', () {
      final d = resolveSyncActivePane(
        groupId: null,
        groupPaneIds: const [],
        sessionIdFor: (_) => null,
        storeActivePaneId: 'p9',
        storeActiveGroupId: 'g9',
        defaultActivePaneID: 'solo:s1',
      );
      expect(d, isA<SyncActivePaneResetSolo>());
      expect((d as SyncActivePaneResetSolo).defaultPaneId, 'solo:s1');
    });

    test('valid store-active pane is re-affirmed', () {
      final d = resolveSyncActivePane(
        groupId: 'g1',
        groupPaneIds: const ['p1', 'p2'],
        sessionIdFor: (paneId) => paneId == 'p2' ? 's2' : null,
        storeActivePaneId: 'p2',
        storeActiveGroupId: 'g1',
        defaultActivePaneID: 'p1',
      );
      expect(d, isA<SyncActivePaneReaffirm>());
      final r = d as SyncActivePaneReaffirm;
      expect(r.paneId, 'p2');
      expect(r.groupId, 'g1');
      expect(r.sessionId, 's2');
    });

    test('store-active pane from another group falls back to default', () {
      final d = resolveSyncActivePane(
        groupId: 'g1',
        groupPaneIds: const ['p1', 'p2'],
        sessionIdFor: (_) => null,
        storeActivePaneId: 'p2',
        storeActiveGroupId: 'g2',
        defaultActivePaneID: 'p1',
      );
      expect(d, isA<SyncActivePaneActivateDefault>());
      expect((d as SyncActivePaneActivateDefault).defaultPaneId, 'p1');
    });

    test('store-active pane that left the group falls back to default', () {
      final d = resolveSyncActivePane(
        groupId: 'g1',
        groupPaneIds: const ['p1'],
        sessionIdFor: (_) => null,
        storeActivePaneId: 'p2',
        storeActiveGroupId: 'g1',
        defaultActivePaneID: 'p1',
      );
      expect(d, isA<SyncActivePaneActivateDefault>());
    });
  });

  group('resolveFocusRequest', () {
    String? request({
      String? groupId = 'g1',
      bool pending = true,
    }) =>
        resolveFocusRequest(
          groupId: groupId,
          requestGroupId: 'g1',
          requestPaneId: 'p2',
          groupRepresentativeSessionId: 's1',
          selectedSessionId: 's1',
          groupPaneIds: const ['p1', 'p2'],
          requestStillPending: pending,
        );

    test('valid request returns the pane to activate', () {
      expect(request(), 'p2');
    });

    test('no group rejects', () {
      expect(request(groupId: null), isNull);
    });

    test('stale (consumed) request rejects', () {
      expect(request(pending: false), isNull);
    });

    test('group mismatch rejects', () {
      expect(
        resolveFocusRequest(
          groupId: 'g1',
          requestGroupId: 'g2',
          requestPaneId: 'p2',
          groupRepresentativeSessionId: 's1',
          selectedSessionId: 's1',
          groupPaneIds: const ['p1', 'p2'],
          requestStillPending: true,
        ),
        isNull,
      );
    });

    test('representative mismatch rejects', () {
      expect(
        resolveFocusRequest(
          groupId: 'g1',
          requestGroupId: 'g1',
          requestPaneId: 'p2',
          groupRepresentativeSessionId: 's1',
          selectedSessionId: 's9',
          groupPaneIds: const ['p1', 'p2'],
          requestStillPending: true,
        ),
        isNull,
      );
    });

    test('pane that left the group rejects', () {
      expect(
        resolveFocusRequest(
          groupId: 'g1',
          requestGroupId: 'g1',
          requestPaneId: 'p9',
          groupRepresentativeSessionId: 's1',
          selectedSessionId: 's1',
          groupPaneIds: const ['p1', 'p2'],
          requestStillPending: true,
        ),
        isNull,
      );
    });
  });

  group('resolveClaimPendingReveal', () {
    test('valid pending marker is claimed', () {
      expect(
        resolveClaimPendingReveal(
          groupId: 'g1',
          pendingPaneId: 'p2',
          presentedPaneIds: const ['p1', 'p2'],
        ),
        'p2',
      );
    });

    test('no group, no pending, or missing pane yields nothing', () {
      expect(
        resolveClaimPendingReveal(
          groupId: null,
          pendingPaneId: 'p2',
          presentedPaneIds: const ['p1', 'p2'],
        ),
        isNull,
      );
      expect(
        resolveClaimPendingReveal(
          groupId: 'g1',
          pendingPaneId: null,
          presentedPaneIds: const ['p1', 'p2'],
        ),
        isNull,
      );
      expect(
        resolveClaimPendingReveal(
          groupId: 'g1',
          pendingPaneId: 'p9',
          presentedPaneIds: const ['p1', 'p2'],
        ),
        isNull,
      );
    });
  });

  group('resolveZoomedPane', () {
    test('zoomed pane in this group and present resolves', () {
      expect(
        resolveZoomedPane(
          groupId: 'g1',
          zoomedGroupId: 'g1',
          zoomedPaneId: 'p2',
          presentedPaneIds: const ['p1', 'p2'],
        ),
        'p2',
      );
    });

    test('zoom for another group or a missing pane yields nothing', () {
      expect(
        resolveZoomedPane(
          groupId: 'g1',
          zoomedGroupId: 'g2',
          zoomedPaneId: 'p2',
          presentedPaneIds: const ['p1', 'p2'],
        ),
        isNull,
      );
      expect(
        resolveZoomedPane(
          groupId: 'g1',
          zoomedGroupId: 'g1',
          zoomedPaneId: 'p9',
          presentedPaneIds: const ['p1', 'p2'],
        ),
        isNull,
      );
      expect(
        resolveZoomedPane(
          groupId: null,
          zoomedGroupId: 'g1',
          zoomedPaneId: 'p2',
          presentedPaneIds: const ['p1', 'p2'],
        ),
        isNull,
      );
    });
  });

  group('liveDividerRatio', () {
    test('active drag for the path key wins', () {
      expect(
        liveDividerRatio(
          dragPathKey: 'a,b',
          dragRatio: 0.7,
          pathKey: 'a,b',
          splitRatio: 0.5,
        ),
        0.7,
      );
    });

    test('otherwise the model ratio applies', () {
      expect(
        liveDividerRatio(
          dragPathKey: 'a,c',
          dragRatio: 0.7,
          pathKey: 'a,b',
          splitRatio: 0.5,
        ),
        0.5,
      );
      expect(
        liveDividerRatio(
          dragPathKey: null,
          dragRatio: null,
          pathKey: 'a,b',
          splitRatio: 0.5,
        ),
        0.5,
      );
    });
  });

  group('synthetic guards', () {
    test('synthetic panes cannot detach or host a launch', () {
      expect(shouldDetachPane(isSynthetic: true), isFalse);
      expect(shouldLaunchIntoPane(isSynthetic: true), isFalse);
    });

    test('real panes can detach and host a launch', () {
      expect(shouldDetachPane(isSynthetic: false), isTrue);
      expect(shouldLaunchIntoPane(isSynthetic: false), isTrue);
    });
  });

  group('paneBanners', () {
    test('non-local scope shows nothing', () {
      expect(
        paneBanners(
          isOwnLocalScope: false,
          hasRestartRecommendation: true,
          hasResumeFailure: true,
        ),
        isEmpty,
      );
    });

    test('local scope shows each applicable banner independently', () {
      expect(
        paneBanners(
          isOwnLocalScope: true,
          hasRestartRecommendation: true,
          hasResumeFailure: false,
        ),
        {PaneBannerKind.restartRecommended},
      );
      expect(
        paneBanners(
          isOwnLocalScope: true,
          hasRestartRecommendation: true,
          hasResumeFailure: true,
        ),
        {PaneBannerKind.restartRecommended, PaneBannerKind.resumeFailed},
      );
      expect(
        paneBanners(
          isOwnLocalScope: true,
          hasRestartRecommendation: false,
          hasResumeFailure: false,
        ),
        isEmpty,
      );
    });
  });
}
