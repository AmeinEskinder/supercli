/// Tests for the nearby-Host discovery catalog.
///
/// Port of the `NearbyHostCatalog` behaviour in `NearbyHostBrowser.swift`:
/// TXT-record parsing, dedup, exclusion, and sorting.
library;

import 'package:supercli_app/nearby_hosts.dart';
import 'package:test/test.dart';

void main() {
  group('NearbyHostCatalog.candidate', () {
    test('parses macid and service name', () {
      final c = NearbyHostCatalog.candidate(
        serviceName: 'Amein\u2019s MacBook',
        txt: {'macid': 'host-123'},
      );
      expect(c, isNotNull);
      expect(c!.hostId, 'host-123');
      expect(c.name, 'Amein\u2019s MacBook');
    });

    test('returns null when macid is missing', () {
      expect(NearbyHostCatalog.candidate(serviceName: 'Mac', txt: {}), isNull);
    });

    test('returns null when macid is blank', () {
      expect(
        NearbyHostCatalog.candidate(serviceName: 'Mac', txt: {'macid': '   '}),
        isNull,
      );
    });

    test('trims whitespace around macid', () {
      final c = NearbyHostCatalog.candidate(
        serviceName: 'Mac',
        txt: {'macid': '  host-9\n'},
      );
      expect(c!.hostId, 'host-9');
    });

    test('falls back to Supercli Host when the name is blank', () {
      final c = NearbyHostCatalog.candidate(
        serviceName: '   ',
        txt: {'macid': 'host-1'},
      );
      expect(c!.name, 'Supercli Host');
    });

    test('service type matches the Swift constant', () {
      expect(NearbyHostCatalog.serviceType, '_supercli-remote._tcp');
    });
  });

  group('NearbyHostCatalog.merging', () {
    NearbyHostCandidate cand(String id, String name) =>
        NearbyHostCandidate(hostId: id, name: name);

    test('deduplicates by host id case-insensitively', () {
      final merged = NearbyHostCatalog.merging([
        cand('HOST-1', 'Mac A'),
        cand('host-1', 'Mac A duplicate'),
        cand('host-2', 'Mac B'),
      ]);
      expect(merged.map((c) => c.hostId), ['HOST-1', 'host-2']);
    });

    test('excludes the given host id case-insensitively', () {
      final merged = NearbyHostCatalog.merging([
        cand('host-1', 'Mac A'),
        cand('host-2', 'Mac B'),
      ], excludingHostId: 'HOST-1');
      expect(merged.map((c) => c.hostId), ['host-2']);
    });

    test('sorts by name case-insensitively then host id', () {
      final merged = NearbyHostCatalog.merging([
        cand('host-b', 'zebra'),
        cand('host-a', 'Apple'),
        cand('host-c', 'apple'),
      ]);
      expect(merged.map((c) => c.hostId), ['host-a', 'host-c', 'host-b']);
    });

    test('empty input yields empty output', () {
      expect(NearbyHostCatalog.merging([]), isEmpty);
    });
  });

  group('NearbyHostCandidate', () {
    test('equality is by value', () {
      expect(
        const NearbyHostCandidate(hostId: 'a', name: 'b'),
        const NearbyHostCandidate(hostId: 'a', name: 'b'),
      );
      expect(
        const NearbyHostCandidate(hostId: 'a', name: 'b'),
        isNot(const NearbyHostCandidate(hostId: 'a', name: 'c')),
      );
    });
  });
}
