/// Tests for the wire-faithful list/page/content/tree/text-box models:
/// `appkit_protocol_lists.dart`.
///
/// Mirrors `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIProtocol.swift`.
library;

import 'package:supercli_app/widgets/appkit_protocol_lists.dart';
import 'package:supercli_app/widgets/appkit_protocol_widgets.dart';
import 'package:test/test.dart';

UIListItemSpec _item(String id,
        {UIListItemSlot? leading,
        UIListItemSlot? trailing,
        UIListItemSlot? accessory,
        String? activate,
        UIListItemActionRole actionRole =
            UIListItemActionRole.standard}) =>
    UIListItemSpec(
      id: id,
      label: 'Item $id',
      leading: leading,
      trailing: trailing,
      accessory: accessory,
      activate: activate,
      actionRole: actionRole,
    );

void main() {
  group('UIListSpec invariants', () {
    test('selectedId must identify an item', () {
      final json = {
        'id': 'list',
        'items': [
          {'id': 'a', 'label': 'A'},
        ],
        'selectedId': 'missing',
      };
      expect(() => UIListSpec.fromJson(json), throwsFormatException);
    });

    test('selectedId round trip', () {
      final list = UIListSpec(
        id: 'list',
        items: [_item('a'), _item('b')],
        selectedID: 'b',
      );
      expect(UIListSpec.fromJson(list.toJson()), list);
    });

    test('scrollPadding must fit in UInt16', () {
      final json = {
        'id': 'list',
        'items': [],
        'scrollPadding': 70000,
      };
      expect(() => UIListSpec.fromJson(json), throwsFormatException);
    });
  });

  group('UIListItemSpec roles and slot rules', () {
    test('toggle slot gives the toggle primary role', () {
      final item = _item('a',
          trailing: UIListItemSlotToggle(const UIToggleSpec(
              id: 't', label: 'T', value: false, setValue: 'set')));
      expect(item.primaryRole, UIListItemPrimaryRole.toggle);
    });

    test('checkmark in accessory gives the checkmark primary role', () {
      final item = _item('a',
          accessory: UIListItemSlotCheckmark(const UICheckmarkSpec(
              id: 'c', label: 'C', value: true, setValue: 'set')));
      expect(item.primaryRole, UIListItemPrimaryRole.checkmark);
    });

    test('checkmark is rejected outside the accessory slot', () {
      final json = _item('a',
              trailing: UIListItemSlotCheckmark(const UICheckmarkSpec(
                  id: 'c', label: 'C', value: true, setValue: 'set')))
          .toJson();
      expect(() => UIListItemSpec.fromJson(json), throwsFormatException);
    });

    test('disclosure accessory requires activate', () {
      final json =
          _item('a', accessory: const UIListItemSlotDisclosure())
              .toJson();
      expect(() => UIListItemSpec.fromJson(json), throwsFormatException);
    });

    test('activate gives the command role', () {
      expect(_item('a', activate: 'run').primaryRole,
          UIListItemPrimaryRole.command);
    });

    test('destructive action role gives the destructive role', () {
      expect(
          _item('a',
                  activate: 'run',
                  actionRole: UIListItemActionRole.destructive)
              .primaryRole,
          UIListItemPrimaryRole.destructive);
    });

    test('plain item is a static row', () {
      expect(
          _item('a').primaryRole, UIListItemPrimaryRole.staticRow);
    });

    test('ambiguous primary role is rejected', () {
      final json = _item('a',
              trailing: UIListItemSlotToggle(const UIToggleSpec(
                  id: 't', label: 'T', value: false, setValue: 'set')),
              activate: 'run')
          .toJson();
      expect(() => UIListItemSpec.fromJson(json), throwsFormatException);
    });

    test('status and badge slots round trip', () {
      final item = _item('a',
          leading: const UIListItemSlotStatus(
              UIStatusSymbolSpec(symbol: 'ok', label: 'OK')),
          trailing: const UIListItemSlotBadge(UIBadgeSpec(text: '3')));
      expect(UIListItemSpec.fromJson(item.toJson()), item);
    });

    test('sparkline band round trip', () {
      final item = UIListItemSpec(
        id: 'a',
        label: 'A',
        top: UIListItemBandSparkline(const UISparklineSpec(
            id: 'sp',
            series: [1.0, 2.0],
            accessibilityText: 'rising')),
      );
      expect(UIListItemSpec.fromJson(item.toJson()), item);
    });
  });

  group('charts', () {
    test('sparkline isValid requires finite in-bounds series', () {
      const ok = UISparklineSpec(
          id: 'sp',
          series: [1.0, 2.0, 1.5],
          accessibilityText: 'ok');
      expect(ok.isValid, isTrue);
      const bad = UISparklineSpec(
          id: 'sp', series: [], accessibilityText: 'ok');
      expect(bad.isValid, isFalse);
      const unbounded = UISparklineSpec(
          id: 'sp',
          series: [1.0, 5.0],
          min: 2.0,
          accessibilityText: 'ok');
      expect(unbounded.isValid, isFalse);
    });

    test('sparkline decode validates', () {
      expect(
          () => UISparklineSpec.fromJson({
                'id': 'sp',
                'series': [],
                'accessibilityText': 'x',
              }),
          throwsFormatException);
    });

    test('gauge requires ratio in 0...1', () {
      expect(
          () => UIGaugeSpec.fromJson({
                'id': 'g',
                'ratio': 1.5,
                'label': 'Load',
                'accessibilityText': 'high',
              }),
          throwsFormatException);
      const ok = UIGaugeSpec(
          id: 'g',
          ratio: 0.5,
          label: 'Load',
          accessibilityText: 'half');
      expect(ok.isValid, isTrue);
      expect(UIGaugeSpec.fromJson(ok.toJson()), ok);
    });

    test('bar chart normalizes bounds', () {
      final chart = UIBarChartSpec.fromJson({
        'id': 'b',
        'bars': [
          {'label': 'a', 'value': 2.0},
          {'label': 'b', 'value': 4.0},
        ],
        'accessibilityText': 'bars',
      });
      expect(chart.isValid, isTrue);
      expect(UIBarChartSpec.fromJson(chart.toJson()), chart);
    });

    test('line chart round trip', () {
      final chart = UILineChartSpec.fromJson({
        'id': 'l',
        'series': [
          {
            'name': 's1',
            'points': [
              {'x': 0.0, 'y': 1.0},
              {'x': 1.0, 'y': 2.0}
            ]
          },
        ],
        'accessibilityText': 'line',
      });
      expect(chart.isValid, isTrue);
      expect(UILineChartSpec.fromJson(chart.toJson()), chart);
    });
  });

  group('UIContentSpec', () {
    test('duplicate line ids are rejected', () {
      expect(
          () => UIContentSpec.fromJson({
                'id': 'c',
                'label': 'Log',
                'lines': [
                  {'id': 'l1'},
                  {'id': 'l1'},
                ],
              }),
          throwsFormatException);
    });

    test('selection must reference known line ids', () {
      expect(
          () => UIContentSpec.fromJson({
                'id': 'c',
                'label': 'Log',
                'lines': [
                  {'id': 'l1'},
                ],
                'selection': {'anchorId': 'l1', 'headId': 'nope'},
              }),
          throwsFormatException);
    });

    test('content round trip', () {
      const content = UIContentSpec(
        id: 'c',
        label: 'Log',
        lines: [
          UIContentLine(id: 'l1'),
          UIContentLine(id: 'l2'),
        ],
      );
      expect(UIContentSpec.fromJson(content.toJson()), content);
    });
  });

  group('PageSpec', () {
    test('body dispatch decodes a list body', () {
      final page = PageSpec(
        title: 'T',
        body: UIPageBodySlotList(UIListSpec(
          id: 'list',
          items: [_item('a')],
        )),
      );
      final decoded = PageSpec.fromJson(page.toJson());
      expect(decoded.body, isA<UIPageBodySlotList>());
      expect(decoded, page);
    });

    test('requiredCapabilities includes the page capability', () {
      final page = PageSpec(
        title: 'T',
        body: UIPageBodySlotList(UIListSpec(
          id: 'list',
          items: [_item('a')],
        )),
      );
      expect(page.requiredCapabilities,
          contains(UIProtocol.pageCapability));
    });

    test('invalid footer nulls the capabilities', () {
      final page = PageSpec(
        title: 'T',
        body: UIPageBodySlotList(UIListSpec(
          id: 'list',
          items: [_item('a')],
        )),
        footer: const UIFooterActionsSpec(actions: [
          UIFooterActionSpec(id: 'x', label: 'X', action: 'a'),
          UIFooterActionSpec(id: 'x', label: 'Y', action: 'b'),
        ]),
      );
      expect(page.requiredCapabilities, isNull);
    });
  });

  group('UITreeSpec', () {
    UITreeItem leaf(String id) =>
        UITreeItem(id: id, label: id, kind: UITreeItemKind.file);

    test('selectedId must exist', () {
      const tree = UITreeSpec(
        label: 'Files',
        location: '/tmp',
        items: [],
        selectedID: 'missing',
      );
      expect(tree.requiredCapabilities, isNull);
    });

    test('outline presentation requires setExpanded', () {
      final tree = UITreeSpec(
        label: 'Files',
        location: '/tmp',
        presentation: UITreePresentation.outline,
        items: [],
      );
      expect(tree.requiredCapabilities, isNull);
    });

    test('valid tree reports hierarchy capability', () {
      final tree = UITreeSpec(
        label: 'Files',
        location: '/tmp',
        items: [
          UITreeItem(
            id: 'dir',
            label: 'dir',
            kind: UITreeItemKind.directory,
            children: [leaf('a')],
          ),
        ],
      );
      expect(tree.requiredCapabilities,
          contains(UIProtocol.treeHierarchyCapability));
    });

    test('duplicate ids invalidate the tree', () {
      final tree = UITreeSpec(
        label: 'Files',
        location: '/tmp',
        items: [leaf('a'), leaf('a')],
      );
      expect(tree.requiredCapabilities, isNull);
    });

    test('filter round trip', () {
      final tree = UITreeSpec(
        label: 'Files',
        location: '/tmp',
        filter: const UITreeFilter(
            id: 'f', label: 'Filter', setValue: 'set-filter'),
        items: [leaf('a')],
      );
      expect(UITreeSpec.fromJson(tree.toJson()), tree);
      expect(tree.requiredCapabilities,
          contains(UIProtocol.treeFilterCapability));
    });
  });

  group('TextBoxSpec', () {
    test('minRows must be at least 1 and at most maxRows', () {
      expect(
          () => TextBoxSpec.fromJson({'minRows': 0, 'maxRows': 10}),
          throwsFormatException);
      expect(
          () => TextBoxSpec.fromJson({'minRows': 11, 'maxRows': 10}),
          throwsFormatException);
    });

    test('title positions round trip', () {
      const title = TextBoxTitle(
          text: 'T', position: TextBoxTitlePosition.topRight);
      expect(TextBoxTitle.fromJson(title.toJson()), title);
    });
  });

  group('UIComponent dispatch', () {
    test('every component type round-trips through UINode', () {
      final nodes = [
        UINode(
            id: 'n1',
            component:
                const UIComponentTextBox(TextBoxSpec(text: 'hi'))),
        UINode(
            id: 'n2',
            component: UIComponentMenu(const UIMenuSpec(
                label: 'M',
                items: [
                  UIMenuItemSpec(id: 'a', label: 'A', action: 'do-a'),
                ]))),
        UINode(
            id: 'n3',
            component: UIComponentPage(PageSpec(
              title: 'T',
              body: UIPageBodySlotList(
                  UIListSpec(id: 'l', items: [_item('a')])),
            ))),
      ];
      for (final node in nodes) {
        expect(UINode.fromJson(node.toJson()), node);
      }
    });

    test('unknown component type throws', () {
      expect(() => UINode.fromJson({'id': 'n', 'type': 'teleporter'}),
          throwsFormatException);
    });
  });
}
