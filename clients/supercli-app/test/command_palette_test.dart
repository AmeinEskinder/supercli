/// Behavior tests for the Cmd-K command palette and Ctrl-Tab MRU switcher.
///
/// Covers fuzzy-match scoring/order, palette keyboard navigation, MRU
/// ordering with wraparound, the RLE-fallback render tree, and the
/// declared key chords.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/keybindings.dart';
import 'package:supercli_app/screens/commandpaletteview.dart';
import 'package:supercli_app/widgets/list_navigation.dart';
import 'package:test/test.dart';

List<PaletteCommand> sampleCommands() => const [
      PaletteCommand(id: 'new-session', title: 'New Session', shortcut: 'meta+n'),
      PaletteCommand(id: 'open-settings', title: 'Open Settings', shortcut: 'meta+,'),
      PaletteCommand(
          id: 'sess-web', title: 'web — npm run dev', kind: PaletteCommandKind.session),
      PaletteCommand(
          id: 'sess-api', title: 'api — cargo watch', kind: PaletteCommandKind.session),
      PaletteCommand(
          id: 'preset-rust', title: 'Rust preset', kind: PaletteCommandKind.preset),
    ];

void main() {
  group('FuzzyMatch', () {
    test('empty query matches everything at full score', () {
      expect(FuzzyMatch.score('', 'anything'), 1.0);
    });

    test('non-subsequence returns 0', () {
      expect(FuzzyMatch.score('zzz', 'New Session'), 0.0);
      expect(FuzzyMatch.score('session', 'Open Settings'), 0.0);
    });

    test('matching is case-insensitive', () {
      expect(FuzzyMatch.score('SESSION', 'new session') > 0, isTrue);
    });

    test('exact/prefix ranks above scattered subsequence', () {
      final prefix = FuzzyMatch.score('sess', 'session list');
      final scattered = FuzzyMatch.score('sess', 'some extra silly string');
      expect(prefix, greaterThan(scattered));
    });

    test('word-boundary match ranks above mid-word match', () {
      final boundary = FuzzyMatch.score('set', 'Open Settings');
      final midword = FuzzyMatch.score('set', 'asset panel');
      expect(boundary, greaterThan(midword));
    });

    test('shorter target ranks above longer target for same query', () {
      final short = FuzzyMatch.score('new', 'New');
      final long = FuzzyMatch.score('new', 'New Session With A Long Name');
      expect(short, greaterThan(long));
    });
  });

  group('CommandPaletteState', () {
    test('empty filter shows all commands in order', () {
      final s = CommandPaletteState(commands: sampleCommands());
      expect(s.visible.map((c) => c.id),
          ['new-session', 'open-settings', 'sess-web', 'sess-api', 'preset-rust']);
    });

    test('filter narrows to fuzzy matches, best first', () {
      final s = CommandPaletteState(commands: const [
        PaletteCommand(id: 'scattered', title: 'New Session'),
        PaletteCommand(id: 'prefix', title: 'Sessions Panel'),
        PaletteCommand(id: 'unrelated', title: 'Quit App'),
      ]);
      s.setFilter('ses');
      final ids = s.visible.map((c) => c.id).toList();
      expect(ids, contains('prefix'));
      expect(ids, contains('scattered'));
      expect(ids, isNot(contains('unrelated')));
      // prefix/word-start match outranks the mid-string match
      expect(ids.indexOf('prefix'), lessThan(ids.indexOf('scattered')));
    });

    test('setFilter resets selection to the top', () {
      final s = CommandPaletteState(commands: sampleCommands());
      s.moveDown();
      s.moveDown();
      s.setFilter('api');
      expect(s.selectedIndex, 0);
      expect(s.visible.single.id, 'sess-api');
    });

    test('moveDown/moveUp wrap around', () {
      final s = CommandPaletteState(commands: sampleCommands());
      s.moveUp();
      expect(s.selectedIndex, s.visible.length - 1);
      s.moveDown();
      expect(s.selectedIndex, 0);
    });

    test('confirm returns the highlighted command', () {
      final s = CommandPaletteState(commands: sampleCommands());
      s.setFilter('settings');
      expect(s.confirm()?.id, 'open-settings');
    });

    test('confirm returns null when nothing matches', () {
      final s = CommandPaletteState(commands: sampleCommands());
      s.setFilter('zzz-no-match');
      expect(s.visible, isEmpty);
      expect(s.confirm(), isNull);
    });

    test('navigation on empty list is a no-op', () {
      final s = CommandPaletteState(commands: sampleCommands());
      s.setFilter('zzz-no-match');
      s.moveDown();
      s.moveUp();
      expect(s.confirm(), isNull);
    });

    test('clear restores the full list', () {
      final s = CommandPaletteState(commands: sampleCommands());
      s.setFilter('api');
      s.clear();
      expect(s.visible.length, sampleCommands().length);
      expect(s.selectedIndex, 0);
    });
  });

  group('CommandPaletteView', () {
    test('build renders input plus one row per visible result', () {
      final view = CommandPaletteView(commands: sampleCommands());
      final root = view.build() as UiColumn;
      expect(root.id, 'command-palette');
      expect(root.children.length, 2);
      expect(root.children[0], isA<UiInput>());
      final results = root.children[1] as UiColumn;
      expect(results.children.length, sampleCommands().length);
      expect(results.children.first, isA<UiRow>());
    });

    test('build highlights the selected row', () {
      final view =
          CommandPaletteView(commands: sampleCommands(), selectedIndex: 1);
      final results = (view.build() as UiColumn).children[1] as UiColumn;
      final selected = results.children[1] as UiRow;
      final unselected = results.children[0] as UiRow;
      String bg(UiRow row) {
        final json = row.toJson();
        return (json['style'] as Map)['background'] as String;
      }

      expect(bg(selected), '#264f78');
      expect(bg(unselected), '#1e1e1e');
    });

    test('build respects the filter', () {
      final view =
          CommandPaletteView(commands: sampleCommands(), filter: 'api');
      final results = (view.build() as UiColumn).children[1] as UiColumn;
      expect(results.children.length, 1);
    });

    test('dataset filters by query (screens_test compat)', () {
      final palette = CommandPaletteView(
        commands: const [
          PaletteCommand(id: 'c1', title: 'New Session'),
          PaletteCommand(id: 'c2', title: 'Open Settings'),
        ],
        filter: 'session',
      );
      final ds = palette.dataset();
      expect(ds.rowCount, 1);
      expect(ds.row(0)[0], 'New Session');
    });

    test('actions declare meta+k open and node-scoped nav keys', () {
      final actions = CommandPaletteView(commands: sampleCommands()).actions();
      final byName = {for (final a in actions) a.name: a};
      expect(byName['palette.open']!.keys, 'meta+k');
      expect(byName['palette.down']!.keys, 'down');
      expect(byName['palette.up']!.keys, 'up');
      expect(byName['palette.confirm']!.keys, 'enter');
      expect(byName['palette.dismiss']!.keys, 'escape');
    });
  });

  group('MruSwitcher', () {
    MruSwitcher make() => MruSwitcher(entries: const [
          MruEntry(id: 'a', title: 'web'),
          MruEntry(id: 'b', title: 'api'),
          MruEntry(id: 'c', title: 'db'),
        ]);

    test('starts at the most recent entry', () {
      expect(make().current?.id, 'a');
    });

    test('markUsed moves the entry to the front', () {
      final m = make();
      m.markUsed('c');
      expect(m.entries.map((e) => e.id), ['c', 'a', 'b']);
      expect(m.current?.id, 'c');
    });

    test('next/previous cycle with wraparound', () {
      final m = make();
      expect(m.next()?.id, 'b');
      expect(m.next()?.id, 'c');
      expect(m.next()?.id, 'a'); // wraps
      expect(m.previous()?.id, 'c'); // wraps back
    });

    test('add inserts at front, dedupes', () {
      final m = make();
      m.add(const MruEntry(id: 'b', title: 'api (renamed)'));
      expect(m.entries.map((e) => e.id), ['b', 'a', 'c']);
      expect(m.entries.first.title, 'api (renamed)');
    });

    test('remove drops the entry and clamps the index', () {
      final m = make();
      m.next(); // at b
      m.remove('b');
      expect(m.entries.map((e) => e.id), ['a', 'c']);
      expect(m.current?.id, isNot('b'));
    });

    test('empty switcher returns null', () {
      final m = MruSwitcher();
      expect(m.current, isNull);
      expect(m.next(), isNull);
      expect(m.previous(), isNull);
    });
  });

  group('MruSwitcherView', () {
    test('build renders entries and highlights the current one', () {
      final switcher = MruSwitcher(entries: const [
        MruEntry(id: 'a', title: 'web'),
        MruEntry(id: 'b', title: 'api'),
      ]);
      switcher.next(); // current = b
      final root = MruSwitcherView(switcher: switcher).build() as UiColumn;
      expect(root.id, 'mru-switcher');
      final entries = root.children[1] as UiColumn;
      expect(entries.children.length, 2);
      String bg(UiNode row) {
        final json = (row as UiRow).toJson();
        return (json['style'] as Map)['background'] as String;
      }

      expect(bg(entries.children[0]), '#1e1e1e');
      expect(bg(entries.children[1]), '#264f78');
    });

    test('actions declare ctrl+tab chords', () {
      final actions =
          MruSwitcherView(switcher: MruSwitcher()).actions();
      final byName = {for (final a in actions) a.name: a};
      expect(byName['switcher.next']!.keys, 'ctrl+tab');
      expect(byName['switcher.previous']!.keys, 'ctrl+shift+tab');
      expect(byName['switcher.dismiss']!.keys, 'escape');
    });
  });

  group('KeyboardListNavigator', () {
    test('wraps by default', () {
      final n = KeyboardListNavigator(length: 3);
      n.moveUp();
      expect(n.index, 2);
      n.moveDown();
      expect(n.index, 0);
    });

    test('clamps when wrap is false', () {
      final n = KeyboardListNavigator(length: 3, wrap: false);
      n.moveUp();
      expect(n.index, 0);
      n.index = 2;
      n.moveDown();
      expect(n.index, 2);
    });

    test('empty list is a no-op', () {
      final n = KeyboardListNavigator();
      expect(n.moveDown(), isFalse);
      expect(n.moveUp(), isFalse);
    });

    test('setLength clamps a stale index', () {
      final n = KeyboardListNavigator(length: 5, index: 4);
      n.setLength(2);
      expect(n.index, 1);
    });
  });

  group('AppKeybindings', () {
    test('global actions include palette open and switcher chords', () {
      const kb = AppKeybindings();
      final byName = {for (final a in kb.globalActions()) a.name: a};
      expect(byName['palette.open']!.keys, AppKeybindings.paletteOpen);
      expect(byName['switcher.next']!.keys, 'ctrl+tab');
      expect(byName['switcher.previous']!.keys, 'ctrl+shift+tab');
    });

    test('palette open uses the platform meta key', () {
      expect(AppKeybindings.paletteOpen, 'meta+k');
    });
  });
}
