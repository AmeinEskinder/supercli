/// Tests for [appkit_delta.dart]: the UIDelta wire protocol decode side.
///
/// Swift source: `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIDelta.swift`
/// (`UIDeltaOperation` Codable, `UIDelta` init validation).
library;

import 'package:test/test.dart';

import 'package:supercli_app/widgets/appkit_delta.dart';

/// Minimal valid fixture per wire `op` name.
final Map<String, Map<String, dynamic>> opFixtures = {
  'replaceRoot': {
    'op': 'replaceRoot',
    'root': {'id': 'r', 'type': 'page', 'title': 't'},
  },
  'markdownReplaceRange': {
    'op': 'markdownReplaceRange',
    'nodeId': 'm',
    'edit': {'range': [0, 1]},
  },
  'markdownSetSelection': {
    'op': 'markdownSetSelection',
    'nodeId': 'm',
    'selection': {'anchor': 0},
  },
  'markdownSetPresentation': {
    'op': 'markdownSetPresentation',
    'nodeId': 'm',
    'presentation': {'mode': 'edit'},
  },
  'markdownSetDirty': {'op': 'markdownSetDirty', 'nodeId': 'm', 'dirty': true},
  'markdownSetReadOnly': {
    'op': 'markdownSetReadOnly',
    'nodeId': 'm',
    'readOnly': false
  },
  'markdownSetTitle': {
    'op': 'markdownSetTitle',
    'nodeId': 'm',
    'title': 'hello',
  },
  'markdownSetPlaceholder': {
    'op': 'markdownSetPlaceholder',
    'nodeId': 'm',
    'placeholder': 'type…'
  },
  'markdownSetCommandHint': {
    'op': 'markdownSetCommandHint',
    'nodeId': 'm',
    'commandHint': {'text': 'hint'}
  },
  'markdownSetActions': {
    'op': 'markdownSetActions',
    'nodeId': 'm',
    'actions': {'replaceRange': 'act'}
  },
  'markdownSetMenus': {
    'op': 'markdownSetMenus',
    'nodeId': 'm',
    'insertMenu': {'id': 'im', 'items': []},
    'contextMenu': null,
  },
  'menuSetSelection': {
    'op': 'menuSetSelection',
    'nodeId': 'm',
    'selectedId': 'i1'
  },
  'mediaSetSource': {
    'op': 'mediaSetSource',
    'nodeId': 'm',
    'source': {'kind': 'url'},
    'intrinsic': {'width': 1, 'height': 1},
  },
  'surfaceSetReference': {
    'op': 'surfaceSetReference',
    'nodeId': 's',
    'reference': {'surfaceId': 'x'}
  },
  'toggleSetValue': {'op': 'toggleSetValue', 'nodeId': 't', 'value': true},
  'checkmarkSetValue': {
    'op': 'checkmarkSetValue',
    'nodeId': 'c',
    'value': false
  },
  'sparklineSetData': {
    'op': 'sparklineSetData',
    'nodeId': 's',
    'series': [1.0, 2.0],
    'accessibilityText': 'trend'
  },
  'barChartSetData': {
    'op': 'barChartSetData',
    'nodeId': 'b',
    'bars': [
      {'label': 'a', 'value': 1.0}
    ],
    'accessibilityText': 'bars'
  },
  'lineChartSetData': {
    'op': 'lineChartSetData',
    'nodeId': 'l',
    'series': [
      {'points': []}
    ],
    'xAxis': {'label': 'x'},
    'yAxis': {'label': 'y'},
    'accessibilityText': 'lines'
  },
  'gaugeSetData': {
    'op': 'gaugeSetData',
    'nodeId': 'g',
    'ratio': 0.5,
    'label': 'CPU',
    'accessibilityText': 'cpu at 50 percent'
  },
  'footerSetActions': {
    'op': 'footerSetActions',
    'nodeId': 'f',
    'actions': [
      {'id': 'a', 'label': 'Save', 'action': 'save'}
    ],
    'status': 'ok'
  },
  'inputSetValue': {'op': 'inputSetValue', 'nodeId': 'i', 'value': 'text'},
  'listInsertItem': {
    'op': 'listInsertItem',
    'listId': 'l',
    'index': 0,
    'item': {'id': 'i1', 'label': 'one'}
  },
  'listSetSelection': {
    'op': 'listSetSelection',
    'listId': 'l',
    'selectedId': null
  },
  'listRemoveItem': {'op': 'listRemoveItem', 'listId': 'l', 'itemId': 'i1'},
  'contentSetSelection': {
    'op': 'contentSetSelection',
    'contentId': 'c',
    'selection': null
  },
  'contentSpliceLines': {
    'op': 'contentSpliceLines',
    'contentId': 'c',
    'index': 0,
    'deleteCount': 1,
    'lines': [
      {'id': 'l1', 'text': 'new'}
    ]
  },
  'treeSetSelection': {
    'op': 'treeSetSelection',
    'nodeId': 't',
    'selectedId': 'n1'
  },
  'treeSetFilter': {'op': 'treeSetFilter', 'filterId': 'f', 'value': 'q'},
  'treeSetLocation': {
    'op': 'treeSetLocation',
    'nodeId': 't',
    'location': '/root'
  },
  'treeSpliceChildren': {
    'op': 'treeSpliceChildren',
    'nodeId': 't',
    'parentId': null,
    'index': 0,
    'deleteCount': 0,
    'items': [
      {'id': 'n1', 'label': 'node'}
    ]
  },
  'treeSetChildState': {
    'op': 'treeSetChildState',
    'nodeId': 't',
    'itemId': 'n1',
    'childState': {'hasChildren': true}
  },
  'treeSetExpanded': {
    'op': 'treeSetExpanded',
    'nodeId': 't',
    'itemId': 'n1',
    'expanded': true
  },
};

