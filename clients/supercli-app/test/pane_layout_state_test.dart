/// Behavior tests for the normative multi-group pane-layout state model.
///
/// Mirrors `PaneLayoutStateTests.swift` (id canonicalization, structural
/// identity, clamping, error paths, codec) and runs the shared
/// `protocol/pane-layout-operations-v1.json` fixture — the normative
/// cross-implementation contract (same file the Swift
/// `PaneLayoutOperationsConformanceTests` and the TUI run).
library;

import 'dart:convert';
import 'dart:io';

import 'package:supercli_app/pane_layout.dart' show SplitDirection;
import 'package:supercli_app/pane_layout_state.dart';
import 'package:test/test.dart';

const String _paneA = '00000000-0000-4000-8000-00000000000a';
const String _paneB = '00000000-0000-4000-8000-00000000000b';
const String _paneC = '00000000-0000-4000-8000-00000000000c';
const String _groupID = '11111111-0000-4000-8000-000000000001';

LayoutPaneLayoutState twoPaneState() => LayoutPaneLayoutState(
  groups: [
    LayoutPaneGroup(
      id: _groupID,
      representativePaneID: _paneA,
      root: LayoutPaneSplitNode(
        LayoutPaneSplit(
          direction: SplitDirection.horizontal,
          ratio: 0.6,
          left: LayoutPaneLeaf(
            LayoutPane(id: _paneA, content: const PaneSession('s1')),
          ),
          right: LayoutPaneLeaf(
            LayoutPane(id: _paneB, content: const PaneSession('s2')),
          ),
        ),
      ),
    ),
  ],
);

PaneLayoutErrorKind throwsKind(void Function() fn) {
  try {
    fn();
  } on PaneLayoutException catch (e) {
    return e.kind;
  }
  fail('expected PaneLayoutException');
}

String fixturePath(String name) {
  final override = Platform.environment['SUPERCLI_PROTOCOL_DIR'];
  if (override != null && override.isNotEmpty) return '$override/$name';
  // Probe upward for <repo>/protocol, from the cwd and from this file,
  // mirroring wire_fixtures_test.dart.
  File probe(Directory root) => File('${root.path}/protocol/$name');
  var dir = Directory.current;
  for (var i = 0; i < 8; i++) {
    if (probe(dir).existsSync()) return probe(dir).path;
    dir = dir.parent;
  }
  var fdir = File(Platform.script.toFilePath()).parent;
  for (var i = 0; i < 8; i++) {
    if (probe(fdir).existsSync()) return probe(fdir).path;
    fdir = fdir.parent;
  }
  throw StateError(
    'protocol/$name not found (cwd=${Directory.current.path})',
  );
}

