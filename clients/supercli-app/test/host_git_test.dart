import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:test/test.dart';

import 'package:supercli_app/host_client.dart';
import 'package:supercli_app/host_models.dart';

HostClient clientWith(MockClient mock) => HostClient(
      baseUrl: Uri.parse('http://127.0.0.1:8137'),
      httpClient: mock,
    );

void main() {
  group('HostClient git', () {
    test('gitStatus parses branch, ahead/behind, and files', () async {
      final mock = MockClient((request) async {
        expect(request.url.path, '/mobile/git/status');
        expect(request.url.queryParameters['path'], '/repo');
        return http.Response(
          jsonEncode({
            'repoRoot': '/repo',
            'branch': 'supercli-next',
            'ahead': 2,
            'behind': 1,
            'files': [
              {
                'path': 'a.dart',
                'indexStatus': 'M',
                'worktreeStatus': ' ',
                'staged': true,
              },
              {
                'path': 'b.dart',
                'indexStatus': ' ',
                'worktreeStatus': 'M',
                'staged': false,
              },
            ],
          }),
          200,
        );
      });
      final client = clientWith(mock);
      final status = await client.gitStatus('/repo');
      expect(status.branch, 'supercli-next');
      expect(status.ahead, 2);
      expect(status.behind, 1);
      expect(status.files, hasLength(2));
      expect(status.files[0].path, 'a.dart');
      expect(status.files[0].staged, isTrue);
      expect(status.files[1].staged, isFalse);
      client.close();
    });

    test('gitStatus throws on non-200', () async {
      final mock = MockClient((_) async => http.Response('no repo', 404));
      final client = clientWith(mock);
      expect(() => client.gitStatus('/nope'), throwsA(isA<HostException>()));
      client.close();
    });

    test('gitDiff returns the diff text', () async {
      final mock = MockClient((request) async {
        expect(request.url.path, '/mobile/git/diff');
        expect(request.url.queryParameters['file'], 'a.dart');
        return http.Response(jsonEncode({'file': 'a.dart', 'diff': '@@ -1 +1 @@\n-x\n+y\n'}), 200);
      });
      final client = clientWith(mock);
      final diff = await client.gitDiff('/repo', 'a.dart');
      expect(diff, contains('+y'));
      client.close();
    });

    test('gitHistory parses commits', () async {
      final mock = MockClient((request) async {
        expect(request.url.path, '/mobile/git/history');
        return http.Response(
          jsonEncode({
            'commits': [
              {'sha': 'abc1234def', 'author': 'Amein', 'date': '2026-09-26', 'message': 'feat'},
            ],
          }),
          200,
        );
      });
      final client = clientWith(mock);
      final commits = await client.gitHistory('/repo');
      expect(commits, hasLength(1));
      expect(commits[0].shortSha, 'abc1234');
      expect(commits[0].message, 'feat');
      client.close();
    });

    test('gitStage posts path and files', () async {
      String? body;
      final mock = MockClient((request) async {
        expect(request.url.path, '/mobile/git/stage');
        body = request.body;
        return http.Response('{"ok":true}', 200);
      });
      final client = clientWith(mock);
      await client.gitStage('/repo', ['a.dart', 'b.dart']);
      final decoded = jsonDecode(body!) as Map<String, dynamic>;
      expect(decoded['path'], '/repo');
      expect(decoded['files'], ['a.dart', 'b.dart']);
      client.close();
    });

    test('gitCommit posts message, throws on Host error', () async {
      final mock = MockClient((_) async => http.Response('nothing to commit', 422));
      final client = clientWith(mock);
      expect(
        () => client.gitCommit('/repo', 'msg'),
        throwsA(isA<HostException>()),
      );
      client.close();
    });

    test('gitFetch/gitPull/gitPush hit the right routes', () async {
      final paths = <String>[];
      final mock = MockClient((request) async {
        paths.add(request.url.path);
        return http.Response('{"ok":true}', 200);
      });
      final client = clientWith(mock);
      await client.gitFetch('/repo');
      await client.gitPull('/repo');
      await client.gitPush('/repo');
      expect(paths, ['/mobile/git/fetch', '/mobile/git/pull', '/mobile/git/push']);
      client.close();
    });
  });

  group('HostClient files', () {
    test('filesList parses entries', () async {
      final mock = MockClient((request) async {
        expect(request.url.path, '/mobile/files/list');
        return http.Response(
          jsonEncode({
            'path': '/repo',
            'entries': [
              {'name': 'lib', 'isDir': true, 'size': 0},
              {'name': 'a.dart', 'isDir': false, 'size': 42},
            ],
          }),
          200,
        );
      });
      final client = clientWith(mock);
      final entries = await client.filesList('/repo');
      expect(entries, hasLength(2));
      expect(entries[0].isDir, isTrue);
      expect(entries[1].size, 42);
      client.close();
    });

    test('filesRead decodes base64 body', () async {
      final mock = MockClient((_) async => http.Response(
            jsonEncode({'dataBase64': base64Encode(utf8.encode('hello'))}),
            200,
          ));
      final client = clientWith(mock);
      expect(await client.filesRead('/repo/a.dart'), 'hello');
      client.close();
    });

    test('filesWrite posts base64 content', () async {
      String? body;
      final mock = MockClient((request) async {
        body = request.body;
        return http.Response('{"ok":true,"bytesWritten":5}', 200);
      });
      final client = clientWith(mock);
      await client.filesWrite('/repo/a.dart', 'hello');
      final decoded = jsonDecode(body!) as Map<String, dynamic>;
      expect(decoded['path'], '/repo/a.dart');
      expect(utf8.decode(base64Decode(decoded['contentBase64'] as String)), 'hello');
      client.close();
    });
  });

  group('HostClient usage', () {
    test('usageStats parses providers', () async {
      final mock = MockClient((request) async {
        expect(request.url.path, '/mobile/usage/stats');
        return http.Response(
          jsonEncode({
            'hostVersion': '0.9.0',
            'sessionsTotal': 3,
            'sessionsRunning': 1,
            'providers': [
              {'provider': 'claude', 'transcriptDir': '/h/.claude/projects', 'transcriptSessions': 7},
            ],
          }),
          200,
        );
      });
      final client = clientWith(mock);
      final stats = await client.usageStats();
      expect(stats.sessionsRunning, 1);
      expect(stats.providers, hasLength(1));
      expect(stats.providers[0].transcriptSessions, 7);
      client.close();
    });
  });

  group('host models', () {
    test('GitStatus tolerates missing fields', () async {
      final status = GitStatus.fromJson({});
      expect(status.branch, isNull);
      expect(status.files, isEmpty);
    });
  });
}
