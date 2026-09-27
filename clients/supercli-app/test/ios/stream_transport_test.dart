/// Tests for the stream transport protocol logic (port of
/// RemoteTerminalStreamTransport.swift).
library;

import 'dart:convert';

import 'package:supercli_app/ios/stream_transport.dart';
import 'package:test/test.dart';

const _fingerprint =
    'a3f5c8d2e1b4967081726354aedcbf0987654321fedcba9876543210abcdef12';

void main() {
  group('RemoteTerminalTransportSelector', () {
    test('fingerprint normalization', () {
      expect(
        RemoteTerminalTransportSelector.normalizedFingerprint(_fingerprint),
        _fingerprint,
      );
      // Strips sha256: prefix, colons, whitespace; lowercases.
      expect(
        RemoteTerminalTransportSelector.normalizedFingerprint(
          '  SHA256:A3:F5:C8:D2:E1:B4:96:70:81:72:63:54:AE:DC:BF:09:87:65:43:21:FE:DC:BA:98:76:54:32:10:AB:CD:EF:12  ',
        ),
        _fingerprint,
      );
      // Wrong length / non-hex rejected.
      expect(
        RemoteTerminalTransportSelector.normalizedFingerprint('abc'),
        isNull,
      );
      expect(
        RemoteTerminalTransportSelector.normalizedFingerprint('z' * 64),
        isNull,
      );
      expect(
        RemoteTerminalTransportSelector.normalizedFingerprint(null),
        isNull,
      );
    });

    test('fingerprints match after normalization', () {
      expect(
        RemoteTerminalTransportSelector.fingerprintsMatch(
          'SHA256:$_fingerprint',
          _fingerprint.toUpperCase(),
        ),
        isTrue,
      );
      expect(
        RemoteTerminalTransportSelector.fingerprintsMatch(
          _fingerprint,
          'b' * 64,
        ),
        isFalse,
      );
      expect(
        RemoteTerminalTransportSelector.fingerprintsMatch(null, _fingerprint),
        isFalse,
      );
    });

    test('endpoint requires port and fingerprint', () {
      final ep = RemoteTerminalTransportSelector.endpoint(
        port: 8443,
        fingerprint: _fingerprint,
      );
      expect(ep?.port, 8443);
      expect(ep?.certificateFingerprint, _fingerprint);
      expect(
        RemoteTerminalTransportSelector.endpoint(
          port: null,
          fingerprint: _fingerprint,
        ),
        isNull,
      );
      expect(
        RemoteTerminalTransportSelector.endpoint(
          port: 0,
          fingerprint: _fingerprint,
        ),
        isNull,
      );
      expect(
        RemoteTerminalTransportSelector.endpoint(
          port: 70000,
          fingerprint: _fingerprint,
        ),
        isNull,
      );
      expect(
        RemoteTerminalTransportSelector.endpoint(
          port: 8443,
          fingerprint: 'nope',
        ),
        isNull,
      );
    });

    test('candidate selects WS or long-poll', () {
      final ep = RemoteTerminalTransportSelector.endpoint(
        port: 8443,
        fingerprint: _fingerprint,
      );
      final candidate = RemoteTerminalTransportSelector.candidate(
        endpoint: ep,
        host: 'mac.local',
        authToken: 'tok',
      );
      expect(candidate?.host, 'mac.local');
      expect(candidate?.port, 8443);
      expect(candidate?.certificateFingerprint, _fingerprint);
      expect(candidate?.token, 'tok');
      // No endpoint, host, or token: long-poll (null).
      expect(
        RemoteTerminalTransportSelector.candidate(
          endpoint: null,
          host: 'mac.local',
          authToken: 'tok',
        ),
        isNull,
      );
      expect(
        RemoteTerminalTransportSelector.candidate(
          endpoint: ep,
          host: '',
          authToken: 'tok',
        ),
        isNull,
      );
      expect(
        RemoteTerminalTransportSelector.candidate(
          endpoint: ep,
          host: 'mac.local',
          authToken: '',
        ),
        isNull,
      );
      expect(
        RemoteTerminalTransportSelector.candidate(
          endpoint: ep,
          host: 'mac.local',
          authToken: null,
        ),
        isNull,
      );
    });

    test('websocket output URL', () {
      final url = RemoteTerminalTransportSelector.webSocketOutputUrl(
        host: 'mac.local',
        port: 8443,
        sessionId: 's1',
        token: 'tok',
        offset: 1234,
      );
      expect(
        url.toString(),
        'wss://mac.local:8443/api/sessions/s1/output?token=tok&offset=1234',
      );
      final noOffset = RemoteTerminalTransportSelector.webSocketOutputUrl(
        host: 'mac.local',
        port: 8443,
        sessionId: 's1',
        token: 'tok',
        offset: null,
      );
      expect(
        noOffset.toString(),
        'wss://mac.local:8443/api/sessions/s1/output?token=tok',
      );
      expect(
        RemoteTerminalTransportSelector.webSocketOutputUrl(
          host: '',
          port: 8443,
          sessionId: 's1',
          token: 't',
          offset: null,
        ),
        isNull,
      );
    });
  });

  group('RemoteTerminalWSServerMessage', () {
    test('parses hello and error', () {
      final helloJson = json.encode({
        'type': 'hello',
        'protocol': 2,
        'session_id': 's1',
        'state': 'live',
        'output_size': 98765,
        'requested_offset': 100,
        'start_offset': 120,
        'rebased': true,
        'cols': 80,
        'rows': 24,
        'mode_preamble_base64': base64.encode([0x1b, 0x5b, 0x33, 0x34, 0x68]),
      });
      final message = parseWsServerMessage(helloJson);
      expect(message, isA<WsHelloMessage>());
      final hello = (message as WsHelloMessage).hello;
      expect(hello.protocolVersion, 2);
      expect(hello.sessionId, 's1');
      expect(hello.outputSize, 98765);
      expect(hello.requestedOffset, 100);
      expect(hello.startOffset, 120);
      expect(hello.rebased, isTrue);
      expect(hello.modePreamble, [0x1b, 0x5b, 0x33, 0x34, 0x68]);

      final error = parseWsServerMessage(
        json.encode({'type': 'error', 'message': 'boom'}),
      );
      expect(error, const WsErrorMessage('boom'));

      expect(parseWsServerMessage('not json'), const WsUnknownMessage());
      expect(
        parseWsServerMessage(json.encode({'type': 'weird'})),
        const WsUnknownMessage(),
      );
      // Malformed hello falls back to unknown, not a crash.
      expect(
        parseWsServerMessage(json.encode({'type': 'hello'})),
        const WsUnknownMessage(),
      );
    });

    test('hello carries optional mode preamble', () {
      final helloJson = json.encode({
        'type': 'hello',
        'protocol': 1,
        'session_id': 's1',
        'state': 'live',
        'output_size': 10,
        'start_offset': 0,
        'rebased': false,
      });
      final message = parseWsServerMessage(helloJson);
      final hello = (message as WsHelloMessage).hello;
      expect(hello.modePreamble, isNull);
      expect(hello.requestedOffset, isNull);
      expect(hello.cols, isNull);
    });
  });

  group('RemoteTerminalWSBinaryFrame', () {
    test('parses 8-byte big-endian offset prefix', () {
      // Offset 0x0102030405060708, payload 'hi'.
      final data = [1, 2, 3, 4, 5, 6, 7, 8, 0x68, 0x69];
      final frame = RemoteTerminalWSBinaryFrame.parse(data);
      expect(frame?.offset, 0x0102030405060708);
      expect(frame?.payload, [0x68, 0x69]);
    });

    test('short frames rejected', () {
      expect(RemoteTerminalWSBinaryFrame.parse([1, 2, 3]), isNull);
      expect(RemoteTerminalWSBinaryFrame.parse(const []), isNull);
    });

    test('empty payload is fine', () {
      final frame = RemoteTerminalWSBinaryFrame.parse([
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        42,
      ]);
      expect(frame?.offset, 42);
      expect(frame?.payload, isEmpty);
    });
  });

  group('RemoteTerminalWSClientMessage', () {
    test('single frame carries write id', () {
      final frames = RemoteTerminalWSClientMessage.inputFrames(
        'ls\n',
        writeId: 'w1',
      );
      expect(frames.length, 1);
      final decoded = json.decode(frames.single) as Map<String, Object?>;
      expect(decoded['type'], 'input');
      expect(decoded['data'], 'ls\n');
      expect(decoded['wid'], 'w1');
    });

    test('empty input yields no frames', () {
      expect(RemoteTerminalWSClientMessage.inputFrames(''), isEmpty);
    });

    test('input frames chunk on character boundaries', () {
      // 'é' is 2 bytes in UTF-8; maxBytes=3 forces a split that must not
      // tear the multi-byte scalar.
      final frames = RemoteTerminalWSClientMessage.inputFrames(
        'aéb',
        maxBytes: 3,
      );
      expect(frames.length, 2);
      final first = json.decode(frames[0]) as Map<String, Object?>;
      final second = json.decode(frames[1]) as Map<String, Object?>;
      expect(first['data'], 'aé');
      expect(second['data'], 'b');
      // Multi-frame sends drop the idempotency key (no ambiguous partial).
      expect(first.containsKey('wid'), isFalse);
    });

    test('large input chunks under the cap', () {
      final big =
          'x' * (RemoteTerminalWSClientMessage.maxInputBytesPerFrame + 10);
      final frames = RemoteTerminalWSClientMessage.inputFrames(big);
      expect(frames.length, 2);
      for (final frame in frames) {
        final decoded = json.decode(frame) as Map<String, Object?>;
        final data = decoded['data'] as String;
        expect(
          utf8.encode(data).length,
          lessThanOrEqualTo(
            RemoteTerminalWSClientMessage.maxInputBytesPerFrame,
          ),
        );
      }
    });
  });

  group('RemoteTerminalReconnectBackoff', () {
    test('exponential backoff resets after a frame paints', () {
      final backoff = RemoteTerminalReconnectBackoff(healthySerial: 7);
      expect(backoff.delayAfterFailure(7), 500);
      expect(backoff.delayAfterFailure(7), 1000);
      expect(backoff.delayAfterFailure(7), 2000);
      // A frame painted (new healthy serial): back to the initial delay.
      expect(backoff.delayAfterFailure(8), 500);
      expect(backoff.delayAfterFailure(8), 1000);
    });

    test('backoff caps at the maximum', () {
      final backoff = RemoteTerminalReconnectBackoff(healthySerial: 1);
      var delay = 0;
      for (var i = 0; i < 10; i++) {
        delay = backoff.delayAfterFailure(1);
      }
      expect(delay, RemoteTerminalReconnectBackoff.maximumDelayMs);
    });
  });
}
