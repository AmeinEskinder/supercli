import 'package:gpuidart/gpuidart.dart';
import 'package:test/test.dart';

import 'package:supercli_app/app.dart';
import 'package:supercli_app/models.dart';

void main() {
  group('SupercliApp shell tree', () {
    test('build returns the app shell row: sidebar + content', () {
      final app = SupercliApp();
      final root = app.build();
      expect(root, isA<UiRow>());
      expect(root.id, 'app-shell');
      final row = root as UiRow;
      // sidebar (or collapsed placeholder) + content-area
      expect(row.children, hasLength(2));
      expect((row.children[1] as UiColumn).id, 'content-area');
    });

    test('approval panel appears when an approval is pending', () {
      final app = SupercliApp()
        ..pendingApprovals = const [
          PendingApproval(
            id: 'a1',
            tool: 'write_file',
            summary: 'Write /tmp/x',
            detail: '',
          ),
        ];
      final root = app.build() as UiRow;
      final content = root.children[1] as UiColumn;
      // terminal-area-root, mcp-approval-overlay, toast-center, composer
      expect(content.children, hasLength(4));
      expect(content.children[1].id, 'mcp-approval-overlay');
    });

    test('session dataset reflects sessions', () {
      final app = SupercliApp()
        ..sessions = [
          SessionSummary(
            id: 's1',
            title: 'Hello',
            updatedAt: DateTime.now(),
          ),
        ];
      final dataset = app.sessionDataset;
      expect(dataset.id, 'sessions');
      expect(dataset.columns, ['Title', 'Updated']);
    });
  });

  group('SupercliApp keyboard actions (UiAction API)', () {
    test('declares approve/deny/list/focus bindings', () {
      final app = SupercliApp();
      final actions = app.actions();
      final byName = {for (final a in actions) a.name: a};

      expect(byName['mcp.approve']!.keys, 'ctrl+enter');
      expect(byName['mcp.deny']!.keys, 'ctrl+shift+enter');
      expect(byName['sessions.up']!.keys, 'up');
      expect(byName['sessions.down']!.keys, 'down');
      expect(byName['composer.focus']!.keys, 'ctrl+l');
      expect(byName['sidebar.toggle']!.keys, 'cmd+b');
    });

    test('list navigation is scoped to the session-list node', () {
      final app = SupercliApp();
      final actions = app.actions();
      final up = actions.firstWhere((a) => a.name == 'sessions.up');
      final context = up.context;
      expect(context, isA<UiNodeActionContext>());
      expect((context as UiNodeActionContext).id, 'session-list');
    });

    test('all actions serialize to JSON without throwing', () {
      final app = SupercliApp();
      for (final action in app.actions()) {
        final json = action.toJson();
        expect(json['name'], isNotEmpty);
        expect(json['keys'], isNotEmpty);
      }
    });
  });
}
