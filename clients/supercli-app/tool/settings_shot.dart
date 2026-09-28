/// Open the real settings UI in a native 1440x900 window, wired to a LIVE
/// Host over the authenticated mobile API.
///
/// This is the verification harness for the SettingsView wiring: it proves
/// the settings overlay mounts against real Host data, not fixtures.
///
/// Usage:
///   dart tool/settings_shot.dart \
///     --base-url `https://localhost:39585` \
///     --token `<paired-device-bearer-token>` \
///     --cert `/path/to/host-home/remote/tls/cert.pem`
///
/// The Host's direct /mobile endpoint serves TLS with a self-signed
/// certificate and never accepts Bearer tokens over plaintext, so the
/// HTTP client pins the Host certificate (--cert). Pair with
/// tool/pair_live_host.py to obtain a genuine token.
///
/// The window stays open until the process is killed; drive it with
/// tool/capture_shot.py under Xvfb for the real screenshot.
library;

import 'dart:io';

import 'package:gpuidart/gpuidart.dart';
import 'package:http/http.dart' as http;
import 'package:http/io_client.dart';

import 'package:supercli_app/app.dart';
import 'package:supercli_app/host_client.dart';
import 'package:supercli_app/screens/settings_controller.dart';
import 'package:supercli_app/screens/settings_local_store.dart';

String _arg(List<String> args, String name, String fallback) {
  final prefix = '--$name=';
  for (final arg in args) {
    if (arg.startsWith(prefix)) return arg.substring(prefix.length);
  }
  return fallback;
}

/// An HTTP client that pins the Host's self-signed certificate: TLS
/// failures (wrong cert) are rejected, the exact pinned cert is accepted.
/// The Bearer token still provides authentication; pinning only replaces
/// the public CA trust chain for this test Host.
http.Client _pinnedClient(String certPemPath) {
  final expectedPem = File(certPemPath).readAsStringSync().trim();
  final io = HttpClient();
  io.badCertificateCallback =
      (X509Certificate cert, String host, int port) {
    return cert.pem.trim() == expectedPem;
  };
  return IOClient(io);
}

Future<void> main(List<String> args) async {
  final baseUrl = _arg(args, 'base-url', 'https://localhost:39585');
  final token = _arg(args, 'token', '');
  final certPath = _arg(args, 'cert', '');
  if (token.isEmpty || certPath.isEmpty) {
    stderr.writeln('usage: dart tool/settings_shot.dart '
        '--base-url=<https://host:port> --token=<bearer> --cert=<cert.pem>');
    exit(2);
  }

  final client = HostClient(
    baseUrl: Uri.parse(baseUrl),
    token: token,
    httpClient: _pinnedClient(certPath),
  );

  // Local-only settings persist through the app config, like production.
  final localStore =
      FileSettingsLocalStore(FileSettingsLocalStore.defaultPath());
  final controller = SettingsController(
    host: client,
    localStore: localStore,
    onError: (message) => stderr.writeln('settings: $message'),
  );
  await controller.load();

  final app = SupercliApp();
  app.settingsController = controller;
  app.openSettings();
  stderr.writeln(
      'settings loaded from live Host; overlay open=${app.settingsOpen}');

  final host = await GpuiHost.openView(
    app.build,
    window: const GpuiWindowOptions(
      title: 'supercli — Settings',
      width: 1440,
      height: 900,
    ),
    actions: app.actions(),
  );
  stderr.writeln('native window open (1440x900)');

  // Route native UI events back through the app, then re-render — the same
  // loop the production shell uses.
  await for (final event in host.events) {
    var handled = false;
    switch (event.type) {
      case 'action':
        final action = event.action;
        if (action != null) handled = app.handleAction(action.name);
      case 'click':
        final id = event.id;
        if (id != null) handled = app.handleClick(id);
    }
    if (handled) await host.rebuild();
    if (event.type == 'closed') break;
  }

  controller.dispose();
  client.close();
}
