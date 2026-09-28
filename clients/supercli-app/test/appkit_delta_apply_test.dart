/// Tests for [appkit_delta_apply.dart]: the UIDelta incremental application.
///
/// Swift source: `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIDelta.swift`
/// (`UISnapshot.applying(_:)`, `UINode.applying(_:)`, helpers).
library;

import 'package:test/test.dart';

import 'package:supercli_app/widgets/appkit_delta.dart';
import 'package:supercli_app/widgets/appkit_delta_apply.dart';
import 'package:supercli_app/widgets/appkit_protocol.dart';

AppKitSnapshot _snapshot({
  String protocolName = 'unpeel-ui-v1',
  int protocolVersion = 1,
  String appInstanceId = 'app',
  String clientId = 'client',
  String viewId = 'view',
  int revision = 5,
  required AppKitNode root,
}) =>
    AppKitSnapshot(
      protocolName: protocolName,
      protocolVersion: protocolVersion,
      appInstanceId: appInstanceId,
      clientId: clientId,
      viewId: viewId,
      revision: revision,
      root: root,
    );

AppKitDelta _delta({
  String protocolName = 'unpeel-ui-v1',
  int protocolVersion = 1,
  String appInstanceId = 'app',
  String clientId = 'client',
  String viewId = 'view',
  int baseRevision = 5,
  int revision = 6,
  required List<AppKitDeltaOperation> operations,
}) =>
    AppKitDelta(
      protocolName: protocolName,
      protocolVersion: protocolVersion,
      appInstanceId: appInstanceId,
      clientId: clientId,
      viewId: viewId,
      baseRevision: baseRevision,
      revision: revision,
      operations: operations,
    );

AppKitNode _pageNode({
  String id = 'root',
  PageBody body = const PageBodyList(ListSpec(id: 'list', items: [])),
  PageHeader header = const PageHeaderUnsupported('none'),
  FooterActionsSpec footer = const FooterActionsSpec(),
}) =>
    AppKitNode(
      id: id,
      component: AppKitPage(PageSpec(
        title: 't',
        body: body,
        header: header,
        footer: footer,
      )),
    );

