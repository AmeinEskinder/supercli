/// Tests for terminal features: find bar keys, font size, OSC links,
/// cmd-click paths, file drop. Rows 177–181 — [DESKTOP] parity.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/screens/clickablepath.dart';
import 'package:supercli_app/screens/terminalfindbar.dart';
import 'package:supercli_app/screens/terminalsettings.dart';
import 'package:test/test.dart';

void main() {
  group('TerminalFindBar keybindings (row 177)', () {
    test('find.open uses platform primary modifier + f', () {
      final actions = TerminalFindBar().actions();
      final open = actions.firstWhere((a) => a.name == 'find.open');
      expect(open.keys, endsWith('+f'));
    });

    test('find.next/prev use mod+g and shift+mod+g', () {
      final actions = TerminalFindBar().actions();
      final next = actions.firstWhere((a) => a.name == 'find.next');
      final prev = actions.firstWhere((a) => a.name == 'find.prev');
      expect(next.keys, endsWith('+g'));
      expect(prev.keys, startsWith('shift+'));
      expect(prev.keys, endsWith('+g'));
    });

    test('escape still closes', () {
      final actions = TerminalFindBar().actions();
      expect(actions.firstWhere((a) => a.name == 'find.close').keys, 'escape');
    });
  });

  group('TerminalFontSize (row 178)', () {
    test('increase/decrease clamp to bounds', () {
      final fs = TerminalFontSize();
      for (var i = 0; i < 100; i++) {
        fs.increase();
      }
      expect(fs.size, maxTerminalFontSize);
      for (var i = 0; i < 100; i++) {
        fs.decrease();
      }
      expect(fs.size, minTerminalFontSize);
    });

    test('reset restores default', () {
      final fs = TerminalFontSize();
      fs.increase();
      fs.increase();
      fs.reset();
      expect(fs.size, defaultTerminalFontSize);
    });

    test('actions use primary modifier chords', () {
      final actions = TerminalFontSize().actions();
      expect(actions.map((a) => a.name),
          containsAll(['font.increase', 'font.decrease', 'font.reset']));
      expect(actions.firstWhere((a) => a.name == 'font.reset').keys,
          endsWith('+0'));
    });

    test('build shows current size', () {
      final node = TerminalFontSize(15).build() as UiRow;
      expect((node.children[1] as UiText).text, '15pt');
    });
  });

  group('OscSequenceParser (row 179)', () {
    test('parses OSC 8 hyperlinks', () {
      const out =
          '\x1b]8;;https://example.com\x1b\\click here\x1b]8;;\x1b\\ done';
      final links = OscSequenceParser.hyperlinks(out);
      expect(links.length, 1);
      expect(links[0].uri, 'https://example.com');
      expect(links[0].text, 'click here');
    });

    test('empty output has no links', () {
      expect(OscSequenceParser.hyperlinks('plain text'), isEmpty);
    });

    test('tracks last OSC 7 cwd', () {
      const out = '\x1b]7;file://host/home/a\x1b\\'
          '\x1b]7;file://host/home/b\x1b\\';
      expect(OscSequenceParser.cwd(out), '/home/b');
    });

    test('no OSC 7 means null cwd', () {
      expect(OscSequenceParser.cwd('no sequences'), isNull);
    });
  });

  group('FileLocation.parse (row 180)', () {
    test('parses path:line:col', () {
      final loc = FileLocation.parse('src/main.dart:42:7')!;
      expect(loc.path, 'src/main.dart');
      expect(loc.line, 42);
      expect(loc.column, 7);
    });

    test('parses path:line', () {
      final loc = FileLocation.parse('src/main.dart:42')!;
      expect(loc.path, 'src/main.dart');
      expect(loc.line, 42);
      expect(loc.column, isNull);
    });

    test('parses bare path', () {
      final loc = FileLocation.parse('src/main.dart')!;
      expect(loc.path, 'src/main.dart');
      expect(loc.line, isNull);
    });

    test('rejects non-paths', () {
      expect(FileLocation.parse('hello'), isNull);
      expect(FileLocation.parse(''), isNull);
    });

    test('opener shows position and target', () {
      final opener = CmdClickPathOpener(
        location: FileLocation.parse('a.dart:3:1')!,
        openInApp: false,
      );
      final node = opener.build() as UiRow;
      expect((node.children[0] as UiText).text, 'a.dart:3:1');
      expect((node.children[1] as UiButton).label, 'Open in Editor');
    });
  });

  group('FileDropTarget (row 181)', () {
    test('shows hover state', () {
      final node =
          FileDropTarget(targetId: 't1', hovering: true).build() as UiRow;
      expect((node.children[0] as UiText).text, contains('Drop files'));
    });
  });
}
