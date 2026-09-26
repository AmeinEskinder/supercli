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
}