void main() {
  group('AppKitSnapshot.applying envelope guards', () {
    final root = _pageNode();

    test('applies a contiguous route-matched delta', () {
      final snapshot = _snapshot(root: root);
      final delta = _delta(operations: [
        DeltaReplaceRoot(_pageNode(id: 'new-root')),
      ]);
      final next = snapshot.applying(delta);
      expect(next.revision, 6);
      expect(next.root.id, 'new-root');
      // Original is unchanged (immutability).
      expect(snapshot.revision, 5);
      expect(snapshot.root.id, 'root');
    });

    test('rejects route mismatch', () {
      final snapshot = _snapshot(root: root);
      final delta = _delta(viewId: 'other', operations: [
        DeltaReplaceRoot(root),
      ]);
      expect(
          () => snapshot.applying(delta),
          throwsA(isA<AppKitDeltaApplicationError>().having(
              (e) => e.message, 'message', contains('route'))));
    });

    test('rejects non-contiguous delta', () {
      final snapshot = _snapshot(root: root, revision: 5);
      final delta = _delta(baseRevision: 4, revision: 6, operations: [
        DeltaReplaceRoot(root),
      ]);
      expect(
          () => snapshot.applying(delta),
          throwsA(isA<AppKitDeltaApplicationError>().having(
              (e) => e.message, 'message', contains('contiguous'))));
    });

    test('rejects non-advancing revision', () {
      final snapshot = _snapshot(root: root, revision: 5);
      final delta = _delta(baseRevision: 5, revision: 5, operations: [
        DeltaReplaceRoot(root),
      ]);
      expect(() => snapshot.applying(delta),
          throwsA(isA<AppKitDeltaApplicationError>()));
    });

    test('applies operations in order', () {
      final snapshot = _snapshot(
          root: _pageNode(
              body: const PageBodyList(ListSpec(id: 'l', items: []))));
      final delta = _delta(operations: [
        DeltaListInsertItem(
            listId: 'l',
            index: 0,
            item: const ListItemSpec(id: 'a', label: 'A')),
        DeltaListInsertItem(
            listId: 'l',
            index: 1,
            item: const ListItemSpec(id: 'b', label: 'B')),
      ]);
      final next = snapshot.applying(delta);
      final body = (next.root.component as AppKitPage).spec.body;
      expect(body, isA<PageBodyList>());
      final items = (body as PageBodyList).list.items;
      expect(items.map((i) => i.id), ['a', 'b']);
    });
  });

  group('list ops', () {
    AppKitSnapshot listSnapshot() => _snapshot(
        root: _pageNode(
            body: PageBodyList(ListSpec(id: 'l', items: [
          const ListItemSpec(id: 'a', label: 'A'),
          const ListItemSpec(id: 'b', label: 'B'),
        ]))));

    test('listRemoveItem removes by id', () {
      final next = listSnapshot().applying(_delta(operations: [
        const DeltaListRemoveItem(listId: 'l', itemId: 'a'),
      ]));
      final items =
          ((next.root.component as AppKitPage).spec.body as PageBodyList)
              .list
              .items;
      expect(items.map((i) => i.id), ['b']);
    });

    test('listRemoveItem throws for unknown id', () {
      expect(
          () => listSnapshot().applying(_delta(operations: [
                const DeltaListRemoveItem(listId: 'l', itemId: 'zzz'),
              ])),
          throwsA(isA<AppKitDeltaApplicationError>()));
    });

    test('listSetSelection sets and clears', () {
      final s1 = listSnapshot().applying(_delta(operations: [
        const DeltaListSetSelection(listId: 'l', selectedId: 'b'),
      ]));
      expect(
          ((s1.root.component as AppKitPage).spec.body as PageBodyList)
              .list
              .selectedId,
          'b');
      final s2 = s1.applying(_delta(
          baseRevision: 6,
          revision: 7,
          operations: [const DeltaListSetSelection(listId: 'l', selectedId: null)]));
      expect(
          ((s2.root.component as AppKitPage).spec.body as PageBodyList)
              .list
              .selectedId,
          isNull);
    });

    test('listSetSelection rejects unknown id', () {
      expect(
          () => listSnapshot().applying(_delta(operations: [
                const DeltaListSetSelection(listId: 'l', selectedId: 'zzz'),
              ])),
          throwsA(isA<AppKitDeltaApplicationError>()));
    });

    test('listInsertItem rejects out-of-range index', () {
      expect(
          () => listSnapshot().applying(_delta(operations: [
                DeltaListInsertItem(
                    listId: 'l',
                    index: 99,
                    item: const ListItemSpec(id: 'c', label: 'C')),
              ])),
          throwsA(isA<AppKitDeltaApplicationError>()));
    });
  });

  group('toggle / checkmark ops', () {
    AppKitSnapshot toggleSnapshot() => _snapshot(
        root: _pageNode(
            body: PageBodyList(ListSpec(id: 'l', items: [
          ListItemSpec(
              id: 'row',
              label: 'Row',
              leading: const SlotToggle(id: 't1', on: false)),
        ]))));

    test('toggleSetValue flips the toggle and marks done', () {
      final next = toggleSnapshot().applying(_delta(operations: [
        const DeltaToggleSetValue(nodeId: 't1', value: true),
      ]));
      final item = ((next.root.component as AppKitPage).spec.body
              as PageBodyList)
          .list
          .items
          .first;
      expect((item.leading as SlotToggle).on, isTrue);
      expect(item.done, isTrue);
    });

    test('toggleSetValue throws for unknown id', () {
      expect(
          () => toggleSnapshot().applying(_delta(operations: [
                const DeltaToggleSetValue(nodeId: 'nope', value: true),
              ])),
          throwsA(isA<AppKitDeltaApplicationError>()));
    });
  });

  group('content ops', () {
    AppKitSnapshot contentSnapshot() => _snapshot(
        root: _pageNode(
            body: PageBodyContent(ContentSpec(id: 'c', label: 'C', lines: [
          const ContentLine(id: 'l1', text: 'one'),
          const ContentLine(id: 'l2', text: 'two'),
        ]))));

    test('contentSpliceLines replaces a range', () {
      final next = contentSnapshot().applying(_delta(operations: [
        DeltaContentSpliceLines(
            contentId: 'c',
            index: 1,
            deleteCount: 1,
            lines: [const ContentLine(id: 'l3', text: 'three')]),
      ]));
      final lines = ((next.root.component as AppKitPage).spec.body
              as PageBodyContent)
          .content
          .lines;
      expect(lines.map((l) => l.id), ['l1', 'l3']);
    });

    test('contentSpliceLines rejects out-of-range', () {
      expect(
          () => contentSnapshot().applying(_delta(operations: [
                const DeltaContentSpliceLines(
                    contentId: 'c', index: 5, deleteCount: 0, lines: []),
              ])),
          throwsA(isA<AppKitDeltaApplicationError>()));
    });
  });

  group('tree ops', () {
    AppKitSnapshot treeSnapshot() => _snapshot(
        root: AppKitNode(
            id: 't',
            component: AppKitTree(TreeSpec(id: 't', items: [
              TreeItemSpec(id: 'a', label: 'A', kind: 'directory', children: [
                const TreeItemSpec(id: 'a1', label: 'A1'),
              ]),
              const TreeItemSpec(id: 'b', label: 'B'),
            ]))));

    test('treeSetSelection sets and validates', () {
      final next = treeSnapshot().applying(_delta(operations: [
        const DeltaTreeSetSelection(nodeId: 't', selectedId: 'a1'),
      ]));
      expect((next.root.component as AppKitTree).spec.selectedId, 'a1');
    });

    test('treeSetSelection rejects unknown id', () {
      expect(
          () => treeSnapshot().applying(_delta(operations: [
                const DeltaTreeSetSelection(nodeId: 't', selectedId: 'zzz'),
              ])),
          throwsA(isA<AppKitDeltaApplicationError>()));
    });

    test('treeSetExpanded toggles expansion', () {
      final next = treeSnapshot().applying(_delta(operations: [
        const DeltaTreeSetExpanded(nodeId: 't', itemId: 'a', expanded: true),
      ]));
      final item = (next.root.component as AppKitTree)
          .spec
          .items
          .firstWhere((i) => i.id == 'a');
      expect(item.expanded, isTrue);
    });

    test('treeSpliceChildren inserts under a directory', () {
      final next = treeSnapshot().applying(_delta(operations: [
        DeltaTreeSpliceChildren(
            nodeId: 't',
            parentId: 'a',
            index: 1,
            deleteCount: 0,
            items: [const TreeItemSpec(id: 'a2', label: 'A2')]),
      ]));
      final a = (next.root.component as AppKitTree)
          .spec
          .items
          .firstWhere((i) => i.id == 'a');
      expect(a.children.map((c) => c.id), ['a1', 'a2']);
    });

    test('treeSetChildState clears children when not loaded', () {
      final next = treeSnapshot().applying(_delta(operations: [
        DeltaTreeSetChildState(
            nodeId: 't',
            itemId: 'a',
            childState: const {'state': 'loading'}),
      ]));
      final a = (next.root.component as AppKitTree)
          .spec
          .items
          .firstWhere((i) => i.id == 'a');
      expect(a.childState, 'loading');
      expect(a.children, isEmpty);
    });
  });

  group('menu op', () {
    test('menuSetSelection validates against items', () {
      final snapshot = _snapshot(
          root: AppKitNode(
              id: 'm',
              component: AppKitMenu(MenuSpec(id: 'm', items: [
                const MenuItemSpec(id: 'i1', label: 'One'),
                const MenuItemSpec(
                    id: 'i2', label: 'Two', disabled: true),
              ]))));
      final next = snapshot.applying(_delta(operations: [
        const DeltaMenuSetSelection(nodeId: 'm', selectedId: 'i1'),
      ]));
      expect((next.root.component as AppKitMenu).spec.selectedId, 'i1');

      // Disabled item is rejected.
      expect(
          () => snapshot.applying(_delta(operations: [
                const DeltaMenuSetSelection(nodeId: 'm', selectedId: 'i2'),
              ])),
          throwsA(isA<AppKitDeltaApplicationError>()));

      // Unknown item is rejected.
      expect(
          () => snapshot.applying(_delta(operations: [
                const DeltaMenuSetSelection(nodeId: 'm', selectedId: 'zzz'),
              ])),
          throwsA(isA<AppKitDeltaApplicationError>()));
    });
  });

  group('markdown ops', () {
    AppKitSnapshot markdownSnapshot() => _snapshot(
        root: const AppKitNode(
            id: 'm',
            component: AppKitMarkdownEditor(
                MarkdownEditorSpec(text: 'hello\nworld', placeholder: 'ph'))));

    test('markdownSetDirty / markdownSetReadOnly', () {
      final next = markdownSnapshot().applying(_delta(operations: [
        const DeltaMarkdownSetDirty(nodeId: 'm', dirty: true),
        const DeltaMarkdownSetReadOnly(nodeId: 'm', readOnly: true),
      ]));
      final spec = (next.root.component as AppKitMarkdownEditor).spec;
      expect(spec.dirty, isTrue);
      expect(spec.readOnly, isTrue);
    });

    test('markdownReplaceRange edits text', () {
      final next = markdownSnapshot().applying(_delta(operations: [
        DeltaMarkdownReplaceRange(nodeId: 'm', edit: {
          'range': {
            'start': {'line': 0, 'utf16Column': 5},
            'end': {'line': 0, 'utf16Column': 5},
          },
          'text': '!',
        }),
      ]));
      final spec = (next.root.component as AppKitMarkdownEditor).spec;
      expect(spec.text, 'hello!\nworld');
    });

    test('markdownReplaceRange rejects reversed range', () {
      expect(
          () => markdownSnapshot().applying(_delta(operations: [
                DeltaMarkdownReplaceRange(nodeId: 'm', edit: {
                  'range': {
                    'start': {'line': 1, 'utf16Column': 0},
                    'end': {'line': 0, 'utf16Column': 0},
                  },
                  'text': 'x',
                }),
              ])),
          throwsA(isA<AppKitDeltaApplicationError>()));
    });

    test('markdownSetTitle sets and clears', () {
      final s1 = markdownSnapshot().applying(_delta(operations: [
        const DeltaMarkdownSetTitle(nodeId: 'm', title: 'Hello'),
      ]));
      expect((s1.root.component as AppKitMarkdownEditor).spec.title, 'Hello');
      final s2 = s1.applying(_delta(
          baseRevision: 6,
          revision: 7,
          operations: [const DeltaMarkdownSetTitle(nodeId: 'm', title: null)]));
      expect((s2.root.component as AppKitMarkdownEditor).spec.title, isNull);
    });

    test('markdown op on wrong node throws', () {
      expect(
          () => markdownSnapshot().applying(_delta(operations: [
                const DeltaMarkdownSetDirty(nodeId: 'other', dirty: true),
              ])),
          throwsA(isA<AppKitDeltaApplicationError>()));
    });
  });

  group('chart ops', () {
    test('gaugeSetData updates a page-body gauge, preserving activate', () {
      final snapshot = _snapshot(
          root: _pageNode(
              body: PageBodyGauge(GaugeSpec(
                  id: 'g',
                  ratio: 0.5,
                  label: 'L',
                  accessibilityText: 'a',
                  activate: 'act'))));
      final next = snapshot.applying(_delta(operations: [
        DeltaGaugeSetData(GaugeSpec(
            id: 'g',
            ratio: 0.75,
            label: 'L',
            accessibilityText: 'a')),
      ]));
      final gauge = ((next.root.component as AppKitPage).spec.body
              as PageBodyGauge)
          .gauge;
      expect(gauge.ratio, 0.75);
      expect(gauge.activate, 'act');
    });

    test('gaugeSetData throws for unknown id', () {
      final snapshot = _snapshot(
          root: _pageNode(
              body: PageBodyGauge(GaugeSpec(
                  id: 'g',
                  ratio: 0.5,
                  label: 'L',
                  accessibilityText: 'a'))));
      expect(
          () => snapshot.applying(_delta(operations: [
                DeltaGaugeSetData(GaugeSpec(
                    id: 'other',
                    ratio: 0.5,
                    label: 'L',
                    accessibilityText: 'a')),
              ])),
          throwsA(isA<AppKitDeltaApplicationError>()));
    });
  });

  group('footer / input ops', () {
    test('footerSetActions replaces footer on a page', () {
      final snapshot = _snapshot(root: _pageNode());
      final next = snapshot.applying(_delta(operations: [
        DeltaFooterSetActions(nodeId: 'root', actions: [
          const FooterActionSpec(id: 'save', label: 'Save', action: 'save'),
        ], status: 'busy'),
      ]));
      final footer = (next.root.component as AppKitPage).spec.footer;
      expect(footer.actions.map((a) => a.id), ['save']);
      expect(footer.status, 'busy');
    });

    test('footerSetActions rejects wrong node id', () {
      final snapshot = _snapshot(root: _pageNode(id: 'root'));
      expect(
          () => snapshot.applying(_delta(operations: [
                const DeltaFooterSetActions(nodeId: 'other', actions: []),
              ])),
          throwsA(isA<AppKitDeltaApplicationError>()));
    });

    test('inputSetValue updates the page header input', () {
      final snapshot = _snapshot(
          root: _pageNode(
              header: const PageHeaderInput(
                  UIInputSpec(id: 'q', label: 'Search'))));
      final next = snapshot.applying(_delta(operations: [
        const DeltaInputSetValue(nodeId: 'q', value: 'hello'),
      ]));
      final header =
          (next.root.component as AppKitPage).spec.header as PageHeaderInput;
      expect(header.input.value, 'hello');
    });
  });
}
