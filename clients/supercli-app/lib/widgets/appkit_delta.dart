/// UIDelta wire protocol ported from Swift to Dart.
///
/// Port of `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIDelta.swift`
/// (1336 lines): the `UIDeltaOperation` decode side and the `UIDelta` envelope
/// with its revision validation. Incremental application
/// (`UISnapshot.applying(_:)`) is in `appkit_delta_apply.dart`.
///
/// A delta is a contiguous server-to-renderer change: the renderer applies it
/// only when its snapshot revision equals `baseRevision`. JSON keys match the
/// Swift `CodingKeys` exactly (`op`, `nodeId`, `listId`, `selectedId`, …).
/// Unknown `op` values throw [AppKitDeltaFormatException], mirroring Swift's
/// `DecodingError` (the wire contract is closed; unknown components — not
/// unknown ops — get the terminal fallback).
///
/// Decode-time validation mirrors Swift:
/// - `gaugeSetData` reuses [GaugeSpec.fromJson], which throws on invalid data.
/// - The envelope requires `baseRevision >= 0`, `revision > baseRevision`,
///   and 1…4096 operations.
library;

import 'appkit_protocol.dart';

/// Thrown when delta JSON is malformed. Mirrors Swift `DecodingError`.
final class AppKitDeltaFormatException implements Exception {
  const AppKitDeltaFormatException(this.message, [this.source]);

  final String message;
  final Object? source;

  @override
  String toString() => 'AppKitDeltaFormatException: $message';
}

/// Thrown when applying a delta fails. Mirrors Swift `UIDeltaApplicationError`.
final class AppKitDeltaApplicationError implements Exception {
  const AppKitDeltaApplicationError(this.message);

  final String message;

  @override
  String toString() => 'AppKitDeltaApplicationError: $message';
}

String _str(Map<String, dynamic> json, String key) {
  final v = json[key];
  if (v is! String) {
    throw AppKitDeltaFormatException('expected string for "$key"', json);
  }
  return v;
}

String? _optStr(Map<String, dynamic> json, String key) {
  final v = json[key];
  if (v == null) return null;
  if (v is! String) {
    throw AppKitDeltaFormatException('expected string? for "$key"', json);
  }
  return v;
}

int _int(Map<String, dynamic> json, String key) {
  final v = json[key];
  if (v is! int) {
    throw AppKitDeltaFormatException('expected int for "$key"', json);
  }
  return v;
}

bool _bool(Map<String, dynamic> json, String key) {
  final v = json[key];
  if (v is! bool) {
    throw AppKitDeltaFormatException('expected bool for "$key"', json);
  }
  return v;
}

Map<String, dynamic> _map(Map<String, dynamic> json, String key) {
  final v = json[key];
  if (v is! Map<String, dynamic>) {
    throw AppKitDeltaFormatException('expected object for "$key"', json);
  }
  return v;
}

Map<String, dynamic>? _optMap(Map<String, dynamic> json, String key) {
  final v = json[key];
  if (v == null) return null;
  if (v is! Map<String, dynamic>) {
    throw AppKitDeltaFormatException('expected object? for "$key"', json);
  }
  return v;
}

/// One ordered server-side mutation between immutable UI revisions.
///
/// Mirrors Swift `UIDeltaOperation`. Nested editor-internal payloads
/// (`UITextEdit`, `UITextSelection`, `MarkdownPresentation`, …) decode as raw
/// wire maps: they are opaque to the renderer, which re-renders full
/// snapshots. Payloads with a Dart spec type decode through it.
sealed class AppKitDeltaOperation {
  const AppKitDeltaOperation();

  /// Wire `op` name. Mirrors Swift `UIDeltaOperation.Operation.rawValue`.
  String get op;

