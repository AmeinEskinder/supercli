/// UIDelta incremental application ported from Swift to Dart.
///
/// Port of `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIDelta.swift`
/// (`UISnapshot.applying(_:)`, `UINode.applying(_:)`, and helpers).
///
/// A delta applies to a snapshot only when:
/// - the route envelope matches (protocol name/version, app instance, client,
///   view ID), and
/// - the delta is contiguous (`snapshot.revision == delta.baseRevision` and
///   `delta.revision > delta.baseRevision`), and
/// - it carries 1…4096 operations.
///
/// Each operation is applied in order to produce a new immutable snapshot;
/// the original is never mutated. Failures throw
/// [AppKitDeltaApplicationError], mirroring Swift `UIDeltaApplicationError`.
library;

import 'appkit_delta.dart';
import 'appkit_protocol.dart';

/// Applies a route-matched contiguous delta and returns the new snapshot.
///
/// Mirrors Swift `UISnapshot.applying(_:)`.
extension AppKitSnapshotApply on AppKitSnapshot {
  AppKitSnapshot applying(AppKitDelta delta) {
    if (protocolName != delta.protocolName ||
        protocolVersion != delta.protocolVersion ||
        appInstanceId != delta.appInstanceId ||
        clientId != delta.clientId ||
        viewId != delta.viewId) {
      throw const AppKitDeltaApplicationError(
          'Delta route does not match the current snapshot');
    }
    if (revision != delta.baseRevision || delta.revision <= delta.baseRevision) {
      throw const AppKitDeltaApplicationError(
          'Delta is not contiguous with the current snapshot');
    }
    if (delta.operations.isEmpty || delta.operations.length > 4096) {
      throw const AppKitDeltaApplicationError(
          'Delta must contain 1...4096 operations');
    }

    var root = this.root;
    for (final operation in delta.operations) {
      root = root.applying(operation);
    }
    return AppKitSnapshot(
      protocolName: delta.protocolName,
      protocolVersion: delta.protocolVersion,
      appInstanceId: delta.appInstanceId,
      clientId: delta.clientId,
      viewId: delta.viewId,
      revision: delta.revision,
      root: root,
    );
  }
}

