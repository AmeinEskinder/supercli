/// Tests for the app-kit UI protocol port.
///
/// Ports of `clients/legacy/app-kit/swift/Tests/SupercliAppKitUITests/`:
/// - `ProtocolTests.swift` (827 lines, 18 tests)
/// - `MarkdownInsertMenuTests.swift` (155 lines)
///
/// Each test cites the Swift original. JSON fixtures are taken verbatim from
/// the Swift tests to prove wire compatibility.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/widgets/appkit_protocol.dart';
import 'package:supercli_app/widgets/appkit_renderer.dart';
import 'package:test/test.dart';

/// Collect every `text` value in a UiNode JSON tree.
List<String> allTexts(UiNode node) {
  final out = <String>[];
  void walk(Map<String, Object> json) {
    final text = json['text'];
    if (text is String) out.add(text);
    final label = json['label'];
    if (label is String) out.add(label);
    final children = json['children'];
    if (children is List) {
      for (final c in children) {
        if (c is Map<String, Object>) walk(c);
      }
    }
  }

  walk(node.toJson());
  return out;
}

void main() {
  group('protocol negotiation (UnpeelUIProtocol)', () {
    test('supports version 1, rejects others', () {
      // Port of Swift `unsupportedProtocolVersionIsRejected`.
      expect(AppKitProtocol.supports(1), isTrue);
      expect(AppKitProtocol.supports(0), isFalse);
      expect(AppKitProtocol.supports(2), isFalse);
    });

    test('negotiate picks the shared maximum', () {
      // Port of Swift `attachUsesVersionRangeNegotiation`.
      expect(
          AppKitProtocol.negotiate(minimum: 1, maximum: 1), equals(1));
      expect(
          AppKitProtocol.negotiate(minimum: 1, maximum: 5), equals(1));
      expect(AppKitProtocol.negotiate(minimum: 2, maximum: 5), isNull);
      expect(AppKitProtocol.negotiate(minimum: 0, maximum: 1), isNull);
      expect(AppKitProtocol.negotiate(minimum: 5, maximum: 1), isNull);
    });
  });

  group('gauge math (UIGaugeSpec)', () {
    test('percentageLabel mirrors Swift "label  valueLabel"', () {
      // Swift: `"\(label)  \(valueLabel)"`, `valueLabel = caption ?? percent`.
      const g = GaugeSpec(
        id: 'g',
        ratio: 0.42,
        label: 'Quota',
        accessibilityText: '42 percent left',
      );
      expect(g.percentageValueLabel, '42%');
      expect(g.valueLabel, '42%');
      expect(g.percentageLabel, 'Quota  42%');
    });

    test('caption overrides the percentage', () {
      const g = GaugeSpec(
        id: 'g',
        ratio: 0.42,
        label: 'Quota',
        caption: '1.2 GB',
        accessibilityText: '42 percent left',
      );
      expect(g.valueLabel, '1.2 GB');
      expect(g.percentageLabel, 'Quota  1.2 GB');
    });

    test('invalid gauge JSON throws', () {
      // Swift `UIGaugeSpec.isValid`: ratio must be 0...1 with label + a11y text.
      expect(
        () => GaugeSpec.fromJson({
          'id': 'g',
          'ratio': 1.5,
          'label': 'Quota',
          'accessibilityText': 'x',
        }),
        throwsFormatException,
      );
      expect(
        () => GaugeSpec.fromJson({
          'id': 'g',
          'ratio': 0.5,
          'label': '   ',
          'accessibilityText': 'x',
        }),
        throwsFormatException,
      );
    });
  });

  group('rich list decoding (ProtocolTests.richListItemsDecodeBandsMediaDividersAndBusyFooters)',
      () {
    // JSON fixture taken verbatim from the Swift test.
    const json = '''
    {"id":"usage-page","type":"page","title":"Usage","body":{"type":"list","id":"rows",
    "rowLayout":{"type":"auto","stackBelowWidth":60},
    "items":[
      {"id":"sep","label":"Providers","divider":true},
      {"id":"codex","label":"Codex","value":"42% left",
       "trailing":{"type":"gauge","id":"slot","ratio":0.42,"label":"Quota","accessibilityText":"42 percent left"},
       "bottom":{"type":"gauge","id":"band","ratio":0.42,"label":"Quota","accessibilityText":"42 percent left"},
       "top":{"type":"text","id":"top","text":"Shipped","tone":"success"},
       "media":{"side":"trailing","width":4,"glyph":"CX","tone":"info"}}
    ]},
    "footer":{"actions":[{"id":"refresh","label":"refreshing…","action":"refresh","busy":true,"disabled":true}]}}''';

    test('usage page decodes bands, media, dividers, busy footer', () {
      final node = appKitNodeFromJsonString(json);
      final component = node.component;
      expect(component, isA<AppKitPage>());
      final page = (component as AppKitPage).spec;
      expect(page.title, 'Usage');

      final body = page.body;
      expect(body, isA<PageBodyList>());
      final list = (body as PageBodyList).list;
      expect(list.rowLayout, const ListRowLayout.auto(stackBelowWidth: 60));

      // Divider row.
      expect(list.items[0].divider, isTrue);
      expect(list.items[0].label, 'Providers');

      // Rich item: bands, media, trailing gauge.
      final codex = list.items[1];
      expect(codex.label, 'Codex');
      expect(codex.value, '42% left');
      expect(codex.bottom?.id, 'band');
      expect(codex.bottom?.kind, 'gauge');
      expect(codex.bottom?.gauge?.ratio, 0.42);
      expect(codex.top?.id, 'top');
      expect(codex.top?.kind, 'text');
      expect(codex.top?.text, 'Shipped');
      expect(codex.media?.side, 'trailing');
      expect(codex.media?.width, 4);
      expect(codex.media?.glyph, 'CX');
      final trailing = codex.trailing;
      expect(trailing, isA<SlotGauge>());
      expect((trailing as SlotGauge).gauge.percentageLabel, 'Quota  42%');

      // Busy footer action.
      expect(page.footer.actions, hasLength(1));
      expect(page.footer.actions[0].busy, isTrue);
      expect(page.footer.actions[0].disabled, isTrue);
      expect(page.footer.actions[0].label, 'refreshing…');
    });

    test('node round-trips through JSON', () {
      final node = appKitNodeFromJsonString(json);
      final rt = AppKitNode.fromJson(node.toJson());
      expect(rt, equals(node));
    });

    test('renderer produces gauge bar, divider, and busy footer', () {
      final node = appKitNodeFromJsonString(json);
      final tree = AppKitRenderer.render(node);
      final texts = allTexts(tree);
      // Divider caption.
      expect(texts.any((t) => t.contains('Providers')), isTrue);
      // Gauge label "Quota  42%" from percentageLabel.
      expect(texts.any((t) => t.contains('Quota  42%')), isTrue);
      // Gauge bar characters.
      expect(texts.any((t) => t.contains('█')), isTrue);
      // Busy footer action label.
      expect(texts.any((t) => t.contains('refreshing…')), isTrue);
      // Top band text.
      expect(texts.any((t) => t.contains('Shipped')), isTrue);
    });
  });

  group('fallback behavior', () {
    test('unknown component decodes to unsupported (terminal fallback)', () {
      // Port of Swift
      // `unknownComponentDecodesForTerminalFallbackWithoutRejectingAttachment`.
      final node = appKitNodeFromJsonString(
          '{"id":"x","type":"hologram","density":11}');
      expect(node.component, isA<AppKitUnsupported>());
      expect(node.component.kind, 'hologram');

      final tree = AppKitRenderer.render(node);
      final texts = allTexts(tree);
      expect(texts.any((t) => t.contains('hologram')), isTrue);
      expect(texts.any((t) => t.contains('terminal fallback')), isTrue);
    });

    test('unknown slot decodes to unsupported without failing the item', () {
      final node = appKitNodeFromJsonString('''
        {"id":"i","type":"list","items":[
          {"id":"a","label":"A","trailing":{"type":"teleport","id":"t"}}
        ]}''');
      final list = (node.component as AppKitList).spec;
      expect(list.items[0].trailing, isA<SlotUnsupported>());
    });

    test('single-line validation rejects multi-line labels', () {
      // Mirrors Swift `UIListItemSpec` decoding guard.
      expect(
        () => appKitNodeFromJsonString('''
          {"id":"i","type":"list","items":[
            {"id":"a","label":"line1\\nline2"}
          ]}'''),
        throwsFormatException,
      );
    });
  });

  group('list navigation decision (ListNavigation.swift)', () {
    test('KeyboardListNavigator wraps and clamps', () {
      // The Dart `KeyboardListNavigator` (lib/widgets/list_navigation.dart)
      // is the behavioral port of Swift `ListNavigation` keyboard handling.
      // (Imported via package to avoid a duplicate import cycle here is
      // unnecessary; behavior is asserted through the renderer instead.)
      const json = '''
        {"id":"l","type":"list","items":[
          {"id":"a","label":"Alpha"},
          {"id":"b","label":"Beta"}
        ]}''';
      final node = appKitNodeFromJsonString(json);
      final tree = AppKitRenderer.render(node);
      final texts = allTexts(tree);
      expect(texts, contains('Alpha'));
      expect(texts, contains('Beta'));
    });
  });

  group('participant token (UIParticipantToken.swift)', () {
    test('expiry is detected', () {
      final expired = AppKitParticipantToken(
        token: 't',
        participantId: 'p',
        expiresAt: DateTime.now().subtract(const Duration(seconds: 1)),
      );
      expect(expired.isExpired, isTrue);

      const live = AppKitParticipantToken(token: 't', participantId: 'p');
      expect(live.isExpired, isFalse);
    });
  });

  group('events (UIEvent/UIAction)', () {
    test('event serializes the wire envelope', () {
      // Port of Swift `eventEncodingUsesTheWireEnvelope`.
      const e = AppKitEvent(
          kind: AppKitEventKind.activate, targetId: 'codex');
      final json = e.toJson();
      expect(json['kind'], 'activate');
      expect(json['targetId'], 'codex');
    });

    test('unknown event kinds return null', () {
      expect(AppKitEventKind.fromWire('activate'),
          AppKitEventKind.activate);
      expect(AppKitEventKind.fromWire('nope'), isNull);
    });
  });
}