void main() {
  group('PaneStableID', () {
    test('non-UUID pane ids are replaced with canonical ones', () {
      final pane = LayoutPane(id: 'p1', content: const PaneSession('s1'));
      expect(pane.id, isNot('p1'));
      expect(PaneStableID.canonical(pane.id), isNotNull);
      expect(pane.id, pane.id.toLowerCase());
    });

    test('uppercase UUID input is lowercased', () {
      final pane = LayoutPane(
        id: _paneA.toUpperCase(),
        content: const PaneSession('s1'),
      );
      expect(pane.id, _paneA);
    });

    test('make() produces unique valid UUIDs', () {
      final a = PaneStableID.make();
      final b = PaneStableID.make();
      expect(a, isNot(b));
      expect(PaneStableID.canonical(a), a);
      expect(PaneStableID.canonical('not-a-uuid'), isNull);
    });
  });

  group('structuralKey', () {
    test('ignores ratio changes', () {
      final state = twoPaneState();
      final before = state.group(_groupID)!.root.structuralKey;
      state.resizeSplit(_groupID, const LayoutPaneSplitPath(), 0.31);
      expect(state.group(_groupID)!.root.structuralKey, before);
      state.equalize(_groupID);
      expect(state.group(_groupID)!.root.structuralKey, before);
    });

    test('tracks shape and content', () {
      final state = twoPaneState();
      final before = state.group(_groupID)!.root.structuralKey;
      state.insertSessionSplitting(
        sessionID: 's3',
        targetPaneID: _paneB,
        edge: LayoutPaneEdge.down,
        newPaneID: _paneC,
      );
      expect(state.group(_groupID)!.root.structuralKey, isNot(before));
      state.detachPane(_paneC);
      expect(state.group(_groupID)!.root.structuralKey, before);
    });
  });

  group('gridDimensions', () {
    test('leaf is 1x1; splits sum along their axis', () {
      final leaf = LayoutPaneLeaf(
        LayoutPane(content: const PaneSession('s1')),
      );
      expect(leaf.gridDimensions, (1.0, 1.0));
      final sideBySide = LayoutPaneSplitNode(
        LayoutPaneSplit(
          direction: SplitDirection.horizontal,
          ratio: 0.5,
          left: leaf,
          right: LayoutPaneLeaf(
            LayoutPane(content: const PaneSession('s2')),
          ),
        ),
      );
      expect(sideBySide.gridDimensions, (2.0, 1.0));
      final stacked = LayoutPaneSplitNode(
        LayoutPaneSplit(
          direction: SplitDirection.vertical,
          ratio: 0.5,
          left: leaf,
          right: LayoutPaneLeaf(
            LayoutPane(content: const PaneSession('s3')),
          ),
        ),
      );
      expect(stacked.gridDimensions, (1.0, 2.0));
    });
  });

  group('clamping', () {
    test('split init clamps ratio', () {
      LayoutPaneSplit split(double ratio) => LayoutPaneSplit(
        direction: SplitDirection.horizontal,
        ratio: ratio,
        left: LayoutPaneLeaf(
          LayoutPane(id: _paneA, content: const PaneSession('s1')),
        ),
        right: LayoutPaneLeaf(
          LayoutPane(id: _paneB, content: const PaneSession('s2')),
        ),
      );
      expect(split(0.02).ratio, layoutMinSplitRatio);
      expect(split(0.99).ratio, layoutMaxSplitRatio);
      expect(split(double.nan).ratio, 0.5);
    });

    test('resize rejects non-finite ratio', () {
      final state = twoPaneState();
      expect(
        throwsKind(
          () => state.resizeSplit(
            _groupID,
            const LayoutPaneSplitPath(),
            double.nan,
          ),
        ),
        PaneLayoutErrorKind.invalidRatio,
      );
    });
  });

  group('error paths', () {
    test('empty session ids are rejected', () {
      final state = twoPaneState();
      expect(
        throwsKind(
          () => state.insertSessionSplitting(
            sessionID: '',
            targetPaneID: _paneA,
            edge: LayoutPaneEdge.right,
          ),
        ),
        PaneLayoutErrorKind.invalidSessionID,
      );
      expect(
        throwsKind(
          () => state.insertSession(
            sessionID: 's9',
            besideSessionID: '',
            edge: LayoutPaneEdge.right,
          ),
        ),
        PaneLayoutErrorKind.invalidSessionID,
      );
    });

    test('createGroup rejects same session', () {
      final state = LayoutPaneLayoutState();
      expect(
        throwsKind(
          () => state.createGroup(
            representativeSessionID: 's1',
            addingSessionID: 's1',
            edge: LayoutPaneEdge.right,
          ),
        ),
        PaneLayoutErrorKind.sameSession,
      );
    });

    test('bind rejects non-launcher pane', () {
      final state = twoPaneState();
      expect(
        throwsKind(() => state.bindLauncher(_paneA, 's9')),
        PaneLayoutErrorKind.paneIsNotLauncher,
      );
    });

    test('duplicate session is rejected', () {
      final state = twoPaneState();
      expect(
        throwsKind(
          () => state.insertSessionSplitting(
            sessionID: 's1',
            targetPaneID: _paneB,
            edge: LayoutPaneEdge.right,
          ),
        ),
        PaneLayoutErrorKind.duplicateSession,
      );
    });
  });

  group('preorder', () {
    test('leaves enumerate in preorder', () {
      final root = LayoutPaneSplitNode(
        LayoutPaneSplit(
          direction: SplitDirection.vertical,
          ratio: 0.5,
          left: LayoutPaneSplitNode(
            LayoutPaneSplit(
              direction: SplitDirection.horizontal,
              ratio: 0.5,
              left: LayoutPaneLeaf(
                LayoutPane(id: _paneA, content: const PaneSession('s1')),
              ),
              right: LayoutPaneLeaf(
                LayoutPane(id: _paneB, content: const PaneSession('s2')),
              ),
            ),
          ),
          right: LayoutPaneLeaf(
            LayoutPane(id: _paneC, content: const PaneSession('s3')),
          ),
        ),
      );
      expect(root.leaves.map((pane) => pane.id).toList(), [
        _paneA,
        _paneB,
        _paneC,
      ]);
    });
  });

  group('launcher lifecycle', () {
    test('insert/bind/remove launcher round trip', () {
      final state = twoPaneState();
      final location = state.insertLauncher(
        projectID: 'proj1',
        splittingPaneID: _paneA,
        edge: LayoutPaneEdge.right,
        newPaneID: _paneC,
      );
      expect(location.paneID, _paneC);
      expect(state.group(_groupID)!.root.containsLauncher, isTrue);
      // Capacity counts the launcher prospectively.
      expect(
        throwsKind(
          () => state.insertLauncher(
            projectID: 'proj2',
            splittingPaneID: _paneB,
            edge: LayoutPaneEdge.right,
          ),
        ),
        PaneLayoutErrorKind.launcherAlreadyPresent,
      );
      state.bindLauncher(_paneC, 's3');
      expect(state.group(_groupID)!.root.containsLauncher, isFalse);
      expect(state.locationOfSession('s3')!.paneID, _paneC);
      expect(state.group(_groupID)!.panes, hasLength(3));
    });

    test('removeLauncher cancels back to the snapshot', () {
      final state = twoPaneState();
      final before = state.group(_groupID)!.root.structuralKey;
      final location = state.insertLauncher(
        projectID: 'proj1',
        splittingPaneID: _paneA,
        edge: LayoutPaneEdge.right,
        newPaneID: _paneC,
      );
      state.removeLauncher(location.paneID);
      expect(state.group(_groupID)!.root.structuralKey, before);
      expect(state.group(_groupID)!.root.containsLauncher, isFalse);
    });
  });

  group('detach', () {
    test('detaching a leaf collapses the parent split', () {
      final state = twoPaneState();
      state.insertSessionSplitting(
        sessionID: 's3',
        targetPaneID: _paneB,
        edge: LayoutPaneEdge.down,
        newPaneID: _paneC,
      );
      final change = state.detachPane(_paneC);
      expect(change.dissolved, isFalse);
      expect(change.removedPaneIDs, [_paneC]);
      expect(change.releasedSessionIDs, ['s3']);
      expect(
        state.group(_groupID)!.root.leaves.map((pane) => pane.id).toList(),
        [_paneA, _paneB],
      );
    });

    test('detaching from a two-pane group dissolves it', () {
      final state = twoPaneState();
      final change = state.detachPane(_paneB);
      expect(change.dissolved, isTrue);
      expect(change.representativePaneID, isNull);
      expect(state.groups, isEmpty);
    });

    test('representative is promoted when it disappears', () {
      final state = twoPaneState();
      final change = state.detachPane(_paneA);
      expect(change.dissolved, isTrue);
      // Single remaining session cannot stay grouped.
      expect(state.groups, isEmpty);
    });
  });

  group('swap', () {
    test('swapPanes exchanges leaf positions', () {
      final state = twoPaneState();
      state.insertSessionSplitting(
        sessionID: 's3',
        targetPaneID: _paneB,
        edge: LayoutPaneEdge.down,
        newPaneID: _paneC,
      );
      expect(state.swapPanes(_paneA, _paneC), isTrue);
      final leaves = state.group(_groupID)!.root.leaves;
      expect(leaves.map((pane) => pane.content.sessionID).toList(), [
        's3',
        's2',
        's1',
      ]);
      // Representative id is unaffected (travels with the leaf).
      expect(state.group(_groupID)!.representativePaneID, _paneA);
    });

    test('swapping a pane with itself returns false', () {
      final state = twoPaneState();
      expect(state.swapPanes(_paneA, _paneA), isFalse);
    });
  });

  group('reconcile', () {
    test('drops ineligible sessions and dissolves small groups', () {
      final state = twoPaneState();
      final changes = state.reconcile({'s1'});
      expect(changes, hasLength(1));
      expect(changes.first.dissolved, isTrue);
      expect(state.groups, isEmpty);
    });

    test('keeps groups with two eligible sessions', () {
      final state = twoPaneState();
      final changes = state.reconcile({'s1', 's2'});
      expect(changes, isEmpty);
      expect(state.groups, hasLength(1));
    });
  });

  group('spatial neighbor', () {
    test('uses grid dimensions, not ratios', () {
      final state = twoPaneState();
      state.resizeSplit(_groupID, const LayoutPaneSplitPath(), 0.1);
      expect(
        state.spatialNeighbor(_paneA, LayoutPaneEdge.right)?.id,
        _paneB,
      );
      expect(
        state.spatialNeighbor(_paneB, LayoutPaneEdge.left)?.id,
        _paneA,
      );
      expect(state.spatialNeighbor(_paneA, LayoutPaneEdge.up), isNull);
      expect(state.spatialNeighbor(_paneA, LayoutPaneEdge.down), isNull);
    });
  });

  group('durable codec', () {
    test('v1 decodes legacy Rust key spellings and migrates', () {
      final json = {
        'version': 1,
        'groups': [
          {
            'id': _groupID,
            'representativePaneId': _paneA,
            'panes': [
              {'id': _paneA, 'sessionId': 's1', 'fraction': 0.7},
              {'id': _paneB, 'sessionId': 's2', 'fraction': 0.3},
            ],
          },
        ],
      };
      final durable = DurablePaneLayout.fromJson(json);
      final state = durable.restoredState();
      final group = state.groups.first;
      expect(group.representativePaneID, _paneA);
      expect(group.sessionIDs, ['s1', 's2']);
      final root = group.root;
      expect(root, isA<LayoutPaneSplitNode>());
      final split = (root as LayoutPaneSplitNode).split;
      expect(split.direction, SplitDirection.horizontal);
      expect(split.ratio, closeTo(0.7, 1e-9));
    });

    test('unknown future version decodes empty but keeps version', () {
      final durable = DurablePaneLayout.fromJson(
        jsonDecode('{"version": 99, "groups": [{"whatever": true}]}')
            as Map<String, Object?>,
      );
      expect(durable.version, 99);
      expect(durable.groups, isEmpty);
      expect(DurablePaneLayout.supportsVersion(durable.version), isFalse);
    });

    test('encode always writes the current version', () {
      final state = twoPaneState();
      final encoded = DurablePaneLayout.fromState(state).toJson();
      expect(encoded['version'], DurablePaneLayout.currentVersion);
      final groups = encoded['groups'] as List;
      final root = (groups.first as Map)['root'] as Map;
      expect(root.containsKey('split'), isTrue);
    });

    test('durable round trip preserves ids and geometry', () {
      final state = twoPaneState();
      state.insertSessionSplitting(
        sessionID: 's3',
        targetPaneID: _paneB,
        edge: LayoutPaneEdge.down,
        newPaneID: _paneC,
      );
      state.resizeSplit(
        _groupID,
        LayoutPaneSplitPath(const [LayoutPaneSplitBranch.right]),
        0.42,
      );

      final json = DurablePaneLayout.fromState(state).toJson();
      final decoded = DurablePaneLayout.fromJson(
        jsonDecode(jsonEncode(json)) as Map<String, Object?>,
      );
      final restored = decoded.restoredState();

      expect(
        restored.groups.map((group) => group.id).toList(),
        state.groups.map((group) => group.id).toList(),
      );
      expect(
        restored.groups.expand((group) => group.panes).map((pane) => pane.id),
        state.groups.expand((group) => group.panes).map((pane) => pane.id),
      );
      expect(
        restored.group(_groupID)!.root.structuralKey,
        state.group(_groupID)!.root.structuralKey,
      );
    });

    test('launchers are omitted from the durable form', () {
      final state = twoPaneState();
      state.insertLauncher(
        projectID: 'proj1',
        splittingPaneID: _paneA,
        edge: LayoutPaneEdge.right,
        newPaneID: _paneC,
      );
      // A group with a live launcher + snapshot persists the snapshot.
      final durable = DurablePaneLayout.fromState(state);
      expect(durable.groups, hasLength(1));
      expect(
        durable.groups.first.root.paneNode().containsLauncher,
        isFalse,
      );
    });
  });

  group('conformance fixture', () {
    test('runs protocol/pane-layout-operations-v1.json', () {
      final path = fixturePath('pane-layout-operations-v1.json');
      expect(File(path).existsSync(), isTrue, reason: 'missing $path');
      final root =
          jsonDecode(File(path).readAsStringSync()) as Map<String, dynamic>;
      final cases = (root['cases'] as List).cast<Map<String, dynamic>>();
      expect(cases, isNotEmpty);
      for (final testCase in cases) {
        _runCase(testCase);
      }
    });
  });
}

