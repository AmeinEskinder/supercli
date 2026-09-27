/// Tests for the Direct transport decisions (port of the socket-free
/// logic in RemoteDirectTransport.swift).
library;

import 'package:supercli_app/ios/direct_transport.dart';
import 'package:test/test.dart';

void main() {
  group('RemoteServerVersion', () {
    test('parses plain versions', () {
      expect(
        RemoteServerVersion.parse('0.5.3'),
        const RemoteServerVersion(0, 5, 3),
      );
      expect(
        RemoteServerVersion.parse('v0.5.3'),
        const RemoteServerVersion(0, 5, 3),
      );
      expect(
        RemoteServerVersion.parse('1'),
        const RemoteServerVersion(1, 0, 0),
      );
      expect(
        RemoteServerVersion.parse('0.10'),
        const RemoteServerVersion(0, 10, 0),
      );
    });

    test('ignores pre-release and build suffixes', () {
      expect(
        RemoteServerVersion.parse('0.5.3-beta.1'),
        const RemoteServerVersion(0, 5, 3),
      );
      expect(
        RemoteServerVersion.parse('1.2.3+build.5'),
        const RemoteServerVersion(1, 2, 3),
      );
    });

    test('rejects malformed versions', () {
      expect(RemoteServerVersion.parse(null), isNull);
      expect(RemoteServerVersion.parse(''), isNull);
      expect(RemoteServerVersion.parse('  '), isNull);
      expect(RemoteServerVersion.parse('a.b.c'), isNull);
      expect(RemoteServerVersion.parse('1.2.3.4'), isNull);
      expect(RemoteServerVersion.parse('1..3'), isNull);
      expect(RemoteServerVersion.parse('1.2.x'), isNull);
    });

    test('ordering', () {
      expect(
        const RemoteServerVersion(
          0,
          5,
          3,
        ).compareTo(const RemoteServerVersion(0, 5, 3)),
        0,
      );
      expect(
        const RemoteServerVersion(
              0,
              4,
              9,
            ).compareTo(const RemoteServerVersion(0, 5, 3)) <
            0,
        isTrue,
      );
      expect(
        const RemoteServerVersion(
              0,
              10,
              0,
            ).compareTo(const RemoteServerVersion(0, 5, 3)) >
            0,
        isTrue,
      );
      expect(
        const RemoteServerVersion(
              1,
              0,
              0,
            ).compareTo(const RemoteServerVersion(0, 99, 99)) >
            0,
        isTrue,
      );
    });
  });

  group('transportDecision', () {
    test('capability flag selects TLS', () {
      const ad = RemoteDirectTransportAdvertisement(
        certificateFingerprint: 'ABCDEF1234',
        hostCapabilities: {kMobileTlsCapability},
      );
      expect(
        transportDecision(ad),
        const DirectTransportTls('abcdef1234'),
      ); // normalized
    });

    test('capability flag without fingerprint is unpinnable', () {
      const ad = RemoteDirectTransportAdvertisement(
        hostCapabilities: {kMobileTlsCapability},
      );
      expect(transportDecision(ad), const DirectTransportTlsUnpinnable());
    });

    test('version at/after minimum means TLS', () {
      const ad = RemoteDirectTransportAdvertisement(
        certificateFingerprint: 'ff',
        serverVersion: '0.5.3',
      );
      expect(transportDecision(ad), const DirectTransportTls('ff'));
      const newer = RemoteDirectTransportAdvertisement(
        certificateFingerprint: 'ff',
        serverVersion: '0.10.0',
      );
      expect(transportDecision(newer), const DirectTransportTls('ff'));
    });

    test('version below minimum means plaintext', () {
      const ad = RemoteDirectTransportAdvertisement(serverVersion: '0.4.9');
      expect(transportDecision(ad), const DirectTransportPlaintext());
    });

    test('no signal keeps the current transport', () {
      const ad = RemoteDirectTransportAdvertisement();
      expect(transportDecision(ad), const DirectTransportUnknown());
      // Unparseable version is also no signal.
      const bad = RemoteDirectTransportAdvertisement(serverVersion: 'nope');
      expect(transportDecision(bad), const DirectTransportUnknown());
    });
  });

  group('normalizedFingerprint', () {
    test('trims and lowercases; empty becomes null', () {
      expect(normalizedFingerprint('  AB:CD  '), 'ab:cd');
      expect(normalizedFingerprint(''), isNull);
      expect(normalizedFingerprint('   '), isNull);
      expect(normalizedFingerprint(null), isNull);
    });
  });

  group('isPlaintextRefusal', () {
    test('426 is always a refusal', () {
      expect(isPlaintextRefusal(statusCode: 426), isTrue);
      expect(
        isPlaintextRefusal(statusCode: 426, serverMessage: 'whatever'),
        isTrue,
      );
    });

    test('401 naming https/tls is a refusal', () {
      expect(
        isPlaintextRefusal(
          statusCode: 401,
          serverMessage: 'use https://host/mobile',
        ),
        isTrue,
      );
      expect(
        isPlaintextRefusal(statusCode: 401, serverMessage: 'TLS required'),
        isTrue,
      );
    });

    test('other 4xx keep their meaning', () {
      expect(isPlaintextRefusal(statusCode: 401), isFalse);
      expect(
        isPlaintextRefusal(statusCode: 401, serverMessage: 'bad credentials'),
        isFalse,
      );
      expect(isPlaintextRefusal(statusCode: 403), isFalse);
      expect(isPlaintextRefusal(statusCode: 404), isFalse);
    });
  });

  group('canonicalStoredEndpoint', () {
    test('https endpoints stored canonically as http', () {
      expect(
        canonicalStoredEndpoint('https://mac.local:8443/mobile'),
        'http://mac.local:8443/mobile',
      );
    });

    test('http endpoints unchanged', () {
      expect(
        canonicalStoredEndpoint('http://mac.local:8443/mobile'),
        'http://mac.local:8443/mobile',
      );
    });

    test('unparseable input returned as-is', () {
      expect(canonicalStoredEndpoint('not a url'), 'not a url');
    });
  });

  group('RemoteBootstrapDeadline', () {
    test('direct is always 4s', () {
      expect(RemoteBootstrapDeadline.seconds(isRelay: false), 4.0);
      expect(
        RemoteBootstrapDeadline.seconds(
          isRelay: false,
          measuredRoundTrip: 30.0,
        ),
        4.0,
      );
    });

    test('relay without measurement uses the minimum', () {
      expect(RemoteBootstrapDeadline.seconds(isRelay: true), 10.0);
      expect(
        RemoteBootstrapDeadline.seconds(isRelay: true, measuredRoundTrip: 0),
        10.0,
      );
      expect(
        RemoteBootstrapDeadline.seconds(isRelay: true, measuredRoundTrip: -1),
        10.0,
      );
      expect(
        RemoteBootstrapDeadline.seconds(
          isRelay: true,
          measuredRoundTrip: double.nan,
        ),
        10.0,
      );
    });

    test('relay deadline scales with round trip within bounds', () {
      // 5x multiplier: 1s RTT -> 10s (minimum), 3s RTT -> 15s, 10s RTT -> 20s (maximum).
      expect(
        RemoteBootstrapDeadline.seconds(isRelay: true, measuredRoundTrip: 1.0),
        10.0,
      );
      expect(
        RemoteBootstrapDeadline.seconds(isRelay: true, measuredRoundTrip: 3.0),
        15.0,
      );
      expect(
        RemoteBootstrapDeadline.seconds(isRelay: true, measuredRoundTrip: 10.0),
        20.0,
      );
    });
  });

  group('pushTokenRegistrationPlan', () {
    test('active mac on relay skips the LAN', () {
      expect(
        pushTokenRegistrationPlan(
          isActiveMac: true,
          usingRelay: true,
          hasRelayCredentials: true,
        ),
        [PushTokenRegistrationRoute.activeRelayClient],
      );
    });

    test('direct macs try LAN then link connection', () {
      expect(
        pushTokenRegistrationPlan(
          isActiveMac: false,
          usingRelay: false,
          hasRelayCredentials: true,
        ),
        [
          PushTokenRegistrationRoute.direct,
          PushTokenRegistrationRoute.transientRelay,
        ],
      );
    });

    test('no relay credentials means direct only', () {
      expect(
        pushTokenRegistrationPlan(
          isActiveMac: false,
          usingRelay: false,
          hasRelayCredentials: false,
        ),
        [PushTokenRegistrationRoute.direct],
      );
    });
  });
}
