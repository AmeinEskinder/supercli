/// Tests for the wire-faithful protocol message layer:
/// `appkit_protocol_messages.dart` (+ `appkit_protocol_delta.dart`).
///
/// Mirrors `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIProtocol.swift`
/// (message envelope section) and `UIDelta.swift`.
library;

import 'package:supercli_app/widgets/appkit_protocol_delta.dart';
import 'package:supercli_app/widgets/appkit_protocol_lists.dart';
import 'package:supercli_app/widgets/appkit_protocol_messages.dart';
import 'package:supercli_app/widgets/appkit_protocol_widgets.dart';
import 'package:test/test.dart';

Map<String, dynamic> _baseMessage(String type, Map<String, dynamic> extra) =>
    {
      'protocol': UIProtocol.name,
      'protocolVersion': UIProtocol.version,
      ...extra,
      'type': type,
    };

UIAttach _attach() => UIAttach(
      participantToken: 'secret-token-123',
      clientID: 'client',
      renderer: const UIRendererMetadata(
          id: 'renderer', kind: 'test', capabilities: ['markdownEditor']),
      viewID: 'view',
    );

void main() {
  group('UIProtocol version negotiation', () {
    test('supports the current version', () {
      expect(UIProtocol.supports(UIProtocol.version), isTrue);
      expect(UIProtocol.supports(UIProtocol.minimumVersion), isTrue);
      expect(UIProtocol.supports(UIProtocol.maximumVersion), isTrue);
    });

    test('rejects out-of-range versions', () {
      expect(UIProtocol.supports(0), isFalse);
      expect(UIProtocol.supports(-1), isFalse);
      expect(UIProtocol.supports(UIProtocol.maximumVersion + 1), isFalse);
    });

    test('negotiate picks the shared maximum', () {
      expect(
          UIProtocol.negotiate(
              minimum: UIProtocol.minimumVersion,
              maximum: UIProtocol.maximumVersion),
          UIProtocol.maximumVersion);
    });

    test('negotiate fails outside the supported range', () {
      expect(
          UIProtocol.negotiate(
              minimum: UIProtocol.maximumVersion + 1,
              maximum: UIProtocol.maximumVersion + 2),
          isNull);
      expect(
          UIProtocol.negotiate(
              minimum: 1, maximum: UIProtocol.minimumVersion - 1),
          isNull);
      expect(UIProtocol.negotiate(minimum: 0, maximum: 5), isNull);
      expect(UIProtocol.negotiate(minimum: 5, maximum: 1), isNull);
    });
  });

  group('UITextPosition ordering', () {
    test('orders by line first, then column', () {
      const a = UITextPosition(line: 0, utf16Column: 10);
      const b = UITextPosition(line: 1, utf16Column: 0);
      const c = UITextPosition(line: 0, utf16Column: 4);
      expect(a.compareTo(b), lessThan(0));
      expect(b.compareTo(a), greaterThan(0));
      expect(a.compareTo(c), greaterThan(0));
      expect(a.compareTo(a), 0);
    });

    test('round-trips JSON', () {
      const p = UITextPosition(line: 3, utf16Column: 7);
      expect(UITextPosition.fromJson(p.toJson()), p);
    });

    test('selection caret factory collapses anchor and head', () {
      const p = UITextPosition(line: 1, utf16Column: 2);
      expect(UITextSelection.caret(p), UITextSelection(anchor: p, head: p));
    });
  });

  group('UIEventValue wire round trips (all nine cases)', () {
    UIEvent eventFor(UIEventValue value) => UIEvent(
          appInstanceID: 'app',
          participantID: 'participant',
          clientID: 'client',
          rendererID: 'renderer',
          viewID: 'view',
          eventID: 'evt',
          baseRevision: 3,
          action: UIAction(
            nodeID: 'node',
            action: 'activate',
            kind: UIEventKind.activate,
            value: value,
          ),
        );

    void check(UIEventValue value) {
      final event = eventFor(value);
      expect(UIEvent.fromJson(event.toJson()), event);
    }

    test('none', () => check(const UIEventValueNone()));
    test('bool', () => check(const UIEventValueBool(true)));
    test('index', () => check(const UIEventValueIndex(4)));
    test('integer', () => check(const UIEventValueInteger(-42)));
    test('number', () => check(const UIEventValueNumber(2.5)));
    test('text', () => check(const UIEventValueText('hello')));
    test('textList',
        () => check(const UIEventValueTextList(['a', 'b', 'c'])));
    test(
        'textEdit',
        () => check(const UIEventValueTextEdit(UITextEdit(
            range: UITextRange(
                start: UITextPosition(line: 0, utf16Column: 1),
                end: UITextPosition(line: 0, utf16Column: 3)),
            text: 'xy'))));
    test(
        'textSelection',
        () => check(const UIEventValueTextSelection(UITextSelection(
            anchor: UITextPosition(line: 0, utf16Column: 0),
            head: UITextPosition(line: 2, utf16Column: 5)))));

    test('fromJson dispatch covers every type string', () {
      final cases = <String, UIEventValue>{
        'none': const UIEventValueNone(),
        'bool': const UIEventValueBool(false),
        'index': const UIEventValueIndex(0),
        'integer': const UIEventValueInteger(1),
        'number': const UIEventValueNumber(0.5),
        'text': const UIEventValueText('x'),
        'textList': const UIEventValueTextList([]),
        'textEdit': const UIEventValueTextEdit(UITextEdit(
            range: UITextRange(
                start: UITextPosition(line: 0, utf16Column: 0),
                end: UITextPosition(line: 0, utf16Column: 0)),
            text: '')),
        'textSelection': UIEventValueTextSelection(UITextSelection.caret(
            const UITextPosition(line: 0, utf16Column: 0))),
      };
      for (final entry in cases.entries) {
        final json = eventFor(entry.value).toJson();
        final valueJson =
            (json['action'] as Map<String, dynamic>)['value']
                as Map<String, dynamic>;
        expect(valueJson['type'], entry.key);
        expect(UIEvent.fromJson(json), eventFor(entry.value));
      }
    });

    test('unknown event value type throws', () {
      final json = eventFor(const UIEventValueNone()).toJson();
      final action = json['action'] as Map<String, dynamic>;
      action['value'] = {'type': 'nope'};
      expect(() => UIEvent.fromJson(json), throwsFormatException);
    });
  });

  group('UIMessage envelope validation', () {
    Map<String, dynamic> ackJson() => _baseMessage('ack', {
          'appInstanceId': 'app',
          'clientId': 'client',
          'rendererId': 'renderer',
          'viewId': 'view',
          'eventId': 'evt',
          'status': 'applied',
          'revision': 4,
        });

    test('rejects the wrong protocol name', () {
      final json = ackJson()..['protocol'] = 'other.protocol';
      expect(() => UIMessage.fromJson(json), throwsFormatException);
    });

    test('rejects an unsupported protocol version', () {
      final json = ackJson()
        ..['protocolVersion'] = UIProtocol.maximumVersion + 99;
      expect(() => UIMessage.fromJson(json), throwsFormatException);
    });

    test('rejects a missing protocol version', () {
      final json = ackJson()..remove('protocolVersion');
      expect(() => UIMessage.fromJson(json), throwsFormatException);
    });

    test('attach is exempt from version selection checks', () {
      final message = UIMessage.fromJson({
        'protocol': UIProtocol.name,
        'type': 'attach',
        ..._attach().toJson(),
      });
      expect(message, isA<UIMessageAttach>());
      expect(message.protocolVersion, isNull);
    });

    test('attach rejects an invalid version range', () {
      final json = _attach().toJson()
        ..['minProtocolVersion'] = 0
        ..['maxProtocolVersion'] = 0;
      expect(
          () => UIMessage.fromJson(
              {'protocol': UIProtocol.name, 'type': 'attach', ...json}),
          throwsFormatException);
    });

    test('attached rejects a version outside the server range', () {
      const attached = UIAttached(
        protocolVersion: UIProtocol.version,
        minProtocolVersion: UIProtocol.minimumVersion,
        maxProtocolVersion: UIProtocol.maximumVersion,
        app: AppMetadata(id: 'app', name: 'App', version: '1.0'),
        appInstanceID: 'app',
        participantID: 'p',
        clientID: 'c',
        rendererID: 'r',
        viewID: 'v',
        resumed: false,
      );
      final json = attached.toJson()..['protocolVersion'] = 999;
      expect(
          () => UIMessage.fromJson(
              {'protocol': UIProtocol.name, 'type': 'attached', ...json}),
          throwsFormatException);
    });

    test('unknown message type throws', () {
      expect(
          () => UIMessage.fromJson(_baseMessage('frobnicate', {})),
          throwsFormatException);
    });
  });

  group('attach token redaction', () {
    test('toString never prints the participant token', () {
      final printed = _attach().toString();
      expect(printed, contains('[REDACTED]'));
      expect(printed, isNot(contains('secret-token-123')));
    });

    test('attach round-trips through JSON', () {
      final attach = _attach();
      expect(UIAttach.fromJson(attach.toJson()), attach);
    });

    test('empty participant token is rejected', () {
      final json = _attach().toJson()..['participantToken'] = '';
      expect(
          () => UIMessage.fromJson(
              {'protocol': UIProtocol.name, 'type': 'attach', ...json}),
          throwsFormatException);
    });
  });

  group('message round trips', () {
    UISnapshot snapshot() => const UISnapshot(
          protocolVersion: 1,
          appInstanceID: 'app',
          clientID: 'client',
          viewID: 'view',
          revision: 7,
          root: UINode(
              id: 'root',
              component: UIComponentTextBox(
                  TextBoxSpec(text: 'hi'))),
        );

    test('snapshot round trip', () {
      final message = UIMessageSnapshot(snapshot());
      final decoded = UIMessage.fromJson(
          {'type': 'snapshot', ...snapshot().toJson()});
      expect(decoded, message);
      expect((decoded as UIMessageSnapshot).snapshot.revision, 7);
    });

    test('ack round trip', () {
      const ack = UIAck(
        appInstanceID: 'app',
        clientID: 'client',
        rendererID: 'renderer',
        viewID: 'view',
        eventID: 'evt-1',
        status: UIAckStatus.applied,
        revision: 9,
      );
      final decoded =
          UIMessage.fromJson({'type': 'ack', ...ack.toJson()});
      expect(decoded, UIMessageAck(ack));
    });

    test('presence round trip', () {
      const presence = UIPresence(
        appInstanceID: 'app',
        viewID: 'view',
        members: [
          UIPresenceMember(
            participant: UIParticipant(id: 'p1'),
            clientID: 'c1',
            renderer: UIRendererMetadata(
                id: 'r1', kind: 'test', capabilities: []),
            state: UIRendererState.terminal,
          ),
        ],
      );
      final decoded =
          UIMessage.fromJson({'type': 'presence', ...presence.toJson()});
      expect(decoded, UIMessagePresence(presence));
    });

    test('error round trip', () {
      const error = UIErrorMessage(code: 'badInput', message: 'nope');
      final decoded =
          UIMessage.fromJson({'type': 'error', ...error.toJson()});
      expect(decoded, UIMessageError(error));
    });

    test('lifecycle round trip', () {
      const lifecycle = UILifecycle(
        appInstanceID: 'app',
        clientID: 'client',
        rendererID: 'renderer',
        viewID: 'view',
        state: UIRendererState.terminal,
      );
      final decoded =
          UIMessage.fromJson({'type': 'lifecycle', ...lifecycle.toJson()});
      expect(decoded, UIMessageLifecycle(lifecycle));
    });

    test('requestSnapshot round trip', () {
      const request = UIRequestSnapshot(
        appInstanceID: 'app',
        clientID: 'client',
        rendererID: 'renderer',
        viewID: 'view',
      );
      final decoded = UIMessage.fromJson(
          {'type': 'requestSnapshot', ...request.toJson()});
      expect(decoded, UIMessageRequestSnapshot(request));
    });

    test('attached round trip', () {
      const attached = UIAttached(
        protocolVersion: 1,
        minProtocolVersion: 1,
        maxProtocolVersion: 1,
        app: AppMetadata(id: 'app', name: 'App', version: '1.0'),
        appInstanceID: 'app',
        participantID: 'p',
        clientID: 'c',
        rendererID: 'r',
        viewID: 'v',
        resumed: true,
        currentRevision: 3,
      );
      final decoded = UIMessage.fromJson(
          {'type': 'attached', ...attached.toJson()});
      expect(decoded, UIMessageAttached(attached));
    });

    test('delta message round-trips through the typed UIDelta', () {
      final delta = UIDelta(
        protocolVersion: UIProtocol.version,
        appInstanceID: 'app',
        clientID: 'client',
        viewID: 'view',
        baseRevision: 7,
        revision: 8,
        operations: [
          const UIDeltaOperationMarkdownReplaceRange(
            nodeID: 'editor',
            edit: UITextEdit(
              range: UITextRange(
                  start: UITextPosition(line: 0, utf16Column: 0),
                  end: UITextPosition(line: 0, utf16Column: 0)),
              text: 'hi',
            ),
          ),
        ],
      );
      final message = UIMessageDelta(delta);
      final decoded =
          UIMessage.fromJson({'type': 'delta', ...delta.toJson()});
      expect(decoded, isA<UIMessageDelta>());
      expect((decoded as UIMessageDelta).delta, delta);
      expect(message, decoded);
    });
  });
}