  factory AppKitDeltaOperation.fromJson(Map<String, dynamic> json) {
    final op = json['op'];
    if (op is! String) {
      throw AppKitDeltaFormatException('missing "op"', json);
    }
    switch (op) {
      case 'replaceRoot':
        return DeltaReplaceRoot(
            AppKitNode.fromJson(_map(json, 'root')));
      case 'markdownReplaceRange':
        return DeltaMarkdownReplaceRange(
            nodeId: _str(json, 'nodeId'), edit: _map(json, 'edit'));
      case 'markdownSetSelection':
        return DeltaMarkdownSetSelection(
            nodeId: _str(json, 'nodeId'),
            selection: _map(json, 'selection'));
      case 'markdownSetPresentation':
        return DeltaMarkdownSetPresentation(
            nodeId: _str(json, 'nodeId'),
            presentation: _map(json, 'presentation'));
      case 'markdownSetDirty':
        return DeltaMarkdownSetDirty(
            nodeId: _str(json, 'nodeId'), dirty: _bool(json, 'dirty'));
      case 'markdownSetReadOnly':
        return DeltaMarkdownSetReadOnly(
            nodeId: _str(json, 'nodeId'), readOnly: _bool(json, 'readOnly'));
      case 'markdownSetTitle':
        return DeltaMarkdownSetTitle(
            nodeId: _str(json, 'nodeId'), title: _optStr(json, 'title'));
      case 'markdownSetPlaceholder':
        return DeltaMarkdownSetPlaceholder(
            nodeId: _str(json, 'nodeId'),
            placeholder: _str(json, 'placeholder'));
      case 'markdownSetCommandHint':
        return DeltaMarkdownSetCommandHint(
            nodeId: _str(json, 'nodeId'),
            commandHint: _optMap(json, 'commandHint'));
      case 'markdownSetActions':
        return DeltaMarkdownSetActions(
            nodeId: _str(json, 'nodeId'), actions: _map(json, 'actions'));
      case 'markdownSetMenus':
        return DeltaMarkdownSetMenus(
          nodeId: _str(json, 'nodeId'),
          insertMenu: _optMap(json, 'insertMenu') == null
              ? null
              : MenuSpec.fromJson(_map(json, 'insertMenu')),
          contextMenu: _optMap(json, 'contextMenu') == null
              ? null
              : MenuSpec.fromJson(_map(json, 'contextMenu')),
        );
      case 'menuSetSelection':
        return DeltaMenuSetSelection(
            nodeId: _str(json, 'nodeId'),
            selectedId: _optStr(json, 'selectedId'));
      case 'mediaSetSource':
        return DeltaMediaSetSource(
          nodeId: _str(json, 'nodeId'),
          source: _map(json, 'source'),
          intrinsic: _map(json, 'intrinsic'),
        );
      case 'surfaceSetReference':
        return DeltaSurfaceSetReference(
            nodeId: _str(json, 'nodeId'),
            reference: _map(json, 'reference'));
      case 'toggleSetValue':
        return DeltaToggleSetValue(
            nodeId: _str(json, 'nodeId'), value: _bool(json, 'value'));
      case 'checkmarkSetValue':
        return DeltaCheckmarkSetValue(
            nodeId: _str(json, 'nodeId'), value: _bool(json, 'value'));
      case 'sparklineSetData':
        return DeltaSparklineSetData(
          nodeId: _str(json, 'nodeId'),
          series: _numList(json, 'series'),
          min: _optNum(json, 'min'),
          max: _optNum(json, 'max'),
          caption: _optStr(json, 'caption'),
          unit: _optStr(json, 'unit'),
          accessibilityText: _str(json, 'accessibilityText'),
        );
      case 'barChartSetData':
        return DeltaBarChartSetData(
          nodeId: _str(json, 'nodeId'),
          bars: _mapList(json, 'bars'),
          accessibilityText: _str(json, 'accessibilityText'),
        );
      case 'lineChartSetData':
        return DeltaLineChartSetData(
          nodeId: _str(json, 'nodeId'),
          series: _mapList(json, 'series'),
          xAxis: _map(json, 'xAxis'),
          yAxis: _map(json, 'yAxis'),
          accessibilityText: _str(json, 'accessibilityText'),
        );
      case 'gaugeSetData':
        // GaugeSpec.fromJson throws on invalid data, mirroring the Swift
        // `guard gauge.isValid else { throw … }`.
        return DeltaGaugeSetData(GaugeSpec.fromJson({
          'id': _str(json, 'nodeId'),
          'ratio': json['ratio'],
          'label': _str(json, 'label'),
          'caption': _optStr(json, 'caption'),
          'accessibilityText': _str(json, 'accessibilityText'),
        }));
      case 'footerSetActions':
        return DeltaFooterSetActions(
          nodeId: _str(json, 'nodeId'),
          actions: (_list(json, 'actions'))
              .map((a) =>
                  FooterActionSpec.fromJson(a as Map<String, dynamic>))
              .toList(),
          status: _optStr(json, 'status'),
        );
      case 'inputSetValue':
        return DeltaInputSetValue(
            nodeId: _str(json, 'nodeId'), value: _str(json, 'value'));
      case 'listInsertItem':
        return DeltaListInsertItem(
          listId: _str(json, 'listId'),
          index: _int(json, 'index'),
          item: ListItemSpec.fromJson(_map(json, 'item')),
        );
      case 'listSetSelection':
        return DeltaListSetSelection(
            listId: _str(json, 'listId'),
            selectedId: _optStr(json, 'selectedId'));
      case 'listRemoveItem':
        return DeltaListRemoveItem(
            listId: _str(json, 'listId'), itemId: _str(json, 'itemId'));
      case 'contentSetSelection':
        return DeltaContentSetSelection(
            contentId: _str(json, 'contentId'),
            selection: _optMap(json, 'selection'));
      case 'contentSpliceLines':
        return DeltaContentSpliceLines(
          contentId: _str(json, 'contentId'),
          index: _int(json, 'index'),
          deleteCount: _int(json, 'deleteCount'),
          lines: (_list(json, 'lines'))
              .map((l) => ContentLine.fromJson(l as Map<String, dynamic>))
              .toList(),
        );
      case 'treeSetSelection':
        return DeltaTreeSetSelection(
            nodeId: _str(json, 'nodeId'),
            selectedId: _optStr(json, 'selectedId'));
      case 'treeSetFilter':
        return DeltaTreeSetFilter(
            filterId: _str(json, 'filterId'), value: _str(json, 'value'));
      case 'treeSetLocation':
        return DeltaTreeSetLocation(
            nodeId: _str(json, 'nodeId'), location: _str(json, 'location'));
      case 'treeSpliceChildren':
        return DeltaTreeSpliceChildren(
          nodeId: _str(json, 'nodeId'),
          parentId: _optStr(json, 'parentId'),
          index: _int(json, 'index'),
          deleteCount: _int(json, 'deleteCount'),
          items: (_list(json, 'items'))
              .map((i) => TreeItemSpec.fromJson(i as Map<String, dynamic>))
              .toList(),
        );
      case 'treeSetChildState':
        return DeltaTreeSetChildState(
          nodeId: _str(json, 'nodeId'),
          itemId: _str(json, 'itemId'),
          childState: _map(json, 'childState'),
        );
      case 'treeSetExpanded':
        return DeltaTreeSetExpanded(
          nodeId: _str(json, 'nodeId'),
          itemId: _str(json, 'itemId'),
          expanded: _bool(json, 'expanded'),
        );
      default:
        throw AppKitDeltaFormatException('unknown delta op "$op"', json);
    }
  }
}

