/// Git, files, and usage models for the supercli desktop app.
///
/// These mirror the Host's JSON wire format
/// (`crates/supercli-core/src/host_git.rs`). Keep in sync with the Rust side.
///
/// The wire DTO classes below mirror `crates/supercli-client/src/dto.rs`
/// (Swift `RemoteControlProtocol.swift`). They decode the canonical fixtures
/// in `protocol/fixtures/*.json`; see `test/wire_fixtures_test.dart`. Any
/// wire change must update the fixtures and keep both suites green.
///
/// NOTE: `SessionSummary` and `PendingApproval` already exist in
/// `models.dart` (older snake_case domain models) and are intentionally not
/// redefined here to avoid ambiguous imports; the fixture test decodes those
/// two fixtures through `models.dart`.
library;

/// One changed file in `git status --porcelain` output.
final class GitFileChange {
  const GitFileChange({
    required this.path,
    required this.indexStatus,
    required this.worktreeStatus,
    required this.staged,
  });

  final String path;
  final String indexStatus;
  final String worktreeStatus;
  final bool staged;

  factory GitFileChange.fromJson(Map<String, dynamic> json) {
    return GitFileChange(
      path: json['path'] as String,
      indexStatus: (json['indexStatus'] as String?) ?? ' ',
      worktreeStatus: (json['worktreeStatus'] as String?) ?? ' ',
      staged: (json['staged'] as bool?) ?? false,
    );
  }
}

/// `GET /mobile/git/status` result.
final class GitStatus {
  const GitStatus({
    required this.repoRoot,
    this.branch,
    this.ahead = 0,
    this.behind = 0,
    this.files = const [],
  });

  final String repoRoot;
  final String? branch;
  final int ahead;
  final int behind;
  final List<GitFileChange> files;

  factory GitStatus.fromJson(Map<String, dynamic> json) {
    final files = (json['files'] as List?) ?? const [];
    return GitStatus(
      repoRoot: (json['repoRoot'] as String?) ?? '',
      branch: json['branch'] as String?,
      ahead: (json['ahead'] as num?)?.toInt() ?? 0,
      behind: (json['behind'] as num?)?.toInt() ?? 0,
      files: files
          .map((f) => GitFileChange.fromJson(f as Map<String, dynamic>))
          .toList(),
    );
  }
}

/// One commit in `GET /mobile/git/history`.
final class GitHistoryCommit {
  const GitHistoryCommit({
    required this.sha,
    required this.author,
    required this.date,
    required this.message,
  });

  final String sha;
  final String author;
  final String date;
  final String message;

  String get shortSha => sha.length > 7 ? sha.substring(0, 7) : sha;

  factory GitHistoryCommit.fromJson(Map<String, dynamic> json) {
    return GitHistoryCommit(
      sha: (json['sha'] as String?) ?? '',
      author: (json['author'] as String?) ?? '',
      date: (json['date'] as String?) ?? '',
      message: (json['message'] as String?) ?? '',
    );
  }
}

/// One entry of `GET /mobile/files/list`.
final class HostFileEntry {
  const HostFileEntry({required this.name, required this.isDir, this.size = 0});

  final String name;
  final bool isDir;
  final int size;

  factory HostFileEntry.fromJson(Map<String, dynamic> json) {
    return HostFileEntry(
      name: json['name'] as String,
      isDir: (json['isDir'] as bool?) ?? false,
      size: (json['size'] as num?)?.toInt() ?? 0,
    );
  }
}

/// One provider row of `GET /mobile/usage/stats`.
final class UsageProvider {
  const UsageProvider({
    required this.provider,
    required this.transcriptDir,
    required this.transcriptSessions,
  });

  final String provider;
  final String transcriptDir;
  final int transcriptSessions;

  factory UsageProvider.fromJson(Map<String, dynamic> json) {
    return UsageProvider(
      provider: (json['provider'] as String?) ?? '',
      transcriptDir: (json['transcriptDir'] as String?) ?? '',
      transcriptSessions: (json['transcriptSessions'] as num?)?.toInt() ?? 0,
    );
  }
}

