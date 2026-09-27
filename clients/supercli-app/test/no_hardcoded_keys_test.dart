/// Guard: no hardcoded key chords outside `lib/keymap.dart`.
///
/// Fails if any Dart file under `lib/` or `bin/` — except `lib/keymap.dart`
/// itself — contains a chord literal (`'ctrl+…'`, `'cmd+…'`, `'meta+…'`,
/// in either quote style) or an interpolated `keys:`/`shortcut:` string.
/// All shortcuts must come from [Keymap] (`lib/keymap.dart`), the single
/// source of truth ported from the native macOS menu (Amein directive
/// 2026-09-27: fix all 20 hardcoded bindings with one central keymap).
library;

import 'dart:io';

import 'package:test/test.dart';

void main() {
  test('no hardcoded key chords outside lib/keymap.dart', () {
    // Package root from this file's location:
    // <root>/test/no_hardcoded_keys_test.dart -> <root>.
    final packageRoot = File(Platform.script.toFilePath()).parent.parent;

    // A quoted chord literal carrying a modifier: 'ctrl+…', 'cmd+…',
    // 'meta+…' (single- or double-quoted).
    final chordLiteral = RegExp('''['"](ctrl|cmd|meta)\\+''');
    // An interpolated keys:/shortcut: string, e.g. keys: '$mod+b'.
    final interpolatedKeys = RegExp('''(keys|shortcut):\\s*['"]\\\$''');

    final offenders = <String>[];
    for (final dirName in ['lib', 'bin']) {
      final dir = Directory('${packageRoot.path}/$dirName');
      if (!dir.existsSync()) continue;
      final files = dir
          .listSync(recursive: true)
          .whereType<File>()
          .where((f) => f.path.endsWith('.dart'))
          // The keymap itself is the single source of truth.
          .where((f) => !f.path.endsWith('lib/keymap.dart'))
          .toList()
        ..sort((a, b) => a.path.compareTo(b.path));
      for (final file in files) {
        final lines = file.readAsLinesSync();
        for (var i = 0; i < lines.length; i++) {
          final line = lines[i];
          if (chordLiteral.hasMatch(line) ||
              interpolatedKeys.hasMatch(line)) {
            final rel = file.path.substring(packageRoot.path.length + 1);
            offenders.add('$rel:${i + 1}: ${line.trim()}');
          }
        }
      }
    }

    expect(offenders, isEmpty,
        reason: 'Hardcoded key chords found outside lib/keymap.dart. '
            'Move them into Keymap (lib/keymap.dart):\n'
            '${offenders.join('\n')}');
  });
}
