/// Dart FFI bindings to the Supercli Rust client logic.
///
/// Per Amein's rule: one implementation, in Rust. This file contains ONLY
/// the FFI declarations and thin data marshaling — no decision logic.
/// All catalog data, preset decisions, pool policy, registry codecs, presence
/// parsing, and drop-map validation live in Rust (`supercli-client-ffi`).
///
/// The native library (`libsupercli_client_ffi.so` / `.dylib` / `.dll`)
/// is built from `crates/supercli-client-ffi`.
library;

import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';

/// Low-level C ABI bindings. Prefer [SupercliNative] for typed access.
///
/// The native library is located as follows:
/// 1. The `SUPERCLI_FFI_LIB` environment variable, when set (CI and dev).
/// 2. Platform default search paths (`libsupercli_client_ffi.so` / `.dylib`,
///    `supercli_client_ffi.dll`, or the iOS process image).
///
/// At load time the Dart side checks [abiVersion] against
/// [kExpectedAbiVersion] and throws on mismatch, so a stale library can
/// never silently serve wrong-shaped data.
///
/// ## Shipping the library
///
/// The app bundle must ship the cdylib built from
/// `crates/supercli-client-ffi` (`cargo build -p supercli-client-ffi
/// --release`):
/// - macOS: `libsupercli_client_ffi.dylib` inside the app bundle's
///   `Frameworks/` (gpuidart desktop shell).
/// - Linux: `libsupercli_client_ffi.so` next to the executable or on the
///   loader path.
/// - Windows: `supercli_client_ffi.dll` next to the executable.
/// - Android: `libsupercli_client_ffi.so` in the APK's `lib/<abi>/`.
/// - iOS: statically linked into the process image (`DynamicLibrary.process()`).
final class SupercliNativeBindings {
  /// ABI version this Dart code was written against. Must match
  /// `SUPERCLI_FFI_ABI_VERSION` in `supercli_client_ffi.h`.
  static const int kExpectedAbiVersion = 2;

  static DynamicLibrary? _lib;
  static bool _abiChecked = false;

  static DynamicLibrary get lib {
    final lib = _lib ??= _open();
    if (!_abiChecked) {
      _abiChecked = true;
      final version = _abiVersionOf(lib);
      if (version != kExpectedAbiVersion) {
        throw StateError(
          'supercli-client-ffi ABI mismatch: library reports $version, '
          'Dart expects $kExpectedAbiVersion. Rebuild the cdylib from '
          'crates/supercli-client-ffi.',
        );
      }
    }
    return lib;
  }

  static int _abiVersionOf(DynamicLibrary lib) {
    return lib
        .lookup<NativeFunction<Uint32 Function()>>(
          'supercli_ffi_abi_version',
        )
        .asFunction<int Function()>()();
  }

  /// ABI version reported by the loaded library (for diagnostics).
  static int get abiVersion => _abiVersionOf(lib);

  /// Last error recorded by the Rust library, or null when none.
  /// The returned string is freed after reading.
  static String? get lastError {
    final ptr = lib
        .lookup<NativeFunction<Pointer<Char> Function()>>(
          'supercli_last_error',
        )
        .asFunction<Pointer<Char> Function()>()();
    if (ptr == nullptr) return null;
    final s = ptr.cast<Utf8>().toDartString();
    _stringFreeOf(lib, ptr);
    return s.isEmpty ? null : s;
  }

  static void _stringFreeOf(DynamicLibrary lib, Pointer<Char> ptr) {
    lib
        .lookup<NativeFunction<Void Function(Pointer<Char>)>>(
          'supercli_string_free',
        )
        .asFunction<void Function(Pointer<Char>)>()(ptr);
  }

  /// Override the loaded library (tests).
  static set debugLibrary(DynamicLibrary? value) {
    _lib = value;
    _abiChecked = value != null;
  }

  static DynamicLibrary _open() {
    final envPath = Platform.environment['SUPERCLI_FFI_LIB'];
    if (envPath != null && envPath.isNotEmpty) {
      return DynamicLibrary.open(envPath);
    }
    if (Platform.isMacOS) {
      return DynamicLibrary.open('libsupercli_client_ffi.dylib');
    }
    if (Platform.isLinux) {
      return DynamicLibrary.open('libsupercli_client_ffi.so');
    }
    if (Platform.isWindows) {
      return DynamicLibrary.open('supercli_client_ffi.dll');
    }
    if (Platform.isAndroid) {
      return DynamicLibrary.open('libsupercli_client_ffi.so');
    }
    if (Platform.isIOS) {
      return DynamicLibrary.process();
    }
    throw UnsupportedError('supercli-client-ffi: unsupported platform');
  }