/// `GET /mobile/usage/stats` result.
final class UsageStats {
  const UsageStats({
    required this.hostVersion,
    required this.sessionsTotal,
    required this.sessionsRunning,
    this.providers = const [],
  });

  final String hostVersion;
  final int sessionsTotal;
  final int sessionsRunning;
  final List<UsageProvider> providers;

  factory UsageStats.fromJson(Map<String, dynamic> json) {
    final providers = (json['providers'] as List?) ?? const [];
    return UsageStats(
      hostVersion: (json['hostVersion'] as String?) ?? '',
      sessionsTotal: (json['sessionsTotal'] as num?)?.toInt() ?? 0,
      sessionsRunning: (json['sessionsRunning'] as num?)?.toInt() ?? 0,
      providers: providers
          .map((p) => UsageProvider.fromJson(p as Map<String, dynamic>))
          .toList(),
    );
  }
}

/// Per-session capability flags the Host advertises on each session summary.
/// Mirrors `SessionCapabilities` in `crates/supercli-client/src/dto.rs`.
final class SessionCapabilities {
  const SessionCapabilities({
    this.restart = false,
    this.resumeAgent = false,
    this.archive = false,
    this.notifyWhenDone = false,
  });

  final bool restart;
  final bool resumeAgent;
  final bool archive;
  final bool notifyWhenDone;

  factory SessionCapabilities.fromJson(Map<String, dynamic> json) {
    return SessionCapabilities(
      restart: (json['restart'] as bool?) ?? false,
      resumeAgent: (json['resumeAgent'] as bool?) ?? false,
      archive: (json['archive'] as bool?) ?? false,
      notifyWhenDone: (json['notifyWhenDone'] as bool?) ?? false,
    );
  }
}

/// A project/group from the Host's bootstrap. Mirrors `ProjectSummary`.
final class ProjectSummary {
  const ProjectSummary({
    required this.id,
    this.name = '',
    this.path = '',
    this.parentProjectID,
    this.sortOrder,
    this.isFolder,
    this.worktreeBranch,
  });

  final String id;
  final String name;
  final String path;
  final String? parentProjectID;
  final int? sortOrder;
  final bool? isFolder;
  final String? worktreeBranch;

  factory ProjectSummary.fromJson(Map<String, dynamic> json) {
    return ProjectSummary(
      id: json['id'] as String,
      name: (json['name'] as String?) ?? '',
      path: (json['path'] as String?) ?? '',
      parentProjectID: json['parentProjectID'] as String?,
      sortOrder: (json['sortOrder'] as num?)?.toInt(),
      // The Host's bootstrap names plain child groups `isGroup`; the wire
      // DTO accepts both spellings, canonical is `isFolder`.
      isFolder: (json['isFolder'] as bool?) ?? (json['isGroup'] as bool?),
      worktreeBranch: json['worktreeBranch'] as String?,
    );
  }
}

/// One content block inside a transcript entry. Mirrors `TranscriptBlock`.
final class TranscriptBlock {
  const TranscriptBlock({this.kind = '', this.text});

  final String kind;
  final String? text;

  factory TranscriptBlock.fromJson(Map<String, dynamic> json) {
    return TranscriptBlock(
      kind: (json['kind'] as String?) ?? '',
      text: json['text'] as String?,
    );
  }
}

/// One entry of a session transcript. Mirrors `TranscriptEntry`.
final class TranscriptEntry {
  const TranscriptEntry({
    required this.id,
    this.role = 'other',
    this.blocks = const [],
  });

  final String id;
  final String role;
  final List<TranscriptBlock> blocks;

  factory TranscriptEntry.fromJson(Map<String, dynamic> json) {
    final blocks = (json['blocks'] as List?) ?? const [];
    return TranscriptEntry(
      id: json['id'] as String,
      role: (json['role'] as String?) ?? 'other',
      blocks: blocks
          .map((b) => TranscriptBlock.fromJson(b as Map<String, dynamic>))
          .toList(),
    );
  }
}