void _runCase(Map<String, dynamic> testCase) {
  final caseID = testCase['id'] as String;
  final initial = testCase['initial'] as Map<String, dynamic>;
  final durable = DurablePaneLayout.fromJson(
    jsonDecode(jsonEncode(initial)) as Map<String, Object?>,
  );
  final state = durable.restoredState();

  final operations = (testCase['operations'] as List? ?? [])
      .cast<Map<String, dynamic>>();
  final expect = testCase['expect'] as Map<String, dynamic>;
  final expectedError = expect['error'] as String?;
  String? focusResult;

  for (var index = 0; index < operations.length; index++) {
    final operation = operations[index];
    final isLast = index == operations.length - 1;
    try {
      final result = _apply(operation, state, caseID);
      if (result != null) focusResult = result;
      if (isLast && expectedError != null) {
        fail('$caseID: expected error $expectedError, got success');
      }
    } on PaneLayoutException catch (e) {
      if (!isLast || expectedError == null) {
        fail('$caseID: unexpected error ${e.kind} at op $index');
      }
      expect(e.kind.name, expectedError, reason: '$caseID: error kind');
      return;
    }
  }
  if (expectedError != null) {
    fail('$caseID: expected error but no operation failed');
  }

  if (expect.containsKey('focusPaneID')) {
    expect(
      focusResult,
      expect['focusPaneID'],
      reason: '$caseID: focusPaneID',
    );
  }

  final layoutJson = expect['layout'] as Map<String, dynamic>?;
  if (layoutJson != null) {
    final expected = DurablePaneLayout.fromJson(
      jsonDecode(jsonEncode(layoutJson)) as Map<String, Object?>,
    );
    final actual = DurablePaneLayout.fromState(state);
    _assertLayoutsEqual(actual, expected, caseID);
  }

  final liveLeaves = expect['expectLiveLeaves'] as List?;
  if (liveLeaves != null) {
    final actual = state.groups.expand((group) => group.panes).toList();
    expect(actual.length, liveLeaves.length, reason: '$caseID: live leaves');
    for (var i = 0; i < actual.length; i++) {
      final expectedLeaf = liveLeaves[i] as Map<String, dynamic>;
      expect(
        actual[i].id,
        expectedLeaf['paneID'],
        reason: '$caseID: live leaf pane id',
      );
      final sessionID = expectedLeaf['sessionID'] as String?;
      if (sessionID != null) {
        expect(
          actual[i].content.sessionID,
          sessionID,
          reason: '$caseID: live leaf session',
        );
      }
      final projectID = expectedLeaf['launcherProjectID'] as String?;
      if (projectID != null) {
        final content = actual[i].content;
        expect(
          content,
          isA<PaneLauncher>(),
          reason: '$caseID: expected launcher leaf ${actual[i].id}',
        );
        expect(
          (content as PaneLauncher).projectID,
          projectID,
          reason: '$caseID: launcher project',
        );
      }
    }
  }
}

