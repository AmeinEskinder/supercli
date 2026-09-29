/// Wire-faithful Dart port of the AppKit UI delta engine.
///
/// Ports `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIDelta.swift`:
/// [UIDeltaOperation] (33 flat `op`-discriminated cases), [UIDelta] (the
/// contiguous server-to-renderer change carried by the `delta` message), and
/// the snapshot-application engine.
///
/// Like the other `appkit_protocol_*` files this one is wire-faithful rather
/// than renderer-shaped: it works on the same decoded model types and
/// validates exactly what Swift validates. Swift's value-type mutation
/// (`var` copies) is mirrored functionally: applying an operation returns new
/// instances and never mutates the input snapshot.
///
/// Depends only on the widgets and lists layers, so the messages layer (home
/// of `UIMessage`/`UISnapshot`) can import this file without an import cycle.
library;

import 'appkit_protocol_widgets.dart';
import 'appkit_protocol_lists.dart';

bool _listEq<T>(List<T> a, List<T> b) {
  if (identical(a, b)) return true;
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

/// Mirrors Swift `UIDeltaApplicationError` (a message-carrying struct, hence
/// value equality).
final class UIDeltaApplicationError extends Error {
  UIDeltaApplicationError(this.message);

  final String message;

  @override
  String toString() => 'UIDeltaApplicationError: $message';

  @override
  bool operator ==(Object other) =>
      other is UIDeltaApplicationError && other.message == message;

  @override
  int get hashCode => message.hashCode;
}

/// Private sentinel distinguishing "leave the optional unchanged" from
/// "set it (possibly to null)", mirroring Swift's `OptionalStringChange`,
/// `OptionalMenuChange`, and `OptionalCommandHintChange`.
final class _Set<T> {
  const _Set(this.value);
  final T value;
}

/// Mirrors Swift `UIDeltaOperation` with its custom `op`-discriminated
/// Codable. Payloads are flat: every field sits alongside `op`, using the
/// exact CodingKey spellings (`nodeId`, `listId`, `contentId`, `itemId`,
/// `selectedId`, `filterId`, `parentId`).
sealed class UIDeltaOperation {
  const UIDeltaOperation();

  String get op;

  factory UIDeltaOperation.fromJson(Map<String, dynamic> json) {
    switch (json['op'] as String) {
      case 'replaceRoot':
        return UIDeltaOperationReplaceRoot(
            UINode.fromJson(json['root'] as Map<String, dynamic>));
      case 'markdownReplaceRange':
        return UIDeltaOperationMarkdownReplaceRange(
          nodeID: json['nodeId'] as String,
          edit: UITextEdit.fromJson(json['edit'] as Map<String, dynamic>),
        );
      case 'markdownSetSelection':
        return UIDeltaOperationMarkdownSetSelection(
          nodeID: json['nodeId'] as String,
          selection: UITextSelection.fromJson(
              json['selection'] as Map<String, dynamic>),
        );
      case 'markdownSetPresentation':
        return UIDeltaOperationMarkdownSetPresentation(
          nodeID: json['nodeId'] as String,
          presentation: MarkdownPresentation.fromJson(
              json['presentation'] as String),
        );
      case 'markdownSetDirty':
        return UIDeltaOperationMarkdownSetDirty(
          nodeID: json['nodeId'] as String,
          dirty: json['dirty'] as bool,
        );
      case 'markdownSetReadOnly':
        return UIDeltaOperationMarkdownSetReadOnly(
          nodeID: json['nodeId'] as String,
          readOnly: json['readOnly'] as bool,
        );
      case 'markdownSetTitle':
        return UIDeltaOperationMarkdownSetTitle(
          nodeID: json['nodeId'] as String,
          title: json['title'] as String?,
        );
      case 'markdownSetPlaceholder':
        return UIDeltaOperationMarkdownSetPlaceholder(
          nodeID: json['nodeId'] as String,
          placeholder: json['placeholder'] as String,
        );
      case 'markdownSetCommandHint':
        return UIDeltaOperationMarkdownSetCommandHint(
          nodeID: json['nodeId'] as String,
          commandHint: json['commandHint'] == null
              ? null
              : MarkdownCommandHint.fromJson(
                  json['commandHint'] as Map<String, dynamic>),
        );
      case 'markdownSetActions':
        return UIDeltaOperationMarkdownSetActions(
          nodeID: json['nodeId'] as String,
          actions: MarkdownEditorActions.fromJson(
              json['actions'] as Map<String, dynamic>),
        );
      case 'markdownSetMenus':
        return UIDeltaOperationMarkdownSetMenus(
          nodeID: json['nodeId'] as String,
          insertMenu: json['insertMenu'] == null
              ? null
              : UIMenuSpec.fromJson(
                  json['insertMenu'] as Map<String, dynamic>),
          contextMenu: json['contextMenu'] == null
              ? null
              : UIMenuSpec.fromJson(
                  json['contextMenu'] as Map<String, dynamic>),
        );
      case 'menuSetSelection':
        return UIDeltaOperationMenuSetSelection(
          nodeID: json['nodeId'] as String,
          selectedID: json['selectedId'] as String?,
        );
      case 'mediaSetSource':
        return UIDeltaOperationMediaSetSource(
          nodeID: json['nodeId'] as String,
          source: MediaSource.fromJson(json['source'] as Map<String, dynamic>),
          intrinsic: MediaPixelSize.fromJson(
              json['intrinsic'] as Map<String, dynamic>),
        );
      case 'surfaceSetReference':
        return UIDeltaOperationSurfaceSetReference(
          nodeID: json['nodeId'] as String,
          reference: SurfaceReference.fromJson(
              json['reference'] as Map<String, dynamic>),
        );
      case 'toggleSetValue':
        return UIDeltaOperationToggleSetValue(
          nodeID: json['nodeId'] as String,
          value: json['value'] as bool,
        );
      case 'checkmarkSetValue':
        return UIDeltaOperationCheckmarkSetValue(
          nodeID: json['nodeId'] as String,
          value: json['value'] as bool,
        );
      case 'sparklineSetData':
        return UIDeltaOperationSparklineSetData(
            _decodeSparkline(json));
      case 'barChartSetData':
        return UIDeltaOperationBarChartSetData(
            _decodeBarChart(json));
      case 'lineChartSetData':
        return UIDeltaOperationLineChartSetData(
            _decodeLineChart(json));
      case 'gaugeSetData':
        return UIDeltaOperationGaugeSetData(
            _decodeGauge(json));
      case 'footerSetActions':
        final footerActions = (json['actions'] as List)
            .map((a) => UIFooterActionSpec.fromJson(a as Map<String, dynamic>))
            .toList();
        final footerStatus = json['status'] as String?;
        if (!UIFooterActionsSpec(actions: footerActions, status: footerStatus)
            .isValid) {
          throw FormatException('FooterActions delta data is invalid', json);
        }
        return UIDeltaOperationFooterSetActions(
          nodeID: json['nodeId'] as String,
          actions: footerActions,
          status: footerStatus,
        );
      case 'inputSetValue':
        return UIDeltaOperationInputSetValue(
          nodeID: json['nodeId'] as String,
          value: json['value'] as String,
        );
      case 'listInsertItem':
        return UIDeltaOperationListInsertItem(
          listID: json['listId'] as String,
          index: json['index'] as int,
          item: UIListItemSpec.fromJson(json['item'] as Map<String, dynamic>),
        );
      case 'listSetSelection':
        return UIDeltaOperationListSetSelection(
          listID: json['listId'] as String,
          selectedID: json['selectedId'] as String?,
        );
      case 'listRemoveItem':
        return UIDeltaOperationListRemoveItem(
          listID: json['listId'] as String,
          itemID: json['itemId'] as String,
        );
      case 'contentSetSelection':
        return UIDeltaOperationContentSetSelection(
          contentID: json['contentId'] as String,
          selection: json['selection'] == null
              ? null
              : UIContentSelection.fromJson(
                  json['selection'] as Map<String, dynamic>),
        );
      case 'contentSpliceLines':
        return UIDeltaOperationContentSpliceLines(
          contentID: json['contentId'] as String,
          index: json['index'] as int,
          deleteCount: json['deleteCount'] as int,
          lines: (json['lines'] as List)
              .map((l) => UIContentLine.fromJson(l as Map<String, dynamic>))
              .toList(),
        );
      case 'treeSetSelection':
        return UIDeltaOperationTreeSetSelection(
          nodeID: json['nodeId'] as String,
          selectedID: json['selectedId'] as String?,
        );
      case 'treeSetFilter':
        return UIDeltaOperationTreeSetFilter(
          filterID: json['filterId'] as String,
          value: json['value'] as String,
        );
      case 'treeSetLocation':
        return UIDeltaOperationTreeSetLocation(
          nodeID: json['nodeId'] as String,
          location: json['location'] as String,
        );
      case 'treeSpliceChildren':
        return UIDeltaOperationTreeSpliceChildren(
          nodeID: json['nodeId'] as String,
          parentID: json['parentId'] as String?,
          index: json['index'] as int,
          deleteCount: json['deleteCount'] as int,
          items: (json['items'] as List)
              .map((i) => UITreeItem.fromJson(i as Map<String, dynamic>))
              .toList(),
        );
      case 'treeSetChildState':
        return UIDeltaOperationTreeSetChildState(
          nodeID: json['nodeId'] as String,
          itemID: json['itemId'] as String,
          childState:
              UITreeChildState.fromJson(json['childState'] as String),
        );
      case 'treeSetExpanded':
        return UIDeltaOperationTreeSetExpanded(
          nodeID: json['nodeId'] as String,
          itemID: json['itemId'] as String,
          expanded: json['expanded'] as bool,
        );
      default:
        throw FormatException('Unknown UIDeltaOperation ${json['op']}', json);
    }
  }

  Map<String, dynamic> toJson();
}

/// Mirrors Swift's chart-data delta decodes: the spec is rebuilt from the
/// flat keys (with `nodeId` as its id) and must be valid.
UISparklineSpec _decodeSparkline(Map<String, dynamic> json) {
  final sparkline = UISparklineSpec(
    id: json['nodeId'] as String,
    series: (json['series'] as List).map((v) => (v as num).toDouble()).toList(),
    min: (json['min'] as num?)?.toDouble(),
    max: (json['max'] as num?)?.toDouble(),
    caption: json['caption'] as String?,
    unit: json['unit'] as String?,
    accessibilityText: json['accessibilityText'] as String,
  );
  if (!sparkline.isValid) {
    throw FormatException('Sparkline delta data is invalid', json);
  }
  return sparkline;
}

UIBarChartSpec _decodeBarChart(Map<String, dynamic> json) {
  final chart = UIBarChartSpec(
    id: json['nodeId'] as String,
    bars: (json['bars'] as List)
        .map((b) => UIBarChartBar.fromJson(b as Map<String, dynamic>))
        .toList(),
    accessibilityText: json['accessibilityText'] as String,
  );
  if (!chart.isValid) {
    throw FormatException('BarChart delta data is invalid', json);
  }
  return chart;
}

UILineChartSpec _decodeLineChart(Map<String, dynamic> json) {
  final chart = UILineChartSpec(
    id: json['nodeId'] as String,
    series: (json['series'] as List)
        .map((s) => UILineChartSeries.fromJson(s as Map<String, dynamic>))
        .toList(),
    xAxis: UILineChartAxis.fromJson(json['xAxis'] as Map<String, dynamic>),
    yAxis: UILineChartAxis.fromJson(json['yAxis'] as Map<String, dynamic>),
    accessibilityText: json['accessibilityText'] as String,
  );
  if (!chart.isValid) {
    throw FormatException('LineChart delta data is invalid', json);
  }
  return chart;
}

UIGaugeSpec _decodeGauge(Map<String, dynamic> json) {
  final gauge = UIGaugeSpec(
    id: json['nodeId'] as String,
    ratio: (json['ratio'] as num).toDouble(),
    label: json['label'] as String,
    caption: json['caption'] as String?,
    accessibilityText: json['accessibilityText'] as String,
  );
  if (!gauge.isValid) {
    throw FormatException('Gauge delta data is invalid', json);
  }
  return gauge;
}

final class UIDeltaOperationReplaceRoot extends UIDeltaOperation {
  const UIDeltaOperationReplaceRoot(this.root);
  final UINode root;
  @override
  String get op => 'replaceRoot';
  @override
  Map<String, dynamic> toJson() => {'op': op, 'root': root.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationReplaceRoot && other.root == root;
  @override
  int get hashCode => root.hashCode;
}

final class UIDeltaOperationMarkdownReplaceRange extends UIDeltaOperation {
  const UIDeltaOperationMarkdownReplaceRange(
      {required this.nodeID, required this.edit});
  final String nodeID;
  final UITextEdit edit;
  @override
  String get op => 'markdownReplaceRange';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'edit': edit.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationMarkdownReplaceRange &&
      other.nodeID == nodeID &&
      other.edit == edit;
  @override
  int get hashCode => Object.hash(nodeID, edit);
}

final class UIDeltaOperationMarkdownSetSelection extends UIDeltaOperation {
  const UIDeltaOperationMarkdownSetSelection(
      {required this.nodeID, required this.selection});
  final String nodeID;
  final UITextSelection selection;
  @override
  String get op => 'markdownSetSelection';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'selection': selection.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationMarkdownSetSelection &&
      other.nodeID == nodeID &&
      other.selection == selection;
  @override
  int get hashCode => Object.hash(nodeID, selection);
}

final class UIDeltaOperationMarkdownSetPresentation extends UIDeltaOperation {
  const UIDeltaOperationMarkdownSetPresentation(
      {required this.nodeID, required this.presentation});
  final String nodeID;
  final MarkdownPresentation presentation;
  @override
  String get op => 'markdownSetPresentation';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'presentation': presentation.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationMarkdownSetPresentation &&
      other.nodeID == nodeID &&
      other.presentation == presentation;
  @override
  int get hashCode => Object.hash(nodeID, presentation);
}

final class UIDeltaOperationMarkdownSetDirty extends UIDeltaOperation {
  const UIDeltaOperationMarkdownSetDirty(
      {required this.nodeID, required this.dirty});
  final String nodeID;
  final bool dirty;
  @override
  String get op => 'markdownSetDirty';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'dirty': dirty};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationMarkdownSetDirty &&
      other.nodeID == nodeID &&
      other.dirty == dirty;
  @override
  int get hashCode => Object.hash(nodeID, dirty);
}

final class UIDeltaOperationMarkdownSetReadOnly extends UIDeltaOperation {
  const UIDeltaOperationMarkdownSetReadOnly(
      {required this.nodeID, required this.readOnly});
  final String nodeID;
  final bool readOnly;
  @override
  String get op => 'markdownSetReadOnly';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'readOnly': readOnly};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationMarkdownSetReadOnly &&
      other.nodeID == nodeID &&
      other.readOnly == readOnly;
  @override
  int get hashCode => Object.hash(nodeID, readOnly);
}