/// A snapshot (or page) of a session transcript. Mirrors `TranscriptSnapshot`.
final class TranscriptSnapshot {
  const TranscriptSnapshot({this.entries = const [], this.hasMore = false});

  final List<TranscriptEntry> entries;
  final bool hasMore;

  factory TranscriptSnapshot.fromJson(Map<String, dynamic> json) {
    final entries = (json['entries'] as List?) ?? const [];
    return TranscriptSnapshot(
      entries: entries
          .map((e) => TranscriptEntry.fromJson(e as Map<String, dynamic>))
          .toList(),
      hasMore: (json['hasMore'] as bool?) ?? false,
    );
  }
}

/// A launch preset row. Mirrors `PresetSummary`.
final class PresetSummary {
  const PresetSummary({
    required this.id,
    this.label = '',
    this.command = '',
    this.pluginID,
    this.projectID,
    this.cliID,
    this.enabled = true,
    this.quickLaunch = false,
    this.isDefault = false,
    this.tintColorHex,
  });

  final String id;
  final String label;
  final String command;
  final String? pluginID;
  final String? projectID;
  final String? cliID;

  /// Missing on older Hosts means enabled.
  final bool enabled;
  final bool quickLaunch;
  final bool isDefault;
  final int? tintColorHex;

  factory PresetSummary.fromJson(Map<String, dynamic> json) {
    return PresetSummary(
      id: json['id'] as String,
      label: (json['label'] as String?) ?? '',
      command: (json['command'] as String?) ?? '',
      pluginID: json['pluginID'] as String?,
      projectID: json['projectID'] as String?,
      cliID: json['cliID'] as String?,
      enabled: (json['enabled'] as bool?) ?? true,
      quickLaunch: (json['quickLaunch'] as bool?) ?? false,
      isDefault: (json['isDefault'] as bool?) ?? false,
      tintColorHex: (json['tintColorHex'] as num?)?.toInt(),
    );
  }
}

/// The Host's bootstrap snapshot. Mirrors `BootstrapSnapshot`. Nested
/// collections decode as raw maps (the typed wire decoders for session /
/// project / preset / approval rows live alongside this class).
final class BootstrapSnapshot {
  const BootstrapSnapshot({
    this.protocolVersion = 0,
    this.hostProtocol,
    this.macID,
    this.macName,
    this.presets = const [],
    this.sessions = const [],
    this.projects = const [],
    this.pendingApprovals = const [],
    this.capturedAtUnixMs = 0,
    this.remoteServerPort,
    this.remoteServerCertificateFingerprint,
    this.proEntitled,
  });

  final int protocolVersion;
  final Map<String, dynamic>? hostProtocol;
  final String? macID;
  final String? macName;
  final List<Map<String, dynamic>> presets;
  final List<Map<String, dynamic>> sessions;
  final List<Map<String, dynamic>> projects;
  final List<Map<String, dynamic>> pendingApprovals;
  final int capturedAtUnixMs;
  final int? remoteServerPort;
  final String? remoteServerCertificateFingerprint;
  final bool? proEntitled;

  static List<Map<String, dynamic>> _mapList(dynamic v) {
    final list = (v as List?) ?? const [];
    return list.map((e) => e as Map<String, dynamic>).toList();
  }

  factory BootstrapSnapshot.fromJson(Map<String, dynamic> json) {
    return BootstrapSnapshot(
      protocolVersion: (json['protocolVersion'] as num?)?.toInt() ?? 0,
      hostProtocol: json['hostProtocol'] as Map<String, dynamic>?,
      macID: json['macID'] as String?,
      macName: json['macName'] as String?,
      presets: _mapList(json['presets']),
      sessions: _mapList(json['sessions']),
      projects: _mapList(json['projects']),
      pendingApprovals: _mapList(json['pendingApprovals']),
      capturedAtUnixMs: (json['capturedAtUnixMs'] as num?)?.toInt() ?? 0,
      remoteServerPort: (json['remoteServerPort'] as num?)?.toInt(),
      remoteServerCertificateFingerprint:
          json['remoteServerCertificateFingerprint'] as String?,
      proEntitled: json['proEntitled'] as bool?,
    );
  }
}

