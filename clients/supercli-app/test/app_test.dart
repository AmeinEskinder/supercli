import 'package:gpuidart/gpuidart.dart';
import 'package:test/test.dart';

import 'package:supercli_app/app.dart';
import 'package:supercli_app/models.dart';

void main() {
  group('SupercliApp UI tree', () {
    test('build returns a column with expected children', () {
      final app = SupercliApp();
      final root = app.build();
      expect(root, isA<UiColumn>());
      final column = root as UiColumn;
      // title, status, session-list table, no-approval text, composer
      expect(column.children, hasLength(5));
      expect((column.children[0] as UiText).text, 'supercli');
      expect(column.children[2], isA<UiTable>());
      expect(column.children[4], isA<UiInput>());
    });

    test('approval card appears when an approval is pending', () {
      final app = SupercliApp()
        ..pendingApproval = const PendingApproval(
          id: 'a1',
          tool: 'write_file',
          summary: 'Write /tmp/x',
          detail: '',
        );
      final column = app.build() as UiColumn;
      final card = column.children[3] as UiColumn;
      expect(card.id, 'approval-card');
      // title, tool, summary, button row (no detail since empty)
      expect(card.children, hasLength(4));
      final buttons = card.children[3] as UiRow;
      expect((buttons.children[0] as UiButton).label, contains('Approve'));
      expect((buttons.children[1] as UiButton).label, 'Deny');
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

      expect(byName['approval.approve']!.keys, 'ctrl+enter');
      expect(byName['approval.deny']!.keys, 'ctrl+shift+enter');
      expect(byName['sessions.up']!.keys, 'up');
      expect(byName['sessions.down']!.keys, 'down');
      expect(byName['composer.focus']!.keys, 'ctrl+l');
    });

    test('list navigation is scoped to the session-list node', () {
      final app = SupercliApp();
      final actions = app.actions();
      final up = actions.firstWhere((a) => a.name == 'sessions.up');
      final context = up.context;
      expect(context, isA<UiNodeActionContext>());
      expect((context as UiNodeActionContext).id, 'session-list');
    });

    test('approve is global (fires from anywhere)', () {
      final app = SupercliApp();
      final actions = app.actions();
      final approve = actions.firstWhere((a) => a.name == 'approval.approve');
      expect(approve.context, isA<UiGlobalActionContext>());
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
