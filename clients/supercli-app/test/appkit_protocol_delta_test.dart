/// Tests for the delta layer: `appkit_protocol_delta.dart` (`UIDelta.swift`
/// port) plus `UISnapshot.applying` in `appkit_protocol_messages.dart`.
///
/// Mirrors
/// `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIDelta.swift`.
library;

import 'package:supercli_app/widgets/appkit_protocol_delta.dart';
import 'package:supercli_app/widgets/appkit_protocol_lists.dart';
import 'package:supercli_app/widgets/appkit_protocol_messages.dart';
import 'package:supercli_app/widgets/appkit_protocol_widgets.dart';
import 'package:test/test.dart';

const _edit = UITextEdit(
  range: UITextRange(
      start: UITextPosition(line: 0, utf16Column: 0),
      end: UITextPosition(line: 0, utf16Column: 5)),
  text: 'hi',
);

MarkdownEditorSpec _editor() => const MarkdownEditorSpec(
      text: 'hello\nworld',
      anchorLine: 0,
      anchorColumn: 0,
      headLine: 0,
      headColumn: 0,
    );

UISnapshot _snapshot(UINode root, {int revision = 7}) => UISnapshot(
      appInstanceID: 'app',
      clientID: 'client',
      viewID: 'view',
      revision: revision,
      root: root,
    );

UIDelta _delta(int base, int revision, List<UIDeltaOperation> ops) =>
    UIDelta(
      protocolVersion: UIProtocol.version,
      appInstanceID: 'app',
      clientID: 'client',
      viewID: 'view',
      baseRevision: base,
      revision: revision,
      operations: ops,
    );

UIListItemSpec _toggleItem(String id, bool value) => UIListItemSpec(
      id: id,
      label: 'Task',
      trailing: UIListItemSlotToggle(UIToggleSpec(
          id: 'tog', label: 'Done', value: value, setValue: 'set')),
    );

