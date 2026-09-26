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

    test('settingsSet POSTs to /mobile/workspace-settings', () async {
      String? seenPath;
      Map<String, dynamic>? seenBody;
      final mock = MockClient((request) async {
        seenPath = request.url.path;
        expect(request.method, 'POST');
        seenBody = jsonDecode(request.body) as Map<String, dynamic>;
        return http.Response('{"ok":true}', 200);
      });
      final client = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: mock,
      );
      await client.settingsSet({
        'autoStopArchiveMinutes': 120,
        'browserDefaultAccess': 'ask',
      });
      expect(seenPath, '/mobile/workspace-settings');
      expect(seenBody?['autoStopArchiveMinutes'], 120);
      expect(seenBody?['browserDefaultAccess'], 'ask');
      client.close();
    });

    test('settingsSet throws HostException on validation error', () async {
      final mock = MockClient((request) async {
        return http.Response('{"error":"bad value"}', 400);
      });
      final client = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: mock,
      );
      expect(
        () => client.settingsSet({'autoStopArchiveMinutes': 45}),
        throwsA(isA<HostException>()),
      );
      client.close();
    });

    test('settingsGet GETs /mobile/workspace-settings and parses', () async {
      String? seenPath;
      String? seenMethod;
      final mock = MockClient((request) async {
        seenPath = request.url.path;
        seenMethod = request.method;
        return http.Response(
          jsonEncode({
            'autoStopArchiveMinutes': 240,
            'browserDefaultAccess': 'on',
            'experimentalSettings': {'sessionsMcp': false},
          }),
          200,
        );
      });
      final client = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: mock,
      );
      final settings = await client.settingsGet();
      expect(seenMethod, 'GET');
      expect(seenPath, '/mobile/workspace-settings');
      expect(settings['autoStopArchiveMinutes'], 240);
      expect(settings['browserDefaultAccess'], 'on');
      expect(
        (settings['experimentalSettings'] as Map)['sessionsMcp'],
        false,
      );
      client.close();
    });

    test('settingsGet throws HostException on non-200', () async {
      final mock = MockClient((request) async {
        return http.Response('nope', 503);
      });
      final client = HostClient(
        baseUrl: Uri.parse('http://127.0.0.1:8137'),
        httpClient: mock,
      );
      expect(() => client.settingsGet(), throwsA(isA<HostException>()));
      client.close();
    });
  });
}
