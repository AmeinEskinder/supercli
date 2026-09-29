/// Terminal path-drag map: short-lived map from visible grid rows to
/// Host-local paths, published by terminal apps for AppKit file-URL drags.
///
/// Port of `TerminalPathDragMap.swift` from
/// `native/SupercliNative/Sources/SupercliNative`.
///
/// A terminal app can publish a short-lived map from visible grid rows to
/// Host-local paths in its own hosted-session directory. The native terminal
/// combines that semantic map with its exact point→cell hit test to begin a
/// normal file-URL drag. This is presentation-only state: it never enters
/// Host manifests, remote protocol state, or durable app state.
///
/// Behaviours ported:
/// - `Row` JSON codec → [TerminalPathDragRow.fromJson]
/// - `path(atScreenRow:column:nowMilliseconds:)` →
///   [TerminalPathDragMap.pathAt]
/// - JSON wire contract (snake_case keys) → [TerminalPathDragMap.fromJson]
/// - `maximumBytes` / `maximumAgeMilliseconds` /
///   `maximumFutureSkewMilliseconds` constants
///
/// Not ported (platform I/O, not portable logic):
/// - `load(from:)` — reads from the session directory via FileManager
/// - `nowMilliseconds` — wall-clock source
library;

/// One mapped row: the columns `[startColumn, endColumn)` show `path`.
final class TerminalPathDragRow {
  const TerminalPathDragRow({
    required this.screenRow,
    required this.startColumn,
    required this.endColumn,
    required this.path,
  });

  factory TerminalPathDragRow.fromJson(Map<String, dynamic> json) {
    return TerminalPathDragRow(
      screenRow: json['screen_row'] as int,
      startColumn: json['start_column'] as int,
      endColumn: json['end_column'] as int,
      path: json['path'] as String,
    );
  }

  final int screenRow;
  final int startColumn;
  final int endColumn;
  final String path;

  @override
  bool operator ==(Object other) =>
      other is TerminalPathDragRow &&
      screenRow == other.screenRow &&
      startColumn == other.startColumn &&
      endColumn == other.endColumn &&
      path == other.path;

  @override
  int get hashCode => Object.hash(screenRow, startColumn, endColumn, path);
}

/// Short-lived map from terminal grid rows to Host-local paths.
final class TerminalPathDragMap {
  const TerminalPathDragMap({
    required this.version,
    required this.processID,
    required this.updatedAt,
    required this.rows,
  });

  factory TerminalPathDragMap.fromJson(Map<String, dynamic> json) {
    return TerminalPathDragMap(
      version: json['version'] as int,
      processID: json['pid'] as int,
      updatedAt: json['updated_at'] as int,
      rows: (json['rows'] as List)
          .map((r) => TerminalPathDragRow.fromJson(r as Map<String, dynamic>))
          .toList(),
    );
  }

  /// Name of the map file inside the session directory.
  static const String filename = 'terminal-drag-map.json';

  /// Maps larger than this are rejected (fail-closed).
  static const int maximumBytes = 64 * 1024;

  /// A map older than this (ms) is stale.
  static const int maximumAgeMilliseconds = 5000;

  /// A map from the future beyond this skew (ms) is rejected.
  static const int maximumFutureSkewMilliseconds = 5000;

  final int version;
  final int processID;
  final int updatedAt;
  final List<TerminalPathDragRow> rows;

  /// Resolves the Host-local path for the given cell, or null when the map
  /// is stale, malformed, or the cell is unmapped.
  ///
  /// Mirrors `TerminalPathDragMap.path(atScreenRow:column:nowMilliseconds:)`:
  /// version must be 1, pid positive, timestamp within
  /// `[updatedAt - futureSkew, updatedAt + maxAge]`; the first row whose
  /// `screenRow` matches and whose `[startColumn, endColumn)` covers the
  /// column wins; relative paths fail closed (only absolute paths resolve).
  String? pathAt({
    required int row,
    required int column,
    required int nowMilliseconds,
  }) {
    if (version != 1) return null;
    if (processID <= 0) return null;
    if (updatedAt > nowMilliseconds + maximumFutureSkewMilliseconds) return null;
    if (nowMilliseconds > updatedAt + maximumAgeMilliseconds) return null;
    for (final r in rows) {
      if (r.screenRow == row &&
          column >= r.startColumn &&
          column < r.endColumn &&
          _isAbsolutePath(r.path)) {
        return r.path;
      }
    }
    return null;
  }

  /// Mirrors `(match.path as NSString).isAbsolutePath`: a path is absolute
  /// iff it starts with `/`.
  static bool _isAbsolutePath(String path) => path.startsWith('/');
}