  late final _stringFree = lib
      .lookup<NativeFunction<Void Function(Pointer<Char>)>>(
        'supercli_string_free',
      )
      .asFunction<void Function(Pointer<Char>)>();

  String _takeString(Pointer<Char> ptr) {
    if (ptr == nullptr) return '';
    final s = ptr.cast<Utf8>().toDartString();
    _stringFree(ptr);
    return s;
  }

  String? _takeNullableString(Pointer<Char> ptr) {
    if (ptr == nullptr) return null;
    return _takeString(ptr);
  }

  late final _runtimeCatalogJson = lib
      .lookup<NativeFunction<Pointer<Char> Function()>>(
        'supercli_runtime_catalog_json',
      )
      .asFunction<Pointer<Char> Function()>();

  late final _runtimeByIdJson = lib
      .lookup<NativeFunction<Pointer<Char> Function(Pointer<Char>)>>(
        'supercli_runtime_by_id_json',
      )
      .asFunction<Pointer<Char> Function(Pointer<Char>)>();

  late final _runtimeDetectTool = lib
      .lookup<NativeFunction<Pointer<Char> Function(Pointer<Char>)>>(
        'supercli_runtime_detect_tool',
      )
      .asFunction<Pointer<Char> Function(Pointer<Char>)>();

  late final _presetToolIsQuickLaunchable = lib
      .lookup<NativeFunction<Uint8 Function(Pointer<Char>)>>(
        'supercli_preset_tool_is_quick_launchable',
      )
      .asFunction<int Function(Pointer<Char>)>();

  late final _presetToolDisplayName = lib
      .lookup<NativeFunction<Pointer<Char> Function(Pointer<Char>)>>(
        'supercli_preset_tool_display_name',
      )
      .asFunction<Pointer<Char> Function(Pointer<Char>)>();

  late final _presetSplitForMenuJson = lib
      .lookup<NativeFunction<Pointer<Char> Function(Pointer<Char>)>>(
        'supercli_preset_split_for_menu_json',
      )
      .asFunction<Pointer<Char> Function(Pointer<Char>)>();

  late final _quickPresetGroupsJson = lib
      .lookup<NativeFunction<Pointer<Char> Function(Pointer<Char>)>>(
        'supercli_quick_preset_groups_json',
      )
      .asFunction<Pointer<Char> Function(Pointer<Char>)>();

  late final _poolBackoffDelayMs = lib
      .lookup<NativeFunction<Uint64 Function(Uint32)>>(
        'supercli_pool_backoff_delay_ms',
      )
      .asFunction<int Function(int)>();

  late final _poolPolicyJson = lib
      .lookup<NativeFunction<Pointer<Char> Function()>>(
        'supercli_pool_policy_json',
      )
      .asFunction<Pointer<Char> Function()>();

  late final _registrySlugify = lib
      .lookup<NativeFunction<Pointer<Char> Function(Pointer<Char>)>>(
        'supercli_registry_slugify',
      )
      .asFunction<Pointer<Char> Function(Pointer<Char>)>();

  late final _presenceParse = lib
      .lookup<
        NativeFunction<Pointer<Char> Function(Pointer<Uint8>, UintPtr, Pointer<Char>)>
      >('supercli_presence_parse')
      .asFunction<Pointer<Char> Function(Pointer<Uint8>, int, Pointer<Char>)>();

  late final _presenceDisplayName = lib
      .lookup<NativeFunction<Pointer<Char> Function(Pointer<Char>, Pointer<Char>)>>(
        'supercli_presence_display_name',
      )
      .asFunction<Pointer<Char> Function(Pointer<Char>, Pointer<Char>)>();

  late final _dropMapAccepts = lib
      .lookup<
        NativeFunction<
          Uint8 Function(Pointer<Uint8>, UintPtr, Uint32, Uint32, Uint64)
        >
      >('supercli_drop_map_accepts')
      .asFunction<int Function(Pointer<Uint8>, int, int, int, int)>();