/// Applies one fixture operation. Returns the focus query result for
/// `focusNeighbor`, null otherwise.
String? _apply(
  Map<String, dynamic> operation,
  LayoutPaneLayoutState state,
  String caseID,
) {
  final op = operation['op'] as String? ?? '';
  LayoutPaneEdge edge(String? key) {
    final parsed = LayoutPaneEdge.fromName(operation[key] as String?);
    if (parsed == null) fail('$caseID: bad edge ${operation[key]}');
    return parsed;
  }

  switch (op) {
    case 'insertSession':
      final sessionID = operation['sessionID'] as String? ?? '';
      final targetPaneID = operation['targetPaneID'] as String?;
      final groupID = operation['groupID'] as String?;
      if (targetPaneID != null) {
        state.insertSessionSplitting(
          sessionID: sessionID,
          targetPaneID: targetPaneID,
          edge: edge('edge'),
          newPaneID: operation['newPaneID'] as String?,
        );
      } else if (groupID != null) {
        state.insertSessionAtGroupEdge(
          sessionID: sessionID,
          edge: edge('groupEdge'),
          groupID: groupID,
          newPaneID: operation['newPaneID'] as String?,
        );
      } else {
        state.insertSession(
          sessionID: sessionID,
          besideSessionID: operation['besideSessionID'] as String? ?? '',
          edge: edge('edge'),
          newGroupID: operation['newGroupID'] as String?,
          newRepresentativePaneID:
              operation['newRepresentativePaneID'] as String?,
          newPaneID: operation['newPaneID'] as String?,
        );
      }
    case 'insertLauncher':
      state.insertLauncher(
        projectID: operation['projectID'] as String? ?? '',
        splittingPaneID: operation['targetPaneID'] as String? ?? '',
        edge: edge('edge'),
        newPaneID: operation['newPaneID'] as String?,
      );
    case 'bindLauncher':
      state.bindLauncher(
        operation['paneID'] as String? ?? '',
        operation['sessionID'] as String? ?? '',
      );
    case 'removeLauncher':
      state.removeLauncher(operation['paneID'] as String? ?? '');
    case 'detachPane':
      state.detachPane(operation['paneID'] as String? ?? '');
    case 'closeGroup':
      state.closeGroup(operation['groupID'] as String? ?? '');
    case 'resizeSplit':
      final components = ((operation['path'] as List?) ?? []).map((e) {
        final branch = LayoutPaneSplitBranch.fromName(e as String);
        if (branch == null) fail('$caseID: bad path branch $e');
        return branch;
      }).toList();
      state.resizeSplit(
        operation['groupID'] as String? ?? '',
        LayoutPaneSplitPath(components),
        (operation['ratio'] as num?)?.toDouble() ?? double.nan,
      );
    case 'equalize':
      state.equalize(operation['groupID'] as String? ?? '');
    case 'swapPanes':
      state.swapPanes(
        operation['paneID'] as String? ?? '',
        operation['otherPaneID'] as String? ?? '',
      );
    case 'reconcile':
      state.reconcile(
        ((operation['eligibleSessionIDs'] as List?) ?? [])
            .map((e) => e as String)
            .toSet(),
      );
    case 'focusNeighbor':
      return state
          .spatialNeighbor(
            operation['paneID'] as String? ?? '',
            edge('direction'),
          )
          ?.id;
    default:
      fail('$caseID: unknown op $op');
  }
  return null;
}

