/// Behavior tests for the Cmd-K command palette and Ctrl-Tab MRU switcher.
///
/// Covers fuzzy-match scoring/order, palette keyboard navigation, MRU
/// ordering with wraparound, the RLE-fallback render tree, and the
/// declared key chords.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/app.dart';
import 'package:supercli_app/keybindings.dart';
import 'package:supercli_app/models.dart';
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


/// Mounted-app behavior: the palette is built from the live action registry
/// + live sessions (no fixtures), and the MRU switcher tracks real sessions.
List<SessionSummary> _testSessions() => [
      SessionSummary(
          id: 's1', title: 'web — npm run dev', updatedAt: DateTime(2026, 9, 26)),
      SessionSummary(
          id: 's2', title: 'api — cargo watch', updatedAt: DateTime(2026, 9, 26)),
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
  group('SupercliApp palette mounting', () {
    test('paletteCommands lists real actions, not fixtures', () {
      final app = SupercliApp();
      app.sessions = _testSessions();
      final commands = app.paletteCommands();
      // Every registered global action appears (except palette.open itself
      // would be circular — but it IS listed so users can discover the chord).
      final ids = commands.map((c) => c.id).toSet();
      expect(ids, contains('action:sidebar.toggle'));
      expect(ids, contains('action:pane.splitRight'));
      expect(ids, contains('action:approval.approve'));
      expect(ids, contains('action:composer.focus'));
      // Shortcuts match the registered UiAction chords.
      final byId = {for (final c in commands) c.id: c};
      expect(byId['action:sidebar.toggle']!.shortcut, 'cmd+b');
      expect(byId['action:pane.splitRight']!.shortcut, 'cmd+d');
    });

    test('paletteCommands includes live sessions as session entries', () {
      final app = SupercliApp();
      app.sessions = _testSessions();
      final commands = app.paletteCommands();
      final sessionCmds = commands
          .where((c) => c.kind == PaletteCommandKind.session)
          .toList();
      expect(sessionCmds.map((c) => c.id), ['session:s1', 'session:s2']);
      expect(sessionCmds.map((c) => c.title),
          ['web — npm run dev', 'api — cargo watch']);
    });

    test('openPalette builds state from live commands; closePalette clears',
        () {
      final app = SupercliApp();
      app.sessions = _testSessions();
      expect(app.paletteOpen, isFalse);
      app.openPalette();
      expect(app.paletteOpen, isTrue);
      expect(app.paletteState!.commands.length,
          app.paletteCommands().length);
      // Filter narrows to the matching live session.
      app.paletteState!.setFilter('cargo');
      final visible = app.paletteState!.visible;
      expect(visible.map((c) => c.id), ['session:s2']);
      app.closePalette();
      expect(app.paletteOpen, isFalse);
      expect(app.paletteState, isNull);
    });

    test('palette confirm returns the highlighted live command', () {
      final app = SupercliApp();
      app.sessions = _testSessions();
      app.openPalette();
      app.paletteState!.setFilter('toggle sidebar');
      final cmd = app.paletteState!.confirm();
      expect(cmd, isNotNull);
      expect(cmd!.id, 'action:sidebar.toggle');
    });
  });

  group('SupercliApp MRU sync', () {
    test('syncMru seeds entries in session-list order', () {
      final app = SupercliApp();
      app.sessions = _testSessions();
      app.syncMru();
      // Session-list order is preserved (most-recent-first from the Host),
      // not reversed by the front-insert.
      expect(app.mruSwitcher.entries.map((e) => e.id), ['s1', 's2']);
    });

    test('syncMru refreshes titles of existing entries in place', () {
      final app = SupercliApp();
      app.sessions = _testSessions();
      app.syncMru();
      app.mruSwitcher.markUsed('s2'); // establish MRU order: s2, s1
      // s1 is renamed on the Host.
      app.sessions = [
        SessionSummary(
            id: 's1',
            title: 'web — npm run dev (renamed)',
            updatedAt: DateTime(2026, 9, 26)),
        SessionSummary(
            id: 's2', title: 'api — cargo watch', updatedAt: DateTime(2026, 9, 26)),
      ];
      app.syncMru();
      // Order preserved (s2 still most recent), title updated.
      expect(app.mruSwitcher.entries.map((e) => e.id), ['s2', 's1']);
      expect(app.mruSwitcher.entries.last.title, 'web — npm run dev (renamed)');
    });

    test('syncMru drops gone sessions and keeps MRU order', () {
      final app = SupercliApp();
      app.sessions = _testSessions();
      app.syncMru();
      // Mark s1 as most recently used.
      app.mruSwitcher.markUsed('s1');
      expect(app.mruSwitcher.entries.first.id, 's1');
      // s2 disappears from the Host; s3 appears.
      app.sessions = [
        SessionSummary(
            id: 's1',
            title: 'web — npm run dev',
            updatedAt: DateTime(2026, 9, 26)),
        SessionSummary(
            id: 's3', title: 'db — psql', updatedAt: DateTime(2026, 9, 26)),
      ];
      app.syncMru();
      final ids = app.mruSwitcher.entries.map((e) => e.id).toList();
      expect(ids, isNot(contains('s2')));
      // Brand-new sessions land at the front (most recent); s1's
      // explicitly-marked recency is preserved among survivors.
      expect(ids, contains('s1'));
      expect(ids, contains('s3'));
      // s1 was marked used before s3 appeared, so s1 stays ahead of
      // any older survivors (none here besides s1 itself).
      expect(ids.indexOf('s3'), 0);
    });

    test('switcher next/previous cycles live sessions with wraparound', () {
      final app = SupercliApp();
      app.sessions = _testSessions();
      app.syncMru();
      final first = app.mruSwitcher.current!.id;
      app.mruSwitcher.next();
      final second = app.mruSwitcher.current!.id;
      expect(second, isNot(first));
      app.mruSwitcher.next();
      expect(app.mruSwitcher.current!.id, first); // wrapped
      app.mruSwitcher.previous();
      expect(app.mruSwitcher.current!.id, second);
    });
  });

  group('SupercliApp palette actions', () {
    test('actions() includes palette and switcher chords when open', () {
      final app = SupercliApp();
      app.sessions = _testSessions();
      // Closed: only globals.
      var names = app.actions().map((a) => a.name).toSet();
      expect(names, contains('palette.open'));
      expect(names, contains('switcher.next'));
      expect(names, isNot(contains('palette.confirm')));
      // Open: scoped navigation appears.
      app.openPalette();
      names = app.actions().map((a) => a.name).toSet();
      expect(names, contains('palette.confirm'));
      expect(names, contains('palette.up'));
      expect(names, contains('palette.dismiss'));
      app.switcherOpen = true;
      names = app.actions().map((a) => a.name).toSet();
      expect(names, contains('switcher.dismiss'));
      expect(names, contains('switcher.confirm'));
    });

    test('sessions.up/down are scoped to the real sidebar node', () {
      final app = SupercliApp();
      final byName = {for (final a in app.actions()) a.name: a};
      for (final name in ['sessions.up', 'sessions.down']) {
        final action = byName[name]!;
        final json = action.toJson();
        // UiActionContext.toJson() is the node id string. The old
        // 'session-list' scope matched no rendered node and was dead;
        // 'sidebar' is the SidebarView root (UiColumn('sidebar')).
        expect(json['context'], 'sidebar', reason: name);
      }
    });
  });

  group('Selection clamping', () {
    test('confirm clamps a stale selectedIndex instead of failing', () {
      final s = CommandPaletteState(commands: sampleCommands());
      s.selectedIndex = 99; // stale: list shrank without a filter change
      final cmd = s.confirm();
      expect(cmd, isNotNull);
      expect(cmd!.id, sampleCommands().last.id);
      expect(s.selectedIndex, sampleCommands().length - 1);
    });

    test('confirm still returns null on an empty list', () {
      final s = CommandPaletteState(commands: sampleCommands());
      s.setFilter('zzz-no-match');
      s.selectedIndex = 99;
      expect(s.confirm(), isNull);
    });

    test('view build clamps an out-of-range selectedIndex', () {
      final view = CommandPaletteView(
          commands: sampleCommands(), selectedIndex: 99);
      final results = (view.build() as UiColumn).children[1] as UiColumn;
      String bg(UiNode row) {
        final json = (row as UiRow).toJson();
        return (json['style'] as Map)['background'] as String;
      }

      // The last row is highlighted, not none.
      expect(bg(results.children.last), '#264f78');
      expect(bg(results.children.first), '#1e1e1e');
    });
  });

  group('MruSwitcher.update', () {
    test('refreshes the title in place, keeping MRU position', () {
      final m = MruSwitcher(entries: const [
        MruEntry(id: 'a', title: 'web'),
        MruEntry(id: 'b', title: 'api'),
      ]);
      m.update(const MruEntry(id: 'b', title: 'api (renamed)'));
      expect(m.entries.map((e) => e.id), ['a', 'b']);
      expect(m.entries.last.title, 'api (renamed)');
    });

    test('unknown id is a no-op', () {
      final m = MruSwitcher(entries: const [MruEntry(id: 'a', title: 'web')]);
      m.update(const MruEntry(id: 'zzz', title: 'nope'));
      expect(m.entries.map((e) => e.id), ['a']);
    });
  });

  group('switcher.confirm', () {
    test('AppKeybindings declares enter confirm scoped to the switcher node',
        () {
      const kb = AppKeybindings();
      final actions = kb.switcherActions('mru-switcher');
      final byName = {for (final a in actions) a.name: a};
      expect(byName['switcher.confirm']!.keys, 'enter');
      final json = byName['switcher.confirm']!.toJson();
      expect(json['context'], 'mru-switcher');
    });

    test('MruSwitcherView.actions includes the enter confirm', () {
      final actions =
          MruSwitcherView(switcher: MruSwitcher()).actions();
      final byName = {for (final a in actions) a.name: a};
      expect(byName['switcher.confirm']!.keys, 'enter');
    });
  });

  group('paletteCommands shortcut honesty', () {
    test('pane.focusNext/focusPrev claim no shortcut (ctrl+tab is the switcher)',
        () {
      final app = SupercliApp();
      final byId = {for (final c in app.paletteCommands()) c.id: c};
      expect(byId['action:pane.focusNext']!.shortcut, isEmpty);
      expect(byId['action:pane.focusPrev']!.shortcut, isEmpty);
      // Other actions still advertise their real chords.
      expect(byId['action:sidebar.toggle']!.shortcut, 'cmd+b');
    });
  });

  group('SupercliApp.executePaletteCommand', () {
    test('session command switches session and records MRU use', () {
      final app = SupercliApp();
      app.sessions = _testSessions();
      app.syncMru(); // MRU: s1, s2
      final cmd = PaletteCommand(
          id: 'session:s2',
          title: 'api — cargo watch',
          kind: PaletteCommandKind.session);
      final actionName = app.executePaletteCommand(cmd);
      expect(actionName, isNull);
      expect(app.selectedSession, 1);
      expect(app.mruSwitcher.entries.first.id, 's2');
      expect(app.mruSwitcher.current?.id, 's2');
    });

    test('unknown session id changes nothing', () {
      final app = SupercliApp();
      app.sessions = _testSessions();
      app.syncMru();
      final before = app.mruSwitcher.entries.map((e) => e.id).toList();
      final cmd = PaletteCommand(
          id: 'session:gone',
          title: 'gone',
          kind: PaletteCommandKind.session);
      expect(app.executePaletteCommand(cmd), isNull);
      expect(app.selectedSession, 0);
      expect(app.mruSwitcher.entries.map((e) => e.id).toList(), before);
    });

    test('action command returns the action name for dispatch', () {
      final app = SupercliApp();
      final cmd = const PaletteCommand(
          id: 'action:sidebar.toggle', title: 'Toggle sidebar');
      expect(app.executePaletteCommand(cmd), 'sidebar.toggle');
      // State untouched until the dispatcher runs it.
      expect(app.sidebarCollapsed, isFalse);
    });

    test('non-action, non-session id returns null', () {
      final app = SupercliApp();
      const cmd = PaletteCommand(id: 'weird', title: 'weird');
      expect(app.executePaletteCommand(cmd), isNull);
    });
  });
}