/// Applies a single delta operation to a node, returning the new node.
///
/// Mirrors Swift `UINode.applying(_:)` (private extension).
extension AppKitNodeApply on AppKitNode {
  AppKitNode applying(AppKitDeltaOperation operation) {
    switch (operation) {
      case DeltaReplaceRoot(:final root):
        return root;

      case DeltaMarkdownReplaceRange(:final nodeId, :final edit):
        final editor = _markdownEditor(nodeId);
        final newText = _replacing(edit, editor.text);
        return AppKitNode(
            id: id,
            component: AppKitMarkdownEditor(editor.copyWith(text: newText)));

      case DeltaMarkdownSetSelection(:final nodeId, :final selection):
        final editor = _markdownEditor(nodeId);
        return AppKitNode(
            id: id,
            component:
                AppKitMarkdownEditor(editor.copyWith(selection: selection)));

      case DeltaMarkdownSetPresentation(:final nodeId, :final presentation):
        final editor = _markdownEditor(nodeId);
        return AppKitNode(
            id: id,
            component: AppKitMarkdownEditor(
                editor.copyWith(presentation: presentation)));

      case DeltaMarkdownSetDirty(:final nodeId, :final dirty):
        final editor = _markdownEditor(nodeId);
        return AppKitNode(
            id: id,
            component:
                AppKitMarkdownEditor(editor.copyWith(dirty: dirty)));

      case DeltaMarkdownSetReadOnly(:final nodeId, :final readOnly):
        final editor = _markdownEditor(nodeId);
        return AppKitNode(
            id: id,
            component: AppKitMarkdownEditor(
                editor.copyWith(readOnly: readOnly)));

      case DeltaMarkdownSetTitle(:final nodeId, :final title):
        final editor = _markdownEditor(nodeId);
        return AppKitNode(
            id: id,
            component: AppKitMarkdownEditor(editor.copyWith(
              title: title,
              clearTitle: title == null,
            )));

      case DeltaMarkdownSetPlaceholder(:final nodeId, :final placeholder):
        final editor = _markdownEditor(nodeId);
        return AppKitNode(
            id: id,
            component: AppKitMarkdownEditor(
                editor.copyWith(placeholder: placeholder)));

      case DeltaMarkdownSetCommandHint(:final nodeId, :final commandHint):
        final editor = _markdownEditor(nodeId);
        return AppKitNode(
            id: id,
            component: AppKitMarkdownEditor(
                editor.copyWith(commandHint: commandHint)));

      case DeltaMarkdownSetActions(:final nodeId, :final actions):
        final editor = _markdownEditor(nodeId);
        return AppKitNode(
            id: id,
            component:
                AppKitMarkdownEditor(editor.copyWith(actions: actions)));

      case DeltaMarkdownSetMenus(:final nodeId, :final insertMenu, :final contextMenu):
        final editor = _markdownEditor(nodeId);
        return AppKitNode(
            id: id,
            component: AppKitMarkdownEditor(editor.copyWith(
              insertMenu: insertMenu,
              clearInsertMenu: insertMenu == null,
              contextMenu: contextMenu,
              clearContextMenu: contextMenu == null,
            )));

      case DeltaMenuSetSelection(:final nodeId, :final selectedId):
        final menu = _menu(nodeId);
        final selected =
            selectedId == null ? null : menu.items.where((i) => i.id == selectedId).firstOrNull;
        if (selectedId != null && (selected == null || selected.disabled)) {
          throw const AppKitDeltaApplicationError(
              'Delta selects an unavailable Menu item');
        }
        return AppKitNode(
            id: id,
            component: AppKitMenu(menu.copyWith(selectedId: selectedId)));

      case DeltaMediaSetSource(:final nodeId, :final source, :final intrinsic):
        final media = _media(nodeId);
        // The wire `source` is a MediaSource object; the Dart render subset
        // keeps the source URI string. Preserve the URI when present.
        final uri = source['uri'] as String? ?? source['url'] as String?;
        return AppKitNode(
            id: id,
            component: AppKitMedia(media.copyWith(
              source: uri ?? media.source,
              intrinsic: intrinsic,
            )));

      case DeltaSurfaceSetReference(:final nodeId, :final reference):
        final c = component;
        if (c is AppKitSurface && id == nodeId) {
          return AppKitNode(
              id: id,
              component: AppKitSurface(c.spec.copyWith(reference: reference)));
        }
        // CanvasPage surfaces are not in the Dart render subset; reject.
        throw const AppKitDeltaApplicationError(
            'Delta targets an unavailable Surface node');

      case DeltaToggleSetValue(:final nodeId, :final value):
        final page = _page();
        final body = page.body;
        if (body is! PageBodyList) {
          throw const AppKitDeltaApplicationError(
              'Delta targets an unavailable Toggle');
        }
        final list = body.list;
        var found = false;
        final items = list.items.map((item) {
          if (found) return item;
          var updated = item;
          var itemFound = false;
          updated = updated.copyWith(
              leading: _setToggle(item.leading, nodeId, value, (v) => itemFound = v));
          updated = updated.copyWith(
              trailing: _setToggle(item.trailing, nodeId, value, (v) => itemFound = v));
          updated = updated.copyWith(
              accessory: _setToggle(item.accessory, nodeId, value, (v) => itemFound = v));
          if (itemFound) {
            found = true;
            return updated.copyWith(done: value);
          }
          return item;
        }).toList();
        if (!found) {
          throw const AppKitDeltaApplicationError(
              'Delta targets an unavailable Toggle');
        }
        return AppKitNode(
            id: id,
            component: AppKitPage(PageSpec(
              title: page.title,
              tabs: page.tabs,
              toolbar: page.toolbar,
              back: page.back,
              header: page.header,
              body: PageBodyList(list.copyWith(items: items)),
              footer: page.footer,
            )));

      case DeltaCheckmarkSetValue(:final nodeId, :final value):
        final page = _page();
        final body = page.body;
        if (body is! PageBodyList) {
          throw const AppKitDeltaApplicationError(
              'Delta targets an unavailable Checkmark');
        }
        final list = body.list;
        var found = false;
        final items = list.items.map((item) {
          if (found) return item;
          var itemFound = false;
          final leading =
              _setCheckmark(item.leading, nodeId, value, (v) => itemFound = v);
          final trailing =
              _setCheckmark(item.trailing, nodeId, value, (v) => itemFound = v);
          final accessory = _setCheckmark(
              item.accessory, nodeId, value, (v) => itemFound = v);
          if (itemFound) {
            found = true;
            return item.copyWith(
                leading: leading, trailing: trailing, accessory: accessory);
          }
          return item;
        }).toList();
        if (!found) {
          throw const AppKitDeltaApplicationError(
              'Delta targets an unavailable Checkmark');
        }
        return AppKitNode(
            id: id,
            component: AppKitPage(PageSpec(
              title: page.title,
              tabs: page.tabs,
              toolbar: page.toolbar,
              back: page.back,
              header: page.header,
              body: PageBodyList(list.copyWith(items: items)),
              footer: page.footer,
            )));

      case DeltaSparklineSetData(:final gauge):
        return _applyChartData(
            gauge.id,
            (current) => PageBodySparkline(SparklineSpec(
              id: gauge.id,
              series: gauge.series,
              min: gauge.min,
              max: gauge.max,
              caption: gauge.caption,
              unit: gauge.unit,
              accessibilityText: gauge.accessibilityText,
              activate: current is PageBodySparkline
                  ? current.sparkline.activate
                  : null,
            )),
            'Sparkline');

      case DeltaBarChartSetData(:final nodeId, :final bars, :final accessibilityText):
        final spec = BarChartSpec(
          id: nodeId,
          bars: bars
              .map((b) => BarChartBar.fromJson(b))
              .toList(),
          accessibilityText: accessibilityText,
        );
        return _applyBarLineChart(nodeId, spec, 'BarChart');

      case DeltaLineChartSetData(
          :final nodeId,
          :final series,
          :final xAxis,
          :final yAxis,
          :final accessibilityText
        ):
        final spec = LineChartSpec(
          id: nodeId,
          series: series.map((s) => LineChartSeries.fromJson(s)).toList(),
          xAxis: LineChartAxis.fromJson(xAxis),
          yAxis: LineChartAxis.fromJson(yAxis),
          accessibilityText: accessibilityText,
        );
        return _applyBarLineChart(nodeId, spec, 'LineChart');

      case DeltaGaugeSetData(:final gauge):
        return _applyChartData(
            gauge.id,
            (current) => PageBodyGauge(GaugeSpec(
              id: gauge.id,
              ratio: gauge.ratio,
              label: gauge.label,
              caption: gauge.caption,
              accessibilityText: gauge.accessibilityText,
              activate: current is PageBodyGauge ? current.gauge.activate : null,
            )),
            'Gauge');

      case DeltaFooterSetActions(:final nodeId, :final actions, :final status):
        if (id != nodeId) {
          throw const AppKitDeltaApplicationError(
              'Delta targets an unavailable footer root');
        }
        final footer = FooterActionsSpec(actions: actions, status: status);
        if (!footer.isValid) {
          throw const AppKitDeltaApplicationError(
              'Delta carries invalid FooterActions');
        }
        final c = component;
        switch (c) {
          case AppKitPage(:final spec):
            return AppKitNode(
                id: id,
                component: AppKitPage(PageSpec(
                  title: spec.title,
                  tabs: spec.tabs,
                  toolbar: spec.toolbar,
                  back: spec.back,
                  header: spec.header,
                  body: spec.body,
                  footer: footer,
                )));
          case AppKitTree(:final spec):
            return AppKitNode(
                id: id,
                component: AppKitTree(spec.copyWith(footer: footer)));
          case AppKitMarkdownEditor(:final spec):
            return AppKitNode(
                id: id,
                component:
                    AppKitMarkdownEditor(spec.copyWith(footer: footer)));
          default:
            throw const AppKitDeltaApplicationError(
                'Delta root has no FooterActions slot');
        }

      case DeltaInputSetValue(:final nodeId, :final value):
        final page = _page();
        final header = page.header;
        if (header is! PageHeaderInput || header.input.id != nodeId) {
          throw const AppKitDeltaApplicationError(
              'Delta targets an unavailable Input');
        }
        return AppKitNode(
            id: id,
            component: AppKitPage(PageSpec(
              title: page.title,
              tabs: page.tabs,
              toolbar: page.toolbar,
              back: page.back,
              header: PageHeaderInput(header.input.copyWith(value: value)),
              body: page.body,
              footer: page.footer,
            )));

      case DeltaListInsertItem(:final listId, :final index, :final item):
        final page = _page();
        final body = page.body;
        if (body is! PageBodyList ||
            body.list.id != listId ||
            index < 0 ||
            index > body.list.items.length) {
          throw const AppKitDeltaApplicationError(
              'Delta targets an unavailable List insertion');
        }
        final items = List<ListItemSpec>.from(body.list.items)
          ..insert(index, item);
        return _pageWithBody(page, PageBodyList(body.list.copyWith(items: items)));

      case DeltaListRemoveItem(:final listId, :final itemId):
        final page = _page();
        final body = page.body;
        if (body is! PageBodyList || body.list.id != listId) {
          throw const AppKitDeltaApplicationError(
              'Delta targets an unavailable ListItem');
        }
        final index = body.list.items.indexWhere((i) => i.id == itemId);
        if (index < 0) {
          throw const AppKitDeltaApplicationError(
              'Delta targets an unavailable ListItem');
        }
        final items = List<ListItemSpec>.from(body.list.items)
          ..removeAt(index);
        return _pageWithBody(page, PageBodyList(body.list.copyWith(items: items)));

      case DeltaListSetSelection(:final listId, :final selectedId):
        final page = _page();
        final body = page.body;
        if (body is! PageBodyList ||
            body.list.id != listId ||
            (selectedId != null &&
                !body.list.items.any((i) => i.id == selectedId))) {
          throw const AppKitDeltaApplicationError(
              'Delta targets an unavailable List selection');
        }
        return _pageWithBody(
            page,
            PageBodyList(body.list.copyWith(
              selectedId: selectedId,
              clearSelectedId: selectedId == null,
            )));

      case DeltaContentSetSelection(:final contentId, :final selection):
        final page = _page();
        final body = page.body;
        if (body is! PageBodyContent || body.content.id != contentId) {
          throw const AppKitDeltaApplicationError(
              'Delta targets unavailable Content selection');
        }
        if (selection != null) {
          final ids = body.content.lines.map((l) => l.id).toSet();
          final anchorId = selection['anchorId'] as String?;
          final headId = selection['headId'] as String?;
          if ((anchorId != null && !ids.contains(anchorId)) ||
              (headId != null && !ids.contains(headId))) {
            throw const AppKitDeltaApplicationError(
                'Delta selects an unavailable Content line');
          }
        }
        return _pageWithBody(
            page,
            PageBodyContent(body.content.copyWith(
              selection: selection,
              clearSelection: selection == null,
            )));

      case DeltaContentSpliceLines(
          :final contentId,
          :final index,
          :final deleteCount,
          :final lines
        ):
        final page = _page();
        final body = page.body;
        if (body is! PageBodyContent ||
            body.content.id != contentId ||
            index < 0 ||
            deleteCount < 0 ||
            index > body.content.lines.length ||
            deleteCount > body.content.lines.length - index) {
          throw const AppKitDeltaApplicationError(
              'Content splice is outside its collection');
        }
        final newLines = List<ContentLine>.from(body.content.lines)
          ..replaceRange(index, index + deleteCount, lines);
        return _pageWithBody(
            page, PageBodyContent(body.content.copyWith(lines: newLines)));

      case DeltaTreeSetSelection(:final nodeId, :final selectedId):
        final tree = _tree(nodeId);
        if (selectedId != null &&
            !_flattenTreeItems(tree.items).any((i) => i.id == selectedId)) {
          throw const AppKitDeltaApplicationError(
              'Delta selects an unavailable Tree item');
        }
        return AppKitNode(
            id: id,
            component: AppKitTree(tree.copyWith(
              selectedId: selectedId,
              clearSelectedId: selectedId == null,
            )));

      case DeltaTreeSetFilter(:final filterId, :final value):
        final c = component;
        if (c is! AppKitTree ||
            c.spec.filter == null ||
            c.spec.filter!.id != filterId) {
          throw const AppKitDeltaApplicationError(
              'Delta targets an unavailable Tree filter');
        }
        return AppKitNode(
            id: id,
            component: AppKitTree(
                c.spec.copyWith(filter: c.spec.filter!.copyWith(value: value))));

      case DeltaTreeSetLocation(:final nodeId, :final location):
        final tree = _tree(nodeId);
        return AppKitNode(
            id: id, component: AppKitTree(tree.copyWith(location: location)));

      case DeltaTreeSpliceChildren(
          :final nodeId,
          :final parentId,
          :final index,
          :final deleteCount,
          :final items
        ):
        final tree = _tree(nodeId);
        if (index < 0 || deleteCount < 0) {
          throw const AppKitDeltaApplicationError(
              'Tree splice has a negative range');
        }
        final newItems = tree.items.map((i) => i.copyWith()).toList();
        if (parentId != null) {
          if (!_spliceTreeChildren(newItems, parentId, index, deleteCount, items)) {
            throw const AppKitDeltaApplicationError(
                'Delta targets unavailable Tree children');
          }
        } else {
          if (index > newItems.length ||
              deleteCount > newItems.length - index) {
            throw const AppKitDeltaApplicationError(
                'Tree root splice is outside its collection');
          }
          newItems.replaceRange(index, index + deleteCount, items);
        }
        return AppKitNode(
            id: id, component: AppKitTree(tree.copyWith(items: newItems)));

      case DeltaTreeSetChildState(:final nodeId, :final itemId, :final childState):
        final tree = _tree(nodeId);
        final newItems = tree.items.map((i) => i.copyWith()).toList();
        final state = childState['state'] as String? ??
            childState['childState'] as String? ??
            'loaded';
        final updated = _updateTreeItem(newItems, itemId, (item) {
          final children = state != 'loaded' ? <TreeItemSpec>[] : item.children;
          return item.copyWith(childState: state, children: children);
        });
        if (!updated) {
          throw const AppKitDeltaApplicationError(
              'Delta targets an unavailable Tree item');
        }
        return AppKitNode(
            id: id, component: AppKitTree(tree.copyWith(items: newItems)));

      case DeltaTreeSetExpanded(:final nodeId, :final itemId, :final expanded):
        final tree = _tree(nodeId);
        final newItems = tree.items.map((i) => i.copyWith()).toList();
        final updated = _updateTreeItem(
            newItems, itemId, (item) => item.copyWith(expanded: expanded));
        if (!updated) {
          throw const AppKitDeltaApplicationError(
              'Delta targets an unavailable expandable Tree item');
        }
        return AppKitNode(
            id: id, component: AppKitTree(tree.copyWith(items: newItems)));
    }
  }

