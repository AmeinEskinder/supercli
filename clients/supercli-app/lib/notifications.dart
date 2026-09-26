/// Notification model and queue for transient user notifications.
///
/// Backs the toast center ([ToastCenter]) and the approvals panel.
/// Notifications are transient: they auto-dismiss after [defaultTtl],
/// can be dismissed manually, and carry an optional focus target so a
/// click focuses the relevant session or panel.
///
/// This is the Dart-side model. Delivery (banners, sounds, Notification
/// Center) is a platform concern — see docs/gpuidart-gaps-approvals.md
/// for the missing gpuidart APIs.
library;

/// Severity of a notification. Drives the attention dot color in the UI.
enum NotificationSeverity {
  /// Informational (e.g. device connected).
  info,

  /// Needs attention (e.g. approval requested).
  warning,

  /// Error (e.g. connection lost).
  error,
}

/// A single transient notification.
final class AppNotification {
  const AppNotification({
    required this.id,
    required this.title,
    required this.message,
    this.severity = NotificationSeverity.info,
    this.createdAtUnixMs = 0,
    this.ttlMs = NotificationQueue.defaultTtlMs,
    this.focusTarget,
  });

  /// Unique id (used for dismissal).
  final String id;

  /// Short title shown in the toast.
  final String title;

  /// Body text.
  final String message;

  final NotificationSeverity severity;

  /// Epoch millis when the notification was created.
  final int createdAtUnixMs;

  /// Milliseconds before auto-dismiss. 0 = never auto-dismiss.
  final int ttlMs;

  /// Optional node id to focus when the toast is clicked
  /// (e.g. 'mcp-approval-overlay' for approval notifications).
  final String? focusTarget;

  /// True if this notification has expired relative to [nowUnixMs].
  bool isExpired(int nowUnixMs) =>
      ttlMs > 0 && nowUnixMs - createdAtUnixMs >= ttlMs;

  Map<String, Object?> toJson() => {
        'id': id,
        'title': title,
        'message': message,
        'severity': severity.name,
        'createdAtUnixMs': createdAtUnixMs,
        'ttlMs': ttlMs,
        if (focusTarget != null) 'focusTarget': focusTarget,
      };
}

/// FIFO queue of transient notifications with auto-dismiss.
///
/// The queue is bounded ([maxSize]); when full, the oldest info-level
/// notification is evicted first. Warning/error notifications are never
/// auto-evicted — they require explicit dismissal.
final class NotificationQueue {
  NotificationQueue({this.maxSize = 8});

  /// Default time-to-live for transient notifications (5 seconds).
  static const int defaultTtlMs = 5000;

  final int maxSize;
  final List<AppNotification> _items = [];

  /// Current notifications, oldest first.
  List<AppNotification> get items => List.unmodifiable(_items);

  int get length => _items.length;
  bool get isEmpty => _items.isEmpty;
  bool get isNotEmpty => _items.isNotEmpty;

  /// Add a notification, evicting the oldest info notification if full.
  void add(AppNotification notification) {
    // Replace any existing notification with the same id.
    _items.removeWhere((n) => n.id == notification.id);
    if (_items.length >= maxSize) {
      final evictable =
          _items.indexWhere((n) => n.severity == NotificationSeverity.info);
      if (evictable >= 0) {
        _items.removeAt(evictable);
      } else {
        // All warnings/errors: drop the oldest anyway to stay bounded.
        _items.removeAt(0);
      }
    }
    _items.add(notification);
  }

  /// Dismiss a notification by id. Returns true if one was removed.
  bool dismiss(String id) {
    final before = _items.length;
    _items.removeWhere((n) => n.id == id);
    return _items.length < before;
  }

  /// Remove all expired notifications relative to [nowUnixMs].
  /// Returns the number removed.
  int pruneExpired(int nowUnixMs) {
    final before = _items.length;
    _items.removeWhere((n) => n.isExpired(nowUnixMs));
    return before - _items.length;
  }

  /// Dismiss all notifications.
  void clear() => _items.clear();
}
