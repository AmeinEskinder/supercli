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
final class SupercliNativeBindings {
  static DynamicLibrary? _lib;

  static DynamicLibrary get lib {
    return _lib ??= _open();
  }

  /// Override the loaded library (tests).
  static set debugLibrary(DynamicLibrary? value) => _lib = value;

  static DynamicLibrary _open() {
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