  late final _pathDragMapPathAt = lib
      .lookup<
        NativeFunction<
          Pointer<Char> Function(Pointer<Uint8>, UintPtr, Uint32, Uint32, Uint64)
        >
      >('supercli_path_drag_map_path_at')
      .asFunction<Pointer<Char> Function(Pointer<Uint8>, int, int, int, int)>();
}

/// Typed Dart API over the Rust client logic. No decision logic lives here.
final class SupercliNative {
  static final SupercliNativeBindings _b = SupercliNativeBindings();

  /// All runtime descriptors from the generated Rust catalog.
  static List<Map<String, dynamic>> runtimeCatalog() {
    final json = _b._takeString(_b._runtimeCatalogJson());
    final decoded = jsonDecode(json);
    if (decoded is! List) return const [];
    return decoded.whereType<Map<String, dynamic>>().toList();
  }

  /// Runtime descriptor for a stable id or legacy slug, or null.
  static Map<String, dynamic>? runtimeById(String id) {
    final idPtr = id.toNativeUtf8().cast<Char>();
    try {
      final json = _b._takeNullableString(_b._runtimeByIdJson(idPtr));
      if (json == null || json.isEmpty) return null;
      final decoded = jsonDecode(json);
      return decoded is Map<String, dynamic> ? decoded : null;
    } finally {
      malloc.free(idPtr);
    }
  }

  /// Legacy slug of the runtime that launches [command], or null.
  static String? runtimeDetectTool(String command) {
    final cmdPtr = command.toNativeUtf8().cast<Char>();
    try {
      final slug = _b._takeNullableString(_b._runtimeDetectTool(cmdPtr));
      return (slug == null || slug.isEmpty) ? null : slug;
    } finally {
      malloc.free(cmdPtr);
    }
  }

  /// Whether [command] resolves to a quick-launchable runtime.
  static bool presetToolIsQuickLaunchable(String command) {
    final cmdPtr = command.toNativeUtf8().cast<Char>();
    try {
      return _b._presetToolIsQuickLaunchable(cmdPtr) != 0;
    } finally {
      malloc.free(cmdPtr);
    }
  }

  /// Display name for a tool's legacy slug, or null when unknown.
  static String? presetToolDisplayName(String legacySlug) {
    final slugPtr = legacySlug.toNativeUtf8().cast<Char>();
    try {
      final name = _b._takeNullableString(_b._presetToolDisplayName(slugPtr));
      return (name == null || name.isEmpty) ? null : name;
    } finally {
      malloc.free(slugPtr);
    }
  }

  /// Splits presets into the Agents/Plugins sections of the new-session menu.
  ///
  /// Single source of truth: Rust `split_presets_for_new_session_menu`.
  /// [pluginCommands] carries the Host App-catalog plugin classification
  /// (a command is plugin-backed iff in this set). Returns
  /// `{'agents': [...], 'plugins': [...]}` (preset JSON objects, in order).
  static Map<String, List<Map<String, dynamic>>> presetSplitForMenu({
    required List<Map<String, dynamic>> presets,
    required Set<String> pluginCommands,
  }) {
    final inputPtr = jsonEncode({
      'presets': presets,
      'plugin_commands': pluginCommands.toList(),
    }).toNativeUtf8().cast<Char>();
    try {
      final json = _b._takeNullableString(
        _b._presetSplitForMenuJson(inputPtr),
      );
      if (json == null || json.isEmpty) {
        return const {
          'agents': <Map<String, dynamic>>[],
          'plugins': <Map<String, dynamic>>[],
        };
      }
      final decoded = jsonDecode(json);
      if (decoded is! Map<String, dynamic>) {
        return const {
          'agents': <Map<String, dynamic>>[],
          'plugins': <Map<String, dynamic>>[],
        };
      }
      List<Map<String, dynamic>> at(String key) =>
          (decoded[key] as List?)
              ?.whereType<Map<String, dynamic>>()
              .toList() ??
          [];
      return {'agents': at('agents'), 'plugins': at('plugins')};
    } finally {
      malloc.free(inputPtr);
    }
  }