/// A device paired with the Host. Mirrors `PairedDeviceSummary`.
final class PairedDeviceSummary {
  const PairedDeviceSummary({
    required this.id,
    required this.name,
    required this.platform,
    this.appVersion,
    required this.pairedAtUnixMs,
    this.lastSeenAtUnixMs,
    this.relayAllowed,
  });

  final String id;
  final String name;
  final String platform;
  final String? appVersion;
  final int pairedAtUnixMs;
  final int? lastSeenAtUnixMs;

  /// Nil means allowed (pre-flag records) — the flag only ever narrows.
  final bool? relayAllowed;

  factory PairedDeviceSummary.fromJson(Map<String, dynamic> json) {
    return PairedDeviceSummary(
      id: json['id'] as String,
      name: json['name'] as String,
      platform: json['platform'] as String,
      appVersion: json['appVersion'] as String?,
      pairedAtUnixMs: (json['pairedAtUnixMs'] as num).toInt(),
      lastSeenAtUnixMs: (json['lastSeenAtUnixMs'] as num?)?.toInt(),
      relayAllowed: json['relayAllowed'] as bool?,
    );
  }
}

/// A workspace on the connected Host. Mirrors `WorkspaceSummary`.
final class WorkspaceSummary {
  const WorkspaceSummary({
    required this.id,
    required this.name,
    this.tintHue,
    required this.isCurrent,
    required this.isRunning,
    this.kind,
  });

  final String id;
  final String name;
  final double? tintHue;
  final bool isCurrent;
  final bool isRunning;

  /// "local" | "ssh" | "paired"; older Hosts omit it (nil = local).
  final String? kind;

  String get effectiveKind => kind ?? 'local';

  factory WorkspaceSummary.fromJson(Map<String, dynamic> json) {
    return WorkspaceSummary(
      id: json['id'] as String,
      name: json['name'] as String,
      tintHue: (json['tintHue'] as num?)?.toDouble(),
      isCurrent: (json['isCurrent'] as bool?) ?? false,
      isRunning: (json['isRunning'] as bool?) ?? false,
      kind: json['kind'] as String?,
    );
  }
}

/// Progress of a resumable artifact upload. Mirrors `ArtifactUploadProgress`.
final class ArtifactUploadProgress {
  const ArtifactUploadProgress({
    required this.uploadID,
    required this.sessionID,
    required this.fileName,
    this.mimeType,
    required this.totalBytes,
    required this.receivedBytes,
    required this.chunkSize,
    required this.nextOffset,
    required this.complete,
    this.artifactID,
    required this.updatedAtUnixMs,
  });

  final String uploadID;
  final String sessionID;
  final String fileName;
  final String? mimeType;
  final int totalBytes;
  final int receivedBytes;
  final int chunkSize;
  final int nextOffset;
  final bool complete;
  final String? artifactID;
  final int updatedAtUnixMs;

  factory ArtifactUploadProgress.fromJson(Map<String, dynamic> json) {
    return ArtifactUploadProgress(
      uploadID: json['uploadID'] as String,
      sessionID: json['sessionID'] as String,
      fileName: json['fileName'] as String,
      mimeType: json['mimeType'] as String?,
      totalBytes: (json['totalBytes'] as num).toInt(),
      receivedBytes: (json['receivedBytes'] as num).toInt(),
      chunkSize: (json['chunkSize'] as num).toInt(),
      nextOffset: (json['nextOffset'] as num).toInt(),
      complete: (json['complete'] as bool?) ?? false,
      artifactID: json['artifactID'] as String?,
      updatedAtUnixMs: (json['updatedAtUnixMs'] as num).toInt(),
    );
  }
}

/// Request to create a new session. Mirrors `CreateSessionRequest`.
final class CreateSessionRequest {
  const CreateSessionRequest({
    required this.projectID,
    this.presetID,
    this.command,
    this.worktreePath,
    this.worktreeBranch,
    this.initialText,
    this.initialTextSubmitMode = 'pasteAndSubmit',
  });