List<dynamic> _list(Map<String, dynamic> json, String key) {
  final v = json[key];
  if (v is! List) {
    throw AppKitDeltaFormatException('expected list for "$key"', json);
  }
  return v;
}

List<double> _numList(Map<String, dynamic> json, String key) => _list(json, key)
    .map((e) => e is num
        ? e.toDouble()
        : throw AppKitDeltaFormatException('expected number in "$key"', json))
    .toList();

List<Map<String, dynamic>> _mapList(Map<String, dynamic> json, String key) =>
    _list(json, key)
        .map((e) => e is Map<String, dynamic>
            ? e
            : throw AppKitDeltaFormatException(
                'expected object in "$key"', json))
        .toList();

double? _optNum(Map<String, dynamic> json, String key) {
  final v = json[key];
  if (v == null) return null;
  if (v is! num) {
    throw AppKitDeltaFormatException('expected number? for "$key"', json);
  }
  return v.toDouble();
}

/// Mirrors `UIDeltaOperation.replaceRoot`.
final class DeltaReplaceRoot extends AppKitDeltaOperation {
  const DeltaReplaceRoot(this.root);
  final AppKitNode root;
  @override
  String get op => 'replaceRoot';
}

/// Mirrors `UIDeltaOperation.markdownReplaceRange`. `edit` is the raw
/// `UITextEdit` wire payload.
final class DeltaMarkdownReplaceRange extends AppKitDeltaOperation {
  const DeltaMarkdownReplaceRange({required this.nodeId, required this.edit});
  final String nodeId;
  final Map<String, dynamic> edit;
  @override
  String get op => 'markdownReplaceRange';
}

