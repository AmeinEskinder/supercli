/// Command palette (Cmd-K) and MRU session switcher (Ctrl-Tab).
///
/// Port of `CommandPaletteView.swift`. The Swift version is a floating panel
/// with a filter field and a keyboard-navigable results list; this port keeps
/// that behavior against upstream gpuidart:
///
/// - [FuzzyMatch] scores subsequence matches (consecutive-run and
///   word-boundary bonuses, case-insensitive).
/// - [CommandPaletteState] holds filter text + selection and exposes
///   keyboard navigation (up/down/enter/escape).
/// - [CommandPaletteView.build] renders through the RLE fallback
///   (UiRow/UiText, like TerminalPane.buildFallback): one row per result,
///   the selected row highlighted via style. No native list widget needed.
/// - [MruSwitcher] tracks recent sessions/panes in most-recently-used order;
///   [MruSwitcherView] renders the Ctrl-Tab overlay, also via UiRow/UiText.
///
/// Key chords live in package:supercli_app/keybindings.dart.
library;

import 'package:gpuidart/gpuidart.dart';

import '../keybindings.dart';

/// What kind of entry a palette row represents.
enum PaletteCommandKind {
  command('command'),
  session('session'),
  project('project'),
  preset('preset');

  const PaletteCommandKind(this.wire);
  final String wire;
}

/// A command in the palette.
final class PaletteCommand {
  const PaletteCommand({
    required this.id,
    required this.title,
    this.subtitle = '',
    this.shortcut = '',
    this.kind = PaletteCommandKind.command,
  });

  final String id;
  final String title;
  final String subtitle;
  final String shortcut;
  final PaletteCommandKind kind;
}

/// Fuzzy subsequence matcher.
///
/// Returns a score in (0, 1] when every character of [query] appears in
/// [target] in order (case-insensitive), 0 otherwise. Scoring rewards:
/// - matches at the start of the string or a word boundary (` `, `_`, `-`,
///   `.`, `/`),
/// - consecutive matched runs,
/// - shorter targets (prefix-heavy matches rank first).
/// An empty query scores 1.0 so the unfiltered list keeps insertion order.
final class FuzzyMatch {
  const FuzzyMatch._();

  static const _boundaries = {' ', '_', '-', '.', '/', '\\', ':'};

  static double score(String query, String target) {
    if (query.isEmpty) return 1.0;
    if (target.isEmpty) return 0.0;
    final q = query.toLowerCase();
    final t = target.toLowerCase();
    var ti = 0;
    var points = 0.0;
    var run = 0; // length of the current consecutive run
    var firstAt = -1;
    for (var qi = 0; qi < q.length; qi++) {
      final qc = q[qi];
      var found = -1;
      for (var j = ti; j < t.length; j++) {
        if (t[j] == qc) {
          found = j;
          break;
        }
      }
      if (found < 0) return 0.0; // query char not present in order
      if (firstAt < 0) firstAt = found;
      final boundary = found == 0 || _boundaries.contains(t[found - 1]);
      if (found == ti) {
        run++;
        points += 2.0 + run; // consecutive run bonus grows
      } else {
        run = 0;
        points += 1.0;
      }
      if (boundary) points += 2.0;
      ti = found + 1;
    }
    // Prefer early, compact matches on short targets.
    final span = ti - firstAt;
    points += 4.0 * q.length / (span + 1);
    points += 2.0 * q.length / (t.length + 1);
    return points;
  }
}

/// Mutable command-palette state: filter text plus keyboard selection.
///
/// The host owns one of these while the palette is open and feeds it key
/// actions (`palette.up`, `palette.down`, `palette.confirm`,
/// `palette.dismiss`) from the event stream.
final class CommandPaletteState {
  CommandPaletteState({required this.commands});

  final List<PaletteCommand> commands;

  String filter = '';
  int selectedIndex = 0;

