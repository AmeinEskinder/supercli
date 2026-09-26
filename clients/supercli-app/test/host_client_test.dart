import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:test/test.dart';

import 'package:supercli_app/host_client.dart';
import 'package:supercli_app/models.dart';

void main() {
  group('HostClient', () {
    test('listSessions parses the session list', () async {
      final mock = MockClient((request) async {
        expect(request.url.path, '/mobile/sessions');
        return http.Response(
          jsonEncode({
            'sessions': [
              {'id': 's1', 'title': 'First'},
              {'id': 's2', 'title': 'Second', 'unread_count': 2},
            ],
          }),
          200,
        );
      });
      final client = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: mock,
      );
      final sessions = await client.listSessions();
      expect(sessions, hasLength(2));
      expect(sessions[0].id, 's1');
      expect(sessions[1].unreadCount, 2);
      client.close();
    });

    test('listApprovals parses pending approvals', () async {
      final mock = MockClient((request) async {
        return http.Response(
          jsonEncode({
            'approvals': [
              {'id': 'a1', 'tool': 'write_file', 'summary': 'Write x'},
            ],
          }),
          200,
        );
      });
      final client = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: mock,
      );
      final approvals = await client.listApprovals();
      expect(approvals, hasLength(1));
      expect(approvals.first.tool, 'write_file');
      client.close();
    });

    test('answerApproval is idempotent client-side (no double send)', () async {
      var calls = 0;
      final mock = MockClient((request) async {
        calls++;
        return http.Response('{}', 200);
      });
      final client = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: mock,
      );
      final first = await client.answerApproval(const ApprovalAnswer.approve('a1'));
      final second = await client.answerApproval(const ApprovalAnswer.approve('a1'));
      expect(first, isTrue);
      expect(second, isFalse, reason: 'second answer must not be sent');
      expect(calls, 1, reason: 'exactly one HTTP POST');
      client.close();
    });

    test('answerApproval throws on transport error', () async {
      final mock = MockClient((request) async {
        return http.Response('boom', 500);
      });
      final client = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: mock,
      );
      expect(
        () => client.answerApproval(const ApprovalAnswer.deny('a9')),
        throwsA(isA<HostException>()),
      );
      client.close();
    });

    test('listSessions throws HostException on non-200', () async {
      final mock = MockClient((request) async {
        return http.Response('nope', 403);
      });
      final client = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: mock,
      );
      expect(() => client.listSessions(), throwsA(isA<HostException>()));
      client.close();
    });
  });
}