final class UIDeltaOperationMarkdownSetTitle extends UIDeltaOperation {
  const UIDeltaOperationMarkdownSetTitle(
      {required this.nodeID, required this.title});
  final String nodeID;
  final String? title;
  @override
  String get op => 'markdownSetTitle';
  // Mirrors Swift: the `title` key is always emitted, even when null.
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'title': title};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationMarkdownSetTitle &&
      other.nodeID == nodeID &&
      other.title == title;
  @override
  int get hashCode => Object.hash(nodeID, title);
}

final class UIDeltaOperationMarkdownSetPlaceholder extends UIDeltaOperation {
  const UIDeltaOperationMarkdownSetPlaceholder(
      {required this.nodeID, required this.placeholder});
  final String nodeID;
  final String placeholder;
  @override
  String get op => 'markdownSetPlaceholder';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'placeholder': placeholder};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationMarkdownSetPlaceholder &&
      other.nodeID == nodeID &&
      other.placeholder == placeholder;
  @override
  int get hashCode => Object.hash(nodeID, placeholder);
}

final class UIDeltaOperationMarkdownSetCommandHint extends UIDeltaOperation {
  const UIDeltaOperationMarkdownSetCommandHint(
      {required this.nodeID, required this.commandHint});
  final String nodeID;
  final MarkdownCommandHint? commandHint;
  @override
  String get op => 'markdownSetCommandHint';
  // Mirrors Swift: the `commandHint` key is always emitted, even when null.
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        'nodeId': nodeID,
        'commandHint': commandHint?.toJson(),
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationMarkdownSetCommandHint &&
      other.nodeID == nodeID &&
      other.commandHint == commandHint;
  @override
  int get hashCode => Object.hash(nodeID, commandHint);
}