  /// Visible commands, fuzzy-filtered by [filter] and sorted by score.
  /// Stable for ties: insertion order wins.
  List<PaletteCommand> get visible {
    final scored = <({PaletteCommand command, double score, int index})>[];
    for (var i = 0; i < commands.length; i++) {
      final c = commands[i];
      final s = FuzzyMatch.score(filter, '${c.title} ${c.subtitle}');
      if (s > 0) scored.add((command: c, score: s, index: i));
    }
    scored.sort((a, b) {
      final byScore = b.score.compareTo(a.score);
      return byScore != 0 ? byScore : a.index.compareTo(b.index);
    });
    return [for (final s in scored) s.command];
  }

  void setFilter(String value) {
    filter = value;
    selectedIndex = 0;
  }

  void clear() {
    filter = '';
    selectedIndex = 0;
  }

  void moveDown() {
    final n = visible.length;
    if (n == 0) return;
    selectedIndex = (selectedIndex + 1) % n;
  }

  void moveUp() {
    final n = visible.length;
    if (n == 0) return;
    selectedIndex = (selectedIndex - 1 + n) % n;
  }

  /// The highlighted command, or null when the list is empty.
  PaletteCommand? confirm() {
    final v = visible;
    if (v.isEmpty) return null;
    if (selectedIndex < 0 || selectedIndex >= v.length) return null;
    return v[selectedIndex];
  }
}

/// The command palette overlay.
///
/// Renders the filter input plus one UiRow per visible result (RLE
/// fallback pattern, like TerminalPane.buildFallback). The selected row is
/// highlighted through its style.
final class CommandPaletteView {
  CommandPaletteView({
    this.commands = const [],
    this.filter = '',
    this.selectedIndex = 0,
    this.nodeId = 'command-palette',
  });

  final List<PaletteCommand> commands;
  final String filter;
  final int selectedIndex;
  final String nodeId;

  List<PaletteCommand> _visible() {
    final state = CommandPaletteState(commands: commands);
    state.setFilter(filter);
    return state.visible;
  }

  /// RLE fallback tree: input + one row per result.
  UiNode build() {
    final visible = _visible();
    final rows = <UiNode>[];
    for (var i = 0; i < visible.length; i++) {
      rows.add(_resultRow(visible[i], selected: i == selectedIndex));
    }
    return UiColumn(nodeId, [
      const UiInput(
        'command-palette-filter',
        placeholder: 'Type a command or search sessions…',
      ),
      UiColumn('$nodeId-results', rows),
    ]);
  }

  UiRow _resultRow(PaletteCommand c, {required bool selected}) {
    final bg = selected ? '#264f78' : '#1e1e1e';
    final fg = selected ? '#ffffff' : '#d3d7cf';
    return UiRow(
      '$nodeId-row-${c.id}',
      [
        UiText(
          '$nodeId-title-${c.id}',
          c.title,
          style: UiStyle(
            foreground: UiColor.hex(fg),
            fontWeight:
                selected ? UiFontWeight.semibold : UiFontWeight.normal,
          ),
        ),
        if (c.subtitle.isNotEmpty)
          UiText(
            '$nodeId-sub-${c.id}',
            c.subtitle,
            style: UiStyle(foreground: UiColor.hex('#8a8a8a')),
          ),
        if (c.shortcut.isNotEmpty)
          UiText(
            '$nodeId-keys-${c.id}',
            c.shortcut,
            style: UiStyle(foreground: UiColor.hex('#8a8a8a')),
          ),
      ],
      style: UiStyle(background: UiColor.hex(bg)),
    );
  }

  /// Table dataset of the filtered commands (kept for host interop).
  TableDataset dataset() {
    final visible = _visible();
    return TableDataset(
      'palette-results',
      columns: const ['Command', 'Shortcut'],
      rows: visible.map((c) => [c.title, c.shortcut]).toList(),
    );
  }

  List<UiAction> actions() => [
        const UiAction(name: 'palette.open', keys: AppKeybindings.paletteOpen),
        ...const AppKeybindings().paletteActions('command-palette'),
      ];
}

