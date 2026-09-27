/// Behaviour tests for the TerminalPaneView port.
///
/// Covers the new terminal modules, each mapped to the Swift behaviours
/// they port from `TerminalPaneView.swift` (native/SupercliNative,
/// 2,224 lines):
///
/// - `ansi_parser.dart` — VT sequence interpretation (the Host streams raw
///   PTY bytes; the parser reconstructs the grid with colors).
/// - `session_output_stream.dart` — authenticated `GET /mobile/output`
///   polling: offset tracking, truncated reset, input/resize via the
///   matching `POST /mobile/write` + `POST /mobile/resize` routes.
/// - `screens/terminalpaneview.dart` — `TerminalPaneView.hosted` binds a
///   pane to a live Host session through [HostClient].
/// - `terminal_pane_geometry.dart` — `TerminalPaneDropPreviewGeometry`
///   (groupEdgePaneExtent, insetHighlightExtent).
/// - `terminal_pane_close.dart` — `terminalPaneCloseAction`
///   (detachPane / removeSession / confirmArchive).
///
/// The wire format asserted here is the REAL supercli-serve local gateway
/// (`crates/supercli-serve/src/mobile.rs`, `handle_output`):
/// `{sessionID, offset, nextOffset, dataBase64, truncated, capturedAtUnixMs}`.
/// Input rides `POST /mobile/write` (`controller_api.rs`, `write_session`):
/// `{sessionID, data, wid}`. Resize rides `POST /mobile/resize`
/// (`resize_session`): `{sessionID, columns, rows}`.
library;

import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:test/test.dart';

import 'package:supercli_app/host_client.dart';
import 'package:supercli_app/screens/terminalpaneview.dart';
import 'package:supercli_app/terminal/ansi_parser.dart';
import 'package:supercli_app/terminal/terminal_pane_close.dart';
import 'package:supercli_app/terminal/terminal_pane_geometry.dart';
import 'package:supercli_app/terminal/terminal_types.dart';

TerminalState makeState({int cols = 20, int rows = 6}) =>
    TerminalState(cols: cols, rows: rows);

String rowText(TerminalState s, int row) =>
    s.grid[row].map((c) => c.char).join();

/// HostClient backed by a mock, with a Bearer token so the auth header is
/// asserted on every request.
HostClient mockHost(MockClient mock, {String? token}) => HostClient(
  baseUrl: Uri.parse('http://127.0.0.1:8137'),
  httpClient: mock,
  token: token ?? 't',
);