final class UIDeltaOperationMarkdownSetActions extends UIDeltaOperation {
  const UIDeltaOperationMarkdownSetActions(
      {required this.nodeID, required this.actions});
  final String nodeID;
  final MarkdownEditorActions actions;
  @override
  String get op => 'markdownSetActions';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'actions': actions.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationMarkdownSetActions &&
      other.nodeID == nodeID &&
      other.actions == actions;
  @override
  int get hashCode => Object.hash(nodeID, actions);
}

final class UIDeltaOperationMarkdownSetMenus extends UIDeltaOperation {
  const UIDeltaOperationMarkdownSetMenus(
      {required this.nodeID, this.insertMenu, this.contextMenu});
  final String nodeID;
  final UIMenuSpec? insertMenu;
  final UIMenuSpec? contextMenu;
  @override
  String get op => 'markdownSetMenus';
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        'nodeId': nodeID,
        if (insertMenu != null) 'insertMenu': insertMenu!.toJson(),
        if (contextMenu != null) 'contextMenu': contextMenu!.toJson(),
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationMarkdownSetMenus &&
      other.nodeID == nodeID &&
      other.insertMenu == insertMenu &&
      other.contextMenu == contextMenu;
  @override
  int get hashCode => Object.hash(nodeID, insertMenu, contextMenu);
}

final class UIDeltaOperationMenuSetSelection extends UIDeltaOperation {
  const UIDeltaOperationMenuSetSelection(
      {required this.nodeID, required this.selectedID});
  final String nodeID;
  final String? selectedID;
  @override
  String get op => 'menuSetSelection';
  // Mirrors Swift: the `selectedId` key is always emitted, even when null.
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'selectedId': selectedID};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationMenuSetSelection &&
      other.nodeID == nodeID &&
      other.selectedID == selectedID;
  @override
  int get hashCode => Object.hash(nodeID, selectedID);
}

final class UIDeltaOperationMediaSetSource extends UIDeltaOperation {
  const UIDeltaOperationMediaSetSource(
      {required this.nodeID, required this.source, required this.intrinsic});
  final String nodeID;
  final MediaSource source;
  final MediaPixelSize intrinsic;
  @override
  String get op => 'mediaSetSource';
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        'nodeId': nodeID,
        'source': source.toJson(),
        'intrinsic': intrinsic.toJson(),
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationMediaSetSource &&
      other.nodeID == nodeID &&
      other.source == source &&
      other.intrinsic == intrinsic;
  @override
  int get hashCode => Object.hash(nodeID, source, intrinsic);
}

final class UIDeltaOperationSurfaceSetReference extends UIDeltaOperation {
  const UIDeltaOperationSurfaceSetReference(
      {required this.nodeID, required this.reference});
  final String nodeID;
  final SurfaceReference reference;
  @override
  String get op => 'surfaceSetReference';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'reference': reference.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationSurfaceSetReference &&
      other.nodeID == nodeID &&
      other.reference == reference;
  @override
  int get hashCode => Object.hash(nodeID, reference);
}

final class UIDeltaOperationToggleSetValue extends UIDeltaOperation {
  const UIDeltaOperationToggleSetValue(
      {required this.nodeID, required this.value});
  final String nodeID;
  final bool value;
  @override
  String get op => 'toggleSetValue';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'value': value};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationToggleSetValue &&
      other.nodeID == nodeID &&
      other.value == value;
  @override
  int get hashCode => Object.hash(nodeID, value);
}

final class UIDeltaOperationCheckmarkSetValue extends UIDeltaOperation {
  const UIDeltaOperationCheckmarkSetValue(
      {required this.nodeID, required this.value});
  final String nodeID;
  final bool value;
  @override
  String get op => 'checkmarkSetValue';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'value': value};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationCheckmarkSetValue &&
      other.nodeID == nodeID &&
      other.value == value;
  @override
  int get hashCode => Object.hash(nodeID, value);
}
final class UIDeltaOperationSparklineSetData extends UIDeltaOperation {
  const UIDeltaOperationSparklineSetData(this.sparkline);
  final UISparklineSpec sparkline;
  @override
  String get op => 'sparklineSetData';
  // Mirrors Swift: the chart id travels as `nodeId`; min/max/caption/unit
  // are always emitted, even when null.
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        'nodeId': sparkline.id,
        'series': sparkline.series,
        'min': sparkline.min,
        'max': sparkline.max,
        'caption': sparkline.caption,
        'unit': sparkline.unit,
        'accessibilityText': sparkline.accessibilityText,
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationSparklineSetData &&
      other.sparkline == sparkline;
  @override
  int get hashCode => sparkline.hashCode;
}

final class UIDeltaOperationBarChartSetData extends UIDeltaOperation {
  const UIDeltaOperationBarChartSetData(this.chart);
  final UIBarChartSpec chart;
  @override
  String get op => 'barChartSetData';
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        'nodeId': chart.id,
        'bars': chart.bars.map((b) => b.toJson()).toList(),
        'accessibilityText': chart.accessibilityText,
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationBarChartSetData && other.chart == chart;
  @override
  int get hashCode => chart.hashCode;
}

final class UIDeltaOperationLineChartSetData extends UIDeltaOperation {
  const UIDeltaOperationLineChartSetData(this.chart);
  final UILineChartSpec chart;
  @override
  String get op => 'lineChartSetData';
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        'nodeId': chart.id,
        'series': chart.series.map((s) => s.toJson()).toList(),
        'xAxis': chart.xAxis.toJson(),
        'yAxis': chart.yAxis.toJson(),
        'accessibilityText': chart.accessibilityText,
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationLineChartSetData && other.chart == chart;
  @override
  int get hashCode => chart.hashCode;
}

final class UIDeltaOperationGaugeSetData extends UIDeltaOperation {
  const UIDeltaOperationGaugeSetData(this.gauge);
  final UIGaugeSpec gauge;
  @override
  String get op => 'gaugeSetData';
  // Mirrors Swift: the chart id travels as `nodeId`; `caption` is always
  // emitted, even when null.
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        'nodeId': gauge.id,
        'ratio': gauge.ratio,
        'label': gauge.label,
        'caption': gauge.caption,
        'accessibilityText': gauge.accessibilityText,
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationGaugeSetData && other.gauge == gauge;
  @override
  int get hashCode => gauge.hashCode;
}

final class UIDeltaOperationFooterSetActions extends UIDeltaOperation {
  const UIDeltaOperationFooterSetActions(
      {required this.nodeID, required this.actions, required this.status});
  final String nodeID;
  final List<UIFooterActionSpec> actions;
  final String? status;
  @override
  String get op => 'footerSetActions';
  // Mirrors Swift's encode order: status (when present), nodeId, actions.
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        if (status != null) 'status': status,
        'nodeId': nodeID,
        'actions': actions.map((a) => a.toJson()).toList(),
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationFooterSetActions &&
      other.nodeID == nodeID &&
      _listEq(other.actions, actions) &&
      other.status == status;
  @override
  int get hashCode => Object.hash(nodeID, actions.length, status);
}

final class UIDeltaOperationInputSetValue extends UIDeltaOperation {
  const UIDeltaOperationInputSetValue(
      {required this.nodeID, required this.value});
  final String nodeID;
  final String value;
  @override
  String get op => 'inputSetValue';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'value': value};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationInputSetValue &&
      other.nodeID == nodeID &&
      other.value == value;
  @override
  int get hashCode => Object.hash(nodeID, value);
}

final class UIDeltaOperationListInsertItem extends UIDeltaOperation {
  const UIDeltaOperationListInsertItem(
      {required this.listID, required this.index, required this.item});
  final String listID;
  final int index;
  final UIListItemSpec item;
  @override
  String get op => 'listInsertItem';
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        'listId': listID,
        'index': index,
        'item': item.toJson(),
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationListInsertItem &&
      other.listID == listID &&
      other.index == index &&
      other.item == item;
  @override
  int get hashCode => Object.hash(listID, index, item);
}

final class UIDeltaOperationListSetSelection extends UIDeltaOperation {
  const UIDeltaOperationListSetSelection(
      {required this.listID, required this.selectedID});
  final String listID;
  final String? selectedID;
  @override
  String get op => 'listSetSelection';
  // Mirrors Swift: the `selectedId` key is always emitted, even when null.
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'listId': listID, 'selectedId': selectedID};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationListSetSelection &&
      other.listID == listID &&
      other.selectedID == selectedID;
  @override
  int get hashCode => Object.hash(listID, selectedID);
}