  /// Applies a sparkline/gauge data op to a page body or a list-item slot.
  AppKitNode _applyChartData(
      String chartId, PageBody Function(PageBody?) build, String kind) {
    final page = _page();
    final body = page.body;
    final bodyMatches = switch (body) {
      PageBodySparkline(:final sparkline) => sparkline.id == chartId,
      PageBodyGauge(:final gauge) => gauge.id == chartId,
      _ => false,
    };
    if (bodyMatches) {
      // Preserve the current `activate` action, mirroring Swift.
      return _pageWithBody(page, build(body));
    }
    if (body is PageBodyList) {
      final list = body.list;
      var found = false;
      final items = list.items.map((item) {
        if (found) return item;
        ListItemSlot? updateSlot(ListItemSlot? slot) {
          if (kind == 'Sparkline' &&
              slot is SlotSparkline &&
              slot.id == chartId) {
            found = true;
            final built = build(null);
            if (built is PageBodySparkline) {
              return SlotSparkline(
                  values: built.sparkline.series, id: built.sparkline.id);
            }
          }
          if (kind == 'Gauge' && slot is SlotGauge && slot.gauge.id == chartId) {
            found = true;
            final built = build(null);
            if (built is PageBodyGauge) {
              return SlotGauge(built.gauge);
            }
          }
          return slot;
        }

        final leading = updateSlot(item.leading);
        final trailing = updateSlot(item.trailing);
        final accessory = updateSlot(item.accessory);
        if (found) {
          return item.copyWith(
              leading: leading, trailing: trailing, accessory: accessory);
        }
        return item;
      }).toList();
      if (found) {
        return _pageWithBody(page, PageBodyList(list.copyWith(items: items)));
      }
    }
    throw AppKitDeltaApplicationError('Delta targets an unavailable $kind');
  }

