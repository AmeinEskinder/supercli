/// Tests for the push registration state machine (port of
/// PushRegistrationState in PushManager.swift).
library;

import 'package:supercli_app/ios/push_state.dart';
import 'package:test/test.dart';

void main() {
  group('PushRegistrationState', () {
    test('diagnostic labels name permission and environment states', () {
      expect(
        const PushRegistrationState.notRequested().diagnosticLabel,
        'Not requested',
      );
      expect(
        const PushRegistrationState.requestingPermission().diagnosticLabel,
        'Waiting for notification permission…',
      );
      expect(
        const PushRegistrationState.permissionDenied().diagnosticLabel,
        'Notifications are denied in iOS Settings',
      );
      expect(
        const PushRegistrationState.registering().diagnosticLabel,
        'Waiting for an APNs device token…',
      );
      expect(
        const PushRegistrationState.registered(environment: 'production')
            .diagnosticLabel,
        'Ready (production)',
      );
      expect(
        const PushRegistrationState.registered(environment: 'sandbox')
            .diagnosticLabel,
        'Ready (sandbox)',
      );
      expect(
        const PushRegistrationState.failed('nope').diagnosticLabel,
        'Registration failed: nope',
      );
    });

    test('sidebar warning only for broken delivery', () {
      expect(
        const PushRegistrationState.permissionDenied().sidebarWarning,
        'Notifications are off',
      );
      expect(
        const PushRegistrationState.failed('x').sidebarWarning,
        "Notifications aren't working",
      );
      // Transient startup states stay quiet.
      expect(const PushRegistrationState.notRequested().sidebarWarning, isNull);
      expect(
        const PushRegistrationState.requestingPermission().sidebarWarning,
        isNull,
      );
      expect(const PushRegistrationState.registering().sidebarWarning, isNull);
      expect(
        const PushRegistrationState.registered(environment: 'production')
            .sidebarWarning,
        isNull,
      );
    });

    test('canRetry gates on terminal states', () {
      expect(const PushRegistrationState.notRequested().canRetry, isTrue);
      expect(const PushRegistrationState.permissionDenied().canRetry, isTrue);
      expect(const PushRegistrationState.failed('x').canRetry, isTrue);
      expect(
        const PushRegistrationState.requestingPermission().canRetry,
        isFalse,
      );
      expect(const PushRegistrationState.registering().canRetry, isFalse);
      expect(
        const PushRegistrationState.registered(environment: 'sandbox').canRetry,
        isFalse,
      );
    });

    test('permissionWasDenied', () {
      expect(
        const PushRegistrationState.permissionDenied().permissionWasDenied,
        isTrue,
      );
      expect(
        const PushRegistrationState.failed('x').permissionWasDenied,
        isFalse,
      );
      expect(
        const PushRegistrationState.notRequested().permissionWasDenied,
        isFalse,
      );
    });

    test('equality', () {
      expect(
        const PushRegistrationState.notRequested(),
        const PushRegistrationState.notRequested(),
      );
      expect(
        const PushRegistrationState.registered(environment: 'production'),
        const PushRegistrationState.registered(environment: 'production'),
      );
      expect(
        const PushRegistrationState.registered(environment: 'production'),
        isNot(const PushRegistrationState.registered(environment: 'sandbox')),
      );
      expect(
        const PushRegistrationState.failed('a'),
        const PushRegistrationState.failed('a'),
      );
      expect(
        const PushRegistrationState.failed('a'),
        isNot(const PushRegistrationState.failed('b')),
      );
      expect(
        const PushRegistrationState.notRequested(),
        isNot(const PushRegistrationState.registering()),
      );
    });
  });

  group('hexEncodeDeviceToken', () {
    test('encodes bytes as lowercase hex', () {
      expect(hexEncodeDeviceToken([0xDE, 0xAD, 0xBE, 0xEF]), 'deadbeef');
    });

    test('pads single nibbles', () {
      expect(hexEncodeDeviceToken([0x0, 0x1, 0xF]), '00010f');
    });

    test('empty token encodes to empty string', () {
      expect(hexEncodeDeviceToken([]), isEmpty);
    });
  });
}