final class UIDeltaOperationListRemoveItem extends UIDeltaOperation {
  const UIDeltaOperationListRemoveItem(
      {required this.listID, required this.itemID});
  final String listID;
  final String itemID;
  @override
  String get op => 'listRemoveItem';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'listId': listID, 'itemId': itemID};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationListRemoveItem &&
      other.listID == listID &&
      other.itemID == itemID;
  @override
  int get hashCode => Object.hash(listID, itemID);
}

final class UIDeltaOperationContentSetSelection extends UIDeltaOperation {
  const UIDeltaOperationContentSetSelection(
      {required this.contentID, required this.selection});
  final String contentID;
  final UIContentSelection? selection;
  @override
  String get op => 'contentSetSelection';
  // Mirrors Swift: the `selection` key is always emitted, even when null.
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        'contentId': contentID,
        'selection': selection?.toJson(),
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationContentSetSelection &&
      other.contentID == contentID &&
      other.selection == selection;
  @override
  int get hashCode => Object.hash(contentID, selection);
}

final class UIDeltaOperationContentSpliceLines extends UIDeltaOperation {
  const UIDeltaOperationContentSpliceLines(
      {required this.contentID,
      required this.index,
      required this.deleteCount,
      required this.lines});
  final String contentID;
  final int index;
  final int deleteCount;
  final List<UIContentLine> lines;
  @override
  String get op => 'contentSpliceLines';
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        'contentId': contentID,
        'index': index,
        'deleteCount': deleteCount,
        'lines': lines.map((l) => l.toJson()).toList(),
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationContentSpliceLines &&
      other.contentID == contentID &&
      other.index == index &&
      other.deleteCount == deleteCount &&
      _listEq(other.lines, lines);
  @override
  int get hashCode => Object.hash(contentID, index, deleteCount, lines.length);
}

final class UIDeltaOperationTreeSetSelection extends UIDeltaOperation {
  const UIDeltaOperationTreeSetSelection(
      {required this.nodeID, required this.selectedID});
  final String nodeID;
  final String? selectedID;
  @override
  String get op => 'treeSetSelection';
  // Mirrors Swift: the `selectedId` key is always emitted, even when null.
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'selectedId': selectedID};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationTreeSetSelection &&
      other.nodeID == nodeID &&
      other.selectedID == selectedID;
  @override
  int get hashCode => Object.hash(nodeID, selectedID);
}

final class UIDeltaOperationTreeSetFilter extends UIDeltaOperation {
  const UIDeltaOperationTreeSetFilter(
      {required this.filterID, required this.value});
  final String filterID;
  final String value;
  @override
  String get op => 'treeSetFilter';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'filterId': filterID, 'value': value};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationTreeSetFilter &&
      other.filterID == filterID &&
      other.value == value;
  @override
  int get hashCode => Object.hash(filterID, value);
}

final class UIDeltaOperationTreeSetLocation extends UIDeltaOperation {
  const UIDeltaOperationTreeSetLocation(
      {required this.nodeID, required this.location});
  final String nodeID;
  final String location;
  @override
  String get op => 'treeSetLocation';
  @override
  Map<String, dynamic> toJson() =>
      {'op': op, 'nodeId': nodeID, 'location': location};
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationTreeSetLocation &&
      other.nodeID == nodeID &&
      other.location == location;
  @override
  int get hashCode => Object.hash(nodeID, location);
}

final class UIDeltaOperationTreeSpliceChildren extends UIDeltaOperation {
  const UIDeltaOperationTreeSpliceChildren(
      {required this.nodeID,
      required this.parentID,
      required this.index,
      required this.deleteCount,
      required this.items});
  final String nodeID;
  final String? parentID;
  final int index;
  final int deleteCount;
  final List<UITreeItem> items;
  @override
  String get op => 'treeSpliceChildren';
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        'nodeId': nodeID,
        if (parentID != null) 'parentId': parentID,
        'index': index,
        'deleteCount': deleteCount,
        'items': items.map((i) => i.toJson()).toList(),
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationTreeSpliceChildren &&
      other.nodeID == nodeID &&
      other.parentID == parentID &&
      other.index == index &&
      other.deleteCount == deleteCount &&
      _listEq(other.items, items);
  @override
  int get hashCode =>
      Object.hash(nodeID, parentID, index, deleteCount, items.length);
}

final class UIDeltaOperationTreeSetChildState extends UIDeltaOperation {
  const UIDeltaOperationTreeSetChildState(
      {required this.nodeID, required this.itemID, required this.childState});
  final String nodeID;
  final String itemID;
  final UITreeChildState childState;
  @override
  String get op => 'treeSetChildState';
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        'nodeId': nodeID,
        'itemId': itemID,
        'childState': childState.toJson(),
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationTreeSetChildState &&
      other.nodeID == nodeID &&
      other.itemID == itemID &&
      other.childState == childState;
  @override
  int get hashCode => Object.hash(nodeID, itemID, childState);
}

final class UIDeltaOperationTreeSetExpanded extends UIDeltaOperation {
  const UIDeltaOperationTreeSetExpanded(
      {required this.nodeID, required this.itemID, required this.expanded});
  final String nodeID;
  final String itemID;
  final bool expanded;
  @override
  String get op => 'treeSetExpanded';
  @override
  Map<String, dynamic> toJson() => {
        'op': op,
        'nodeId': nodeID,
        'itemId': itemID,
        'expanded': expanded,
      };
  @override
  bool operator ==(Object other) =>
      other is UIDeltaOperationTreeSetExpanded &&
      other.nodeID == nodeID &&
      other.itemID == itemID &&
      other.expanded == expanded;
  @override
  int get hashCode => Object.hash(nodeID, itemID, expanded);
}
/// Mirrors Swift `UIDelta`: a contiguous server-to-renderer change. A
/// renderer applies it only when its complete snapshot revision equals
/// `baseRevision`.
final class UIDelta {
  const UIDelta({
    required this.appInstanceID,
    required this.clientID,
    required this.viewID,
    required this.baseRevision,
    required this.revision,
    required this.operations,
    this.protocolVersion = UIProtocol.version,
  });

  final String protocolName = UIProtocol.name;
  final int protocolVersion;
  final String appInstanceID;
  final String clientID;
  final String viewID;
  final int baseRevision;
  final int revision;
  final List<UIDeltaOperation> operations;

  factory UIDelta.fromJson(Map<String, dynamic> json) {
    final baseRevision = json['baseRevision'] as int;
    final revision = json['revision'] as int;
    final operations = (json['operations'] as List)
        .map((o) => UIDeltaOperation.fromJson(o as Map<String, dynamic>))
        .toList();
    if (!(baseRevision >= 0 &&
        revision > baseRevision &&
        operations.length >= 1 &&
        operations.length <= 4096)) {
      throw FormatException(
          'Delta must advance its base with 1...4096 operations', json);
    }
    return UIDelta(
      protocolVersion: json['protocolVersion'] as int,
      appInstanceID: json['appInstanceId'] as String,
      clientID: json['clientId'] as String,
      viewID: json['viewId'] as String,
      baseRevision: baseRevision,
      revision: revision,
      operations: operations,
    );
  }