  /// Applies a bar/line chart data op (page-body only in Swift).
  AppKitNode _applyBarLineChart(String chartId, dynamic spec, String kind) {
    final page = _page();
    final body = page.body;
    final matches = (kind == 'BarChart' &&
            body is PageBodyBarChart &&
            body.chart.id == chartId) ||
        (kind == 'LineChart' &&
            body is PageBodyLineChart &&
            body.chart.id == chartId);
    if (!matches) {
      throw AppKitDeltaApplicationError('Delta targets an unavailable $kind');
    }
    // Preserve the current `activate` action, mirroring Swift.
    if (kind == 'BarChart' && body is PageBodyBarChart) {
      final current = body.chart;
      final next = BarChartSpec(
        id: (spec as BarChartSpec).id,
        bars: spec.bars,
        accessibilityText: spec.accessibilityText,
        activate: current.activate,
      );
      return _pageWithBody(page, PageBodyBarChart(next));
    }
    if (kind == 'LineChart' && body is PageBodyLineChart) {
      final current = body.chart;
      final s = spec as LineChartSpec;
      final next = LineChartSpec(
        id: s.id,
        series: s.series,
        xAxis: s.xAxis,
        yAxis: s.yAxis,
        accessibilityText: s.accessibilityText,
        activate: current.activate,
      );
      return _pageWithBody(page, PageBodyLineChart(next));
    }
    throw const AppKitDeltaApplicationError('Delta targets an unavailable chart');
  }

