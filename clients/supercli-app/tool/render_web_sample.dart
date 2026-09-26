/// Renders a sample App tree to a standalone HTML page for parity proof.
///
/// Usage: `dart tool/render_web_sample.dart [out.html]`
///
/// The sample mirrors the widgets a real screen produces (heading, search
/// input, session list table, footer actions, help links) and feeds their
/// `UiNode.toJson()` output through the DOM renderer, proving the same App
/// tree renders in a browser.
library;

import 'dart:io';

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/web/web.dart';

void main(List<String> args) {
  final out = args.isNotEmpty
      ? args.first
      : '../../docs/internal/proofs/proof-screenshots/parity-web-147.html';

  // A realistic App tree: heading + filter input + session list (table
  // backed by a host dataset) + footer actions + the help-links section.
  final tree = UiColumn('sample-page', [
    const UiText('sample-title', 'Sessions', style: UiStyle(fontSize: 20.0, fontWeight: UiFontWeight.bold)),
    const UiInput('sample-filter', placeholder: 'Filter sessions…'),
    const UiTable('sample-list', dataset: 'sessions'),
    UiRow('sample-footer', [
      const UiButton('sample-new', 'New session'),
      const UiButton('sample-archive', 'Archive'),
    ]),
    const HelpLinksView().build(),
  ]);

  final page = renderWebPage(rootJson: tree.toJson(), title: 'supercli web renderer sample');
  File(out).writeAsStringSync(page);
  stdout.writeln('wrote ${File(out).path} (${page.length} bytes)');
}
