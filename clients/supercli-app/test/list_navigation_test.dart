/// Tests for the role-aware list navigation decision table.
/// Port of the decision-table coverage implied by ListNavigation.swift;
/// every (key, role) combination is pinned.
library;

import 'package:supercli_app/widgets/list_navigation.dart';
import 'package:test/test.dart';

void main() {
  group('uiListNavigationDecision', () {
    test('enter invokes primary for every non-static role', () {
      for (final role in UIListItemPrimaryRole.values) {
        final decision = uiListNavigationDecision(
          key: UIListNavigationKey.enter,
          primaryRole: role,
        );
        if (role == UIListItemPrimaryRole.static_) {
          expect(decision, isNull,
              reason: 'enter on a static row must not invoke anything');
        } else {
          expect(decision, UIListNavigationDecision.invokePrimary,
              reason: 'enter on $role');
        }
      }
    });

    test('space toggles only toggle rows, pages down otherwise', () {
      for (final role in UIListItemPrimaryRole.values) {
        final decision = uiListNavigationDecision(
          key: UIListNavigationKey.space,
          primaryRole: role,
        );
        if (role == UIListItemPrimaryRole.toggle) {
          expect(decision, UIListNavigationDecision.invokePrimary);
        } else {
          expect(decision, UIListNavigationDecision.pageDown,
              reason: 'space on $role');
        }
      }
    });

    test('movement keys pass through regardless of role', () {
      const passthrough = {
        UIListNavigationKey.down: UIListNavigationDecision.down,
        UIListNavigationKey.up: UIListNavigationDecision.up,
        UIListNavigationKey.first: UIListNavigationDecision.first,
        UIListNavigationKey.last: UIListNavigationDecision.last,
        UIListNavigationKey.pageDown: UIListNavigationDecision.pageDown,
        UIListNavigationKey.pageUp: UIListNavigationDecision.pageUp,
        UIListNavigationKey.back: UIListNavigationDecision.back,
      };
      for (final role in UIListItemPrimaryRole.values) {
        for (final entry in passthrough.entries) {
          expect(
            uiListNavigationDecision(key: entry.key, primaryRole: role),
            entry.value,
            reason: '${entry.key} on $role',
          );
        }
      }
    });

    test('enter on command and destructive rows invokes primary', () {
      expect(
        uiListNavigationDecision(
          key: UIListNavigationKey.enter,
          primaryRole: UIListItemPrimaryRole.command,
        ),
        UIListNavigationDecision.invokePrimary,
      );
      expect(
        uiListNavigationDecision(
          key: UIListNavigationKey.enter,
          primaryRole: UIListItemPrimaryRole.destructive,
        ),
        UIListNavigationDecision.invokePrimary,
      );
    });

    test('space on checkmark and disclosure rows pages down', () {
      expect(
        uiListNavigationDecision(
          key: UIListNavigationKey.space,
          primaryRole: UIListItemPrimaryRole.checkmark,
        ),
        UIListNavigationDecision.pageDown,
      );
      expect(
        uiListNavigationDecision(
          key: UIListNavigationKey.space,
          primaryRole: UIListItemPrimaryRole.disclosure,
        ),
        UIListNavigationDecision.pageDown,
      );
    });
  });

  group('KeyboardListNavigator', () {
    test('wraps by default', () {
      final nav = KeyboardListNavigator(length: 3, index: 2);
      expect(nav.moveDown(), isTrue);
      expect(nav.index, 0);
      expect(nav.moveUp(), isTrue);
      expect(nav.index, 2);
    });

    test('clamps when wrap is false', () {
      final nav = KeyboardListNavigator(length: 3, index: 2, wrap: false);
      expect(nav.moveDown(), isTrue);
      expect(nav.index, 2);
      nav.reset();
      expect(nav.moveUp(), isTrue);
      expect(nav.index, 0);
    });

    test('empty list is a no-op', () {
      final nav = KeyboardListNavigator();
      expect(nav.isEmpty, isTrue);
      expect(nav.moveDown(), isFalse);
      expect(nav.moveUp(), isFalse);
      expect(nav.index, 0);
    });

    test('setLength clamps a stale index', () {
      final nav = KeyboardListNavigator(length: 5, index: 4);
      nav.setLength(2);
      expect(nav.index, 1);
      nav.setLength(0);
      expect(nav.index, 0);
    });
  });
}