  final String projectID;
  final String? presetID;
  final String? command;
  final String? worktreePath;
  final String? worktreeBranch;
  final String? initialText;
  final String initialTextSubmitMode;

  factory CreateSessionRequest.fromJson(Map<String, dynamic> json) {
    return CreateSessionRequest(
      projectID: json['projectID'] as String,
      presetID: json['presetID'] as String?,
      command: json['command'] as String?,
      worktreePath: json['worktreePath'] as String?,
      worktreeBranch: json['worktreeBranch'] as String?,
      initialText: json['initialText'] as String?,
      initialTextSubmitMode:
          (json['initialTextSubmitMode'] as String?) ?? 'pasteAndSubmit',
    );
  }
}

/// Response to a session creation request. Mirrors `CreateSessionResponse`.
final class CreateSessionResponse {
  const CreateSessionResponse({
    required this.sessionID,
    this.capturedAtUnixMs,
    this.session,
  });

  final String sessionID;
  final int? capturedAtUnixMs;
  final Map<String, dynamic>? session;

  factory CreateSessionResponse.fromJson(Map<String, dynamic> json) {
    return CreateSessionResponse(
      sessionID: json['sessionID'] as String,
      capturedAtUnixMs: (json['capturedAtUnixMs'] as num?)?.toInt(),
      session: json['session'] as Map<String, dynamic>?,
    );
  }
}

/// Text input to a session. Mirrors `SessionTextInput`.
final class SessionTextInput {
  const SessionTextInput({
    required this.sessionID,
    required this.text,
    required this.submitMode,
  });

  final String sessionID;
  final String text;
  final String submitMode;

  factory SessionTextInput.fromJson(Map<String, dynamic> json) {
    return SessionTextInput(
      sessionID: json['sessionID'] as String,
      text: json['text'] as String,
      submitMode: json['submitMode'] as String,
    );
  }
}

/// A terminal write request. Mirrors `TerminalWriteRequest`.
final class TerminalWriteRequest {
  const TerminalWriteRequest({
    required this.sessionID,
    required this.data,
    this.idempotencyKey,
  });

  final String sessionID;

  /// Base64-encoded terminal input data.
  final String data;
  final String? idempotencyKey;

  factory TerminalWriteRequest.fromJson(Map<String, dynamic> json) {
    return TerminalWriteRequest(
      sessionID: json['sessionID'] as String,
      data: json['data'] as String,
      idempotencyKey: json['idempotencyKey'] as String?,
    );
  }
}

/// A terminal resize request. Mirrors `TerminalResizeRequest`.
final class TerminalResizeRequest {
  const TerminalResizeRequest({
    required this.sessionID,
    required this.cols,
    required this.rows,
  });

  final String sessionID;
  final int cols;
  final int rows;

  factory TerminalResizeRequest.fromJson(Map<String, dynamic> json) {
    return TerminalResizeRequest(
      sessionID: json['sessionID'] as String,
      cols: (json['cols'] as num).toInt(),
      rows: (json['rows'] as num).toInt(),
    );
  }
}

/// A terminal color. Mirrors `TerminalColor`.
final class TerminalColor {
  const TerminalColor({
    required this.kind,
    this.index,
    this.red,
    this.green,
    this.blue,
  });

  /// "defaultForeground" | "defaultBackground" | "ansi" | "rgb".
  final String kind;
  final int? index;
  final int? red;
  final int? green;
  final int? blue;

  factory TerminalColor.fromJson(Map<String, dynamic> json) {
    return TerminalColor(
      kind: json['kind'] as String,
      index: (json['index'] as num?)?.toInt(),
      red: (json['red'] as num?)?.toInt(),
      green: (json['green'] as num?)?.toInt(),
      blue: (json['blue'] as num?)?.toInt(),
    );
  }
}

/// Terminal text style. Mirrors `TerminalStyle`.
final class TerminalStyle {
  const TerminalStyle({
    this.bold = false,
    this.italic = false,
    this.underline = false,
    this.inverse = false,
    this.dim = false,
    this.strikethrough = false,
  });