List<UIDeltaOperation> _allOperations() => [
      UIDeltaOperationReplaceRoot(UINode(
          id: 'root',
          component: const UIComponentTextBox(TextBoxSpec(text: 'x')))),
      const UIDeltaOperationMarkdownReplaceRange(
          nodeID: 'editor', edit: _edit),
      const UIDeltaOperationMarkdownSetSelection(
          nodeID: 'editor',
          selection: UITextSelection(
              anchor: UITextPosition(line: 0, utf16Column: 1),
              head: UITextPosition(line: 1, utf16Column: 2))),
      const UIDeltaOperationMarkdownSetPresentation(
          nodeID: 'editor',
          presentation: MarkdownPresentation.preview),
      const UIDeltaOperationMarkdownSetReadOnly(
          nodeID: 'editor', readOnly: true),
      const UIDeltaOperationMarkdownSetTitle(
          nodeID: 'editor', title: 'Title'),
      const UIDeltaOperationMarkdownSetPlaceholder(
          nodeID: 'editor', placeholder: 'Type…'),
      const UIDeltaOperationMarkdownSetCommandHint(
          nodeID: 'editor',
          commandHint: MarkdownCommandHint(text: 'try /help')),
      const UIDeltaOperationMarkdownSetActions(
          nodeID: 'editor',
          actions: MarkdownEditorActions(save: null)),
      UIDeltaOperationMarkdownSetMenus(
          nodeID: 'editor',
          insertMenu: const UIMenuSpec(label: 'Insert', items: [
            UIMenuItemSpec(id: 'a', label: 'A', action: 'do-a'),
          ]),
          contextMenu: null),
      const UIDeltaOperationMenuSetSelection(
          nodeID: 'menu', selectedID: 'a'),
      UIDeltaOperationMediaSetSource(
          nodeID: 'media',
          source: const MediaSourcePath('/a.png'),
          intrinsic: const MediaPixelSize(w: 10, h: 20)),
      const UIDeltaOperationSurfaceSetReference(
          nodeID: 'surface',
          reference:
              SurfaceReference(sessionID: 's', streamID: 't')),
      const UIDeltaOperationToggleSetValue(nodeID: 'tog', value: true),
      const UIDeltaOperationCheckmarkSetValue(
          nodeID: 'chk', value: true),
      const UIDeltaOperationSparklineSetData(UISparklineSpec(
          id: 'sp',
          series: [1.0, 2.0],
          accessibilityText: 'up')),
      UIDeltaOperationBarChartSetData(
          UIBarChartSpec.fromJson({
            'id': 'b',
            'bars': [
              {'label': 'a', 'value': 1.0}
            ],
            'accessibilityText': 'bars',
          })),
      UIDeltaOperationLineChartSetData(
          UILineChartSpec.fromJson({
            'id': 'l',
            'series': [
              {
                'name': 's',
                'points': [
                  {'x': 0.0, 'y': 1.0}
                ]
              }
            ],
            'accessibilityText': 'line',
          })),
      const UIDeltaOperationGaugeSetData(UIGaugeSpec(
          id: 'g',
          ratio: 0.5,
          label: 'Load',
          accessibilityText: 'half')),
      const UIDeltaOperationFooterSetActions(
          nodeID: 'editor',
          actions: [
            UIFooterActionSpec(id: 'ok', label: 'OK', action: 'submit'),
          ],
          status: null),
      const UIDeltaOperationInputSetValue(
          nodeID: 'input', value: 'typed'),
      UIDeltaOperationListInsertItem(
          listID: 'list',
          index: 0,
          item: UIListItemSpec(id: 'i1', label: 'One')),
      const UIDeltaOperationListRemoveItem(
          listID: 'list', itemID: 'i1'),
      const UIDeltaOperationListSetSelection(
          listID: 'list', selectedID: 'i1'),
      const UIDeltaOperationContentSetSelection(
          contentID: 'content',
          selection: UIContentSelection(anchorID: 'l1', headID: 'l2')),
      const UIDeltaOperationContentSpliceLines(
          contentID: 'content',
          index: 0,
          deleteCount: 1,
          lines: [UIContentLine(id: 'l9')]),
      UIDeltaOperationTreeSpliceChildren(
          nodeID: 'tree',
          parentID: null,
          index: 0,
          deleteCount: 0,
          items: [
            UITreeItem(
                id: 'n1',
                label: 'n1',
                kind: UITreeItemKind.file),
          ]),
      const UIDeltaOperationTreeSetSelection(
          nodeID: 'tree', selectedID: 'n1'),
      const UIDeltaOperationTreeSetFilter(filterID: 'f', value: 'foo'),
      const UIDeltaOperationTreeSetLocation(
          nodeID: 'tree', location: '/elsewhere'),
      const UIDeltaOperationTreeSetExpanded(
          nodeID: 'tree', itemID: 'n1', expanded: true),
      const UIDeltaOperationTreeSetChildState(
          nodeID: 'tree',
          itemID: 'n1',
          childState: UITreeChildState.loaded),
    ];