  AppKitNode _pageWithBody(PageSpec page, PageBody body) => AppKitNode(
      id: id,
      component: AppKitPage(PageSpec(
        title: page.title,
        tabs: page.tabs,
        toolbar: page.toolbar,
        back: page.back,
        header: page.header,
        body: body,
        footer: page.footer,
      )));

  MarkdownEditorSpec _markdownEditor(String nodeId) {
    final c = component;
    if (id == nodeId && c is AppKitMarkdownEditor) return c.spec;
    throw const AppKitDeltaApplicationError(
        'Delta targets an unavailable Markdown node');
  }

  MediaSpec _media(String nodeId) {
    final c = component;
    if (id == nodeId && c is AppKitMedia) return c.spec;
    throw const AppKitDeltaApplicationError(
        'Delta targets an unavailable Media node');
  }

  MenuSpec _menu(String nodeId) {
    final c = component;
    if (id == nodeId && c is AppKitMenu) return c.spec;
    throw const AppKitDeltaApplicationError(
        'Delta targets an unavailable Menu node');
  }

  PageSpec _page() {
    final c = component;
    if (c is AppKitPage) return c.spec;
    throw const AppKitDeltaApplicationError(
        'Delta targets an unavailable Page');
  }

  TreeSpec _tree(String nodeId) {
    final c = component;
    if (id == nodeId && c is AppKitTree) return c.spec;
    throw const AppKitDeltaApplicationError(
        'Delta targets an unavailable Tree node');
  }
}

