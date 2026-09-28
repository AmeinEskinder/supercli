/// app-kit widget. Re-exported from appkit_widgets.dart.
library;

export 'appkit_widgets.dart' show ListNavigation;

/// Index-based keyboard navigation for vertical lists (command palette,
/// MRU switcher, menus).
///
/// Wraps the index by default (moving down past the last item lands on the
/// first); set [wrap] false to clamp instead. All mutating methods are
/// no-ops on an empty list.
final class KeyboardListNavigator {
  KeyboardListNavigator({this.length = 0, this.index = 0, this.wrap = true});

  int length;
  int index;
  bool wrap;

  bool get isEmpty => length == 0;

  void setLength(int n) {
    length = n < 0 ? 0 : n;
    _clamp();
  }

  void reset() => index = 0;

  /// Returns false when the list is empty.
  bool moveDown() {
    if (isEmpty) return false;
    index = wrap ? (index + 1) % length : (index + 1).clamp(0, length - 1);
    return true;
  }

  /// Returns false when the list is empty.
  bool moveUp() {
    if (isEmpty) return false;
    index = wrap
        ? (index - 1 + length) % length
        : (index - 1).clamp(0, length - 1);
    return true;
  }

  void _clamp() {
    if (length == 0) {
      index = 0;
    } else {
      index = index.clamp(0, length - 1);
    }
  }
}

/// Platform-neutral keys understood by the shared focused-row decision table.
/// Port of `UIListNavigationKey` (ListNavigation.swift).
enum UIListNavigationKey {
  down,
  up,
  first,
  last,
  pageDown,
  pageUp,
  enter,
  space,
  back,
}

/// Primary role of a list item, driving role-aware navigation decisions.
/// Port of `UIListItemPrimaryRole` (UIProtocol.swift).
enum UIListItemPrimaryRole {
  /// `static` is a reserved word in Dart.
  static_,
  toggle,
  checkmark,
  disclosure,
  command,
  destructive,
}

/// Navigation decisions produced by [uiListNavigationDecision].
/// Port of `UIListNavigationDecision` (ListNavigation.swift).
enum UIListNavigationDecision {
  down,
  up,
  first,
  last,
  pageDown,
  pageUp,
  invokePrimary,
  back,
}

/// One keyboard decision table shared by every native Page/List renderer.
/// Routing remains server-driven; `invokePrimary` only asks the caller to emit
/// the action declared by the current authoritative row.
///
/// Port of `uiListNavigationDecision(key:primaryRole:)` (ListNavigation.swift).
UIListNavigationDecision? uiListNavigationDecision({
  required UIListNavigationKey key,
  required UIListItemPrimaryRole primaryRole,
}) {
  switch (key) {
    case UIListNavigationKey.enter:
      return primaryRole == UIListItemPrimaryRole.static_
          ? null
          : UIListNavigationDecision.invokePrimary;
    case UIListNavigationKey.space:
      return primaryRole == UIListItemPrimaryRole.toggle
          ? UIListNavigationDecision.invokePrimary
          : UIListNavigationDecision.pageDown;
    case UIListNavigationKey.down:
      return UIListNavigationDecision.down;
    case UIListNavigationKey.up:
      return UIListNavigationDecision.up;
    case UIListNavigationKey.first:
      return UIListNavigationDecision.first;
    case UIListNavigationKey.last:
      return UIListNavigationDecision.last;
    case UIListNavigationKey.pageDown:
      return UIListNavigationDecision.pageDown;
    case UIListNavigationKey.pageUp:
      return UIListNavigationDecision.pageUp;
    case UIListNavigationKey.back:
      return UIListNavigationDecision.back;
  }
}