/// One entry in the MRU switcher: a session or pane.
final class MruEntry {
  const MruEntry({
    required this.id,
    required this.title,
    this.subtitle = '',
  });

  final String id;
  final String title;
  final String subtitle;
}

/// Most-recently-used session/pane ordering for Ctrl-Tab.
///
/// [markUsed] moves an entry to the front. [next]/[previous] cycle through
/// the list with wraparound and return the newly current entry, or null
/// when empty.
final class MruSwitcher {
  MruSwitcher({List<MruEntry> entries = const []})
      : _entries = List.of(entries);

  final List<MruEntry> _entries;
  int _index = 0;

  List<MruEntry> get entries => List.unmodifiable(_entries);
  int get index => _index;
  int get length => _entries.length;
  bool get isEmpty => _entries.isEmpty;

  MruEntry? get current =>
      _entries.isEmpty ? null : _entries[_index.clamp(0, _entries.length - 1)];

  void add(MruEntry entry) {
    _entries.removeWhere((e) => e.id == entry.id);
    _entries.insert(0, entry);
    _index = 0;
  }

  void remove(String id) {
    final at = _entries.indexWhere((e) => e.id == id);
    if (at < 0) return;
    _entries.removeAt(at);
    if (_index >= _entries.length) _index = _entries.length - 1;
    if (_index < 0) _index = 0;
  }

  /// Record use: move [id] to the front (most recent).
  void markUsed(String id) {
    final at = _entries.indexWhere((e) => e.id == id);
    if (at < 0) return;
    final entry = _entries.removeAt(at);
    _entries.insert(0, entry);
    _index = 0;
  }

  MruEntry? next() {
    if (_entries.isEmpty) return null;
    _index = (_index + 1) % _entries.length;
    return current;
  }

  MruEntry? previous() {
    if (_entries.isEmpty) return null;
    _index = (_index - 1 + _entries.length) % _entries.length;
    return current;
  }

  void resetSelection() => _index = 0;
}

/// The Ctrl-Tab overlay: MRU entries rendered as UiRow/UiText (RLE
/// fallback). The current entry is highlighted; releasing Ctrl (or Enter)
/// confirms, Escape dismisses.
final class MruSwitcherView {
  MruSwitcherView({
    required this.switcher,
    this.nodeId = 'mru-switcher',
  });

  final MruSwitcher switcher;
  final String nodeId;

  UiNode build() {
    final rows = <UiNode>[];
    for (var i = 0; i < switcher.entries.length; i++) {
      rows.add(_entryRow(switcher.entries[i], selected: i == switcher.index));
    }
    return UiColumn(nodeId, [
      UiText('$nodeId-hint',
          'Ctrl+Tab / Ctrl+Shift+Tab to switch · release to open',
          style: UiStyle(foreground: UiColor.hex('#8a8a8a'))),
      UiColumn('$nodeId-entries', rows),
    ]);
  }

  UiRow _entryRow(MruEntry e, {required bool selected}) {
    final bg = selected ? '#264f78' : '#1e1e1e';
    final fg = selected ? '#ffffff' : '#d3d7cf';
    return UiRow(
      '$nodeId-row-${e.id}',
      [
        UiText(
          '$nodeId-title-${e.id}',
          e.title,
          style: UiStyle(
            foreground: UiColor.hex(fg),
            fontWeight:
                selected ? UiFontWeight.semibold : UiFontWeight.normal,
          ),
        ),
        if (e.subtitle.isNotEmpty)
          UiText(
            '$nodeId-sub-${e.id}',
            e.subtitle,
            style: UiStyle(foreground: UiColor.hex('#8a8a8a')),
          ),
      ],
      style: UiStyle(background: UiColor.hex(bg)),
    );
  }

  List<UiAction> actions() => [
        const UiAction(
            name: 'switcher.next', keys: AppKeybindings.switcherNext),
        const UiAction(
            name: 'switcher.previous',
            keys: AppKeybindings.switcherPrevious),
        ...const AppKeybindings().switcherActions('mru-switcher'),
      ];
}