List<TreeItemSpec> _flattenTreeItems(List<TreeItemSpec> items) =>
    items.expand((item) => [item, ..._flattenTreeItems(item.children)]).toList();

/// Updates the first tree item with [id], recursing into children.
/// Returns true when an item was updated.
bool _updateTreeItem(List<TreeItemSpec> items, String id,
    TreeItemSpec Function(TreeItemSpec) update) {
  for (var i = 0; i < items.length; i++) {
    if (items[i].id == id) {
      items[i] = update(items[i]);
      return true;
    }
    if (_updateTreeItem(items[i].children, id, update)) return true;
  }
  return false;
}

/// Splices children of the directory item with [parentId].
/// Returns false when the parent is missing or the range is invalid.
bool _spliceTreeChildren(List<TreeItemSpec> items, String parentId, int index,
    int deleteCount, List<TreeItemSpec> replacement) {
  for (var i = 0; i < items.length; i++) {
    if (items[i].id == parentId) {
      if (items[i].kind != 'directory' ||
          index > items[i].children.length ||
          deleteCount > items[i].children.length - index) {
        return false;
      }
      final children = List<TreeItemSpec>.from(items[i].children)
        ..replaceRange(index, index + deleteCount, replacement);
      items[i] = items[i].copyWith(children: children);
      return true;
    }
    if (_spliceTreeChildren(
        items[i].children, parentId, index, deleteCount, replacement)) {
      return true;
    }
  }
  return false;
}