void main() {
  group('AnsiParser: printable and C0 controls', () {
    test('plain text lands at the cursor', () {
      final s = makeState();
      AnsiParser().parse('hello', s);
      expect(rowText(s, 0).startsWith('hello'), isTrue);
      expect(s.cursor.col, 5);
    });

    test('LF moves to next line start, scrolling at the bottom', () {
      final s = makeState(rows: 2);
      final p = AnsiParser();
      p.parse('a\nb\nc\n', s);
      // After 3 LFs on 2 rows: 'a' and 'b' scrolled off, 'c' on row 0.
      expect(s.scrollback, hasLength(2));
      expect(rowText(s, 0).startsWith('c'), isTrue);
    });

    test('CR returns to column 0 without advancing the row', () {
      final s = makeState();
      final p = AnsiParser();
      p.parse('hello\rXY', s);
      expect(rowText(s, 0).startsWith('XYllo'), isTrue);
    });

    test('BS moves the cursor back one column', () {
      final s = makeState();
      final p = AnsiParser();
      p.parse('abc\x08X', s);
      expect(rowText(s, 0).startsWith('abX'), isTrue);
    });

    test('BEL is ignored', () {
      final s = makeState();
      AnsiParser().parse('a\x07b', s);
      expect(rowText(s, 0).startsWith('ab'), isTrue);
    });
  });

  group('AnsiParser: SGR colors', () {
    test('31 sets red foreground, 0 resets', () {
      final s = makeState();
      final p = AnsiParser();
      p.parse('\x1b[31mR\x1b[0mN', s);
      final rCell = s.grid[0][0];
      final nCell = s.grid[0][1];
      expect((rCell.fg as PaletteColor).index, 1);
      expect((nCell.fg as PaletteColor).index, 7);
      expect(rCell.bold, isFalse);
    });

    test('1 sets bold, 22 clears it', () {
      final s = makeState();
      final p = AnsiParser();
      p.parse('\x1b[1mB\x1b[22mN', s);
      expect(s.grid[0][0].bold, isTrue);
      expect(s.grid[0][1].bold, isFalse);
    });

    test('256-color fg: 38;5;196 is palette 196', () {
      final s = makeState();
      final p = AnsiParser();
      p.parse('\x1b[38;5;196mX', s);
      expect((s.grid[0][0].fg as PaletteColor).index, 196);
    });

    test('truecolor fg: 38;2;18;52;86', () {
      final s = makeState();
      final p = AnsiParser();
      p.parse('\x1b[38;2;18;52;86mX', s);
      final fg = s.grid[0][0].fg;
      expect(fg, isA<RgbColor>());
      final rgb = fg as RgbColor;
      expect(rgb.r, 18);
      expect(rgb.g, 52);
      expect(rgb.b, 86);
    });
  });

  group('AnsiParser: cursor movement and erase', () {
    test('cursor positioning H moves to row/col', () {
      final s = makeState();
      final p = AnsiParser();
      p.parse('\x1b[3;5HX', s);
      expect(s.cursor.row, 2);
      expect(s.cursor.col, 5);
      expect(s.grid[2][4].char, 'X');
    });

    test('erase display 2J clears the grid', () {
      final s = makeState();
      final p = AnsiParser();
      p.parse('hello\x1b[2J', s);
      expect(rowText(s, 0).trim(), isEmpty);
    });

    test('erase line 2K clears the current line', () {
      final s = makeState();
      final p = AnsiParser();
      p.parse('hello\x1b[2K', s);
      expect(rowText(s, 0).trim(), isEmpty);
    });

    test('unknown sequences are skipped without crashing', () {
      final s = makeState();
      final p = AnsiParser();
      p.parse('a\x1b[?25lb', s);
      expect(rowText(s, 0).startsWith('ab'), isTrue);
    });
  });

  group('TerminalPaneDropPreviewGeometry', () {
    test('groupEdgePaneExtent reserves one divider then splits', () {
      // (900 - 8) / (3 + 1) = 223.
      expect(
        TerminalPaneDropPreviewGeometry.groupEdgePaneExtent(
          totalExtent: 900,
          existingSessionLeafCount: 3,
        ),
        223.0,
      );
    });

    test('groupEdgePaneExtent clamps a zero/negative available extent', () {
      expect(
        TerminalPaneDropPreviewGeometry.groupEdgePaneExtent(
          totalExtent: 4,
          existingSessionLeafCount: 3,
        ),
        0.0,
      );
    });

    test('insetHighlightExtent insets the preview band', () {
      expect(TerminalPaneDropPreviewGeometry.insetHighlightExtent(900), 888.0);
    });
  });

  group('terminalPaneCloseAction', () {
    test('null session (launcher) detaches the pane', () {
      expect(
        terminalPaneCloseAction(sessionId: null, canArchiveSession: false),
        TerminalPaneCloseAction.detachPane,
      );
    });

    test('disposable session removes it', () {
      expect(
        terminalPaneCloseAction(sessionId: 's1', canArchiveSession: false),
        TerminalPaneCloseAction.removeSession('s1'),
      );
    });

    test('agent conversation asks for confirmation before archive', () {
      expect(
        terminalPaneCloseAction(sessionId: 's1', canArchiveSession: true),
        TerminalPaneCloseAction.confirmArchive('s1'),
      );
    });
  });

  group('SessionOutputStream (real /mobile/* wire format)', () {
    /// The REAL handle_output body (camelCase).
    String pollBody({
      String data = '',
      int offset = 0,
      int nextOffset = 0,
      bool truncated = false,
    }) => jsonEncode({
      'sessionID': 's1',
      'offset': offset,
      'nextOffset': nextOffset,
      'dataBase64': base64Encode(utf8.encode(data)),
      'truncated': truncated,
      'capturedAtUnixMs': 0,
    });

    test('feeds PTY bytes through the ANSI parser', () async {
      var calls = 0;
      late final SessionOutputStream stream;
      final mock = MockClient((request) async {
        calls++;
        expect(request.url.path, '/mobile/output');
        expect(request.url.queryParameters['session_id'], 's1');
        expect(request.headers['authorization'], 'Bearer t');
        if (calls >= 2) stream.stop();
        if (calls == 1) {
          return http.Response(
            pollBody(data: '\x1b[31mR', offset: 0, nextOffset: 6),
            200,
          );
        }
        return http.Response(pollBody(data: '', offset: 6, nextOffset: 6), 200);
      });
      final state = makeState();
      stream = SessionOutputStream(
        client: mockHost(mock),
        sessionId: 's1',
        state: state,
        pollWait: Duration.zero,
      );
      await stream.start();
      expect(stream.offset, 6);
      expect((state.grid[0][0].fg as PaletteColor).index, 1);
      expect(rowText(state, 0).startsWith('R'), isTrue);
    });

    test('truncated chunk resets the grid as a fresh baseline', () async {
      var calls = 0;
      late final SessionOutputStream stream;
      final mock = MockClient((request) async {
        calls++;
        if (calls >= 2) stream.stop();
        if (calls == 1) {
          return http.Response(
            pollBody(data: 'OLD', offset: 0, nextOffset: 3),
            200,
          );
        }
        // Journal rotated: truncated, fresh baseline from offset 100.
        return http.Response(
          pollBody(data: 'NEW', offset: 100, nextOffset: 103, truncated: true),
          200,
        );
      });
      final state = makeState();
      stream = SessionOutputStream(
        client: mockHost(mock),
        sessionId: 's1',
        state: state,
        pollWait: Duration.zero,
      );
      await stream.start();
      expect(rowText(state, 0).startsWith('NEW'), isTrue);
      expect(rowText(state, 0).contains('OLD'), isFalse);
    });

    test('401 stops the loop as fatal', () async {
      var onErrorCalled = false;
      final mock = MockClient((request) async => http.Response('no', 401));
      final stream = SessionOutputStream(
        client: mockHost(mock),
        sessionId: 's1',
        state: makeState(),
        pollWait: Duration.zero,
      );
      await stream.start(onError: (_) => onErrorCalled = true);
      expect(stream.running, isFalse);
      expect(onErrorCalled, isTrue);
    });

    test('sendInput posts to /mobile/write with sessionID + wid', () async {
      Map<String, dynamic>? sawJson;
      final mock = MockClient((request) async {
        expect(request.url.path, '/mobile/write');
        expect(request.headers['authorization'], 'Bearer t');
        sawJson = jsonDecode(request.body) as Map<String, dynamic>;
        return http.Response('{"ok":true}', 200);
      });
      final stream = SessionOutputStream(
        client: mockHost(mock),
        sessionId: 's1',
        state: makeState(),
      );
      await stream.sendInput([0x1b, 0x5b, 0x41], writeId: 'wid-1'); // up arrow
      expect(sawJson?['sessionID'], 's1');
      expect(sawJson?['data'], '\x1b[A');
      expect(sawJson?['wid'], 'wid-1');
    });

    test('resize posts to /mobile/resize with columns/rows', () async {
      Map<String, dynamic>? sawJson;
      final mock = MockClient((request) async {
        expect(request.url.path, '/mobile/resize');
        sawJson = jsonDecode(request.body) as Map<String, dynamic>;
        return http.Response('{"ok":true}', 200);
      });
      final stream = SessionOutputStream(
        client: mockHost(mock),
        sessionId: 's1',
        state: makeState(),
      );
      await stream.resize(120, 40);
      expect(sawJson, {'sessionID': 's1', 'columns': 120, 'rows': 40});
    });
  });

  group('HostedTerminalPaneView', () {
    test('hosted() wires the stream state, input, and resize', () {
      final client = mockHost(
        MockClient((request) async => http.Response('{"ok":true}', 200)),
      );
      final view = TerminalPaneView.hosted(
        client: client,
        sessionId: 's1',
        paneId: 'p1',
        title: 'zsh',
      );
      // The view renders the SAME TerminalState the stream feeds.
      expect(identical(view.state, view.stream!.state), isTrue);
      expect(view.state.cols, 80);
      expect(view.state.rows, 24);
      expect(view.sessionId, 's1');
      expect(view.isLive, isTrue);
      // Input and resize forward to the stream's Host routes.
      expect(view.onInput, isNotNull);
      expect(view.onResize, isNotNull);
      client.close();
    });

    test('start/stop delegate to the stream lifecycle', () async {
      var calls = 0;
      late final SessionOutputStream stream;
      final mock = MockClient((request) async {
        calls++;
        if (calls >= 2) stream.stop();
        return http.Response(
          jsonEncode({
            'sessionID': 's1',
            'offset': 0,
            'nextOffset': 0,
            'dataBase64': '',
            'truncated': false,
            'capturedAtUnixMs': 0,
          }),
          200,
        );
      });
      final client = mockHost(mock);
      final view = TerminalPaneView.hosted(
        client: client,
        sessionId: 's1',
        paneId: 'p1',
        title: 'zsh',
        onStreamError: (_) {},
      );
      stream = view.stream!;
      await view.start();
      expect(calls, greaterThan(0));
      expect(view.stream!.running, isFalse);
      view.stop();
      expect(view.stream!.running, isFalse);
      client.close();
    });

    test('static panes are not live', () {
      final view = TerminalPaneView(paneId: 'p1', title: 'zsh');
      expect(view.isLive, isFalse);
      expect(view.stream, isNull);
      expect(view.sessionId, isNull);
    });
  });

  group('TerminalOutputChunk', () {
    test('parses the real handle_output body', () {
      final chunk = TerminalOutputChunk.fromJson({
        'sessionID': 's1',
        'offset': 10,
        'nextOffset': 16,
        'dataBase64': base64Encode(utf8.encode('hi')),
        'truncated': true,
        'capturedAtUnixMs': 123,
      });
      expect(chunk.sessionId, 's1');
      expect(chunk.offset, 10);
      expect(chunk.nextOffset, 16);
      expect(utf8.decode(chunk.data), 'hi');
      expect(chunk.truncated, isTrue);
    });

    test('tolerates missing fields', () {
      final chunk = TerminalOutputChunk.fromJson({});
      expect(chunk.sessionId, '');
      expect(chunk.offset, 0);
      expect(chunk.nextOffset, 0);
      expect(chunk.data, isEmpty);
      expect(chunk.truncated, isFalse);
    });
  });
}