final Map<String, Type> opTypes = {
  'replaceRoot': DeltaReplaceRoot,
  'markdownReplaceRange': DeltaMarkdownReplaceRange,
  'markdownSetSelection': DeltaMarkdownSetSelection,
  'markdownSetPresentation': DeltaMarkdownSetPresentation,
  'markdownSetDirty': DeltaMarkdownSetDirty,
  'markdownSetReadOnly': DeltaMarkdownSetReadOnly,
  'markdownSetTitle': DeltaMarkdownSetTitle,
  'markdownSetPlaceholder': DeltaMarkdownSetPlaceholder,
  'markdownSetCommandHint': DeltaMarkdownSetCommandHint,
  'markdownSetActions': DeltaMarkdownSetActions,
  'markdownSetMenus': DeltaMarkdownSetMenus,
  'menuSetSelection': DeltaMenuSetSelection,
  'mediaSetSource': DeltaMediaSetSource,
  'surfaceSetReference': DeltaSurfaceSetReference,
  'toggleSetValue': DeltaToggleSetValue,
  'checkmarkSetValue': DeltaCheckmarkSetValue,
  'sparklineSetData': DeltaSparklineSetData,
  'barChartSetData': DeltaBarChartSetData,
  'lineChartSetData': DeltaLineChartSetData,
  'gaugeSetData': DeltaGaugeSetData,
  'footerSetActions': DeltaFooterSetActions,
  'inputSetValue': DeltaInputSetValue,
  'listInsertItem': DeltaListInsertItem,
  'listSetSelection': DeltaListSetSelection,
  'listRemoveItem': DeltaListRemoveItem,
  'contentSetSelection': DeltaContentSetSelection,
  'contentSpliceLines': DeltaContentSpliceLines,
  'treeSetSelection': DeltaTreeSetSelection,
  'treeSetFilter': DeltaTreeSetFilter,
  'treeSetLocation': DeltaTreeSetLocation,
  'treeSpliceChildren': DeltaTreeSpliceChildren,
  'treeSetChildState': DeltaTreeSetChildState,
  'treeSetExpanded': DeltaTreeSetExpanded,
};

Map<String, dynamic> deltaEnvelope(List<Map<String, dynamic>> ops,
        {int baseRevision = 7, int revision = 8}) =>
    {
      'protocol': 'unpeel.ui',
      'protocolVersion': 1,
      'appInstanceId': 'app',
      'clientId': 'client',
      'viewId': 'view',
      'baseRevision': baseRevision,
      'revision': revision,
      'operations': ops,
    };

