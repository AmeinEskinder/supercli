/// Settings persistence controller: bridges [AppSettings], the Host, and
/// the local app config.
///
/// Loads the local snapshot first ([SettingsLocalStore]), then the workspace
/// settings from the Host (`GET /mobile/workspace-settings`), which override
/// only the Host-owned fields. Applies user edits to the in-memory model
/// immediately and persists them back with a debounce (`POST
/// /mobile/workspace-settings` followed by a local snapshot rewrite).
/// Failures are surfaced through [onError] so the app shell can show a
/// toast; the in-memory model is never rolled back on a failed save (the
/// user keeps editing, the next debounced save retries).
///
/// Port note: there is no Swift equivalent — the Swift app talks to the
/// Host over the native provider; this is the gpuidart/HTTP path.
library;

import 'dart:async';

import '../host_client.dart';
import 'settingspanels.dart';
import 'settings_local_store.dart';

/// Callback for settings errors (load or save). The app shell routes
/// these to the ToastCenter.
typedef SettingsErrorCallback = void Function(String message);

/// Owns an [AppSettings] and keeps it in sync with the Host.
///
/// Host-owned fields (see [AppSettings.toHostJson]) are loaded from and
/// saved to the Host. Desktop-only fields (theme, appearance, notifications,
/// advanced prefs) persist through [localStore] — the app's own config —
/// so they survive restarts and work before the Host is reachable.
///
/// Load order: the local snapshot is applied first, then the Host response
/// overrides only the Host-owned fields. Save order: the Host first, then
/// the local snapshot.
final class SettingsController {
  SettingsController({
    required this.host,
    this.debounce = const Duration(milliseconds: 500),
    this.onError,
    this.localStore,
    AppSettings? initial,
  }) : settings = initial ?? AppSettings();

  final HostClient host;

  /// How long to wait after the last edit before persisting.
  final Duration debounce;

  /// Called with a human-readable message when a load or save fails.
  final SettingsErrorCallback? onError;

  /// Local config persistence for desktop-only fields. Null disables it
  /// (Host-only mode, e.g. in tests that don't cover local persistence).
  final SettingsLocalStore? localStore;

  /// The live settings model. Mutate its fields, then call [edited].
  final AppSettings settings;

  Timer? _debounceTimer;
  bool _disposed = false;
  bool _loading = false;

  /// Whether a load from the Host is in flight.
  bool get isLoading => _loading;

  /// Load settings: the local snapshot first (so desktop-only fields and
  /// offline values are present), then the Host, which overrides only the
  /// Host-owned fields. A Host failure keeps the local values and reports
  /// through [onError].
  Future<void> load() async {
    _loading = true;
    try {
      final store = localStore;
      if (store != null) {
        final local = await store.load();
        if (local.isNotEmpty) settings.applyLocalJson(local);
      }
      final wire = await host.settingsGet();
      final loaded = AppSettings.fromHostJson(wire);
      _applyLoaded(loaded);
    } on HostException catch (e) {
      onError?.call('Could not load settings: ${e.message}');
    } on Exception catch (e) {
      // Transport failures (unreachable Host, TLS errors) surface here
      // rather than as HostException; the local snapshot still applies.
      onError?.call('Could not load settings: $e');
    } finally {
      _loading = false;
    }
  }

  /// Copy Host-managed fields from [loaded] into [settings], preserving
  /// desktop-only fields (theme and friends are local-only; the Host never
  /// overrides them).
  void _applyLoaded(AppSettings loaded) {
    settings.sessionsMcp = loaded.sessionsMcp;
    settings.browserMcp = loaded.browserMcp;
    settings.browserDefaultAccess = loaded.browserDefaultAccess;
    settings.writePolicy = loaded.writePolicy;
    settings.worktreeAccess = loaded.worktreeAccess;
    settings.autoAddBrowserScreenshots = loaded.autoAddBrowserScreenshots;
    settings.autoStopArchiveMinutes = loaded.autoStopArchiveMinutes;
    settings.sidebarStoppedLimit = loaded.sidebarStoppedLimit;
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
    } on Exception catch (e) {
      onError?.call('Could not save settings: $e');
    }
    // Persist locally even when the Host save failed: the in-memory model
    // is never rolled back, so the snapshot must not lose the user's edits.
    final store = localStore;
    if (store == null) return;
    try {
      await store.save(settings.toLocalJson());
    } catch (e) {
      // A local IO failure must not lose the in-memory model — report it
      // and keep going.
      onError?.call('Could not persist local settings: $e');
    }
  }

  void dispose() {
    _disposed = true;
    _debounceTimer?.cancel();
  }
}
