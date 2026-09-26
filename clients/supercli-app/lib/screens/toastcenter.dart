/// Toast notification center.
///
/// Port of `ToastCenter.swift`. Transient bottom-edge notifications.
library;

import 'package:gpuidart/gpuidart.dart';

final class Toast {
  const Toast({required this.id, required this.message});

  final String id;
  final String message;
}

final class ToastCenter {
  const ToastCenter({this.toasts = const []});

  final List<Toast> toasts;

  UiNode build() {
    return UiColumn('toast-center', [
      for (final t in toasts) UiText('toast-${t.id}', t.message),
    ]);
  }
}