void main() {
  group('UIDeltaOperation wire round trips (all 33 cases)', () {
    test('every operation encodes and decodes identically', () {
      final ops = _allOperations();
      expect(ops.length, 33);
      for (final op in ops) {
        final json = op.toJson();
        expect(json['op'], isA<String>());
        final decoded = UIDeltaOperation.fromJson(json);
        expect(decoded, op, reason: 'round trip of ${op.op}');
        expect(decoded.op, op.op);
      }
    });

    test('unknown operation throws', () {
      expect(() => UIDeltaOperation.fromJson({'op': 'frobnicate'}),
          throwsFormatException);
    });

    test('explicit null and omitted keys behave identically', () {
      // Swift uses decodeIfPresent: explicit null == absent.
      final withNull = UIDeltaOperation.fromJson({
        'op': 'menuSetSelection',
        'nodeId': 'menu',
        'selectedId': null,
      });
      final omitted = UIDeltaOperation.fromJson({
        'op': 'menuSetSelection',
        'nodeId': 'menu',
      });
      expect(withNull, omitted);
      expect(
          (withNull as UIDeltaOperationMenuSetSelection).selectedID,
          isNull);
    });
  });

  group('UIDelta validation', () {
    Map<String, dynamic> deltaJson(
            int base, int revision, List<Map<String, dynamic>> ops) =>
        {
          'protocol': UIProtocol.name,
          'protocolVersion': UIProtocol.version,
          'appInstanceId': 'app',
          'clientId': 'client',
          'viewId': 'view',
          'baseRevision': base,
          'revision': revision,
          'operations': ops,
        };

    test('empty operations are rejected', () {
      expect(
          () => UIDelta.fromJson(deltaJson(7, 8, [])),
          throwsFormatException);
    });

    test('more than 4096 operations are rejected', () {
      final ops = List.generate(
          4097,
          (_) => const UIDeltaOperationMarkdownSetReadOnly(
                  nodeID: 'e', readOnly: true)
              .toJson());
      expect(
          () => UIDelta.fromJson(deltaJson(7, 8, ops)),
          throwsFormatException);
    });

    test('revision must advance past the base revision', () {
      final ops = [
        const UIDeltaOperationMarkdownSetReadOnly(
                nodeID: 'e', readOnly: true)
            .toJson()
      ];
      expect(
          () => UIDelta.fromJson(deltaJson(7, 7, ops)),
          throwsFormatException);
      expect(
          () => UIDelta.fromJson(deltaJson(8, 7, ops)),
          throwsFormatException);
    });

    test('a valid delta round-trips', () {
      final delta = _delta(7, 8, [
        const UIDeltaOperationMarkdownSetReadOnly(
            nodeID: 'e', readOnly: true),
      ]);
      expect(UIDelta.fromJson(delta.toJson()), delta);
    });
  });

  group('UISnapshot.applying', () {
    UINode editorNode() =>
        UINode(id: 'editor', component: UIComponentMarkdownEditor(_editor()));

    test('applies a markdown text edit and bumps the revision', () {
      final snapshot = _snapshot(editorNode());
      final next = snapshot.applying(_delta(7, 8, [
        const UIDeltaOperationMarkdownReplaceRange(
            nodeID: 'editor', edit: _edit),
      ]));
      expect(next.revision, 8);
      final editor = (next.root.component as UIComponentMarkdownEditor)
          .editor;
      expect(editor.text, 'hi\nworld');
      // The input snapshot is untouched.
      expect(
          (snapshot.root.component as UIComponentMarkdownEditor)
              .editor
              .text,
          'hello\nworld');
    });

    test('edits use UTF-16 offsets', () {
      // '😀' is one code point but two UTF-16 code units.
      const spec = MarkdownEditorSpec(
        text: 'a😀b',
        anchorLine: 0,
        anchorColumn: 0,
        headLine: 0,
        headColumn: 0,
      );
      final snapshot = _snapshot(
          UINode(id: 'editor', component: UIComponentMarkdownEditor(spec)));
      final next = snapshot.applying(_delta(7, 8, [
        const UIDeltaOperationMarkdownReplaceRange(
          nodeID: 'editor',
          edit: UITextEdit(
            range: UITextRange(
                start: UITextPosition(line: 0, utf16Column: 1),
                end: UITextPosition(line: 0, utf16Column: 3)),
            text: '',
          ),
        ),
      ]));
      expect(
          (next.root.component as UIComponentMarkdownEditor).editor.text,
          'ab');
    });

    test('out-of-range edit positions throw', () {
      final snapshot = _snapshot(editorNode());
      expect(
          () => snapshot.applying(_delta(7, 8, [
                const UIDeltaOperationMarkdownReplaceRange(
                  nodeID: 'editor',
                  edit: UITextEdit(
                    range: UITextRange(
                        start: UITextPosition(line: 9, utf16Column: 0),
                        end: UITextPosition(line: 9, utf16Column: 1)),
                    text: 'x',
                  ),
                ),
              ])),
          throwsA(isA<UIDeltaApplicationError>()));
    });

    test('route mismatch is rejected', () {
      final snapshot = _snapshot(editorNode());
      final delta = UIDelta(
        protocolVersion: UIProtocol.version,
        appInstanceID: 'app',
        clientID: 'client',
        viewID: 'other-view',
        baseRevision: 7,
        revision: 8,
        operations: [
          const UIDeltaOperationMarkdownSetReadOnly(
              nodeID: 'editor', readOnly: true),
        ],
      );
      expect(() => snapshot.applying(delta),
          throwsA(isA<UIDeltaApplicationError>()));
    });

    test('non-contiguous revisions are rejected', () {
      final snapshot = _snapshot(editorNode());
      expect(
          () => snapshot.applying(_delta(6, 7, [
                const UIDeltaOperationMarkdownSetReadOnly(
                    nodeID: 'editor', readOnly: true),
              ])),
          throwsA(isA<UIDeltaApplicationError>()));
    });

    test('final selection offsets are validated', () {
      final snapshot = _snapshot(editorNode());
      expect(
          () => snapshot.applying(_delta(7, 8, [
                const UIDeltaOperationMarkdownSetSelection(
                  nodeID: 'editor',
                  anchor: UITextPosition(line: 0, utf16Column: 999),
                  head: UITextPosition(line: 0, utf16Column: 999),
                ),
              ])),
          throwsA(isA<UIDeltaApplicationError>()));
    });

    test('markdown property operations apply', () {
      final snapshot = _snapshot(editorNode());
      final next = snapshot.applying(_delta(7, 9, [
        const UIDeltaOperationMarkdownSetTitle(
            nodeID: 'editor', title: 'Doc'),
        const UIDeltaOperationMarkdownSetReadOnly(
            nodeID: 'editor', readOnly: true),
        const UIDeltaOperationMarkdownSetPresentation(
            nodeID: 'editor',
            presentation: MarkdownPresentation.split),
      ]));
      final editor =
          (next.root.component as UIComponentMarkdownEditor).editor;
      expect(editor.title, 'Doc');
      expect(editor.readOnly, isTrue);
      expect(editor.presentation, MarkdownPresentation.split);
      expect(next.revision, 9);
    });

    test('toggle mutation sets the item done flag', () {
      final page = PageSpec(
        title: 'T',
        body: UIPageBodySlotList(
            UIListSpec(id: 'list', items: [_toggleItem('i1', false)])),
      );
      final snapshot = _snapshot(
          UINode(id: 'page', component: UIComponentPage(page)));
      final next = snapshot.applying(_delta(7, 8, [
        const UIDeltaOperationToggleSetValue(
            nodeID: 'tog', value: true),
      ]));
      final list = ((next.root.component as UIComponentPage).page.body
              as UIPageBodySlotList)
          .list;
      final item = list.items.single;
      expect(item.done, isTrue);
      final slot = item.trailing as UIListItemSlotToggle;
      expect(slot.value.value, isTrue);
    });

    test('toggle targeting an unknown id throws', () {
      final page = PageSpec(
        title: 'T',
        body: UIPageBodySlotList(
            UIListSpec(id: 'list', items: [_toggleItem('i1', false)])),
      );
      final snapshot = _snapshot(
          UINode(id: 'page', component: UIComponentPage(page)));
      expect(
          () => snapshot.applying(_delta(7, 8, [
                const UIDeltaOperationToggleSetValue(
                    nodeID: 'nope', value: true),
              ])),
          throwsA(isA<UIDeltaApplicationError>()));
    });

    test('list insert and remove apply in order', () {
      final page = PageSpec(
        title: 'T',
        body: UIPageBodySlotList(UIListSpec(
            id: 'list',
            items: [UIListItemSpec(id: 'i1', label: 'One')])),
      );
      final snapshot = _snapshot(
          UINode(id: 'page', component: UIComponentPage(page)));
      final next = snapshot.applying(_delta(7, 9, [
        UIDeltaOperationListInsertItem(
            listID: 'list',
            index: 1,
            item: UIListItemSpec(id: 'i2', label: 'Two')),
        const UIDeltaOperationListRemoveItem(
            listID: 'list', itemID: 'i1'),
      ]));
      final list = ((next.root.component as UIComponentPage).page.body
              as UIPageBodySlotList)
          .list;
      expect(list.items.map((i) => i.id), ['i2']);
    });

    test('content splice replaces lines', () {
      final page = PageSpec(
        title: 'T',
        body: UIPageBodySlotContent(const UIContentSpec(
          id: 'content',
          label: 'Log',
          lines: [
            UIContentLine(id: 'l1'),
            UIContentLine(id: 'l2'),
          ],
        )),
      );
      final snapshot = _snapshot(
          UINode(id: 'page', component: UIComponentPage(page)));
      final next = snapshot.applying(_delta(7, 8, [
        const UIDeltaOperationContentSpliceLines(
          contentID: 'content',
          index: 0,
          deleteCount: 1,
          lines: [UIContentLine(id: 'l9')],
        ),
      ]));
      final content = ((next.root.component as UIComponentPage).page.body
              as UIPageBodySlotContent)
          .content;
      expect(content.lines.map((l) => l.id), ['l9', 'l2']);
    });

    test('tree splice and expansion apply', () {
      final tree = UITreeSpec(
        label: 'Files',
        location: '/tmp',
        items: [
          UITreeItem(
              id: 'dir', label: 'dir', kind: UITreeItemKind.directory),
        ],
      );
      final snapshot = _snapshot(
          UINode(id: 'tree', component: UIComponentTree(tree)));
      final next = snapshot.applying(_delta(7, 9, [
        UIDeltaOperationTreeSpliceChildren(
          nodeID: 'tree',
          parentID: 'dir',
          index: 0,
          deleteCount: 0,
          items: [
            UITreeItem(
                id: 'f1', label: 'f1', kind: UITreeItemKind.file),
          ],
        ),
        const UIDeltaOperationTreeSetExpanded(
            nodeID: 'tree', itemID: 'dir', expanded: true),
      ]));
      final result =
          (next.root.component as UIComponentTree).tree;
      expect(result.items.single.children.map((c) => c.id), ['f1']);
      expect(result.items.single.expanded, isTrue);
    });

    test('menu selection validates the target item', () {
      final snapshot = _snapshot(UINode(
          id: 'menu',
          component: UIComponentMenu(const UIMenuSpec(
              label: 'M',
              items: [
                UIMenuItemSpec(id: 'a', label: 'A', action: 'do-a'),
                UIMenuItemSpec(
                    id: 'b',
                    label: 'B',
                    action: 'do-b',
                    disabled: true),
              ]))));
      final ok = snapshot.applying(_delta(7, 8, [
        const UIDeltaOperationMenuSetSelection(
            nodeID: 'menu', selectedID: 'a'),
      ]));
      expect(
          (ok.root.component as UIComponentMenu).menu.selectedID, 'a');
      expect(
          () => snapshot.applying(_delta(7, 8, [
                const UIDeltaOperationMenuSetSelection(
                    nodeID: 'menu', selectedID: 'b'),
              ])),
          throwsA(isA<UIDeltaApplicationError>()));
    });

    test('input value applies through the page header', () {
      final page = PageSpec(
        title: 'T',
        header: const UIPageHeaderSlotInput(
            UIInputSpec(id: 'input', label: 'Name')),
        body: UIPageBodySlotList(
            UIListSpec(id: 'list', items: const [])),
      );
      final snapshot = _snapshot(
          UINode(id: 'page', component: UIComponentPage(page)));
      final next = snapshot.applying(_delta(7, 8, [
        const UIDeltaOperationInputSetValue(
            nodeID: 'input', value: 'typed'),
      ]));
      final header = (next.root.component as UIComponentPage).page.header
          as UIPageHeaderSlotInput;
      expect(header.input.value, 'typed');
    });

    test('surface reference applies to a canvas page surface', () {
      final page = CanvasPageSpec(
        title: 'C',
        surface: const UICanvasSurfaceSpec(
            id: 'surface',
            surface: SurfaceSpec(
                reference: SurfaceReference(
                    sessionID: 's', streamID: 'old'))),
      );
      final snapshot = _snapshot(
          UINode(id: 'canvas', component: UIComponentCanvasPage(page)));
      final next = snapshot.applying(_delta(7, 8, [
        const UIDeltaOperationSurfaceSetReference(
            nodeID: 'surface',
            reference:
                SurfaceReference(sessionID: 's', streamID: 'new')),
      ]));
      final canvas =
          (next.root.component as UIComponentCanvasPage).page;
      expect(canvas.surface.surface.reference.streamID, 'new');
    });
  });
}
