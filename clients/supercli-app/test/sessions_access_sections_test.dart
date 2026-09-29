/// Tests for sessions_access_sections.dart
import 'package:test/test.dart';
import '../lib/sessions_access_sections.dart';

void main() {
  group('SessionsAccessData.approvedPairs', () {
    test('flattens and sorts pairs', () {
      final approvals = {
        'session-b': ['session-a'],
        'session-a': ['session-b', 'session-c'],
      };
      final pairs = SessionsAccessData.approvedPairs(approvals);
      expect(pairs.map((p) => p.id), [
        'session-a→session-b',
        'session-a→session-c',
        'session-b→session-a',
      ]);
    });

    test('returns empty for empty map', () {
      expect(SessionsAccessData.approvedPairs({}), isEmpty);
    });
  });

  group('SessionsAccessData.approvedApps', () {
    test('flattens and sorts apps', () {
      final approvals = {
        'session-a': ['app-2', 'app-1'],
      };
      final apps = SessionsAccessData.approvedApps(approvals);
      expect(apps.map((a) => a.id), [
        'session-a→app-1',
        'session-a→app-2',
      ]);
      expect(apps[0].caller, 'session-a');
      expect(apps[0].appID, 'app-1');
    });
  });
}