  final bool bold;
  final bool italic;
  final bool underline;
  final bool inverse;
  final bool dim;
  final bool strikethrough;

  factory TerminalStyle.fromJson(Map<String, dynamic> json) {
    return TerminalStyle(
      bold: (json['bold'] as bool?) ?? false,
      italic: (json['italic'] as bool?) ?? false,
      underline: (json['underline'] as bool?) ?? false,
      inverse: (json['inverse'] as bool?) ?? false,
      dim: (json['dim'] as bool?) ?? false,
      strikethrough: (json['strikethrough'] as bool?) ?? false,
    );
  }
}

/// A single terminal cell. Mirrors `TerminalCell`.
final class TerminalCell {
  const TerminalCell({
    required this.text,
    this.foreground,
    this.background,
    required this.style,
  });

  final String text;
  final TerminalColor? foreground;
  final TerminalColor? background;
  final TerminalStyle style;

  factory TerminalCell.fromJson(Map<String, dynamic> json) {
    return TerminalCell(
      text: json['text'] as String,
      foreground: json['foreground'] == null
          ? null
          : TerminalColor.fromJson(json['foreground'] as Map<String, dynamic>),
      background: json['background'] == null
          ? null
          : TerminalColor.fromJson(json['background'] as Map<String, dynamic>),
      style: TerminalStyle.fromJson(
        (json['style'] as Map<String, dynamic>?) ?? const {},
      ),
    );
  }
}

/// A terminal cursor. Mirrors `TerminalCursor`.
final class TerminalCursor {
  const TerminalCursor({
    required this.row,
    required this.column,
    required this.shape,
    required this.visible,
  });

  final int row;
  final int column;

  /// "block" | "beam" | "underline" | "hidden".
  final String shape;
  final bool visible;

  factory TerminalCursor.fromJson(Map<String, dynamic> json) {
    return TerminalCursor(
      row: (json['row'] as num).toInt(),
      column: (json['column'] as num).toInt(),
      shape: json['shape'] as String,
      visible: (json['visible'] as bool?) ?? false,
    );
  }
}

/// A full viewport frame. Mirrors `ViewportFrame`.
final class ViewportFrame {
  const ViewportFrame({
    required this.sessionID,
    required this.sequence,
    required this.rows,
    required this.columns,
    required this.cells,
    this.cursor,
    required this.alternateScreen,
    required this.capturedAtUnixMs,
  });

  final String sessionID;
  final int sequence;
  final int rows;
  final int columns;
  final List<TerminalCell> cells;
  final TerminalCursor? cursor;
  final bool alternateScreen;
  final int capturedAtUnixMs;

  factory ViewportFrame.fromJson(Map<String, dynamic> json) {
    final cells = (json['cells'] as List?) ?? const [];
    return ViewportFrame(
      sessionID: json['sessionID'] as String,
      sequence: (json['sequence'] as num).toInt(),
      rows: (json['rows'] as num).toInt(),
      columns: (json['columns'] as num).toInt(),
      cells: cells
          .map((c) => TerminalCell.fromJson(c as Map<String, dynamic>))
          .toList(),
      cursor: json['cursor'] == null
          ? null
          : TerminalCursor.fromJson(json['cursor'] as Map<String, dynamic>),
      alternateScreen: (json['alternateScreen'] as bool?) ?? false,
      capturedAtUnixMs: (json['capturedAtUnixMs'] as num).toInt(),
    );
  }
}

/// A viewport subscription request. Mirrors `ViewportSubscription`.
final class ViewportSubscription {
  const ViewportSubscription({
    required this.sessionID,
    required this.rows,
    required this.columns,
  });

  final String sessionID;
  final int rows;
  final int columns;

  factory ViewportSubscription.fromJson(Map<String, dynamic> json) {
    return ViewportSubscription(
      sessionID: json['sessionID'] as String,
      rows: (json['rows'] as num).toInt(),
      columns: (json['columns'] as num).toInt(),
    );
  }
}

/// A run of cells with the same style. Mirrors `TerminalCellRun`.
final class TerminalCellRun {
  const TerminalCellRun({
    required this.row,
    required this.startColumn,
    required this.cells,
  });

