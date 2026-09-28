import 'package:test/test.dart';

import 'package:supercli_app/host_management_state.dart';

void main() {
  group('HostManagementValue', () {
    test('equality ignores list identity', () {
      const a = HostManagementValue(
        devices: [HostManagementDevice(id: '1', name: 'Phone')],
        endpoint: null,
        error: null,
      );
      const b = HostManagementValue(
        devices: [HostManagementDevice(id: '1', name: 'Phone')],
      );
      expect(a, equals(b));
      expect(a.hashCode, equals(b.hashCode));
    });

    test('devices compare by value', () {
      const a = HostManagementValue(
        devices: [HostManagementDevice(id: '1', name: 'Phone')],
      );
      const b = HostManagementValue(
        devices: [HostManagementDevice(id: '1', name: 'Tablet')],
      );
      expect(a, isNot(equals(b)));
    });

    test('endpoint and error participate in equality', () {
      const base = HostManagementValue();
      expect(base, isNot(equals(const HostManagementValue(error: 'x'))));
      expect(
        base,
        isNot(equals(HostManagementValue(endpoint: Uri.parse('https://h:1')))),
      );
    });
  });

  group('HostManagementState', () {
    test('apply publishes on change', () {
      final state = HostManagementState();
      var notified = 0;
      state.addListener(() => notified++);
      state.apply(const HostManagementValue(error: 'boom'));
      expect(notified, 1);
      expect(state.value.error, 'boom');
    });

    test('apply is a no-op for a repeated response', () {
      final state = HostManagementState(
        const HostManagementValue(error: 'boom'),
      );
      var notified = 0;
      state.addListener(() => notified++);
      // Same value again: no publication.
      state.apply(const HostManagementValue(error: 'boom'));
      expect(notified, 0);
    });

    test('updateEndpoint preserves devices and error', () {
      final state = HostManagementState(
        const HostManagementValue(
          devices: [HostManagementDevice(id: '1', name: 'Phone')],
          error: 'old',
        ),
      );
      final endpoint = Uri.parse('https://host:8443');
      state.updateEndpoint(endpoint);
      expect(state.value.endpoint, endpoint);
      expect(state.value.devices, hasLength(1));
      expect(state.value.error, 'old');
    });

    test('updateEndpoint(null) clears the endpoint', () {
      final state = HostManagementState(
        HostManagementValue(endpoint: Uri.parse('https://host:8443')),
      );
      state.updateEndpoint(null);
      expect(state.value.endpoint, isNull);
    });

    test('updateError preserves devices and endpoint', () {
      final endpoint = Uri.parse('https://host:8443');
      final state = HostManagementState(
        HostManagementValue(
          devices: [HostManagementDevice(id: '1', name: 'Phone')],
          endpoint: endpoint,
        ),
      );
      state.updateError('denied');
      expect(state.value.error, 'denied');
      expect(state.value.endpoint, endpoint);
      expect(state.value.devices, hasLength(1));
    });

    test('removeListener stops notifications', () {
      final state = HostManagementState();
      var notified = 0;
      void listener() => notified++;
      state.addListener(listener);
      state.removeListener(listener);
      state.updateError('x');
      expect(notified, 0);
    });
  });
}