void _assertLayoutsEqual(
  DurablePaneLayout actual,
  DurablePaneLayout expected,
  String caseID,
) {
  expect(actual.groups.length, expected.groups.length,
      reason: '$caseID: group count');
  for (var i = 0; i < actual.groups.length; i++) {
    final actualGroup = actual.groups[i];
    final expectedGroup = expected.groups[i];
    expect(actualGroup.id, expectedGroup.id, reason: '$caseID: group id');
    expect(
      actualGroup.representativePaneID,
      expectedGroup.representativePaneID,
      reason: '$caseID: representative',
    );
    _assertNodesEqual(actualGroup.root, expectedGroup.root, caseID, 'root');
  }
}

void _assertNodesEqual(
  DurablePaneNode actual,
  DurablePaneNode expected,
  String caseID,
  String path,
) {
  if (actual is DurablePaneLeaf && expected is DurablePaneLeaf) {
    expect(actual.id, expected.id, reason: '$caseID: $path pane id');
    expect(actual.sessionID, expected.sessionID,
        reason: '$caseID: $path session');
  } else if (actual is DurablePaneSplit && expected is DurablePaneSplit) {
    expect(actual.direction, expected.direction,
        reason: '$caseID: $path direction');
    expect(actual.ratio, closeTo(expected.ratio, 1e-9),
        reason: '$caseID: $path ratio');
    _assertNodesEqual(actual.left, expected.left, caseID, '$path.left');
    _assertNodesEqual(actual.right, expected.right, caseID, '$path.right');
  } else {
    fail('$caseID: $path node kind mismatch');
  }
}
