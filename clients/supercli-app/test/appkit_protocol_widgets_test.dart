/// Tests for the wire-faithful widget primitives:
/// `appkit_protocol_widgets.dart` (media, surface, markdown, footer, canvas,
/// toggle/checkmark, status symbol, badge, `ratioCeil`, identifiers).
///
/// Mirrors `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIProtocol.swift`.
library;

import 'package:supercli_app/widgets/appkit_protocol_lists.dart';
import 'package:supercli_app/widgets/appkit_protocol_widgets.dart';
import 'package:test/test.dart';

void main() {
  group('MediaSource', () {
    test('path source round trip', () {
      const source = MediaSourcePath('/tmp/a.png');
      expect(MediaSource.fromJson(source.toJson()), source);
    });

    test('path rejects empty and NUL-containing paths', () {
      expect(
          () => MediaSource.fromJson({'kind': 'path', 'path': ''}),
          throwsFormatException);
      expect(
          () => MediaSource.fromJson(
              {'kind': 'path', 'path': 'a\x00b'}),
          throwsFormatException);
    });

    test('inline source round trip', () {
      // 'aGVsbG8=' is the canonical base64 of 'hello'.
      const source =
          MediaSourceInline(mediaType: 'image/png', base64: 'aGVsbG8=');
      expect(MediaSource.fromJson(source.toJson()), source);
    });

    test('inline rejects non-image MIME types', () {
      expect(
          () => MediaSource.fromJson({
                'kind': 'inline',
                'mediaType': 'text/plain',
                'base64': 'aGVsbG8=',
              }),
          throwsFormatException);
    });

    test('inline rejects invalid base64', () {
      expect(
          () => MediaSource.fromJson({
                'kind': 'inline',
                'mediaType': 'image/png',
                'base64': '!!!not-base64!!!',
              }),
          throwsFormatException);
    });

    test('inline rejects non-canonical base64', () {
      // 'aGVsbG8' is missing padding: decodes but does not re-encode.
      expect(
          () => MediaSource.fromJson({
                'kind': 'inline',
                'mediaType': 'image/png',
                'base64': 'aGVsbG8',
              }),
          throwsFormatException);
    });

    test('blob source round trip', () {
      final source = MediaSourceBlob(MediaBlobReference(
        sha256: List.filled(64, 'a').join(),
        mediaType: 'image/jpeg',
        byteLength: 1234,
      ));
      expect(MediaSource.fromJson(source.toJson()), source);
    });

    test('blob rejects a malformed sha256', () {
      expect(
          () => MediaSource.fromJson({
                'kind': 'blob',
                'sha256': 'not-hex',
                'mediaType': 'image/png',
                'byteLength': 10,
              }),
          throwsFormatException);
    });

    test('unknown kind throws', () {
      expect(
          () => MediaSource.fromJson({'kind': 'mystery'}),
          throwsFormatException);
    });

    test('MediaSpec validates alt length', () {
      final json = {
        'source': {'kind': 'path', 'path': '/a.png'},
        'intrinsic': {'w': 10, 'h': 10},
        'alt': List.filled(16385, 'x').join(),
      };
      expect(() => MediaSpec.fromJson(json), throwsFormatException);
    });
  });

  group('SurfaceSpec', () {
    SurfaceSpec surface() => const SurfaceSpec(
          reference:
              SurfaceReference(sessionID: 'sess1', streamID: 'stream1'),
        );

    test('round trip with transparent background default', () {
      final spec = surface();
      final decoded = SurfaceSpec.fromJson(spec.toJson());
      expect(decoded, spec);
      expect(decoded.background,
          isA<SurfaceBackgroundTransparent>());
    });

    test('solid background round trip', () {
      final spec = SurfaceSpec(
        reference:
            const SurfaceReference(sessionID: 's', streamID: 't'),
        background: const SurfaceBackgroundSolid('#112233'),
      );
      expect(SurfaceSpec.fromJson(spec.toJson()), spec);
    });

    test('rejects non-portable identifiers', () {
      final json = surface().toJson()
        ..['reference'] = {'sessionId': 'has space', 'streamId': 't'};
      expect(() => SurfaceSpec.fromJson(json), throwsFormatException);
    });

    test('resolvedPointSize derives the missing axis', () {
      final spec = SurfaceSpec(
        reference:
            const SurfaceReference(sessionID: 's', streamID: 't'),
        points: const SurfacePointSize(w: 100),
      );
      final size =
          spec.resolvedPointSize(const SurfaceViewportSize(w: 50, h: 25));
      expect(size, isNotNull);
      // ratioCeil(100, 25, 50) == 50.
      expect(size, (w: 100, h: 50));
    });
  });

  group('ratioCeil', () {
    test('exact division', () {
      expect(ratioCeil(100, 1, 2), 50);
    });

    test('rounds up', () {
      expect(ratioCeil(101, 1, 2), 51);
    });

    test('clamps negative inputs', () {
      expect(ratioCeil(-5, 1, 2), 0);
    });

    test('clamps zero numerator/denominator to one', () {
      expect(ratioCeil(10, 0, 0), 10);
    });
  });

  group('isPortableUIIdentifier', () {
    test('accepts portable identifiers', () {
      expect(isPortableUIIdentifier('abc-123_X.y:z/w'), isTrue);
    });

    test('rejects empty and invalid identifiers', () {
      expect(isPortableUIIdentifier(''), isFalse);
      expect(isPortableUIIdentifier('has space'), isFalse);
      expect(isPortableUIIdentifier('semi;colon'), isFalse);
    });
  });

  group('MarkdownEditorSpec', () {
    MarkdownEditorSpec editor() => const MarkdownEditorSpec(
          text: 'hello',
          anchorLine: 0,
          anchorColumn: 0,
          headLine: 0,
          headColumn: 5,
        );

    test('round trip', () {
      expect(
          MarkdownEditorSpec.fromJson(editor().toJson()), editor());
    });

    test('visibility: explicit insert menu suppresses the trigger', () {
      final withMenu = MarkdownEditorSpec.fromJson({
        ...editor().toJson(),
        'insertMenu': {
          'label': 'Insert',
          'items': [
            {'id': 'a', 'label': 'A', 'action': 'do-a'}
          ],
        },
      });
      expect(withMenu.insertMenu, isNotNull);
      expect(withMenu.menuTriggerForTextInput('/'), isNull);
    });

    test('slash triggers the palette when openMenu is set', () {
      final slashable = MarkdownEditorSpec(
        text: '',
        anchorLine: 0,
        anchorColumn: 0,
        headLine: 0,
        headColumn: 0,
        actions: const MarkdownEditorActions(openMenu: 'open-menu'),
      );
      expect(slashable.menuTriggerForTextInput('/'),
          MarkdownMenuTrigger.slash);
      expect(slashable.menuTriggerForTextInput('x'), isNull);
      expect(
          const MarkdownEditorSpec(
            text: '',
            anchorLine: 0,
            anchorColumn: 0,
            headLine: 0,
            headColumn: 0,
            readOnly: true,
            actions: MarkdownEditorActions(openMenu: 'open-menu'),
          ).menuTriggerForTextInput('/'),
          isNull);
    });

    test('footer validity and duplicate detection', () {
      const footer = UIFooterActionsSpec(actions: [
        UIFooterActionSpec(
            id: 'ok', label: 'OK', action: 'submit', accelerator: 'enter'),
        UIFooterActionSpec(
            id: 'ok', label: 'Again', action: 'submit'),
      ]);
      expect(footer.isValid, isFalse,
          reason: 'duplicate action ids must invalidate the footer');
    });

    test('accelerator validation', () {
      expect(isFooterAccelerator('enter'), isTrue);
      expect(isFooterAccelerator('ctrl+s'), isTrue);
      expect(isFooterAccelerator('x'), isTrue);
      expect(isFooterAccelerator('ctrl+'), isFalse);
      expect(isFooterAccelerator('shift+x'), isFalse);
    });
  });

  group('UICanvasControl', () {
    test('button dispatch round trip', () {
      const control = UICanvasControlButton(
          UIButtonSpec(id: 'b1', label: 'Run', action: 'run'));
      expect(UICanvasControl.fromJson(control.toJson()), control);
    });

    test('unsupported control preserves its kind', () {
      const control = UICanvasControlUnsupported('slider');
      final decoded = UICanvasControl.fromJson(control.toJson());
      expect(decoded, isA<UICanvasControlUnsupported>());
      expect((decoded as UICanvasControlUnsupported).kind, 'slider');
    });

    test('unknown control type decodes as unsupported', () {
      // Swift keeps unknown controls as `.unsupported`, not an error.
      final decoded =
          UICanvasControl.fromJson({'type': 'teleporter'});
      expect(decoded, isA<UICanvasControlUnsupported>());
    });
  });

  group('toggle/checkmark encoding', () {
    test('toggle round trip preserves role', () {
      const toggle = UIToggleSpec(
        id: 't1',
        label: 'Done',
        value: true,
        setValue: 'set',
        role: UIToggleRole.completion,
      );
      expect(UIToggleSpec.fromJson(toggle.toJson()), toggle);
    });

    test('toggle role defaults to completion', () {
      final json = {
        'id': 't1',
        'label': 'Done',
        'value': false,
        'setValue': 'set',
      };
      expect(UIToggleSpec.fromJson(json).role,
          UIToggleRole.completion);
    });

    test('checkmark round trip', () {
      const checkmark = UICheckmarkSpec(
          id: 'c1', label: 'Picked', value: true, setValue: 'set');
      expect(UICheckmarkSpec.fromJson(checkmark.toJson()), checkmark);
    });
  });

  group('status symbol and badge', () {
    test('status symbol round trip', () {
      const symbol = UIStatusSymbolSpec(
        symbol: 'ok',
        label: 'OK',
        tone: UIListItemTone.success,
      );
      expect(UIStatusSymbolSpec.fromJson(symbol.toJson()), symbol);
    });

    test('badge round trip', () {
      const badge =
          UIBadgeSpec(text: '3', tone: UIListItemTone.muted);
      expect(UIBadgeSpec.fromJson(badge.toJson()), badge);
    });
  });
}
