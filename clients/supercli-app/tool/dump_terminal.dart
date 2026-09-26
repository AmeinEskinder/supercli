/// Builds a sample terminal pane and dumps its JSON snapshot for rendering.
library;

import 'dart:convert';
import 'dart:io';

import 'package:supercli_app/terminal/terminal_pane.dart';
import 'package:supercli_app/terminal/terminal_state.dart';
import 'package:supercli_app/terminal/terminal_types.dart';

void main() {
  final state = TerminalState(cols: 60, rows: 16, fontSize: 13.0);

  // Simulate a colorful shell session.
  state.writeString('\$ ', fg: TerminalColor.palette(2), bold: true);
  state.writeString('ls --color=always\n', bold: true);
  state.writeString('src/\n', fg: TerminalColor.palette(4), bold: true);
  state.writeString('Cargo.toml\n', fg: TerminalColor.palette(3));
  state.writeString('README.md\n');
  state.writeString('target/\n', fg: TerminalColor.palette(4), bold: true);
  state.writeString('\$ ', fg: TerminalColor.palette(2), bold: true);
  state.writeString('cargo test ',
      fg: TerminalColor.rgb(255, 165, 0)); // truecolor orange
  state.writeString('--workspace\n');
  state.writeString('test result: ', fg: TerminalColor.palette(7));
  state.writeString('ok', fg: TerminalColor.palette(2), bold: true);
  state.writeString('. 226 passed; 0 failed\n', fg: TerminalColor.palette(7));
  state.writeString('\$ ', fg: TerminalColor.palette(2), bold: true);
  // 256-color cube sample.
  state.writeString('cube: ');
  for (final i in [196, 46, 21, 201, 226]) {
    state.writeString('██', fg: TerminalColor.palette(i));
  }
  state.writeString('\n\$ ', fg: TerminalColor.palette(2), bold: true);

  // A selection to show the highlight.
  state.select(2, 6, 12, 6);

  final pane = TerminalPane(paneId: 'demo', title: 'zsh — supercli', state: state);
  final node = pane.build();
  final out = File('/tmp/terminal-snapshot.json');
  out.writeAsStringSync(
      const JsonEncoder.withIndent('  ').convert(node.toJson()));
  print('wrote ${out.path}');
}