  final int row;
  final int startColumn;
  final List<TerminalCell> cells;

  factory TerminalCellRun.fromJson(Map<String, dynamic> json) {
    final cells = (json['cells'] as List?) ?? const [];
    return TerminalCellRun(
      row: (json['row'] as num).toInt(),
      startColumn: (json['startColumn'] as num).toInt(),
      cells: cells
          .map((c) => TerminalCell.fromJson(c as Map<String, dynamic>))
          .toList(),
    );
  }
}

/// A viewport patch (incremental update). Mirrors `ViewportPatch`.
final class ViewportPatch {
  const ViewportPatch({
    required this.sessionID,
    required this.sequence,
    required this.baseSequence,
    required this.changedRuns,
    this.cursor,
    required this.capturedAtUnixMs,
  });

  final String sessionID;
  final int sequence;
  final int baseSequence;
  final List<TerminalCellRun> changedRuns;
  final TerminalCursor? cursor;
  final int capturedAtUnixMs;

  factory ViewportPatch.fromJson(Map<String, dynamic> json) {
    final runs = (json['changedRuns'] as List?) ?? const [];
    return ViewportPatch(
      sessionID: json['sessionID'] as String,
      sequence: (json['sequence'] as num).toInt(),
      baseSequence: (json['baseSequence'] as num).toInt(),
      changedRuns: runs
          .map((r) => TerminalCellRun.fromJson(r as Map<String, dynamic>))
          .toList(),
      cursor: json['cursor'] == null
          ? null
          : TerminalCursor.fromJson(json['cursor'] as Map<String, dynamic>),
      capturedAtUnixMs: (json['capturedAtUnixMs'] as num).toInt(),
    );
  }
}

/// A generic stream event envelope. Mirrors `StreamEvent`. The payload is
/// opaque bytes on the wire (base64 in JSON); kept as the base64 string here.
final class StreamEvent {
  const StreamEvent({
    required this.protocolVersion,
    required this.id,
    this.requestID,
    required this.kind,
    this.sessionID,
    this.payload,
    required this.createdAtUnixMs,
  });

  final int protocolVersion;
  final String id;
  final String? requestID;

  /// e.g. "viewportFrame", "transcriptSnapshot", "heartbeat".
  final String kind;
  final String? sessionID;
  final String? payload;
  final int createdAtUnixMs;

  factory StreamEvent.fromJson(Map<String, dynamic> json) {
    return StreamEvent(
      protocolVersion: (json['protocolVersion'] as num).toInt(),
      id: json['id'] as String,
      requestID: json['requestID'] as String?,
      kind: json['kind'] as String,
      sessionID: json['sessionID'] as String?,
      payload: json['payload'] as String?,
      createdAtUnixMs: (json['createdAtUnixMs'] as num).toInt(),
    );
  }
}

/// Answer a pending MCP approval prompt. Mirrors `ApprovalAnswerRequest`.
final class ApprovalAnswerRequest {
  const ApprovalAnswerRequest({required this.id, required this.approved});

  final String id;
  final bool approved;

  factory ApprovalAnswerRequest.fromJson(Map<String, dynamic> json) {
    return ApprovalAnswerRequest(
      id: json['id'] as String,
      approved: (json['approved'] as bool?) ?? false,
    );
  }
}

/// Tell the Host the client opened a session, clearing unread.
/// Mirrors `MarkReadRequest`.
final class MarkReadRequest {
  const MarkReadRequest({required this.sessionID});

  final String sessionID;

  factory MarkReadRequest.fromJson(Map<String, dynamic> json) {
    return MarkReadRequest(sessionID: json['sessionID'] as String);
  }
}

/// Request to perform a session action. Mirrors `SessionActionRequest`.
final class SessionActionRequest {
  const SessionActionRequest({required this.sessionID, required this.action});

  final String sessionID;

  /// "stop" | "restart" | "restart_agent" | "resume_agent" | "remove".
  final String action;

