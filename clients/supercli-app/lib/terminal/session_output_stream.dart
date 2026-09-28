/// Authenticated Host output stream for one terminal pane.
///
/// Polls `GET /mobile/output` (the supercli-serve local gateway route) with
/// the paired-device Bearer token and feeds the returned PTY bytes through
/// [AnsiParser] into the pane's [TerminalState].
///
/// Output arrives over the authenticated Host API. The client NEVER reads
/// Host journal files from the local filesystem — that path bypasses auth
/// and breaks for remote Hosts and paired devices.
///
/// Wire format (`crates/supercli-serve/src/mobile.rs`, `handle_output`):
/// ```json
/// {
///   "sessionID": "<id>",
///   "offset": <u64>, "nextOffset": <u64>,
///   "dataBase64": "<raw PTY bytes>",
///   "truncated": <bool>, "capturedAtUnixMs": <u64>
/// }
/// ```
/// - `offset`/`nextOffset` track the byte position in the Host's output
///   journal. The stream resumes from `nextOffset`.
/// - `truncated: true` means the requested offset fell off the journal;
///   the client resets the VT and treats the chunk as a fresh baseline.
/// - Long-poll: `wait_ms` holds the request until new bytes arrive
///   (server-bounded at 25 s), so the loop is event-driven, not a busy poll.
/// - The route reports no session-exit flag; the stream runs until [stop].
///   Session end is learned from bootstrap / Host events.
///
/// Input and resize ride the matching authenticated POST routes
/// (`crates/supercli-core/src/controller_api.rs`), via [HostClient]:
/// - `POST /mobile/write` with `{"sessionID": id, "data": text, "wid": id}`
/// - `POST /mobile/resize` with `{"sessionID": id, "columns": N, "rows": N}`
library;

import 'dart:async';
import 'dart:convert';

import '../host_client.dart';
import 'ansi_parser.dart';
import 'terminal_state.dart';
import 'terminal_types.dart';

/// Streams one session's PTY output from the Host into a [TerminalState].
final class SessionOutputStream {
  SessionOutputStream({
    required this.client,
    required this.sessionId,
    required this.state,
    AnsiParser? parser,
    this.pollWait = const Duration(seconds: 25),
    this.maxBytes = 65536,
  }) : _parser = parser ?? AnsiParser();

  /// The authenticated Host client (single auth surface for output,
  /// input, and resize).
  final HostClient client;
  final String sessionId;
  final TerminalState state;

  /// Long-poll hold time per request (server clamps to its own max).
  final Duration pollWait;

  /// Max bytes per poll response (server clamps to its own max).
  final int maxBytes;

  final AnsiParser _parser;

  int? _offset;
  bool _running = false;

  /// True while the poll loop is active.
  bool get running => _running;

  /// Current journal offset (null until the first chunk arrives).
  int? get offset => _offset;

  /// Start the poll loop. Returns immediately; the loop runs until [stop].
  /// Errors are delivered to [onError]; the loop backs off and retries
  /// unless the error is fatal (401/404).
  Future<void> start({void Function(Object error)? onError}) async {
    if (_running) return;
    _running = true;
    var backoff = const Duration(milliseconds: 500);
    while (_running) {
      try {
        await _pollOnce();
        backoff = const Duration(milliseconds: 500);
      } catch (e) {
        if (e is _FatalStreamError) {
          _running = false;
          onError?.call(e);
          break;
        }
        onError?.call(e);
        await Future.delayed(backoff);
        backoff = Duration(
          milliseconds: (backoff.inMilliseconds * 2).clamp(500, 10000),
        );
      }
    }
    _running = false;
  }

  /// Stop the poll loop. In-flight requests are abandoned.
  void stop() {
    _running = false;
  }

  /// One poll round: fetch the next journal chunk and feed it to the parser.
  Future<void> _pollOnce() async {
    late final TerminalOutputChunk chunk;
    try {
      chunk = await client.terminalOutput(
        sessionId,
        offset: _offset,
        limit: maxBytes,
        wait: pollWait,
      );
    } on HostException catch (e) {
      if (e.statusCode == 401 || e.statusCode == 404) {
        throw _FatalStreamError('output poll failed: $e', e.statusCode ?? 0);
      }
      rethrow;
    }
    if (chunk.truncated) {
      // The journal rotated under us: reset the VT and treat this chunk
      // as a fresh baseline.
      _resetState();
    }
    if (chunk.data.isNotEmpty) {
      _feedBytes(chunk.data);
    }
    _offset = chunk.nextOffset;
  }

  void _feedBytes(List<int> bytes) {
    // The Host emits UTF-8 PTY bytes; decode lossily so one bad sequence
    // never kills the stream.
    _parser.parse(utf8.decode(bytes, allowMalformed: true), state);
  }

  void _resetState() {
    for (var r = 0; r < state.rows; r++) {
      state.updateCells([
        for (var c = 0; c < state.cols; c++)
          TerminalCellUpdate(r, c, const TerminalCell()),
      ]);
    }
    // A journal rebase is a fresh VT baseline: clear grid, home cursor.
    final cursor = state.cursor;
    state.setCursor(0, 0, cursor.style, cursor.visible);
    state.scrollback.clear();
  }

  /// Send raw input bytes to the session's PTY via `POST /mobile/write`.
  ///
  /// [writeId] is the idempotency key (`wid` on the wire); callers should
  /// pass a unique id per keypress batch so a retried POST never
  /// double-types.
  Future<void> sendInput(List<int> bytes, {String? writeId}) {
    // The Host's write route takes a JSON string; terminal input is text
    // (escape sequences are ASCII, printable input is UTF-8).
    return client.writeToSession(
      sessionId,
      utf8.decode(bytes, allowMalformed: true),
      writeId: writeId,
    );
  }

  /// Resize the session's PTY via `POST /mobile/resize`.
  Future<void> resize(int cols, int rows) =>
      client.resizeSession(sessionId, cols, rows);
}

final class _FatalStreamError extends StateError {
  _FatalStreamError(super.message, this.statusCode);
  final int statusCode;
}
