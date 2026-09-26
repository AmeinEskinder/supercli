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
