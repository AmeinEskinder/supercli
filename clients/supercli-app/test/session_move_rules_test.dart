/// Port of `SessionMoveRulesTests.swift`
/// (`clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/`).
///
/// A Session's shell runs in one git checkout. Its sidebar row may be filed
/// only at its home (worktree or root) or in a plain group directly under
/// that home — never across a checkout boundary — and the drag, the
/// "Move to ▸" menu, and the project-sidebar pin all read this one rule.
library;

import 'package:supercli_app/screens/session_move_rules.dart';
import 'package:test/test.dart';

MoveRuleProject project(
  String id, {
  String? parent,
  String? branch,
  bool group = false,
  int order = 0,
}) =>
    MoveRuleProject(
      id: id,
      parentProjectId: parent,
      worktreeBranch: branch,
      isFolder: group,
      sortOrder: order,
    );

Map<String, MoveRuleProject> get catalog => {
      'root': project('root'),
      'group': project('group', parent: 'root', group: true, order: 1),
      'wt': project('wt', parent: 'root', branch: 'feature', order: 2),
      'wt-group': project('wt-group', parent: 'wt', group: true),
      'other': project('other'),
    };

void main() {
  group('SessionMoveRules (SessionMoveRules.swift)', () {
    test('home is the worktree or the root', () {
      home(String id) => SessionMoveRules.homeProjectId(
          forProjectId: id, projectsById: catalog);
      expect(home('wt'), 'wt');
      expect(home('wt-group'), 'wt');
      expect(home('group'), 'root');
      expect(home('root'), 'root');
      expect(
          SessionMoveRules.isWorktreeBound(
              sessionProjectId: 'wt-group', projectsById: catalog),
          isTrue);
      expect(
          SessionMoveRules.isWorktreeBound(
              sessionProjectId: 'group', projectsById: catalog),
          isFalse);
    });

    test('worktree session files only inside its worktree', () {
      can(String target, {String effective = 'wt'}) =>
          SessionMoveRules.canFile(
            sessionProjectId: 'wt',
            effectiveProjectId: effective,
            targetId: target,
            projectsById: catalog,
          );
      expect(can('wt-group'), isTrue,
          reason: 'a group inside the worktree is a valid target');
      expect(can('wt', effective: 'wt-group'), isTrue,
          reason: 'back to the worktree itself');
      expect(can('wt'), isFalse, reason: 'already there');
      expect(can('root'), isFalse,
          reason: 'the parent project is another checkout');
      expect(can('group'), isFalse,
          reason: 'a group under the parent is another checkout');
      expect(can('other'), isFalse);
      expect(can('missing'), isFalse);
    });

    test('root session never enters a worktree', () {
      can(String target) => SessionMoveRules.canFile(
            sessionProjectId: 'root',
            effectiveProjectId: 'root',
            targetId: target,
            projectsById: catalog,
          );
      expect(can('group'), isTrue);
      expect(can('wt'), isFalse);
      expect(can('wt-group'), isFalse);
      expect(can('other'), isFalse);
    });

    test('move menu offers exactly what the drag accepts', () {
      destinations(String session, String effective,
              {bool Function(String) hidden = _neverHidden}) =>
          SessionMoveRules.destinations(
            sessionProjectId: session,
            effectiveProjectId: effective,
            projectsById: catalog,
            isHiddenGroup: hidden,
          ).map((p) => p.id).toList();

      expect(destinations('wt', 'wt'), ['wt-group']);
      expect(destinations('wt', 'wt-group'), ['wt']);
      expect(destinations('root', 'root', hidden: (id) => id == 'group'), isEmpty,
          reason: 'the hidden storage group never surfaces');
    });

    test('crossing a checkout is refused while sibling reorder is not', () {
      crosses(String session, String hovered) =>
          SessionMoveRules.crossesCheckout(
            sessionProjectId: session,
            hoveredProjectId: hovered,
            projectsById: catalog,
          );
      // Reordering among siblings inside the worktree (and its groups).
      expect(crosses('wt', 'wt'), isFalse);
      expect(crosses('wt', 'wt-group'), isFalse);
      // Out of the worktree: parent, its group, the root list, another project.
      expect(crosses('wt', 'root'), isTrue);
      expect(crosses('wt', 'group'), isTrue);
      expect(crosses('wt', 'other'), isTrue);
      // A root Session over the worktree's rows is the same boundary.
      expect(crosses('root', 'wt'), isTrue);
      expect(crosses('group', 'wt-group'), isTrue);
      // Two ordinary projects do not shake: that was never a filing target.
      expect(crosses('root', 'other'), isFalse);
    });

    test('unknown project id is not a filing target', () {
      expect(
          SessionMoveRules.canFile(
            sessionProjectId: 'missing',
            effectiveProjectId: 'missing',
            targetId: 'root',
            projectsById: catalog,
          ),
          isFalse);
    });
  });
}

bool _neverHidden(String _) => false;