void main() {
  group('UIDeltaOperation decode (UIDelta.swift)', () {
    test('all 33 wire op names dispatch to the right class', () {
      expect(opFixtures.keys, hasLength(33));
      expect(opTypes.keys, hasLength(33));
      for (final entry in opFixtures.entries) {
        final decoded = AppKitDeltaOperation.fromJson(entry.value);
        expect(decoded.runtimeType, opTypes[entry.key],
            reason: 'op ${entry.key}');
        expect(decoded.op, entry.key);
      }
    });

    test('unknown op throws', () {
      expect(
        () => AppKitDeltaOperation.fromJson({'op': 'nope'}),
        throwsA(isA<AppKitDeltaFormatException>()),
      );
    });

    test('missing op throws', () {
      expect(
        () => AppKitDeltaOperation.fromJson({}),
        throwsA(isA<AppKitDeltaFormatException>()),
      );
    });

    test('wire keys use nodeId/listId/selectedId (not nodeID)', () {
      final op = AppKitDeltaOperation.fromJson(
          opFixtures['listInsertItem']!) as DeltaListInsertItem;
      expect(op.listId, 'l');
      expect(op.index, 0);
      expect(op.item.id, 'i1');
    });

    test('gauge payload is validated on decode like Swift', () {
      final good = AppKitDeltaOperation.fromJson(
          opFixtures['gaugeSetData']!) as DeltaGaugeSetData;
      expect(good.gauge.ratio, 0.5);
      expect(
        () => AppKitDeltaOperation.fromJson({
          'op': 'gaugeSetData',
          'nodeId': 'g',
          'ratio': 9.9,
          'label': 'CPU',
          'accessibilityText': 'x'
        }),
        throwsA(anyOf(
            isA<AppKitDeltaFormatException>(), isA<FormatException>())),
        reason: 'Swift throws dataCorrupted on invalid gauge delta data',
      );
    });

    test('nullable fields decode to null', () {
      final title = AppKitDeltaOperation.fromJson(
              {'op': 'markdownSetTitle', 'nodeId': 'm'}) as DeltaMarkdownSetTitle;
      expect(title.title, isNull);
      final sel = AppKitDeltaOperation.fromJson(
              opFixtures['listSetSelection']!) as DeltaListSetSelection;
      expect(sel.selectedId, isNull);
    });
  });

  group('AppKitDelta envelope (UIDelta.swift)', () {
    test('valid delta decodes with route fields', () {
      final delta = AppKitDelta.fromJson(
          deltaEnvelope([opFixtures['toggleSetValue']!]));
      expect(delta.protocolName, 'unpeel.ui');
      expect(delta.appInstanceId, 'app');
      expect(delta.clientId, 'client');
      expect(delta.viewId, 'view');
      expect(delta.baseRevision, 7);
      expect(delta.revision, 8);
      expect(delta.operations, hasLength(1));
    });

    test('contiguity check mirrors UISnapshot.applying guards', () {
      final delta = AppKitDelta.fromJson(
          deltaEnvelope([opFixtures['toggleSetValue']!]));
      expect(delta.isContiguousWith(7), isTrue);
      expect(delta.isContiguousWith(6), isFalse);
      expect(delta.isContiguousWith(8), isFalse);
    });

    test('negative baseRevision is rejected', () {
      expect(
        () => AppKitDelta.fromJson(
            deltaEnvelope([opFixtures['toggleSetValue']!], baseRevision: -1)),
        throwsA(isA<AppKitDeltaFormatException>()),
      );
    });

    test('non-advancing revision is rejected', () {
      expect(
        () => AppKitDelta.fromJson(deltaEnvelope(
            [opFixtures['toggleSetValue']!],
            baseRevision: 8,
            revision: 8)),
        throwsA(isA<AppKitDeltaFormatException>()),
      );
    });

    test('empty operations are rejected', () {
      expect(
        () => AppKitDelta.fromJson(deltaEnvelope([])),
        throwsA(isA<AppKitDeltaFormatException>()),
      );
    });

    test('more than 4096 operations are rejected', () {
      final ops = List.generate(
          4097, (_) => Map<String, dynamic>.from(opFixtures['toggleSetValue']!));
      expect(
        () => AppKitDelta.fromJson(deltaEnvelope(ops)),
        throwsA(isA<AppKitDeltaFormatException>()),
      );
    });
  });
}