ListItemSlot? _setToggle(ListItemSlot? slot, String id, bool value,
    void Function(bool) markFound) {
  if (slot is SlotToggle && slot.id == id) {
    markFound(true);
    return slot.copyWith(on: value);
  }
  return slot;
}

ListItemSlot? _setCheckmark(ListItemSlot? slot, String id, bool value,
    void Function(bool) markFound) {
  if (slot is SlotCheckmark && slot.id == id) {
    markFound(true);
    return slot.copyWith(checked: value);
  }
  return slot;
}

/// Applies a UTF-16 range edit to text, mirroring Swift `replacing(_:in:)`.
///
/// The edit's range uses (line, utf16Column) positions; they are resolved to
/// UTF-16 offsets with the same validation as Swift.
String _replacing(Map<String, dynamic> edit, String text) {
  final range = edit['range'] as Map<String, dynamic>? ?? edit;
  final start = range['start'] as Map<String, dynamic>? ?? {};
  final end = range['end'] as Map<String, dynamic>? ?? {};
  final startLine = (start['line'] as num?)?.toInt() ?? 0;
  final startCol = (start['utf16Column'] as num?)?.toInt() ??
      (start['column'] as num?)?.toInt() ??
      0;
  final endLine = (end['line'] as num?)?.toInt() ?? startLine;
  final endCol = (end['utf16Column'] as num?)?.toInt() ??
      (end['column'] as num?)?.toInt() ??
      startCol;
  if (startLine > endLine || (startLine == endLine && startCol > endCol)) {
    throw const AppKitDeltaApplicationError(
        'Markdown text edit range is reversed');
  }
  final startOffset = _utf16Offset(startLine, startCol, text);
  final endOffset = _utf16Offset(endLine, endCol, text);
  final units = text.codeUnits;
  final replacement = edit['text'] as String? ?? '';
  return String.fromCharCodes([
    ...units.sublist(0, startOffset),
    ...replacement.codeUnits,
    ...units.sublist(endOffset),
  ]);
}

/// Resolves a (line, utf16Column) position to a UTF-16 offset.
///
/// Mirrors Swift `utf16Offset(for:in:)`: negative positions throw, lines past
/// the end throw, and columns past the line end (or inside a surrogate pair)
/// throw.
int _utf16Offset(int line, int utf16Column, String text) {
  if (line < 0 || utf16Column < 0) {
    throw const AppKitDeltaApplicationError(
        'Negative Markdown text position');
  }
  final units = text.codeUnits;
  var lineStart = 0;
  for (var l = 0; l < line; l++) {
    final newline = units.indexOf(10, lineStart);
    if (newline < 0) {
      throw const AppKitDeltaApplicationError(
          'Markdown text line is outside the document');
    }
    lineStart = newline + 1;
  }
  var lineEnd = units.indexOf(10, lineStart);
  if (lineEnd < 0) lineEnd = units.length;
  final target = lineStart + utf16Column;
  if (target > lineEnd) {
    throw const AppKitDeltaApplicationError(
        'Markdown UTF-16 column is outside a scalar boundary');
  }
  // Reject columns that split a surrogate pair, mirroring Swift's
  // `String.Index(target, within: text) != nil` check.
  if (target > lineStart &&
      target < units.length &&
      _isLeadSurrogate(units[target - 1]) &&
      _isTrailSurrogate(units[target])) {
    throw const AppKitDeltaApplicationError(
        'Markdown UTF-16 column is outside a scalar boundary');
  }
  return target;
}

bool _isLeadSurrogate(int unit) => unit >= 0xD800 && unit <= 0xDBFF;
bool _isTrailSurrogate(int unit) => unit >= 0xDC00 && unit <= 0xDFFF;
