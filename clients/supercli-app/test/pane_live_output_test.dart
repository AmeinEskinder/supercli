/// Proves the attached pane renders real terminal output instead of the
/// legacy `[$title]` placeholder.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/pane_layout.dart';
import 'package:supercli_app/session_output.dart';
import 'package:supercli_app/terminal/terminal_state.dart';
import 'package:test/test.dart';

/// Collects every UiText's text in the tree.
List<String> collectText(UiNode node) {
  final out = <String>[];
  void visit(UiNode n) {
    if (n is UiText) out.add(n.text);
    final List<UiNode> children = switch (n) {
      UiColumn() => n.children,
      UiRow() => n.children,
      _ => const <UiNode>[],
    };
    for (final child in children) {
      visit(child);
    }
  }

  visit(node);
  return out;
}

void main() {
  UiNode buildLeaf(PaneLeaf leaf) =>
      leaf.build(focusedId: leaf.paneId, zoomedId: null, depth: 0);

  group('attached pane live output', () {
    test('leaf with terminal state renders real output, not [title]', () {
      final state = TerminalState(cols: 40, rows: 10);
      state.writeString(
        decodeOutputText(
          '\x1B[32mHello from live session 2\x1B[0m\n\$ '.codeUnits,
        ),
      );

      final leaf = PaneLeaf(
        paneId: 'p1',
        title: 'session-beta',
        terminalState: state,
      );
      final texts = collectText(buildLeaf(leaf));
      final joined = texts.join('\n');
      expect(joined, contains('Hello from live session 2'));
      expect(joined, isNot(contains('[session-beta]')));
    });

    test('leaf without terminal state keeps the placeholder', () {
      const leaf = PaneLeaf(paneId: 'p1', title: 'zsh');
      final texts = collectText(buildLeaf(leaf));
      expect(texts.join('\n'), contains('[zsh]'));
    });

    test('withTerminalState preserves identity fields', () {
      const leaf = PaneLeaf(paneId: 'p1', title: 'zsh');
      final state = TerminalState(cols: 40, rows: 10);
      final attached = leaf.withTerminalState(state);
      expect(attached.paneId, 'p1');
      expect(attached.title, 'zsh');
      expect(attached.terminalState, same(state));
      expect(leaf.terminalState, isNull);
    });
  });
}