  Map<String, dynamic> toJson() => {
        'protocol': UIProtocol.name,
        'protocolVersion': protocolVersion,
        'appInstanceId': appInstanceID,
        'clientId': clientID,
        'viewId': viewID,
        'baseRevision': baseRevision,
        'revision': revision,
        'operations': operations.map((o) => o.toJson()).toList(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIDelta &&
      other.protocolVersion == protocolVersion &&
      other.appInstanceID == appInstanceID &&
      other.clientID == clientID &&
      other.viewID == viewID &&
      other.baseRevision == baseRevision &&
      other.revision == revision &&
      _listEq(other.operations, operations);

  @override
  int get hashCode => Object.hash(protocolVersion, appInstanceID, clientID,
      viewID, baseRevision, revision, operations.length);
}

// ---------------------------------------------------------------------------
// Snapshot application
// ---------------------------------------------------------------------------

/// Mirrors Swift's `replacing(_:in:)`: applies a UTF-16 text edit.
String _replacing(UITextEdit edit, String text) {
  if (edit.range.start.compareTo(edit.range.end) > 0) {
    throw UIDeltaApplicationError('Markdown text edit range is reversed');
  }
  final start = utf16OffsetForPosition(edit.range.start, text);
  final end = utf16OffsetForPosition(edit.range.end, text);
  return text.replaceRange(start, end, edit.text);
}

/// Mirrors Swift's `utf16Offset(for:in:)`: resolves a (line, UTF-16 column)
/// position to a UTF-16 code-unit offset. Dart strings are UTF-16 natively,
/// so the offset math maps directly.
///
/// Swift additionally requires the offset to sit on a grapheme-cluster
/// boundary (`String.Index(_:within:)` returns nil otherwise); without a
/// segmenter the portable approximation checked here is the UTF-16 scalar
/// boundary — the offset must not split a surrogate pair.
int utf16OffsetForPosition(UITextPosition position, String text) {
  if (position.line < 0 || position.utf16Column < 0) {
    throw UIDeltaApplicationError('Negative Markdown text position');
  }
  var lineStart = 0;
  for (var i = 0; i < position.line; i++) {
    final newline = text.indexOf('\n', lineStart);
    if (newline < 0) {
      throw UIDeltaApplicationError(
          'Markdown text line is outside the document');
    }
    lineStart = newline + 1;
  }
  var lineEnd = text.indexOf('\n', lineStart);
  if (lineEnd < 0) lineEnd = text.length;
  final target = lineStart + position.utf16Column;
  if (target < 0 || target > lineEnd) {
    throw UIDeltaApplicationError(
        'Markdown UTF-16 column is outside a scalar boundary');
  }
  if (target > 0 &&
      target < text.length &&
      _isLeadSurrogate(text.codeUnitAt(target - 1)) &&
      _isTrailSurrogate(text.codeUnitAt(target))) {
    throw UIDeltaApplicationError(
        'Markdown UTF-16 column is outside a scalar boundary');
  }
  return target;
}

bool _isLeadSurrogate(int c) => c >= 0xD800 && c <= 0xDBFF;
bool _isTrailSurrogate(int c) => c >= 0xDC00 && c <= 0xDFFF;

/// Mirrors Swift's private `MarkdownEditorSpec.copying`, including the
/// optional-change sentinels that distinguish "unchanged" from
/// "set (possibly to null)".
extension _MarkdownEditorDeltaCopy on MarkdownEditorSpec {
  MarkdownEditorSpec copying({
    String? text,
    UITextSelection? selection,
    MarkdownPresentation? presentation,
    bool? readOnly,
    bool? dirty,
    String? placeholder,
    _Set<MarkdownCommandHint?>? commandHint,
    _Set<String?>? title,
    MarkdownEditorActions? actions,
    _Set<UIMenuSpec?>? insertMenu,
    _Set<UIMenuSpec?>? contextMenu,
    UIFooterActionsSpec? footer,
  }) {
    final sel = selection;
    return MarkdownEditorSpec(
      text: text ?? this.text,
      anchorLine: sel?.anchor.line ?? anchorLine,
      anchorColumn: sel?.anchor.utf16Column ?? anchorColumn,
      headLine: sel?.head.line ?? headLine,
      headColumn: sel?.head.utf16Column ?? headColumn,
      presentation: presentation ?? this.presentation,
      readOnly: readOnly ?? this.readOnly,
      dirty: dirty ?? this.dirty,
      placeholder: placeholder ?? this.placeholder,
      commandHint: commandHint == null ? this.commandHint : commandHint.value,
      title: title == null ? this.title : title.value,
      actions: actions ?? this.actions,
      insertMenu: insertMenu == null ? this.insertMenu : insertMenu.value,
      contextMenu: contextMenu == null ? this.contextMenu : contextMenu.value,
      footer: footer ?? this.footer,
    );
  }
}

/// Mirrors Swift's private `MediaSpec.copying`.
extension _MediaDeltaCopy on MediaSpec {
  MediaSpec copying({MediaSource? source, MediaPixelSize? intrinsic}) =>
      MediaSpec(
        source: source ?? this.source,
        intrinsic: intrinsic ?? this.intrinsic,
        cells: cells,
        points: points,
        fit: fit,
        alt: alt,
        activate: activate,
      );
}

/// Mirrors Swift's private `SurfaceSpec.copying`.
extension _SurfaceDeltaCopy on SurfaceSpec {
  SurfaceSpec copying({required SurfaceReference reference}) => SurfaceSpec(
        reference: reference,
        cells: cells,
        points: points,
        background: background,
        inputPolicy: inputPolicy,
      );
}

UIListItemSpec _copyItem(
  UIListItemSpec item, {
  UIListItemSlot? leading,
  UIListItemSlot? trailing,
  UIListItemSlot? accessory,
  bool? done,
}) =>
    UIListItemSpec(
      id: item.id,
      label: item.label,
      labelRuns: item.labelRuns,
      labelTone: item.labelTone,
      emphasis: item.emphasis,
      detail: item.detail,
      detailRuns: item.detailRuns,
      value: item.value,
      valueRuns: item.valueRuns,
      valueTone: item.valueTone,
      valueMinWidth: item.valueMinWidth,
      done: done ?? item.done,
      busy: item.busy,
      leading: leading ?? item.leading,
      trailing: trailing ?? item.trailing,
      accessory: accessory ?? item.accessory,
      top: item.top,
      bottom: item.bottom,
      media: item.media,
      divider: item.divider,
      delete: item.delete,
      activate: item.activate,
      actionRole: item.actionRole,
    );

UIListSpec _copyList(
  UIListSpec list, {
  List<UIListItemSpec>? items,
  _Set<String?>? selectedID,
}) =>
    UIListSpec(
      id: list.id,
      items: items ?? list.items,
      emptyMessage: list.emptyMessage,
      selectedID: selectedID == null ? list.selectedID : selectedID.value,
      select: list.select,
      scrollPadding: list.scrollPadding,
      pageOverlap: list.pageOverlap,
      pageBehavior: list.pageBehavior,
      spacePagesDown: list.spacePagesDown,
      rowLayout: list.rowLayout,
      contextMenu: list.contextMenu,
    );

UIContentSpec _copyContent(
  UIContentSpec content, {
  List<UIContentLine>? lines,
  _Set<UIContentSelection?>? selection,
}) =>
    UIContentSpec(
      id: content.id,
      label: content.label,
      lines: lines ?? content.lines,
      wrap: content.wrap,
      font: content.font,
      emptyMessage: content.emptyMessage,
      selection: selection == null ? content.selection : selection.value,
      select: content.select,
      contextMenu: content.contextMenu,
    );

PageSpec _copyPage(
  PageSpec page, {
  UIPageHeaderSlot? header,
  UIPageBodySlot? body,
  UIFooterActionsSpec? footer,
}) =>
    PageSpec(
      title: page.title,
      tabs: page.tabs,
      toolbar: page.toolbar,
      back: page.back,
      header: header ?? page.header,
      body: body ?? page.body,
      footer: footer ?? page.footer,
    );

UITreeItem _copyTreeItem(
  UITreeItem item, {
  List<UITreeItem>? children,
  UITreeChildState? childState,
  bool? expanded,
}) =>
    UITreeItem(
      id: item.id,
      label: item.label,
      kind: item.kind,
      detail: item.detail,
      hidden: item.hidden,
      symlink: item.symlink,
      childState: childState ?? item.childState,
      expanded: expanded ?? item.expanded,
      children: children ?? item.children,
    );

UITreeSpec _copyTree(
  UITreeSpec tree, {
  String? location,
  UITreeFilter? filter,
  List<UITreeItem>? items,
  _Set<String?>? selectedID,
  UIFooterActionsSpec? footer,
}) =>
    UITreeSpec(
      label: tree.label,
      location: location ?? tree.location,
      presentation: tree.presentation,
      filter: filter ?? tree.filter,
      items: items ?? tree.items,
      selectedID: selectedID == null ? tree.selectedID : selectedID.value,
      emptyMessage: tree.emptyMessage,
      primaryAction: tree.primaryAction,
      contextMenu: tree.contextMenu,
      actions: tree.actions,
      footer: footer ?? tree.footer,
    );

/// Mirrors Swift's `flattenTreeItems`.
List<String> _flattenTreeIds(List<UITreeItem> items) =>
    items.expand((item) => [item.id, ..._flattenTreeIds(item.children)]).toList();

/// Mirrors Swift's `updateTreeItem`: functionally rebuilds the path to the
/// item with matching id. Returns false when no item matched.
bool _updateTreeItem(List<UITreeItem> items, String id,
    UITreeItem Function(UITreeItem) update) {
  for (var i = 0; i < items.length; i++) {
    if (items[i].id == id) {
      items[i] = update(items[i]);
      return true;
    }
    final children = items[i].children.toList();
    if (_updateTreeItem(children, id, update)) {
      items[i] = _copyTreeItem(items[i], children: children);
      return true;
    }
  }
  return false;
}

/// Mirrors Swift's `spliceTreeChildren`. Returns false when the parent is
/// missing or is not a directory, or when the range is out of bounds.
bool _spliceTreeChildren(List<UITreeItem> items, String parentID, int index,
    int deleteCount, List<UITreeItem> replacement) {
  for (var i = 0; i < items.length; i++) {
    if (items[i].id == parentID) {
      final item = items[i];
      if (!(item.kind == UITreeItemKind.directory &&
          index <= item.children.length &&
          deleteCount <= item.children.length - index)) {
        return false;
      }
      final children = item.children.toList()
        ..replaceRange(index, index + deleteCount, replacement);
      items[i] = _copyTreeItem(item, children: children);
      return true;
    }
    final children = items[i].children.toList();
    if (_spliceTreeChildren(children, parentID, index, deleteCount, replacement)) {
      items[i] = _copyTreeItem(items[i], children: children);
      return true;
    }
  }
  return false;
}

T? _firstWhereOrNull<T>(Iterable<T> items, bool Function(T) test) {
  for (final item in items) {
    if (test(item)) return item;
  }
  return null;
}

/// Mirrors Swift's `setToggle`: returns the updated slot and whether it
/// matched.
(UIListItemSlot?, bool) _setToggle(UIListItemSlot? slot, String id, bool value) {
  final s = slot;
  if (s is UIListItemSlotToggle && s.value.id == id) {
    final toggle = s.value;
    // Mirrors Swift: role is NOT carried over, so it resets to the default
    // (completion), exactly as the Swift helper reconstructs the spec.
    return (
      UIListItemSlotToggle(UIToggleSpec(
        id: toggle.id,
        label: toggle.label,
        value: value,
        setValue: toggle.setValue,
      )),
      true
    );
  }
  return (slot, false);
}

/// Mirrors Swift's `setCheckmark`.
(UIListItemSlot?, bool) _setCheckmark(
    UIListItemSlot? slot, String id, bool value) {
  final s = slot;
  if (s is UIListItemSlotCheckmark && s.value.id == id) {
    final checkmark = s.value;
    return (
      UIListItemSlotCheckmark(UICheckmarkSpec(
        id: checkmark.id,
        label: checkmark.label,
        value: value,
        setValue: checkmark.setValue,
      )),
      true
    );
  }
  return (slot, false);
}

/// Applies one delta operation to a node, mirroring Swift's private
/// `UINode.applying(_:)`. The input node is never mutated; a new node is
/// returned. Throws [UIDeltaApplicationError] when the operation targets an
/// unavailable node or carries invalid data.
extension UIDeltaNodeApplication on UINode {
  UINode applying(UIDeltaOperation operation) {
    switch (operation) {
      case UIDeltaOperationReplaceRoot(:final root):
        return root;
      case UIDeltaOperationMarkdownReplaceRange(
          :final nodeID,
          :final edit
        ):
        final editor = _markdownEditor(nodeID);
        return UINode(
            id: id,
            component: UIComponentMarkdownEditor(
                editor.copying(text: _replacing(edit, editor.text))));
      case UIDeltaOperationMarkdownSetSelection(
          :final nodeID,
          :final selection
        ):
        final editor = _markdownEditor(nodeID);
        return UINode(
            id: id,
            component:
                UIComponentMarkdownEditor(editor.copying(selection: selection)));
      case UIDeltaOperationMarkdownSetPresentation(
          :final nodeID,
          :final presentation
        ):
        final editor = _markdownEditor(nodeID);
        return UINode(
            id: id,
            component: UIComponentMarkdownEditor(
                editor.copying(presentation: presentation)));
      case UIDeltaOperationMarkdownSetDirty(:final nodeID, :final dirty):
        final editor = _markdownEditor(nodeID);
        return UINode(
            id: id,
            component:
                UIComponentMarkdownEditor(editor.copying(dirty: dirty)));
      case UIDeltaOperationMarkdownSetReadOnly(
          :final nodeID,
          :final readOnly
        ):
        final editor = _markdownEditor(nodeID);
        return UINode(
            id: id,
            component: UIComponentMarkdownEditor(
                editor.copying(readOnly: readOnly)));
      case UIDeltaOperationMarkdownSetTitle(:final nodeID, :final title):
        final editor = _markdownEditor(nodeID);
        return UINode(
            id: id,
            component: UIComponentMarkdownEditor(
                editor.copying(title: _Set<String?>(title))));
      case UIDeltaOperationMarkdownSetPlaceholder(
          :final nodeID,
          :final placeholder
        ):
        final editor = _markdownEditor(nodeID);
        return UINode(
            id: id,
            component: UIComponentMarkdownEditor(
                editor.copying(placeholder: placeholder)));
      case UIDeltaOperationMarkdownSetCommandHint(
          :final nodeID,
          :final commandHint
        ):
        final editor = _markdownEditor(nodeID);
        return UINode(
            id: id,
            component: UIComponentMarkdownEditor(editor.copying(
                commandHint: _Set<MarkdownCommandHint?>(commandHint))));
      case UIDeltaOperationMarkdownSetActions(
          :final nodeID,
          :final actions
        ):
        final editor = _markdownEditor(nodeID);
        return UINode(
            id: id,
            component: UIComponentMarkdownEditor(
                editor.copying(actions: actions)));
      case UIDeltaOperationMarkdownSetMenus(
          :final nodeID,
          :final insertMenu,
          :final contextMenu
        ):
        final editor = _markdownEditor(nodeID);
        return UINode(
            id: id,
            component: UIComponentMarkdownEditor(editor.copying(
              insertMenu: _Set<UIMenuSpec?>(insertMenu),
              contextMenu: _Set<UIMenuSpec?>(contextMenu),
            )));
      case UIDeltaOperationMenuSetSelection(
          :final nodeID,
          :final selectedID
        ):
        final menu = _menu(nodeID);
        final selected = selectedID == null
            ? null
            : _firstWhereOrNull(menu.items, (i) => i.id == selectedID);
        if (!(selectedID == null ||
            (selected != null && !selected.disabled))) {
          throw UIDeltaApplicationError(
              'Delta selects an unavailable Menu item');
        }
        return UINode(
            id: id,
            component: UIComponentMenu(UIMenuSpec(
              label: menu.label,
              presentation: menu.presentation,
              anchor: menu.anchor,
              items: menu.items,
              selectedID: selectedID,
              dismiss: menu.dismiss,
            )));
      case UIDeltaOperationMediaSetSource(
          :final nodeID,
          :final source,
          :final intrinsic
        ):
        final media = _media(nodeID);
        return UINode(
            id: id,
            component: UIComponentMedia(
                media.copying(source: source, intrinsic: intrinsic)));
      case UIDeltaOperationSurfaceSetReference(
          :final nodeID,
          :final reference
        ):
        final c = component;
        if (c is UIComponentSurface && id == nodeID) {
          return UINode(
              id: id,
              component:
                  UIComponentSurface(c.surface.copying(reference: reference)));
        }
        if (c is UIComponentCanvasPage && c.page.surface.id == nodeID) {
          final nested =
              c.page.surface.surface.copying(reference: reference);
          return UINode(
              id: id,
              component: UIComponentCanvasPage(CanvasPageSpec(
                title: c.page.title,
                surface: UICanvasSurfaceSpec(
                    id: c.page.surface.id, surface: nested),
                controls: c.page.controls,
              )));
        }
        throw UIDeltaApplicationError(
            'Delta targets an unavailable Surface node');
      case UIDeltaOperationToggleSetValue(:final nodeID, :final value):
        return _applyingToListSlot(
            nodeID, 'Toggle', _setToggle, (item) => _copyItem(item, done: value),
            value: value);
      case UIDeltaOperationCheckmarkSetValue(:final nodeID, :final value):
        return _applyingToListSlot(
            nodeID, 'Checkmark', _setCheckmark, (item) => item,
            value: value);
      case UIDeltaOperationSparklineSetData(:final sparkline):
        return _applyingChartData(
          'Sparkline',
          ifBodyMatches: (body) => body is UIPageBodySlotSparkline &&
                  body.sparkline.id == sparkline.id
              ? UISparklineSpec(
                  id: sparkline.id,
                  series: sparkline.series,
                  min: sparkline.min,
                  max: sparkline.max,
                  caption: sparkline.caption,
                  unit: sparkline.unit,
                  accessibilityText: sparkline.accessibilityText,
                  activate: body.sparkline.activate,
                )
              : null,
          wrapBody: (s) => UIPageBodySlotSparkline(s),
          matchSlot: (slot) =>
              slot is UIListItemSlotSparkline && slot.value.id == sparkline.id
                  ? UISparklineSpec(
                      id: sparkline.id,
                      series: sparkline.series,
                      min: sparkline.min,
                      max: sparkline.max,
                      caption: sparkline.caption,
                      unit: sparkline.unit,
                      accessibilityText: sparkline.accessibilityText,
                      activate: slot.value.activate,
                    )
                  : null,
          wrapSlot: (slot, s) => slot is UIListItemSlotSparkline
              ? UIListItemSlotSparkline(s as UISparklineSpec)
              : slot,
        );
      case UIDeltaOperationBarChartSetData(:final chart):
        final page = _page();
        final body = page.body;
        if (body is UIPageBodySlotBarChart && body.chart.id == chart.id) {
          return UINode(
              id: id,
              component: UIComponentPage(_copyPage(page,
                  body: UIPageBodySlotBarChart(UIBarChartSpec(
                    id: chart.id,
                    bars: chart.bars,
                    accessibilityText: chart.accessibilityText,
                    activate: body.chart.activate,
                  )))));
        }
        throw UIDeltaApplicationError(
            'Delta targets an unavailable BarChart');
      case UIDeltaOperationLineChartSetData(:final chart):
        final page = _page();
        final body = page.body;
        if (body is UIPageBodySlotLineChart && body.chart.id == chart.id) {
          return UINode(
              id: id,
              component: UIComponentPage(_copyPage(page,
                  body: UIPageBodySlotLineChart(UILineChartSpec(
                    id: chart.id,
                    series: chart.series,
                    xAxis: chart.xAxis,
                    yAxis: chart.yAxis,
                    accessibilityText: chart.accessibilityText,
                    activate: body.chart.activate,
                  )))));
        }
        throw UIDeltaApplicationError(
            'Delta targets an unavailable LineChart');
      case UIDeltaOperationGaugeSetData(:final gauge):
        return _applyingChartData(
          'Gauge',
          ifBodyMatches: (body) =>
              body is UIPageBodySlotGauge && body.gauge.id == gauge.id
                  ? UIGaugeSpec(
                      id: gauge.id,
                      ratio: gauge.ratio,
                      label: gauge.label,
                      caption: gauge.caption,
                      accessibilityText: gauge.accessibilityText,
                      activate: body.gauge.activate,
                    )
                  : null,
          wrapBody: (g) => UIPageBodySlotGauge(g),
          matchSlot: (slot) =>
              slot is UIListItemSlotGauge && slot.value.id == gauge.id
                  ? UIGaugeSpec(
                      id: gauge.id,
                      ratio: gauge.ratio,
                      label: gauge.label,
                      caption: gauge.caption,
                      accessibilityText: gauge.accessibilityText,
                      activate: slot.value.activate,
                    )
                  : null,
          wrapSlot: (slot, g) => slot is UIListItemSlotGauge
              ? UIListItemSlotGauge(g as UIGaugeSpec)
              : slot,
        );
      case UIDeltaOperationFooterSetActions(
          :final nodeID,
          :final actions,
          :final status
        ):
        if (id != nodeID) {
          throw UIDeltaApplicationError(
              'Delta targets an unavailable footer root');
        }
        final footer =
            UIFooterActionsSpec(actions: actions, status: status);
        if (!footer.isValid) {
          throw UIDeltaApplicationError(
              'Delta carries invalid FooterActions');
        }
        final c = component;
        if (c is UIComponentPage) {
          return UINode(
              id: id,
              component: UIComponentPage(_copyPage(c.page, footer: footer)));
        }
        if (c is UIComponentTree) {
          return UINode(
              id: id,
              component: UIComponentTree(_copyTree(c.tree, footer: footer)));
        }
        if (c is UIComponentMarkdownEditor) {
          return UINode(
              id: id,
              component: UIComponentMarkdownEditor(
                  c.editor.copying(footer: footer)));
        }
        throw UIDeltaApplicationError('Delta root has no FooterActions slot');
      case UIDeltaOperationInputSetValue(:final nodeID, :final value):
        final page = _page();
        final header = page.header;
        if (header is! UIPageHeaderSlotInput || header.input.id != nodeID) {
          throw UIDeltaApplicationError('Delta targets an unavailable Input');
        }
        final input = header.input;
        return UINode(
            id: id,
            component: UIComponentPage(_copyPage(page,
                header: UIPageHeaderSlotInput(UIInputSpec(
                  id: input.id,
                  label: input.label,
                  value: value,
                  placeholder: input.placeholder,
                  setValue: input.setValue,
                  submit: input.submit,
                )))));
      case UIDeltaOperationListInsertItem(
          :final listID,
          :final index,
          :final item
        ):
        final list = _list(listID, 'List insertion');
        if (index < 0 || index > list.items.length) {
          throw UIDeltaApplicationError(
              'Delta targets an unavailable List insertion');
        }
        final items = list.items.toList()..insert(index, item);
        return _pageNodeWithList(list, items);
      case UIDeltaOperationListRemoveItem(
          :final listID,
          :final itemID
        ):
        final list = _list(listID, 'ListItem');
        var index = -1;
        for (var i = 0; i < list.items.length; i++) {
          if (list.items[i].id == itemID) {
            index = i;
            break;
          }
        }
        if (index < 0) {
          throw UIDeltaApplicationError(
              'Delta targets an unavailable ListItem');
        }
        final items = list.items.toList()..removeAt(index);
        return _pageNodeWithList(list, items);
      case UIDeltaOperationListSetSelection(
          :final listID,
          :final selectedID
        ):
        final list = _list(listID, 'List selection');
        if (!(selectedID == null ||
            list.items.any((i) => i.id == selectedID))) {
          throw UIDeltaApplicationError(
              'Delta targets an unavailable List selection');
        }
        return _pageNodeWithList(list, list.items,
            selectedID: _Set<String?>(selectedID));
      case UIDeltaOperationContentSetSelection(
          :final contentID,
          :final selection
        ):
        final content = _content(contentID);
        final ids = content.lines.map((l) => l.id).toSet();
        if (!(selection == null ||
            (ids.contains(selection.anchorID) &&
                ids.contains(selection.headID)))) {
          throw UIDeltaApplicationError(
              'Delta selects an unavailable Content line');
        }
        final page = _page();
        return UINode(
            id: id,
            component: UIComponentPage(_copyPage(page,
                body: UIPageBodySlotContent(_copyContent(content,
                    selection: _Set<UIContentSelection?>(selection))))));
      case UIDeltaOperationContentSpliceLines(
          :final contentID,
          :final index,
          :final deleteCount,
          :final lines
        ):
        // Mirrors Swift's single guard: a body/id mismatch and a bad range
        // share the "outside its collection" error.
        final page = _page();
        final body = page.body;
        if (body is! UIPageBodySlotContent ||
            body.content.id != contentID) {
          throw UIDeltaApplicationError(
              'Content splice is outside its collection');
        }
        final content = body.content;
        if (!(index >= 0 &&
            deleteCount >= 0 &&
            index <= content.lines.length &&
            deleteCount <= content.lines.length - index)) {
          throw UIDeltaApplicationError(
              'Content splice is outside its collection');
        }
        final newLines = content.lines.toList()
          ..replaceRange(index, index + deleteCount, lines);
        return UINode(
            id: id,
            component: UIComponentPage(_copyPage(page,
                body: UIPageBodySlotContent(
                    _copyContent(content, lines: newLines)))));
      case UIDeltaOperationTreeSetSelection(
          :final nodeID,
          :final selectedID
        ):
        final tree = _tree(nodeID);
        final ids = _flattenTreeIds(tree.items).toSet();
        if (!(selectedID == null || ids.contains(selectedID))) {
          throw UIDeltaApplicationError(
              'Delta selects an unavailable Tree item');
        }
        return UINode(
            id: id,
            component: UIComponentTree(
                _copyTree(tree, selectedID: _Set<String?>(selectedID))));
      case UIDeltaOperationTreeSetFilter(
          :final filterID,
          :final value
        ):
        // Mirrors Swift: operates on the root component directly; nodeID is
        // not consulted.
        final c = component;
        if (c is! UIComponentTree) {
          throw UIDeltaApplicationError(
              'Delta targets an unavailable Tree filter');
        }
        final filter = c.tree.filter;
        if (filter == null || filter.id != filterID) {
          throw UIDeltaApplicationError(
              'Delta targets an unavailable Tree filter');
        }
        return UINode(
            id: id,
            component: UIComponentTree(_copyTree(c.tree,
                filter: UITreeFilter(
                  id: filter.id,
                  label: filter.label,
                  value: value,
                  placeholder: filter.placeholder,
                  setValue: filter.setValue,
                ))));
      case UIDeltaOperationTreeSetLocation(
          :final nodeID,
          :final location
        ):
        final tree = _tree(nodeID);
        return UINode(
            id: id,
            component:
                UIComponentTree(_copyTree(tree, location: location)));
      case UIDeltaOperationTreeSpliceChildren(
          :final nodeID,
          :final parentID,
          :final index,
          :final deleteCount,
          :final items
        ):
        final tree = _tree(nodeID);
        if (!(index >= 0 && deleteCount >= 0)) {
          throw UIDeltaApplicationError('Tree splice has a negative range');
        }
        final newItems = tree.items.toList();
        if (parentID != null) {
          if (!_spliceTreeChildren(
              newItems, parentID, index, deleteCount, items)) {
            throw UIDeltaApplicationError(
                'Delta targets unavailable Tree children');
          }
        } else {
          if (!(index <= newItems.length &&
              deleteCount <= newItems.length - index)) {
            throw UIDeltaApplicationError(
                'Tree root splice is outside its collection');
          }
          newItems.replaceRange(index, index + deleteCount, items);
        }
        final next = _copyTree(tree, items: newItems);
        if (next.requiredCapabilities == null) {
          throw UIDeltaApplicationError(
              'Tree splice produced an invalid hierarchy');
        }
        return UINode(id: id, component: UIComponentTree(next));
      case UIDeltaOperationTreeSetChildState(
          :final nodeID,
          :final itemID,
          :final childState
        ):
        final tree = _tree(nodeID);
        final newItems = tree.items.toList();
        final updated = _updateTreeItem(newItems, itemID, (item) {
          return _copyTreeItem(item,
              childState: childState,
              children: childState != UITreeChildState.loaded
                  ? const <UITreeItem>[]
                  : item.children);
        });
        if (!updated) {
          throw UIDeltaApplicationError(
              'Delta targets an unavailable Tree item');
        }
        return UINode(
            id: id,
            component: UIComponentTree(_copyTree(tree, items: newItems)));
      case UIDeltaOperationTreeSetExpanded(
          :final nodeID,
          :final itemID,
          :final expanded
        ):
        final tree = _tree(nodeID);
        final newItems = tree.items.toList();
        final updated = _updateTreeItem(
            newItems, itemID, (item) => _copyTreeItem(item, expanded: expanded));
        final next = _copyTree(tree, items: newItems);
        if (!(updated && next.requiredCapabilities != null)) {
          throw UIDeltaApplicationError(
              'Delta targets an unavailable expandable Tree item');
        }
        return UINode(id: id, component: UIComponentTree(next));
    }
  }

  MarkdownEditorSpec _markdownEditor(String nodeID) {
    final c = component;
    if (id == nodeID && c is UIComponentMarkdownEditor) return c.editor;
    throw UIDeltaApplicationError(
        'Delta targets an unavailable Markdown node');
  }

  MediaSpec _media(String nodeID) {
    final c = component;
    if (id == nodeID && c is UIComponentMedia) return c.media;
    throw UIDeltaApplicationError('Delta targets an unavailable Media node');
  }

  UIMenuSpec _menu(String nodeID) {
    final c = component;
    if (id == nodeID && c is UIComponentMenu) return c.menu;
    throw UIDeltaApplicationError('Delta targets an unavailable Menu node');
  }

  PageSpec _page() {
    final c = component;
    if (c is UIComponentPage) return c.page;
    throw UIDeltaApplicationError('Delta targets an unavailable Page');
  }

  UITreeSpec _tree(String nodeID) {
    final c = component;
    if (id == nodeID && c is UIComponentTree) return c.tree;
    throw UIDeltaApplicationError('Delta targets an unavailable Tree node');
  }

  /// The page-body list with matching id, or throws naming [what].
  UIListSpec _list(String listID, String what) {
    final page = _page();
    final body = page.body;
    if (body is UIPageBodySlotList && body.list.id == listID) {
      return body.list;
    }
    throw UIDeltaApplicationError('Delta targets an unavailable $what');
  }

  UIContentSpec _content(String contentID) {
    final page = _page();
    final body = page.body;
    if (body is UIPageBodySlotContent && body.content.id == contentID) {
      return body.content;
    }
    throw UIDeltaApplicationError(
        'Delta targets unavailable Content selection');
  }

  /// Rebuilds the page node with a list's items (and optional selection)
  /// replaced, mirroring Swift's `list.items`/`list.selectedID` mutation.
  UINode _pageNodeWithList(UIListSpec list, List<UIListItemSpec> items,
      {_Set<String?>? selectedID}) {
    final page = _page();
    return UINode(
        id: id,
        component: UIComponentPage(_copyPage(page,
            body: UIPageBodySlotList(
                _copyList(list, items: items, selectedID: selectedID)))));
  }

  /// Shared shape of the toggle/checkmark delta cases: find the slot with
  /// matching id across leading/trailing/accessory of each list item, update
  /// it, and rebuild the page node.
  UINode _applyingToListSlot(
    String nodeID,
    String what,
    (UIListItemSlot?, bool) Function(UIListItemSlot?, String, bool) setSlot,
    UIListItemSpec Function(UIListItemSpec) finishItem, {
    required bool value,
  }) {
    final list = _listForSlot();
    final items = list.items.toList();
    var found = false;
    for (var index = 0; index < items.length && !found; index++) {
      final item = items[index];
      final (leading, lFound) = setSlot(item.leading, nodeID, value);
      final (trailing, tFound) = setSlot(item.trailing, nodeID, value);
      final (accessory, aFound) = setSlot(item.accessory, nodeID, value);
      if (lFound || tFound || aFound) {
        items[index] = finishItem(_copyItem(item,
            leading: leading, trailing: trailing, accessory: accessory));
        found = true;
      }
    }
    if (!found) {
      throw UIDeltaApplicationError('Delta targets an unavailable $what');
    }
    return _pageNodeWithList(list, items);
  }

  UIListSpec _listForSlot() {
    final page = _page();
    final body = page.body;
    if (body is UIPageBodySlotList) return body.list;
    throw UIDeltaApplicationError('Delta targets an unavailable List');
  }

  /// Shared shape of the sparkline/gauge delta cases: the chart data either
  /// replaces a page-body chart with matching id (preserving `activate`), or
  /// the matching trailing slot of a list item.
  UINode _applyingChartData<T>(
    String what, {
    required T? Function(UIPageBodySlot) ifBodyMatches,
    required UIPageBodySlot Function(T) wrapBody,
    required T? Function(UIListItemSlot) matchSlot,
    required UIListItemSlot Function(UIListItemSlot, T) wrapSlot,
  }) {
    final page = _page();
    final body = page.body;
    final bodyData = ifBodyMatches(body);
    if (bodyData != null) {
      return UINode(
          id: id,
          component:
              UIComponentPage(_copyPage(page, body: wrapBody(bodyData))));
    }
    if (body is! UIPageBodySlotList) {
      throw UIDeltaApplicationError('Delta targets an unavailable List');
    }
    final list = body.list;
    final items = list.items.toList();
    var found = false;
    for (var index = 0; index < items.length && !found; index++) {
      final item = items[index];
      final trailing = item.trailing;
      if (trailing != null) {
        final data = matchSlot(trailing);
        if (data != null) {
          items[index] =
              _copyItem(item, trailing: wrapSlot(trailing, data));
          found = true;
        }
      }
    }
    if (!found) {
      throw UIDeltaApplicationError('Delta targets an unavailable $what');
    }
    return _pageNodeWithList(list, items);
  }
}