/// Mirrors `UIDeltaOperation.markdownSetSelection`. `selection` is the raw
/// `UITextSelection` wire payload.
final class DeltaMarkdownSetSelection extends AppKitDeltaOperation {
  const DeltaMarkdownSetSelection(
      {required this.nodeId, required this.selection});
  final String nodeId;
  final Map<String, dynamic> selection;
  @override
  String get op => 'markdownSetSelection';
}

/// Mirrors `UIDeltaOperation.markdownSetPresentation`.
final class DeltaMarkdownSetPresentation extends AppKitDeltaOperation {
  const DeltaMarkdownSetPresentation(
      {required this.nodeId, required this.presentation});
  final String nodeId;
  final Map<String, dynamic> presentation;
  @override
  String get op => 'markdownSetPresentation';
}

/// Mirrors `UIDeltaOperation.markdownSetDirty`.
final class DeltaMarkdownSetDirty extends AppKitDeltaOperation {
  const DeltaMarkdownSetDirty({required this.nodeId, required this.dirty});
  final String nodeId;
  final bool dirty;
  @override
  String get op => 'markdownSetDirty';
}

/// Mirrors `UIDeltaOperation.markdownSetReadOnly`.
final class DeltaMarkdownSetReadOnly extends AppKitDeltaOperation {
  const DeltaMarkdownSetReadOnly(
      {required this.nodeId, required this.readOnly});
  final String nodeId;
  final bool readOnly;
  @override
  String get op => 'markdownSetReadOnly';
}

/// Mirrors `UIDeltaOperation.markdownSetTitle`.
final class DeltaMarkdownSetTitle extends AppKitDeltaOperation {
  const DeltaMarkdownSetTitle({required this.nodeId, required this.title});
  final String nodeId;
  final String? title;
  @override
  String get op => 'markdownSetTitle';
}

/// Mirrors `UIDeltaOperation.markdownSetPlaceholder`.
final class DeltaMarkdownSetPlaceholder extends AppKitDeltaOperation {
  const DeltaMarkdownSetPlaceholder(
      {required this.nodeId, required this.placeholder});
  final String nodeId;
  final String placeholder;
  @override
  String get op => 'markdownSetPlaceholder';
}

/// Mirrors `UIDeltaOperation.markdownSetCommandHint`.
final class DeltaMarkdownSetCommandHint extends AppKitDeltaOperation {
  const DeltaMarkdownSetCommandHint(
      {required this.nodeId, required this.commandHint});
  final String nodeId;
  final Map<String, dynamic>? commandHint;
  @override
  String get op => 'markdownSetCommandHint';
}

/// Mirrors `UIDeltaOperation.markdownSetActions`.
final class DeltaMarkdownSetActions extends AppKitDeltaOperation {
  const DeltaMarkdownSetActions(
      {required this.nodeId, required this.actions});
  final String nodeId;
  final Map<String, dynamic> actions;
  @override
  String get op => 'markdownSetActions';
}

/// Mirrors `UIDeltaOperation.markdownSetMenus`.
final class DeltaMarkdownSetMenus extends AppKitDeltaOperation {
  const DeltaMarkdownSetMenus(
      {required this.nodeId, this.insertMenu, this.contextMenu});
  final String nodeId;
  final MenuSpec? insertMenu;
  final MenuSpec? contextMenu;
  @override
  String get op => 'markdownSetMenus';
}

/// Mirrors `UIDeltaOperation.menuSetSelection`.
final class DeltaMenuSetSelection extends AppKitDeltaOperation {
  const DeltaMenuSetSelection({required this.nodeId, required this.selectedId});
  final String nodeId;
  final String? selectedId;
  @override
  String get op => 'menuSetSelection';
}

/// Mirrors `UIDeltaOperation.mediaSetSource`.
final class DeltaMediaSetSource extends AppKitDeltaOperation {
  const DeltaMediaSetSource(
      {required this.nodeId, required this.source, required this.intrinsic});
  final String nodeId;
  final Map<String, dynamic> source;
  final Map<String, dynamic> intrinsic;
  @override
  String get op => 'mediaSetSource';
}

/// Mirrors `UIDeltaOperation.surfaceSetReference`.
final class DeltaSurfaceSetReference extends AppKitDeltaOperation {
  const DeltaSurfaceSetReference(
      {required this.nodeId, required this.reference});
  final String nodeId;
  final Map<String, dynamic> reference;
  @override
  String get op => 'surfaceSetReference';
}

/// Mirrors `UIDeltaOperation.toggleSetValue`.
final class DeltaToggleSetValue extends AppKitDeltaOperation {
  const DeltaToggleSetValue({required this.nodeId, required this.value});
  final String nodeId;
  final bool value;
  @override
  String get op => 'toggleSetValue';
}

