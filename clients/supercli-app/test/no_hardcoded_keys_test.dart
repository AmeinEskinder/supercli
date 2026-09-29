/// Guard: no hardcoded key chords outside `lib/keymap.dart`.
///
/// Fails if any Dart file under `lib/` or `bin/` — except `lib/keymap.dart`
/// itself — contains a chord literal (`'ctrl+…'`, `'cmd+…'`, `'meta+…'`,
/// in either quote style) or an interpolated `keys:`/`shortcut:` string.
/// All shortcuts must come from [Keymap] (`lib/keymap.dart`), the single
/// source of truth ported from the native macOS menu (Amein directive
/// 2026-09-27: fix all 20 hardcoded bindings with one central keymap).
///
/// Additionally, `lib/app_shell_menu.dart` must derive every menu key
/// equivalent from [Keymap] via the `_menuItem(chord: …)` helper: no
/// `key: '<literal>'` and no `modifiers: {<literal>}` named-argument
/// literals may appear there (Claude review 2026-09-29).
library;

import 'dart:io';
import 'dart:isolate';

import 'package:test/test.dart';

/// Package root, resolved from the resolved package config — NOT from
/// `Platform.script`, which under `dart test` points at a compiled kernel
/// file in a temp dir (the old resolution made this guard vacuous).
Future<Directory> _packageRoot() async {
  final configUri = await Isolate.packageConfig;
  if (configUri == null) {
    throw StateError('could not resolve package config');
  }
  // <root>/.dart_tool/package_config.json -> <root>.
  return File.fromUri(configUri).parent.parent;
}

void main() async {
  final packageRoot = await _packageRoot();

  test('no hardcoded key chords outside lib/keymap.dart', () {
    // A quoted chord literal carrying a modifier: 'ctrl+…', 'cmd+…',
    // 'meta+…' (single- or double-quoted).
    final chordLiteral = RegExp('''['"](ctrl|cmd|meta)\\+''');
    // An interpolated keys:/shortcut: string, e.g. keys: '$mod+b'.
    final interpolatedKeys = RegExp('''(keys|shortcut):\\s*['"]\\\$''');

    final offenders = <String>[];
    var scanned = 0;
    for (final dirName in ['lib', 'bin']) {
      final dir = Directory('${packageRoot.path}/$dirName');
      if (!dir.existsSync()) continue;
      final files =
          dir
              .listSync(recursive: true)
              .whereType<File>()
              .where((f) => f.path.endsWith('.dart'))
              // The keymap itself is the single source of truth.
              .where((f) => !f.path.endsWith('lib/keymap.dart'))
              .toList()
            ..sort((a, b) => a.path.compareTo(b.path));
      for (final file in files) {
        scanned++;
        final lines = file.readAsLinesSync();
        for (var i = 0; i < lines.length; i++) {
          final line = lines[i];
          if (chordLiteral.hasMatch(line) || interpolatedKeys.hasMatch(line)) {
            final rel = file.path.substring(packageRoot.path.length + 1);
            offenders.add('$rel:${i + 1}: ${line.trim()}');
          }
        }
      }
    }

    // The guard must actually scan files; a zero count means the package
    // root resolution broke and the test would pass vacuously.
    expect(
      scanned,
      greaterThan(0),
      reason: 'guard scanned 0 files under ${packageRoot.path}',
    );
    expect(
      offenders,
      isEmpty,
      reason:
          'Hardcoded key chords found outside lib/keymap.dart. '
          'Move them into Keymap (lib/keymap.dart):\n'
          '${offenders.join('\n')}',
    );
  });

  test('no hardcoded menu keys in lib/app_shell_menu.dart', () {
    // app_shell_menu.dart must derive every key equivalent from Keymap via
    // the `_menuItem(chord: …)` helper: no `key: '<literal>'` and no
    // `modifiers: {<literal>}` named-argument literals may appear there.
    // The helper passes computed values (`key: key`,
    // `modifiers: parsed.modifiers`), so it is not flagged.
    final menuFile = File('${packageRoot.path}/lib/app_shell_menu.dart');
    expect(
      menuFile.existsSync(),
      isTrue,
      reason: 'lib/app_shell_menu.dart not found under ${packageRoot.path}',
    );
    final hardcodedKey = RegExp('''key:\\s*['"]''');
    final hardcodedModifiers = RegExp('''modifiers:\\s*\\{''');

    final offenders = <String>[];
    final lines = menuFile.readAsLinesSync();
    for (var i = 0; i < lines.length; i++) {
      final line = lines[i];
      if (hardcodedKey.hasMatch(line) || hardcodedModifiers.hasMatch(line)) {
        offenders.add('lib/app_shell_menu.dart:${i + 1}: ${line.trim()}');
      }
    }

    expect(
      offenders,
      isEmpty,
      reason:
          'Hardcoded menu key equivalents in lib/app_shell_menu.dart. '
          'Derive them from Keymap via `_menuItem(chord: …)`:\n'
          '${offenders.join('\n')}',
    );
  });
}
