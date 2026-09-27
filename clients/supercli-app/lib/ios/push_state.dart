/// Push-notification registration state machine.
///
/// Port of `PushRegistrationState` in `PushManager.swift`
/// (`clients/legacy/ios/SupercliIOS`). The `@MainActor` manager itself
/// (`UNUserNotificationCenter`, `UIApplication`) and the UIKit app delegate
/// are platform-bound and stay out — what ports is the pure state machine:
/// diagnostic labels, sidebar warnings, retry gating, and the APNs
/// device-token hex encoding.
library;

/// Push-notification registration state.
sealed class PushRegistrationState {
  const PushRegistrationState._();

  const factory PushRegistrationState.notRequested() = _NotRequested;
  const factory PushRegistrationState.requestingPermission() =
      _RequestingPermission;
  const factory PushRegistrationState.permissionDenied() = _PermissionDenied;
  const factory PushRegistrationState.registering() = _Registering;
  const factory PushRegistrationState.registered({
    required String environment,
  }) = _Registered;
  const factory PushRegistrationState.failed(String message) = _Failed;

  /// Human-readable diagnostic for the settings sheet.
  String get diagnosticLabel {
    final self = this;
    if (self is _NotRequested) return 'Not requested';
    if (self is _RequestingPermission) {
      return 'Waiting for notification permission…';
    }
    if (self is _PermissionDenied) {
      return 'Notifications are denied in iOS Settings';
    }
    if (self is _Registering) return 'Waiting for an APNs device token…';
    if (self is _Registered) {
      return self.environment == 'production'
          ? 'Ready (production)'
          : 'Ready (sandbox)';
    }
    if (self is _Failed) return 'Registration failed: ${self.message}';
    throw StateError('unreachable');
  }

  bool get permissionWasDenied => this is _PermissionDenied;

  /// Broken-delivery states worth surfacing outside the settings sheet
  /// (the sidebar warning). Transient startup states stay quiet so a
  /// healthy launch never flashes a warning while registration settles.
  String? get sidebarWarning {
    final self = this;
    if (self is _PermissionDenied) return 'Notifications are off';
    if (self is _Failed) return "Notifications aren't working";
    return null;
  }

  bool get canRetry {
    final self = this;
    return self is _NotRequested ||
        self is _PermissionDenied ||
        self is _Failed;
  }

  @override
  bool operator ==(Object other) {
    final self = this;
    if (self is _NotRequested) return other is _NotRequested;
    if (self is _RequestingPermission) return other is _RequestingPermission;
    if (self is _PermissionDenied) return other is _PermissionDenied;
    if (self is _Registering) return other is _Registering;
    if (self is _Registered) {
      return other is _Registered && other.environment == self.environment;
    }
    if (self is _Failed) {
      return other is _Failed && other.message == self.message;
    }
    return false;
  }

  @override
  int get hashCode {
    final self = this;
    if (self is _Registered) return Object.hash(4, self.environment);
    if (self is _Failed) return Object.hash(5, self.message);
    if (self is _NotRequested) return 0;
    if (self is _RequestingPermission) return 1;
    if (self is _PermissionDenied) return 2;
    return 3; // _Registering
  }
}

final class _NotRequested extends PushRegistrationState {
  const _NotRequested() : super._();
}

final class _RequestingPermission extends PushRegistrationState {
  const _RequestingPermission() : super._();
}

final class _PermissionDenied extends PushRegistrationState {
  const _PermissionDenied() : super._();
}

final class _Registering extends PushRegistrationState {
  const _Registering() : super._();
}

final class _Registered extends PushRegistrationState {
  const _Registered({required this.environment}) : super._();
  final String environment;
}

final class _Failed extends PushRegistrationState {
  const _Failed(this.message) : super._();
  final String message;
}

/// Hex-encodes an APNs device token (port of the `deviceToken.map` hex
/// formatting in `PushManager.didRegisterForRemoteNotifications`).
String hexEncodeDeviceToken(List<int> deviceToken) {
  final sb = StringBuffer();
  for (final byte in deviceToken) {
    sb.write(byte.toRadixString(16).padLeft(2, '0'));
  }
  return sb.toString();
}
