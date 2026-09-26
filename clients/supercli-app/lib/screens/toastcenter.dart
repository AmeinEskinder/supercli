/// Toast notification center.
///
/// Port of `ToastCenter.swift`. Transient bottom-edge notifications backed
/// by [NotificationQueue]: FIFO with a bound, auto-dismiss by TTL, manual
/// dismiss, and click-to-focus via the notification's [AppNotification.focusTarget].
///
/// Rendering uses UiColumn + UiRow + UiText primitives (RLE fallback
/// pattern). Each toast is a row: severity dot, title, message, and a
/// dismiss button. Clicking a toast fires the `toast.focus` action with
/// the toast id; the host focuses the notification's target.
///
/// GAP: No UiToast/UiBanner widget in gpuidart. Missing: slide-in
/// animation, progress-bar TTL indicator, stacked offset layout.
/// See docs/gpuidart-gaps-approvals.md.
library;

import 'package:gpuidart/gpuidart.dart';

import '../notifications.dart';

export '../notifications.dart' show AppNotification, NotificationQueue, NotificationSeverity;

/// A single toast row. Kept for API compatibility with the scaffold.
final class Toast {
  const Toast({required this.id, required this.message});

  final String id;
  final String message;

  AppNotification toNotification() => AppNotification(
        id: id,
        title: '',
        message: message,
      );
}

/// The toast center: renders the current [NotificationQueue] contents.
///
/// Severity dots: info `○`, warning `●`, error `●!`.
final class ToastCenter {
  const ToastCenter({
    this.toasts = const [],
    this.queue,
    this.onDismiss,
    this.onFocus,
  });

  /// Legacy simple toast list (scaffold API).
  final List<Toast> toasts;

  /// The live notification queue. When non-null, it drives rendering
  /// instead of [toasts].
  final NotificationQueue? queue;

  /// Called when the user dismisses a toast.
  final void Function(String id)? onDismiss;

  /// Called when the user clicks a toast (focus its target).
  final void Function(String id)? onFocus;

  /// Effective notifications to render.
  List<AppNotification> get _items {
    final q = queue;
    if (q != null) return q.items;
    return toasts.map((t) => t.toNotification()).toList();
  }

  UiNode build() {
    final items = _items;
    return UiColumn('toast-center', [
      for (final n in items)
        UiRow('toast-${n.id}', [
          UiText('toast-dot-${n.id}', _dotFor(n.severity)),
          UiColumn('toast-body-${n.id}', [
            if (n.title.isNotEmpty)
              UiText('toast-title-${n.id}', n.title),
            UiText('toast-message-${n.id}', n.message),
          ]),
          const UiButton('toast-dismiss', '✕'),
        ]),
    ]);
  }

  /// Handle a UI action by name. Returns true if consumed.
  bool handleAction(String actionName, String toastId) {
    switch (actionName) {
      case 'toast.dismiss':
        onDismiss?.call(toastId);
        queue?.dismiss(toastId);
        return true;
      case 'toast.focus':
        onFocus?.call(toastId);
        return true;
    }
    return false;
  }

  List<UiAction> actions() => const [
        UiAction(
          name: 'toast.dismiss',
          keys: 'escape',
          context: UiActionContext.node('toast-center'),
        ),
        UiAction(
          name: 'toast.focus',
          keys: 'enter',
          context: UiActionContext.node('toast-center'),
        ),
      ];

  static String _dotFor(NotificationSeverity severity) => switch (severity) {
        NotificationSeverity.info => '○',
        NotificationSeverity.warning => '●',
        NotificationSeverity.error => '●!',
      };
}
