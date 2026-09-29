/// Terminal drop-target / path-drag maps: Dart wiring over the Rust FFI.
///
/// Hosted terminal apps publish two short-lived JSON files into their
/// session directory (`~/.supercli/app-sessions/<id>/`):
/// - `terminal-drop-target-map.json`: terminal-cell rectangles that accept
///   semantic file/folder drops (written by the app; see
///   `clients/legacy/app-kit/src/drop_target.rs`);
/// - `terminal-drag-map.json`: grid rows mapped to Host-local paths, so a
///   drag starting on a cell can begin a normal file-URL drag.
///
/// Ports of `TerminalDropTargetMap.swift` / `TerminalPathDragMap.swift`
/// (`clients/legacy/native/SupercliNative/Sources/SupercliNative/`).
///
/// Per Amein's rule the decision logic lives in Rust
/// (`crates/supercli-core/src/terminal_drop_maps.rs`, exposed through
/// `crates/supercli-client-ffi` as `supercli_drop_map_accepts` /
/// `supercli_path_drag_map_path_at`). This file only reads the session map
/// files and marshals bytes across the FFI — no hit-test or TTL logic.
///
/// File reads fail closed: a missing, oversize, or unreadable map behaves
/// like an empty one (no drop accepted, no drag path). A session directory
/// is only available for LOCAL hosts; remote/paired sessions leave the maps
/// disabled (null session directory).
library;

import 'dart:io';
import 'dart:typed_data';

import '../native_client.dart';

/// Session-directory reader + FFI marshaling for the two terminal maps.
abstract final class TerminalDropMaps {
  const TerminalDropMaps._();

  /// Name of the drop-target map file inside the session directory.
  /// Mirrors `TerminalDropTargetMap.filename` /
  /// `DropTargetMap::FILENAME`.
  static const String dropTargetMapFilename = 'terminal-drop-target-map.json';

  /// Name of the path-drag map file inside the session directory.
  /// Mirrors `TerminalPathDragMap.filename` / `PathDragMap::FILENAME`.
  static const String dragMapFilename = 'terminal-drag-map.json';

  /// Maps larger than this are rejected (fail-closed).
  /// Mirrors `TerminalDropTargetMap.maximumBytes` / `MAXIMUM_BYTES`.
  static const int maximumBytes = 64 * 1024;

  /// Session directory for a Host session id.
  static String sessionDir(String appSessionsDir, String sessionId) =>
      '$appSessionsDir/$sessionId';

  /// Whether the drop-target map accepts a drop at (row, column).
  ///
  /// Reads `terminal-drop-target-map.json` from [sessionDir] and asks the
  /// Rust FFI. Fails closed (false) when the file is missing, oversize,
  /// unreadable, or the native library is unavailable. [nowMs] is
  /// injectable for tests; defaults to the wall clock.
  static bool acceptsDropAt({
    required String sessionDir,
    required int row,
    required int column,
    int? nowMs,
  }) {
    final json = _readMapFile(sessionDir, dropTargetMapFilename);
    if (json == null) return false;
    try {
      return SupercliNative.dropMapAccepts(
        json,
        row,
        column,
        nowMs ?? DateTime.now().millisecondsSinceEpoch,
      );
    } catch (_) {
      return false;
    }
  }

  /// Host-local path for the path-drag map at (row, column), or null.
  ///
  /// Reads `terminal-drag-map.json` from [sessionDir] and asks the Rust
  /// FFI. Fails closed (null) when the file is missing, oversize,
  /// unreadable, unmapped, stale, or the native library is unavailable.
  /// [nowMs] is injectable for tests; defaults to the wall clock.
  static String? dragPathAt({
    required String sessionDir,
    required int row,
    required int column,
    int? nowMs,
  }) {
    final json = _readMapFile(sessionDir, dragMapFilename);
    if (json == null) return null;
    try {
      return SupercliNative.pathDragMapPathAt(
        json,
        row,
        column,
        nowMs ?? DateTime.now().millisecondsSinceEpoch,
      );
    } catch (_) {
      return null;
    }
  }

  /// Read one map file, or null when it must not be trusted.
  /// Mirrors `TerminalDropTargetMap.load(from:)`'s size gate.
  static Uint8List? _readMapFile(String sessionDir, String filename) {
    try {
      final file = File('$sessionDir/$filename');
      final length = file.lengthSync();
      if (length > maximumBytes) return null;
      return file.readAsBytesSync();
    } catch (_) {
      return null;
    }
  }
}
