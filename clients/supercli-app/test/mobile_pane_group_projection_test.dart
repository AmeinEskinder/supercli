/// Tests for [MobilePaneGroupProjection].
///
/// Port of `MobilePaneGroupProjectionTests.swift` from
/// `native/SupercliNative/Tests/SupercliNativeTests`.
library;

import 'package:test/test.dart';

import '../lib/pane_layout_state.dart';
import '../lib/terminal/mobile_pane_group_projection.dart';

void main() {
  group('MobilePaneGroupProjection.summaries', () {
    test('summaries preserve representative and preorder session IDs', () {
      final state = LayoutPaneLayoutState();
      final location = state.createGroup(
        representativeSessionID: 'session-main',
        addingSessionID: 'session-notes',
        edge: LayoutPaneEdge.right,
      );

      final summaries = MobilePaneGroupProjection.summaries(state);
      expect(summaries, isNotNull);
      expect(summaries!.length, 1);
      expect(summaries[0].id, location.groupID);
      expect(summaries[0].representativeSessionID, 'session-main');
      expect(summaries[0].sessionIDs, ['session-main', 'session-notes']);
    });

    test('empty layout omits projection', () {
      expect(
        MobilePaneGroupProjection.summaries(LayoutPaneLayoutState()),
        isNull,
      );
    });

    test('single-session groups are omitted', () {
      final state = LayoutPaneLayoutState();
      // A fresh state has no groups; summaries must be null.
      expect(MobilePaneGroupProjection.summaries(state), isNull);
    });
  });

  group('MobilePaneGroupProjection.scopeIDForSelectionKey', () {
    test('workspace selection keys map to controller pane scopes', () {
      expect(
        MobilePaneGroupProjection.scopeIDForSelectionKey(null),
        'local',
      );
      expect(
        MobilePaneGroupProjection.scopeIDForSelectionKey('local:/tmp/client'),
        'workspace:/tmp/client',
      );
      expect(
        MobilePaneGroupProjection.scopeIDForSelectionKey('ssh:linux-host'),
        'host:linux-host',
      );
      expect(
        MobilePaneGroupProjection.scopeIDForSelectionKey('host:paired-mac'),
        'host:paired-mac',
      );
      expect(
        MobilePaneGroupProjection.scopeIDForSelectionKey('local:'),
        isNull,
      );
      expect(
        MobilePaneGroupProjection.scopeIDForSelectionKey('unknown:x'),
        isNull,
      );
    });
  });
}