  /// Groups quick-launch presets for the project-row quick-preset strip.
  ///
  /// Single source of truth: Rust `collect_quick_preset_groups`.
  /// [pluginCommands] and [appCatalog] carry the Host App-catalog
  /// classifications ([appCatalog] maps executable head ->
  /// `[app_id, app_name]`). Returns the groups in strip order.
  static List<Map<String, dynamic>> quickPresetGroups({
    required List<Map<String, dynamic>> presets,
    required Set<String> pluginCommands,
    required Map<String, List<String>> appCatalog,
  }) {
    final inputPtr = jsonEncode({
      'presets': presets,
      'plugin_commands': pluginCommands.toList(),
      'app_catalog': appCatalog,
    }).toNativeUtf8().cast<Char>();
    try {
      final json = _b._takeNullableString(_b._quickPresetGroupsJson(inputPtr));
      if (json == null || json.isEmpty) return const [];
      final decoded = jsonDecode(json);
      if (decoded is! List) return const [];
      return decoded.whereType<Map<String, dynamic>>().toList();
    } finally {
      malloc.free(inputPtr);
    }
  }

  /// Exponential backoff delay in ms for [consecutiveFailures] (>= 1).
  static int poolBackoffDelayMs(int consecutiveFailures) =>
      _b._poolBackoffDelayMs(consecutiveFailures);

  /// Pool policy tunables.
  static Map<String, dynamic> poolPolicy() {
    final json = _b._takeString(_b._poolPolicyJson());
    final decoded = jsonDecode(json);
    return decoded is Map<String, dynamic> ? decoded : const {};
  }

  /// URL-safe slug for a workspace display name.
  static String registrySlugify(String name) {
    final namePtr = name.toNativeUtf8().cast<Char>();
    try {
      return _b._takeString(_b._registrySlugify(namePtr));
    } finally {
      malloc.free(namePtr);
    }
  }

  /// Parse one presence feed; `{session_id: [{id, device_id, display_name,
  /// last_seen_ms}]}`.
  static Map<String, List<Map<String, dynamic>>> presenceParse(
    List<int> data,
    String source,
  ) {
    final dataPtr = malloc<Uint8>(data.length);
    final sourcePtr = source.toNativeUtf8().cast<Char>();
    try {
      dataPtr.asTypedList(data.length).setAll(0, data);
      final json = _b._takeString(
        _b._presenceParse(dataPtr, data.length, sourcePtr),
      );
      final decoded = jsonDecode(json);
      if (decoded is! Map<String, dynamic>) return const {};
      return decoded.map(
        (k, v) => MapEntry(
          k,
          (v as List).whereType<Map<String, dynamic>>().toList(),
        ),
      );
    } finally {
      malloc.free(dataPtr);
      malloc.free(sourcePtr);
    }
  }

  /// Display name for a `device` ("Name (id)") / `ip` pair.
  static String presenceDisplayName({String? device, String? ip}) {
    final devicePtr = (device ?? '').toNativeUtf8().cast<Char>();
    final ipPtr = (ip ?? '').toNativeUtf8().cast<Char>();
    try {
      return _b._takeString(_b._presenceDisplayName(devicePtr, ipPtr));
    } finally {
      malloc.free(devicePtr);
      malloc.free(ipPtr);
    }
  }

  /// Whether the drop-target map accepts a drop at (row, column) at nowMs.
  static bool dropMapAccepts(
    List<int> json,
    int row,
    int column,
    int nowMs,
  ) {
    final jsonPtr = malloc<Uint8>(json.length);
    try {
      jsonPtr.asTypedList(json.length).setAll(0, json);
      return _b._dropMapAccepts(jsonPtr, json.length, row, column, nowMs) != 0;
    } finally {
      malloc.free(jsonPtr);
    }
  }

  /// Host-local path for the path-drag map at (row, column) at nowMs.
  static String? pathDragMapPathAt(
    List<int> json,
    int row,
    int column,
    int nowMs,
  ) {
    final jsonPtr = malloc<Uint8>(json.length);
    try {
      jsonPtr.asTypedList(json.length).setAll(0, json);
      final path = _b._takeNullableString(
        _b._pathDragMapPathAt(jsonPtr, json.length, row, column, nowMs),
      );
      return (path == null || path.isEmpty) ? null : path;
    } finally {
      malloc.free(jsonPtr);
    }
  }
}
