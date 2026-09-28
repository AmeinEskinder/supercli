/// Terminal drop-target map: short-lived rectangles published by hosted
/// terminal apps that accept semantic file/folder drops.
///
/// Port of `TerminalDropTargetMap.swift` from
/// `native/SupercliNative/Sources/SupercliNative`.
///
/// Hosted Ratatui apps can publish short-lived terminal-cell rectangles that
/// accept semantic file/folder drops. The native Ghostty destination writes
/// hover and drop events back into that session directory, allowing editors
/// to move their own caret and scroll while a drag is in flight.
///
/// Behaviours ported:
/// - `Region.contains(row:column:)` → [TerminalDropTargetRegion.contains]
/// - `accepts(row:column:nowMilliseconds:)` → [TerminalDropTargetMap.accepts]
/// - JSON wire contract (snake_case keys) → [TerminalDropTargetMap.fromJson]
/// - `maximumBytes` / `maximumAgeMilliseconds` /
///   `maximumFutureSkewMilliseconds` constants
///
/// Not ported (platform I/O, not portable logic):
/// - `load(from:)` — reads from the session directory via FileManager
/// - `writeEvent(...)` — writes the drop event JSON to the session directory
/// - `nowMilliseconds` — wall-clock source
library;

/// A rectangular region of terminal cells that accepts drops.
final class TerminalDropTargetRegion {
  const TerminalDropTargetRegion({
    required this.screenRow,
    required this.startColumn,
    required this.endRow,
    required this.endColumn,
  });

  factory TerminalDropTargetRegion.fromJson(Map<String, dynamic> json) {
    return TerminalDropTargetRegion(
      screenRow: json['screen_row'] as int,
      startColumn: json['start_column'] as int,
      endRow: json['end_row'] as int,
      endColumn: json['end_column'] as int,
    );
  }

  final int screenRow;
  final int startColumn;
  final int endRow;
  final int endColumn;

  /// Half-open hit test: `[screenRow, endRow) × [startColumn, endColumn)`.
  ///
  /// Mirrors `TerminalDropTargetMap.Region.contains(row:column:)`.
  bool contains({required int row, required int column}) {
    return row >= screenRow &&
        row < endRow &&
        column >= startColumn &&
        column < endColumn;
  }

  @override
  bool operator ==(Object other) =>
      other is TerminalDropTargetRegion &&
      screenRow == other.screenRow &&
      startColumn == other.startColumn &&
      endRow == other.endRow &&
      endColumn == other.endColumn;

  @override
  int get hashCode => Object.hash(screenRow, startColumn, endRow, endColumn);
}

/// Short-lived map from terminal cells to drop targets.
final class TerminalDropTargetMap {
  const TerminalDropTargetMap({
    required this.version,
    required this.processID,
    required this.updatedAt,
    required this.regions,
  });

  factory TerminalDropTargetMap.fromJson(Map<String, dynamic> json) {
    return TerminalDropTargetMap(
      version: json['version'] as int,
      processID: json['pid'] as int,
      updatedAt: json['updated_at'] as int,
      regions: (json['regions'] as List)
          .map((r) => TerminalDropTargetRegion.fromJson(r as Map<String, dynamic>))
          .toList(),
    );
  }

  /// Name of the map file inside the session directory.
  static const String filename = 'terminal-drop-target-map.json';

  /// Name of the event file written back into the session directory.
  static const String eventFilename = 'terminal-drop-target-event.json';

  /// Maps larger than this are rejected (fail-closed).
  static const int maximumBytes = 64 * 1024;

  /// A map older than this (ms) is stale.
  static const int maximumAgeMilliseconds = 5000;

  /// A map from the future beyond this skew (ms) is rejected.
  static const int maximumFutureSkewMilliseconds = 5000;

  final int version;
  final int processID;
  final int updatedAt;
  final List<TerminalDropTargetRegion> regions;

  /// Whether the map is fresh, well-formed, and covers the given cell.
  ///
  /// Mirrors `TerminalDropTargetMap.accepts(row:column:nowMilliseconds:)`:
  /// version must be 1, pid positive, timestamp within
  /// `[updatedAt - futureSkew, updatedAt + maxAge]`, and some region must
  /// contain the cell.
  bool accepts({
    required int row,
    required int column,
    required int nowMilliseconds,
  }) {
    if (version != 1) return false;
    if (processID <= 0) return false;
    if (updatedAt > nowMilliseconds + maximumFutureSkewMilliseconds) return false;
    if (nowMilliseconds > updatedAt + maximumAgeMilliseconds) return false;
    return regions.any((r) => r.contains(row: row, column: column));
  }
}

/// A drop event written back into the session directory.
final class TerminalDropTargetEvent {
  const TerminalDropTargetEvent({
    required this.version,
    required this.eventID,
    required this.updatedAt,
    required this.kind,
    this.screenRow,
    this.column,
    this.text,
    this.references,
  });

  final int version;
  final String eventID;
  final int updatedAt;
  final TerminalDropTargetEventKind kind;
  final int? screenRow;
  final int? column;
  final String? text;
  final List<String>? references;

  Map<String, dynamic> toJson() => {
        'version': version,
        'event_id': eventID,
        'updated_at': updatedAt,
        'kind': kind.name,
        if (screenRow != null) 'screen_row': screenRow,
        if (column != null) 'column': column,
        if (text != null) 'text': text,
        if (references != null) 'references': references,
      };
}

enum TerminalDropTargetEventKind { hover, leave, drop }
