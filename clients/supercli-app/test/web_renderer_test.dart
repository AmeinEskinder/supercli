import 'package:gpuidart/gpuidart.dart' hide TableDataset;
import 'package:supercli_app/web/web.dart';
import 'package:test/test.dart';

void main() {
  const renderer = DomRenderer();

  Map<String, Object?> node(
    String kind, {
    String id = 'n1',
    Map<String, Object?> extra = const {},
  }) =>
      {'kind': kind, 'id': id, ...extra};

  group('DomRenderer', () {
    test('renders text with HTML-escaped content', () {
      final html = renderer.renderNode(
        node('text', extra: {'text': '<b>hi</b> & "bye"'}),
      );
      expect(html, contains('<span class="sc-text"'));
      expect(html, contains('&lt;b&gt;hi&lt;/b&gt; &amp; &quot;bye&quot;'));
      expect(html, isNot(contains('<b>hi</b>')));
    });

    test('renders button as a real <button>', () {
      final html =
          renderer.renderNode(node('button', extra: {'label': 'Approve'}));
      expect(html, contains('<button type="button" class="sc-button"'));
      expect(html, contains('>Approve</button>'));
    });

    test('renders input with placeholder and aria-label', () {
      final html = renderer.renderNode(
        node('input', extra: {'placeholder': 'Search sessions…'}),
      );
      expect(html, contains('<input type="text" class="sc-input"'));
      expect(html, contains('placeholder="Search sessions…"'));
      expect(html, contains('aria-label="Search sessions…"'));
    });

    test('renders column and row as groups with children', () {
      final html = renderer.renderNode(node('column', extra: {
        'children': [
          node('text', id: 'c1', extra: {'text': 'one'}),
          node('button', id: 'c2', extra: {'label': 'two'}),
        ],
      }));
      expect(html, contains('<div role="group" class="sc-column"'));
      expect(html, contains('data-node-id="n1"'));
      expect(html, contains('>one</span>'));
      expect(html, contains('>two</button>'));
    });

    test('renders table with dataset reference for host hydration', () {
      final html = renderer.renderNode(
        node('table', extra: {'dataset': 'sessions'}),
      );
      expect(html, contains('<table'));
      expect(html, contains('data-dataset="sessions"'));
      expect(html, contains('aria-label="Data table: sessions"'));
    });

    test('unknown kinds degrade to a labelled placeholder', () {
      final html = renderer.renderNode(node('fancy-widget'));
      expect(html, contains('class="sc-unknown"'));
      expect(html, contains('data-kind="fancy-widget"'));
      expect(html, contains('role="note"'));
    });

    test('style maps to inline CSS with token colors as variables', () {
      final html = renderer.renderNode(node('text', extra: {
        'text': 'styled',
        'style': {
          'foreground': '#ff0000',
          'background': 'token:background',
          'font_weight': 'bold',
          'font_size': 14,
        },
      }));
      expect(html, contains('color:#ff0000;'));
      expect(html, contains('background-color:var(--sc-background);'));
      expect(html, contains('font-weight:700;'));
      expect(html, contains('font-size:14px;'));
    });

    test('attribute values are escaped (no injection via id)', () {
      final html = renderer.renderNode(
        node('text', id: 'a"b><script>', extra: {'text': 'x'}),
      );
      expect(html, isNot(contains('<script>')));
      expect(html, contains('data-node-id="a&quot;b&gt;&lt;script&gt;"'));
    });

    test('border-radius gets px units (valid CSS)', () {
      final html = renderer.renderNode(node('button', extra: {
        'label': 'x',
        'style': {'border_radius': 6.0},
      }));
      expect(html, contains('border-radius:6.0px;'));
      expect(html, isNot(contains('border-radius:6.0;')));
    });

    test('style maps gap, sizes, border color, alignment', () {
      final html = renderer.renderNode(node('column', extra: {
        'style': {
          'gap': 8.0,
          'width': {'px': 320.0},
          'height': 'full',
          'border_color': 'token:border',
          'align': 'center',
          'justify': 'space_between',
        },
        'children': [],
      }));
      expect(html, contains('gap:8.0px;'));
      expect(html, contains('width:320.0px;'));
      expect(html, contains('height:100%;'));
      expect(html, contains('border-color:var(--sc-border);'));
      expect(html, contains('align-items:center;'));
      expect(html, contains('justify-content:space-between;'));
    });

    test('style maps fit size to auto and ignores unknown sizes', () {
      final html = renderer.renderNode(node('row', extra: {
        'style': {'width': 'fit', 'height': 'bogus'},
        'children': [],
      }));
      expect(html, contains('width:auto;'));
      expect(html, isNot(contains('height:')));
    });
  });

  group('renderWebPage', () {
    test('wraps the tree in a full HTML document', () {
      final page = renderWebPage(
        rootJson: node('column', extra: {
          'children': [node('text', extra: {'text': 'hello'})],
        }),
        title: 'Sessions',
      );
      expect(page, startsWith('<!DOCTYPE html>'));
      expect(page, contains('<html lang="en">'));
      expect(page, contains('<title>Sessions</title>'));
      expect(page, contains('<main aria-label="Sessions">'));
      expect(page, contains('>hello</span>'));
      expect(page, endsWith('</html>\n'));
    });

    test('escapes the title', () {
      final page = renderWebPage(
        rootJson: node('text', extra: {'text': 'x'}),
        title: '<Sessions>',
      );
      expect(page, contains('<title>&lt;Sessions&gt;</title>'));
    });
  });

  group('sample App tree (PageView-like)', () {
    test('renders a realistic page end to end', () {
      // Mirrors what PageView/ListNavigation/FooterActionsView produce:
      // a column with a heading, an input, a list-as-table and actions.
      final tree = node('column', id: 'page', extra: {
        'children': [
          node('text', id: 'h', extra: {'text': 'Sessions'}),
          node('input', id: 'q', extra: {'placeholder': 'Filter…'}),
          node('table', id: 'list', extra: {'dataset': 'sessions'}),
          node('row', id: 'footer', extra: {
            'children': [
              node('button', id: 'b1', extra: {'label': 'New session'}),
              node('button', id: 'b2', extra: {'label': 'Archive'}),
            ],
          }),
        ],
      });
      final html = renderer.renderNode(tree);
      expect(html, contains('>Sessions</span>'));
      expect(html, contains('placeholder="Filter…"'));
      expect(html, contains('data-dataset="sessions"'));
      expect(html, contains('>New session</button>'));
      expect(html, contains('>Archive</button>'));
    });
  });

  group('buttons carry actions for the event bridge', () {
    test('action renders as data-action', () {
      final html = renderer.renderNode(
        node('button', id: 'ok', extra: {'label': 'Allow', 'action': 'mcp.approve'}),
      );
      expect(html, contains('data-node-id="ok"'));
      expect(html, contains('data-action="mcp.approve"'));
    });

    test('icon-only buttons get an explicit aria-label', () {
      final html = renderer.renderNode(
        node('button', id: 'x', extra: {'label': '', 'aria_label': 'Close'}),
      );
      expect(html, contains('aria-label="Close"'));
      // Without an explicit label, no empty aria-label is emitted.
      final noLabel = renderer.renderNode(
        node('button', id: 'y', extra: {'label': ''}),
      );
      expect(noLabel, isNot(contains('aria-label')));
    });

    test('disabled buttons get disabled + aria-disabled', () {
      final html = renderer.renderNode(
        node('button', extra: {'label': 'Wait', 'disabled': true}),
      );
      expect(html, contains(' disabled'));
      expect(html, contains('aria-disabled="true"'));
    });

    test('input renders value and disabled', () {
      final html = renderer.renderNode(
        node('input', id: 'c', extra: {
          'placeholder': 'Message',
          'value': 'hello <world>',
          'disabled': true,
        }),
      );
      expect(html, contains('value="hello &lt;world&gt;"'));
      expect(html, contains(' disabled'));
    });

    test('input omits aria-label when there is no accessible name', () {
      // An empty aria-label would mask the accessible-name computation.
      final html = renderer.renderNode(node('input', id: 'q'));
      expect(html, isNot(contains('aria-label')));
    });

    test('input prefers an explicit label over the placeholder', () {
      final html = renderer.renderNode(node('input', id: 'q', extra: {
        'placeholder': 'Search',
        'label': 'Filter sessions',
      }));
      expect(html, contains('aria-label="Filter sessions"'));
      expect(html, contains('placeholder="Search"'));
    });
  });

  group('new node kinds', () {
    test('heading renders hN with clamped level', () {
      expect(
        renderer.renderNode(node('heading', extra: {'text': 'Hi', 'level': 1})),
        contains('<h1 class="sc-heading"'),
      );
      // Out-of-range levels fall back to h2.
      expect(
        renderer.renderNode(node('heading', extra: {'text': 'Hi', 'level': 9})),
        contains('<h2 class="sc-heading"'),
      );
      expect(
        renderer.renderNode(node('heading', extra: {'text': 'Hi'})),
        contains('<h2 class="sc-heading"'),
      );
    });

    test('list renders ul/ol with listitems', () {
      final html = renderer.renderNode(node('list', extra: {
        'children': [
          node('listitem', id: 'i1', extra: {'text': 'one'}),
          node('listitem', id: 'i2', extra: {
            'children': [node('text', id: 't', extra: {'text': 'two'})],
            'role': 'option',
            'selected': true,
          }),
        ],
      }));
      expect(html, contains('<ul class="sc-list"'));
      expect(html, contains('<li class="sc-listitem" data-node-id="i1">one</li>'));
      expect(html, contains('role="option"'));
      expect(html, contains('aria-selected="true"'));
    });

    test('aria-selected is only emitted with role=option', () {
      // A plain <li> with aria-selected is invalid ARIA, so selection is
      // suppressed without the option role.
      final html = renderer.renderNode(
        node('listitem', extra: {'text': 'x', 'selected': true}),
      );
      expect(html, isNot(contains('aria-selected')));
      expect(html, isNot(contains('role="option"')));
    });

    test('ordered list renders ol', () {
      final html = renderer.renderNode(
        node('list', extra: {'ordered': true, 'children': []}),
      );
      expect(html, contains('<ol class="sc-list"'));
    });

    test('image renders with alt text', () {
      final html = renderer.renderNode(node('image', extra: {
        'src': 'qr.png',
        'alt': 'Pairing QR code',
        'width': 128,
        'height': 128,
      }));
      expect(html, contains('<img class="sc-image"'));
      expect(html, contains('src="qr.png"'));
      expect(html, contains('alt="Pairing QR code"'));
      expect(html, contains('width="128"'));
    });

    test('link renders anchor, external gets rel', () {
      final html = renderer.renderNode(node('link', extra: {
        'label': 'Docs',
        'url': 'https://superc.li/docs',
        'external': true,
      }));
      expect(html, contains('<a class="sc-link"'));
      expect(html, contains('href="https://superc.li/docs"'));
      expect(html, contains('rel="noopener noreferrer"'));
      expect(html, contains('>Docs</a>'));
    });
  });

  group('WebEvent', () {
    test('fromJson parses click/input/action', () {
      final click = WebEvent.fromJson({'type': 'click', 'id': 'b1'});
      expect(click, isNotNull);
      expect(click!.type, 'click');
      expect(click.id, 'b1');
      expect(click.value, isNull);

      final input = WebEvent.fromJson(
          {'type': 'input', 'id': 'q', 'value': 'hello'});
      expect(input!.value, 'hello');
    });

    test('fromJson rejects malformed maps', () {
      expect(WebEvent.fromJson({'type': 'click'}), isNull);
      expect(WebEvent.fromJson({'id': 'x'}), isNull);
      expect(WebEvent.fromJson({}), isNull);
      expect(WebEvent.fromJson({'type': '', 'id': 'x'}), isNull);
    });

    test('toJson round-trips', () {
      const e = WebEvent(type: 'action', id: 'b1', value: 'mcp.approve');
      final back = WebEvent.fromJson(e.toJson());
      expect(back!.type, 'action');
      expect(back.id, 'b1');
      expect(back.value, 'mcp.approve');
    });

    test('WebEventHandler dispatches to callbacks', () async {
      String? clicked;
      String? actioned;
      String? inputted;
      int? selected;
      final h = WebEventHandler(
        onClick: (id, action) async => clicked = '$id:$action',
        onAction: (id, action) async => actioned = '$id:$action',
        onInput: (id, value) async => inputted = '$id:$value',
        onTableSelection: (id, row) async => selected = row,
      );
      expect(await h.handle(const WebEvent(type: 'click', id: 'b1', value: 'mcp.approve')), isTrue);
      expect(clicked, 'b1:mcp.approve');
      expect(await h.handle(const WebEvent(type: 'action', id: 'b2', value: 'sidebar.toggle')), isTrue);
      expect(actioned, 'b2:sidebar.toggle');
      expect(await h.handle(const WebEvent(type: 'input', id: 'q', value: 'hi')), isTrue);
      expect(inputted, 'q:hi');
      expect(await h.handle(const WebEvent(type: 'table_selection', id: 't', value: '3')), isTrue);
      expect(selected, 3);
      expect(await h.handle(const WebEvent(type: 'bogus', id: 'x')), isFalse);
      expect(await h.handle(const WebEvent(type: 'table_selection', id: 't', value: 'NaN')), isFalse);
    });
  });

  group('webEventBridgeJs', () {
    test('posts to the given endpoint, same-origin only', () {
      final js = webEventBridgeJs(endpoint: '/web/event');
      expect(js, contains('"/web/event"'));
      // No absolute URLs: the bridge must never contact a third party.
      expect(js, isNot(contains('https://')));
      expect(js, isNot(contains('http://')));
      // Covers the three event types.
      expect(js, contains('"click"'));
      expect(js, contains('"input"'));
      expect(js, contains('"table_selection"'));
      // Input debounce.
      expect(js, contains('setTimeout'));
    });

    test('custom endpoint and debounce are honored', () {
      final js = webEventBridgeJs(endpoint: '/custom', inputDebounceMs: 500);
      expect(js, contains('"/custom"'));
      expect(js, contains('500'));
    });

    test('interactive elements inside rows keep their own clicks', () {
      final js = webEventBridgeJs();
      // The bridge must distinguish bare row clicks (table_selection)
      // from clicks on buttons/links/inputs nested in a row.
      expect(js, contains('button,a,input,select,textarea'));
      expect(js, contains('row.contains'));
    });

    test('endpoint is JS-string-escaped', () {
      final js = webEventBridgeJs(endpoint: '/a"b');
      expect(js, contains(r'"/a\"b"'));
      expect(js, isNot(contains('"/a"b"')));
    });
  });

  group('web hydration', () {
    test('TableDataset.fromJson parses host shape', () {
      final ds = TableDataset.fromJson({
        'columns': ['Title', 'Updated'],
        'rows': [
          ['a', '1m'],
          ['b', '2m'],
        ],
      });
      expect(ds, isNotNull);
      expect(ds!.columns, ['Title', 'Updated']);
      expect(ds.rows.length, 2);
      expect(TableDataset.fromJson({'nope': 1}), isNull);
    });

    test('hydrateTable renders header + rows with escaping', () {
      const ds = TableDataset(
        columns: ['Title'],
        rows: [
          ['<b>hi</b>'],
          ['plain'],
        ],
      );
      final html = hydrateTable(ds);
      expect(html, startsWith('<tbody>'));
      expect(html, contains('<th scope="col">Title</th>'));
      expect(html, contains('<tr data-row="0">'));
      expect(html, contains('<td>&lt;b&gt;hi&lt;/b&gt;</td>'));
      expect(html, contains('<tr data-row="1">'));
      expect(html, endsWith('</tbody>'));
    });

    test('hydrateTable marks selected row and handles empty', () {
      const ds = TableDataset(columns: ['A'], rows: [['x']]);
      expect(hydrateTable(ds, selectedRow: 0),
          contains('aria-selected="true"'));
      const empty = TableDataset(columns: ['A'], rows: []);
      expect(hydrateTable(empty), contains('No rows'));
    });

    test('hydrateTables fills placeholders, leaves unknown datasets', () {
      final page = renderer.renderNode(node('column', extra: {
        'children': [
          node('table', id: 't1', extra: {'dataset': 'sessions'}),
          node('table', id: 't2', extra: {'dataset': 'missing'}),
        ],
      }));
      final out = hydrateTables(page, {
        'sessions': const TableDataset(
          columns: ['Title'],
          rows: [['live row']],
        ),
      });
      // Hydrated table: real rows, node id preserved for the event bridge.
      expect(out, contains('data-node-id="t1"'));
      expect(out, contains('<td>live row</td>'));
      // Exactly one Loading placeholder remains (t2's); t1 was hydrated.
      expect('Loading…'.allMatches(out).length, 1);
      // Unknown dataset keeps its placeholder.
      expect(out, contains('data-node-id="t2"'));
      expect(out, contains('Loading…'));
    });
  });

  group('renderWebPage with event bridge', () {
    test('embeds the bridge script when an endpoint is given', () {
      final page = renderWebPage(
        rootJson: node('text', extra: {'text': 'x'}),
        title: 'T',
        eventEndpoint: '/web/event',
      );
      expect(page, contains('<script>'));
      expect(page, contains('"/web/event"'));
    });

    test('omits the script without an endpoint', () {
      final page = renderWebPage(
        rootJson: node('text', extra: {'text': 'x'}),
        title: 'T',
      );
      expect(page, isNot(contains('<script>')));
    });

    test('stylesheet covers all rendered node classes', () {
      final page = renderWebPage(
        rootJson: node('text', extra: {'text': 'x'}),
        title: 'T',
      );
      for (final cls in [
        '.sc-column', '.sc-row', '.sc-text', '.sc-heading', '.sc-button',
        '.sc-input', '.sc-table', '.sc-list', '.sc-listitem', '.sc-link',
        '.sc-image', '.sc-unknown',
      ]) {
        expect(page, contains(cls), reason: 'missing style for $cls');
      }
    });
  });

  group('real gpuidart node JSON', () {
    // Feeds actual UiNode.toJson() output through the renderer, so the
    // wire format can never drift from what the renderer expects.
    test('renders every gpuidart node kind', () {
      final tree = UiColumn('root', [
        const UiText('t', 'hello'),
        const UiButton('b', 'Click'),
        const UiInput('i', placeholder: 'Type…'),
        const UiTable('tab', dataset: 'sessions'),
        UiRow('r', [const UiText('t2', 'nested')]),
      ]);
      final html = renderer.renderNode(
          tree.toJson() as Map<String, Object?>);
      expect(html, contains('>hello</span>'));
      expect(html, contains('>Click</button>'));
      expect(html, contains('placeholder="Type…"'));
      expect(html, contains('aria-label="Type…"'));
      expect(html, contains('data-dataset="sessions"'));
      expect(html, contains('>nested</span>'));
    });

    test('renders a styled gpuidart node', () {
      final node = UiText(
        's',
        'styled',
        style: const UiStyle(
          foreground: UiColor.token(ThemeToken.danger),
          fontSize: 14,
          fontWeight: UiFontWeight.bold,
          borderRadius: 6,
        ),
      );
      final html = renderer.renderNode(
          node.toJson() as Map<String, Object?>);
      expect(html, contains('color:var(--sc-danger);'));
      expect(html, contains('font-size:14.0px;'));
      expect(html, contains('font-weight:700;'));
      expect(html, contains('border-radius:6.0px;'));
    });
  });
}
