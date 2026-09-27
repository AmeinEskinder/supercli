/// Screenshot tool: opens a REAL native window (1440x900) showing the LIVE
/// TerminalPaneView streaming a real Host session.
///
/// Usage:
///   dart run tool/terminal_pane_shot.dart \
///     --url https://127.0.0.1:PORT --token TOKEN --session SESSION_ID
///
/// The pane is built with [TerminalPaneView.hosted]: the grid is fed by a
/// live [SessionOutputStream] long-polling the authenticated Host's
/// GET /mobile/output — never synthetic content. Keep the window open for
/// the screenshot tool, then exit.
///
/// ENVIRONMENTAL BLOCKER (2026-09-27): this tool cannot run on the current
/// VM because the gpuidart native library fails to load:
/// `libxkbcommon-x11.so.0 => not found` (see `ldd libgpuidart.so`).
/// Until that system library is present, no native window — and therefore
/// no real screenshot — can be produced here. Do NOT substitute a synthetic
/// render and present it as evidence.
library;

import 'dart:async';
import 'dart:io';

import 'package:gpuidart/gpuidart.dart';
import 'package:http/io_client.dart';
import 'package:supercli_app/host_client.dart';
import 'package:supercli_app/screens/terminalpaneview.dart';

Future<void> main(List<String> args) async {
  String? url;
  String? token;
  String? sessionId;
  for (var i = 0; i < args.length; i++) {
    switch (args[i]) {
      case '--url':
        url = args[++i];
      case '--token':
        token = args[++i];
      case '--session':
        sessionId = args[++i];
    }
  }
  if (url == null || token == null || sessionId == null) {
    stderr.writeln(
      'usage: dart run tool/terminal_pane_shot.dart --url <https://host:port> '
      '--token <Bearer> --session <session-id>',
    );
    exit(2);
  }

  // Local-Host convenience: trust the Host's self-signed mobile certificate.
  final httpClient = HttpClient()
    ..badCertificateCallback = (cert, host, port) => true;
  final client = HostClient(
    baseUrl: Uri.parse(url),
    httpClient: IOClient(httpClient),
    token: token,
  );

  // One output poll first so the window opens with real content, not blank.
  final first = await client.terminalOutput(sessionId, limit: 65536);
  stdout.writeln(
    'first chunk: ${first.data.length} bytes '
    '(offset ${first.offset} -> ${first.nextOffset})',
  );

  final view = TerminalPaneView.hosted(
    client: client,
    sessionId: sessionId,
    paneId: 'shot-pane',
    title: 'live — $sessionId',
    onStreamError: (Object e) => stderr.writeln('stream error: $e'),
  );
  // Best-effort initial size; gpuidart has no window-size observation API.
  await view.start();

  final host = await GpuiHost.open(
    view.build(),
    actions: view.actions(),
    window: const GpuiWindowOptions(
      title: 'supercli — live terminal',
      width: 1440,
      height: 900,
    ),
  );

  // Keep the window open for the screenshot tool.
  await Future<void>.delayed(const Duration(seconds: 20));
  view.stop();
  await host.close();
  client.close();
}
