/// Settings persistence controller: bridges [AppSettings] and the Host.
///
/// Loads the workspace settings from the Host on startup
/// (`GET /mobile/workspace-settings`), applies user edits to the in-memory
/// model immediately, and persists them back with a debounce
/// (`POST /mobile/workspace-settings`). Failures are surfaced through
/// [onError] so the app shell can show a toast; the in-memory model is
/// never rolled back on a failed save (the user keeps editing, the next
/// debounced save retries).
///
/// Port note: there is no Swift equivalent — the Swift app talks to the
/// Host over the native provider; this is the gpuidart/HTTP path.
library;

import 'dart:async';

import '../host_client.dart';
import 'settingspanels.dart';

/// Callback for settings errors (load or save). The app shell routes
/// these to the ToastCenter.
typedef SettingsErrorCallback = void Function(String message);

/// Owns an [AppSettings] and keeps it in sync with the Host.
final class SettingsController {
  SettingsController({
    required this.host,
    this.debounce = const Duration(milliseconds: 500),
    this.onError,
    AppSettings? initial,
  }) : settings = initial ?? AppSettings();

  final HostClient host;

  /// How long to wait after the last edit before persisting.
  final Duration debounce;

  /// Called with a human-readable message when a load or save fails.
  final SettingsErrorCallback? onError;

  /// The live settings model. Mutate its fields, then call [edited].
  final AppSettings settings;

  Timer? _debounceTimer;
  bool _disposed = false;
  bool _loading = false;

  /// Whether a load from the Host is in flight.
  bool get isLoading => _loading;

  /// Load settings from the Host, replacing the in-memory model fields.
  /// Desktop-only fields (not in the Host wire format) keep their values.
  Future<void> load() async {
    _loading = true;
    try {
      final wire = await host.settingsGet();
      final loaded = AppSettings.fromHostJson(wire);
      _applyLoaded(loaded);
    } on HostException catch (e) {
      onError?.call('Could not load settings: ${e.message}');
    } finally {
      _loading = false;
    }
  }

  /// Copy Host-managed fields from [loaded] into [settings], preserving
  /// desktop-only fields.
  void _applyLoaded(AppSettings loaded) {
    settings.sessionsMcp = loaded.sessionsMcp;
    settings.browserMcp = loaded.browserMcp;
    settings.browserDefaultAccess = loaded.browserDefaultAccess;
    settings.writePolicy = loaded.writePolicy;
    settings.worktreeAccess = loaded.worktreeAccess;
    settings.autoAddBrowserScreenshots = loaded.autoAddBrowserScreenshots;
    settings.autoStopArchiveMinutes = loaded.autoStopArchiveMinutes;
    settings.sidebarStoppedLimit = loaded.sidebarStoppedLimit;
    settings.theme = loaded.theme;
  }

  /// Call after mutating [settings]. Schedules a debounced save.
  void edited() {
    if (_disposed) return;
    _debounceTimer?.cancel();
    _debounceTimer = Timer(debounce, _save);
  }

  /// Persist immediately, bypassing the debounce. Used by tests and by
  /// explicit "save now" paths.
  Future<void> saveNow() async {
    _debounceTimer?.cancel();
    await _save();
  }

  Future<void> _save() async {
    if (_disposed) return;
    try {
      await host.settingsSet(settings.toHostJson());
    } on HostException catch (e) {
      onError?.call('Could not save settings: ${e.message}');
    }
  }

  void dispose() {
    _disposed = true;
    _debounceTimer?.cancel();
  }
}
