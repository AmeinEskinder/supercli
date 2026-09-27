/// Host management state: devices, endpoint, and error for the
/// device-management surface.
///
/// Port of `HostManagementState.swift` (the pure state half).
///
/// Settings observe this small model directly. Device polling never
/// invalidates the terminal/sidebar store, and a repeated response is a
/// publication no-op ([apply] ignores an unchanged value).
///
/// The Swift original is an `@MainActor ObservableObject` with `@Published`.
/// The Dart app drives rebuilds through its own state layer, so this is a
/// plain state holder: call [addListener] to be notified on change.
library;

/// A paired device summary for the host-management surface.
///
/// The Swift original uses `RemotePairedDeviceSummary` from SupercliShared;
/// the Dart app models the summary fields it needs here.
final class HostManagementDevice {
  const HostManagementDevice({required this.id, required this.name});

  /// Stable device identity.
  final String id;

  /// Human-readable device name.
  final String name;

  @override
  bool operator ==(Object other) =>
      other is HostManagementDevice && other.id == id && other.name == name;

  @override
  int get hashCode => Object.hash(id, name);

  @override
  String toString() => 'HostManagementDevice(id: $id, name: $name)';
}

/// The observable value: devices, endpoint, and error.
final class HostManagementValue {
  const HostManagementValue({
    this.devices = const [],
    this.endpoint,
    this.error,
  });

  final List<HostManagementDevice> devices;
  final Uri? endpoint;
  final String? error;

  HostManagementValue copyWith({
    List<HostManagementDevice>? devices,
    Uri? endpoint,
    bool clearEndpoint = false,
    String? error,
    bool clearError = false,
  }) => HostManagementValue(
    devices: devices ?? this.devices,
    endpoint: clearEndpoint ? null : (endpoint ?? this.endpoint),
    error: clearError ? null : (error ?? this.error),
  );

  static bool _devicesEqual(
    List<HostManagementDevice> a,
    List<HostManagementDevice> b,
  ) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  @override
  bool operator ==(Object other) =>
      other is HostManagementValue &&
      _devicesEqual(other.devices, devices) &&
      other.endpoint == endpoint &&
      other.error == error;

  @override
  int get hashCode => Object.hash(Object.hashAll(devices), endpoint, error);

  @override
  String toString() =>
      'HostManagementValue(devices: ${devices.length}, endpoint: $endpoint, error: $error)';
}

/// Small observable model for the host-management surface.
///
/// Mirrors `HostManagementState`: [apply] is a no-op when the new value
/// equals the current one, so repeated poll responses never invalidate
/// observers.
final class HostManagementState {
  HostManagementState([this._value = const HostManagementValue()]);

  HostManagementValue _value;
  final _listeners = <void Function()>[];

  HostManagementValue get value => _value;

  void addListener(void Function() listener) => _listeners.add(listener);

  void removeListener(void Function() listener) => _listeners.remove(listener);

  void _notify() {
    for (final listener in List.of(_listeners)) {
      listener();
    }
  }

  /// Publish [next], unless it equals the current value (no-op).
  void apply(HostManagementValue next) {
    if (next == _value) return;
    _value = next;
    _notify();
  }

  /// Update just the endpoint, preserving devices and error.
  void updateEndpoint(Uri? endpoint) {
    apply(_value.copyWith(endpoint: endpoint, clearEndpoint: endpoint == null));
  }

  /// Update just the error, preserving devices and endpoint.
  void updateError(String? error) {
    apply(_value.copyWith(error: error, clearError: error == null));
  }
}
