import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/web/web.dart';
import 'package:test/test.dart';

void main() {
  group('HelpLinks', () {
    test('canonical URLs use the superc.li contract', () {
      expect(HelpLinks.docs.url, 'https://superc.li/docs');
      expect(HelpLinks.downloadMac.url, 'https://superc.li/download/mac');
      expect(HelpLinks.downloadIos.url, 'https://superc.li/download/ios');
      expect(HelpLinks.installCli.url, 'https://superc.li/install.sh');
    });

    test('slugs are unique and URL-safe', () {
      final slugs = HelpLinks.all.map((l) => l.slug).toList();
      expect(slugs.toSet().length, slugs.length);
      for (final slug in slugs) {
        expect(slug, matches(RegExp(r'^[a-z0-9-]+$')));
      }
    });

    test('urlForSlug resolves every link', () {
      for (final link in HelpLinks.all) {
        expect(HelpLinks.urlForSlug(link.slug), link.url);
      }
      expect(HelpLinks.urlForSlug('nope'), isNull);
    });

    test('urlForButtonId maps view buttons back to URLs', () {
      expect(
        HelpLinks.urlForButtonId('help-link-docs'),
        'https://superc.li/docs',
      );
      expect(
        HelpLinks.urlForButtonId('help-link-download-mac'),
        'https://superc.li/download/mac',
      );
      expect(HelpLinks.urlForButtonId('other-button'), isNull);
      expect(HelpLinks.urlForButtonId('help-link-bogus'), isNull);
    });
  });

  group('HelpLinksView', () {
    test('builds a UiNode tree with one button per link', () {
      final tree = const HelpLinksView().build();
      expect(tree, isA<UiColumn>());
      final column = tree as UiColumn;
      // title + one row per link
      expect(column.children.length, 1 + HelpLinks.all.length);
      expect((column.children.first as UiText).text, 'Help & downloads');

      final buttonIds = <String>[];
      void collect(UiNode n) {
        if (n is UiButton) buttonIds.add(n.id);
        if (n is UiColumn) {
          for (final c in n.children) {
            collect(c);
          }
        }
        if (n is UiRow) {
          for (final c in n.children) {
            collect(c);
          }
        }
      }

      collect(tree);
      expect(
        buttonIds,
        HelpLinks.all.map((l) => 'help-link-${l.slug}').toList(),
      );
      // Every button id resolves to its URL: the host can open links
      // without screens hard-coding URLs.
      for (final id in buttonIds) {
        expect(HelpLinks.urlForButtonId(id), isNotNull);
      }
    });

    test('tree serializes to JSON the web renderer can render', () {
      final tree = const HelpLinksView().build();
      final json = tree.toJson();
      final html = const DomRenderer().renderNode(json);
      expect(html, contains('Help &amp; downloads'));
      expect(html, contains('>Documentation</button>'));
      expect(html, contains('>Download for macOS</button>'));
    });
  });
}