  factory SessionActionRequest.fromJson(Map<String, dynamic> json) {
    return SessionActionRequest(
      sessionID: json['sessionID'] as String,
      action: json['action'] as String,
    );
  }
}

/// Patch for session organization. Mirrors `SessionOrganizationPatch`.
final class SessionOrganizationPatch {
  const SessionOrganizationPatch({
    required this.sessionID,
    this.title,
    this.pinned,
    this.archived,
    this.notifyWhenDone,
    this.projectID,
  });

  final String sessionID;
  final String? title;
  final bool? pinned;
  final bool? archived;
  final bool? notifyWhenDone;
  final String? projectID;

  factory SessionOrganizationPatch.fromJson(Map<String, dynamic> json) {
    return SessionOrganizationPatch(
      sessionID: json['sessionID'] as String,
      title: json['title'] as String?,
      pinned: json['pinned'] as bool?,
      archived: json['archived'] as bool?,
      notifyWhenDone: json['notifyWhenDone'] as bool?,
      projectID: json['projectID'] as String?,
    );
  }
}

/// Request a screenshot of a session. Mirrors `ScreenshotRequest`.
final class ScreenshotRequest {
  const ScreenshotRequest({required this.sessionID});

  final String sessionID;

  factory ScreenshotRequest.fromJson(Map<String, dynamic> json) {
    return ScreenshotRequest(sessionID: json['sessionID'] as String);
  }
}

/// Response to a screenshot request. Mirrors `ScreenshotRequestResponse`.
final class ScreenshotRequestResponse {
  const ScreenshotRequestResponse({
    required this.accepted,
    required this.requestedAtUnixMs,
  });

  final bool accepted;
  final int requestedAtUnixMs;

  factory ScreenshotRequestResponse.fromJson(Map<String, dynamic> json) {
    return ScreenshotRequestResponse(
      accepted: (json['accepted'] as bool?) ?? false,
      requestedAtUnixMs: (json['requestedAtUnixMs'] as num).toInt(),
    );
  }
}

/// A plugin update. Mirrors `PluginUpdate`.
final class PluginUpdate {
  const PluginUpdate({
    required this.id,
    required this.state,
    this.installedVersion,
    this.latestVersion,
    required this.updateAvailable,
  });

  final String id;
  final String state;
  final String? installedVersion;
  final String? latestVersion;
  final bool updateAvailable;

  factory PluginUpdate.fromJson(Map<String, dynamic> json) {
    return PluginUpdate(
      id: json['id'] as String,
      state: json['state'] as String,
      installedVersion: json['installedVersion'] as String?,
      latestVersion: json['latestVersion'] as String?,
      updateAvailable: (json['updateAvailable'] as bool?) ?? false,
    );
  }
}

/// Plugin updates status. Mirrors `PluginUpdates`.
final class PluginUpdates {
  const PluginUpdates({required this.checking, this.items = const []});

  final bool checking;
  final List<PluginUpdate> items;

  factory PluginUpdates.fromJson(Map<String, dynamic> json) {
    final items = (json['items'] as List?) ?? const [];
    return PluginUpdates(
      checking: (json['checking'] as bool?) ?? false,
      items: items
          .map((i) => PluginUpdate.fromJson(i as Map<String, dynamic>))
          .toList(),
    );
  }
}

/// Request to restart a session. Mirrors `RestartSessionRequest`.
final class RestartSessionRequest {
  const RestartSessionRequest({required this.sessionID});

  final String sessionID;

  factory RestartSessionRequest.fromJson(Map<String, dynamic> json) {
    return RestartSessionRequest(sessionID: json['sessionID'] as String);
  }
}

/// Push token registration. Mirrors `PushTokenRegistration`.
final class PushTokenRegistration {
  const PushTokenRegistration({
    required this.token,
    required this.platform,
    this.appVersion,
  });

  final String token;
  final String platform;
  final String? appVersion;

  factory PushTokenRegistration.fromJson(Map<String, dynamic> json) {
    return PushTokenRegistration(
      token: json['token'] as String,
      platform: json['platform'] as String,
      appVersion: json['appVersion'] as String?,
    );
  }
}