/// Mirrors `UIDeltaOperation.checkmarkSetValue`.
final class DeltaCheckmarkSetValue extends AppKitDeltaOperation {
  const DeltaCheckmarkSetValue({required this.nodeId, required this.value});
  final String nodeId;
  final bool value;
  @override
  String get op => 'checkmarkSetValue';
}

/// Mirrors `UIDeltaOperation.sparklineSetData`.
final class DeltaSparklineSetData extends AppKitDeltaOperation {
  const DeltaSparklineSetData({
    required this.nodeId,
    required this.series,
    this.min,
    this.max,
    this.caption,
    this.unit,
    required this.accessibilityText,
  });
  final String nodeId;
  final List<double> series;
  final double? min;
  final double? max;
  final String? caption;
  final String? unit;
  final String accessibilityText;
  @override
  String get op => 'sparklineSetData';
}

/// Mirrors `UIDeltaOperation.barChartSetData`.
final class DeltaBarChartSetData extends AppKitDeltaOperation {
  const DeltaBarChartSetData(
      {required this.nodeId,
      required this.bars,
      required this.accessibilityText});
  final String nodeId;
  final List<Map<String, dynamic>> bars;
  final String accessibilityText;
  @override
  String get op => 'barChartSetData';
}

/// Mirrors `UIDeltaOperation.lineChartSetData`.
final class DeltaLineChartSetData extends AppKitDeltaOperation {
  const DeltaLineChartSetData({
    required this.nodeId,
    required this.series,
    required this.xAxis,
    required this.yAxis,
    required this.accessibilityText,
  });
  final String nodeId;
  final List<Map<String, dynamic>> series;
  final Map<String, dynamic> xAxis;
  final Map<String, dynamic> yAxis;
  final String accessibilityText;
  @override
  String get op => 'lineChartSetData';
}

/// Mirrors `UIDeltaOperation.gaugeSetData`. The spec is validated on decode,
/// mirroring the Swift `guard gauge.isValid`.
final class DeltaGaugeSetData extends AppKitDeltaOperation {
  const DeltaGaugeSetData(this.gauge);
  final GaugeSpec gauge;
  @override
  String get op => 'gaugeSetData';
}

/// Mirrors `UIDeltaOperation.footerSetActions`.
final class DeltaFooterSetActions extends AppKitDeltaOperation {
  const DeltaFooterSetActions(
      {required this.nodeId, required this.actions, this.status});
  final String nodeId;
  final List<FooterActionSpec> actions;
  final String? status;
  @override
  String get op => 'footerSetActions';
}

/// Mirrors `UIDeltaOperation.inputSetValue`.
final class DeltaInputSetValue extends AppKitDeltaOperation {
  const DeltaInputSetValue({required this.nodeId, required this.value});
  final String nodeId;
  final String value;
  @override
  String get op => 'inputSetValue';
}

/// Mirrors `UIDeltaOperation.listInsertItem`.
final class DeltaListInsertItem extends AppKitDeltaOperation {
  const DeltaListInsertItem(
      {required this.listId, required this.index, required this.item});
  final String listId;
  final int index;
  final ListItemSpec item;
  @override
  String get op => 'listInsertItem';
}

/// Mirrors `UIDeltaOperation.listSetSelection`.
final class DeltaListSetSelection extends AppKitDeltaOperation {
  const DeltaListSetSelection({required this.listId, required this.selectedId});
  final String listId;
  final String? selectedId;
  @override
  String get op => 'listSetSelection';
}

/// Mirrors `UIDeltaOperation.listRemoveItem`.
final class DeltaListRemoveItem extends AppKitDeltaOperation {
  const DeltaListRemoveItem({required this.listId, required this.itemId});
  final String listId;
  final String itemId;
  @override
  String get op => 'listRemoveItem';
}

/// Mirrors `UIDeltaOperation.contentSetSelection`.
final class DeltaContentSetSelection extends AppKitDeltaOperation {
  const DeltaContentSetSelection(
      {required this.contentId, required this.selection});
  final String contentId;
  final Map<String, dynamic>? selection;
  @override
  String get op => 'contentSetSelection';
}

