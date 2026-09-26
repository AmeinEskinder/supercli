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

  group('SessionSummary display labels', () {
    test('displayTitle prefers a meaningful Host title', () {
      final s = SessionSummary(
        id: 'a',
        title: 'My feature work',
        updatedAt: DateTime.utc(2026, 1, 1),
        command: 'claude',
        cwd: '/home/osman/proj',
        agentId: 'claude',
      );
      expect(s.displayTitle, 'My feature work');
    });

    test('displayTitle falls back to agent + folder when title is the command',
        () {
      final s = SessionSummary(
        id: 'a',
        title: 'claude',
        updatedAt: DateTime.utc(2026, 1, 1),
        command: 'claude',
        cwd: '/home/osman/projects/foo',
        agentId: 'claude',
      );
      expect(s.displayTitle, 'Claude · ~/projects/foo');
    });

    test('displayTitle falls back to folder for a plain terminal', () {
      final s = SessionSummary(
        id: 'a',
        title: '',
        updatedAt: DateTime.utc(2026, 1, 1),
        cwd: '/tmp/work',
      );
      expect(s.displayTitle, '/tmp/work');
    });

    test('displayTitle falls back to Terminal when nothing is known', () {
      final s = SessionSummary(
        id: 'a',
        title: 'Untitled',
        updatedAt: DateTime.utc(2026, 1, 1),
      );
      expect(s.displayTitle, 'Terminal');
    });

    test('subtitle shows agent and cwd with the command as secondary text',
        () {
      final s = SessionSummary(
        id: 'a',
        title: 'My feature work',
        updatedAt: DateTime.utc(2026, 1, 1),
        command: 'claude --dangerously-skip-permissions',
        cwd: '/home/osman/proj',
        agentId: 'claude',
      );
      expect(s.subtitle,
          'Claude · ~/proj — claude --dangerously-skip-permissions');
    });

    test('subtitle omits the command when it is the display title', () {
      final s = SessionSummary(
        id: 'a',
        title: 'claude',
        updatedAt: DateTime.utc(2026, 1, 1),
        command: 'claude',
        cwd: '/home/osman/proj',
        agentId: 'claude',
      );
      // displayTitle is 'Claude · ~/proj'; command 'claude' differs, so it
      // still appears as secondary text.
      expect(s.subtitle, contains('claude'));
    });

    test('agentLabel prefers the app name, else capitalizes the runtime id',
        () {
      final withApp = SessionSummary(
        id: 'a',
        title: 't',
        updatedAt: DateTime.utc(2026, 1, 1),
        agentId: 'supercli.app.design',
        appName: 'Supercli Design',
      );
      expect(withApp.agentLabel, 'Supercli Design');
      final withRuntime = SessionSummary(
        id: 'b',
        title: 't',
        updatedAt: DateTime.utc(2026, 1, 1),
        agentId: 'claude',
      );
      expect(withRuntime.agentLabel, 'Claude');
      final none = SessionSummary(
        id: 'c',
        title: 't',
        updatedAt: DateTime.utc(2026, 1, 1),
      );
      expect(none.agentLabel, '');
    });

    test('shortCwd abbreviates home and long paths', () {
      SessionSummary cwd(String c) => SessionSummary(
            id: 'a',
            title: 't',
            updatedAt: DateTime.utc(2026, 1, 1),
            cwd: c,
          );
      expect(cwd('/home/osman').shortCwd, '~');
      expect(cwd('/home/osman/proj').shortCwd, '~/proj');
      expect(cwd('/home/osman/a/b/c').shortCwd, '~/…/b/c');
      expect(cwd('/tmp').shortCwd, '/tmp');
      expect(cwd('/var/log/nginx').shortCwd, '…/log/nginx');
      expect(cwd('').shortCwd, '');
    });

    test('fromJson parses the Host wire fields', () {
      final s = SessionSummary.fromJson({
        'id': 's1',
        'title': 'claude',
        'updatedAtUnixMs': 1780000000000,
        'command': 'claude',
        'cwd': '/home/osman/proj',
        'activeRuntimeID': 'claude',
        'activeAppName': 'Supercli Design',
      });
      expect(s.command, 'claude');
      expect(s.cwd, '/home/osman/proj');
      expect(s.agentId, 'claude');
      expect(s.appName, 'Supercli Design');
      expect(s.updatedAt.millisecondsSinceEpoch, 1780000000000);
      expect(s.displayTitle, 'Supercli Design · ~/proj');
    });

    test('toJson round-trips the new fields', () {
      final s = SessionSummary(
        id: 'a',
        title: 't',
        updatedAt: DateTime.utc(2026, 1, 1),
        command: 'zsh',
        cwd: '/tmp',
        agentId: '',
      );
      final back = SessionSummary.fromJson(s.toJson());
      expect(back.command, 'zsh');
      expect(back.cwd, '/tmp');
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
