import 'package:test/test.dart';

import 'package:supercli_app/models.dart';

void main() {
  group('SessionSummary', () {
    test('fromJson parses all fields', () {
      final s = SessionSummary.fromJson({
        'id': 'sess-1',
        'title': 'Hello',
        'updated_at': '2026-09-26T00:00:00Z',
        'unread_count': 3,
      });
      expect(s.id, 'sess-1');
      expect(s.title, 'Hello');
      expect(s.unreadCount, 3);
    });

    test('fromJson tolerates missing fields', () {
      final s = SessionSummary.fromJson({'id': 'sess-2'});
      expect(s.title, 'Untitled');
      expect(s.unreadCount, 0);
    });

    test('toJson round-trips', () {
      final s = SessionSummary(
        id: 'a',
        title: 'b',
        updatedAt: DateTime.utc(2026, 1, 1),
      );
      final back = SessionSummary.fromJson(s.toJson());
      expect(back.id, 'a');
      expect(back.title, 'b');
    });
  });

  group('PendingApproval', () {
    test('fromJson parses all fields', () {
      final a = PendingApproval.fromJson({
        'id': 'appr-1',
        'tool': 'write_file',
        'summary': 'Write /tmp/x',
        'detail': 'contents…',
        'generation': 7,
      });
      expect(a.id, 'appr-1');
      expect(a.tool, 'write_file');
      expect(a.generation, 7);
    });

    test('fromJson tolerates missing fields', () {
      final a = PendingApproval.fromJson({'id': 'appr-2'});
      expect(a.tool, 'unknown');
      expect(a.summary, '');
    });
  });

  group('ApprovalAnswer', () {
    test('approve serializes correctly', () {
      const ans = ApprovalAnswer.approve('x');
      expect(ans.toJson(), {'id': 'x', 'approved': true});
    });

    test('deny serializes correctly', () {
      const ans = ApprovalAnswer.deny('x');
      expect(ans.toJson(), {'id': 'x', 'approved': false});
    });
  });
}