/// Mirrors `UIDeltaOperation.contentSpliceLines`.
final class DeltaContentSpliceLines extends AppKitDeltaOperation {
  const DeltaContentSpliceLines({
    required this.contentId,
    required this.index,
    required this.deleteCount,
    required this.lines,
  });
  final String contentId;
  final int index;
  final int deleteCount;
  final List<ContentLine> lines;
  @override
  String get op => 'contentSpliceLines';
}

/// Mirrors `UIDeltaOperation.treeSetSelection`.
final class DeltaTreeSetSelection extends AppKitDeltaOperation {
  const DeltaTreeSetSelection({required this.nodeId, required this.selectedId});
  final String nodeId;
  final String? selectedId;
  @override
  String get op => 'treeSetSelection';
}

/// Mirrors `UIDeltaOperation.treeSetFilter`.
final class DeltaTreeSetFilter extends AppKitDeltaOperation {
  const DeltaTreeSetFilter({required this.filterId, required this.value});
  final String filterId;
  final String value;
  @override
  String get op => 'treeSetFilter';
}

/// Mirrors `UIDeltaOperation.treeSetLocation`.
final class DeltaTreeSetLocation extends AppKitDeltaOperation {
  const DeltaTreeSetLocation({required this.nodeId, required this.location});
  final String nodeId;
  final String location;
  @override
  String get op => 'treeSetLocation';
}

/// Mirrors `UIDeltaOperation.treeSpliceChildren`.
final class DeltaTreeSpliceChildren extends AppKitDeltaOperation {
  const DeltaTreeSpliceChildren({
    required this.nodeId,
    this.parentId,
    required this.index,
    required this.deleteCount,
    required this.items,
  });
  final String nodeId;
  final String? parentId;
  final int index;
  final int deleteCount;
  final List<TreeItemSpec> items;
  @override
  String get op => 'treeSpliceChildren';
}

/// Mirrors `UIDeltaOperation.treeSetChildState`.
final class DeltaTreeSetChildState extends AppKitDeltaOperation {
  const DeltaTreeSetChildState(
      {required this.nodeId, required this.itemId, required this.childState});
  final String nodeId;
  final String itemId;
  final Map<String, dynamic> childState;
  @override
  String get op => 'treeSetChildState';
}

/// Mirrors `UIDeltaOperation.treeSetExpanded`.
final class DeltaTreeSetExpanded extends AppKitDeltaOperation {
  const DeltaTreeSetExpanded(
      {required this.nodeId, required this.itemId, required this.expanded});
  final String nodeId;
  final String itemId;
  final bool expanded;
  @override
  String get op => 'treeSetExpanded';
}

/// Contiguous server-to-renderer change.
///
/// Mirrors Swift `UIDelta`. A renderer applies it only when its complete
/// snapshot revision equals [baseRevision]. Decode enforces the Swift guards:
/// `baseRevision >= 0`, `revision > baseRevision`, 1…4096 operations.
final class AppKitDelta {
  const AppKitDelta({
    required this.protocolName,
    required this.protocolVersion,
    required this.appInstanceId,
    required this.clientId,
    required this.viewId,
    required this.baseRevision,
    required this.revision,
    required this.operations,
  });

  final String protocolName;
  final int protocolVersion;
  final String appInstanceId;
  final String clientId;
  final String viewId;
  final int baseRevision;
  final int revision;
  final List<AppKitDeltaOperation> operations;

  /// True when the delta is contiguous with a snapshot at [snapshotRevision].
  bool isContiguousWith(int snapshotRevision) =>
      snapshotRevision == baseRevision && revision > baseRevision;

  factory AppKitDelta.fromJson(Map<String, dynamic> json) {
    final baseRevision = _int(json, 'baseRevision');
    final revision = _int(json, 'revision');
    final operations = (_list(json, 'operations'))
        .map((o) => AppKitDeltaOperation.fromJson(o as Map<String, dynamic>))
        .toList();
    if (baseRevision < 0 ||
        revision <= baseRevision ||
        operations.isEmpty ||
        operations.length > 4096) {
      throw AppKitDeltaFormatException(
        'Delta must advance its base with 1...4096 operations',
        json,
      );
    }
    return AppKitDelta(
      protocolName: _str(json, 'protocol'),
      protocolVersion: _int(json, 'protocolVersion'),
      appInstanceId: _str(json, 'appInstanceId'),
      clientId: _str(json, 'clientId'),
      viewId: _str(json, 'viewId'),
      baseRevision: baseRevision,
      revision: revision,
      operations: operations,
    );
  }
}
